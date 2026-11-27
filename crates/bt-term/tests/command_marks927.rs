//! 명령 표시(#927): 셸 통합(OSC 133 A·B·C·D)이 알린 명령의 결과가 프롬프트 줄에 붙는다.
//! PTY 없이 `Emulator`에 바이트를 먹여 확인한다(셸을 띄우지 않는다). 격자 칸과 커서는 표시와 상관없이 같아야 한다

use bt_core::Theme;
use bt_term::command_marks::MarkState;
use bt_term::{Emulator, GridSize, Screen};
use std::time::Duration;

fn emu(cols: u16, rows: u16) -> Emulator {
    Emulator::new(GridSize::new(cols, rows), Theme::default())
}

/// 프롬프트 한 번: A, 프롬프트 글, B, 명령 글, C, 출력, D(종료 코드). 끝나면 다음 프롬프트 직전이다
fn command(cmd: &str, out: &str, code: Option<i32>) -> String {
    let d = match code {
        Some(c) => format!("\x1b]133;D;{c}\x07"),
        None => "\x1b]133;D\x07".to_string(),
    };
    format!("\x1b]133;A\x07$ \x1b]133;B\x07{cmd}\r\n\x1b]133;C\x07{out}\r\n{d}")
}

fn done(code: i32) -> MarkState {
    MarkState::Done {
        exit_code: Some(code),
        duration: Duration::ZERO,
    }
}

/// 상태에서 걸린 시간만 지운다(시험이 시계를 재지 않는다)
fn plain(state: MarkState) -> MarkState {
    match state {
        MarkState::Done { exit_code, .. } => MarkState::Done {
            exit_code,
            duration: Duration::ZERO,
        },
        other => other,
    }
}

fn marks(s: &Screen) -> Vec<(usize, MarkState)> {
    s.command_marks.iter().map(|m| (m.row, plain(m.state))).collect()
}

#[test]
fn without_shell_integration_nothing_is_marked() {
    let mut e = emu(20, 6);
    e.feed(b"hello\r\nworld\r\n");
    assert!(e.snapshot().command_marks.is_empty());
}

#[test]
fn each_finished_command_marks_its_prompt_row_with_its_exit_code() {
    let mut e = emu(20, 10);
    e.feed(command("true", "", Some(0)).as_bytes());
    e.feed(command("false", "out1\r\nout2", Some(1)).as_bytes());
    e.feed(command("ls", "x", Some(0)).as_bytes());
    let s = e.snapshot();
    // 줄: 0 프롬프트(true) 1 빈 출력 2 D 뒤 새 줄... 프롬프트마다 자리를 따라간다
    let rows: Vec<usize> = (0..s.rows).filter(|r| s.row_text(*r).starts_with("$ ")).collect();
    assert_eq!(
        rows.len(),
        3,
        "{:?}",
        (0..s.rows).map(|r| s.row_text(r)).collect::<Vec<_>>()
    );
    assert_eq!(
        marks(&s),
        vec![(rows[0], done(0)), (rows[1], done(1)), (rows[2], done(0))]
    );
}

#[test]
fn a_command_without_exit_code_is_done_but_unknown() {
    let mut e = emu(20, 6);
    e.feed(command("x", "", None).as_bytes());
    assert_eq!(
        marks(&e.snapshot()),
        vec![(
            0,
            MarkState::Done {
                exit_code: None,
                duration: Duration::ZERO
            }
        )]
    );
}

#[test]
fn a_running_command_is_marked_running_and_a_bare_prompt_as_prompt() {
    let mut e = emu(20, 8);
    e.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07sleep 9\r\n\x1b]133;C\x07");
    assert_eq!(marks(&e.snapshot()), vec![(0, MarkState::Running)]);
    let mut e = emu(20, 8);
    e.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07");
    assert_eq!(marks(&e.snapshot()), vec![(0, MarkState::Prompt)]);
}

#[test]
fn a_redrawn_prompt_on_the_same_row_is_one_mark() {
    let mut e = emu(20, 8);
    e.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07");
    e.feed(b"\r\x1b]133;A\x07$ \x1b]133;B\x07");
    assert_eq!(e.snapshot().command_marks.len(), 1);
}

#[test]
fn marks_follow_their_rows_into_the_scrollback_and_back() {
    let mut e = emu(20, 5);
    for i in 0..8 {
        e.feed(command(&format!("c{i}"), "", Some(i)).as_bytes());
    }
    e.feed(b"\x1b]133;A\x07$ ");
    let bottom = e.snapshot();
    // 맨 아래: 화면 5줄에 마지막 프롬프트들만 보인다
    let visible_bottom = marks(&bottom);
    // 위로 올려 보면 더 오래된 명령의 종료 코드가 보이고, 줄은 스크롤한 만큼 내려간다
    e.scroll(4);
    let up = e.snapshot();
    assert!(up.display_offset > 0);
    for m in &up.command_marks {
        assert!(up.row_text(m.row).starts_with("$ c"), "{}", up.row_text(m.row));
    }
    // 줄 글에 적힌 명령 번호와 종료 코드가 같다(cN → 종료 코드 N)
    for m in &up.command_marks {
        let n: i32 = up.row_text(m.row)[3..].trim().parse().unwrap();
        assert_eq!(plain(m.state), done(n), "{}", up.row_text(m.row));
    }
    assert!(!up.command_marks.is_empty());
    e.scroll(-4);
    assert_eq!(marks(&e.snapshot()), visible_bottom);
}

