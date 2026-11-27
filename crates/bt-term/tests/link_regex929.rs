//! #929: 사용자 정규식 링크(kitty `detect_url_regex`). 엔진에 바이트를 먹이는 시험(PTY 없음).
//! kitty `screen.c`의 `detect_url_with_regex`: 보통 URL이 없을 때만, 앞에서부터, 마우스 칸에 겹치는 맞음, 접힌 줄은 위아래 3줄까지.

use bt_core::Theme;
use bt_term::link_regex::UserLinkRegexes;
use bt_term::links::{LinkOptions, link_at_with};
use bt_term::{Emulator, GridSize};
use std::time::{Duration, Instant};

fn emu(cols: u16, rows: u16, bytes: &[u8]) -> Emulator {
    let mut e = Emulator::new(GridSize::new(cols, rows), Theme::default());
    e.feed(bytes);
    e
}

fn user(p: &[&str]) -> UserLinkRegexes {
    let v: Vec<String> = p.iter().map(|s| s.to_string()).collect();
    let (u, w) = UserLinkRegexes::new(&v);
    assert!(w.is_empty(), "{w:?}");
    u
}

fn hit(e: &Emulator, u: &UserLinkRegexes, c: usize, r: usize) -> Option<bt_term::links::LinkHit> {
    e.link_probe_with(c, r, LinkOptions::default(), Some(u)).hit
}

#[test]
fn matches_under_mouse_only() {
    let e = emu(30, 3, b"see JIRA-123 and more");
    let u = user(&["JIRA-[0-9]+"]);
    let h = hit(&e, &u, 6, 0).unwrap();
    assert_eq!(h.url, "JIRA-123");
    assert!(!h.explicit);
    assert_eq!((h.spans[0].row, h.spans[0].start, h.spans[0].end), (0, 4, 12));
    assert!(hit(&e, &u, 1, 0).is_none(), "마우스가 맞음 밖");
    assert!(hit(&e, &u, 12, 0).is_none(), "끝 칸 다음");
    // 정규식이 없으면 옛 길과 같다
    assert!(e.link_probe_with(6, 0, LinkOptions::default(), None).hit.is_none());
}

#[test]
fn built_in_url_wins_and_osc8_wins() {
    let mut e = emu(40, 3, b"https://example.com/JIRA-1 x\r\n");
    let u = user(&["JIRA-[0-9]+", "https://[a-z.]+"]);
    let h = hit(&e, &u, 22, 0).unwrap();
    assert_eq!(
        h.url, "https://example.com/JIRA-1",
        "보통 URL이 있으면 정규식은 보지 않는다"
    );
    e.feed(b"\x1b]8;;https://osc8.example/\x07JIRA-5\x1b]8;;\x07\r\n");
    let h = hit(&e, &u, 2, 1).unwrap();
    assert!(h.explicit && h.url == "https://osc8.example/");
}

#[test]
fn first_matching_expression_in_order_wins() {
    let e = emu(30, 2, b"abc-12");
    let u = user(&["[a-z]+-", "[a-z]+-[0-9]+"]);
    assert_eq!(hit(&e, &u, 0, 0).unwrap().url, "abc-", "식 순서대로 시도한다");
    assert_eq!(
        hit(&e, &u, 5, 0).unwrap().url,
        "abc-12",
        "첫 식이 마우스 칸에 안 닿으면 다음 식"
    );
}

#[test]
fn respects_link_url_switch() {
    let e = emu(30, 2, b"JIRA-1");
    let u = user(&["JIRA-[0-9]+"]);
    let off = LinkOptions { url: false, osc8: true };
    assert!(
        e.link_probe_with(2, 0, off, Some(&u)).hit.is_none(),
        "link-url = false면 정규식도 끈다"
    );
}

#[test]
fn soft_wrapped_text_is_one_line_up_to_three_rows_each_side() {
    // 폭 8칸으로 접힌 글
    let e = emu(8, 8, b"xxJIRA-12345678 yy");
    let u = user(&["JIRA-[0-9]+"]);
    let a = hit(&e, &u, 3, 0).unwrap();
    assert_eq!(a.url, "JIRA-12345678");
    assert_eq!(a.spans.len(), 2, "두 줄에 걸친 구간");
    let b = hit(&e, &u, 3, 1).unwrap();
    assert_eq!(a, b);
    // 마우스 위·아래 3줄까지만 본다: 7줄을 걸친 맞음은 마우스에서 3줄 안 끝만 이어진다
    let long = format!("{}{}", "a".repeat(8), "b".repeat(8 * 6));
    let e = emu(8, 10, long.as_bytes());
    let u = user(&["a+b+"]);
    let h = hit(&e, &u, 0, 0).unwrap();
    assert!(h.url.len() <= 8 * 4, "위·아래 3줄 안만 이어 붙인다: {}", h.url.len());
}

#[test]
fn wide_chars_give_cell_spans() {
    let e = emu(30, 2, "한글 JIRA-12".as_bytes());
    let u = user(&["JIRA-[0-9]+"]);
    let h = hit(&e, &u, 6, 0).unwrap();
    assert_eq!(h.url, "JIRA-12");
    assert_eq!((h.spans[0].start, h.spans[0].end), (5, 12), "한·글이 4칸");
}

#[test]
fn empty_matches_and_bad_patterns_are_ignored() {
    let e = emu(30, 2, b"abc");
    let u = user(&["x*"]);
    assert!(hit(&e, &u, 1, 0).is_none(), "빈 맞음은 링크가 아니다");
    let v = vec!["(bad".to_string(), "abc".to_string()];
    let (u, w) = UserLinkRegexes::new(&v);
    assert_eq!(w.len(), 1);
    assert!(w[0].contains("(bad"));
    assert_eq!(hit(&e, &u, 1, 0).unwrap().url, "abc");
}

#[test]
fn catastrophic_pattern_stays_fast_on_a_long_line() {
    let line = "a".repeat(5000);
    let e = emu(200, 30, line.as_bytes());
    let u = user(&["(a*)*b", "(a|aa)+$"]);
    let t = Instant::now();
    for c in (0..200).step_by(20) {
        let _ = hit(&e, &u, c, 5);
    }
    assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
}

#[test]
fn without_user_regex_the_probe_is_unchanged() {
    let mut e = emu(30, 3, b"go https://x.y/z JIRA-1\r\n");
    let snap = e.snapshot();
    for c in 0..30 {
        assert_eq!(
            e.link_probe_with(c, 0, LinkOptions::default(), None).hit,
            link_at_with(&snap, c, 0, LinkOptions::default())
        );
    }
}
