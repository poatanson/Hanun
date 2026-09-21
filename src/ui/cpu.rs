use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{LineGauge, Paragraph};

use crate::model::CpuSnapshot;
use crate::ui::theme;

/// Draws the CPU panel: a usage gauge plus per-core percentages. Shows a placeholder until
/// the first sample arrives.
pub fn draw(frame: &mut Frame, area: Rect, cpu: Option<&CpuSnapshot>, ascii_mode: bool) {
    let Some(cpu) = cpu else {
        frame.render_widget(Paragraph::new("sampling..."), area);
        return;
    };

    let [gauge_area, cores_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);

    let percent = cpu.global_usage_percent.clamp(0.0, 100.0);
    let label = match cpu.frequency_mhz {
        Some(mhz) => format!("{percent:.0}% ({} cores @ {mhz} MHz)", cpu.core_count),
        None => format!("{percent:.0}% ({} cores)", cpu.core_count),
    };
    let (filled, unfilled) = theme::gauge_symbols(ascii_mode);
    let gauge = LineGauge::default()
        .ratio(f64::from(percent) / 100.0)
        .filled_style(theme::style_for_usage(percent))
        .filled_symbol(filled)
        .unfilled_symbol(unfilled)
        .label(label);
    frame.render_widget(gauge, gauge_area);

    let cores_line = cpu
        .per_core_percent
        .iter()
        .enumerate()
        .map(|(i, pct)| format!("c{i}:{pct:.0}%"))
        .collect::<Vec<_>>()
        .join(" ");
    frame.render_widget(Paragraph::new(cores_line), cores_area);
}
