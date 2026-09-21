use std::borrow::Cow;

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::Paragraph;

use crate::model::ProcessSnapshot;

const BYTES_PER_MIB: f64 = 1024.0 * 1024.0;
const NAME_WIDTH: usize = 24;

/// Draws the process panel: a header row plus the processes named by `order`, scrolled by
/// `scroll_offset` when the list overflows the panel. Shows a placeholder until the first
/// sample arrives. `order` is the already filtered+sorted index list into `processes`
/// (see `model::ProcessOrder`); this function does no filtering or sorting of its own and
/// only ever formats the rows that are actually visible.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    processes: Option<&[ProcessSnapshot]>,
    order: &[usize],
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

    let max_offset = order.len().saturating_sub(1);
    let offset = scroll_offset.min(max_offset);
    let visible_rows = list_area.height as usize;

    let lines: Vec<String> = order
        .iter()
        .skip(offset)
        .take(visible_rows)
        .map(|&idx| {
            let process = &processes[idx];
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

/// Returns `name` unchanged (borrowed, no allocation) when it already fits `max_chars`,
/// or a truncated copy otherwise.
fn truncate(name: &str, max_chars: usize) -> Cow<'_, str> {
    if name.chars().count() <= max_chars {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(name.chars().take(max_chars).collect())
    }
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / BYTES_PER_MIB
}

#[cfg(test)]
mod perf_probe {
    use std::time::{Duration, Instant};

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;

    use super::draw;
    use crate::model::{ProcessOrder, ProcessSnapshot, SortMode};

    fn synthetic_processes(count: usize) -> Vec<ProcessSnapshot> {
        (0..count)
            .map(|i| ProcessSnapshot {
                pid: i as u32 + 1,
                name: format!("synthetic-process-{i}.exe"),
                cpu_percent_raw: (i % 800) as f32 / 8.0,
                cpu_percent_normalized: (i % 100) as f32,
                mem_bytes: (i as u64 + 1) * 1024 * 1024,
                status: "Runnable".to_string(),
            })
            .collect()
    }

    /// Measures the steady-state per-frame cost of `process::draw()` with the process
    /// order already cached (the post-fix path: a `Tick` with no new snapshot, sort, or
    /// filter change reuses this same `order` every time). Ignored by default since it is
    /// a perf probe, not a correctness test; run explicitly to get numbers:
    /// `cargo test --release --bin hanun ui::process::perf_probe -- --ignored --nocapture`
    #[test]
    #[ignore = "perf probe: run explicitly with --ignored --nocapture"]
    fn bench_process_panel_render_with_cached_order() {
        for &count in &[50usize, 400, 1000] {
            let processes = synthetic_processes(count);
            let mut order = ProcessOrder::default();
            order.rebuild(&processes, "", SortMode::CpuDesc);

            let area = Rect::new(0, 0, 80, 40);
            let backend = TestBackend::new(80, 40);
            let mut terminal = Terminal::new(backend).expect("test backend");

            const ITERATIONS: u32 = 1000;
            let mut prep_total = Duration::ZERO;
            for _ in 0..ITERATIONS {
                terminal
                    .draw(|frame| {
                        let start = Instant::now();
                        draw(frame, area, Some(&processes), order.indices(), 0);
                        prep_total += start.elapsed();
                    })
                    .expect("draw");
            }
            println!(
                "[cached]  process_count={count:>5}  iterations={ITERATIONS}  prep_total={prep_total:?}  avg_prep_per_frame={:?}",
                prep_total / ITERATIONS
            );
        }
    }

    /// Measures `ProcessOrder::rebuild()` alone -- the cost that used to run on every
    /// `Tick` (4 Hz) and now runs only on an actual snapshot/sort/filter change.
    #[test]
    #[ignore = "perf probe: run explicitly with --ignored --nocapture"]
    fn bench_process_order_rebuild() {
        for &count in &[50usize, 400, 1000] {
            let processes = synthetic_processes(count);
            let mut order = ProcessOrder::default();

            const ITERATIONS: u32 = 1000;
            let start = Instant::now();
            for _ in 0..ITERATIONS {
                order.rebuild(&processes, "", SortMode::CpuDesc);
            }
            let total = start.elapsed();
            println!(
                "[rebuild] process_count={count:>5}  iterations={ITERATIONS}  total={total:?}  avg_per_rebuild={:?}",
                total / ITERATIONS
            );
        }
    }
}
