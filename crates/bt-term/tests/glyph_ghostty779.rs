//! Glyph Protocol(APC `25a1`)를 Ghostty 1.4.0 앱처럼 받고 답하는지(#779). 바이트를 `Emulator`에 넣고 PTY로 갈 답을 본다.
//!
//! 옮긴 시험(Ghostty 1.4.0 `src/terminal/` 아래). 같은 입력과 기대로 옮겼다:
//! - `stream_terminal.zig` "glyph protocol APC with write_pty callback"(3303줄), "full reset"의 글리프 부분(3275줄),
//!   "ignores query actions"의 `25a1;r;cp=41;%%%invalid%%%` 줄(3348줄)
//! - `Terminal.zig` "Terminal: glyph APC stores session glossary entries"(17336줄)
//! - `apc.zig` "garbage glyph command"(649줄), "valid glyph command"(660줄), "feedSlice valid glyph command"(726줄)
//! - `apc/glyph/execute.zig` 시험 12개 전부(138줄 `execute register stores glyph and returns success` ~ 356줄 `execute query without cp returns no response`)
//!
//! 따르지 않은 곳:
//! - `apc.zig` "disabled glyph command is ignored"(757줄): 앱에는 글리프 APC를 끄는 설정이 없다(빌드 옵션 `glyph_protocol`뿐). 설정을 만들지 않는다.
//! - `Terminal.zig`의 `flags.dirty.glyph_glossary`(다시 그리기 표시): 우리는 글리프를 그리지 않으므로 표시할 것이 없다.
//! - Ghostty 시험은 파서·실행기를 따로 부른다. 여기서는 모두 APC 바이트로 넣고 답 바이트로 본다.

use bt_core::Theme;
use bt_term::{Emulator, GridSize};

const ESC: &str = "\x1b";

fn emu() -> Emulator {
    Emulator::new(GridSize::new(80, 24), Theme::default())
}

/// APC 한 줄(`ESC _ 25a1 ; body ESC \`)을 넣고 나온 답을 글로 돌려준다
fn ask(e: &mut Emulator, body: &str) -> String {
    e.feed(format!("{ESC}_25a1;{body}{ESC}\\").as_bytes());
    String::from_utf8(e.take_replies()).expect("답은 UTF-8")
}

/// 답 바이트 틀: `ESC _ 25a1 ; body ESC \`
fn wire(body: &str) -> String {
    format!("{ESC}_25a1;{body}{ESC}\\")
}

const GLYPH: &str = "AAAAAAAAAAAAAA==";

// ---- stream_terminal.zig ----

/// stream_terminal.zig 3303줄 `glyph protocol APC with write_pty callback`
#[test]
fn write_pty_callback_support_then_register() {
    let mut e = emu();
    assert_eq!(ask(&mut e, "s"), wire("s;fmt=glyf"));
    assert_eq!(ask(&mut e, &format!("r;cp=e0a0;{GLYPH}")), wire("r;cp=e0a0;status=0"));
    assert_eq!(ask(&mut e, "q;cp=e0a0"), wire("q;cp=e0a0;status=glossary"));
}

/// stream_terminal.zig 3275줄 `full reset`: 등록하고 `ESC c`로 초기화하면 등록이 사라진다
#[test]
fn full_reset_clears_the_glossary() {
    let mut e = emu();
    e.feed(b"Hello\x1b[10;20H\x1b[5;20r\x1b[?7l");
    assert_eq!(ask(&mut e, &format!("r;cp=e0a0;{GLYPH}")), wire("r;cp=e0a0;status=0"));
    assert_eq!(ask(&mut e, "q;cp=e0a0"), wire("q;cp=e0a0;status=glossary"));
    e.feed(b"\x1bc");
    assert_eq!(e.take_replies(), b"");
    assert_eq!(ask(&mut e, "q;cp=e0a0"), wire("q;cp=e0a0;status="));
}

/// 소프트 리셋(DECSTR)은 글리프 등록을 지우지 않는다. Ghostty는 `fullReset`만 지운다(Terminal.zig 5134줄)
#[test]
fn soft_reset_keeps_the_glossary() {
    let mut e = emu();
    ask(&mut e, &format!("r;cp=e0a0;{GLYPH}"));
    e.feed(b"\x1b[!p");
    assert_eq!(ask(&mut e, "q;cp=e0a0"), wire("q;cp=e0a0;status=glossary"));
}

/// stream_terminal.zig 3348줄 `ignores query actions`의 `r;cp=41;%%%invalid%%%`: 거절하고 터미널은 멀쩡하다
#[test]
fn rejected_glyph_leaves_the_terminal_working() {
    let mut e = emu();
    assert_eq!(
        ask(&mut e, "r;cp=41;%%%invalid%%%"),
        wire("r;cp=41;status=1;reason=malformed_payload")
    );
    e.feed(b"Test");
    assert_eq!(e.snapshot().row_text(0), "Test");
}

