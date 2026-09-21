use ratatui::style::{Color, Style};
use ratatui::symbols::border;

use crate::model::ChartKind;

/// Usage percentage at or above this is rendered in the warning color.
pub const WARN_THRESHOLD: f32 = 70.0;
/// Usage percentage at or above this is rendered in the critical color.
pub const CRIT_THRESHOLD: f32 = 90.0;

/// Returns the style to use for a usage percentage, applying the warn/critical thresholds.
pub fn style_for_usage(percent: f32) -> Style {
    if percent >= CRIT_THRESHOLD {
        Style::new().fg(Color::Red)
    } else if percent >= WARN_THRESHOLD {
        Style::new().fg(Color::Yellow)
    } else {
        Style::new().fg(Color::Green)
    }
}

const ASCII_BORDER: border::Set = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

/// Border character set to use for panel/popup borders, honoring `--ascii`.
pub fn border_set(ascii_mode: bool) -> border::Set<'static> {
    if ascii_mode {
        ASCII_BORDER
    } else {
        border::PLAIN
    }
}

/// (filled, unfilled) symbols for `LineGauge` bars, honoring `--ascii`.
pub fn gauge_symbols(ascii_mode: bool) -> (&'static str, &'static str) {
    if ascii_mode {
        ("#", "-")
    } else {
        ("\u{2588}", "\u{2591}")
    }
}

/// Up/down throughput arrows, honoring `--ascii`.
pub fn arrows(ascii_mode: bool) -> (&'static str, &'static str) {
    if ascii_mode {
        ("D", "U")
    } else {
        ("\u{2193}", "\u{2191}")
    }
}

/// (primary, secondary) slice colors for the CHART panel's donut, per `ChartKind`. The
/// secondary ("free"/"idle") slice always uses a neutral gray so the accent color alone
/// identifies which resource is shown. All colors are basic ANSI-safe named colors (no RGB
/// truecolor) for Windows console (cmd.exe / Windows Terminal) compatibility.
pub fn chart_slice_colors(kind: ChartKind) -> (Color, Color) {
    let primary = match kind {
        ChartKind::Memory => Color::Cyan,
        ChartKind::Disk => Color::Magenta,
        ChartKind::Cpu => Color::Blue,
    };
    (primary, Color::DarkGray)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_below_warn_threshold_is_green() {
        assert_eq!(style_for_usage(0.0).fg, Some(Color::Green));
        assert_eq!(style_for_usage(WARN_THRESHOLD - 0.1).fg, Some(Color::Green));
    }

    #[test]
    fn usage_at_warn_threshold_is_yellow() {
        assert_eq!(style_for_usage(WARN_THRESHOLD).fg, Some(Color::Yellow));
        assert_eq!(
            style_for_usage(CRIT_THRESHOLD - 0.1).fg,
            Some(Color::Yellow)
        );
    }

    #[test]
    fn usage_at_crit_threshold_is_red() {
        assert_eq!(style_for_usage(CRIT_THRESHOLD).fg, Some(Color::Red));
        assert_eq!(style_for_usage(100.0).fg, Some(Color::Red));
    }

    #[test]
    fn chart_slice_colors_have_distinct_primary_per_kind() {
        let (mem_primary, mem_secondary) = chart_slice_colors(ChartKind::Memory);
        let (disk_primary, disk_secondary) = chart_slice_colors(ChartKind::Disk);
        let (cpu_primary, cpu_secondary) = chart_slice_colors(ChartKind::Cpu);

        assert_ne!(mem_primary, disk_primary);
        assert_ne!(disk_primary, cpu_primary);
        assert_ne!(mem_primary, cpu_primary);
        assert_eq!(mem_secondary, Color::DarkGray);
        assert_eq!(disk_secondary, Color::DarkGray);
        assert_eq!(cpu_secondary, Color::DarkGray);
    }
}
