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

/// Normalizes a process's raw CPU usage (sysinfo reports up to `100 * core_count` on
/// multi-core machines) to a 0-100 range by dividing by the core count.
pub fn normalize_process_cpu(raw_percent: f32, core_count: usize) -> f32 {
    (raw_percent / core_count.max(1) as f32).clamp(0.0, 100.0)
}

/// Converts a byte delta since the last sample into a bytes/sec rate. Forced to 0 on the
/// first sample, since sysinfo has no prior reading to diff against yet and would
/// otherwise report the resource's entire lifetime total as if it happened in one tick.
pub fn bytes_per_sec(delta_bytes: u64, elapsed_secs: f64, first_sample: bool) -> u64 {
    if first_sample {
        0
    } else {
        (delta_bytes as f64 / elapsed_secs) as u64
    }
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

/// Filtered + sorted view over a snapshot's process list, expressed as indices into
/// `Snapshot::processes`. The caller (`App`) rebuilds it only when the snapshot, sort
/// mode, or filter text actually changes -- never on a bare tick -- so the render path
/// only ever slices an already-ordered list instead of re-filtering/re-sorting every
/// frame.
#[derive(Default)]
pub struct ProcessOrder {
    indices: Vec<usize>,
}

impl ProcessOrder {
    /// Recomputes the filtered, sorted index list in place, reusing the existing
    /// allocation rather than allocating a fresh `Vec` on every rebuild. Filtering and
    /// name sorting compare case-insensitively without allocating a lowercase copy per
    /// process (see `contains_ascii_case_insensitive`).
    pub fn rebuild(&mut self, processes: &[ProcessSnapshot], filter: &str, sort_mode: SortMode) {
        self.indices.clear();
        self.indices.extend(
            processes
                .iter()
                .enumerate()
                .filter_map(|(i, p)| contains_ascii_case_insensitive(&p.name, filter).then_some(i)),
        );
        self.sort(processes, sort_mode);
    }

    fn sort(&mut self, processes: &[ProcessSnapshot], sort_mode: SortMode) {
        match sort_mode {
            SortMode::CpuDesc => self.indices.sort_by(|&a, &b| {
                processes[b]
                    .cpu_percent_normalized
                    .total_cmp(&processes[a].cpu_percent_normalized)
            }),
            SortMode::MemDesc => self
                .indices
                .sort_by_key(|&i| std::cmp::Reverse(processes[i].mem_bytes)),
            SortMode::PidAsc => self.indices.sort_by_key(|&i| processes[i].pid),
            SortMode::NameAsc => self.indices.sort_by(|&a, &b| {
                ascii_lowercased(&processes[a].name).cmp(ascii_lowercased(&processes[b].name))
            }),
        }
    }

    /// Indices into the snapshot's process list, in filtered+sorted display order.
    pub fn indices(&self) -> &[usize] {
        &self.indices
    }

    /// Number of processes currently matching the filter.
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    /// Finds `pid`'s current position in the order. Used to keep the scroll anchor on the
    /// same process across a resort/refilter/new snapshot instead of snapping to whatever
    /// now sits at the same index.
    pub fn position_of_pid(&self, processes: &[ProcessSnapshot], pid: u32) -> Option<usize> {
        self.indices.iter().position(|&i| processes[i].pid == pid)
    }
}

/// ASCII case-insensitive substring test with no allocation. An empty `needle` matches
/// everything. Process/executable names on Windows are effectively always ASCII, so this
/// trades full Unicode case folding -- which would need a new heap-allocated lowercase
/// copy of every process name on every filter keystroke -- for a zero-allocation scan.
fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    needle.len() <= haystack.len()
        && haystack
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle))
}

/// ASCII-lowercased characters of `s`, compared lazily so `NameAsc` sorting never
/// allocates a lowercase copy of any process name.
fn ascii_lowercased(s: &str) -> impl Iterator<Item = char> + '_ {
    s.chars().map(|c| c.to_ascii_lowercase())
}

/// The six monitored panels, in the order used by the 1-6 keybinding.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PanelId {
    Cpu,
    Memory,
    Disk,
    Network,
    Process,
    Chart,
}

/// Which resource the CHART panel currently visualizes, cycled by the `g` key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChartKind {
    Memory,
    Disk,
    Cpu,
}

