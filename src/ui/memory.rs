use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{LineGauge, Paragraph};

use crate::model::MemSnapshot;
use crate::ui::theme;

const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Draws the memory panel: a RAM usage gauge plus a swap readout. Shows a placeholder until
/// the first sample arrives.
pub fn draw(frame: &mut Frame, area: Rect, mem: Option<&MemSnapshot>, ascii_mode: bool) {
    let Some(mem) = mem else {
        frame.render_widget(Paragraph::new("sampling..."), area);
        return;
    };

    let [gauge_area, swap_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);

    let percent = if mem.total_bytes == 0 {
        0.0
    } else {
        (mem.used_bytes as f64 / mem.total_bytes as f64 * 100.0).clamp(0.0, 100.0)
    };
    let (filled, unfilled) = theme::gauge_symbols(ascii_mode);
    let gauge = LineGauge::default()
        .ratio(percent / 100.0)
        .filled_style(theme::style_for_usage(percent as f32))
        .filled_symbol(filled)
        .unfilled_symbol(unfilled)
        .label(format!(
            "{:.1}G / {:.1}G ({percent:.0}%)",
            gib(mem.used_bytes),
            gib(mem.total_bytes)
        ));
    frame.render_widget(gauge, gauge_area);

    let swap_line = format!(
        "avail {:.1}G  swap {:.1}G / {:.1}G",
        gib(mem.available_bytes),
        gib(mem.swap_used_bytes),
        gib(mem.swap_total_bytes)
    );
    frame.render_widget(Paragraph::new(swap_line), swap_area);
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / BYTES_PER_GIB
}
