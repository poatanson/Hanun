use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::Paragraph;

use crate::model::DiskSnapshot;

const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;
const BYTES_PER_MIB: f64 = 1024.0 * 1024.0;

/// Draws the disk panel: one line per volume with capacity and I/O rate, scrolled by
/// `scroll_offset` when the list overflows the panel. Shows a placeholder until the first
/// sample arrives.
pub fn draw(frame: &mut Frame, area: Rect, disks: Option<&[DiskSnapshot]>, scroll_offset: usize) {
    let Some(disks) = disks else {
        frame.render_widget(Paragraph::new("sampling..."), area);
        return;
    };
    if disks.is_empty() {
        frame.render_widget(Paragraph::new("no disks found"), area);
        return;
    }

    let max_offset = disks.len().saturating_sub(1);
    let offset = scroll_offset.min(max_offset);
    let visible_rows = area.height as usize;

    let lines: Vec<String> = disks
        .iter()
        .skip(offset)
        .take(visible_rows)
        .map(|disk| {
            let label = if disk.name.is_empty() {
                disk.mount_point.clone()
            } else {
                format!("{} ({})", disk.mount_point, disk.name)
            };
            format!(
                "{:<12} {:>5.0}G/{:<5.0}G  R {:>6.1}M/s  W {:>6.1}M/s",
                label,
                gib(disk.available_bytes),
                gib(disk.total_bytes),
                mib(disk.read_bytes_per_sec),
                mib(disk.write_bytes_per_sec),
            )
        })
        .collect();

    frame.render_widget(Paragraph::new(lines.join("\n")), area);
}

fn gib(bytes: u64) -> f64 {
    bytes as f64 / BYTES_PER_GIB
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / BYTES_PER_MIB
}
