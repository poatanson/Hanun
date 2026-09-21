use std::sync::mpsc::Sender;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};

use crate::event::AppEvent;
use crate::model::{ChartKind, PanelId, ProcessOrder, Snapshot, SortMode};
use crate::sampler::{MAX_INTERVAL, MIN_INTERVAL, SamplerCommand};

const INTERVAL_STEP_MS: u64 = 250;
const DEFAULT_INTERVAL_MS: u64 = 1000;

/// Application state, mutated only through `update()` in response to `AppEvent`s.
pub struct App {
    pub should_quit: bool,
    pub focused_panel: PanelId,
    pub latest: Option<Snapshot>,
    pub sort_mode: SortMode,
    pub filter: String,
    pub filter_editing: bool,
    pub paused: bool,
    pub tick_interval_ms: u64,
    pub scroll_offset: usize,
    pub show_help: bool,
    pub ascii_mode: bool,
    pub chart_kind: ChartKind,
    /// Filtered+sorted process order, cached across frames. Rebuilt only by
    /// `refresh_process_order`, which every filter/sort/snapshot-changing path must call.
    pub process_order: ProcessOrder,
    sampler_cmd_tx: Sender<SamplerCommand>,
    /// Counts `refresh_process_order` calls so tests can prove it runs exactly on the
    /// documented invalidation triggers and not on a bare tick.
    #[cfg(test)]
    pub recompute_count: u32,
}

impl App {
    /// Creates the initial application state: CPU panel focused, running, no data yet.
    /// `sampler_cmd_tx` is used to forward pause/resume/interval changes to the sampler.
    /// `ascii_mode` is fixed for the process lifetime (set from the `--ascii` CLI flag).
    pub fn new(sampler_cmd_tx: Sender<SamplerCommand>, ascii_mode: bool) -> Self {
        Self {
            should_quit: false,
            focused_panel: PanelId::Cpu,
            latest: None,
            sort_mode: SortMode::CpuDesc,
            filter: String::new(),
            filter_editing: false,
            paused: false,
            tick_interval_ms: DEFAULT_INTERVAL_MS,
            scroll_offset: 0,
            show_help: false,
            ascii_mode,
            chart_kind: ChartKind::Memory,
            process_order: ProcessOrder::default(),
            sampler_cmd_tx,
            #[cfg(test)]
            recompute_count: 0,
        }
    }

