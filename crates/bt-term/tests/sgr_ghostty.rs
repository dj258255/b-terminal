//! Ghostty v1.4.0 `src/terminal/sgr.zig`의 SGR 해석 시험을 같은 매개변수로 옮긴 것.
//! Ghostty 시험은 매개변수 배열과 구분자(`;`·`:`) 표지를 파서에 직접 넣는다. 여기서는 같은 매개변수를 `CSI … m` 바이트로 만들어
//! vendor/vte에 넣고 나온 `Attr`를 견준다. Ghostty의 `unknown`(알 수 없는 속성, 아무것도 바꾸지 않음)은 `Attr`가 나오지 않는 것으로 본다.
//! 시험 이름 위 주석에 Ghostty 시험 이름과 sgr.zig 줄 번호를 적었다.
//!
//! 옮기지 않은 시험: `sgr: Attribute C compat`(548줄, C ABI 타입 확인이라 해당 없음).

use alacritty_terminal::vte::ansi::{Attr, Color, Handler, NamedColor, Processor, Rgb};

#[derive(Default)]
struct Collect(Vec<Attr>);

impl Handler for Collect {
    fn terminal_attribute(&mut self, attr: Attr) {
        self.0.push(attr);
    }
}

fn parse(bytes: &[u8]) -> Vec<Attr> {
    let mut parser: Processor = Processor::new();
    let mut handler = Collect::default();
    parser.advance(&mut handler, bytes);
    handler.0
}

/// Ghostty `testParse`·`Parser{ .params, .params_sep }`처럼: 매개변수와 "이 매개변수 뒤가 `:`인가" 표지로 `CSI … m`을 만든다
fn sgr(params: &[u16], colon_after: &[usize]) -> Vec<Attr> {
    let mut s = String::from("\x1b[");
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            s.push(if colon_after.contains(&(i - 1)) { ':' } else { ';' });
        }
        s.push_str(&p.to_string());
    }
    // 끝 매개변수 뒤 구분자 표지(퍼징 시험): 빈 하위 매개변수
    if let Some(last) = params.len().checked_sub(1)
        && colon_after.contains(&last)
    {
        s.push(':');
    }
    s.push('m');
    parse(s.as_bytes())
}

/// Ghostty `testParseColon`: 끝을 뺀 모든 매개변수 뒤가 `:`
fn colon(params: &[u16]) -> Vec<Attr> {
    let all: Vec<usize> = (0..params.len().saturating_sub(1)).collect();
    sgr(params, &all)
}

fn semi(params: &[u16]) -> Vec<Attr> {
    sgr(params, &[])
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::Spec(Rgb { r, g, b })
}

// sgr: Parser (552)
#[test]
fn parser_basics() {
    assert_eq!(parse(b"\x1b[m"), vec![Attr::Reset]);
    assert_eq!(semi(&[0]), vec![Attr::Reset]);
    assert_eq!(semi(&[38, 2, 40, 44, 52]), vec![Attr::Foreground(rgb(40, 44, 52))]);
    assert_eq!(semi(&[38, 2, 44, 52]), vec![], "색 값이 모자라면 unknown");
    assert_eq!(semi(&[48, 2, 40, 44, 52]), vec![Attr::Background(rgb(40, 44, 52))]);
    assert_eq!(semi(&[48, 2, 44, 52]), vec![], "색 값이 모자라면 unknown");
}

// sgr: Parser multiple (577)
#[test]
fn parser_multiple() {
    assert_eq!(
        semi(&[0, 38, 2, 40, 44, 52]),
        vec![Attr::Reset, Attr::Foreground(rgb(40, 44, 52))]
    );
}

// sgr: unsupported with colon (585)
#[test]
fn unsupported_with_colon() {
    assert_eq!(sgr(&[0, 4, 1], &[0]), vec![Attr::Bold], "0:4는 unknown, 1은 굵게");
}

// sgr: unsupported with multiple colon (599)
#[test]
fn unsupported_with_multiple_colon() {
    assert_eq!(
        sgr(&[0, 4, 2, 1], &[0, 1]),
        vec![Attr::Bold],
        "0:4:2는 unknown, 1은 굵게"
    );
}

