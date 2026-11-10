//! DCS 질의: DECRQSS(`DCS $ q 요청 ST`)와 XTGETTCAP(`DCS + q 16진수이름;… ST`)(#265).
//!
//! Ghostty `src/terminal/dcs.zig`(해석과 DECRQSS 답)와 `stream_handler.zig`(XTGETTCAP 답), `src/terminfo/Source.zig`(답 형식)를 옮겼다.
//! 터미널에 찍힌 글이 PTY 입력을 만드는 길이므로 **요청에서 온 바이트는 답에 넣지 않는다**: DECRQSS 답은 터미널이 가진 상태에서
//! 만든 숫자뿐이고, XTGETTCAP 답의 이름은 우리가 아는 이름의 16진 표기뿐이다(ADR-023).

use alacritty_terminal::event::EventListener;
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::Flags as CellFlags;
use alacritty_terminal::vte::ansi::{Color, CursorShape};

/// DECRQSS가 묻는 설정. 모르는 것은 `None`이고 `DCS 0 $ r ST`로 답한다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decrqss {
    None,
    Sgr,
    Decscusr,
    Decstbm,
    Decslrm,
}

/// 스캐너가 뽑은 DCS 질의
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Query {
    Decrqss(Decrqss),
    /// XTGETTCAP 본문(`;`로 이은 16진수 이름들). 길이는 `MAX_XTGETTCAP` 이하다
    XtGetTcap(Vec<u8>),
}

/// XTGETTCAP 본문의 상한(바이트). Ghostty는 1MiB지만 이름은 짧고 한 요청이 만드는 답을 작게 묶는다
pub const MAX_XTGETTCAP: usize = 4096;
/// 한 요청에서 답하는 이름 수의 상한(같은 이름을 되풀이해 묻는 증폭을 막는다)
const MAX_NAMES: usize = 64;

/// DECRQSS 본문(요청 글)을 읽는다. 두 바이트를 넘으면 Ghostty처럼 잘못된 명령으로 보고 `None`(답 없음)을 돌려준다
pub fn parse_decrqss(body: &[u8]) -> Option<Decrqss> {
    Some(match body {
        [] => Decrqss::None,
        b"m" => Decrqss::Sgr,
        b"r" => Decrqss::Decstbm,
        b"s" => Decrqss::Decslrm,
        b" q" => Decrqss::Decscusr,
        [_] | [_, _] => Decrqss::None,
        _ => return Option::None,
    })
}

/// DECRQSS 답(`DCS 1 $ r 값 ST`, 모르면 `DCS 0 $ r ST`). 값은 모두 터미널의 상태에서 만든 숫자이고 요청 글은 들어가지 않는다
pub fn encode_decrqss<L: EventListener>(q: Decrqss, term: &Term<L>) -> Vec<u8> {
    let value: Option<String> = match q {
        Decrqss::None | Decrqss::Decslrm => None,
        Decrqss::Sgr => Some(format!("{}m", sgr_params(term))),
        Decrqss::Decscusr => {
            let style = term.cursor_style();
            let n = match (style.shape, style.blinking) {
                (CursorShape::Underline, true) => 3,
                (CursorShape::Underline, false) => 4,
                (CursorShape::Beam, true) => 5,
                (CursorShape::Beam, false) => 6,
                (_, true) => 1,
                (_, false) => 2,
            };
            Some(format!("{n} q"))
        }
        Decrqss::Decstbm => {
            // 범위는 위쪽 포함, 아래쪽 제외다. 답은 1부터 세고 둘 다 포함이다
            let r = term.scroll_region();
            Some(format!("{};{}r", r.start.0.max(0) + 1, r.end.0.max(1)))
        }
    };
    match value {
        Some(v) => format!("\x1bP1$r{v}\x1b\\"),
        None => "\x1bP0$r\x1b\\".to_string(),
    }
    .into_bytes()
}