    /// Applies one event to the application state.
    pub fn update(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => self.handle_key(key),
            AppEvent::Snapshot(snapshot) => {
                let anchor = self.process_anchor_pid();
                self.latest = Some(*snapshot);
                self.refresh_process_order(anchor);
            }
            AppEvent::Tick => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if self.show_help {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
            ) {
                self.show_help = false;
            }
            return;
        }
        if self.filter_editing {
            self.handle_filter_key(key);
            return;
        }

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Tab => self.focus_next(),
            KeyCode::Char('1') => self.set_focus(PanelId::Cpu),
            KeyCode::Char('2') => self.set_focus(PanelId::Memory),
            KeyCode::Char('3') => self.set_focus(PanelId::Disk),
            KeyCode::Char('4') => self.set_focus(PanelId::Network),
            KeyCode::Char('5') => self.set_focus(PanelId::Process),
            KeyCode::Char('6') => self.set_focus(PanelId::Chart),
            KeyCode::Char('g') => self.chart_kind = self.chart_kind.next(),
            KeyCode::Char('s') => {
                let anchor = self.process_anchor_pid();
                self.sort_mode = self.sort_mode.next();
                self.refresh_process_order(anchor);
            }
            KeyCode::Char('/') => self.filter_editing = true,
            KeyCode::Char('p') => self.toggle_paused(),
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.adjust_interval(INTERVAL_STEP_MS as i64)
            }
            KeyCode::Char('-') => self.adjust_interval(-(INTERVAL_STEP_MS as i64)),
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_offset = self.scroll_offset.saturating_add(1)
            }
            _ => {}
        }
    }

    fn handle_filter_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter | KeyCode::Esc => self.filter_editing = false,
            KeyCode::Backspace => {
                let anchor = self.process_anchor_pid();
                self.filter.pop();
                self.refresh_process_order(anchor);
            }
            KeyCode::Char(c) => {
                let anchor = self.process_anchor_pid();
                self.filter.push(c);
                self.refresh_process_order(anchor);
            }
            _ => {}
        }
    }

    /// PID currently anchored at `scroll_offset` in the process panel, if that panel is
    /// focused and holds data. Captured before a resort/refilter/new-snapshot so the same
    /// process stays anchored afterward instead of whatever now lands at the same index.
    fn process_anchor_pid(&self) -> Option<u32> {
        if self.focused_panel != PanelId::Process {
            return None;
        }
        let processes = self.latest.as_ref()?.processes.as_slice();
        let &idx = self.process_order.indices().get(self.scroll_offset)?;
        processes.get(idx).map(|process| process.pid)
    }

    /// Rebuilds the cached process filter/sort order from the current snapshot, filter,
    /// and sort mode, then restores `scroll_offset` to `anchor_pid`'s new position (or
    /// clamps it if that process is gone). This is the only place `ProcessOrder::rebuild`
    /// is called, and every path that changes the snapshot, sort mode, or filter text
    /// must route through it -- a bare `Tick` must never call this.
    fn refresh_process_order(&mut self, anchor_pid: Option<u32>) {
        #[cfg(test)]
        {
            self.recompute_count += 1;
        }
        let Some(processes) = self.latest.as_ref().map(|s| s.processes.as_slice()) else {
            self.process_order = ProcessOrder::default();
            return;
        };
        self.process_order
            .rebuild(processes, &self.filter, self.sort_mode);
        if self.focused_panel == PanelId::Process {
            self.scroll_offset = anchor_pid
                .and_then(|pid| self.process_order.position_of_pid(processes, pid))
                .unwrap_or_else(|| {
                    self.scroll_offset
                        .min(self.process_order.len().saturating_sub(1))
                });
        }
    }

    fn focus_next(&mut self) {
        let next = match self.focused_panel {
            PanelId::Cpu => PanelId::Memory,
            PanelId::Memory => PanelId::Disk,
            PanelId::Disk => PanelId::Network,
            PanelId::Network => PanelId::Process,
            PanelId::Process => PanelId::Chart,
            PanelId::Chart => PanelId::Cpu,
        };
        self.set_focus(next);
    }

    fn set_focus(&mut self, panel: PanelId) {
        self.focused_panel = panel;
        self.scroll_offset = 0;
    }

    fn toggle_paused(&mut self) {
        self.paused = !self.paused;
        let command = if self.paused {
            SamplerCommand::Pause
        } else {
            SamplerCommand::Resume
        };
        let _ = self.sampler_cmd_tx.send(command);
    }

    fn adjust_interval(&mut self, delta_ms: i64) {
        let min_ms = MIN_INTERVAL.as_millis() as i64;
        let max_ms = MAX_INTERVAL.as_millis() as i64;
        let new_ms = (self.tick_interval_ms as i64 + delta_ms).clamp(min_ms, max_ms) as u64;
        if new_ms != self.tick_interval_ms {
            self.tick_interval_ms = new_ms;
            let _ = self
                .sampler_cmd_tx
                .send(SamplerCommand::SetInterval(Duration::from_millis(new_ms)));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Instant;

    use super::*;
    use crate::model::{CpuSnapshot, MemSnapshot, NetSnapshot, ProcessSnapshot};

    fn test_app() -> App {
        let (tx, _rx) = mpsc::channel();
        App::new(tx, false)
    }

    fn press(code: KeyCode) -> AppEvent {
        AppEvent::Key(KeyEvent::new(code, crossterm::event::KeyModifiers::NONE))
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

    fn snapshot_with_processes(processes: Vec<ProcessSnapshot>) -> Snapshot {
        Snapshot {
            sampled_at: Instant::now(),
            cpu: CpuSnapshot {
                global_usage_percent: 0.0,
                per_core_percent: vec![],
                core_count: 1,
                frequency_mhz: None,
            },
            mem: MemSnapshot {
                total_bytes: 0,
                used_bytes: 0,
                available_bytes: 0,
                swap_total_bytes: 0,
                swap_used_bytes: 0,
            },
            disks: vec![],
            net: NetSnapshot { interfaces: vec![] },
            processes,
        }
    }

    #[test]
    fn quits_on_q_or_esc() {
        let mut app = test_app();
        app.update(press(KeyCode::Char('q')));
        assert!(app.should_quit);

        let mut app = test_app();
        app.update(press(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn tab_cycles_focus_through_all_panels_and_back() {
        let mut app = test_app();
        let order = [
            PanelId::Memory,
            PanelId::Disk,
            PanelId::Network,
            PanelId::Process,
            PanelId::Chart,
            PanelId::Cpu,
        ];
        for expected in order {
            app.update(press(KeyCode::Tab));
            assert_eq!(app.focused_panel, expected);
        }
    }

    #[test]
    fn number_keys_jump_to_panel() {
        let mut app = test_app();
        app.update(press(KeyCode::Char('3')));
        assert_eq!(app.focused_panel, PanelId::Disk);
    }

    #[test]
    fn digit_6_focuses_chart_panel() {
        let mut app = test_app();
        app.update(press(KeyCode::Char('6')));
        assert_eq!(app.focused_panel, PanelId::Chart);
    }

    #[test]
    fn g_cycles_chart_kind() {
        let mut app = test_app();
        assert_eq!(app.chart_kind, ChartKind::Memory);
        app.update(press(KeyCode::Char('g')));
        assert_eq!(app.chart_kind, ChartKind::Disk);
        app.update(press(KeyCode::Char('g')));
        assert_eq!(app.chart_kind, ChartKind::Cpu);
        app.update(press(KeyCode::Char('g')));
        assert_eq!(app.chart_kind, ChartKind::Memory);
    }

    #[test]
    fn g_is_captured_as_filter_text_while_filter_editing() {
        let mut app = test_app();
        app.update(press(KeyCode::Char('/')));
        app.update(press(KeyCode::Char('g')));
        assert_eq!(app.filter, "g");
        assert_eq!(app.chart_kind, ChartKind::Memory);
    }

    #[test]
    fn g_is_ignored_while_help_overlay_is_shown() {
        let mut app = test_app();
        app.update(press(KeyCode::Char('?')));
        app.update(press(KeyCode::Char('g')));
        assert_eq!(app.chart_kind, ChartKind::Memory);
    }

    #[test]
    fn s_cycles_sort_mode() {
        let mut app = test_app();
        assert_eq!(app.sort_mode, SortMode::CpuDesc);
        app.update(press(KeyCode::Char('s')));
        assert_eq!(app.sort_mode, SortMode::MemDesc);
    }

    #[test]
    fn slash_enters_filter_mode_and_captures_text_until_enter() {
        let mut app = test_app();
        app.update(press(KeyCode::Char('/')));
        assert!(app.filter_editing);

        app.update(press(KeyCode::Char('c')));
        app.update(press(KeyCode::Char('h')));
        assert_eq!(app.filter, "ch");

        // While editing, q/Esc must not quit -- Esc only leaves filter mode.
        app.update(press(KeyCode::Esc));
        assert!(!app.filter_editing);
        assert!(!app.should_quit);
    }

    #[test]
    fn backspace_removes_last_filter_char() {
        let mut app = test_app();
        app.update(press(KeyCode::Char('/')));
        app.update(press(KeyCode::Char('a')));
        app.update(press(KeyCode::Backspace));
        assert_eq!(app.filter, "");
    }

    #[test]
    fn p_toggles_paused() {
        let mut app = test_app();
        assert!(!app.paused);
        app.update(press(KeyCode::Char('p')));
        assert!(app.paused);
        app.update(press(KeyCode::Char('p')));
        assert!(!app.paused);
    }

    #[test]
    fn interval_adjusts_and_clamps_to_bounds() {
        let mut app = test_app();
        assert_eq!(app.tick_interval_ms, 1000);

        app.update(press(KeyCode::Char('+')));
        assert_eq!(app.tick_interval_ms, 1250);

        for _ in 0..30 {
            app.update(press(KeyCode::Char('-')));
        }
        assert_eq!(app.tick_interval_ms, MIN_INTERVAL.as_millis() as u64);

        for _ in 0..30 {
            app.update(press(KeyCode::Char('+')));
        }
        assert_eq!(app.tick_interval_ms, MAX_INTERVAL.as_millis() as u64);
    }

    #[test]
    fn focus_change_resets_scroll_offset() {
        let mut app = test_app();
        app.update(press(KeyCode::Char('5')));
        app.scroll_offset = 3;
        app.update(press(KeyCode::Char('1')));
        assert_eq!(app.scroll_offset, 0);
    }

    #[test]
    fn question_mark_toggles_help_and_blocks_other_keys() {
        let mut app = test_app();
        app.update(press(KeyCode::Char('?')));
        assert!(app.show_help);

        // While help is shown, other keys (e.g. panel focus) must not take effect.
        app.update(press(KeyCode::Char('3')));
        assert_eq!(app.focused_panel, PanelId::Cpu);

        app.update(press(KeyCode::Esc));
        assert!(!app.show_help);
        assert!(!app.should_quit);
    }

    // -- process order cache invalidation ------------------------------------------
    //
    // These prove `refresh_process_order` runs on exactly the three documented
    // triggers (snapshot arrival, sort change, filter change) and on nothing else --
    // in particular, never on a bare `Tick`, which is what used to force a
    // full re-filter/re-sort of the process list four times a second.

    #[test]
    fn snapshot_arrival_recomputes_process_order() {
        let mut app = test_app();
        assert_eq!(app.recompute_count, 0);

        let snapshot =
            snapshot_with_processes(vec![proc(10, "b.exe", 1.0, 1), proc(20, "a.exe", 2.0, 1)]);
        app.update(AppEvent::Snapshot(Box::new(snapshot)));

        assert_eq!(app.recompute_count, 1);
        assert_eq!(app.process_order.indices().len(), 2);
    }

    #[test]
    fn tick_does_not_recompute_process_order() {
        let mut app = test_app();
        let snapshot = snapshot_with_processes(vec![proc(10, "b.exe", 1.0, 1)]);
        app.update(AppEvent::Snapshot(Box::new(snapshot)));
        let count_after_snapshot = app.recompute_count;

        app.update(AppEvent::Tick);
        app.update(AppEvent::Tick);
        app.update(AppEvent::Tick);

        assert_eq!(app.recompute_count, count_after_snapshot);
    }

    #[test]
    fn unrelated_keys_do_not_recompute_process_order() {
        let mut app = test_app();
        let snapshot = snapshot_with_processes(vec![proc(10, "b.exe", 1.0, 1)]);
        app.update(AppEvent::Snapshot(Box::new(snapshot)));
        let count_before = app.recompute_count;

        app.update(press(KeyCode::Tab));
        app.update(press(KeyCode::Char('p')));
        app.update(press(KeyCode::Up));

        assert_eq!(app.recompute_count, count_before);
    }

    #[test]
    fn sort_key_recomputes_process_order() {
        let mut app = test_app();
        let snapshot =
            snapshot_with_processes(vec![proc(10, "b.exe", 1.0, 1), proc(20, "a.exe", 2.0, 1)]);
        app.update(AppEvent::Snapshot(Box::new(snapshot)));
        let count_before = app.recompute_count;

        app.update(press(KeyCode::Char('s')));

        assert_eq!(app.recompute_count, count_before + 1);
    }

    #[test]
    fn filter_edit_recomputes_process_order() {
        let mut app = test_app();
        let snapshot = snapshot_with_processes(vec![
            proc(10, "chrome.exe", 1.0, 1),
            proc(20, "notepad.exe", 2.0, 1),
        ]);
        app.update(AppEvent::Snapshot(Box::new(snapshot)));
        app.update(press(KeyCode::Char('/')));

        let count_before = app.recompute_count;
        app.update(press(KeyCode::Char('c')));
        assert_eq!(app.recompute_count, count_before + 1);
        assert_eq!(app.process_order.indices().len(), 1); // only "chrome.exe" matches "c"

        let count_before = app.recompute_count;
        app.update(press(KeyCode::Backspace));
        assert_eq!(app.recompute_count, count_before + 1);
        assert_eq!(app.process_order.indices().len(), 2); // filter empty again -> matches both
    }

    // -- PID-anchored selection ------------------------------------------------------

    #[test]
    fn scroll_anchor_follows_pid_across_resort() {
        let mut app = test_app();
        app.update(press(KeyCode::Char('5'))); // focus the process panel

        let snapshot = snapshot_with_processes(vec![
            proc(10, "low-mem.exe", 90.0, 1), // highest CPU
            proc(20, "mid.exe", 50.0, 500),   // highest MEM
            proc(30, "high-cpu.exe", 10.0, 10),
        ]);
        app.update(AppEvent::Snapshot(Box::new(snapshot)));
        // Default sort is CpuDesc: pid 10 (90%) is first, so index order is [0, 1, 2].
        assert_eq!(app.process_order.indices(), &[0, 1, 2]);

        // Anchor on pid 20 ("mid.exe"), which sits at position 1 under CpuDesc.
        app.scroll_offset = 1;

        app.update(press(KeyCode::Char('s'))); // -> MemDesc
        // Under MemDesc, pid 20 (500 MiB) sorts first; the anchor must follow it there.
        assert_eq!(app.process_order.indices(), &[1, 2, 0]);
        assert_eq!(app.scroll_offset, 0);
        let anchored_idx = app.process_order.indices()[app.scroll_offset];
        assert_eq!(app.latest.as_ref().unwrap().processes[anchored_idx].pid, 20);
    }

    #[test]
    fn scroll_offset_clamps_when_anchored_process_disappears() {
        let mut app = test_app();
        app.update(press(KeyCode::Char('5')));

        let snapshot = snapshot_with_processes(vec![
            proc(10, "a.exe", 1.0, 1),
            proc(20, "b.exe", 2.0, 1),
            proc(30, "c.exe", 3.0, 1),
        ]);
        app.update(AppEvent::Snapshot(Box::new(snapshot)));
        // CpuDesc: pid 30 (3%) first, then pid 20, then pid 10 -> indices [2, 1, 0].
        assert_eq!(app.process_order.indices(), &[2, 1, 0]);
        app.scroll_offset = 2; // anchored on pid 10, the last row

        // pid 10 and pid 30 are gone; only one process remains.
        let next_snapshot = snapshot_with_processes(vec![proc(20, "b.exe", 2.0, 1)]);
        app.update(AppEvent::Snapshot(Box::new(next_snapshot)));

        assert_eq!(app.process_order.indices().len(), 1);
        assert_eq!(
            app.scroll_offset, 0,
            "offset must clamp into the shrunk list"
        );
    }

    #[test]
    fn process_order_recompute_does_not_disturb_other_panel_scroll_offset() {
        let mut app = test_app();
        app.update(press(KeyCode::Char('3'))); // focus Disk panel
        app.scroll_offset = 5; // pretend the user scrolled the disk list

        let snapshot =
            snapshot_with_processes(vec![proc(10, "a.exe", 1.0, 1), proc(20, "b.exe", 2.0, 1)]);
        app.update(AppEvent::Snapshot(Box::new(snapshot)));

        assert_eq!(
            app.scroll_offset, 5,
            "process order recompute must not touch scroll_offset for a different panel"
        );
        assert_eq!(app.process_order.indices().len(), 2);
    }
}
