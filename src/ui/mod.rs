use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::widgets::{Block, Paragraph};

use crate::app::App;
use crate::model::PanelId;

mod chart;
mod cpu;
mod disk;
mod help;
mod memory;
mod network;
mod process;
mod theme;

const MIN_WIDTH: u16 = 80;
const MIN_HEIGHT: u16 = 24;

/// Draws the full application frame: top bar, five panels, and the key-hint line. Below the
/// minimum supported size, draws a guidance screen instead.
pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        draw_too_small(frame, area);
        return;
    }

    let [top_bar, body, hint] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_top_bar(frame, top_bar, app);
    draw_body(frame, body, app);
    draw_hint(frame, hint, app.ascii_mode);

    if app.show_help {
        help::draw(frame, app.ascii_mode);
    }
}

fn draw_too_small(frame: &mut Frame, area: Rect) {
    let message = format!(
        "Terminal too small\nhanun needs at least {MIN_WIDTH} x {MIN_HEIGHT}\n(current: {} x {})\n\nResize, or press q",
        area.width, area.height
    );
    let block = Block::bordered().title(" hanun ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(message).centered(), inner);
}

fn draw_top_bar(frame: &mut Frame, area: Rect, app: &App) {
    let freshness = match app.latest.as_ref() {
        Some(snapshot) => format!(
            "updated {:.1}s ago",
            snapshot.sampled_at.elapsed().as_secs_f32()
        ),
        None => "sampling...".to_string(),
    };
    let state = if app.paused { "PAUSED" } else { "running" };
    let sep = if app.ascii_mode { "|" } else { "\u{b7}" };
    frame.render_widget(
        Paragraph::new(format!(
            " hanun  {sep} {}ms {sep} {state} {sep} {freshness}",
            app.tick_interval_ms
        )),
        area,
    );
}

fn draw_body(frame: &mut Frame, area: Rect, app: &App) {
    let [top, bottom, process_row] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(5),
        Constraint::Min(0),
    ])
    .areas(area);

    let [cpu_area, memory_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(top);
    let [disk_area, network_area] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(bottom);
    let [process, chart_area] =
        Layout::horizontal([Constraint::Percentage(65), Constraint::Percentage(35)])
            .areas(process_row);

    let cpu_inner = render_panel(
        frame,
        cpu_area,
        "[1] CPU",
        app.focused_panel == PanelId::Cpu,
        app.ascii_mode,
    );
    let memory_inner = render_panel(
        frame,
        memory_area,
        "[2] MEMORY",
        app.focused_panel == PanelId::Memory,
        app.ascii_mode,
    );
    let disk_inner = render_panel(
        frame,
        disk_area,
        "[3] DISK",
        app.focused_panel == PanelId::Disk,
        app.ascii_mode,
    );
    let network_inner = render_panel(
        frame,
        network_area,
        "[4] NETWORK",
        app.focused_panel == PanelId::Network,
        app.ascii_mode,
    );
    let filter_suffix = match (app.filter_editing, app.filter.is_empty()) {
        (true, _) => format!("  filter:{}_", app.filter),
        (false, false) => format!("  filter:{}", app.filter),
        (false, true) => String::new(),
    };
    let process_title = format!("[5] PROCESS  sort:{}{filter_suffix}", app.sort_mode.label());
    let process_inner = render_panel(
        frame,
        process,
        &process_title,
        app.focused_panel == PanelId::Process,
        app.ascii_mode,
    );
    let chart_title = format!("[6] CHART: {}", app.chart_kind.label());
    let chart_inner = render_panel(
        frame,
        chart_area,
        &chart_title,
        app.focused_panel == PanelId::Chart,
        app.ascii_mode,
    );

    let offset = |panel: PanelId| {
        if app.focused_panel == panel {
            app.scroll_offset
        } else {
            0
        }
    };

    let snapshot = app.latest.as_ref();
    cpu::draw(frame, cpu_inner, snapshot.map(|s| &s.cpu), app.ascii_mode);
    memory::draw(
        frame,
        memory_inner,
        snapshot.map(|s| &s.mem),
        app.ascii_mode,
    );
    disk::draw(
        frame,
        disk_inner,
        snapshot.map(|s| s.disks.as_slice()),
        offset(PanelId::Disk),
    );
    network::draw(
        frame,
        network_inner,
        snapshot.map(|s| s.net.interfaces.as_slice()),
        offset(PanelId::Network),
        app.ascii_mode,
    );
    process::draw(
        frame,
        process_inner,
        snapshot.map(|s| s.processes.as_slice()),
        app.process_order.indices(),
        offset(PanelId::Process),
    );
    chart::draw(
        frame,
        chart_inner,
        snapshot.map(|s| &s.mem),
        snapshot.map(|s| s.disks.as_slice()),
        snapshot.map(|s| &s.cpu),
        app.chart_kind,
        app.ascii_mode,
    );
}

/// Renders a bordered, titled panel and returns its interior area for the panel's own draw
/// function to fill.
fn render_panel(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    focused: bool,
    ascii_mode: bool,
) -> Rect {
    let border_style = if focused {
        Style::new().bold()
    } else {
        Style::new()
    };
    let block = Block::bordered()
        .title(title)
        .border_style(border_style)
        .border_set(theme::border_set(ascii_mode));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    inner
}

fn draw_hint(frame: &mut Frame, area: Rect, ascii_mode: bool) {
    let (up, down) = if ascii_mode {
        ("U", "D")
    } else {
        ("\u{2191}", "\u{2193}")
    };
    let hint = format!(
        "q Quit Tab/1-6 Focus {up}{down}jk Scroll s Sort / Filter p Pause +/- Rate g Chart ? Help"
    );
    frame.render_widget(Paragraph::new(hint), area);
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    use super::draw;
    use crate::app::App;

    fn test_app() -> App {
        let (tx, _rx) = mpsc::channel();
        App::new(tx, false)
    }

    fn buffer_text(buffer: &Buffer) -> String {
        buffer
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    #[test]
    fn shows_guidance_below_minimum_size() {
        let backend = TestBackend::new(60, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = test_app();

        terminal.draw(|frame| draw(frame, &app)).unwrap();

        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("Terminal too small"));
        assert!(text.contains("80 x 24"));
    }

    #[test]
    fn renders_all_panels_at_minimum_size() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = test_app();

        terminal.draw(|frame| draw(frame, &app)).unwrap();

        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("hanun"));
        assert!(text.contains("CPU"));
        assert!(text.contains("MEMORY"));
        assert!(text.contains("DISK"));
        assert!(text.contains("NETWORK"));
        assert!(text.contains("PROCESS"));
        assert!(text.contains("CHART"));
        assert!(!text.contains("Terminal too small"));
    }

    #[test]
    fn help_overlay_shows_when_toggled() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = test_app();
        app.show_help = true;

        terminal.draw(|frame| draw(frame, &app)).unwrap();

        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains("Help"));
    }

    #[test]
    fn ascii_mode_uses_plus_minus_borders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let (tx, _rx) = mpsc::channel();
        let app = App::new(tx, true);

        terminal.draw(|frame| draw(frame, &app)).unwrap();

        let text = buffer_text(terminal.backend().buffer());
        assert!(text.contains('+'));
        assert!(!text.contains('\u{2500}'));
    }
}