impl ChartKind {
    /// Returns the next kind in the cycle: Memory -> Disk -> Cpu -> Memory ...
    pub fn next(self) -> Self {
        match self {
            ChartKind::Memory => ChartKind::Disk,
            ChartKind::Disk => ChartKind::Cpu,
            ChartKind::Cpu => ChartKind::Memory,
        }
    }

    /// Short label shown in the chart panel title.
    pub fn label(self) -> &'static str {
        match self {
            ChartKind::Memory => "MEMORY",
            ChartKind::Disk => "DISK",
            ChartKind::Cpu => "CPU",
        }
    }

    /// Label for the primary (colored) slice of the donut.
    pub fn primary_label(self) -> &'static str {
        match self {
            ChartKind::Memory => "USED",
            ChartKind::Disk => "USED",
            ChartKind::Cpu => "BUSY",
        }
    }

    /// Label for the secondary (neutral) slice of the donut.
    pub fn secondary_label(self) -> &'static str {
        match self {
            ChartKind::Memory => "FREE",
            ChartKind::Disk => "FREE",
            ChartKind::Cpu => "IDLE",
        }
    }
}

/// Percentage of `used` within `total`, clamped to 0-100. Returns 0.0 when `total` is 0
/// (before the first sample, or on a system reporting no capacity for a resource).
pub fn percent_of(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
    }
}

