//! 상태 되돌리기 셋(#530 DECSTR, #531 DECSC의 원점 모드, #532 DECST8C).
//! 기준: Ghostty 35188fe `Terminal.zig`의 `softReset`(5918줄)·`saveCursor`(2703줄)·`restoreCursor`(2719줄)·`tabReset`(3001줄)과 그 시험들.
//! DECSTR(`CSI ! p`)은 해석기가 아니라 bt-term 스캐너 신호가 처리하므로, 실제 길(`Emulator::feed`·세션 읽기 스레드)처럼
//! 스캐너를 지나게 먹인다. PTY를 띄우는 시험은 `/bin/sh`와 `printf`만 쓴다

use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{CharsetIndex, Color, NamedColor, Processor, StandardCharset};
use bt_core::Theme;
use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

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

/// 터미널이 PTY에 쓰는 답(DECRQM 등)을 모은다
#[derive(Clone, Default)]
struct Quiet {
    out: Arc<Mutex<String>>,
}
impl EventListener for Quiet {
    fn send_event(&self, event: Event) {
        if let Event::PtyWrite(s) = event {
            self.out.lock().unwrap().push_str(&s);
        }
    }
}

struct T {
    term: Term<Quiet>,
    parser: Processor,
    scanner: bt_term::notify::Scanner,
    replies: Arc<Mutex<String>>,
}

impl T {
    fn new(cols: usize, rows: usize) -> Self {
        let quiet = Quiet::default();
        let replies = quiet.out.clone();
        T {
            term: Term::new(Config::default(), &Dims(cols, rows), quiet),
            parser: Processor::new(),
            scanner: Default::default(),
            replies,
        }
    }
    /// 실제 길처럼 DECSTR은 스캐너 신호를 거쳐 `kitty_color::soft_reset`이 처리한다
    fn feed(&mut self, s: &str) -> &mut Self {
        let bytes = s.as_bytes();
        let mut pos = 0;
        for (end, sig) in self.scanner.feed_at(bytes) {
            let end = end.min(bytes.len());
            self.parser.advance(&mut self.term, &bytes[pos..end]);
            pos = end;
            if let bt_term::Signal::SoftReset = sig {
                bt_term::kitty_color::soft_reset(&mut self.term);
            }
        }
        self.parser.advance(&mut self.term, &bytes[pos..]);
        self
    }
    /// 줄마다 글(너비 2 글자의 뒤 칸은 건너뜀, 줄 끝 공백 없음), 끝의 빈 줄은 뺌
    fn plain(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        for r in 0..self.term.screen_lines() {
            let mut s = String::new();
            for c in 0..self.term.columns() {
                let cell = &self.term.grid()[Line(r as i32)][Column(c)];
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                s.push(if cell.c == '\t' { ' ' } else { cell.c });
            }
            lines.push(s.trim_end().to_string());
        }
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines.join("\n")
    }
    /// (열, 줄) 0부터
    fn cursor(&self) -> (usize, usize) {
        let p = self.term.grid().cursor.point;
        (p.column.0, p.line.0 as usize)
    }
    /// DECRQM으로 묻는다: 0 모름, 1 켬, 2 끔
    fn decrqm(&mut self, private: bool, n: u16) -> u8 {
        self.replies.lock().unwrap().clear();
        let q = if private {
            format!("\x1b[?{n}$p")
        } else {
            format!("\x1b[{n}$p")
        };
        self.feed(&q);
        let r = self.replies.lock().unwrap().clone();
        let prefix = if private {
            format!("\x1b[?{n};")
        } else {
            format!("\x1b[{n};")
        };
        let v = r
            .strip_prefix(prefix.as_str())
            .and_then(|s| s.strip_suffix("$y"))
            .unwrap_or_else(|| panic!("DECRQM 답이 이상하다: {r:?}"));
        v.parse().unwrap()
    }
    fn on(&mut self, private: bool, n: u16) -> bool {
        self.decrqm(private, n) == 1
    }
    fn template(&self) -> &alacritty_terminal::term::cell::Cell {
        &self.term.grid().cursor.template
    }
}

