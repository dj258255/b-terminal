//! Ghostty v1.4.0의 OSC 해석 시험을 같은 입력·기대로 옮긴 것(#765).
//!
//! 옮긴 시험(Ghostty 소스 `src/terminal/` 아래):
//! - `osc/parsers/color.zig`: "OSC color indexes reject digit separators"(333줄), "OSC 4:"(358줄),
//!   "OSC 4: multiple requests"(481줄), "OSC 104:"(547줄), "OSC 104: empty index"(595줄), "OSC 104: invalid index"(609줄),
//!   "OSC 104: reset all"(621줄), "OSC 10 … dynamic"(656줄), "OSC 10 … dynamic multiple"(~712줄), "OSC 110 … reset dynamic"(~738줄)
//! - `color.zig`: "RGB.parse"(984줄), "RGB.parse rejects digit separators"(1035줄), 거기서 쓰는 `x11_color.zig` 이름 표
//! - `osc/parsers/hyperlink.zig`: "OSC 8: hyperlink", "… with id set", "… with empty id", "… with incomplete key",
//!   "… with empty key", "… with empty key and id", "… with empty uri", "… hyperlink end"
//! - `osc/parsers/change_window_title.zig`: "OSC 0: longer than buffer", "OSC 0: one shorter than buffer length"
//! - `stream.zig` "stream: change window title with invalid utf-8"(4493줄, 처리는 2855줄)
//!
//! Ghostty 시험은 `parseColor`나 `Parser`에 바로 넣는다. 여기서는 같은 문자열을 OSC 바이트로 만들어 vendor/vte에 넣고
//! 핸들러가 받은 호출을 견준다. 우리에게 없는 자리(OSC 4·104의 256~259 특수 색, OSC 13~19)는 호출이 없는 것으로 본다.
//!
//! 따르지 않은 곳(시험에도 그대로 적음):
//! - `#` 꼴의 채널 맞춤. Ghostty는 `#fff`와 `#345`를 0~255로 늘려 맞추고(`ffffff`, `334455`), 우리는 xterm처럼 낮은 자리를
//!   0으로 채운다(`f0f0f0`, `304050`). 전부터 xterm과 같던 곳이라 ADR-056대로 둔다.
//! - OSC 4·104·10~12·110~112의 2048바이트 상한은 따르지 않는다(0·2·8만).

use alacritty_terminal::vte::ansi::{Handler, Hyperlink, Processor, Rgb};

const FOREGROUND: usize = 256;
const BACKGROUND: usize = 257;
const CURSOR: usize = 258;

#[derive(Debug, PartialEq)]
enum Ev {
    Title(Option<String>),
    Set(usize, Rgb),
    Query(usize),
    Reset(usize),
    Link(Option<(Option<String>, String)>),
}

#[derive(Default)]
struct Collect(Vec<Ev>);

impl Handler for Collect {
    fn set_title(&mut self, title: Option<String>) {
        self.0.push(Ev::Title(title));
    }
    fn set_color(&mut self, index: usize, color: Rgb) {
        self.0.push(Ev::Set(index, color));
    }
    fn dynamic_color_sequence(&mut self, _prefix: String, index: usize, _terminator: &str) {
        self.0.push(Ev::Query(index));
    }
    fn reset_color(&mut self, index: usize) {
        self.0.push(Ev::Reset(index));
    }
    fn set_hyperlink(&mut self, link: Option<Hyperlink>) {
        self.0.push(Ev::Link(link.map(|l| (l.id, l.uri))));
    }
}

fn run(bytes: &[u8]) -> Vec<Ev> {
    let mut parser: Processor = Processor::new();
    let mut handler = Collect::default();
    parser.advance(&mut handler, bytes);
    handler.0
}

/// `ESC ] body ST`
fn osc(body: &str) -> Vec<Ev> {
    run(format!("\x1b]{body}\x1b\\").as_bytes())
}

fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}