// ---- Terminal.zig 17336줄 ----

/// Terminal.zig `Terminal: glyph APC stores session glossary entries`
#[test]
fn terminal_stores_session_glossary_entries() {
    let mut e = emu();
    assert_eq!(ask(&mut e, &format!("r;cp=e0a0;{GLYPH}")), wire("r;cp=e0a0;status=0"));
    assert_eq!(ask(&mut e, "q;cp=e0a0"), wire("q;cp=e0a0;status=glossary"));
    e.feed(b"\x1bc");
    assert_eq!(ask(&mut e, "q;cp=e0a0"), wire("q;cp=e0a0;status="));
}

// ---- apc.zig ----

/// apc.zig 649줄 `garbage glyph command`: 알 수 없는 동사는 답이 없다
#[test]
fn garbage_command_gets_no_reply() {
    let mut e = emu();
    assert_eq!(ask(&mut e, "X"), "");
    assert_eq!(ask(&mut e, ""), "");
    assert_eq!(ask(&mut e, "sx"), "");
}

/// apc.zig 660줄 `valid glyph command`, 726줄 `feedSlice valid glyph command`: 한 바이트씩 와도 같다.
/// 식별자(`25a1;`)가 다른 APC나 `;` 없는 `25a1`은 답이 없다
#[test]
fn query_split_into_single_bytes_and_other_identifiers() {
    let mut e = emu();
    let all = format!("{ESC}_25a1;q;cp=E0A0{ESC}\\");
    for b in all.bytes() {
        e.feed(&[b]);
    }
    assert_eq!(String::from_utf8(e.take_replies()).unwrap(), wire("q;cp=e0a0;status="));
    e.feed(
        format!("{ESC}_25a1{ESC}\\{ESC}_25a;q;cp=e0a0{ESC}\\{ESC}_25a2;q;cp=e0a0{ESC}\\{ESC}_25a1x;s{ESC}\\")
            .as_bytes(),
    );
    assert_eq!(e.take_replies(), b"");
}

/// 끝맺음이 CAN·SUB·8비트 ST여도 앱은 끝난 APC를 실행한다(#595·#776). 닫기 전에 끊기지 않은 APC는 답하지 않는다
#[test]
fn terminators_other_than_esc_backslash() {
    let mut e = emu();
    e.feed(b"\x1b_25a1;s\x18");
    assert_eq!(e.take_replies(), wire("s;fmt=glyf").as_bytes());
    e.feed(b"\x1b_25a1;s\x1a");
    assert_eq!(e.take_replies(), wire("s;fmt=glyf").as_bytes());
    e.feed(b"\x1b_25a1;s\x9c");
    assert_eq!(e.take_replies(), wire("s;fmt=glyf").as_bytes());
    e.feed(b"\x1b_25a1;s");
    assert_eq!(e.take_replies(), b"", "끝나지 않은 APC는 답하지 않는다");
}

/// 지원 질의는 `s` 뒤에 아무 옵션이 붙어도 같다(`raw[1] == ';'`만 본다)
#[test]
fn support_query_with_trailing_options() {
    let mut e = emu();
    assert_eq!(ask(&mut e, "s;foo=bar"), wire("s;fmt=glyf"));
    assert_eq!(ask(&mut e, "s;"), wire("s;fmt=glyf"));
}

// ---- execute.zig ----

#[test]
fn execute_register_stores_glyph_and_returns_success() {
    let mut e = emu();
    assert_eq!(ask(&mut e, &format!("r;cp=e0a0;{GLYPH}")), wire("r;cp=e0a0;status=0"));
    assert_eq!(ask(&mut e, "q;cp=e0a0"), wire("q;cp=e0a0;status=glossary"));
}

#[test]
fn execute_register_reply_failures_suppresses_success() {
    let mut e = emu();
    assert_eq!(ask(&mut e, &format!("r;cp=e0a0;reply=2;{GLYPH}")), "");
    assert_eq!(ask(&mut e, "q;cp=e0a0"), wire("q;cp=e0a0;status=glossary"));
}

#[test]
fn execute_register_reply_none_suppresses_failure() {
    let mut e = emu();
    assert_eq!(ask(&mut e, "r;cp=41;reply=0;%%%not-base64%%%"), "");
    assert_eq!(ask(&mut e, "q;cp=41"), wire("q;cp=41;status="));
    // reply=2는 실패는 답한다
    assert_eq!(
        ask(&mut e, "r;cp=41;reply=2;%%%not-base64%%%"),
        wire("r;cp=41;status=1;reason=malformed_payload")
    );
}