// ---- #531: DECSC가 원점 모드를 저장 ----

#[test]
fn save_cursor_keeps_origin_mode() {
    // 이슈 재현 바이트: DECSC 뒤 원점 모드를 끄고 DECRC하면 저장할 때의 값(켬)으로 돌아온다
    let mut t = T::new(10, 6);
    t.feed("\x1b[2;4r\x1b[?6h\x1b[3;4H\x1b7\x1b[?6l\x1b8");
    assert!(t.on(true, 6));
    assert_eq!(t.cursor(), (3, 3));
}

#[test]
fn save_cursor_with_origin_off_turns_origin_off_on_restore() {
    let mut t = T::new(10, 6);
    t.feed("\x1b[2;4r\x1b[3;4H\x1b7\x1b[?6h\x1b8");
    assert!(!t.on(true, 6));
}

#[test]
fn restore_cursor_without_a_save_resets_origin_and_goes_home() {
    // Ghostty `restoreCursor`: 저장한 것이 없으면 원점 모드를 끄고 처음 자리, 기본 스타일로
    let mut t = T::new(10, 6);
    t.feed("\x1b[?6h\x1b[2;4r\x1b[3;3H\x1b[1m\x1b8");
    assert!(!t.on(true, 6));
    assert_eq!(t.cursor(), (0, 0));
    assert!(!t.template().flags.contains(Flags::BOLD));
}

#[test]
fn save_cursor_origin_mode_ghostty_test() {
    // Ghostty "Terminal: saveCursor origin mode"(15189줄): 원점 켠 채 저장, 여백을 바꾸고 복원하면 저장한 절대 자리(0,0)에 X
    let mut t = T::new(10, 5);
    t.feed("\x1b[?6h\x1b7\x1b[?69h\x1b[3;5s\x1b[2;4r\x1b8X");
    assert_eq!(t.plain(), "X");
    assert!(t.on(true, 6));
}

#[test]
fn save_cursor_is_per_screen() {
    // 화면마다 저장한 커서가 따로다(Ghostty `screens.active.saved_cursor`): 대체 화면의 저장이 원점 모드를 끌고 가지 않는다
    let mut t = T::new(10, 6);
    t.feed("\x1b[?47h\x1b[?6h\x1b7\x1b[?6l\x1b8");
    assert!(t.on(true, 6));
}

// ---- #545: DECSC가 SO·SI로 고른 집합(GL)과 SS2·SS3도 저장 ----

#[test]
fn save_cursor_keeps_the_shifted_charset() {
    // Ghostty `saveCursor`의 `charset`(GL): G1을 선 그리기로 두고 SO로 쓰는 중에 저장, SI로 돌린 뒤 복원하면 다시 G1
    let mut t = T::new(10, 3);
    t.feed("\x1b)0\x0e\x1b7\x0f\x1b8q");
    assert_eq!(t.plain(), "─");
}

#[test]
fn restore_cursor_undoes_a_shift_made_after_the_save() {
    let mut t = T::new(10, 3);
    t.feed("\x1b)0\x1b7\x0e\x1b8q");
    assert_eq!(t.plain(), "q");
}

#[test]
fn restore_cursor_puts_back_the_single_shift_state() {
    // 저장할 때 대기 중인 SS2가 없었으면 복원이 저장 뒤의 SS2를 지운다(Ghostty `charset.single_shift`)
    let mut t = T::new(10, 3);
    t.feed("\x1b*0\x1b7\x1bN\x1b8qq");
    assert_eq!(t.plain(), "qq");
    // 저장할 때 대기 중이었으면 복원 뒤 다음 한 글자에 쓴다
    let mut t = T::new(10, 3);
    t.feed("\x1b*0\x1bN\x1b7\x1b8qq");
    assert_eq!(t.plain(), "─q");
}