/// 색 문자열 하나가 OSC 11으로 들어갔을 때 정해지는 배경색(틀린 문자열이면 None)
fn color_of(spec: &str) -> Option<Rgb> {
    match osc(&format!("11;{spec}")).as_slice() {
        [Ev::Set(BACKGROUND, c)] => Some(*c),
        [] => None,
        other => panic!("{spec:?}: 뜻밖의 호출 {other:?}"),
    }
}

const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
const BLUE: Rgb = Rgb { r: 0, g: 0, b: 255 };

// color.zig 333: OSC color indexes reject digit separators
#[test]
fn color_indexes_reject_digit_separators() {
    // 잘못된 번호는 가져오기·바꾸기를 앞부분에서 멈춘다(요청 1개)
    assert_eq!(osc("4;1;red;0_3;blue;2;green"), vec![Ev::Set(1, RED)]);
    // 되돌리기는 잘못된 번호만 건너뛴다(요청 2개)
    assert_eq!(osc("104;1;0_3;2"), vec![Ev::Reset(1), Ev::Reset(2)]);
}

// color.zig 358: OSC 4:
#[test]
fn osc4_every_index() {
    for idx in 0..255usize {
        // printf '\e]4;0;red\\'
        assert_eq!(osc(&format!("4;{idx};red")), vec![Ev::Set(idx, RED)], "set {idx}");
        // printf '\e]4;0;?\\'
        assert_eq!(osc(&format!("4;{idx};?")), vec![Ev::Query(idx)], "query {idx}");
        // 끝의 잘못된 자료는 거기 앞까지의 결과: printf '\e]4;0;red;\e\\'
        assert_eq!(
            osc(&format!("4;{idx};red;")),
            vec![Ev::Set(idx, RED)],
            "trailing ; {idx}"
        );
        // 끝 공백(xterm은 안 되지만 kitty처럼 받는다): printf '\e]4;0;red \e\\'
        assert_eq!(
            osc(&format!("4;{idx};red ")),
            vec![Ev::Set(idx, RED)],
            "trailing space {idx}"
        );
    }
    // 256~259는 Ghostty의 특수 색(굵게·밑줄·깜박임·뒤집기). 우리에겐 그 자리가 없어 호출이 없다.
    for i in 0..4 {
        assert_eq!(osc(&format!("4;{};red", 256 + i)), vec![]);
    }
}

// color.zig 481: OSC 4: multiple requests
#[test]
fn osc4_multiple_requests() {
    assert_eq!(osc("4;0;red;1;blue"), vec![Ev::Set(0, RED), Ev::Set(1, BLUE)]);
    // 같은 번호를 거듭 바꾸면 요청이 둘 다 나간다
    assert_eq!(osc("4;0;red;0;blue"), vec![Ev::Set(0, RED), Ev::Set(0, BLUE)]);
}

// 이슈 #765 재현: 끝 `;`가 붙은 것을 통째로 버리지 않고, 잘못된 번호 뒤로 이어 가지 않는다
#[test]
fn osc4_trailing_semicolon_and_stop_at_bad_index() {
    assert_eq!(osc("4;0;rgb:ff/00/00;"), vec![Ev::Set(0, RED)]);
    assert_eq!(
        osc("4;1;rgb:ff/00/00;0_3;rgb:00/00/ff;2;rgb:00/ff/00"),
        vec![Ev::Set(1, RED)]
    );
    // 틀린 색에서도 멈춘다
    assert_eq!(osc("4;0;red;1;nosuch;2;blue"), vec![Ev::Set(0, RED)]);
}

// color.zig 547: OSC 104:
#[test]
fn osc104_every_index() {
    for idx in 0..255usize {
        assert_eq!(osc(&format!("104;{idx}")), vec![Ev::Reset(idx)], "reset {idx}");
    }
    // 256~259(특수 색)는 우리에게 자리가 없어 호출이 없고, 번호로는 센다(전체 되돌리기로 번지지 않음)
    for i in 0..4 {
        assert_eq!(osc(&format!("104;{}", 256 + i)), vec![]);
    }
}