/// 지금 커서의 글자 속성(`grid.cursor.template`)을 SGR 매개변수로 쓴다. 맨 앞은 늘 `0`이다(Ghostty `printAttributes`와 같다).
/// alacritty 격자에는 깜박임(5)이 없어 빠진다
fn sgr_params<L: EventListener>(term: &Term<L>) -> String {
    let t = &term.grid().cursor.template;
    let f = t.flags;
    let mut out = String::from("0");
    let mut add = |s: &str| {
        out.push(';');
        out.push_str(s);
    };
    if f.contains(CellFlags::BOLD) {
        add("1");
    }
    if f.contains(CellFlags::DIM) {
        add("2");
    }
    if f.contains(CellFlags::ITALIC) {
        add("3");
    }
    if f.contains(CellFlags::UNDERCURL) {
        add("4:3");
    } else if f.contains(CellFlags::DOUBLE_UNDERLINE) {
        add("4:2");
    } else if f.contains(CellFlags::DOTTED_UNDERLINE) {
        add("4:4");
    } else if f.contains(CellFlags::DASHED_UNDERLINE) {
        add("4:5");
    } else if f.contains(CellFlags::UNDERLINE) {
        add("4");
    }
    // 윗줄(53)은 밑줄 바로 뒤, 깜박임(5) 앞이다(Ghostty terminal_dcs.zig, #277)
    if f.contains(CellFlags::OVERLINE) {
        add("53");
    }
    // 깜박임(5)은 53 뒤, 반전(7) 앞(#287). SGR 6도 Ghostty처럼 5로 답한다
    if f.contains(CellFlags::BLINK) {
        add("5");
    }
    if f.contains(CellFlags::INVERSE) {
        add("7");
    }
    if f.contains(CellFlags::HIDDEN) {
        add("8");
    }
    if f.contains(CellFlags::STRIKEOUT) {
        add("9");
    }
    if let Some(c) = color_params(t.fg, 30, 90, "38") {
        add(&c);
    }
    if let Some(c) = color_params(t.bg, 40, 100, "48") {
        add(&c);
    }
    if let Some(c) = t.underline_color().and_then(|c| color_params(c, 0, 0, "58")) {
        add(&c);
    }
    out
}

/// 색 하나를 SGR 매개변수로 쓴다. 기본 색(전경·배경)은 쓰지 않는다. `base`·`bright`는 8색·밝은 8색의 시작 번호다(밑줄 색은 0이고 `58:5:번호`로만 쓴다)
fn color_params(c: Color, base: u8, bright: u8, ext: &str) -> Option<String> {
    match c {
        Color::Named(n) => {
            let i = n as usize;
            match i {
                0..=7 if base > 0 => Some((base as usize + i).to_string()),
                8..=15 if bright > 0 => Some((bright as usize + i - 8).to_string()),
                0..=15 => Some(format!("{ext}:5:{i}")),
                _ => None,
            }
        }
        Color::Indexed(i) => Some(format!("{ext}:5:{i}")),
        Color::Spec(rgb) => Some(format!("{ext}:2::{}:{}:{}", rgb.r, rgb.g, rgb.b)),
    }
}

