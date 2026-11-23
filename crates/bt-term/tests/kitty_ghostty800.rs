//! kitty 그림 셋(#801 그림 뒤 커서 이동, #799 화면 지우기·쫓아내기, #800 스크롤 영역)을 Ghostty 1.4.0과 견준다.
//! 바이트는 `Emulator::feed`(세션과 같은 스캐너·명령 처리·VT 엔진)로 먹이고, 화면에 보이는 그림(`Screen::images`)과 답으로 확인한다.
//!
//! 옮긴 Ghostty 시험(`src/terminal/kitty/`, 1.4.0):
//! - #801 `graphics_exec.zig`: `kittygfx placement bounds cursor movement for untrusted dimensions`(1938),
//!   `kittygfx placement moves cursor past a tall image`(1960), `display`의 `.after`(403)
//!
//! Ghostty 시험은 핀(`trackPin`)으로 자리를 잡고 `Terminal.index`·`scrollUp` 같은 함수를 부르지만, 여기서는 커서를 옮겨 그림을 놓고
//! 같은 일을 하는 바이트(IND `ESC D`, RI `ESC M`, SU `CSI S`, IL·DL)를 보낸다. 줄 번호는 보이는 화면(0부터) 기준이다.

use bt_core::Theme;
use bt_term::{Emulator, GridSize};

fn emu(cols: u16, rows: u16, cw: u16, ch: u16) -> Emulator {
    Emulator::new(
        GridSize {
            cols,
            rows,
            cell_width: cw,
            cell_height: ch,
        },
        Theme::default(),
    )
}

fn send(e: &mut Emulator, apc_body: &str) -> String {
    e.feed(format!("\x1b_G{apc_body}\x1b\\").as_bytes());
    String::from_utf8_lossy(&e.take_replies()).into_owned()
}

// ───────────── #801: 그림 뒤 커서 이동 ─────────────

/// `kittygfx placement bounds cursor movement for untrusted dimensions`(graphics_exec.zig 1938):
/// 5x5 화면에 c·r이 u32 끝이어도 아래로 4줄 + 화면 한 높이(5줄)만 내려 스크롤백은 5줄이다(전체 10줄)
#[test]
fn cursor_movement_after_a_huge_placement_is_bounded_by_the_screen() {
    let mut e = emu(5, 5, 10, 20);
    let r = send(&mut e, "a=T,t=d,f=24,i=1,s=1,v=1,c=4294967295,r=4294967295;AAAA");
    assert_eq!(r, "\x1b_Gi=1;OK\x1b\\");
    assert_eq!(e.snapshot().history, 5, "Ghostty: 전체 10줄 = 스크롤백 5");
}

/// 같은 시험, 스크롤 영역 안: 영역 맨 아래까지 남은 줄 + 화면 높이. 영역 위가 맨 위가 아니면 스크롤백은 생기지 않는다
#[test]
fn cursor_movement_bound_counts_rows_left_in_the_scroll_region() {
    let mut e = emu(5, 5, 10, 20);
    e.feed(b"\x1b[2;3r\x1b[2;1H"); // 영역 2~3줄, 커서 (1,0)
    send(&mut e, "a=T,t=d,f=24,i=1,s=1,v=1,c=1,r=4294967295;AAAA");
    let s = e.snapshot();
    assert_eq!(s.history, 0, "맨 위가 아닌 영역의 스크롤은 스크롤백을 만들지 않는다");
    assert_eq!(
        (s.cursor.col, s.cursor.row),
        (1, 2),
        "영역 맨 아래 줄에 멈추고 그림 오른쪽 칸"
    );
}

/// 영역 밖(아래 여백 줄 아래)의 커서는 영역을 스크롤하지 않고 화면 맨 아래에서 멈춘다(Ghostty `index`)
#[test]
fn cursor_movement_outside_the_scroll_region_stops_at_the_screen_bottom() {
    let mut e = emu(5, 5, 10, 20);
    e.feed(b"\x1b[1;2r\x1b[4;1H");
    send(&mut e, "a=T,t=d,f=24,i=1,s=1,v=1,c=1,r=4294967295;AAAA");
    let s = e.snapshot();
    assert_eq!(s.history, 0);
    assert_eq!((s.cursor.col, s.cursor.row), (1, 4));
}

