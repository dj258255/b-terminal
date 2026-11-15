//! 붙여넣기 인코딩·안전 판단 시험. Ghostty `src/input/paste.zig`의 시험(main 7fa26e8b675f)을 옮기고,
//! 위험 바이트 16종과 멀티바이트 글자 시험을 더했다(#565).
//! 옮기지 않은 시험: Zig의 `[]const u8`/`[]u8` 구분(`MutableRequired`)을 보는 `encode ... const` 둘,
//! 쓰기 장치로 나눠 쓰는 `encodeWriter chunks through a small writer buffer`·`encodeWriter too small`,
//! `isSafeWith`(Ghostty 앱은 `Surface.zig`에서 부르지 않는다). 러스트는 항상 새 `Vec`을 만들어 돌려주므로 해당이 없다.

use super::*;

const START: &str = "\x1b[200~";
const END: &str = "\x1b[201~";

/// 두 모드에서 모두 공백이 되어야 하는 바이트 16종과 이름(paste.zig `strip`)
const STRIPPED: [(u8, &str); 16] = [
    (0x00, "NUL"),
    (0x08, "BS"),
    (0x05, "ENQ"),
    (0x04, "EOT"),
    (0x1B, "ESC"),
    (0x7F, "DEL"),
    (0x03, "VINTR Ctrl+C"),
    (0x1C, "VQUIT Ctrl+\\"),
    (0x15, "VKILL Ctrl+U"),
    (0x1A, "VSUSP Ctrl+Z"),
    (0x11, "VSTART Ctrl+Q"),
    (0x13, "VSTOP Ctrl+S"),
    (0x17, "VWERASE Ctrl+W"),
    (0x16, "VLNEXT Ctrl+V"),
    (0x12, "VREPRINT Ctrl+R"),
    (0x0F, "VDISCARD Ctrl+O"),
];

fn bracketed_bytes(inner: &str) -> Vec<u8> {
    format!("{START}{inner}{END}").into_bytes()
}

// ---- paste.zig `test isSafe` ----

#[test]
fn is_safe() {
    // isSafe는 모드와 상관없는 보수적 규칙이다. 앱에서는 괄호 모드가 꺼진 경우(bracketed=false)가 그 규칙이다
    let safe = |t: &str| paste_is_safe(t, false, true);
    assert!(safe("hello"));
    assert!(safe("hello\tworld"));
    assert!(!safe("hello\n"));
    assert!(!safe("hello\nworld"));
    assert!(!safe("he\x1b[201~llo"));
    // CR은 Enter 키가 보내는 바이트라 LF처럼 명령을 실행한다
    assert!(!safe("hello\r"));
    assert!(!safe("hello\rworld"));
    assert!(!safe("hello\r\nworld"));
    assert!(!safe("\r"));
}

// ---- Surface.zig completeClipboardPaste의 안전 판단(6281~6301줄) ----

#[test]
fn is_safe_bracketed_trusted_allows_line_terminators() {
    // bracketed-safe이면 괄호 모드에서 줄바꿈은 안전하다
    assert!(paste_is_safe("hello", true, true));
    assert!(paste_is_safe("hello\nworld", true, true));
    assert!(paste_is_safe("hello\rworld", true, true));
    assert!(paste_is_safe("hello\r\nworld", true, true));
}

#[test]
fn is_safe_bracketed_end_marker_is_never_trusted() {
    // 괄호 모드에서 글에 끝 표시가 있으면 bracketed-safe여도 안전하지 않다(Surface.zig 6289~6296줄)
    assert!(!paste_is_safe("he\x1b[201~llo", true, true));
    assert!(!paste_is_safe("hello\n\x1b[201~", true, true));
    assert!(!paste_is_safe("\x1b[201~", true, true));
    assert!(!paste_is_safe("he\x1b[201~llo", true, false));
}

#[test]
fn is_safe_bracketed_untrusted_uses_conservative_rule() {
    // bracketed-safe가 꺼져 있으면 괄호 모드여도 보수적 규칙(줄바꿈 금지)을 쓴다
    assert!(paste_is_safe("hello", true, false));
    assert!(!paste_is_safe("hello\nworld", true, false));
    assert!(!paste_is_safe("hello\r", true, false));
}