#[test]
fn restore_cursor_without_a_save_resets_the_shift() {
    // 저장한 것이 없으면 처음 상태(`.charset = .{}`): GL은 G0. 복원 뒤 G1을 다시 선 그리기로 지정해도 G0로 찍는다
    let mut t = T::new(10, 3);
    t.feed("\x1b)0\x0e\x1b8\x1b)0q");
    assert_eq!(t.plain(), "q");
}

#[test]
fn alternate_screen_1049_restores_the_shift_on_exit() {
    // 1049는 들어갈 때 저장하고 나올 때 복원한다: 대체 화면에서 SO로 바꾼 채 나와도 주 화면은 저장할 때의 GL
    let mut t = T::new(10, 3);
    t.feed("\x1b)0\x1b[?1049h\x0e\x1b[?1049lq");
    assert_eq!(t.plain(), "q");
}

#[test]
fn alternate_screen_1049_restores_origin_mode_on_exit() {
    // Ghostty 1049: 켤 때 `saveCursor`, 끌 때 `restoreCursor`라 대체 화면에서 바꾼 원점 모드도 저장할 때의 값으로
    let mut t = T::new(10, 6);
    t.feed("\x1b[?6h\x1b[?1049h\x1b[?6l\x1b[?1049l");
    assert!(t.on(true, 6));
    let mut t = T::new(10, 6);
    t.feed("\x1b[?1049h\x1b[?6h\x1b[?1049l");
    assert!(!t.on(true, 6));
}

// ---- #530: 소프트 리셋 ----

#[test]
fn soft_reset_modes() {
    // Ghostty "Terminal: softReset modes"(Terminal.zig 18506줄): 삽입·원점·자동 줄바꿈·역방향 줄바꿈·커서 보임·커서키·키패드를
    // 기본값으로 되돌리고, 괄호 붙여넣기처럼 DECSTR이 다루지 않는 모드는 그대로 둔다
    let mut t = T::new(10, 10);
    t.feed("\x1b[4h\x1b[?6h\x1b[?7l\x1b[?45h\x1b[?25l\x1b[?1h\x1b[?66h\x1b[?2004h");
    assert!(t.on(false, 4) && t.on(true, 6) && !t.on(true, 7) && t.on(true, 45));
    assert!(!t.on(true, 25) && t.on(true, 1) && t.on(true, 66));
    t.feed("\x1b[!p");
    assert!(!t.on(false, 4), "insert");
    assert!(!t.on(true, 6), "origin");
    assert!(t.on(true, 7), "wraparound");
    assert!(!t.on(true, 45), "reverse_wrap");
    assert!(t.on(true, 25), "cursor_visible");
    assert!(!t.on(true, 1), "cursor_keys");
    assert!(!t.on(true, 66), "keypad_keys");
    // DECSTR이 다루지 않는 모드는 그대로
    assert!(t.on(true, 2004), "bracketed_paste");
}

#[test]
fn soft_reset_leaves_mouse_and_other_modes_alone() {
    let mut t = T::new(10, 10);
    t.feed("\x1b[?1000h\x1b[?1006h\x1b[?1004h\x1b[?2004h\x1b[!p");
    for n in [1000, 1006, 1004, 2004] {
        assert!(t.on(true, n), "?{n}은 그대로여야 한다");
    }
}

#[test]
fn soft_reset_issue_530_repro_bytes() {
    // 이슈 재현 바이트
    let mut t = T::new(10, 10);
    t.feed("\x1b[4h\x1b[?6h\x1b[?7l\x1b[?25l\x1b[?1h\x1b=\x1b[1m\x1b(0\x1b7\x1b[!p");
    assert!(!t.on(false, 4));
    assert!(!t.on(true, 6));
    assert!(t.on(true, 7));
    assert!(t.on(true, 25));
    assert!(!t.on(true, 1));
    assert!(!t.on(true, 66), "ESC =(응용 키패드)도 되돌린다");
    // 굵기와 선 그리기 문자 집합이 없어졌다: 글자가 그대로 찍힌다
    t.feed("\x1b[1;1Hq");
    assert_eq!(t.plain(), "q");
    assert!(!t.template().flags.contains(Flags::BOLD));
    assert_eq!(t.term.grid().cursor.charsets[CharsetIndex::G0], StandardCharset::Ascii);
    // 저장한 커서도 지웠다: DECRC는 처음 자리로
    t.feed("\x1b[5;5H\x1b8");
    assert_eq!(t.cursor(), (0, 0));
}