// color.zig 595: OSC 104: empty index
#[test]
fn osc104_empty_index() {
    assert_eq!(osc("104;0;;1"), vec![Ev::Reset(0), Ev::Reset(1)]);
    // 앞이 빈 조각이어도 전체를 되돌리지 않는다(Ghostty는 쓸 만한 번호가 있으면 그것만)
    assert_eq!(osc("104;;5"), vec![Ev::Reset(5)]);
}

// color.zig 609: OSC 104: invalid index
#[test]
fn osc104_invalid_index() {
    assert_eq!(osc("104;ffff;1"), vec![Ev::Reset(1)]);
}

// color.zig 621: OSC 104: reset all
#[test]
fn osc104_reset_all() {
    let all: Vec<Ev> = (0..256).map(Ev::Reset).collect();
    assert_eq!(osc("104"), all);
    // 인자 쪽 조각이 모두 비면(`104;`) 같다
    assert_eq!(osc("104;"), (0..256).map(Ev::Reset).collect::<Vec<_>>());
}

// color.zig 656: OSC 10, 11, 12 … dynamic (13~19는 우리에게 없다)
#[test]
fn osc10_11_12_dynamic() {
    for (code, index) in [(10, FOREGROUND), (11, BACKGROUND), (12, CURSOR)] {
        // printf '\e]10;red\e\\'
        assert_eq!(osc(&format!("{code};red")), vec![Ev::Set(index, RED)]);
        assert_eq!(osc(&format!("{code};?")), vec![Ev::Query(index)]);
    }
}

// color.zig ~712: dynamic multiple. printf '\e]11;red;blue\e\\'
#[test]
fn osc10_11_12_dynamic_multiple() {
    assert_eq!(
        osc("11;red;blue"),
        vec![Ev::Set(BACKGROUND, RED), Ev::Set(CURSOR, BLUE)]
    );
    // 틀린 색에서 멈춘다(그 뒤 것을 건너 쓰지 않는다)
    assert_eq!(osc("10;nosuch;red"), vec![]);
    assert_eq!(osc("10;red;nosuch;blue"), vec![Ev::Set(FOREGROUND, RED)]);
}

// color.zig ~738: OSC 110 … reset dynamic
#[test]
fn osc110_111_112_reset_dynamic() {
    for (code, index) in [(110, FOREGROUND), (111, BACKGROUND), (112, CURSOR)] {
        // printf '\e]110\e\\'
        assert_eq!(osc(&code.to_string()), vec![Ev::Reset(index)]);
        // xterm은 끝 `;`를 받는다: printf '\e]110;\e\\'
        assert_eq!(osc(&format!("{code};")), vec![Ev::Reset(index)]);
        assert_eq!(osc(&format!("{code};;")), vec![Ev::Reset(index)]);
        // 공백은 안 된다: printf '\e]110 \e\\'
        assert_eq!(osc(&format!("{code} ")), vec![]);
        // 인자가 있으면 되돌리지 않는다(이슈 #765: `110;junk`)
        assert_eq!(osc(&format!("{code};junk")), vec![]);
        assert_eq!(osc(&format!("{code};?")), vec![]);
    }
}

