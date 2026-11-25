//! #822: OSC 해석 세부 셋을 Ghostty 1.4.0과 맞춘다.
//! 1. 133;D의 종료 코드는 `parseInt`(`lib/parse_int.zig`)라 앞뒤 공백을 허용하지 않는다(`osc133::parse` 하나만 쓴다).
//! 2. OSC 133은 `133;` 뒤 2048바이트까지 받고 넘으면 명령 전체를 버린다(`osc.zig` `captureTrailing(.fixed)`, `MAX_BUF = 2048`).
//! 3. OSC 99 페이로드(`;` 뒤)에 0x00~0x1F·0x7F·U+0080~U+009F가 있으면 명령 전체를 버린다(`osc/encoding.zig` `isSafeUtf8`,
//!    `kitty_desktop_notification.zig` `parse`는 페이로드에만 건다. 메타데이터에는 걸지 않는다).

use bt_term::notify::{PromptMark, Scanner, Signal};

fn feed(input: &[u8]) -> Vec<Signal> {
    Scanner::default().feed(input)
}

fn finished(sigs: &[Signal]) -> Option<Option<i32>> {
    sigs.iter().find_map(|s| match s {
        Signal::Prompt(PromptMark::CommandFinished(c)) => Some(*c),
        _ => None,
    })
}

#[test]
fn exit_code_does_not_accept_spaces() {
    // Ghostty: parseInt(" 5 ") = InvalidCharacter -> null
    assert_eq!(finished(&feed(b"\x1b]133;D; 5 \x07")), Some(None));
    assert_eq!(finished(&feed(b"\x1b]133;D;5 \x07")), Some(None));
    assert_eq!(finished(&feed(b"\x1b]133;D; 5\x07")), Some(None));
    // 값이 맞으면 그대로(부호도 parseInt가 받는다)
    assert_eq!(finished(&feed(b"\x1b]133;D;5\x07")), Some(Some(5)));
    assert_eq!(finished(&feed(b"\x1b]133;D;+5\x07")), Some(Some(5)));
    assert_eq!(finished(&feed(b"\x1b]133;D;-1\x07")), Some(Some(-1)));
    assert_eq!(finished(&feed(b"\x1b]133;D;5;aid=1\x07")), Some(Some(5)));
    assert_eq!(finished(&feed(b"\x1b]133;D\x07")), Some(None));
    // 넘치면 null
    assert_eq!(finished(&feed(b"\x1b]133;D;2147483648\x07")), Some(None));
}

#[test]
fn exit_code_of_signals_agrees_with_semantic() {
    // 두 신호가 같은 값을 읽는다(따로 읽지 않는다)
    for input in ["D; 5 ", "D;5", "D;x", "D;", "D;+3", "D;5;k=i"] {
        let sigs = feed(format!("\x1b]133;{input}\x07").as_bytes());
        let sem = sigs.iter().find_map(|s| match s {
            Signal::Semantic(sp) => Some(sp.exit_code),
            _ => None,
        });
        assert_eq!(sem, finished(&sigs), "{input:?}");
    }
}

fn osc133_with_trailing(len: usize) -> Vec<u8> {
    // `133;` 뒤가 정확히 len바이트: `A;aid=` + 채움
    let mut s = b"\x1b]133;A;aid=".to_vec();
    let fill = len - "A;aid=".len();
    s.extend(std::iter::repeat_n(b'x', fill));
    s.push(0x07);
    s
}

#[test]
fn osc133_accepts_2048_trailing_bytes_and_drops_2049() {
    let ok = feed(&osc133_with_trailing(2048));
    assert!(
        ok.iter().any(|s| matches!(s, Signal::Prompt(PromptMark::PromptStart))),
        "2048은 받는다"
    );
    assert!(ok.iter().any(|s| matches!(s, Signal::Semantic(_))));
    let over = feed(&osc133_with_trailing(2049));
    assert!(over.is_empty(), "2049는 명령 전체를 버린다: {over:?}");
}

#[test]
fn osc133_over_limit_drops_d_too() {
    let mut s = b"\x1b]133;D;5;aid=".to_vec();
    s.extend(std::iter::repeat_n(b'x', 2049 - "D;5;aid=".len()));
    s.push(0x07);
    assert!(feed(&s).is_empty());
}

#[test]
fn osc133_limit_counts_bytes_not_chars() {
    // 한글은 3바이트: `A;aid=` 6바이트 + 3바이트 * 681 = 2049 -> 버린다, 680개(2046)는 받는다
    let build = |n: usize| {
        let mut s = b"\x1b]133;A;aid=".to_vec();
        s.extend("가".repeat(n).as_bytes());
        s.push(0x07);
        s
    };
    assert!(!feed(&build(680)).is_empty());
    assert!(feed(&build(681)).is_empty());
}

fn kitty(input: &str) -> Vec<Signal> {
    feed(format!("\x1b]99;{input}\x07").as_bytes())
}

fn body(sigs: &[Signal]) -> Option<String> {
    sigs.iter().find_map(|s| match s {
        Signal::Notify(n) => Some(format!("{}{}", n.title, n.body)),
        _ => None,
    })
}

#[test]
fn osc99_payload_with_c0_del_or_c1_drops_whole_command() {
    assert_eq!(
        body(&kitty(";a b")).as_deref(),
        Some("a b"),
        "기준: 깨끗한 본문은 알림이 된다"
    );
    assert!(kitty(";a\x7fb").is_empty(), "DEL");
    assert!(kitty(";a\u{85}b").is_empty(), "U+0085(NEL)");
    assert!(kitty(";a\u{9f}b").is_empty(), "U+009F");
    // OSC 안의 C0(BEL·CAN·SUB·ESC 말고)는 VT 상태표가 문자열에 넣지 않고 버리므로(엔진·Ghostty 모두) 검사까지 오지 않는다
    assert_eq!(body(&kitty(";a\tb")).as_deref(), Some("ab"));
    // 경계 밖은 받는다: U+00A0, U+007E
    assert_eq!(body(&kitty(";a\u{a0}b")).as_deref(), Some("a\u{a0}b"));
    assert_eq!(body(&kitty(";a~b")).as_deref(), Some("a~b"));
}

#[test]
fn osc99_control_in_payload_drops_title_and_query_too() {
    assert!(kitty("p=title;a\x7fb").is_empty());
    assert!(kitty("i=1:p=?;\x7f").is_empty(), "질의도 통째로 버려 답하지 않는다");
}

#[test]
fn osc99_dropped_chunk_does_not_touch_pending() {
    let mut s = Scanner::default();
    assert!(s.feed(b"\x1b]99;i=7:d=0:p=title;T\x07").is_empty());
    assert!(s.feed(b"\x1b]99;i=7:d=0:p=body;a\x7fb\x07").is_empty());
    let got = s.feed(b"\x1b]99;i=7:p=body;ok\x07");
    assert!(
        matches!(&got[..], [Signal::Notify(n)] if n.title == "T" && n.body == "ok"),
        "{got:?}"
    );
}

#[test]
fn osc99_metadata_is_not_checked_for_control_bytes() {
    // Ghostty `isSafeUtf8`는 페이로드에만 건다. 메타데이터의 제어 문자는 다른 검사(키·값 글자)가 다룬다
    assert_eq!(body(&kitty("i=1\x7f;hello")).as_deref(), Some("hello"));
}
