use std::sync::mpsc::Sender;
use std::time::Duration;

use crossterm::event::{KeyCode, KeyEvent};

use crate::event::AppEvent;
use crate::model::{PanelId, Snapshot, SortMode};
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
    sampler_cmd_tx: Sender<SamplerCommand>,
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
            sampler_cmd_tx,
        }
    }

    /// Applies one event to the application state.
    pub fn update(&mut self, event: AppEvent) {
        match event {
            AppEvent::Key(key) => self.handle_key(key),
            AppEvent::Snapshot(snapshot) => self.latest = Some(*snapshot),
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
            KeyCode::Char('s') => self.sort_mode = self.sort_mode.next(),
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
                self.filter.pop();
            }
            KeyCode::Char(c) => self.filter.push(c),
            _ => {}
        }
    }

    fn focus_next(&mut self) {
        let next = match self.focused_panel {
            PanelId::Cpu => PanelId::Memory,
            PanelId::Memory => PanelId::Disk,
            PanelId::Disk => PanelId::Network,
            PanelId::Network => PanelId::Process,
            PanelId::Process => PanelId::Cpu,
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

    use super::*;

    fn test_app() -> App {
        let (tx, _rx) = mpsc::channel();
        App::new(tx, false)
    }

    fn press(code: KeyCode) -> AppEvent {
        AppEvent::Key(KeyEvent::new(code, crossterm::event::KeyModifiers::NONE))
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
}
