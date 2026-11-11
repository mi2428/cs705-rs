//! Catppuccin Mocha, official palette 1.8.0. No runtime theme dependency.
//! <https://github.com/catppuccin/palette>
//! Roles: <https://github.com/catppuccin/catppuccin/blob/main/docs/style-guide.md>

use ratatui::{
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders},
};

pub const CRUST: Color = Color::Rgb(17, 17, 27);
pub const SURFACE0: Color = Color::Rgb(49, 50, 68);
pub const TEXT: Color = Color::Rgb(205, 214, 244);
pub const SUBTEXT0: Color = Color::Rgb(166, 173, 200);
pub const SUBTEXT1: Color = Color::Rgb(186, 194, 222);
pub const OVERLAY0: Color = Color::Rgb(108, 112, 134);
pub const BLUE: Color = Color::Rgb(137, 180, 250);
pub const MAUVE: Color = Color::Rgb(203, 166, 247);
pub const LAVENDER: Color = Color::Rgb(180, 190, 254);
pub const GREEN: Color = Color::Rgb(166, 227, 161);
pub const YELLOW: Color = Color::Rgb(249, 226, 175);
pub const RED: Color = Color::Rgb(243, 139, 168);

pub fn base() -> Style {
    // Leave the canvas, panels and dialogs on the terminal's own background.
    Style::default().fg(TEXT).bg(Color::Reset)
}
pub fn selected(active: bool) -> Style {
    // Truecolor terminals have no alpha channel. Palette surfaces provide the
    // selection background without replacing warning/error foreground colors.
    Style::default()
        .bg(if active { SURFACE0 } else { CRUST })
        .add_modifier(Modifier::BOLD)
}
pub fn panel(title: impl Into<String>, active: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .title(format!(" {} ", title.into()))
        .style(base())
        .border_style(base().fg(if active { LAVENDER } else { OVERLAY0 }))
        .title_style(
            base()
                .fg(if active { LAVENDER } else { SUBTEXT1 })
                .add_modifier(Modifier::BOLD),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mocha_selection_preserves_semantic_foregrounds() {
        assert_eq!(base().bg, Some(Color::Reset));
        for color in [TEXT, YELLOW, RED] {
            for active in [false, true] {
                let style = base().fg(color).patch(selected(active));
                assert_eq!(style.fg, Some(color));
                let background = if active { SURFACE0 } else { CRUST };
                assert_eq!(style.bg, Some(background));
                let foreground = luminance(color);
                let background = luminance(background);
                let contrast =
                    (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05);
                assert!(
                    contrast >= 4.5,
                    "selected semantic text contrast: {contrast}"
                );
            }
        }
    }

    fn luminance(color: Color) -> f64 {
        let Color::Rgb(r, g, b) = color else {
            panic!("Mocha requires explicit RGB colors")
        };
        [r, g, b]
            .into_iter()
            .zip([0.2126, 0.7152, 0.0722])
            .map(|(channel, weight)| {
                let value = f64::from(channel) / 255.0;
                weight
                    * if value <= 0.04045 {
                        value / 12.92
                    } else {
                        ((value + 0.055) / 1.055).powf(2.4)
                    }
            })
            .sum()
    }
}