/// Sums used and total bytes across all disks. `DiskSnapshot` has no aggregate/used field
/// of its own, so used is derived per-disk as `total_bytes - available_bytes`.
pub fn aggregate_disk_usage(disks: &[DiskSnapshot]) -> (u64, u64) {
    disks.iter().fold((0, 0), |(used, total), disk| {
        (
            used + disk.total_bytes.saturating_sub(disk.available_bytes),
            total + disk.total_bytes,
        )
    })
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

    #[test]
    fn normalize_process_cpu_divides_by_core_count() {
        assert_eq!(normalize_process_cpu(50.0, 1), 50.0);
        assert_eq!(normalize_process_cpu(400.0, 4), 100.0);
    }

    #[test]
    fn normalize_process_cpu_clamps_to_0_100() {
        assert_eq!(normalize_process_cpu(800.0, 4), 100.0); // would be 200 unclamped
        assert_eq!(normalize_process_cpu(-10.0, 4), 0.0);
    }

    #[test]
    fn normalize_process_cpu_treats_zero_cores_as_one() {
        assert_eq!(normalize_process_cpu(50.0, 0), 50.0);
    }

    #[test]
    fn bytes_per_sec_divides_delta_by_elapsed_time() {
        assert_eq!(bytes_per_sec(1000, 2.0, false), 500);
        assert_eq!(bytes_per_sec(0, 1.0, false), 0);
    }

    #[test]
    fn bytes_per_sec_is_zero_on_first_sample() {
        assert_eq!(bytes_per_sec(1_000_000, 1.0, true), 0);
    }

    #[test]
    fn contains_ascii_case_insensitive_matches_regardless_of_case() {
        assert!(contains_ascii_case_insensitive("Chrome.exe", "chrome"));
        assert!(contains_ascii_case_insensitive("Chrome.exe", "CHROME"));
        assert!(contains_ascii_case_insensitive("Chrome.exe", ".exe"));
    }

    #[test]
    fn contains_ascii_case_insensitive_empty_needle_matches_everything() {
        assert!(contains_ascii_case_insensitive("anything.exe", ""));
        assert!(contains_ascii_case_insensitive("", ""));
    }

    #[test]
    fn contains_ascii_case_insensitive_rejects_non_match() {
        assert!(!contains_ascii_case_insensitive("Chrome.exe", "firefox"));
        assert!(!contains_ascii_case_insensitive("short", "way too long"));
    }

    fn proc(pid: u32, name: &str, cpu: f32, mem_mib: u64) -> ProcessSnapshot {
        ProcessSnapshot {
            pid,
            name: name.to_string(),
            cpu_percent_raw: cpu,
            cpu_percent_normalized: cpu,
            mem_bytes: mem_mib * 1024 * 1024,
            status: "Running".to_string(),
        }
    }

    fn sample_processes() -> Vec<ProcessSnapshot> {
        vec![
            proc(30, "chrome.exe", 12.0, 500),
            proc(10, "explorer.exe", 3.0, 100),
            proc(20, "Notepad.exe", 45.0, 20),
        ]
    }

    fn names_in_order(order: &ProcessOrder, processes: &[ProcessSnapshot]) -> Vec<&'static str> {
        order
            .indices()
            .iter()
            .map(|&i| match processes[i].pid {
                30 => "chrome.exe",
                10 => "explorer.exe",
                20 => "Notepad.exe",
                _ => unreachable!(),
            })
            .collect()
    }

    #[test]
    fn process_order_sorts_by_cpu_desc() {
        let processes = sample_processes();
        let mut order = ProcessOrder::default();
        order.rebuild(&processes, "", SortMode::CpuDesc);
        assert_eq!(
            names_in_order(&order, &processes),
            vec!["Notepad.exe", "chrome.exe", "explorer.exe"]
        );
    }

    #[test]
    fn process_order_sorts_by_mem_desc() {
        let processes = sample_processes();
        let mut order = ProcessOrder::default();
        order.rebuild(&processes, "", SortMode::MemDesc);
        assert_eq!(
            names_in_order(&order, &processes),
            vec!["chrome.exe", "explorer.exe", "Notepad.exe"]
        );
    }

    #[test]
    fn process_order_sorts_by_pid_asc() {
        let processes = sample_processes();
        let mut order = ProcessOrder::default();
        order.rebuild(&processes, "", SortMode::PidAsc);
        assert_eq!(
            names_in_order(&order, &processes),
            vec!["explorer.exe", "Notepad.exe", "chrome.exe"]
        );
    }

    #[test]
    fn process_order_sorts_by_name_asc_case_insensitive() {
        let processes = sample_processes();
        let mut order = ProcessOrder::default();
        order.rebuild(&processes, "", SortMode::NameAsc);
        assert_eq!(
            names_in_order(&order, &processes),
            vec!["chrome.exe", "explorer.exe", "Notepad.exe"]
        );
    }

    #[test]
    fn process_order_filters_by_name_substring_case_insensitive() {
        let processes = sample_processes();
        let mut order = ProcessOrder::default();
        order.rebuild(&processes, "NOTE", SortMode::CpuDesc);
        assert_eq!(names_in_order(&order, &processes), vec!["Notepad.exe"]);
    }

    #[test]
    fn process_order_empty_filter_matches_everything() {
        let processes = sample_processes();
        let mut order = ProcessOrder::default();
        order.rebuild(&processes, "", SortMode::PidAsc);
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn process_order_no_match_is_empty() {
        let processes = sample_processes();
        let mut order = ProcessOrder::default();
        order.rebuild(&processes, "zzz", SortMode::CpuDesc);
        assert_eq!(order.len(), 0);
    }

    #[test]
    fn position_of_pid_finds_process_after_resort() {
        let processes = sample_processes();
        let mut order = ProcessOrder::default();
        order.rebuild(&processes, "", SortMode::PidAsc);
        assert_eq!(order.position_of_pid(&processes, 20), Some(1));

        order.rebuild(&processes, "", SortMode::CpuDesc);
        assert_eq!(order.position_of_pid(&processes, 20), Some(0));
    }

    #[test]
    fn position_of_pid_returns_none_when_process_is_gone() {
        let processes = sample_processes();
        let mut order = ProcessOrder::default();
        order.rebuild(&processes, "", SortMode::PidAsc);
        assert_eq!(order.position_of_pid(&processes, 999), None);
    }

    #[test]
    fn chart_kind_cycles_through_all_variants() {
        let start = ChartKind::Memory;
        let after_three = start.next().next().next();
        assert_eq!(after_three, start);
    }

    #[test]
    fn percent_of_divides_used_by_total() {
        assert_eq!(percent_of(50, 100), 50.0);
        assert_eq!(percent_of(1, 4), 25.0);
    }

    #[test]
    fn percent_of_is_zero_when_total_is_zero() {
        assert_eq!(percent_of(5, 0), 0.0);
    }

    fn disk(total_bytes: u64, available_bytes: u64) -> DiskSnapshot {
        DiskSnapshot {
            name: "d".to_string(),
            mount_point: "/".to_string(),
            total_bytes,
            available_bytes,
            read_bytes_per_sec: 0,
            write_bytes_per_sec: 0,
        }
    }

    #[test]
    fn aggregate_disk_usage_sums_across_disks() {
        let disks = vec![disk(100, 60), disk(200, 50)];
        assert_eq!(aggregate_disk_usage(&disks), (40 + 150, 300));
    }

    #[test]
    fn aggregate_disk_usage_empty_slice_is_zero() {
        assert_eq!(aggregate_disk_usage(&[]), (0, 0));
    }
}