// color.zig 984: RGB.parse
#[test]
fn rgb_parse_forms() {
    assert_eq!(color_of("rgbi:1.0/0/0"), Some(rgb(255, 0, 0)));
    assert_eq!(color_of("rgb:7f/a0a0/0"), Some(rgb(127, 160, 0)));
    assert_eq!(color_of("rgb:f/ff/fff"), Some(rgb(255, 255, 255)));
    assert_eq!(color_of("#ffffff"), Some(rgb(255, 255, 255)));
    assert_eq!(color_of("#fffffffff"), Some(rgb(255, 255, 255)));
    assert_eq!(color_of("#ffffffffffff"), Some(rgb(255, 255, 255)));
    assert_eq!(color_of("#ff0010"), Some(rgb(255, 0, 16)));
    // `#` 없는 꼴(Ghostty 설정·테마 값과 같게)
    assert_eq!(color_of("0A0B0C"), Some(rgb(10, 11, 12)));
    assert_eq!(color_of("FFFFFF"), Some(rgb(255, 255, 255)));
    assert_eq!(color_of("FFF"), Some(rgb(255, 255, 255)));
    assert_eq!(color_of(" #AABBCC   "), Some(rgb(170, 187, 204)));
    // 따르지 않음: Ghostty는 `#fff`를 ffffff, `#345`를 334455로 늘려 맞춘다. 우리는 xterm처럼 0으로 채운다.
    assert_eq!(color_of("#fff"), Some(rgb(0xF0, 0xF0, 0xF0)));
    assert_eq!(color_of("#345"), Some(rgb(0x30, 0x40, 0x50)));

    // X11 색 이름: 대소문자 무시, 안쪽 공백 있어도 없어도, 앞뒤 공백·탭 무시
    assert_eq!(color_of("black"), Some(rgb(0, 0, 0)));
    assert_eq!(color_of("red"), Some(rgb(255, 0, 0)));
    assert_eq!(color_of("green"), Some(rgb(0, 255, 0)));
    assert_eq!(color_of("blue"), Some(rgb(0, 0, 255)));
    assert_eq!(color_of("white"), Some(rgb(255, 255, 255)));
    assert_eq!(color_of("LawnGreen"), Some(rgb(124, 252, 0)));
    assert_eq!(color_of("medium spring green"), Some(rgb(0, 250, 154)));
    assert_eq!(color_of(" Forest Green "), Some(rgb(34, 139, 34)));
    assert_eq!(color_of("\tForestGreen\t"), Some(rgb(34, 139, 34)));
    // x11_color.zig 시험의 이름들
    assert_eq!(color_of("FoReStGReen"), Some(rgb(34, 139, 34)));
    assert_eq!(color_of("mediumspringgreen"), Some(rgb(0, 250, 154)));
    // 이슈 #765의 예
    assert_eq!(color_of("Alice Blue"), Some(rgb(240, 248, 255)));
    assert_eq!(color_of("aliceblue"), Some(rgb(240, 248, 255)));
    assert_eq!(color_of("snow"), Some(rgb(255, 250, 250)));

    // 틀린 꼴
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
    ] {
        assert_eq!(color_of(bad), None, "{bad:?}");
    }
}

// color.zig 1035: RGB.parse rejects digit separators
#[test]
fn rgb_parse_rejects_digit_separators() {
    assert_eq!(color_of("rgb:f_f/0/0"), None);
    assert_eq!(color_of("#f_f000000"), None);
    // 부호도 받지 않는다
    assert_eq!(color_of("rgb:+f/0/0"), None);
    assert_eq!(color_of("#+f0+f0+f0"), None);
}

// fraction.zig 시험의 일부(rgbi 채널): 범위 밖·지수·16진 실수는 거절
#[test]
fn rgbi_fractions() {
    assert_eq!(color_of("rgbi:0.5/0.25/1"), Some(rgb(127, 63, 255)));
    assert_eq!(color_of("rgbi:.5/1./+0.5"), Some(rgb(127, 255, 127)));
    assert_eq!(color_of("rgbi:-0/0.0/0"), Some(rgb(0, 0, 0)));
    for bad in [
        "rgbi:2/0/0",
        "rgbi:-0.5/0/0",
        "rgbi:1.0000001/0/0",
        "rgbi:./0/0",
        "rgbi:1e0/0/0",
        "rgbi:0x0.8/0/0",
    ] {
        assert_eq!(color_of(bad), None, "{bad:?}");
    }
}

