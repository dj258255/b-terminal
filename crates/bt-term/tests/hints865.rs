//! #865: 힌트 찾기를 진짜 VT 엔진이 만든 화면(접힌 줄 표시, 넓은 글자의 앞·뒤 칸, OSC 8 링크 번호)에서 확인한다.
//! 손으로 만든 화면(단위 시험)은 엔진이 실제로 칸에 적는 표시와 같다고 가정한 것이라 따로 본다.
//! PTY는 `/bin/sh`와 `printf`만 쓰고, 글이 칸에 나타날 때까지 기다린다.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::hints::{HintKind, find_marks};
use bt_term::{GridSize, Screen, Session, SessionEvent, SpawnOptions};

/// `printf` 서식(작은따옴표 안)을 찍고 입력을 기다리는 셸. 화면에 `ready`가 보일 때까지 기다려 스냅샷을 돌려준다
fn screen_of(cols: u16, rows: u16, printf_format: &str) -> (Session, Screen) {
    let opts = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), format!("printf '{printf_format}READY'; read x")],
        cwd: Some(std::env::temp_dir()),
        scrollback: 100,
        ..Default::default()
    };
    let sink = Arc::new(|_e: SessionEvent| {});
    let s = Session::spawn(opts, GridSize::new(cols, rows), Theme::default(), sink).expect("spawn");
    let end = Instant::now() + Duration::from_secs(10);
    loop {
        let snap = s.snapshot();
        if (0..snap.rows).any(|r| snap.row_text(r).contains("READY")) {
            return (s, snap);
        }
        assert!(Instant::now() < end, "화면에 READY가 나타나지 않았다");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn url_that_the_engine_soft_wrapped_is_one_mark_with_a_span_per_row() {
    // 20칸 화면에 26자 주소: 엔진이 줄 끝 칸에 접힘 표시를 단다
    let (_s, snap) = screen_of(20, 6, "go https://a.b/cdefghijklmnop end\\r\\n");
    let m = find_marks(&snap, HintKind::Url);
    assert_eq!(m.len(), 1, "{m:?}");
    assert_eq!(m[0].text, "https://a.b/cdefghijklmnop");
    assert_eq!(m[0].spans.len(), 2, "접힌 두 줄에 걸친다: {:?}", m[0].spans);
    assert_eq!((m[0].spans[0].row, m[0].spans[0].start, m[0].spans[0].end), (0, 3, 20));
    assert_eq!((m[0].spans[1].row, m[0].spans[1].start), (1, 0));
}

#[test]
fn wide_hangul_before_a_mark_shifts_its_columns_by_two_per_char() {
    let (_s, snap) = screen_of(40, 6, "한글 https://a.b/c 끝 word1\\r\\n");
    let url = find_marks(&snap, HintKind::Url);
    // 한 글 = 4칸, 공백 1칸 → 5칸에서 시작
    assert_eq!(
        (url[0].spans[0].start, url[0].spans[0].end),
        (5, 5 + "https://a.b/c".len())
    );
    let words = find_marks(&snap, HintKind::Word);
    let w: Vec<&str> = words.iter().map(|m| m.text.as_str()).collect();
    // `:`는 낱말 글자가 아니라(kitty `select_by_word_characters`) 주소가 둘로 갈린다. '한글'과 '끝'은 세 글자 미만
    assert_eq!(w, ["https", "//a.b/c", "word1", "READY"]);
    // 낱말 구간은 칸 단위다: `word1`은 "한글 https://a.b/c 끝 " 뒤
    let word1 = &words[2];
    let before = 4 + 1 + "https://a.b/c".len() + 1 + 2 + 1;
    assert_eq!(word1.spans[0].start, before);
}

#[test]
fn osc8_hyperlink_marks_come_from_the_engines_link_table() {
    let (_s, snap) = screen_of(
        40,
        6,
        "a \\033]8;;https://h.example/p\\033\\\\here\\033]8;;\\033\\\\ b \\033]8;;file:///tmp/q\\033\\\\there\\033]8;;\\033\\\\\\r\\n",
    );
    let m = find_marks(&snap, HintKind::Hyperlink);
    let t: Vec<&str> = m.iter().map(|m| m.text.as_str()).collect();
    assert_eq!(t, ["https://h.example/p", "file:///tmp/q"]);
    assert_eq!((m[0].spans[0].start, m[0].spans[0].end), (2, 6));
}

#[test]
fn path_and_linenum_marks_on_a_real_screen() {
    let (_s, snap) = screen_of(60, 6, "error: src/lib.rs:9:1: boom at /tmp/a.txt\\r\\n");
    let p = find_marks(&snap, HintKind::Path);
    let t: Vec<&str> = p.iter().map(|m| m.text.as_str()).collect();
    assert_eq!(t, ["src/lib.rs", "/tmp/a.txt"]);
    let l = find_marks(&snap, HintKind::Linenum);
    assert_eq!(l.len(), 1);
    assert_eq!((l[0].path.as_deref(), l[0].line), (Some("src/lib.rs"), Some(9)));
}
