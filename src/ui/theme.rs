use ratatui::style::{Color, Style};
use ratatui::symbols::border;

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
}
