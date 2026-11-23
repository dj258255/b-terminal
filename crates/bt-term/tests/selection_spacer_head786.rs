//! #786: 줄 끝에서 넘어간 넓은 글자 자리(줄 끝의 자리표시 칸, spacer head)에서 끝나는 고르기.
//! 기준: Ghostty 1.4.0 `formatter.zig` 시험 `PageList selection ending on spacer head`(4587줄). 같은 입력(3열 `1A⚡Z`)을 옮겼다.
//! PTY는 `/bin/sh`와 `sleep`만 쓰고 바이트는 `feed_local`로 넣는다.

use std::sync::Arc;

use bt_core::Theme;
use bt_term::{GridSize, SelectKind, Session, SessionEvent, SpawnOptions};

fn mk(cols: u16, rows: u16, input: &str) -> Session {
    let opts = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), "sleep 120".into()],
        cwd: Some(std::env::temp_dir()),
        scrollback: 1000,
        ..Default::default()
    };
    let sink = Arc::new(|_e: SessionEvent| {});
    let s = Session::spawn(opts, GridSize::new(cols, rows), Theme::default(), sink).expect("spawn");
    s.feed_local(input.as_bytes());
    s
}

fn pick(s: &Session, kind: SelectKind, a: (usize, usize), b: (usize, usize)) -> Option<String> {
    s.select_start(a.0, a.1, kind);
    s.select_update(b.0, b.1);
    s.selection_text()
}

/// 윗줄이 없는 자리(0행)와 윗줄이 있는 자리(1행, 윗줄 첫 글자가 섞이던 자리) 둘 다
#[test]
fn selection_ending_on_a_spacer_head_takes_the_wrapped_wide_char() {
    for (prefix, row) in [("", 0usize), ("Q\r\n", 1)] {
        let s = mk(3, 4, &format!("{prefix}1A⚡Z"));
        let ctx = format!("윗줄 {:?}", prefix);
        // 끝 칸(2열)이 줄 끝의 자리표시 칸이다: 다음 줄로 넘어간 ⚡까지 가져온다
        assert_eq!(
            pick(&s, SelectKind::Simple, (0, row), (2, row)).as_deref(),
            Some("1A⚡"),
            "{ctx}"
        );
        // 그 한 칸만 고르면 ⚡
        assert_eq!(
            pick(&s, SelectKind::Simple, (2, row), (2, row)).as_deref(),
            Some("⚡"),
            "{ctx}"
        );
        // 사각 고르기는 끝을 넓히지 않는다
        assert_eq!(
            pick(&s, SelectKind::Block, (0, row), (2, row)).as_deref(),
            Some("1A"),
            "{ctx}"
        );
    }
}

/// 맨 아래 줄 끝의 자리표시 칸은 아래에 줄이 없다. 격자 밖을 읽어 panic이 나던 자리다(debug 빌드에서 panic이 없어야 한다).
/// `1A⚡`는 0행 `1A_`·1행 `⚡`이고, 아래로 한 줄 밀면(SD) 자리표시 칸이 맨 아래 줄(1행)에 남고 ⚡는 화면 밖으로 나간다
#[test]
fn selection_ending_on_a_spacer_head_at_the_bottom_row_does_not_panic() {
    let s = mk(3, 2, "1A⚡\x1b[T");
    assert_eq!(s.snapshot().row_text(1), "1A");
    // 아래 줄이 없으니 넓히지 않는다
    assert_eq!(pick(&s, SelectKind::Simple, (0, 1), (2, 1)).as_deref(), Some("1A"));
    assert_eq!(pick(&s, SelectKind::Block, (0, 1), (2, 1)).as_deref(), Some("1A"));
    // 한 칸만 고르면 덧붙일 글자가 없으니 빈 글이다(고른 것 없음으로 본다)
    assert_eq!(pick(&s, SelectKind::Simple, (2, 1), (2, 1)), None);
}
