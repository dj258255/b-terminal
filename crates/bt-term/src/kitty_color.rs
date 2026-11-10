//! kitty 색 프로토콜(OSC 21, #266). 해석과 적용을 한다. 요청 형식과 상한은 Ghostty의
//! `src/terminal/osc/parsers/kitty_color.zig`·`src/terminal/kitty/color.zig`, 답은 `stream_handler.zig`의 `kittyColorReport`를 따른다.
//!
//! 보안: 답에는 요청의 글을 넣지 않는다. 알아본 키를 이 파일의 이름으로 다시 적고, 색은 숫자 세 개를 16진수로 적는다.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::term::Term;
use alacritty_terminal::vte::ansi::{Handler, NamedColor, Rgb as VtRgb};
use bt_core::Theme;

/// 이름으로 고르는 색(Ghostty `kitty/color.zig`의 `Special`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Special {
    Foreground,
    Background,
    SelectionForeground,
    SelectionBackground,
    Cursor,
    CursorText,
    VisualBell,
    SecondTransparentBackground,
}

/// 요청이 가리키는 색: 팔레트 0~255번이나 이름 색
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Palette(u8),
    Special(Special),
}

/// 요청 항목 하나. 색은 (r, g, b)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// `키=?`
    Query(Key),
    /// `키=색`
    Set(Key, (u8, u8, u8)),
    /// `키=`(값이 비었거나 `=`가 없다)
    Reset(Key),
}

/// 요청 하나에 받는 항목 수의 상한. Ghostty: `(255 + 이름 색 수) * 2`. 넘으면 요청 전체를 버린다
pub const MAX_ITEMS: usize = (255 + 8) * 2;

impl Special {
    fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "foreground" => Self::Foreground,
            "background" => Self::Background,
            "selection_foreground" => Self::SelectionForeground,
            "selection_background" => Self::SelectionBackground,
            "cursor" => Self::Cursor,
            "cursor_text" => Self::CursorText,
            "visual_bell" => Self::VisualBell,
            "second_transparent_background" => Self::SecondTransparentBackground,
            _ => return None,
        })
    }

    /// 답에 적는 이름. 요청에서 온 글이 아니라 이 표의 글이다
    fn name(self) -> &'static str {
        match self {
            Self::Foreground => "foreground",
            Self::Background => "background",
            Self::SelectionForeground => "selection_foreground",
            Self::SelectionBackground => "selection_background",
            Self::Cursor => "cursor",
            Self::CursorText => "cursor_text",
            Self::VisualBell => "visual_bell",
            Self::SecondTransparentBackground => "second_transparent_background",
        }
    }
}

impl Key {
    /// 이름이거나 10진수 0~255. 부호·밑줄·공백은 받지 않는다
    fn parse(s: &str) -> Option<Self> {
        if let Some(sp) = Special::from_name(s) {
            return Some(Self::Special(sp));
        }
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        s.parse::<u8>().ok().map(Self::Palette)
    }

    /// 엔진의 바꾼 색 칸 번호. 엔진이 들고 있지 않은 색은 없다
    fn engine_index(self) -> Option<usize> {
        match self {
            Self::Palette(n) => Some(n as usize),
            Self::Special(Special::Foreground) => Some(NamedColor::Foreground as usize),
            Self::Special(Special::Background) => Some(NamedColor::Background as usize),
            Self::Special(Special::Cursor) => Some(NamedColor::Cursor as usize),
            Self::Special(_) => None,
        }
    }

    fn write_name(self, out: &mut String) {
        match self {
            Self::Palette(n) => out.push_str(&n.to_string()),
            Self::Special(s) => out.push_str(s.name()),
        }
    }
}

/// OSC 21의 `21;` 뒤 글을 요청 목록으로 읽는다. 항목은 `;`로 나뉘고 `키=값`이다.
/// 키를 모르거나 색이 틀린 항목은 그 항목만 버린다. 항목이 [`MAX_ITEMS`]개를 넘으면 요청 전체를 버린다(없음)
pub fn parse(data: &str) -> Option<Vec<Request>> {
    let mut list = Vec::new();
    for kv in data.split(';') {
        if list.len() >= MAX_ITEMS {
            return None;
        }
        let (k, rest) = kv.split_once('=').unwrap_or((kv, ""));
        let Some(key) = Key::parse(k) else {
            continue;
        };
        let value = rest.trim_matches(' ');
        if value.is_empty() {
            list.push(Request::Reset(key));
        } else if value == "?" {
            list.push(Request::Query(key));
        } else if let Some(c) = parse_color(value) {
            list.push(Request::Set(key, c));
        }
    }
    Some(list)
}