#[test]
fn marks_stay_correct_after_old_rows_fall_off_a_full_scrollback() {
    // 스크롤백 상한(기본 1만 줄)을 넘겨도 줄 번호가 밀리지 않는다
    let mut e = emu(20, 5);
    for i in 0..4000 {
        e.feed(command(&format!("c{i}"), "a\r\nb", Some(i % 256)).as_bytes());
    }
    let s = e.snapshot();
    assert!(s.history >= 10_000, "스크롤백이 가득 찼다: {}", s.history);
    e.scroll(3);
    let s = e.snapshot();
    assert!(!s.command_marks.is_empty());
    for m in &s.command_marks {
        let text = s.row_text(m.row);
        let n: i32 = text[3..].trim().parse().unwrap();
        assert_eq!(plain(m.state), done(n % 256), "{text}");
    }
}

#[test]
fn the_alternate_screen_shows_no_marks_and_the_main_screen_gets_them_back() {
    let mut e = emu(20, 6);
    e.feed(command("false", "", Some(1)).as_bytes());
    assert_eq!(e.snapshot().command_marks.len(), 1);
    e.feed(b"\x1b[?1049h");
    assert!(e.snapshot().command_marks.is_empty());
    e.feed(b"\x1b[?1049l");
    assert_eq!(e.snapshot().command_marks.len(), 1);
}

#[test]
fn marking_does_not_change_cells_or_the_cursor() {
    let mut with = emu(20, 8);
    let mut without = emu(20, 8);
    with.feed(command("false", "boom", Some(1)).as_bytes());
    // 같은 글, 셸 통합 표지 없음
    without.feed(b"$ false\r\nboom\r\n");
    let (a, b) = (with.snapshot(), without.snapshot());
    assert_eq!(a.cells, b.cells);
    assert_eq!((a.cursor.col, a.cursor.row), (b.cursor.col, b.cursor.row));
    assert!(!a.command_marks.is_empty() && b.command_marks.is_empty());
}

#[test]
fn resizing_keeps_marks_on_their_prompt_rows() {
    let mut e = emu(20, 8);
    e.feed(command("false", "boom", Some(1)).as_bytes());
    e.feed(command("true", "", Some(0)).as_bytes());
    e.resize(GridSize::new(10, 8));
    let s = e.snapshot();
    let got = marks(&s);
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got.iter().map(|m| m.1).collect::<Vec<_>>(), vec![done(1), done(0)]);
    for (row, _) in got {
        assert!(s.row_text(row).starts_with("$ "), "{}", s.row_text(row));
    }
}

#[test]
fn clear_screen_forgets_marks_of_the_cleared_rows() {
    let mut e = emu(20, 8);
    e.feed(command("false", "", Some(1)).as_bytes());
    e.clear_screen();
    assert!(e.snapshot().command_marks.is_empty());
}

#[test]
fn clearing_the_last_command_removes_only_its_mark() {
    let mut e = emu(20, 12);
    e.feed(command("false", "", Some(1)).as_bytes());
    e.feed(command("true", "", Some(0)).as_bytes());
    e.feed(b"\x1b]133;A\x07$ \x1b]133;B\x07");
    assert_eq!(e.snapshot().command_marks.len(), 3);
    assert!(e.clear_last_command());
    let got: Vec<MarkState> = marks(&e.snapshot()).into_iter().map(|m| m.1).collect();
    // 지운 명령(true)과 그 뒤 현재 프롬프트의 기억은 버리고, 앞 명령(false)은 남는다
    assert_eq!(got, vec![done(1)]);
}

#[test]
fn marks_follow_their_prompt_rows_when_narrowing_rewraps_long_lines() {
    // 긴 출력 줄이 좁아지며 여러 줄로 접히면 프롬프트 줄의 번호가 밀린다. 기억이 따라가야 한다
    let mut e = emu(40, 6);
    for i in 0..3 {
        e.feed(command(&format!("c{i}"), &"x".repeat(35), Some(i)).as_bytes());
    }
    e.feed(b"\x1b]133;A\x07$ ");
    e.resize(GridSize::new(8, 6));
    e.scroll(100);
    let s = e.snapshot();
    let got = marks(&s);
    assert!(!got.is_empty());
    for (row, state) in got {
        let text = s.row_text(row);
        if let Some(n) = text.strip_prefix("$ c").and_then(|t| t.trim().parse::<i32>().ok()) {
            assert_eq!(state, done(n), "{text}");
        }
    }
}