#[test]
fn execute_register_rejects_non_pua() {
    let mut e = emu();
    assert_eq!(
        ask(&mut e, &format!("r;cp=41;{GLYPH}")),
        wire("r;cp=41;status=1;reason=out_of_namespace")
    );
    assert_eq!(ask(&mut e, "q;cp=41"), wire("q;cp=41;status="));
}

#[test]
fn execute_register_reports_malformed_payload() {
    let mut e = emu();
    assert_eq!(
        ask(&mut e, "r;cp=e0a0;%%%not-base64%%%"),
        wire("r;cp=e0a0;status=1;reason=malformed_payload")
    );
    assert_eq!(ask(&mut e, "q;cp=e0a0"), wire("q;cp=e0a0;status="));
}

#[test]
fn execute_clear_removes_all_glyphs() {
    let mut e = emu();
    ask(&mut e, &format!("r;cp=e0a0;{GLYPH}"));
    ask(&mut e, &format!("r;cp=e0a1;{GLYPH}"));
    assert_eq!(ask(&mut e, "c"), wire("c;status=0"));
    assert_eq!(ask(&mut e, "q;cp=e0a0"), wire("q;cp=e0a0;status="));
    assert_eq!(ask(&mut e, "q;cp=e0a1"), wire("q;cp=e0a1;status="));
}

#[test]
fn execute_clear_removes_one_glyph() {
    let mut e = emu();
    ask(&mut e, &format!("r;cp=e0a0;{GLYPH}"));
    ask(&mut e, &format!("r;cp=e0a1;{GLYPH}"));
    assert_eq!(ask(&mut e, "c;cp=e0a0"), wire("c;status=0"));
    assert_eq!(ask(&mut e, "q;cp=e0a0"), wire("q;cp=e0a0;status="));
    assert_eq!(ask(&mut e, "q;cp=e0a1"), wire("q;cp=e0a1;status=glossary"));
}

#[test]
fn execute_clear_rejects_non_pua() {
    let mut e = emu();
    assert_eq!(ask(&mut e, "c;cp=41"), wire("c;status=1;reason=out_of_namespace"));
}

#[test]
fn execute_clear_rejects_malformed_cp_without_clearing_glossary() {
    let mut e = emu();
    ask(&mut e, &format!("r;cp=e0a0;{GLYPH}"));
    ask(&mut e, &format!("r;cp=e0a1;{GLYPH}"));
    for data in ["c;cp=zz", "c;cp=", "c;cp=200000"] {
        assert_eq!(ask(&mut e, data), wire("c;status=1;reason=malformed_payload"), "{data}");
        assert_eq!(ask(&mut e, "q;cp=e0a0"), wire("q;cp=e0a0;status=glossary"), "{data}");
        assert_eq!(ask(&mut e, "q;cp=e0a1"), wire("q;cp=e0a1;status=glossary"), "{data}");
    }
}

#[test]
fn execute_query_reports_no_coverage() {
    let mut e = emu();
    assert_eq!(ask(&mut e, "q;cp=e0a0"), wire("q;cp=e0a0;status="));
}

#[test]
fn execute_query_reports_glossary_coverage() {
    let mut e = emu();
    ask(&mut e, &format!("r;cp=e0a0;{GLYPH}"));
    assert_eq!(ask(&mut e, "q;cp=e0a0"), wire("q;cp=e0a0;status=glossary"));
}

#[test]
fn execute_query_without_cp_returns_no_response() {
    let mut e = emu();
    assert_eq!(ask(&mut e, "q;foo=bar"), "");
}

// ---- 한도와 보안(Ghostty 앱의 한도를 그대로) ----

/// 등록은 칸마다 1024개. 넘으면 가장 오래된 것부터 밀려난다(Glossary.zig `Glossary register evicts oldest entry`)
#[test]
fn registrations_are_capped_at_1024_and_the_oldest_is_evicted() {
    let mut e = emu();
    let mut script = String::new();
    for i in 0..1025u32 {
        script.push_str(&format!("{ESC}_25a1;r;cp={:x};reply=0;{GLYPH}{ESC}\\", 0xE000 + i));
    }
    e.feed(script.as_bytes());
    assert_eq!(e.take_replies(), b"");
    assert_eq!(ask(&mut e, "q;cp=e000"), wire("q;cp=e000;status="));
    assert_eq!(ask(&mut e, "q;cp=e001"), wire("q;cp=e001;status=glossary"));
    assert_eq!(
        ask(&mut e, &format!("q;cp={:x}", 0xE000 + 1024)),
        wire("q;cp=e400;status=glossary")
    );
}