#[test]
fn soft_reset_margins() {
    // Ghostty "Terminal: softReset margins"(18540줄 부근): 69와 위아래·좌우 여백을 처음으로
    let mut t = T::new(10, 10);
    t.feed("\x1b[3;4r\x1b[?69h\x1b[5;6s");
    assert!(t.on(true, 69));
    t.feed("\x1b[!p");
    assert!(!t.on(true, 69));
    assert_eq!((t.term.left_margin(), t.term.right_margin()), (0, 9));
    assert_eq!(*t.term.scroll_region(), Line(0)..Line(10));
    // 위아래 여백이 풀렸는지 글로도 본다: 맨 아랫줄에서 줄바꿈하면 화면이 스크롤된다
    t.feed("\x1b[1;1HA\x1b[10;1H\nB");
    assert_eq!(t.plain().lines().next(), Some(""), "첫 줄 A가 스크롤로 올라갔다");
}

#[test]
fn soft_reset_keeps_the_cursor_and_screen() {
    // Ghostty "Terminal: softReset keeps the cursor and screen"(18550줄)
    let mut t = T::new(10, 10);
    t.feed("hello\x1b[6;5H\x1b[!p");
    assert_eq!(t.cursor(), (4, 5));
    assert_eq!(t.plain(), "hello");
}

#[test]
fn soft_reset_saved_cursor() {
    // Ghostty "Terminal: softReset saved cursor"(18566줄): 저장한 커서는 처음 자리와 기본 스타일로
    let mut t = T::new(10, 10);
    t.feed("\x1b[6;5H\x1b[1m\x1b7\x1b[!p\x1b8");
    assert_eq!(t.cursor(), (0, 0));
    assert!(!t.template().flags.contains(Flags::BOLD));
}

#[test]
fn soft_reset_pen_protection_and_charsets() {
    // Ghostty "Terminal: softReset pen, protection, and charsets"(18581줄)
    let mut t = T::new(10, 10);
    t.feed("\x1b[1;31m\x1b[1\"q\x1b(0");
    assert!(t.template().flags.contains(Flags::BOLD));
    assert!(t.template().flags.contains(Flags::PROTECTED));
    t.feed("\x1b[!p");
    assert!(!t.template().flags.contains(Flags::BOLD));
    assert!(!t.template().flags.contains(Flags::PROTECTED));
    assert_eq!(t.template().fg, Color::Named(NamedColor::Foreground));
    assert_eq!(t.term.grid().cursor.charsets[CharsetIndex::G0], StandardCharset::Ascii);
}

#[test]
fn soft_reset_also_puts_back_a_shifted_charset() {
    // 문자 집합 전체(Ghostty `screen.charset = .{}`): G1을 선 그리기로 두고 SO로 쓰는 중이어도 처음으로
    let mut t = T::new(10, 10);
    t.feed("\x1b)0\x0e\x1b[!p");
    t.feed("q");
    assert_eq!(t.plain(), "q");
}

#[test]
fn soft_reset_on_the_alternate_screen_resets_that_screen() {
    // Ghostty는 활성 화면(`screens.active`)의 펜·저장 커서를 되돌린다. 대체 화면에서 부르면 대체 화면의 것
    let mut t = T::new(10, 10);
    t.feed("\x1b[?1049h\x1b[1m\x1b[4;4H\x1b7\x1b[!p");
    assert!(!t.template().flags.contains(Flags::BOLD));
    t.feed("\x1b8");
    assert_eq!(t.cursor(), (0, 0));
}