// hyperlink.zig: OSC 8 시험들
#[test]
fn osc8_ghostty_tests() {
    let link = |id: Option<&str>, uri: &str| vec![Ev::Link(Some((id.map(str::to_owned), uri.to_owned())))];

    assert_eq!(osc("8;;http://example.com"), link(None, "http://example.com"));
    assert_eq!(
        osc("8;id=foo;http://example.com"),
        link(Some("foo"), "http://example.com")
    );
    // with empty id: 이슈 #765, 빈 id는 id 없음
    assert_eq!(osc("8;id=;http://example.com"), link(None, "http://example.com"));
    // with incomplete key
    assert_eq!(osc("8;id;http://example.com"), link(None, "http://example.com"));
    // with empty key
    assert_eq!(osc("8;=value;http://example.com"), link(None, "http://example.com"));
    // with empty key and id
    assert_eq!(
        osc("8;=value:id=foo;http://example.com"),
        link(Some("foo"), "http://example.com")
    );
    // with empty uri: 명령이 없다(열린 링크가 이어진다). 이슈 #765
    assert_eq!(osc("8;id=foo;"), vec![]);
    // hyperlink end
    assert_eq!(osc("8;;"), vec![Ev::Link(None)]);
    // 주소의 `;`는 그대로
    assert_eq!(osc("8;;http://a/?x=1;y=2"), link(None, "http://a/?x=1;y=2"));
    // 주소 칸이 없으면(`8;x`) 명령이 아니다
    assert_eq!(osc("8;x"), vec![]);
}

// change_window_title.zig + stream.zig 4493: 깨진 UTF-8은 요청 무시
#[test]
fn title_invalid_utf8_is_ignored() {
    assert_eq!(osc("2;abc"), vec![Ev::Title(Some("abc".into()))]);
    // printf '\e]2;abc\xc0\e\\': 요청 전체를 무시하고 제목을 지우지 않는다
    assert_eq!(run(b"\x1b]2;abc\xc0\x1b\\"), vec![]);
    assert_eq!(run(b"\x1b]0;a;b\xff;c\x07"), vec![]);
    assert_eq!(
        run("\x1b]0;한글;제목\x07".as_bytes()),
        vec![Ev::Title(Some("한글;제목".into()))]
    );
}

// Ghostty는 앞뒤 공백을 지우지 않는다
#[test]
fn title_keeps_surrounding_spaces() {
    assert_eq!(osc("0;  a b  "), vec![Ev::Title(Some("  a b  ".into()))]);
    assert_eq!(osc("2;"), vec![Ev::Title(Some(String::new()))]);
}

// 조각 16개 상한이 제목을 자르지 않는다: `;` 뒤 전체가 제목
#[test]
fn title_with_many_semicolons() {
    let pieces: Vec<String> = (0..20).map(|i| format!("p{i}")).collect();
    let title = pieces.join(";");
    assert_eq!(osc(&format!("0;{title}")), vec![Ev::Title(Some(title.clone()))]);
    assert_eq!(osc(&format!("2;{title}")), vec![Ev::Title(Some(title))]);
    // 빈 조각만 많아도 글자 수가 같다
    let semis = ";".repeat(40);
    assert_eq!(osc(&format!("0;{semis}")), vec![Ev::Title(Some(semis))]);
}

// change_window_title.zig 36·47과 osc.zig 361(MAX_BUF = 2048, 끝 널 한 바이트 포함)
#[test]
fn title_size_limit() {
    let title = |n: usize| "a".repeat(n);
    // one shorter than buffer length(2047)는 받는다
    assert_eq!(osc(&format!("0;{}", title(2047))), vec![Ev::Title(Some(title(2047)))]);
    // 2048부터는 명령이 없다. longer than buffer(MAX_BUF + 2)도 같다
    assert_eq!(osc(&format!("0;{}", title(2048))), vec![]);
    assert_eq!(osc(&format!("2;{}", title(2050))), vec![]);
    // `;`도 글자 수에 든다
    let semi_title = format!("{};{}", title(1023), title(1024));
    assert_eq!(semi_title.len(), 2048);
    assert_eq!(osc(&format!("0;{semi_title}")), vec![]);
}

// OSC 8도 같은 고정 캡처(`params;uri`가 2047바이트까지)
#[test]
fn osc8_size_limit() {
    let uri = |n: usize| format!("http://{}", "a".repeat(n - 7));
    // `;` 하나(빈 매개변수 뒤)를 합쳐 2047
    let ok = uri(2046);
    assert_eq!(osc(&format!("8;;{ok}")), vec![Ev::Link(Some((None, ok.clone())))]);
    let too_long = uri(2047);
    assert_eq!(osc(&format!("8;;{too_long}")), vec![]);
}
