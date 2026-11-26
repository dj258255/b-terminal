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

// ---- kitty `paste_actions`(#866): confirm-if-large · quote-urls-at-prompt · replace-newline ----

#[test]
fn large_paste_limit_is_kittys_16kib_in_utf8_bytes() {
    // kitty window.py: `len(btext) > 16 * 1024`. 같으면 크지 않다
    assert!(!paste_is_large(&"a".repeat(LARGE_PASTE_BYTES)));
    assert!(paste_is_large(&"a".repeat(LARGE_PASTE_BYTES + 1)));
    assert!(paste_is_large(&"a".repeat(1024 * 1024)), "실측 1MB 글");
    // 글자 수가 아니라 바이트 수다: 한글은 3바이트
    assert!(paste_is_large(&"가".repeat(LARGE_PASTE_BYTES / 3 + 1)));
    assert!(!paste_is_large(&"가".repeat(LARGE_PASTE_BYTES / 3)));
    assert!(!paste_is_large(""));
}

#[test]
fn quote_url_follows_kitty_rules() {
    // 안전한 글자만 있으면 shlex.quote처럼 그대로 둔다
    assert_eq!(
        quote_url_for_shell("https://example.com/a/b").as_deref(),
        Some("https://example.com/a/b")
    );
    // 셸이 다르게 읽는 글자가 있으면 작은따옴표로 감싼다
    assert_eq!(
        quote_url_for_shell("https://example.com/a?x=1&y=2").as_deref(),
        Some("'https://example.com/a?x=1&y=2'")
    );
    assert_eq!(
        quote_url_for_shell("http://a.com/(b) c").as_deref(),
        Some("'http://a.com/(b) c'")
    );
    // URL 안의 작은따옴표는 닫고 큰따옴표로 넣고 다시 연다(shlex.quote)
    assert_eq!(
        quote_url_for_shell("http://a.com/it's").as_deref(),
        Some("'http://a.com/it'\"'\"'s'")
    );
    // `scheme://`이거나 mailto·irc일 때만 URL이다
    assert_eq!(quote_url_for_shell("mailto:a@b.com").as_deref(), Some("mailto:a@b.com"));
    assert_eq!(quote_url_for_shell("irc:#room").as_deref(), Some("'irc:#room'"));
    assert_eq!(quote_url_for_shell("http:example.com"), None);
    assert_eq!(quote_url_for_shell("javascript://x"), None);
    assert_eq!(quote_url_for_shell("echo https://a.com"), None);
    assert_eq!(
        quote_url_for_shell("HTTP://a.com"),
        None,
        "kitty는 scheme의 대소문자를 가린다"
    );
    assert_eq!(quote_url_for_shell("https:"), None);
    assert_eq!(quote_url_for_shell(""), None);
    assert_eq!(
        quote_url_for_shell("file:///tmp/a b").as_deref(),
        Some("'file:///tmp/a b'")
    );
}

#[test]
fn replace_newlines_makes_every_line_break_a_space() {
    assert_eq!(replace_newlines("a\nb"), "a b");
    assert_eq!(replace_newlines("a\r\nb"), "a b", "CRLF는 하나");
    assert_eq!(replace_newlines("a\rb"), "a b", "CR도 Enter라 바꾼다");
    assert_eq!(replace_newlines("a\n\nb\n"), "a  b ");
    assert_eq!(replace_newlines("한글\n줄"), "한글 줄");
    assert_eq!(replace_newlines("한 줄"), "한 줄");
    // 줄바꿈이 사라지므로 확인 없이 안전하다(kitty: without confirmation)
    assert!(paste_is_safe(&replace_newlines("ls\nrm -rf ~\n"), false, true));
}

#[test]
fn apply_paste_actions_only_acts_when_asked() {
    let url = "https://example.com/?a=1&b=2".to_string();
    assert_eq!(
        apply_paste_actions(url.clone(), false, true, false),
        url,
        "꺼져 있으면 그대로"
    );
    assert_eq!(
        apply_paste_actions(url.clone(), true, false, false),
        url,
        "프롬프트가 아니면 그대로"
    );
    assert_eq!(
        apply_paste_actions(url.clone(), true, true, false),
        "'https://example.com/?a=1&b=2'"
    );
    assert_eq!(apply_paste_actions("a\nb".into(), false, true, true), "a b");
    // 둘 다: URL이 아닌 여러 줄은 줄바꿈만 바뀐다
    assert_eq!(apply_paste_actions("ls\nls".into(), true, true, true), "ls ls");
}

