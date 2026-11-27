//! #929: 사용자가 쓴 정규식(kitty `detect_url_regex`·마커 `regex`)의 상한과 설정 키.

use bt_core::Config;
use bt_core::user_regex::{MAX_PATTERN_BYTES, compile};
use std::time::{Duration, Instant};

#[test]
fn compiles_plain_and_ignore_case() {
    let r = compile("err(or)?", false).unwrap();
    assert!(r.is_match("an error"));
    assert!(!r.is_match("an ERROR"));
    let r = compile("err(or)?", true).unwrap();
    assert!(r.is_match("an ERROR"));
}

#[test]
fn rejects_empty_syntax_error_backref_and_lookaround() {
    assert!(compile("", false).is_err(), "빈 정규식");
    assert!(compile("(unclosed", false).is_err());
    assert!(compile(r"(a)\1", false).is_err(), "역참조는 없다(선형 시간을 지키려고)");
    assert!(compile("a(?=b)", false).is_err(), "앞 보기는 없다");
    assert!(compile("(?<=a)b", false).is_err(), "뒤 보기는 없다");
}

#[test]
fn rejects_too_long_and_too_big_patterns() {
    let long = "a".repeat(MAX_PATTERN_BYTES + 1);
    assert!(compile(&long, false).unwrap_err().contains("너무 깁니다"), "너무 긴 글");
    // 반복을 겹쳐 컴파일 결과가 상한을 넘는 꼴
    let e = compile("(?:(?:(?:a{100}){100}){100})", false).unwrap_err();
    assert!(e.contains("너무 큽니다"), "{e}");
    // 깊이 겹친 괄호
    let deep = format!("{}a{}", "(".repeat(500), ")".repeat(500));
    assert!(compile(&deep, false).is_err());
}

#[test]
fn catastrophic_pattern_runs_in_linear_time() {
    let r = compile("(a*)*b", false).unwrap();
    let text = "a".repeat(200_000);
    let t = Instant::now();
    assert!(!r.is_match(&text));
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
}

#[test]
fn config_reads_link_url_regex_and_drops_bad_ones() {
    let d = Config::default();
    assert!(d.link_url_regex.is_empty());
    let c = Config::from_text(
        "link-url-regex = JIRA-[0-9]+\nlink-url-regex = (unclosed\nlink-url-regex = [[:alnum:]_./-]+\\.[[:alnum:]]+:[0-9]+\n",
        true,
    );
    assert_eq!(
        c.link_url_regex,
        vec![
            "JIRA-[0-9]+".to_string(),
            "[[:alnum:]_./-]+\\.[[:alnum:]]+:[0-9]+".to_string()
        ]
    );
    assert!(
        c.warnings
            .iter()
            .any(|w| w.contains("link-url-regex") && w.contains("(unclosed")),
        "{:?}",
        c.warnings
    );
    // 빈 값은 지금까지 쌓은 것을 비운다(font-family처럼)
    let c = Config::from_text("link-url-regex = a+\nlink-url-regex =\nlink-url-regex = b+\n", true);
    assert_eq!(c.link_url_regex, vec!["b+".to_string()]);
    // 개수 상한
    let many: String = (0..40).map(|i| format!("link-url-regex = x{i}\n")).collect();
    let c = Config::from_text(&many, true);
    assert_eq!(c.link_url_regex.len(), bt_core::user_regex::MAX_PATTERNS);
    assert!(c.warnings.iter().any(|w| w.contains("link-url-regex")));
}

#[test]
fn config_reads_mark_colors_like_kitty_defaults() {
    let d = Config::default().theme.marks;
    // kitty `mark1_*`~`mark3_*` 기본
    assert_eq!(
        d.pick(1),
        (bt_core::Rgb::new(0, 0, 0), bt_core::Rgb::new(0x98, 0xd3, 0xcb))
    );
    assert_eq!(
        d.pick(2),
        (bt_core::Rgb::new(0, 0, 0), bt_core::Rgb::new(0xf2, 0xdc, 0xd3))
    );
    assert_eq!(
        d.pick(3),
        (bt_core::Rgb::new(0, 0, 0), bt_core::Rgb::new(0xf2, 0x74, 0xbc))
    );
    let c = Config::from_text(
        "mark1-foreground = red\nmark2-background = #102030\nmark3-foreground = nope\n",
        true,
    );
    assert_eq!(c.theme.marks.pick(1).0, bt_core::Rgb::new(255, 0, 0));
    assert_eq!(c.theme.marks.pick(2).1, bt_core::Rgb::new(0x10, 0x20, 0x30));
    assert_eq!(c.theme.marks.pick(3), d.pick(3));
    assert!(c.warnings.iter().any(|w| w.contains("mark3-foreground")));
}