/// X11 색 이름 표(`r g b 이름`). `+list-colors`와 같은 파일이다
const X11_COLORS: &str = include_str!("../../bt-app/src/x11_colors.txt");

/// 16진수 1~4자리를 0~255로 줄인다. 버림이다(Ghostty color.zig `fromHex`: 값*255/최댓값)
fn scale_hex(digits: &str) -> Option<u8> {
    if digits.is_empty() || digits.len() > 4 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let v = u32::from_str_radix(digits, 16).ok()?;
    let max = (1u32 << (4 * digits.len())) - 1;
    Some((v * 255 / max) as u8)
}

/// 0~1 소수를 0~255로. 부호(`+`·`-`)와 숫자, 소수점 하나만 받고 숫자가 하나는 있어야 한다. 버림이다
/// (Ghostty fraction.zig `parse`, color.zig `fromIntensity`)
fn scale_fraction(s: &str) -> Option<u8> {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    let digits = body.bytes().filter(u8::is_ascii_digit).count();
    let dots = body.bytes().filter(|&b| b == b'.').count();
    if digits == 0 || dots > 1 || digits + dots != body.len() {
        return None;
    }
    let f: f64 = s.parse().ok()?;
    (0.0..=1.0).contains(&f).then_some((f * 255.0) as u8)
}

/// 색 글(Ghostty color.zig `RGB.parse`): 앞뒤 빈칸·탭을 자르고, `#`과 16진수 3·6·9·12자리, X11 색 이름(대소문자와
/// 빈칸 무시), `#` 없는 16진수 3·6자리, `rgb:r/g/b`(칸당 1~4자리), `rgbi:r/g/b`(0~1 소수) 순서로 본다
pub fn parse_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim_matches([' ', '\t']);
    if s.is_empty() {
        return None;
    }
    let thirds = |hex: &str| -> Option<(u8, u8, u8)> {
        let n = hex.len();
        if n == 0 || n % 3 != 0 || n > 12 || !hex.is_ascii() {
            return None;
        }
        let w = n / 3;
        Some((
            scale_hex(&hex[..w])?,
            scale_hex(&hex[w..2 * w])?,
            scale_hex(&hex[2 * w..])?,
        ))
    };
    if let Some(hex) = s.strip_prefix('#') {
        return thirds(hex);
    }
    // rgb.txt의 빈칸 든 이름(106개)은 모두 빈칸을 뺀 이름과 같은 값이라, 빈칸을 빼고 찾는다
    let squeezed: String = s.chars().filter(|&c| c != ' ').collect();
    let named = X11_COLORS.lines().find_map(|l| {
        let mut f = l.split_whitespace();
        let (r, g, b, n) = (f.next()?, f.next()?, f.next()?, f.next()?);
        if !n.eq_ignore_ascii_case(&squeezed) {
            return None;
        }
        Some((r.parse().ok()?, g.parse().ok()?, b.parse().ok()?))
    });
    if named.is_some() {
        return named;
    }
    if matches!(s.len(), 3 | 6) {
        return thirds(s);
    }
    let (rest, scale): (&str, fn(&str) -> Option<u8>) = if let Some(r) = s.strip_prefix("rgbi:") {
        (r, scale_fraction)
    } else {
        (s.strip_prefix("rgb:")?, scale_hex)
    };
    let mut it = rest.splitn(3, '/');
    let (r, g, b) = (it.next()?, it.next()?, it.next()?);
    Some((scale(r)?, scale(g)?, scale(b)?))
}

/// 지금 보이는 색: 프로그램이 바꾼 값이 있으면 그것, 없으면 테마. 칸이 따로 들고 있지 않은 색은 없다
fn current<L: EventListener>(term: &Term<L>, theme: &Theme, key: Key) -> Option<(u8, u8, u8)> {
    let idx = key.engine_index()?;
    if let Some(c) = term.colors()[idx] {
        return Some((c.r, c.g, c.b));
    }
    let c = match key {
        Key::Palette(n) => theme.indexed(n),
        Key::Special(Special::Foreground) => theme.foreground,
        Key::Special(Special::Background) => theme.background,
        _ => theme.cursor,
    };
    Some((c.r, c.g, c.b))
}

/// 프로그램이 바꾼 팔레트(0~255번)를 모두 테마 값으로 되돌린다. 초기화(RIS)와 소프트 리셋(DECSTR)에서 쓴다.
/// Ghostty(`Terminal.zig`의 `fullReset`·`softReset`)처럼 팔레트만 되돌리고 전경·바탕·커서는 그대로 둔다
pub fn reset_palette<L: EventListener>(term: &mut Term<L>) {
    for i in 0..256 {
        Handler::reset_color(term, i);
    }
}

