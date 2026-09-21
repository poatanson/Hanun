use std::collections::VecDeque;
use std::time::Instant;

/// A single point-in-time reading of all monitored resources.
pub struct Snapshot {
    pub sampled_at: Instant,
    pub cpu: CpuSnapshot,
    pub mem: MemSnapshot,
    pub disks: Vec<DiskSnapshot>,
    pub net: NetSnapshot,
    pub processes: Vec<ProcessSnapshot>,
}

/// CPU usage for the whole system and per core.
pub struct CpuSnapshot {
    pub global_usage_percent: f32,
    pub per_core_percent: Vec<f32>,
    pub core_count: usize,
    pub frequency_mhz: Option<u64>,
}

/// RAM and swap usage in bytes.
pub struct MemSnapshot {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

/// Usage and I/O rate for a single disk/volume.
pub struct DiskSnapshot {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub read_bytes_per_sec: u64,
    pub write_bytes_per_sec: u64,
}

/// Throughput for a single network interface.
pub struct NetInterfaceSnapshot {
    pub name: String,
    pub rx_bytes_per_sec: u64,
    pub tx_bytes_per_sec: u64,
}

/// All monitored network interfaces.
pub struct NetSnapshot {
    pub interfaces: Vec<NetInterfaceSnapshot>,
}

/// CPU/memory usage for a single process.
///
/// `cpu_percent_raw` can exceed 100 on multi-core machines (sysinfo reports up to
/// `100 * core_count`); `cpu_percent_normalized` is that value divided by the core
/// count and is what the process panel displays by default.
pub struct ProcessSnapshot {
    pub pid: u32,
    pub name: String,
    // Kept for a future raw/normalized display toggle; the process panel shows
    // `cpu_percent_normalized` by default per the project's normalization policy.
    #[allow(dead_code)]
    pub cpu_percent_raw: f32,
    pub cpu_percent_normalized: f32,
    pub mem_bytes: u64,
    pub status: String,
}

/// Fixed-capacity ring buffer backing the sparkline/history views. Wired up once the
/// interactivity step adds sparklines to the CPU/memory/network panels.
#[allow(dead_code)]
pub struct History<T> {
    buf: VecDeque<T>,
    capacity: usize,
}

#[allow(dead_code)]
impl<T> History<T> {
    /// Creates an empty history that holds at most `capacity` samples.
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Pushes a new sample, evicting the oldest one once the buffer is full.
    pub fn push(&mut self, value: T) {
        if self.buf.len() == self.capacity {
            self.buf.pop_front();
        }
        self.buf.push_back(value);
    }

    /// Iterates stored samples oldest-first.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.buf.iter()
    }

    /// Number of samples currently stored.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// True if no samples have been pushed yet.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

/// Process list sort order, cycled by the `s` key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SortMode {
    CpuDesc,
    MemDesc,
    PidAsc,
    NameAsc,
}

impl SortMode {
    /// Returns the next mode in the cycle: CPU -> Mem -> PID -> Name -> CPU ...
    pub fn next(self) -> Self {
        match self {
            SortMode::CpuDesc => SortMode::MemDesc,
            SortMode::MemDesc => SortMode::PidAsc,
            SortMode::PidAsc => SortMode::NameAsc,
            SortMode::NameAsc => SortMode::CpuDesc,
        }
    }

    /// Short label shown in the process panel header.
    pub fn label(self) -> &'static str {
        match self {
            SortMode::CpuDesc => "CPU",
            SortMode::MemDesc => "MEM",
            SortMode::PidAsc => "PID",
            SortMode::NameAsc => "NAME",
        }
    }
}

/// The five monitored panels, in the order used by the 1-5 keybinding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PanelId {
    Cpu,
    Memory,
    Disk,
    Network,
    Process,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_evicts_oldest_when_full() {
        let mut history: History<i32> = History::new(3);
        for value in 1..=5 {
            history.push(value);
        }
        assert_eq!(history.len(), 3);
        assert_eq!(history.iter().copied().collect::<Vec<_>>(), vec![3, 4, 5]);
    }

    #[test]
    fn history_starts_empty() {
        let history: History<i32> = History::new(3);
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn sort_mode_cycles_through_all_variants() {
        let start = SortMode::CpuDesc;
        let after_four = start.next().next().next().next();
        assert_eq!(after_four, start);
    }
}
