use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::Paragraph;

use crate::model::{ProcessSnapshot, SortMode};

const BYTES_PER_MIB: f64 = 1024.0 * 1024.0;
const NAME_WIDTH: usize = 24;

/// Draws the process panel: a header row plus processes matching `filter`, sorted by
/// `sort_mode` and scrolled by `scroll_offset` when the list overflows the panel. Shows a
/// placeholder until the first sample arrives.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    processes: Option<&[ProcessSnapshot]>,
    sort_mode: SortMode,
    filter: &str,
    scroll_offset: usize,
) {
    let Some(processes) = processes else {
        frame.render_widget(Paragraph::new("sampling..."), area);
        return;
    };

    let [header_area, list_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);

    frame.render_widget(
        Paragraph::new(format!(
            "{:>7} {:<NAME_WIDTH$} {:>6} {:>9} {:<8}",
            "PID", "NAME", "CPU%", "MEM", "STATUS"
        )),
        header_area,
    );

    let filter_lower = filter.to_lowercase();
    let mut filtered: Vec<&ProcessSnapshot> = processes
        .iter()
        .filter(|process| {
            filter_lower.is_empty() || process.name.to_lowercase().contains(&filter_lower)
        })
        .collect();
    sort(&mut filtered, sort_mode);

    let max_offset = filtered.len().saturating_sub(1);
    let offset = scroll_offset.min(max_offset);
    let visible_rows = list_area.height as usize;

    let lines: Vec<String> = filtered
        .iter()
        .skip(offset)
        .take(visible_rows)
        .map(|process| {
            format!(
                "{:>7} {:<NAME_WIDTH$} {:>5.1} {:>7.1}M {:<8}",
                process.pid,
                truncate(&process.name, NAME_WIDTH),
                process.cpu_percent_normalized,
                mib(process.mem_bytes),
                process.status,
            )
        })
        .collect();

    frame.render_widget(Paragraph::new(lines.join("\n")), list_area);
}

fn sort(processes: &mut [&ProcessSnapshot], mode: SortMode) {
    match mode {
        SortMode::CpuDesc => {
            processes.sort_by(|a, b| {
                b.cpu_percent_normalized
                    .total_cmp(&a.cpu_percent_normalized)
            });
        }
        SortMode::MemDesc => processes.sort_by_key(|process| std::cmp::Reverse(process.mem_bytes)),
        SortMode::PidAsc => processes.sort_by_key(|process| process.pid),
        SortMode::NameAsc => processes.sort_by_key(|process| process.name.to_lowercase()),
    }
}

fn truncate(name: &str, max_chars: usize) -> String {
    if name.chars().count() <= max_chars {
        name.to_string()
    } else {
        name.chars().take(max_chars).collect()
    }
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / BYTES_PER_MIB
}
