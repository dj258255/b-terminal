//! 픽셀 스크롤(kitty `pixel_scroll`, #902)의 화면 값: 걸친 픽셀(`pixel_shift`)과 화면 맨 아래에 걸쳐 보이는 다음 줄(`edge`).
//! 셀 크기는 8×16, 화면은 10열 3줄이다

use bt_core::Theme;
use bt_term::{CellFlags, Emulator, GridSize};

fn emu_with_lines(n: usize) -> Emulator {
    let mut e = Emulator::new(GridSize::new(10, 3), Theme::default());
    for i in 0..n {
        e.feed(format!("line{i}\r\n").as_bytes());
    }
    e
}

fn text(cells: &[bt_term::Cell]) -> String {
    cells
        .iter()
        .filter(|c| !c.flags.contains(CellFlags::SPACER))
        .map(|c| c.ch)
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// 맨 아래에서 21픽셀(줄 높이 16 + 5) 올리면 두 줄 위에 11픽셀이 걸친다: 화면은 `line6 line7 line8`이고 맨 아래에 `line9`가 11픽셀 보인다
#[test]
fn a_move_of_a_line_and_a_bit_shows_the_next_line_at_the_bottom() {
    let mut e = emu_with_lines(10);
    assert!(e.scroll_pixels(21.0));
    let s = e.snapshot();
    assert_eq!(s.display_offset, 2);
    assert_eq!(s.pixel_shift, 11);
    assert_eq!(s.row_text(0), "line6");
    assert_eq!(s.row_text(2), "line8");
    assert_eq!(s.edge.len(), s.cols);
    assert_eq!(text(&s.edge), "line9");
    assert_eq!(e.pixel_shift(), 11);
}

/// 줄 경계에 서면 걸친 픽셀이 없고 다음 줄도 담지 않는다. 맨 아래도 같다
#[test]
fn on_a_line_boundary_or_at_the_bottom_nothing_hangs() {
    let mut e = emu_with_lines(10);
    let before = e.snapshot();
    assert_eq!((before.pixel_shift, before.edge.len()), (0, 0));
    assert!(e.scroll_pixels(32.0));
    let s = e.snapshot();
    assert_eq!((s.display_offset, s.pixel_shift, s.edge.len()), (2, 0, 0));
    // 되돌리면 맨 아래의 처음 화면과 칸이 같다
    assert!(e.scroll_pixels(-32.0));
    let back = e.snapshot();
    assert_eq!((back.display_offset, back.pixel_shift, back.edge.len()), (0, 0, 0));
    assert_eq!(back.cells, before.cells);
}

/// 줄 단위 스크롤은 걸친 픽셀을 지운다
#[test]
fn a_line_scroll_clears_the_shift() {
    let mut e = emu_with_lines(10);
    e.scroll_pixels(21.0);
    assert_eq!(e.snapshot().pixel_shift, 11);
    e.scroll(1);
    let s = e.snapshot();
    assert_eq!((s.display_offset, s.pixel_shift, s.edge.len()), (3, 0, 0));
    // 같은 줄로 되돌아와도 걸친 픽셀은 돌아오지 않는다
    e.scroll(-1);
    let s = e.snapshot();
    assert_eq!((s.display_offset, s.pixel_shift, s.edge.len()), (2, 0, 0));
}

/// 새 출력이 와도 보던 자리를 지키고(줄이 한 줄 올라가도 같은 내용) 걸친 픽셀이 남는다
#[test]
fn new_output_keeps_the_place_and_the_shift() {
    let mut e = emu_with_lines(10);
    e.scroll_pixels(21.0);
    e.feed(b"more\r\n");
    let s = e.snapshot();
    assert_eq!((s.display_offset, s.pixel_shift), (3, 11));
    assert_eq!(s.row_text(0), "line6");
    assert_eq!(text(&s.edge), "line9");
}

/// 커서가 걸쳐 보이는 줄에 있으면 그 줄(`row == rows`)에서 보인다. 한 줄 안(걸친 줄이 아닌 줄)이면 지금처럼이다
#[test]
fn the_cursor_in_the_hanging_line_is_visible_there() {
    let mut e = emu_with_lines(10);
    // 5픽셀: 한 줄 위, 11픽셀 걸침. 다음 줄은 커서가 있는 맨 아래 줄이다
    e.scroll_pixels(5.0);
    let s = e.snapshot();
    assert_eq!((s.display_offset, s.pixel_shift), (1, 11));
    assert_eq!((s.cursor.row, s.cursor.visible), (s.rows, true));
    // 맨 아래로 돌아오면 줄에 맞추고 커서는 그 줄 안에 있다
    e.scroll_pixels(-5.0);
    let s = e.snapshot();
    assert_eq!((s.pixel_shift, s.cursor.row, s.cursor.visible), (0, 2, true));
    // 더 올리면 커서가 화면 밖이다
    e.scroll_pixels(40.0);
    let s = e.snapshot();
    assert!(!s.cursor.visible);
}

/// 찾기에 맞은 곳은 걸친 줄에도 표시된다
#[test]
fn a_search_match_in_the_hanging_line_is_marked() {
    let mut e = emu_with_lines(10);
    e.search("line9");
    e.scroll_pixels(21.0);
    let s = e.snapshot();
    assert!(
        s.edge.iter().take(5).all(|c| c.flags.contains(CellFlags::MATCH)),
        "{:?}",
        s.edge
    );
    assert!(s.cells.iter().all(|c| !c.flags.contains(CellFlags::MATCH)));
}

/// 대체 화면에는 스크롤백이 없어 아무것도 하지 않는다(kitty와 같다)
#[test]
fn the_alt_screen_does_not_pixel_scroll() {
    let mut e = emu_with_lines(10);
    e.feed(b"\x1b[?1049h");
    assert!(!e.scroll_pixels(21.0));
    let s = e.snapshot();
    assert_eq!((s.pixel_shift, s.edge.len()), (0, 0));
}

/// 스크롤백이 없으면 움직이지 않는다
#[test]
fn without_history_nothing_moves() {
    let mut e = emu_with_lines(1);
    assert!(!e.scroll_pixels(21.0));
    assert_eq!(e.snapshot().pixel_shift, 0);
}

// ---- 실제 PTY(/bin/sh와 printf만): 새 출력과 키 입력 ----

mod pty {
    use std::sync::Arc;
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};

    use bt_core::Theme;
    use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

    fn spawn(script: &str) -> Session {
        let (tx, _rx) = channel();
        let tx = std::sync::Mutex::new(tx);
        let sink = Arc::new(move |e: SessionEvent| {
            let _ = tx.lock().unwrap().send(e);
        });
        let opts = SpawnOptions {
            program: Some("/bin/sh".into()),
            args: vec!["-c".into(), script.into()],
            cwd: Some(std::env::temp_dir()),
            ..Default::default()
        };
        Session::spawn(opts, GridSize::new(40, 5), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"))
    }

    fn wait(s: &Session, mut pred: impl FnMut(&bt_term::Screen) -> bool) -> bt_term::Screen {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let screen = s.snapshot();
            if pred(&screen) {
                return screen;
            }
            assert!(
                Instant::now() < deadline,
                "화면이 기대대로 되지 않았다:\n{}",
                screen.text()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    const SCRIPT: &str = "i=0; while [ $i -lt 30 ]; do printf 'row%s\\n' $i; i=$((i+1)); done; \
                          read x; printf 'tail1\\ntail2\\ntail3\\n'; read y";

    /// 올려 본 뒤 새 출력이 와도 보던 줄과 걸친 픽셀이 그대로이고, 키 입력(맨 아래로)이면 걸친 픽셀 없이 줄에 맞는다
    #[test]
    fn new_output_keeps_the_shift_and_a_key_snaps_to_the_bottom() {
        let s = spawn(SCRIPT);
        wait(&s, |t| t.text().contains("row29"));
        // 셀 높이는 기본 16픽셀: 두 줄 + 5픽셀
        assert!(s.scroll_pixels(37.0));
        let before = s.snapshot();
        assert_eq!((before.display_offset, before.pixel_shift), (3, 11));
        let top = before.row_text(0);
        assert_eq!(s.pixel_shift(), 11);
        // 새 출력: 스크롤 자리를 바꾸지 않는 쓰기로 read를 푼다
        s.write(b"\n");
        let after = wait(&s, |t| t.display_offset > before.display_offset);
        assert_eq!(after.pixel_shift, 11, "새 출력이 와도 걸친 픽셀이 남는다");
        assert_eq!(after.row_text(0), top, "보던 줄이 그대로다");
        // 키 입력은 맨 아래로: 걸친 픽셀 없이 줄에 맞는다
        s.write_input(b"x", true);
        let bottom = s.snapshot();
        assert_eq!(
            (bottom.display_offset, bottom.pixel_shift, bottom.edge.len()),
            (0, 0, 0)
        );
        assert_eq!(s.pixel_shift(), 0);
        s.write(b"\n");
    }
}