// sgr: bold (614), italic (626), underline (638)
#[test]
fn bold_italic_underline() {
    assert_eq!(semi(&[1]), vec![Attr::Bold]);
    assert_eq!(
        semi(&[22]),
        vec![Attr::CancelBoldDim],
        "Ghostty reset_bold는 굵게와 흐리게를 함께 끈다"
    );
    assert_eq!(semi(&[3]), vec![Attr::Italic]);
    assert_eq!(semi(&[23]), vec![Attr::CancelItalic]);
    assert_eq!(semi(&[4]), vec![Attr::Underline]);
    assert_eq!(semi(&[24]), vec![Attr::CancelUnderline]);
}

// sgr: underline styles (651)
#[test]
fn underline_styles() {
    assert_eq!(colon(&[4, 2]), vec![Attr::DoubleUnderline]);
    assert_eq!(colon(&[4, 0]), vec![Attr::CancelUnderline]);
    assert_eq!(colon(&[4, 1]), vec![Attr::Underline]);
    assert_eq!(colon(&[4, 3]), vec![Attr::Undercurl]);
    assert_eq!(colon(&[4, 4]), vec![Attr::DottedUnderline]);
    assert_eq!(colon(&[4, 5]), vec![Attr::DashedUnderline]);
}

// sgr: underline style with more (689)
#[test]
fn underline_style_with_more() {
    assert_eq!(sgr(&[4, 2, 1], &[0]), vec![Attr::DoubleUnderline, Attr::Bold]);
}

// sgr: underline style with too many colons (704)
#[test]
fn underline_style_with_too_many_colons() {
    assert_eq!(
        sgr(&[4, 2, 3, 1], &[0, 1]),
        vec![Attr::Bold],
        "4:2:3은 unknown, 1은 굵게"
    );
}

// sgr: blink (720), inverse (737), strikethrough (749), invisible (835)
#[test]
fn blink_inverse_strike_invisible() {
    assert_eq!(semi(&[5]), vec![Attr::BlinkSlow]);
    // Ghostty는 6도 깜빡임 하나로 본다. vte는 BlinkFast로 내고 엔진이 같은 깜빡임으로 그린다(#277, sgr_parse.rs)
    assert_eq!(semi(&[6]), vec![Attr::BlinkFast]);
    assert_eq!(semi(&[25]), vec![Attr::CancelBlink]);
    assert_eq!(semi(&[7]), vec![Attr::Reverse]);
    assert_eq!(semi(&[27]), vec![Attr::CancelReverse]);
    assert_eq!(semi(&[9]), vec![Attr::Strike]);
    assert_eq!(semi(&[29]), vec![Attr::CancelStrike]);
    assert_eq!(semi(&[8, 28]), vec![Attr::Hidden, Attr::CancelHidden]);
}

// sgr: 8 color (761)
#[test]
fn eight_colors() {
    assert_eq!(
        semi(&[31, 43, 90, 103]),
        vec![
            Attr::Foreground(Color::Named(NamedColor::Red)),
            Attr::Background(Color::Named(NamedColor::Yellow)),
            Attr::Foreground(Color::Named(NamedColor::BrightBlack)),
            Attr::Background(Color::Named(NamedColor::BrightYellow)),
        ]
    );
}

// sgr: 256 color (789), 256 color underline (796)
#[test]
fn indexed_colors() {
    assert_eq!(
        semi(&[38, 5, 161, 48, 5, 236]),
        vec![
            Attr::Foreground(Color::Indexed(161)),
            Attr::Background(Color::Indexed(236))
        ]
    );
    assert_eq!(semi(&[58, 5, 9]), vec![Attr::UnderlineColor(Some(Color::Indexed(9)))]);
}

// sgr: 24-bit bg color (802), underline color (812), reset underline color (830)
#[test]
fn direct_colors_with_colon() {
    assert_eq!(colon(&[48, 2, 1, 2, 3]), vec![Attr::Background(rgb(1, 2, 3))]);
    assert_eq!(colon(&[58, 2, 1, 2, 3]), vec![Attr::UnderlineColor(Some(rgb(1, 2, 3)))]);
    assert_eq!(
        colon(&[58, 2, 0, 1, 2, 3]),
        vec![Attr::UnderlineColor(Some(rgb(1, 2, 3)))]
    );
    assert_eq!(semi(&[59]), vec![Attr::UnderlineColor(None)]);
}

