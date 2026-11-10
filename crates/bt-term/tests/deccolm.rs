//! DECCOLM(`CSI ? 3 h·l`)과 모드 40(`CSI ? 40 h·l`)(#324).
//! 기준: Ghostty Terminal.zig `deccolm` 4837~4861줄과 그 시험들(18943~19040줄), stream_handler.zig 732~748줄(모드 40을 바꾸면 창 크기로
//! 되돌림, 3은 `deccolm`), modes.zig 376·385줄, termio/Termio.zig `resize` 524~564줄(창 크기가 바뀔 때만 PTY에 알리고 격자를 창에 맞춤).
//! 격자는 `Term`을 직접 만들어 본다. PTY를 띄우는 시험은 `/bin/sh`와 `printf`만 쓴다

use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use bt_core::Theme;
use bt_term::{Emulator, GridSize, Session, SessionEvent, SpawnOptions};

struct Dims(usize, usize);
impl Dimensions for Dims {
    fn total_lines(&self) -> usize {
        self.1
    }
    fn screen_lines(&self) -> usize {
        self.1
    }
    fn columns(&self) -> usize {
        self.0
    }
}

#[derive(Clone, Default)]
struct Writes(Arc<Mutex<String>>);
impl EventListener for Writes {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(s) = event {
            self.0.lock().unwrap().push_str(&s);
        }
    }
}

struct T {
    term: Term<Writes>,
    parser: Processor,
    writes: Writes,
}

impl T {
    fn new(cols: usize, rows: usize) -> Self {
        let writes = Writes::default();
        T {
            term: Term::new(Config::default(), &Dims(cols, rows), writes.clone()),
            parser: Processor::new(),
            writes,
        }
    }
    fn feed(&mut self, s: &str) -> &mut Self {
        self.parser.advance(&mut self.term, s.as_bytes());
        self
    }
    fn size(&self) -> (usize, usize) {
        (self.term.columns(), self.term.screen_lines())
    }
    fn cursor(&self) -> (i32, usize) {
        let p = self.term.grid().cursor.point;
        (p.line.0, p.column.0)
    }
    fn row(&self, line: i32) -> String {
        let row = &self.term.grid()[Line(line)];
        let s: String = (0..self.term.columns()).map(|c| row[Column(c)].c).collect();
        s.trim_end().to_string()
    }
    /// DECRQM의 답 상태 숫자(0 모름, 1 켬, 2 끔)
    fn decrqm(&mut self, mode: u16) -> u8 {
        self.writes.0.lock().unwrap().clear();
        self.feed(&format!("\x1b[?{mode}$p"));
        let s = self.writes.0.lock().unwrap().clone();
        let prefix = format!("\x1b[?{mode};");
        let rest = s
            .strip_prefix(prefix.as_str())
            .unwrap_or_else(|| panic!("DECRQM 답이 이상하다: {s:?}"));
        rest.strip_suffix("$y").expect("$y로 끝난다").parse().unwrap()
    }
}

/// 모드 40이 꺼져 있으면 `CSI ? 3 h`는 아무 일도 하지 않는다(Ghostty "DECCOLM without DEC mode 40"): 열 수·화면·커서 그대로, 3은 끔
#[test]
fn deccolm_is_ignored_without_mode_40() {
    let mut t = T::new(20, 5);
    t.feed("hello\x1b[2;3H");
    t.feed("\x1b[?3h");
    assert_eq!(t.size(), (20, 5));
    assert_eq!(t.row(0), "hello");
    assert_eq!(t.cursor(), (1, 2));
    assert_eq!(t.decrqm(3), 2, "40이 꺼져 있으면 3은 끔으로 답한다");
    t.feed("\x1b[?3l");
    assert_eq!(t.size(), (20, 5));
    assert_eq!(t.row(0), "hello");
}

#[test]
fn mode_40_state_and_decrqm() {
    let mut t = T::new(20, 5);
    assert_eq!(t.decrqm(40), 2);
    assert_eq!(t.decrqm(3), 2);
    t.feed("\x1b[?40h");
    assert_eq!(t.decrqm(40), 1);
    t.feed("\x1b[?40l");
    assert_eq!(t.decrqm(40), 2);
}

/// 40이 켜져 있으면 `CSI ? 3 h`는 격자를 132열로(행은 그대로), 화면을 지우고 커서를 1;1로 보낸다. `l`은 80열
#[test]
fn deccolm_with_mode_40_resizes_clears_and_homes() {
    let mut t = T::new(20, 5);
    t.feed("\x1b[?40h").feed("hello\x1b[3;4H");
    t.feed("\x1b[?3h");
    assert_eq!(t.size(), (132, 5));
    assert_eq!(t.row(0), "");
    assert_eq!(t.cursor(), (0, 0));
    assert_eq!(t.decrqm(3), 1);
    t.feed("x\x1b[?3l");
    assert_eq!(t.size(), (80, 5));
    assert_eq!(t.row(0), "");
    assert_eq!(t.cursor(), (0, 0));
    assert_eq!(t.decrqm(3), 2);
}

/// Ghostty "DECCOLM resets pending wrap"와 "resets scroll region"
#[test]
fn deccolm_resets_pending_wrap_and_scroll_region() {
    let mut t = T::new(5, 5);
    t.feed("\x1b[?40h").feed("\x1b[2;3r").feed("\x1b[1;1HABCDE");
    t.feed("\x1b[?3h");
    assert_eq!(t.size(), (132, 5));
    // 스크롤 영역이 풀렸으면 맨 아래 줄에서 줄바꿈이 화면을 민다
    t.feed("a\x1b[5;1Hz\r\nq");
    assert_eq!(t.row(3), "z", "위 칸으로 밀렸다");
    assert_eq!(t.row(4), "q");
}