#[test]
fn is_safe_unbracketed_ignores_bracketed_safe_flag() {
    assert!(paste_is_safe("hello", false, true));
    assert!(!paste_is_safe("hello\nworld", false, true));
    assert!(!paste_is_safe("hello\rworld", false, true));
    assert!(!paste_is_safe("hello\r", false, true));
    // 괄호 모드가 꺼져 있어도 끝 표시는 의심스럽다
    assert!(!paste_is_safe("he\x1b[201~llo", false, true));
    assert!(!paste_is_safe("he\x1b[201~llo", false, false));
}

// ---- paste.zig `encode*` ----

#[test]
fn encode_bracketed() {
    // test "encode bracketed"
    assert_eq!(encode_paste("hello", true), bracketed_bytes("hello"));
}

#[test]
fn encode_unbracketed_no_newlines() {
    // test "encode unbracketed no newlines"
    assert_eq!(encode_paste("hello", false), b"hello");
}

#[test]
fn encode_unbracketed_newlines() {
    // test "encode unbracketed newlines"
    assert_eq!(encode_paste("hello\nworld", false), b"hello\rworld");
}

#[test]
fn encode_unbracketed_windows_style_newline() {
    // test "encode unbracketed windows-stye newline": \r\n은 xterm처럼 \r\r이 된다
    assert_eq!(encode_paste("hello\r\nworld", false), b"hello\r\rworld");
}

#[test]
fn encode_bracketed_keeps_newlines() {
    // 괄호 모드는 줄바꿈을 바꾸지 않는다(틀이 감싸므로)
    assert_eq!(encode_paste("a\nb\r\nc", true), bracketed_bytes("a\nb\r\nc"));
}

#[test]
fn encode_strip_unsafe_bytes_mutable_bracketed() {
    // test "encode strip unsafe bytes mutable bracketed"
    assert_eq!(
        encode_paste("hel\x1blo\x00world", true),
        bracketed_bytes("hel lo world")
    );
}

#[test]
fn encode_strip_unsafe_bytes_mutable_unbracketed() {
    // test "encode strip unsafe bytes mutable unbracketed"
    assert_eq!(encode_paste("hel\x03lo", false), b"hel lo");
}

#[test]
fn encode_strip_multiple_unsafe_bytes() {
    // test "encode strip multiple unsafe bytes"
    assert_eq!(encode_paste("\x00\x08\x7f", true), bracketed_bytes("   "));
}

// ---- paste.zig `encodeWriter *` (한 덩어리로 만드는 쪽에 맞게 옮김) ----

#[test]
fn encode_writer_bracketed() {
    assert_eq!(
        encode_paste("hel\x1blo\nworld", true),
        b"\x1b[200~hel lo\nworld\x1b[201~"
    );
}

#[test]
fn encode_writer_unbracketed() {
    assert_eq!(encode_paste("hel\x00lo\r\nworld", false), b"hel lo\r\rworld");
}

#[test]
fn encode_writer_empty() {
    assert_eq!(encode_paste("", true), b"\x1b[200~\x1b[201~");
    assert_eq!(encode_paste("", false), b"");
}

#[test]
fn encode_writer_multi_line_bracketed() {
    // test "encodeWriter chunks through a small writer buffer"의 글을 한 번에 인코딩
    assert_eq!(
        encode_paste("line one\nline\x1btwo\nline three\n", true),
        b"\x1b[200~line one\nline two\nline three\n\x1b[201~"
    );
}

#[test]
fn max_frame_size() {
    // test max_frame_size: 틀이 더하는 바이트는 시작·끝 표시뿐이다
    let inner = "hello";
    let out = encode_paste(inner, true);
    assert_eq!(out.len(), inner.len() + PASTE_START.len() + PASTE_END.len());
    assert_eq!(PASTE_START.len() + PASTE_END.len(), 12);
}

// ---- 위험 바이트 16종(#565) ----