// sgr: underline, bg, and fg (841)
#[test]
fn underline_bg_and_fg() {
    assert_eq!(
        semi(&[4, 38, 2, 255, 247, 219, 48, 2, 242, 93, 147, 4]),
        vec![
            Attr::Underline,
            Attr::Foreground(rgb(255, 247, 219)),
            Attr::Background(rgb(242, 93, 147)),
            Attr::Underline,
        ]
    );
}

// sgr: direct color fg missing color (871), direct color bg missing color (877): 패닉하지 않는다
#[test]
fn indexed_color_missing_value_does_not_panic() {
    let _ = semi(&[38, 5]);
    let _ = semi(&[48, 5]);
}

// sgr: direct fg/bg/underline ignore optional color space (883)
#[test]
fn colon_skips_color_space_semicolon_does_not() {
    assert_eq!(colon(&[38, 2, 0, 1, 2, 3]), vec![Attr::Foreground(rgb(1, 2, 3))]);
    assert_eq!(colon(&[48, 2, 0, 1, 2, 3]), vec![Attr::Background(rgb(1, 2, 3))]);
    assert_eq!(
        colon(&[58, 2, 0, 1, 2, 3]),
        vec![Attr::UnderlineColor(Some(rgb(1, 2, 3)))]
    );
    // `;`는 색 공간 자리를 두지 않는다: 0,1,2가 색이고 3은 그 뒤의 기울임
    assert_eq!(
        semi(&[38, 2, 0, 1, 2, 3]),
        vec![Attr::Foreground(rgb(0, 1, 2)), Attr::Italic]
    );
    assert_eq!(
        semi(&[48, 2, 0, 1, 2, 3]),
        vec![Attr::Background(rgb(0, 1, 2)), Attr::Italic]
    );
    assert_eq!(
        semi(&[58, 2, 0, 1, 2, 3]),
        vec![Attr::UnderlineColor(Some(rgb(0, 1, 2))), Attr::Italic]
    );
}

// sgr: direct fg colon with too many colons (939)
#[test]
fn direct_fg_colon_with_too_many_colons() {
    assert_eq!(sgr(&[38, 2, 0, 1, 2, 3, 4, 1], &[0, 1, 2, 3, 4, 5]), vec![Attr::Bold]);
}

// sgr: direct fg colon with colorspace and extra param (954)
#[test]
fn direct_fg_colon_with_colorspace_and_extra_param() {
    assert_eq!(
        sgr(&[38, 2, 0, 1, 2, 3, 1], &[0, 1, 2, 3, 4]),
        vec![Attr::Foreground(rgb(1, 2, 3)), Attr::Bold]
    );
}

// sgr: direct fg colon no colorspace and extra param (976)
#[test]
fn direct_fg_colon_no_colorspace_and_extra_param() {
    assert_eq!(
        sgr(&[38, 2, 1, 2, 3, 1], &[0, 1, 2, 3]),
        vec![Attr::Foreground(rgb(1, 2, 3)), Attr::Bold]
    );
}

// sgr: kakoune input (999). Ghostty 시험은 넷째 속성까지만 본다(끝의 null 확인은 주석 처리돼 있다)
#[test]
fn kakoune_input() {
    let got = sgr(
        &[0, 4, 3, 38, 2, 175, 175, 215, 58, 2, 0, 190, 80, 70],
        &[1, 8, 9, 10, 11, 12],
    );
    assert_eq!(
        got[..4],
        [
            Attr::Reset,
            Attr::Undercurl,
            Attr::Foreground(rgb(175, 175, 215)),
            Attr::UnderlineColor(Some(rgb(190, 80, 70))),
        ]
    );
}

// sgr: kakoune input issue underline, fg, and bg (1043)
#[test]
fn kakoune_underline_fg_bg() {
    assert_eq!(
        sgr(
            &[4, 3, 38, 2, 51, 51, 51, 48, 2, 170, 170, 170, 58, 2, 255, 97, 136],
            &[0]
        ),
        vec![
            Attr::Undercurl,
            Attr::Foreground(rgb(51, 51, 51)),
            Attr::Background(rgb(170, 170, 170)),
            Attr::UnderlineColor(Some(rgb(255, 97, 136))),
        ]
    );
}

// sgr: underline colon with trailing separator and short slice (1094): `ESC [ 5 8 : 4 : m`
#[test]
fn underline_colon_with_trailing_separator() {
    assert_eq!(
        parse(b"\x1b[58:4:m"),
        vec![],
        "58:4는 밑줄 색이 아니고, 끝 콜론의 4도 unknown"
    );
}