/// 40을 바꾸면 창 크기의 격자로 돌아간다(stream_handler.zig 732~748줄: 켜든 끄든, 모드 3은 그대로)
#[test]
fn changing_mode_40_returns_to_window_grid() {
    let mut t = T::new(20, 5);
    t.feed("\x1b[?40h\x1b[?3h");
    assert_eq!(t.size(), (132, 5));
    t.feed("\x1b[?40l");
    assert_eq!(t.size(), (20, 5));
    t.feed("\x1b[?40h");
    assert_eq!(t.size(), (20, 5));
    t.feed("\x1b[?3h");
    assert_eq!(t.size(), (132, 5));
    t.feed("\x1b[?40h");
    assert_eq!(t.size(), (20, 5), "켜진 채 다시 켜도 창 크기로 돌아간다");
}

/// 창 크기가 바뀌면 격자는 창 크기가 된다(termio/Termio.zig `resize`). 그 뒤 40을 바꾸면 바뀐 창 크기로 돌아간다
#[test]
fn window_resize_returns_to_window_grid() {
    let mut t = T::new(20, 5);
    t.feed("\x1b[?40h\x1b[?3h");
    assert_eq!(t.size(), (132, 5));
    t.term.resize(Dims(30, 8));
    assert_eq!(t.size(), (30, 8));
    t.feed("\x1b[?3h");
    assert_eq!(t.size(), (132, 8), "행은 창의 행을 따른다");
    t.term.resize(Dims(30, 9));
    assert_eq!(t.size(), (30, 9));
    t.feed("\x1b[?3h\x1b[?40l");
    assert_eq!(t.size(), (30, 9));
}

/// XTSAVE·XTRESTORE는 3과 40도 저장·복원한다(Ghostty는 `modes.restore`한 값을 `setMode`로 다시 적용)
#[test]
fn xtsave_xtrestore_cover_modes_3_and_40() {
    let mut t = T::new(20, 5);
    t.feed("\x1b[?40h\x1b[?40s\x1b[?40l");
    assert_eq!(t.decrqm(40), 2);
    t.feed("\x1b[?40r");
    assert_eq!(t.decrqm(40), 1);

    t.feed("\x1b[?3h\x1b[?3s\x1b[?3l");
    assert_eq!(t.size(), (80, 5));
    t.feed("\x1b[?3r");
    assert_eq!(t.size(), (132, 5));
    assert_eq!(t.decrqm(3), 1);
}

#[test]
fn emulator_grid_follows_deccolm_and_window() {
    let mut emu = Emulator::new(GridSize::new(20, 5), Theme::default());
    emu.feed(b"\x1b[?40h\x1b[?3hx");
    let s = emu.snapshot();
    assert_eq!((s.cols, s.rows), (132, 5));
    assert_eq!(s.cells.len(), 132 * 5);
    emu.resize(GridSize::new(24, 6));
    let s = emu.snapshot();
    assert_eq!((s.cols, s.rows), (24, 6));
}

/// Ghostty는 deccolm에서 PTY 크기를 알리지 않는다(`terminal.resize`만, `backend.resize`는 창 크기가 바뀔 때만).
/// 격자는 132열이어도 `stty size`는 창의 6 80이다
#[test]
fn pty_size_stays_at_window_size() {
    let (tx, rx) = channel();
    let tx = Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec![
            "-c".into(),
            "printf '\\033[?40h\\033[?3h'; printf 'SZ[%s]\\n' \"$(stty size)\"".into(),
        ],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    let s = Session::spawn(o, GridSize::new(80, 6), Theme::default(), sink).expect("spawn");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert!(Instant::now() < deadline, "10초 안에 끝나지 않았다");
        if let Ok(SessionEvent::Exited(_)) = rx.recv_timeout(Duration::from_millis(100)) {
            break;
        }
    }
    let snap = s.snapshot();
    assert_eq!(snap.cols, 132, "격자는 132열");
    let text = snap.text();
    assert!(text.contains("SZ[6 80]"), "PTY 크기는 창 그대로여야 한다:\n{text}");
}

/// 격자가 창보다 넓을 때 그리는 쪽이 앞 열만 쓴다(`Screen::crop_columns`): 칸 수·커서·끝에 걸친 넓은 글자
#[test]
fn crop_columns_keeps_window_columns_only() {
    let mut emu = Emulator::new(GridSize::new(20, 3), Theme::default());
    emu.feed(b"\x1b[?40h\x1b[?3h");
    // 80번째 칸(0부터 79)에 넓은 글자의 앞 칸이 오게
    let mut line = " ".repeat(79).into_bytes();
    line.extend("한x".as_bytes());
    emu.feed(&line);
    let mut s = emu.snapshot();
    assert_eq!(s.cols, 132);
    s.crop_columns(80);
    assert_eq!((s.cols, s.rows), (80, 3));
    assert_eq!(s.cells.len(), 80 * 3);
    assert_eq!(s.cell(79, 0).ch, ' ', "잘린 자리에 넓은 글자의 반쪽이 남지 않는다");
    assert!(!s.cursor.visible || s.cursor.col < 80);
    // 이미 작으면 그대로
    s.crop_columns(100);
    assert_eq!(s.cols, 80);
}
