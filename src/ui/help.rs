use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::ui::theme;

const HELP_TEXT: &str = "\
q / Esc      Quit
Tab, 1-6     Focus panel
\u{2191}\u{2193}, j k       Scroll focused list
s            Cycle sort (CPU/Mem/PID/Name)
/            Filter processes (Enter/Esc to close)
p            Pause / resume sampling
+ / -        Adjust refresh interval (500-5000ms)
g            Cycle chart (Mem/Disk/CPU)
?            Toggle this help";

/// Draws a centered help overlay listing all keybindings.
pub fn draw(frame: &mut Frame, ascii_mode: bool) {
    let area = centered_rect(frame.area(), 44, 12);
    frame.render_widget(Clear, area);
    let block = Block::bordered()
        .title(" Help ")
        .border_set(theme::border_set(ascii_mode));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(Paragraph::new(HELP_TEXT), inner);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}