/// 요청을 차례로 적용하고, 질의가 있으면 답 바이트를 돌려준다.
///
/// 설정·되돌리기는 OSC 4·10·11·12가 쓰는 엔진의 바꾼 색 칸에 그대로 넣는다. 질의는 앞 항목을 적용한 뒤의 값으로 답한다.
/// 답은 하나로 모아 `ESC ] 21 ; 키=rgb:rr/gg/bb … 끝맺음`이고 끝맺음은 요청을 따른다(`bel`).
/// Ghostty처럼 칸이 색을 들고 있지 않은 키(selection_*, cursor_text, visual_bell 등)는 설정·되돌리기를 무시하고,
/// 질의에는 그 키를 빼고 답한다(그 키만 물었으면 `ESC ] 21` 과 끝맺음만 간다)
pub fn apply<L: EventListener>(term: &mut Term<L>, theme: &Theme, requests: &[Request], bel: bool) -> Option<Vec<u8>> {
    let mut reply = String::new();
    for r in requests {
        match *r {
            Request::Query(key) => {
                if reply.is_empty() {
                    reply.push_str("\x1b]21");
                }
                if let Some((r, g, b)) = current(term, theme, key) {
                    reply.push(';');
                    key.write_name(&mut reply);
                    reply.push_str(&format!("=rgb:{r:02x}/{g:02x}/{b:02x}"));
                }
            }
            Request::Set(key, (r, g, b)) => {
                if let Some(i) = key.engine_index() {
                    Handler::set_color(term, i, VtRgb { r, g, b });
                }
            }
            Request::Reset(key) => {
                if let Some(i) = key.engine_index() {
                    Handler::reset_color(term, i);
                }
            }
        }
    }
    if reply.is_empty() {
        return None;
    }
    reply.push_str(if bel { "\x07" } else { "\x1b\\" });
    Some(reply.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ghostty `osc/parsers/kitty_color.zig`의 시험 값
    const GHOSTTY_INPUT: &str = "foreground=?;background=rgb:f0/f8/ff;cursor=aliceblue;cursor_text;visual_bell=;\
        selection_foreground=#xxxyyzz;selection_background=?;selection_background=#aabbcc;2=?;3=rgbi:1.0/1.0/1.0";

    fn sp(s: Special) -> Key {
        Key::Special(s)
    }

    #[test]
    fn parses_the_ghostty_test_input() {
        let got = parse(GHOSTTY_INPUT).expect("해석");
        // 색이 틀린 selection_foreground=#xxxyyzz는 그 항목만 버린다(10개 중 9개)
        assert_eq!(
            got,
            vec![
                Request::Query(sp(Special::Foreground)),
                Request::Set(sp(Special::Background), (0xf0, 0xf8, 0xff)),
                Request::Set(sp(Special::Cursor), (0xf0, 0xf8, 0xff)),
                Request::Reset(sp(Special::CursorText)),
                Request::Reset(sp(Special::VisualBell)),
                Request::Query(sp(Special::SelectionBackground)),
                Request::Set(sp(Special::SelectionBackground), (0xaa, 0xbb, 0xcc)),
                Request::Query(Key::Palette(2)),
                Request::Set(Key::Palette(3), (0xff, 0xff, 0xff)),
            ]
        );
    }

    #[test]
    fn empty_request_is_an_empty_list() {
        assert_eq!(parse(""), Some(vec![]));
    }

    #[test]
    fn unknown_keys_and_bad_palette_numbers_are_dropped() {
        // 빈 키, 모르는 키, 256, 음수, 숫자 구분 밑줄(Ghostty 시험: 1_0), 부호는 버린다
        let got = parse("=?;nope=?;256=?;-1=?;1_0=?;+5=?;07=?;background=?").unwrap();
        assert_eq!(
            got,
            vec![Request::Query(Key::Palette(7)), Request::Query(sp(Special::Background))]
        );
    }

    #[test]
    fn value_is_trimmed_of_spaces_but_the_key_is_not() {
        assert_eq!(
            parse("foreground= ? ;background= #102030 ").unwrap(),
            vec![
                Request::Query(sp(Special::Foreground)),
                Request::Set(sp(Special::Background), (0x10, 0x20, 0x30))
            ]
        );
        assert_eq!(parse(" foreground=?").unwrap(), vec![]);
    }

    #[test]
    fn more_than_the_item_limit_drops_the_whole_request() {
        let ok = vec!["0=?"; MAX_ITEMS].join(";");
        assert_eq!(parse(&ok).unwrap().len(), MAX_ITEMS);
        let over = vec!["0=?"; MAX_ITEMS + 1].join(";");
        assert_eq!(parse(&over), None);
    }

    /// Ghostty src/terminal/color.zig `test "RGB.parse"`(984~1033줄)와 `rejects digit separators`의 값을 그대로 옮겼다
    #[test]
    fn color_parse_matches_ghostty_tests() {
        let ok: &[(&str, (u8, u8, u8))] = &[
            ("rgbi:1.0/0/0", (255, 0, 0)),
            ("rgb:7f/a0a0/0", (127, 160, 0)),
            ("rgb:f/ff/fff", (255, 255, 255)),
            ("#ffffff", (255, 255, 255)),
            ("#fff", (255, 255, 255)),
            ("#fffffffff", (255, 255, 255)),
            ("#ffffffffffff", (255, 255, 255)),
            ("#ff0010", (255, 0, 16)),
            ("0A0B0C", (10, 11, 12)),
            ("FFFFFF", (255, 255, 255)),
            ("FFF", (255, 255, 255)),
            ("#345", (51, 68, 85)),
            (" #AABBCC   ", (170, 187, 204)),
            ("black", (0, 0, 0)),
            ("red", (255, 0, 0)),
            ("green", (0, 255, 0)),
            ("blue", (0, 0, 255)),
            ("white", (255, 255, 255)),
            ("LawnGreen", (124, 252, 0)),
            ("medium spring green", (0, 250, 154)),
            (" Forest Green ", (34, 139, 34)),
            ("\tForestGreen\t", (34, 139, 34)),
        ];
        for (input, want) in ok {
            assert_eq!(parse_color(input), Some(*want), "{input:?}");
        }
        for bad in [
            "",
            "  ",
            "rgb;",
            "rgb:",
            ":a/a/a",
            "a/a/a",
            "rgb:a/a/a/",
            "rgb:00000///",
            "rgb:000/",
            "rgbi:a/a/a",
            "rgb:0.5/0.0/1.0",
            "rgb:not/hex/zz",
            "#",
            "#ff",
            "#ffff",
            "#fffff",
            "#gggggg",
            "#12345",
            "12345",
            "nosuchcolor",
            "rgb:f_f/0/0",
            "#f_f000000",
        ] {
            assert_eq!(parse_color(bad), None, "{bad:?}");
        }
        // 16진수 칸은 버림으로 줄인다(Ghostty `fromHex`: 값*255/최댓값), 소수는 버림(`fromIntensity`)
        assert_eq!(parse_color("rgb:8000/0/0"), Some((127, 0, 0)));
        assert_eq!(parse_color("rgbi:0.5/+0.5/-0"), Some((127, 127, 0)));
        assert_eq!(parse_color("rgbi:.5/1./0"), Some((127, 255, 0)));
        for bad in [
            "rgbi:inf/0/0",
            "rgbi:1e0/0/0",
            "rgbi:-0.5/0/0",
            "rgbi:1.5/0/0",
            "rgbi:./0/0",
            "rgbi:NaN/0/0",
        ] {
            assert_eq!(parse_color(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn color_formats() {
        assert_eq!(parse_color("#aabbcc"), Some((0xaa, 0xbb, 0xcc)));
        assert_eq!(parse_color("#abc"), Some((0xaa, 0xbb, 0xcc)));
        assert_eq!(parse_color("rgb:f0/f8/ff"), Some((0xf0, 0xf8, 0xff)));
        assert_eq!(parse_color("rgb:f/0/8"), Some((0xff, 0x00, 0x88)));
        assert_eq!(parse_color("rgb:ffff/8080/0000"), Some((0xff, 0x80, 0x00)));
        assert_eq!(parse_color("rgbi:1.0/1.0/1.0"), Some((255, 255, 255)));
        assert_eq!(parse_color("rgbi:0/0.5/1"), Some((0, 127, 255)));
        assert_eq!(parse_color("aliceblue"), Some((0xf0, 0xf8, 0xff)));
        assert_eq!(parse_color("AliceBlue"), Some((0xf0, 0xf8, 0xff)));
        for bad in [
            "",
            "#xxxyyzz",
            "#12",
            "#1234567",
            "rgb:1/2",
            "rgb:12345/0/0",
            "rgb:g/0/0",
            "rgbi:2/0/0",
            "nocolor",
        ] {
            assert_eq!(parse_color(bad), None, "{bad}");
        }
    }
}
