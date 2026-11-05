//! VT 엔진의 색 값을 테마로 RGB로 푼다

use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb as VtRgb};
use bt_core::{Rgb, Theme};

fn from_vt(c: VtRgb) -> Rgb {
    Rgb::new(c.r, c.g, c.b)
}

/// 이름 색(0~15번, 전경·배경·커서와 흐린·밝은 변형)을 푼다. OSC 4/10/11로 바꾼 값이 있으면 그것이 먼저다
pub(crate) fn named(theme: &Theme, overrides: &Colors, name: NamedColor) -> Rgb {
    if let Some(c) = overrides[name] {
        return from_vt(c);
    }
    let idx = name as usize;
    match name {
        NamedColor::Foreground | NamedColor::BrightForeground => theme.foreground,
        NamedColor::Background => theme.background,
        NamedColor::Cursor => theme.cursor,
        NamedColor::DimForeground => theme.foreground.mix(theme.background, 0.35),
        _ if idx < 16 => theme.palette[idx],
        _ => {
            // DimBlack..DimWhite: 같은 번호의 기본색을 바탕 쪽으로 흐린다
            let base = idx - NamedColor::DimBlack as usize;
            theme.palette[base.min(7)].mix(theme.background, 0.35)
        }
    }
}

pub(crate) fn resolve(theme: &Theme, overrides: &Colors, color: Color) -> Rgb {
    match color {
        Color::Spec(c) => from_vt(c),
        Color::Indexed(i) => match overrides[i as usize] {
            Some(c) => from_vt(c),
            None => theme.indexed(i),
        },
        Color::Named(n) => named(theme, overrides, n),
    }
}

/// 셀 하나의 전경·배경을 꾸밈까지 반영해 푼다
pub(crate) fn cell_colors(theme: &Theme, overrides: &Colors, fg: Color, bg: Color, flags: Flags) -> (Rgb, Rgb) {
    // 굵은 글자의 0~7번을 밝은 8~15번으로 올리는 것은 설정(bold-is-bright)을 따른다. Ghostty 기본은 끔
    let fg = if !theme.bold_is_bright {
        fg
    } else {
        match fg {
            Color::Named(n) if flags.contains(Flags::BOLD) && (n as usize) < 8 => Color::Indexed(n as u8 + 8),
            Color::Indexed(i) if flags.contains(Flags::BOLD) && i < 8 => Color::Indexed(i + 8),
            other => other,
        }
    };
    let mut fg_rgb = resolve(theme, overrides, fg);
    let mut bg_rgb = resolve(theme, overrides, bg);
    if flags.contains(Flags::DIM) {
        fg_rgb = fg_rgb.mix(bg_rgb, 0.4);
    }
    if flags.contains(Flags::INVERSE) {
        std::mem::swap(&mut fg_rgb, &mut bg_rgb);
    }
    if flags.contains(Flags::HIDDEN) {
        fg_rgb = bg_rgb;
    }
    (fg_rgb, bg_rgb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inverse_swaps_and_bold_brightens() {
        let t = Theme::default();
        let o = Colors::default();
        let (fg, bg) = cell_colors(
            &t,
            &o,
            Color::Named(NamedColor::Foreground),
            Color::Named(NamedColor::Background),
            Flags::INVERSE,
        );
        assert_eq!((fg, bg), (t.background, t.foreground));
        let bright = Theme {
            bold_is_bright: true,
            ..Theme::default()
        };
        let (plain, _) = cell_colors(
            &t,
            &o,
            Color::Named(NamedColor::Red),
            Color::Named(NamedColor::Background),
            Flags::BOLD,
        );
        assert_eq!(plain, t.palette[1], "기본(Ghostty)은 굵어도 밝게 올리지 않는다");
        let t = bright;
        let (fg, _) = cell_colors(
            &t,
            &o,
            Color::Named(NamedColor::Red),
            Color::Named(NamedColor::Background),
            Flags::BOLD,
        );
        assert_eq!(fg, t.palette[9]);
    }
}
