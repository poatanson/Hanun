use std::f64::consts::{FRAC_PI_2, TAU};

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::symbols::Marker;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::widgets::canvas::{Canvas, Points};

use crate::model::{
    ChartKind, CpuSnapshot, DiskSnapshot, MemSnapshot, aggregate_disk_usage, percent_of,
};
use crate::ui::theme;

/// Terminal cells are about twice as tall as they are wide; the donut's `y_bounds` are
/// widened by this factor relative to `x_bounds` so a circle doesn't render as an ellipse.
const CHART_CELL_ASPECT_RATIO: f64 = 2.0;
const OUTER_RADIUS: f64 = 0.9;
const INNER_RADIUS: f64 = OUTER_RADIUS * 0.5;
const RADIAL_SAMPLES: usize = 12;
const ANGULAR_SAMPLES_PER_TURN: usize = 180;
const LEGEND_HEIGHT: u16 = 2;

/// Below this width or height, the donut is replaced by a single-line text summary.
const CHART_MIN_WIDTH: u16 = 20;
const CHART_MIN_HEIGHT: u16 = 8;

/// Draws the chart panel: a donut of the currently selected resource's usage plus a
/// legend, or a plain text summary when the panel is too small for a legible donut. Shows
/// a placeholder until the first sample arrives.
pub fn draw(
    frame: &mut Frame,
    area: Rect,
    mem: Option<&MemSnapshot>,
    disks: Option<&[DiskSnapshot]>,
    cpu: Option<&CpuSnapshot>,
    kind: ChartKind,
    ascii_mode: bool,
) {
    let (Some(mem), Some(disks), Some(cpu)) = (mem, disks, cpu) else {
        frame.render_widget(Paragraph::new("sampling..."), area);
        return;
    };

    let primary_percent = match kind {
        ChartKind::Memory => percent_of(mem.used_bytes, mem.total_bytes),
        ChartKind::Disk => {
            let (used, total) = aggregate_disk_usage(disks);
            percent_of(used, total)
        }
        ChartKind::Cpu => cpu.global_usage_percent.clamp(0.0, 100.0) as f64,
    };

    if area.width < CHART_MIN_WIDTH || area.height < CHART_MIN_HEIGHT {
        // Too narrow/short for a legible donut+legend; a full three-metric summary would
        // itself get clipped at these widths, so show only the currently selected kind.
        let summary = format!("{} {:.0}%", kind.label(), primary_percent);
        frame.render_widget(Paragraph::new(summary), area);
        return;
    }

    let [donut_area, legend_area] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(LEGEND_HEIGHT)]).areas(area);

    draw_donut(frame, donut_area, primary_percent, kind, ascii_mode);
    draw_legend(frame, legend_area, primary_percent, kind, ascii_mode);
}

fn draw_donut(
    frame: &mut Frame,
    area: Rect,
    primary_percent: f64,
    kind: ChartKind,
    ascii_mode: bool,
) {
    let (primary_color, secondary_color) = theme::chart_slice_colors(kind);
    let primary_fraction = (primary_percent / 100.0).clamp(0.0, 1.0);

    let half_y = CHART_CELL_ASPECT_RATIO * area.height as f64 / area.width.max(1) as f64;
    let x_bounds = [-1.0, 1.0];
    let y_bounds = [-half_y, half_y];

    let primary_points = wedge_points(0.0, primary_fraction);
    let secondary_points = wedge_points(primary_fraction, 1.0);

    let label = format!("{:.0}%", primary_percent.round());
    let label_len = label.chars().count() as f64;
    let label_x = -label_len / area.width.max(1) as f64;

    let canvas = Canvas::default()
        .x_bounds(x_bounds)
        .y_bounds(y_bounds)
        .marker(if ascii_mode {
            Marker::Custom('#')
        } else {
            Marker::Braille
        })
        .paint(move |ctx| {
            ctx.draw(&Points {
                coords: &secondary_points,
                color: secondary_color,
            });
            ctx.draw(&Points {
                coords: &primary_points,
                color: primary_color,
            });
            ctx.print(label_x, 0.0, label.clone());
        });
    frame.render_widget(canvas, area);
}