/// `kittygfx placement moves cursor past a tall image`(graphics_exec.zig 1960): 8줄 그림 다음 자리는 8줄 아래에서 시작한다
#[test]
fn a_placement_after_a_tall_image_starts_eight_rows_below() {
    let mut e = emu(5, 5, 10, 20);
    send(&mut e, "a=t,t=d,f=24,i=1,s=1,v=1;////");
    assert_eq!(send(&mut e, "a=p,i=1,p=1,c=5,r=8"), "\x1b_Gi=1,p=1;OK\x1b\\");
    assert_eq!(send(&mut e, "a=p,i=1,p=2,C=1"), "\x1b_Gi=1,p=2;OK\x1b\\");
    let s = e.snapshot();
    let mut ys: Vec<i64> = s.images.iter().map(|i| s.history as i64 + i64::from(i.row)).collect();
    ys.sort();
    assert_eq!(ys, vec![0, 8], "첫 자리는 줄 0, 둘째는 8줄 아래");
}

/// `display`의 `.after`(403): 그림이 오른쪽 끝에 닿으면(`x + c >= 열 수`) 한 줄 더 내려 열 0으로 감긴다. 아니면 그림 오른쪽 칸
#[test]
fn cursor_wraps_to_column_zero_when_the_placement_reaches_the_right_edge() {
    let mut e = emu(5, 5, 10, 20);
    send(&mut e, "a=T,t=d,f=24,i=1,s=1,v=1,c=5,r=1;AAAA");
    let c = e.snapshot().cursor;
    assert_eq!((c.col, c.row), (0, 1), "Ghostty: (0,1)");

    let mut e = emu(5, 5, 10, 20);
    send(&mut e, "a=T,t=d,f=24,i=1,s=1,v=1,c=3,r=1;AAAA");
    let c = e.snapshot().cursor;
    assert_eq!((c.col, c.row), (3, 0));

    // 오른쪽 끝에 닿은 줄바꿈 대기 상태도 푼다: 다음 글자는 0열에 찍힌다
    let mut e = emu(5, 5, 10, 20);
    send(&mut e, "a=T,t=d,f=24,i=1,s=1,v=1,c=5,r=1;AAAA");
    e.feed(b"x");
    let s = e.snapshot();
    assert_eq!(s.cell(0, 1).ch, 'x');
}

/// 줄 이동은 줄바꿈 모드(LNM)와 상관없이 index다: 칸은 `target_x`로 가고 0열로 돌아가지 않는다
#[test]
fn the_row_move_is_an_index_not_a_newline() {
    let mut e = emu(8, 5, 10, 20);
    e.feed(b"\x1b[20h"); // LNM: LF가 CR도 한다
    send(&mut e, "a=T,t=d,f=24,i=1,s=1,v=1,c=2,r=3;AAAA");
    let c = e.snapshot().cursor;
    assert_eq!((c.col, c.row), (2, 2));
}

// ───────────── 세션(PTY, 읽기 스레드) 길 ─────────────
// 위 시험은 `Emulator::feed`로 같은 처리를 본다. 앱이 쓰는 읽기 스레드도 같은 결과를 내는지 `/bin/sh`와 `printf`로 본다
// (사용자 셸 설정이 결과를 바꾸지 않게 한다)

mod pty {
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use bt_core::Theme;
    use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

    fn run(cols: u16, rows: u16, script: String) -> Session {
        let o = SpawnOptions {
            program: Some("/bin/sh".into()),
            args: vec!["-c".into(), format!("{script}; sleep 5"), "sh".into()],
            cwd: Some(std::env::temp_dir()),
            ..Default::default()
        };
        let size = GridSize {
            cols,
            rows,
            cell_width: 10,
            cell_height: 10,
        };
        let sink = Arc::new(|_: SessionEvent| {});
        let s = Session::spawn(o, size, Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"));
        let deadline = Instant::now() + Duration::from_secs(60);
        while !s.snapshot().text().contains("READY") {
            assert!(Instant::now() < deadline, "출력이 끝나지 않았다");
            std::thread::sleep(Duration::from_millis(5));
        }
        s
    }

    /// #801: 읽기 스레드 길에서도 줄 수 상한이 걸린다(5x5 화면, 스크롤백 5줄)
    #[test]
    fn session_bounds_the_cursor_movement_after_a_huge_placement() {
        let s = run(
            5,
            5,
            "printf '\\033_Ga=T,t=d,f=24,i=1,s=1,v=1,c=4294967295,r=4294967295,q=1;AAAA\\033\\\\'; printf READY".into(),
        );
        assert_eq!(s.snapshot().history, 5);
    }
}