#[test]
fn every_dangerous_byte_becomes_space_in_both_modes() {
    for (b, name) in STRIPPED {
        let text = format!("a{}b", b as char);
        assert_eq!(encode_paste(&text, false), b"a b", "{name}(0x{b:02x}) 비괄호");
        assert_eq!(
            encode_paste(&text, true),
            bracketed_bytes("a b"),
            "{name}(0x{b:02x}) 괄호"
        );
    }
}

#[test]
fn dangerous_bytes_at_edges_and_repeated() {
    for (b, name) in STRIPPED {
        let c = b as char;
        let text = format!("{c}{c}x{c}");
        assert_eq!(encode_paste(&text, false), b"  x ", "{name} 가장자리·연속");
    }
}

#[test]
fn only_the_sixteen_are_replaced() {
    // 나머지 제어 바이트는 그대로 둔다(xterm·Ghostty와 같음): 탭, CR, LF(비괄호는 CR로), 0x01·0x02 등
    for b in 0u8..=0x7f {
        if STRIPPED.iter().any(|&(s, _)| s == b) || b == b'\n' {
            continue;
        }
        let text = format!("a{}b", b as char);
        assert_eq!(
            encode_paste(&text, true),
            bracketed_bytes(&text),
            "0x{b:02x}는 그대로(괄호)"
        );
        assert_eq!(
            encode_paste(&text, false),
            text.as_bytes(),
            "0x{b:02x}는 그대로(비괄호)"
        );
    }
}

#[test]
fn ctrl_v_hiding_a_byte_cannot_survive() {
    // Ctrl-V(VLNEXT) 뒤 바이트를 글자로 받게 하는 속임수: Ctrl-V가 공백이 되어 다음 바이트가 문자 그대로가 되지 않는다
    assert_eq!(encode_paste("x\x16\x03y", false), b"x  y");
    assert_eq!(encode_paste("x\x16\x03y", true), bracketed_bytes("x  y"));
}

#[test]
fn end_marker_is_broken_by_esc_replacement() {
    // 끝 표시의 ESC가 공백이 되어 표시가 깨진다: 붙여넣기 구간을 빠져나올 수 없다
    let out = encode_paste("echo hi\x1b[201~rm -rf ~\n", true);
    assert_eq!(out, bracketed_bytes("echo hi [201~rm -rf ~\n"));
    let inner = &out[PASTE_START.len()..out.len() - PASTE_END.len()];
    assert!(!inner.windows(PASTE_END.len()).any(|w| w == PASTE_END));
    assert!(!inner.contains(&0x1b));
    // 비괄호에서도 ESC는 공백
    assert_eq!(encode_paste("a\x1b[201~b", false), b"a [201~b");
    // 시작 표시도 마찬가지
    assert_eq!(encode_paste("\x1b[200~x", false), b" [200~x");
}

// ---- 멀티바이트 UTF-8 ----

#[test]
fn multibyte_utf8_is_untouched() {
    // UTF-8 이어짐 바이트는 모두 0x80 이상이라 위험 바이트와 겹치지 않는다
    for text in [
        "한글 붙여넣기",
        "😀 이모지 👨‍👩‍👧",
        "日本語・中文",
        "é ñ ü",
        "\u{7f}\u{80}\u{9f}",
    ] {
        let want: Vec<u8> = text.bytes().map(|b| if b == 0x7f { b' ' } else { b }).collect();
        assert_eq!(encode_paste(text, false), want, "{text:?} 비괄호");
        assert_eq!(
            encode_paste(text, true),
            [START.as_bytes(), &want, END.as_bytes()].concat(),
            "{text:?} 괄호"
        );
    }
}

#[test]
fn multibyte_next_to_dangerous_bytes() {
    assert_eq!(encode_paste("가\x15나\x03😀\x1b다", false), "가 나 😀 다".as_bytes());
    assert_eq!(encode_paste("가\n나\r\n다", false), "가\r나\r\r다".as_bytes());
}

#[test]
fn c1_controls_in_utf8_are_kept() {
    // U+0085·U+009B 같은 C1 글자는 UTF-8로 0xC2 0x85처럼 두 바이트라 그대로 간다(xterm·Ghostty와 같음)
    assert_eq!(encode_paste("\u{85}\u{9b}", false), "\u{85}\u{9b}".as_bytes());
}
