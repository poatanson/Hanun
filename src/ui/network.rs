use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;

use crate::model::NetInterfaceSnapshot;
use crate::ui::theme;

const BYTES_PER_KIB: f64 = 1024.0;

/// Draws the network panel: one line per interface with rx/tx throughput, scrolled by
/// `scroll_offset` when the list overflows the panel. Shows a placeholder until the first
/// sample arrives.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    interfaces: Option<&[NetInterfaceSnapshot]>,
    scroll_offset: usize,
    ascii_mode: bool,
) {
    let Some(interfaces) = interfaces else {
        frame.render_widget(Paragraph::new("sampling..."), area);
        return;
    };
    if interfaces.is_empty() {
        frame.render_widget(Paragraph::new("no interfaces found"), area);
        return;
    }

    let max_offset = interfaces.len().saturating_sub(1);
    let offset = scroll_offset.min(max_offset);
    let visible_rows = area.height as usize;

    let (down, up) = theme::arrows(ascii_mode);
    let lines: Vec<String> = interfaces
        .iter()
        .skip(offset)
        .take(visible_rows)
        .map(|iface| {
            format!(
                "{:<12} {down} {:>7.1}K/s  {up} {:>7.1}K/s",
                iface.name,
                kib(iface.rx_bytes_per_sec),
                kib(iface.tx_bytes_per_sec),
            )
        })
        .collect();

    frame.render_widget(Paragraph::new(lines.join("\n")), area);
}

fn kib(bytes: u64) -> f64 {
    bytes as f64 / BYTES_PER_KIB
}