/// Generates `(x, y)` points filling the donut ring between `INNER_RADIUS` and
/// `OUTER_RADIUS`, sweeping clockwise from 12 o'clock across the fraction range
/// `[frac_from, frac_to)` of a full turn (each in `0.0..=1.0`). Returns an empty `Vec` for
/// a degenerate (zero or negative) sweep, e.g. at 0% or 100%.
fn wedge_points(frac_from: f64, frac_to: f64) -> Vec<(f64, f64)> {
    let sweep = frac_to - frac_from;
    if sweep <= 0.0 {
        return Vec::new();
    }

    let angular_samples = ((sweep * ANGULAR_SAMPLES_PER_TURN as f64).ceil() as usize).max(1);
    let mut points = Vec::with_capacity((angular_samples + 1) * (RADIAL_SAMPLES + 1));
    for step in 0..=angular_samples {
        let t = frac_from + sweep * step as f64 / angular_samples as f64;
        let angle = FRAC_PI_2 - t * TAU;
        let (sin, cos) = angle.sin_cos();
        for radial_step in 0..=RADIAL_SAMPLES {
            let r = INNER_RADIUS
                + (OUTER_RADIUS - INNER_RADIUS) * radial_step as f64 / RADIAL_SAMPLES as f64;
            points.push((r * cos, r * sin));
        }
    }
    points
}

fn draw_legend(
    frame: &mut Frame,
    area: Rect,
    primary_percent: f64,
    kind: ChartKind,
    ascii_mode: bool,
) {
    let (primary_color, secondary_color) = theme::chart_slice_colors(kind);
    let swatch = if ascii_mode { "#" } else { "\u{25A0}" };

    let primary_line = Line::from(vec![
        Span::styled(swatch, Style::new().fg(primary_color)),
        Span::raw(format!(
            " {:<5} {:>5.1}%",
            kind.primary_label(),
            primary_percent
        )),
    ]);
    let secondary_line = Line::from(vec![
        Span::styled(swatch, Style::new().fg(secondary_color)),
        Span::raw(format!(
            " {:<5} {:>5.1}%",
            kind.secondary_label(),
            100.0 - primary_percent
        )),
    ]);

    frame.render_widget(Paragraph::new(vec![primary_line, secondary_line]), area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;

    use super::*;

    fn buffer_text(buffer: &Buffer) -> String {
        buffer
            .content()
            .iter()
            .map(ratatui::buffer::Cell::symbol)
            .collect()
    }

    fn mem(used_bytes: u64, total_bytes: u64) -> MemSnapshot {
        MemSnapshot {
            total_bytes,
            used_bytes,
            available_bytes: total_bytes - used_bytes,
            swap_total_bytes: 0,
            swap_used_bytes: 0,
        }
    }

    fn cpu(global_usage_percent: f32) -> CpuSnapshot {
        CpuSnapshot {
            global_usage_percent,
            per_core_percent: vec![],
            core_count: 1,
            frequency_mhz: None,
        }
    }

    fn render(width: u16, height: u16, f: impl FnOnce(&mut Frame, Rect)) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|frame| f(frame, frame.area())).unwrap();
        buffer_text(terminal.backend().buffer())
    }

    #[test]
    fn shows_sampling_placeholder_when_no_data() {
        let text = render(30, 12, |frame, area| {
            draw(frame, area, None, None, None, ChartKind::Memory, false);
        });
        assert!(text.contains("sampling..."));
    }

    #[test]
    fn falls_back_to_text_summary_when_area_too_small() {
        let mem = mem(50, 100);
        let cpu = cpu(42.0);
        let text = render(15, 6, |frame, area| {
            draw(
                frame,
                area,
                Some(&mem),
                Some(&[]),
                Some(&cpu),
                ChartKind::Memory,
                false,
            );
        });
        assert!(text.contains("MEMORY"));
        assert!(text.contains("50%"));
    }

    #[test]
    fn renders_legend_with_expected_percent_for_selected_kind() {
        let mem = mem(50, 100);
        let cpu = cpu(0.0);
        let text = render(30, 12, |frame, area| {
            draw(
                frame,
                area,
                Some(&mem),
                Some(&[]),
                Some(&cpu),
                ChartKind::Memory,
                false,
            );
        });
        assert!(text.contains("50.0%"));
        assert!(text.contains("USED"));
        assert!(text.contains("FREE"));
    }

    #[test]
    fn renders_at_minimum_chart_size_without_panicking() {
        let mem = mem(50, 100);
        let cpu = cpu(10.0);

        render(CHART_MIN_WIDTH - 1, CHART_MIN_HEIGHT - 1, |frame, area| {
            draw(
                frame,
                area,
                Some(&mem),
                Some(&[]),
                Some(&cpu),
                ChartKind::Memory,
                false,
            );
        });
        render(CHART_MIN_WIDTH, CHART_MIN_HEIGHT, |frame, area| {
            draw(
                frame,
                area,
                Some(&mem),
                Some(&[]),
                Some(&cpu),
                ChartKind::Memory,
                false,
            );
        });
    }
}