/// XTGETTCAP이 답하는 값의 종류
enum Val {
    /// 값 없는 불리언(이름만 답한다)
    Bool,
    /// 터미널 정보(terminfo) 글. `%`가 없으면 `\E`와 `^X`를 제어 문자로 바꿔 답하고, `%`가 있으면 글 그대로 답한다(Ghostty `Source.zig`)
    Str(&'static str),
    /// TERM 값
    Tn,
}

/// 답하는 능력 목록. b-terminal이 정말 하는 것만 둔다(ADR-023). Ghostty 목록과의 차이는 ADR에 적었다
const CAPS: &[(&str, Val)] = &[
    ("TN", Val::Tn),
    ("Co", Val::Str("256")),
    ("RGB", Val::Str("8")),
    ("colors", Val::Str("256")),
    ("Tc", Val::Bool),
    ("am", Val::Bool),
    ("xenl", Val::Bool),
    ("bce", Val::Bool),
    ("AX", Val::Bool),
    ("Smulx", Val::Str("\\E[4:%p1%dm")),
    ("Ss", Val::Str("\\E[%p1%d q")),
    ("Se", Val::Str("\\E[0 q")),
    ("Ms", Val::Str("\\E]52;%p1%s;%p2%s\\007")),
    ("Sync", Val::Str("\\E[?2026%?%p1%{1}%-%tl%eh%;")),
    ("BD", Val::Str("\\E[?2004l")),
    ("BE", Val::Str("\\E[?2004h")),
    ("PS", Val::Str("\\E[200~")),
    ("PE", Val::Str("\\E[201~")),
    ("XM", Val::Str("\\E[?1006;1000%?%p1%{1}%=%th%el%;")),
    ("xm", Val::Str("\\E[<%i%p3%d;%p1%d;%p2%d;%?%p4%tM%em%;")),
    ("fe", Val::Str("\\E[?1004h")),
    ("fd", Val::Str("\\E[?1004l")),
    ("kxIN", Val::Str("\\E[I")),
    ("kxOUT", Val::Str("\\E[O")),
    ("E3", Val::Str("\\E[3J")),
    // DA2(CSI > c)와 XTVERSION(CSI > q)에 답한다(#264). 값은 Ghostty terminfo/ghostty.zig 140~145줄 그대로
    ("RV", Val::Str("\\E[>c")),
    ("rv", Val::Str("\\E\\\\[[0-9]+;[0-9]+;[0-9]+c")),
    ("XR", Val::Str("\\E[>0q")),
    ("xr", Val::Str("\\EP>\\\\|[ -~]+a\\E\\\\")),
    // 윗줄(SGR 53·55, #277). 값은 Ghostty terminfo/ghostty.zig 110~111줄 그대로
    ("Smol", Val::Str("\\E[53m")),
    ("Rmol", Val::Str("\\E[55m")),
    // 밑줄 모양과 밑줄 색을 그린다(#276). 값은 Ghostty terminfo/ghostty.zig 74줄·114줄 그대로
    ("Su", Val::Bool),
    (
        "Setulc",
        Val::Str("\\E[58:2::%p1%{65536}%/%d:%p1%{256}%/%{255}%&%d:%p1%{255}%&%d%;m"),
    ),
];

fn hex_upper(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02X}");
    }
    s
}

