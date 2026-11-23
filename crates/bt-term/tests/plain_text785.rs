//! #785: 글(plain) 뽑기를 Ghostty 1.4.0 `formatter.zig`처럼. 시험은 `formatter.zig`의 시험을 같은 입력·같은 기대로 옮긴 것이다
//! (시험 이름 위에 원본 이름과 줄 번호). 앱 길은 복사(`selection_text` + `clipboard-trim-trailing-spaces`)와 화면 파일(`plain_text`)이다.
//! 둘 다 `unwrap = true`다(`Surface.zig` 2338·5924줄). 복사는 `trim`이 설정값이고 파일은 `trim = false`다.
//! PTY는 `/bin/sh`와 `sleep`만 쓰고 바이트는 `feed_local`로 넣는다.

use std::sync::Arc;

use bt_core::Theme;
use bt_term::{GridSize, SelectKind, Session, SessionEvent, SpawnOptions, trim_trailing_spaces};

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

/// 앱의 select_all(`SelectKind::Line`, 왼쪽 위에서 오른쪽 아래까지)
fn select_all(s: &Session, cols: u16, rows: u16) {
    s.select_start(0, 0, SelectKind::Line);
    s.select_update(cols as usize - 1, rows as usize - 1);
}

fn range(s: &Session, a: (usize, usize), b: (usize, usize), block: bool) {
    s.select_start(a.0, a.1, if block { SelectKind::Block } else { SelectKind::Simple });
    s.select_update(b.0, b.1);
}

/// 앱의 복사 길: 고른 글에 `clipboard-trim-trailing-spaces`가 켜져 있으면 줄 끝 공백을 지운다
fn copied(s: &Session, trim: bool) -> String {
    let t = s.selection_text().unwrap_or_default();
    if trim { trim_trailing_spaces(&t) } else { t }
}

/// 화면 전체를 골라 복사한 글과 화면 파일(`write_screen_file:plain`)을 함께 견준다
fn whole(cols: u16, rows: u16, input: &str, trim: bool, copy: &str, file: &str) {
    let s = mk(cols, rows, input);
    select_all(&s, cols, rows);
    assert_eq!(copied(&s, trim), copy, "복사 {input:?}");
    assert_eq!(s.plain_text(false), file, "화면 파일 {input:?}");
}

#[allow(clippy::too_many_arguments)]
fn rng(cols: u16, rows: u16, input: &str, a: (usize, usize), b: (usize, usize), block: bool, trim: bool, want: &str) {
    let s = mk(cols, rows, input);
    range(&s, a, b, block);
    assert_eq!(copied(&s, trim), want, "{input:?} {a:?}~{b:?}");
}

// "Page plain single line"(2350줄)·"PageList plain single line"·"Screen plain single line"
#[test]
fn single_line_has_no_trailing_newlines() {
    whole(80, 24, "hello, world", true, "hello, world", "hello, world");
}

// "Page plain single line soft-wrapped unwrapped"
#[test]
fn soft_wrapped_line_is_joined() {
    whole(3, 5, "hello!", true, "hello!", "hello!");
}

// "Page plain single wide char"·"Page plain wide char soft-wrapped unwrapped"(spacer head)
#[test]
fn wide_chars_and_spacer_heads() {
    whole(80, 24, "1A⚡", true, "1A⚡", "1A⚡");
    rng(80, 24, "1A⚡", (2, 0), (79, 23), false, true, "⚡");
    rng(80, 24, "1A⚡", (3, 0), (79, 23), false, true, "⚡");
    whole(3, 24, "1A⚡", true, "1A⚡", "1A⚡");
    rng(3, 24, "1A⚡", (2, 0), (2, 23), false, true, "⚡");
    rng(3, 24, "1A⚡", (1, 1), (2, 23), false, true, "⚡");
}

// "Page plain multiline"·"Screen plain multiline"·"TerminalFormatter plain no selection"
#[test]
fn multiline() {
    whole(80, 24, "hello\r\nworld", true, "hello\nworld", "hello\nworld");
}

// "Page plain multiline rectangle"
#[test]
fn multiline_rectangle() {
    rng(80, 24, "hello\r\nworld", (1, 0), (3, 1), true, true, "ell\norl");
}

// "Page plain multi blank lines": 가운데 빈 줄은 남는다
#[test]
fn blank_lines_in_the_middle_are_kept() {
    whole(
        80,
        24,
        "hello\r\n\r\n\r\nworld",
        true,
        "hello\n\n\nworld",
        "hello\n\n\nworld",
    );
}

// "Page plain trailing blank lines": 끝의 빈 줄은 줄바꿈으로 내지 않는다
#[test]
fn trailing_blank_lines_are_dropped() {
    whole(80, 24, "hello\r\nworld\r\n\r\n", true, "hello\nworld", "hello\nworld");
}

