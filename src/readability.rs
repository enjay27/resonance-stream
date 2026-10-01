//! Text that stays readable over the game.
//!
//! The window can be nearly transparent (`overlay_opacity`) over a bright
//! scene, so chat text cannot rely on the window background. It sits on a
//! dark text box instead -- compact mode always, normal mode once the window
//! is more see-through than [`BACKING_BELOW_OPACITY`]. The box's colours are
//! checked here against the worst case, pure white behind the box, with the
//! WCAG 2 contrast formula.

use crate::ui_types::Channel;

/// An sRGB colour.
pub type Rgb = [u8; 3];

/// Opacity of the black text box (`TEXT_BOX`).
pub const BOX_ALPHA: f64 = 0.70;
/// Opacity of the white original-message text on the box (`ORIGINAL_TEXT`).
pub const ORIGINAL_ALPHA: f64 = 0.75;
/// Below this `overlay_opacity`, normal-mode rows sit on the text box too.
pub const BACKING_BELOW_OPACITY: f32 = 0.5;

/// The text box. Its alpha is `BOX_ALPHA` (pinned by a test); the shadow only
/// adds to the checked contrast.
pub const TEXT_BOX: &str =
    "bg-black/70 text-white rounded-lg [text-shadow:0_1px_2px_rgb(0_0_0/0.9)]";
/// The original message under its translation, on the box.
pub const ORIGINAL_TEXT: &str = "text-white/75";

/// Whether normal-mode rows need the text box at this window opacity.
pub fn needs_backing(overlay_opacity: f32) -> bool {
    overlay_opacity < BACKING_BELOW_OPACITY
}

/// Opacity of the title bar and tab bar behind their text once the window is
/// see-through: near solid, in the theme's own colour, so the theme's text
/// colours keep the contrast they were made for.
pub const CHROME_BACKED_ALPHA: f64 = 0.95;
pub const CHROME_BACKED: &str = "bg-base-300/95";

/// Title bar background at this window opacity.
pub fn title_bar_bg(overlay_opacity: f32) -> &'static str {
    if needs_backing(overlay_opacity) {
        CHROME_BACKED
    } else {
        "bg-base-300/60"
    }
}

/// Tab bar background at this window opacity (normal mode; none by default).
pub fn nav_bar_bg(overlay_opacity: f32) -> &'static str {
    if needs_backing(overlay_opacity) {
        CHROME_BACKED
    } else {
        ""
    }
}

/// Sender-name class on the box and its colour (Tailwind's -300 shades),
/// lighter than the theme's channel colours so they hold up on black.
pub fn box_name(channel: Channel) -> (&'static str, Rgb) {
    match channel {
        Channel::World => ("text-purple-300", [0xd8, 0xb4, 0xfe]),
        Channel::Guild => ("text-emerald-300", [0x6e, 0xe7, 0xb7]),
        Channel::Party => ("text-sky-300", [0x7d, 0xd3, 0xfc]),
        Channel::Local => ("text-neutral-200", [0xe5, 0xe5, 0xe5]),
        Channel::Beginner => ("text-amber-300", [0xfc, 0xd3, 0x4d]),
    }
}

/// A normal-mode row's size and position. Fixed: a row must not change size
/// when the opacity slider crosses [`BACKING_BELOW_OPACITY`].
pub const ROW_LAYOUT: &str = "ml-1.5 px-2.5 py-1 w-fit max-w-[calc(100%-0.5rem)]";

/// Classes of a normal-mode row: the theme's own, or the text box's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowPalette {
    /// The row's size and position -- the same whatever the opacity.
    pub layout: &'static str,
    /// Whether the row sits on the dark text box.
    pub backed: bool,
    /// Colours only (background, shadow, rounding), never size.
    pub container: &'static str,
    pub text: &'static str,
    pub original: &'static str,
    pub meta: &'static str,
}

pub fn row_palette(backed: bool) -> RowPalette {
    if backed {
        RowPalette {
            layout: ROW_LAYOUT,
            backed: true,
            container: TEXT_BOX,
            text: "text-white",
            original: ORIGINAL_TEXT,
            meta: "text-white/60",
        }
    } else {
        RowPalette {
            layout: ROW_LAYOUT,
            backed: false,
            container: "rounded-lg hover:bg-base-content/5",
            text: "text-base-content",
            original: "text-base-content/45",
            meta: "text-base-content/40",
        }
    }
}

/// `fg` at `alpha` over `bg`.
pub fn blend(fg: Rgb, alpha: f64, bg: Rgb) -> Rgb {
    let mix = |f: u8, b: u8| (f as f64 * alpha + b as f64 * (1.0 - alpha)).round() as u8;
    [mix(fg[0], bg[0]), mix(fg[1], bg[1]), mix(fg[2], bg[2])]
}

/// The text box over a scene of colour `scene`.
pub fn box_over(scene: Rgb) -> Rgb {
    blend([0, 0, 0], BOX_ALPHA, scene)
}