/// 적대적인 글 모음: 위험 바이트·끝 표시·작은따옴표·줄바꿈을 URL 꼴 앞뒤에 섞은 것
fn hostile_texts() -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for (b, _) in STRIPPED {
        v.push(format!("http://a.com/{}x", b as char));
        v.push(format!("{}http://a.com/x", b as char));
        v.push(format!("mailto:{}a@b", b as char));
    }
    for body in [
        "http://a.com/\x1b[201~rm -rf ~\n",
        "https://a.com/'\x1b[201~'; rm -rf ~; echo '",
        "http://a.com/\x1b[200~x\x1b[201~\x03\x15",
        "ssh://h/\n\x1b[201~\nreboot\r\n",
        "irc:\x1b[201~",
        "file:///a'b\x03c\x1b d\nrm -rf ~",
        "http://a.com/\u{1b}\u{1b}[201~\u{1b}[201~",
    ] {
        v.push(body.to_string());
    }
    v
}

/// 최종 바이트에 위험 바이트가 남지 않았나. 괄호 모드면 처음과 끝 표시만 ESC를 가질 수 있다
fn assert_no_dangerous_bytes(out: &[u8], bracketed: bool, ctx: &str) {
    let inner = if bracketed {
        assert!(out.starts_with(START.as_bytes()), "{ctx}: 시작 표시");
        assert!(out.ends_with(END.as_bytes()), "{ctx}: 끝 표시");
        &out[START.len()..out.len() - END.len()]
    } else {
        out
    };
    for &b in inner {
        assert!(
            !STRIPPED.iter().any(|(s, _)| *s == b),
            "{ctx}: 위험 바이트 0x{b:02x}이 남음: {out:?}"
        );
    }
    assert!(
        !inner.windows(END.len()).any(|w| w == END.as_bytes()),
        "{ctx}: 끝 표시가 안에 남음"
    );
    if !bracketed {
        assert!(!out.contains(&b'\n'), "{ctx}: LF가 남음");
    }
}

#[test]
fn paste_actions_never_leave_dangerous_bytes_in_final_output() {
    for text in hostile_texts() {
        for quote in [false, true] {
            for replace_nl in [false, true] {
                for bracketed in [false, true] {
                    let ctx = format!("quote={quote} nl={replace_nl} bracketed={bracketed} {text:?}");
                    // 동작을 먼저 적용하고 인코딩한다(앱의 순서)
                    let acted = apply_paste_actions(text.clone(), quote, true, replace_nl);
                    assert_no_dangerous_bytes(&encode_paste(&acted, bracketed), bracketed, &ctx);
                    // 인코딩을 먼저 하고 동작을 뒤에 하는 순서로 바꿔도(방어 깊이) 위험 바이트가 없어야 한다.
                    // 인코딩이 이미 공백으로 바꾼 글에 동작을 적용해도 새 위험 바이트를 만들지 않는다
                    let enc = String::from_utf8(encode_paste(&text, false)).unwrap();
                    let acted2 = apply_paste_actions(enc, quote, true, replace_nl);
                    assert_no_dangerous_bytes(&encode_paste(&acted2, bracketed), bracketed, &ctx);
                }
            }
        }
    }
}

#[test]
fn quoted_url_keeps_a_single_quote_from_breaking_out_of_the_shell_word() {
    // 실제 /bin/sh와 printf로 확인한다: 따옴표를 빠져나가 명령이 돌면 마커 파일이나 다른 출력이 생긴다
    let dir = std::env::temp_dir().join(format!("bt-quote-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let marker = dir.join("pwned");
    let urls = [
        "http://a.com/x'; touch MARK; echo '".to_string(),
        "http://a.com/x\"; touch MARK; echo \"".to_string(),
        "http://a.com/$(touch MARK)`touch MARK`".to_string(),
        "http://a.com/x';touch MARK;'".to_string(),
        "https://a.com/a b&c|d>e<f;g(h)*?[i]#j~k\\l!m".to_string(),
        "ssh://h/'''".to_string(),
        "http://a.com/it's".to_string(),
    ];
    for url in urls {
        let quoted = quote_url_for_shell(&url).expect("URL이다");
        let script = format!("printf '%s' {}", quoted.replace("MARK", marker.to_str().unwrap()));
        let want = url.replace("MARK", marker.to_str().unwrap());
        let out = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&script)
            .current_dir(&dir)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout),
            want,
            "한 낱말로 읽혀야 한다: {quoted}"
        );
        assert!(!marker.exists(), "명령이 실행됐다: {quoted}");
        assert!(out.status.success(), "{quoted}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
