use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use sysinfo::{
    CpuRefreshKind, Disks, MemoryRefreshKind, Networks, ProcessesToUpdate, RefreshKind, System,
};

use crate::event::AppEvent;
use crate::model::{
    self, CpuSnapshot, DiskSnapshot, MemSnapshot, NetInterfaceSnapshot, NetSnapshot,
    ProcessSnapshot, Snapshot,
};

/// Commands sent from the UI thread to control the sampler.
pub enum SamplerCommand {
    Shutdown,
    Pause,
    Resume,
    SetInterval(Duration),
}

const DEFAULT_INTERVAL: Duration = Duration::from_millis(1000);
pub const MIN_INTERVAL: Duration = Duration::from_millis(500);
pub const MAX_INTERVAL: Duration = Duration::from_millis(5000);

/// Spawns the sampler thread. It owns the single `System`, `Disks`, and `Networks` for its
/// entire life, refreshing all resources on each tick and sending a `Snapshot` over
/// `event_tx`. Exits promptly on `SamplerCommand::Shutdown` or once `cmd_rx`'s sender is
/// dropped.
pub fn spawn_sampler_thread(
    event_tx: Sender<AppEvent>,
    cmd_rx: Receiver<SamplerCommand>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut system = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        let mut disks = Disks::new();
        let mut networks = Networks::new();
        let mut last_tick = Instant::now();
        let mut first_sample = true;
        let mut interval = DEFAULT_INTERVAL;
        let mut paused = false;

        loop {
            match cmd_rx.recv_timeout(interval) {
                Ok(SamplerCommand::Shutdown) => return,
                Ok(SamplerCommand::Pause) => {
                    paused = true;
                    continue;
                }
                Ok(SamplerCommand::Resume) => {
                    paused = false;
                    continue;
                }
                Ok(SamplerCommand::SetInterval(requested)) => {
                    interval = requested.clamp(MIN_INTERVAL, MAX_INTERVAL);
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {}
            }
            if paused {
                continue;
            }

            let now = Instant::now();
            let elapsed_secs = now
                .duration_since(last_tick)
                .as_secs_f64()
                .max(f64::EPSILON);
            last_tick = now;

            system.refresh_cpu_usage();
            system.refresh_memory();
            disks.refresh(true);
            networks.refresh(true);
            system.refresh_processes(ProcessesToUpdate::All, true);

            let snapshot = Snapshot {
                sampled_at: now,
                cpu: build_cpu_snapshot(&system, first_sample),
                mem: build_mem_snapshot(&system),
                disks: build_disk_snapshots(&disks, elapsed_secs, first_sample),
                net: build_net_snapshot(&networks, elapsed_secs, first_sample),
                processes: build_process_snapshots(&system),
            };
            first_sample = false;

            if event_tx
                .send(AppEvent::Snapshot(Box::new(snapshot)))
                .is_err()
            {
                return;
            }
        }
    })
}

/// Builds a `CpuSnapshot` from the current `System` state. The very first sample after
/// `System` creation has no prior reading to diff against, so it is reported as 0 rather
/// than trusting sysinfo's own first-call value.
fn build_cpu_snapshot(system: &System, first_sample: bool) -> CpuSnapshot {
    let cpus = system.cpus();
    let (global_usage_percent, per_core_percent) = if first_sample {
        (0.0, vec![0.0; cpus.len()])
    } else {
        (
            system.global_cpu_usage(),
            cpus.iter().map(sysinfo::Cpu::cpu_usage).collect(),
        )
    };

    CpuSnapshot {
        global_usage_percent,
        per_core_percent,
        core_count: cpus.len(),
        frequency_mhz: cpus
            .first()
            .map(sysinfo::Cpu::frequency)
            .filter(|hz| *hz > 0),
    }
}

/// Builds a `MemSnapshot` from the current `System` state.
fn build_mem_snapshot(system: &System) -> MemSnapshot {
    MemSnapshot {
        total_bytes: system.total_memory(),
        used_bytes: system.used_memory(),
        available_bytes: system.available_memory(),
        swap_total_bytes: system.total_swap(),
        swap_used_bytes: system.used_swap(),
    }
}

/// Builds one `DiskSnapshot` per volume. Byte counts come straight from sysinfo; the
/// rate conversion itself is `model::bytes_per_sec` (normalization/rate math lives in
/// `model`, not here -- this function only collects the raw inputs it needs).
fn build_disk_snapshots(disks: &Disks, elapsed_secs: f64, first_sample: bool) -> Vec<DiskSnapshot> {
    disks
        .list()
        .iter()
        .map(|disk| {
            let usage = disk.usage();
            DiskSnapshot {
                name: disk.name().to_string_lossy().into_owned(),
                mount_point: disk.mount_point().to_string_lossy().into_owned(),
                total_bytes: disk.total_space(),
                available_bytes: disk.available_space(),
                read_bytes_per_sec: model::bytes_per_sec(
                    usage.read_bytes,
                    elapsed_secs,
                    first_sample,
                ),
                write_bytes_per_sec: model::bytes_per_sec(
                    usage.written_bytes,
                    elapsed_secs,
                    first_sample,
                ),
            }
        })
        .collect()
}

/// Builds a `NetSnapshot` from the current interface list. See `build_disk_snapshots` --
/// the rate conversion is `model::bytes_per_sec`, not inlined here.
fn build_net_snapshot(networks: &Networks, elapsed_secs: f64, first_sample: bool) -> NetSnapshot {
    let interfaces = networks
        .list()
        .iter()
        .map(|(name, data)| NetInterfaceSnapshot {
            name: name.clone(),
            rx_bytes_per_sec: model::bytes_per_sec(data.received(), elapsed_secs, first_sample),
            tx_bytes_per_sec: model::bytes_per_sec(data.transmitted(), elapsed_secs, first_sample),
        })
        .collect();
    NetSnapshot { interfaces }
}

/// Builds one `ProcessSnapshot` per running process. `cpu_percent_normalized` is computed
/// by `model::normalize_process_cpu`, not inlined here (see `build_disk_snapshots`).
fn build_process_snapshots(system: &System) -> Vec<ProcessSnapshot> {
    let core_count = system.cpus().len();
    system
        .processes()
        .values()
        .map(|process| {
            let cpu_percent_raw = process.cpu_usage();
            ProcessSnapshot {
                pid: process.pid().as_u32(),
                name: process.name().to_string_lossy().into_owned(),
                cpu_percent_raw,
                cpu_percent_normalized: model::normalize_process_cpu(cpu_percent_raw, core_count),
                mem_bytes: process.memory(),
                status: process.status().to_string(),
            }
        })
        .collect()
}