fn luminance(c: Rgb) -> f64 {
    let lin = |v: u8| {
        let s = v as f64 / 255.0;
        if s <= 0.04045 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// WCAG 2 contrast ratio, 1..=21.
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

#[cfg(test)]
fn pct(alpha: f64) -> u32 {
    (alpha * 100.0).round() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_types::Channel;

    const WHITE: Rgb = [255, 255, 255];

    #[test]
    fn main_text_on_the_box_meets_aaa_over_a_white_scene() {
        let bg = box_over(WHITE);
        assert!(contrast(WHITE, bg) >= 7.0, "{}", contrast(WHITE, bg));
    }

    #[test]
    fn original_text_on_the_box_meets_aa_over_a_white_scene() {
        let bg = box_over(WHITE);
        let fg = blend(WHITE, ORIGINAL_ALPHA, bg);
        assert!(contrast(fg, bg) >= 4.5, "{}", contrast(fg, bg));
    }

    #[test]
    fn every_channel_name_on_the_box_meets_aa_over_a_white_scene() {
        let bg = box_over(WHITE);
        for channel in Channel::ALL {
            let (_, rgb) = box_name(channel);
            assert!(
                contrast(rgb, bg) >= 4.5,
                "{channel:?}: {}",
                contrast(rgb, bg)
            );
        }
    }

    #[test]
    fn the_box_classes_carry_the_checked_alphas() {
        assert!(TEXT_BOX
            .split_whitespace()
            .any(|c| c == format!("bg-black/{}", pct(BOX_ALPHA))));
        assert_eq!(ORIGINAL_TEXT, format!("text-white/{}", pct(ORIGINAL_ALPHA)));
    }

    #[test]
    fn normal_rows_get_the_box_only_when_the_window_is_see_through() {
        assert!(needs_backing(0.0));
        assert!(needs_backing(BACKING_BELOW_OPACITY - 0.01));
        assert!(!needs_backing(BACKING_BELOW_OPACITY));
        assert!(
            !needs_backing(0.85),
            "the default opacity keeps the plain look"
        );
    }

    #[test]
    fn a_backed_row_uses_the_box_palette_and_a_plain_row_the_theme() {
        let backed = row_palette(true);
        assert_eq!(
            (backed.text, backed.original),
            ("text-white", ORIGINAL_TEXT)
        );
        assert!(backed.container.contains("bg-black/"));
        let plain = row_palette(false);
        assert_eq!(plain.text, "text-base-content");
        assert!(!plain.container.contains("bg-black/"));
    }

    /// A class that changes the size of the box it is on.
    fn sizes_the_box(class: &str) -> bool {
        if class.starts_with('[') {
            return false; // arbitrary property, e.g. the text shadow
        }
        let name = class.rsplit(':').next().unwrap_or(class);
        [
            "p", "px", "py", "pl", "pr", "pt", "pb", "m", "mx", "my", "ml", "mr", "mt", "mb", "w",
            "h", "min-w", "max-w", "min-h", "max-h", "border", "gap", "size",
        ]
        .iter()
        .any(|prefix| name == *prefix || name.starts_with(&format!("{prefix}-")))
    }

    #[test]
    fn a_rows_size_does_not_change_with_the_window_opacity() {
        let (backed, plain) = (row_palette(true), row_palette(false));
        assert!(!backed.layout.is_empty());
        assert_eq!(backed.layout, plain.layout);
        // Only colours switch with the opacity: nothing that sizes the box.
        for p in [backed, plain] {
            for class in p.container.split_whitespace() {
                assert!(!sizes_the_box(class), "{class}");
            }
        }
    }

    #[test]
    fn contrast_matches_known_wcag_values() {
        assert!((contrast(WHITE, [0, 0, 0]) - 21.0).abs() < 0.01);
        assert!((contrast(WHITE, WHITE) - 1.0).abs() < 0.001);
        assert!((contrast([0x76, 0x76, 0x76], WHITE) - 4.54).abs() < 0.02);
    }

    #[test]
    fn the_bars_turn_near_solid_when_the_window_is_see_through() {
        let low = BACKING_BELOW_OPACITY - 0.01;
        assert_eq!(title_bar_bg(low), CHROME_BACKED);
        assert_eq!(nav_bar_bg(low), CHROME_BACKED);
        assert_eq!(title_bar_bg(0.85), "bg-base-300/60", "plain look kept");
        assert_eq!(nav_bar_bg(0.85), "");
    }

    #[test]
    fn backed_bars_let_at_most_a_tenth_of_the_scene_through() {
        // The theme's text colours are made for base-300; at >= 90 % the
        // scene behind shifts the bar by at most a tenth.
        assert!(CHROME_BACKED_ALPHA >= 0.9);
        assert_eq!(
            CHROME_BACKED,
            format!("bg-base-300/{}", pct(CHROME_BACKED_ALPHA))
        );
    }
}