// "Page plain trailing whitespace"(복사는 trim, 파일은 trim=false라 끝 공백이 남는다)
#[test]
fn typed_trailing_spaces_follow_the_trim_setting() {
    whole(
        80,
        24,
        "hello   \r\nworld   ",
        true,
        "hello\nworld",
        "hello   \nworld   ",
    );
}

// "Page plain trailing whitespace no trim"(2924줄)
#[test]
fn typed_trailing_spaces_are_kept_without_trim() {
    whole(
        80,
        24,
        "hello   \r\nworld  ",
        false,
        "hello   \nworld  ",
        "hello   \nworld  ",
    );
}

// "Page plain soft-wrapped with unwrap"·"...3 lines with unwrap"
#[test]
fn soft_wrapped_lines_are_joined_in_copy_and_file() {
    whole(10, 24, "hello world test", true, "hello world test", "hello world test");
    whole(
        10,
        24,
        "hello world this is a test",
        true,
        "hello world this is a test",
        "hello world this is a test",
    );
}

// "Page plain start_y subset"·"Page plain start_x on first row only"·"PageList plain with start_x only"
#[test]
fn ranges_into_the_blank_rest_of_the_screen() {
    rng(
        80,
        24,
        "hello\r\nworld\r\ntest",
        (0, 1),
        (79, 23),
        false,
        true,
        "world\ntest",
    );
    rng(80, 24, "hello world", (6, 0), (79, 23), false, true, "world");
}

// "Page plain single line with styling"
#[test]
fn styling_does_not_show_in_plain() {
    whole(
        80,
        24,
        "hello, \x1b[1mworld\x1b[0m",
        true,
        "hello, world",
        "hello, world",
    );
}

// "PageList soft-wrapped spanning pages with unwrap"의 스크롤백 판: 접힌 줄이 스크롤백에 걸쳐도 잇는다
#[test]
fn scrollback_file_joins_a_wrapped_line_across_scrollback() {
    let s = mk(10, 3, "hello world test\r\nx\r\ny\r\nz");
    assert_eq!(s.plain_text(true), "hello world test\nx\ny\nz");
}

// #785 실측: `clear; printf 'hello, world\n'` 뒤 select_all은 프롬프트 줄 뒤에 빈 줄을 21개 더 붙였다
#[test]
fn select_all_after_clear_has_no_trailing_blank_lines() {
    let s = mk(80, 24, "\x1b[2J\x1b[Hhello, world\r\n$ ");
    select_all(&s, 80, 24);
    assert_eq!(copied(&s, true), "hello, world\n$");
    assert_eq!(copied(&s, false), "hello, world\n$ ", "trim을 끄면 친 공백이 남는다");
    assert_eq!(s.plain_text(false), "hello, world\n$ ");
}

/// 지우기·새 줄이 만든 칸은 "쓴 공백"이 아니다: 줄 끝 `ESC[K`로 지운 칸과 건너뛴 칸은 빈 칸이라 끝에서 버린다
#[test]
fn erased_and_skipped_cells_are_not_written_spaces() {
    // `ab` 뒤 칸을 ESC[K로 지움, 다음 줄은 커서를 옮겨(CUF) 건너뜀
    whole(10, 3, "abcde\rab\x1b[K\r\n\x1b[3Cxy", true, "ab\n   xy", "ab\n   xy");
}

/// 앞 칸을 건너뛴 칸은 글자 앞에서 공백으로 나온다(Ghostty 빈 칸 규칙). 끝에서는 버린다
#[test]
fn cells_before_text_become_spaces_and_after_text_are_dropped() {
    whole(10, 2, "a\x1b[2Cb\x1b[3C", true, "a  b", "a  b");
}

/// 탭은 이번 범위 밖이다. 지금처럼 탭 글자를 남기고 이동한 칸은 내지 않는다(Ghostty는 공백으로 낸다)
#[test]
fn tabs_are_left_as_they_were() {
    let s = mk(20, 2, "a\tb\t");
    range(&s, (0, 0), (19, 0), false);
    assert_eq!(s.selection_text().as_deref(), Some("a\tb\t"));
}

/// 사각 고르기도 끝 공백을 줄마다 남긴다(trim=false). 칸에서 쓴 공백만
#[test]
fn block_selection_keeps_written_spaces_without_trim() {
    rng(80, 24, "ab  cd\r\nef  gh", (0, 0), (3, 1), true, false, "ab  \nef  ");
    rng(80, 24, "ab  cd\r\nef  gh", (0, 0), (3, 1), true, true, "ab\nef");
}

/// 글 없는 영역만 고르면 복사할 글이 없다
#[test]
fn blank_selection_has_no_text() {
    let s = mk(10, 3, "ab");
    range(&s, (0, 1), (9, 2), false);
    assert_eq!(s.selection_text(), None);
}