#[test]
fn soft_reset_same_function_for_both_paths() {
    // 앱의 두 길(세션 읽기 스레드, Emulator)은 모두 `kitty_color::soft_reset`을 부른다. Emulator 쪽을 DECSTR 바이트로 확인
    let mut e = bt_term::Emulator::new(GridSize::new(10, 5), Theme::default());
    e.feed(b"\x1b[?25l\x1b[1;31m\x1b[!pX");
    let s = e.snapshot();
    assert!(s.cursor.visible, "커서가 다시 보인다");
}

// ---- #532: DECST8C ----

#[test]
fn decst8c_puts_tabs_back_every_eight_columns() {
    // 이슈: `CSI 3 g`로 모두 지운 뒤 `CSI ? 5 W`, HT가 8칸씩
    let mut t = T::new(30, 3);
    t.feed("\x1b[3g\r\tX");
    assert_eq!(t.cursor(), (29, 0), "탭을 다 지우면 줄 끝까지 간다(전제)");
    t.feed("\x1b[2J\x1b[H\x1b[?5W\r\tA\tB\tC");
    assert_eq!(
        t.plain(),
        format!("{}A{}B{}C", " ".repeat(8), " ".repeat(7), " ".repeat(7))
    );
}

#[test]
fn decst8c_removes_custom_tab_stops() {
    let mut t = T::new(30, 3);
    t.feed("\x1b[3g\x1b[1;4H\x1bH\x1b[1;1H\tA");
    assert_eq!(t.cursor(), (4, 0), "직접 둔 탭 정지(열 3)로 간다");
    t.feed("\x1b[?5W\x1b[1;1H\tB");
    assert_eq!(t.cursor(), (9, 0), "되돌린 뒤엔 8칸 간격(열 8)");
}

#[test]
fn decst8c_leaves_the_cursor_and_screen_alone() {
    let mut t = T::new(30, 3);
    t.feed("abc\x1b[2;5H\x1b[?5W");
    assert_eq!(t.cursor(), (4, 1));
    assert_eq!(t.plain(), "abc");
}

// ---- 앱의 세션 읽기 스레드 길(PTY: /bin/sh와 printf만) ----

fn sh(script: &str, size: GridSize) -> (Session, Receiver<SessionEvent>) {
    let (tx, rx) = channel();
    let tx = Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let opts = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into()],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    (
        Session::spawn(opts, size, Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}")),
        rx,
    )
}

fn wait_exit(rx: &Receiver<SessionEvent>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Ok(SessionEvent::Exited(_)) = rx.recv_timeout(Duration::from_millis(100)) {
            return;
        }
    }
    panic!("10초 안에 끝나지 않았다");
}

#[test]
fn session_reader_thread_soft_reset_resets_the_modes() {
    // 세션 읽기 스레드(session.rs의 SoftReset 처리)가 같은 함수로 되돌리는지: 모드를 켜고 DECSTR, DECRQM으로 묻는다
    // 답: ?25;1$y(9바이트), ?6;2$y(8), ?7;1$y(8), ?1;2$y(8)
    let hex = |s: &str| s.bytes().map(|b| format!("{b:02x}")).collect::<String>();
    let q = "\\033[?25$p\\033[?6$p\\033[?7$p\\033[?1$p";
    let script = format!(
        "stty raw -echo; printf '\\033[?6h\\033[?7l\\033[?25l\\033[?1h\\033[!p{q}'; \
         a=$(dd bs=1 count=33 2>/dev/null | od -An -tx1 | tr -d ' \\n'); stty sane; printf 'ans=%s\\n' \"$a\""
    );
    let (s, rx) = sh(&script, GridSize::new(80, 5));
    wait_exit(&rx);
    let text = s.snapshot().text();
    let line = text
        .lines()
        .find(|l| l.starts_with("ans="))
        .unwrap_or_else(|| panic!("답이 없다: {text:?}"));
    let want = hex("\x1b[?25;1$y\x1b[?6;2$y\x1b[?7;1$y\x1b[?1;2$y");
    assert_eq!(line, format!("ans={want}"));
}