/// 터미널 정보 글의 `\E`(ESC)와 `^X`(제어 문자)를 바이트로 바꾼다. `%`가 든 글은 바꾸지 않는다
fn expand(v: &str) -> Vec<u8> {
    if v.contains('%') {
        return v.as_bytes().to_vec();
    }
    let b = v.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match (b[i], b.get(i + 1)) {
            (b'\\', Some(b'E')) => {
                out.push(0x1b);
                i += 2;
            }
            (b'^', Some(b'?')) => {
                out.push(0x7f);
                i += 2;
            }
            (b'^', Some(&c)) if (b'@'..=b'_').contains(&c) => {
                out.push(c - 64);
                i += 2;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// XTGETTCAP 답(이름마다 `DCS 1 + r 이름16진=값16진 ST`, 값 없는 불리언은 `DCS 1 + r 이름16진 ST`).
/// 모르는 이름은 Ghostty처럼 답하지 않는다. 답의 이름은 목록의 이름을 16진수로 바꾼 것이지 요청 글이 아니다. `tn`은 TERM 값
pub fn xtgettcap_reply(body: &[u8], tn: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut answered = 0;
    for token in body.split(|&b| b == b';') {
        if answered >= MAX_NAMES {
            break;
        }
        let Some((name, val)) = CAPS
            .iter()
            .find(|(n, _)| hex_upper(n.as_bytes()).as_bytes().eq_ignore_ascii_case(token))
        else {
            continue;
        };
        answered += 1;
        out.extend_from_slice(b"\x1bP1+r");
        out.extend_from_slice(hex_upper(name.as_bytes()).as_bytes());
        match val {
            Val::Bool => {}
            Val::Str(v) => {
                out.push(b'=');
                out.extend_from_slice(hex_upper(&expand(v)).as_bytes());
            }
            Val::Tn => {
                out.push(b'=');
                out.extend_from_slice(hex_upper(tn.as_bytes()).as_bytes());
            }
        }
        out.extend_from_slice(b"\x1b\\");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::grid::Dimensions;
    use alacritty_terminal::term::Config;
    use alacritty_terminal::vte::ansi::Processor;

    #[derive(Clone, Copy)]
    struct Null;
    impl EventListener for Null {}

    struct Size(usize, usize);
    impl Dimensions for Size {
        fn total_lines(&self) -> usize {
            self.0
        }
        fn screen_lines(&self) -> usize {
            self.0
        }
        fn columns(&self) -> usize {
            self.1
        }
    }

    /// 80열 24행 터미널에 바이트를 먹인다(Ghostty 테스트와 같은 크기)
    fn term(bytes: &str) -> Term<Null> {
        let mut t = Term::new(Config::default(), &Size(24, 80), Null);
        let mut p: Processor = Processor::new();
        p.advance(&mut t, bytes.as_bytes());
        t
    }

    fn reply(q: Decrqss, setup: &str) -> String {
        String::from_utf8(encode_decrqss(q, &term(setup))).unwrap()
    }

    // ---- 요청 해석(Ghostty dcs.zig 테스트 "DECRQSS command", "DECRQSS invalid command") ----

    #[test]
    fn decrqss_body_is_parsed_like_ghostty() {
        assert_eq!(parse_decrqss(b"m"), Some(Decrqss::Sgr));
        assert_eq!(parse_decrqss(b"r"), Some(Decrqss::Decstbm));
        assert_eq!(parse_decrqss(b"s"), Some(Decrqss::Decslrm));
        assert_eq!(parse_decrqss(b" q"), Some(Decrqss::Decscusr));
        // 모르는 한두 글자는 "지원하지 않음"이라 DCS 0으로 답한다
        assert_eq!(parse_decrqss(b"z"), Some(Decrqss::None));
        assert_eq!(parse_decrqss(b""), Some(Decrqss::None));
        assert_eq!(parse_decrqss(b"q "), Some(Decrqss::None));
        // 세 글자부터는 잘못된 명령이라 답이 없다(Ghostty: `"` ` ` `q`는 unhook이 null)
        assert_eq!(parse_decrqss(b"\" q"), None);
    }

    // ---- DECRQSS 답(Ghostty "DECRQSS response encoding") ----

    #[test]
    fn decrqss_answers_zero_for_unknown() {
        assert_eq!(reply(Decrqss::None, ""), "\x1bP0$r\x1b\\");
    }

    #[test]
    fn decrqss_sgr_reads_the_cursor_template() {
        assert_eq!(reply(Decrqss::Sgr, ""), "\x1bP1$r0m\x1b\\");
        // 굵게 + 물결 밑줄(Ghostty: "0;1;4:3m")
        assert_eq!(reply(Decrqss::Sgr, "\x1b[1m\x1b[4:3m"), "\x1bP1$r0;1;4:3m\x1b\\");
        // 속성을 지우면 다시 0m
        assert_eq!(reply(Decrqss::Sgr, "\x1b[1;3m\x1b[0m"), "\x1bP1$r0m\x1b\\");
        // 색: 기본 8색·밝은 8색, 256색, 트루컬러(배경은 48)
        assert_eq!(reply(Decrqss::Sgr, "\x1b[31;44m"), "\x1bP1$r0;31;44m\x1b\\");
        assert_eq!(reply(Decrqss::Sgr, "\x1b[91;104m"), "\x1bP1$r0;91;104m\x1b\\");
        assert_eq!(reply(Decrqss::Sgr, "\x1b[38;5;200m"), "\x1bP1$r0;38:5:200m\x1b\\");
        assert_eq!(
            reply(Decrqss::Sgr, "\x1b[38;2;1;2;3;48;2;4;5;6m"),
            "\x1bP1$r0;38:2::1:2:3;48:2::4:5:6m\x1b\\"
        );
    }

    #[test]
    fn decrqss_sgr_largest_fits_the_limit() {
        // Ghostty "DECRQSS largest response fits fixed buffer": 5는 53 뒤, 7 앞이다(#287).
        // 윗줄(53)은 Ghostty terminal_dcs.zig처럼 밑줄(4) 바로 뒤에 온다(#277)
        let r = reply(
            Decrqss::Sgr,
            "\x1b[1;2;3;4:5;53;5;7;8;9m\x1b[38;2;255;255;255m\x1b[48;2;255;255;255m\x1b[58;2;255;255;255m",
        );
        assert_eq!(
            r,
            "\x1bP1$r0;1;2;3;4:5;53;5;7;8;9;38:2::255:255:255;48:2::255:255:255;58:2::255:255:255m\x1b\\"
        );
        assert!(r.len() <= 256);
    }

    #[test]
    fn decrqss_sgr_reports_overline_after_underline() {
        // #277: 윗줄만, 밑줄과 함께(53은 4 뒤), 끄면 사라짐
        assert_eq!(reply(Decrqss::Sgr, "\x1b[53m"), "\x1bP1$r0;53m\x1b\\");
        assert_eq!(reply(Decrqss::Sgr, "\x1b[53m\x1b[4m"), "\x1bP1$r0;4;53m\x1b\\");
        assert_eq!(reply(Decrqss::Sgr, "\x1b[3;53;9m"), "\x1bP1$r0;3;53;9m\x1b\\");
        assert_eq!(reply(Decrqss::Sgr, "\x1b[53;55m"), "\x1bP1$r0m\x1b\\");
        assert_eq!(reply(Decrqss::Sgr, "\x1b[53;0m"), "\x1bP1$r0m\x1b\\");
    }

    #[test]
    fn decrqss_sgr_reports_blink_between_overline_and_inverse() {
        // #287: Ghostty terminal_dcs.zig 순서는 4, 53, 5, 7. 6(빠른 깜박임)도 5로 답한다(terminal_sgr.zig)
        assert_eq!(reply(Decrqss::Sgr, "\x1b[5m"), "\x1bP1$r0;5m\x1b\\");
        assert_eq!(reply(Decrqss::Sgr, "\x1b[1;5;7m"), "\x1bP1$r0;1;5;7m\x1b\\");
        assert_eq!(reply(Decrqss::Sgr, "\x1b[7;53;5;4m"), "\x1bP1$r0;4;53;5;7m\x1b\\");
        assert_eq!(reply(Decrqss::Sgr, "\x1b[6m"), "\x1bP1$r0;5m\x1b\\");
        // 25와 0은 끈다. 25 뒤에는 5가 없다
        assert_eq!(reply(Decrqss::Sgr, "\x1b[1;5;25m"), "\x1bP1$r0;1m\x1b\\");
        assert_eq!(reply(Decrqss::Sgr, "\x1b[5;0m"), "\x1bP1$r0m\x1b\\");
    }

    #[test]
    fn engine_keeps_blink_in_the_cursor_template() {
        use alacritty_terminal::term::cell::Flags;
        let blink = |s: &str| term(s).grid().cursor.template.flags.contains(Flags::BLINK);
        assert!(blink("\x1b[5m"));
        assert!(blink("\x1b[6m"));
        assert!(!blink("\x1b[5;25m"));
        assert!(!blink("\x1b[5;0m"));
        assert!(!blink("\x1b[5m\x1b[m"));
        // 쓴 칸에도 남는다
        let t = term("\x1b[5ma");
        assert!(
            t.grid()[alacritty_terminal::index::Point::new(
                alacritty_terminal::index::Line(0),
                alacritty_terminal::index::Column(0)
            )]
            .flags
            .contains(Flags::BLINK)
        );
    }

    #[test]
    fn decrqss_decscusr_reports_shape_and_blink() {
        // 처음은 깜박이지 않는 블록(DECSCUSR 2)
        assert_eq!(reply(Decrqss::Decscusr, ""), "\x1bP1$r2 q\x1b\\");
        assert_eq!(reply(Decrqss::Decscusr, "\x1b[1 q"), "\x1bP1$r1 q\x1b\\");
        assert_eq!(reply(Decrqss::Decscusr, "\x1b[3 q"), "\x1bP1$r3 q\x1b\\");
        // Ghostty: steady underline → 4
        assert_eq!(reply(Decrqss::Decscusr, "\x1b[4 q"), "\x1bP1$r4 q\x1b\\");
        assert_eq!(reply(Decrqss::Decscusr, "\x1b[5 q"), "\x1bP1$r5 q\x1b\\");
        assert_eq!(reply(Decrqss::Decscusr, "\x1b[6 q"), "\x1bP1$r6 q\x1b\\");
    }

    #[test]
    fn decrqss_decstbm_is_one_based_and_inclusive() {
        // 처음은 화면 전체
        assert_eq!(reply(Decrqss::Decstbm, ""), "\x1bP1$r1;24r\x1b\\");
        // Ghostty: 위 4, 아래 19(0부터)면 5;20r
        assert_eq!(reply(Decrqss::Decstbm, "\x1b[5;20r"), "\x1bP1$r5;20r\x1b\\");
    }

    #[test]
    fn decrqss_decslrm_is_unsupported() {
        // 왼쪽·오른쪽 여백은 alacritty에 없다. Ghostty도 모드가 꺼져 있으면 DCS 0이다
        assert_eq!(reply(Decrqss::Decslrm, ""), "\x1bP0$r\x1b\\");
        assert_eq!(reply(Decrqss::Decslrm, "\x1b[?69h\x1b[3;70s"), "\x1bP0$r\x1b\\");
    }

    // ---- XTGETTCAP ----

    fn hex(s: &str) -> String {
        s.bytes().map(|b| format!("{b:02X}")).collect()
    }

    fn tcap(body: &str) -> String {
        String::from_utf8(xtgettcap_reply(body.as_bytes(), "xterm-256color")).unwrap()
    }

    #[test]
    fn xtgettcap_answers_su_and_setulc_like_ghostty() {
        // Su는 값 없는 불리언: 이름만(53 75)
        assert_eq!(tcap("5375"), "\x1bP1+r5375\x1b\\");
        // Setulc은 문자열: 값은 Ghostty terminfo 그대로(`%`가 들어 있어 \E도 글자 그대로 16진수가 된다. Smulx와 같다)
        let v = "\\E[58:2::%p1%{65536}%/%d:%p1%{256}%/%{255}%&%d:%p1%{255}%&%d%;m";
        assert_eq!(
            tcap(&hex("Setulc")),
            format!("\x1bP1+r{}={}\x1b\\", hex("Setulc"), hex(v))
        );
        // 16진수 답 자체도 못 박는다(위 값을 따로 16진수로 바꾼 것)
        assert_eq!(
            tcap("536574756C63"),
            "\x1bP1+r536574756C63=5C455B35383A323A3A257031257B36353533367D252F25643A257031257B3235367D252F257B3235357D252625643A257031257B3235357D25262564253B6D\x1b\\"
        );
    }

    #[test]
    fn xtgettcap_known_names() {
        // Ghostty Source.zig 테스트와 같은 모양: 값 없는 불리언은 이름만
        assert_eq!(tcap("5463"), "\x1bP1+r5463\x1b\\");
        // 숫자: Co=256, RGB=8
        assert_eq!(tcap("436F"), format!("\x1bP1+r436F={}\x1b\\", hex("256")));
        assert_eq!(tcap("524742"), format!("\x1bP1+r524742={}\x1b\\", hex("8")));
        // TN은 TERM 값
        assert_eq!(tcap("544E"), format!("\x1bP1+r544E={}\x1b\\", hex("xterm-256color")));
        // %가 든 문자열은 터미널 정보 글 그대로(\E도 글자 그대로)
        assert_eq!(
            tcap("536D756C78"),
            format!("\x1bP1+r536D756C78={}\x1b\\", hex("\\E[4:%p1%dm"))
        );
        // %가 없는 문자열은 \E를 ESC 바이트로 바꾼다
        assert_eq!(tcap("5365"), format!("\x1bP1+r5365={}\x1b\\", hex("\x1b[0 q")));
    }

    #[test]
    fn xtgettcap_mixed_case_and_many_names() {
        // 소문자 16진수도 같은 이름이다(Ghostty "XTGETTCAP mixed case"). 답의 이름은 늘 대문자다
        assert_eq!(tcap("536d756C78"), tcap("536D756C78"));
        // 여러 이름은 `;`로 이어 주고, 이름마다 한 답이다
        let two = tcap("536D756C78;5463");
        assert_eq!(two, format!("{}{}", tcap("536D756C78"), tcap("5463")));
        // 같은 이름을 두 번 물으면 두 번 답한다(Ghostty "multiple keys")
        assert_eq!(tcap("5463;5463"), format!("{0}{0}", tcap("5463")));
    }

    #[test]
    fn xtgettcap_unknown_or_invalid_names_get_no_answer_and_are_never_echoed() {
        // Ghostty "XTGETTCAP command invalid data": 모르는 이름은 건너뛰고 아는 이름만 답한다
        assert_eq!(tcap("who;5463"), tcap("5463"));
        assert_eq!(tcap("who"), "");
        assert_eq!(tcap(""), "");
        assert_eq!(tcap(";;"), "");
        // 아는 이름이 아닌 16진수(`ESC`로 시작하는 글 등)는 되돌려 넣지 않는다
        assert_eq!(tcap("1B5D3532"), "");
        assert_eq!(tcap("DEADBEEF"), "");
        assert!(!tcap("DEADBEEF;5463").contains("DEADBEEF"));
    }

    #[test]
    fn xtgettcap_answers_xtversion_and_da2_caps_like_ghostty() {
        // XTVERSION(CSI > q)과 DA2(CSI > c)에 답하므로(#264) Ghostty terminfo의 RV·rv·XR·xr도 답한다.
        // 값은 Ghostty terminfo/ghostty.zig 140~145줄, 변환은 Source.zig(`%`가 없으면 `\\E`를 ESC로)
        assert_eq!(tcap(&hex("RV")), "\x1bP1+r5256=1B5B3E63\x1b\\");
        assert_eq!(
            tcap(&hex("rv")),
            "\x1bP1+r7276=1B5C5C5B5B302D395D2B3B5B302D395D2B3B5B302D395D2B63\x1b\\"
        );
        assert_eq!(tcap(&hex("XR")), "\x1bP1+r5852=1B5B3E3071\x1b\\");
        assert_eq!(tcap(&hex("xr")), "\x1bP1+r7872=1B503E5C5C7C5B202D7E5D2B611B5C5C\x1b\\");
    }

    #[test]
    fn xtgettcap_does_not_list_what_bterminal_cannot_do() {
        // DECSLRM 여백은 없어서 답하지 않는다
        for name in ["Enmg", "Dsmg", "Cmg"] {
            assert_eq!(tcap(&hex(name)), "", "{name}");
        }
    }

    #[test]
    fn xtgettcap_answers_overline_like_ghostty() {
        // #277: 값은 Ghostty terminfo/ghostty.zig 110~111줄(`\E[53m`, `\E[55m`). `%`가 없어 `\E`를 ESC로 바꾼다
        assert_eq!(
            tcap(&hex("Smol")),
            format!("\x1bP1+r{}={}\x1b\\", hex("Smol"), hex("\x1b[53m"))
        );
        assert_eq!(
            tcap(&hex("Rmol")),
            format!("\x1bP1+r{}={}\x1b\\", hex("Rmol"), hex("\x1b[55m"))
        );
    }

    #[test]
    fn xtgettcap_answers_at_most_max_names() {
        let body = vec!["5463"; 200].join(";");
        let n = tcap(&body).matches("\x1bP1+r").count();
        assert_eq!(n, MAX_NAMES);
    }

    #[test]
    fn every_listed_name_answers_with_well_formed_dcs() {
        for (name, _) in CAPS {
            let r = tcap(&hex(name));
            assert!(r.starts_with("\x1bP1+r") && r.ends_with("\x1b\\"), "{name}: {r:?}");
        }
    }
}