/// 같은 cp를 다시 등록하면 덮어쓰고 가장 새것이 된다. 그래서 오래된 쪽부터 밀려난다
#[test]
fn re_registering_moves_the_entry_to_newest() {
    let mut e = emu();
    let mut script = format!("{ESC}_25a1;r;cp=e000;reply=0;{GLYPH}{ESC}\\");
    for i in 1..1024u32 {
        script.push_str(&format!("{ESC}_25a1;r;cp={:x};reply=0;{GLYPH}{ESC}\\", 0xE000 + i));
    }
    // e000을 다시 등록해 맨 뒤로 보낸 뒤 하나 더 넣으면 e001이 밀려난다
    script.push_str(&format!("{ESC}_25a1;r;cp=e000;reply=0;{GLYPH}{ESC}\\"));
    script.push_str(&format!("{ESC}_25a1;r;cp=f400;reply=0;{GLYPH}{ESC}\\"));
    e.feed(script.as_bytes());
    assert_eq!(ask(&mut e, "q;cp=e000"), wire("q;cp=e000;status=glossary"));
    assert_eq!(ask(&mut e, "q;cp=e001"), wire("q;cp=e001;status="));
}

/// PUA 세 구역 경계(Glossary.zig `isPrivateUse`)
#[test]
fn only_the_three_private_use_areas_are_accepted() {
    let mut e = emu();
    for (cp, ok) in [
        ("dfff", false),
        ("e000", true),
        ("f8ff", true),
        ("f900", false),
        ("effff", false),
        ("f0000", true),
        ("ffffd", true),
        ("ffffe", false),
        ("fffff", false),
        ("fffff", false),
        ("100000", true),
        ("10fffd", true),
        ("10fffe", false),
        ("1fffff", false),
    ] {
        let want = if ok {
            wire(&format!("r;cp={cp};status=0"))
        } else {
            wire(&format!("r;cp={cp};status=1;reason=out_of_namespace"))
        };
        assert_eq!(ask(&mut e, &format!("r;cp={cp};{GLYPH}")), want, "{cp}");
    }
}

/// APC 한 줄은 1MiB까지(`CommandParser.init(alloc, 1024 * 1024)`, apc.zig 409줄). 넘으면 통째로 버리고 답하지 않는다
#[test]
fn request_over_one_mebibyte_is_dropped_without_a_reply() {
    let mut e = emu();
    // 본문이 정확히 1MiB(`r;cp=e0a0;` 뒤를 `A`로 채움): 상한 안이라 읽는다. 길이가 4의 배수가 아니라 base64가 틀렸다고 답한다
    let head = "r;cp=e0a0;";
    let pad = "A".repeat(1024 * 1024 - head.len());
    assert_eq!(
        ask(&mut e, &format!("{head}{pad}")),
        wire("r;cp=e0a0;status=1;reason=malformed_payload")
    );
    // 길이가 4의 배수인 큰 글은 디코드하기 전에 64KiB를 넘는다고 거절한다
    let pad = "A".repeat(1024 * 1024 - head.len() - 2);
    assert_eq!(
        ask(&mut e, &format!("{head}{pad}")),
        wire("r;cp=e0a0;status=1;reason=payload_too_large")
    );
    let over = "A".repeat(1024 * 1024 - head.len() + 1);
    assert_eq!(ask(&mut e, &format!("{head}{over}")), "");
    // 그 뒤 다음 요청은 정상이다
    assert_eq!(ask(&mut e, "s"), wire("s;fmt=glyf"));
}

/// 답에는 요청의 글을 그대로 넣지 않는다: cp는 숫자로 다시 쓴다(`formatWire`의 `cp={x}`). 0 채움·부호·밑줄이 든 입력도 같은 꼴이다
#[test]
fn replies_rewrite_values_as_numbers() {
    let mut e = emu();
    assert_eq!(ask(&mut e, "q;cp=000E0A0"), wire("q;cp=e0a0;status="));
    assert_eq!(ask(&mut e, "q;cp=+E0A0"), wire("q;cp=e0a0;status="));
    assert_eq!(
        ask(&mut e, "q;cp=E0A0x"),
        "",
        "16진수가 아닌 글자가 든 값은 읽히지 않는다"
    );
    // cp가 없거나 읽히지 않는 등록은 `cp=0`으로 답한다(`reg.get(.cp) orelse 0`)
    assert_eq!(
        ask(&mut e, &format!("r;{GLYPH}")),
        "",
        "옵션 구분이 없는 등록은 형식 오류라 답이 없다"
    );
    assert_eq!(
        ask(&mut e, &format!("r;foo=1;{GLYPH}")),
        wire("r;cp=0;status=1;reason=malformed_payload")
    );
}
