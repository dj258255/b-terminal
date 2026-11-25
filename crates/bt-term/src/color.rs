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

/// 밑줄 색(SGR 58)을 RGB로 푼다. 팔레트 번호는 글자색과 같은 길(OSC 4로 바꾼 팔레트 포함)로 푼다.
/// 반전(SGR 7)은 밑줄 색을 바꾸지 않는다(Ghostty `Style.underlineColor`는 반전을 보지 않는다).
/// 흐림(SGR 2)은 글자처럼 `faint-opacity`로 바탕색 쪽으로 섞는다(Ghostty는 밑줄에도 같은 알파를 준다). `bg`는 칸의 바탕색
pub(crate) fn underline_rgb(
    theme: &Theme,
    overrides: &Colors,
    color: Option<Color>,
    flags: Flags,
    bg: Rgb,
) -> Option<Rgb> {
    let rgb = resolve(theme, overrides, color?);
    Some(if flags.contains(Flags::DIM) {
        rgb.mix(bg, 1.0 - theme.faint_opacity)
    } else {
        rgb
    })
}

/// 셀 하나의 전경·배경을 꾸밈(반전·숨김·굵은 글자 색)까지 반영해 푼다. 흐림은 `CellFlags::FAINT`로 따로 넘긴다
pub(crate) fn cell_colors(theme: &Theme, overrides: &Colors, fg: Color, bg: Color, flags: Flags) -> (Rgb, Rgb) {
    // 굵은 글자의 0~7번을 밝은 8~15번으로 올리는 것은 설정(bold-is-bright 또는 bold-color)을 따른다. Ghostty 기본은 끔.
    // bold-color가 색이면 굵은 기본 글자색은 그 색이고, 나머지 0~7번은 밝은 색이다
    let bold = flags.contains(Flags::BOLD);
    let brighten = bold && (theme.bold_is_bright || theme.bold_color.is_some());
    let bold_default = theme.bold_color.filter(|_| bold);
    let fg = if !brighten {
        fg
    } else {
        match fg {
            Color::Named(n) if (n as usize) < 8 => Color::Indexed(n as u8 + 8),
            Color::Indexed(i) if i < 8 => Color::Indexed(i + 8),
            other => other,
        }
    };
    let mut fg_rgb = match (bold_default, fg) {
        (Some(c), Color::Named(NamedColor::Foreground)) => c,
        _ => resolve(theme, overrides, fg),
    };
    let mut bg_rgb = resolve(theme, overrides, bg);
    // 흐림(SGR 2)은 여기서 색을 섞지 않는다. 그릴 때 마지막 글자색에 `faint-opacity`를 건다(#835, `bt_render::text_color`)
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

    /// #835: 흐림(SGR 2)은 엔진의 칸 색을 바꾸지 않는다. 그릴 때 마지막 글자색에 `faint-opacity`를 건다(Ghostty처럼)
    #[test]
    fn faint_does_not_change_cell_colors_and_inverse_keeps_the_full_foreground_as_background() {
        let o = Colors::default();
        let (fg, bg) = (
            Color::Named(NamedColor::Foreground),
            Color::Named(NamedColor::Background),
        );
        let t = Theme::default();
        assert_eq!(cell_colors(&t, &o, fg, bg, Flags::DIM), (t.foreground, t.background));
        // 반전: 바탕은 원래 글자색 그대로(흐려지지 않음), 글자는 원래 바탕색
        assert_eq!(
            cell_colors(&t, &o, fg, bg, Flags::DIM | Flags::INVERSE),
            (t.background, t.foreground)
        );
        let none = Theme {
            faint_opacity: 0.0,
            ..Theme::default()
        };
        assert_eq!(cell_colors(&none, &o, fg, bg, Flags::DIM).0, none.foreground);
    }

    #[test]
    fn bold_color_replaces_default_bold_text_and_brightens_the_rest() {
        let o = Colors::default();
        let bg = Color::Named(NamedColor::Background);
        let orange = Rgb::new(0xff, 0x80, 0);
        let t = Theme {
            bold_color: Some(orange),
            ..Theme::default()
        };
        // 기본 글자색의 굵은 글자는 지정한 색
        let fg = Color::Named(NamedColor::Foreground);
        assert_eq!(cell_colors(&t, &o, fg, bg, Flags::BOLD).0, orange);
        assert_eq!(
            cell_colors(&t, &o, fg, bg, Flags::empty()).0,
            t.foreground,
            "굵지 않으면 그대로"
        );
        // 나머지 굵은 0~7번 색은 밝은 색(Ghostty: "the rest of the bold colors to bright")
        let red = Color::Named(NamedColor::Red);
        assert_eq!(cell_colors(&t, &o, red, bg, Flags::BOLD).0, t.palette[9]);
        assert_eq!(cell_colors(&t, &o, Color::Indexed(2), bg, Flags::BOLD).0, t.palette[10]);
        // 24비트 색은 건드리지 않는다
        let spec = Color::Spec(VtRgb { r: 1, g: 2, b: 3 });
        assert_eq!(cell_colors(&t, &o, spec, bg, Flags::BOLD).0, Rgb::new(1, 2, 3));
        // 색 없이 bright만(bold-color = bright): 기본 글자색은 그대로
        let b = Theme {
            bold_is_bright: true,
            ..Theme::default()
        };
        assert_eq!(cell_colors(&b, &o, fg, bg, Flags::BOLD).0, b.foreground);
    }
}
