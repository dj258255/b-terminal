//! PTY 하나와 그 VT 상태(ADR-001).
//!
//! 읽기 스레드가 PTY 바이트를 받아 같은 순서로 VT 해석기에 먹인다. alacritty의 EventLoop를 쓰지 않는 것은
//! 바이트를 가로채 알림(OSC 9/99/777)을 볼 자리가 필요하기 때문이다

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use alacritty_terminal::event::{Event as VtEvent, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Osc52, Term, TermMode};
use alacritty_terminal::vte::ansi::{
    CursorShape as VtCursorShape, CursorStyle as VtCursorStyle, NamedColor, Processor, Rgb as VtRgb,
};
use bt_core::Theme;
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::color;
use crate::notify::{Notification, Scanner, Signal};
use crate::screen::{Cell, CellFlags, Cursor, CursorShape, Modes, Screen};

/// 격자 크기와 셀 픽셀 크기(TIOCSWINSZ와 CSI 14/16/18 t 응답에 쓴다)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridSize {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: u16,
    pub cell_height: u16,
}

impl GridSize {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            cols: cols.max(2),
            rows: rows.max(1),
            cell_width: 8,
            cell_height: 16,
        }
    }

    fn pty(self) -> PtySize {
        PtySize {
            rows: self.rows,
            cols: self.cols,
            pixel_width: self.cols.saturating_mul(self.cell_width),
            pixel_height: self.rows.saturating_mul(self.cell_height),
        }
    }

    fn window(self) -> WindowSize {
        WindowSize {
            num_lines: self.rows,
            num_cols: self.cols,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
        }
    }
}

struct Dims(GridSize);

impl Dimensions for Dims {
    fn total_lines(&self) -> usize {
        self.0.rows as usize
    }
    fn screen_lines(&self) -> usize {
        self.0.rows as usize
    }
    fn columns(&self) -> usize {
        self.0.cols as usize
    }
}

/// 마우스 보고 모드를 모두 끄는 순서(1000·1002·1003 보고, 1005·1006·1015 꼴)
const MOUSE_OFF: &[u8] = b"\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l";

/// 띄울 프로그램
#[derive(Debug, Clone, Default)]
pub struct SpawnOptions {
    /// 없으면 `$SHELL`(없으면 `/bin/zsh`)을 로그인 셸로 띄운다
    pub program: Option<String>,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// 더하거나 덮을 환경 변수
    pub env: Vec<(String, String)>,
    pub scrollback: usize,
    /// 프로그램이 DECSCUSR로 바꾸기 전의 커서 모양(설정 cursor-style)
    pub cursor_shape: Option<CursorShape>,
    pub cursor_blink: bool,
}

/// 세션이 앱에 알리는 일. 읽기 스레드에서 불린다
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionEvent {
    /// 화면이 바뀌었다
    Output,
    Title(String),
    Bell,
    Notification(Notification),
    /// 셸이 알린 작업 폴더(OSC 7)와 그 호스트
    Cwd {
        host: String,
        path: PathBuf,
    },
    /// 진행 표시(OSC 9;4). 상태 0은 지움
    Progress {
        state: u8,
        percent: u8,
    },
    /// 프로세스가 끝났다. 종료 코드를 모르면 없다
    Exited(Option<u32>),
}

pub type EventSink = Arc<dyn Fn(SessionEvent) + Send + Sync>;

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

/// VT 엔진이 요구하는 이벤트 받이. 응답(장치 속성 등)은 PTY에 바로 쓴다
#[derive(Clone)]
struct Listener {
    writer: Writer,
    sink: EventSink,
    size: Arc<Mutex<GridSize>>,
    theme: Arc<Theme>,
}

impl EventListener for Listener {
    fn send_event(&self, event: VtEvent) {
        match event {
            VtEvent::PtyWrite(text) => {
                let mut w = self.writer.lock();
                let _ = w.write_all(text.as_bytes());
                let _ = w.flush();
            }
            VtEvent::Title(t) => (self.sink)(SessionEvent::Title(t)),
            VtEvent::ResetTitle => (self.sink)(SessionEvent::Title(String::new())),
            VtEvent::Bell => (self.sink)(SessionEvent::Bell),
            VtEvent::TextAreaSizeRequest(format) => {
                let reply = format(self.size.lock().window());
                let mut w = self.writer.lock();
                let _ = w.write_all(reply.as_bytes());
            }
            VtEvent::ColorRequest(index, format) => {
                let theme = &self.theme;
                let rgb = match index {
                    i if i < 256 => theme.indexed(i as u8),
                    i if i == NamedColor::Foreground as usize => theme.foreground,
                    i if i == NamedColor::Background as usize => theme.background,
                    _ => theme.cursor,
                };
                let reply = format(VtRgb {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                });
                let mut w = self.writer.lock();
                let _ = w.write_all(reply.as_bytes());
            }
            // OSC 52는 끈다(SECURITY.md). 나머지는 앱이 쓰지 않는다
            _ => {}
        }
    }
}

struct Inner {
    term: Term<Listener>,
    parser: Processor,
}

/// PTY 하나와 그 화면 상태
pub struct Session {
    inner: Arc<Mutex<Inner>>,
    writer: Writer,
    master: Mutex<Box<dyn MasterPty + Send>>,
    size: Arc<Mutex<GridSize>>,
    theme: Mutex<Arc<Theme>>,
    pid: Option<u32>,
    exited: Arc<Mutex<Option<Option<u32>>>>,
}

impl Session {
    /// 프로그램을 PTY로 띄운다. `sink`는 읽기 스레드에서 불린다
    pub fn spawn(options: SpawnOptions, size: GridSize, theme: Theme, sink: EventSink) -> anyhow::Result<Self> {
        let pty = native_pty_system();
        let pair = pty.openpty(size.pty())?;

        let mut cmd = match &options.program {
            Some(p) => {
                let mut c = CommandBuilder::new(p);
                c.args(&options.args);
                c
            }
            None => {
                let shell = std::env::var("SHELL")
                    .ok()
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "/bin/zsh".into());
                let mut c = CommandBuilder::new(shell);
                c.arg("-l");
                c
            }
        };
        let cwd = options.cwd.clone().or_else(dirs_home);
        if let Some(cwd) = cwd {
            cmd.cwd(cwd);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "b-terminal");
        cmd.env("TERM_PROGRAM_VERSION", env!("CARGO_PKG_VERSION"));
        // Finder에서 띄운 앱은 LANG이 비어 있어 한글이 깨진다. 비어 있을 때만 채운다
        if std::env::var_os("LANG").is_none() && std::env::var_os("LC_ALL").is_none() {
            cmd.env("LANG", "en_US.UTF-8");
        }
        for (k, v) in &options.env {
            cmd.env(k, v);
        }

        let mut child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);
        let pid = child.process_id();
        let mut reader = pair.master.try_clone_reader()?;
        let writer: Writer = Arc::new(Mutex::new(pair.master.take_writer()?));

        let theme = Arc::new(theme);
        let size_cell = Arc::new(Mutex::new(size));
        let listener = Listener {
            writer: writer.clone(),
            sink: sink.clone(),
            size: size_cell.clone(),
            theme: theme.clone(),
        };
        let config = Config {
            scrolling_history: if options.scrollback == 0 {
                10_000
            } else {
                options.scrollback
            },
            osc52: Osc52::Disabled,
            // Claude Code 등이 Shift+Enter를 받으려고 켠다(#41)
            kitty_keyboard: true,
            default_cursor_style: VtCursorStyle {
                shape: match options.cursor_shape {
                    Some(CursorShape::Beam) => VtCursorShape::Beam,
                    Some(CursorShape::Underline) => VtCursorShape::Underline,
                    Some(CursorShape::HollowBlock) => VtCursorShape::HollowBlock,
                    _ => VtCursorShape::Block,
                },
                blinking: options.cursor_blink,
            },
            ..Config::default()
        };
        let term = Term::new(config, &Dims(size), listener);
        let inner = Arc::new(Mutex::new(Inner {
            term,
            parser: Processor::new(),
        }));
        let exited = Arc::new(Mutex::new(None));

        let thread_inner = inner.clone();
        let thread_exited = exited.clone();
        std::thread::Builder::new().name("bt-pty-read".into()).spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            let mut scanner = Scanner::default();
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let bytes = &buf[..n];
                        let notes = scanner.feed(bytes);
                        {
                            let mut g = thread_inner.lock();
                            let Inner { term, parser } = &mut *g;
                            parser.advance(term, bytes);
                        }
                        for sig in notes {
                            sink(signal_event(sig));
                        }
                        sink(SessionEvent::Output);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            let code = child.wait().ok().map(|s| s.exit_code());
            *thread_exited.lock() = Some(code);
            sink(SessionEvent::Exited(code));
        })?;

        Ok(Self {
            inner,
            writer,
            master: Mutex::new(pair.master),
            size: size_cell,
            theme: Mutex::new(theme),
            pid,
            exited,
        })
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// 전경 프로세스 그룹의 pid(가볍다: ioctl 한 번)
    pub fn foreground_pid(&self) -> Option<i32> {
        self.master.lock().process_group_leader()
    }

    /// 지금 키 입력을 받는 프로세스(셸에서 vim을 띄웠으면 vim)
    pub fn foreground(&self) -> Option<crate::process::Foreground> {
        let pid = self.master.lock().process_group_leader()?;
        let name = crate::process::process_name(pid)?;
        Some(crate::process::Foreground { pid, name })
    }

    /// 전경 프로세스의 작업 폴더(없으면 셸의 것)
    pub fn cwd(&self) -> Option<PathBuf> {
        let pid = self
            .master
            .lock()
            .process_group_leader()
            .or(self.pid.map(|p| p as i32))?;
        crate::process::process_cwd(pid)
    }

    /// 끝났으면 `Some(종료 코드)`
    pub fn exit_status(&self) -> Option<Option<u32>> {
        *self.exited.lock()
    }

    pub fn size(&self) -> GridSize {
        *self.size.lock()
    }

    /// PTY에 바이트를 쓴다. 쓰면 스크롤백을 보던 화면을 맨 아래로 돌린다
    pub fn write(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        {
            let mut g = self.inner.lock();
            if g.term.grid().display_offset() != 0 {
                g.term.scroll_display(Scroll::Bottom);
            }
        }
        let mut w = self.writer.lock();
        if let Err(e) = w.write_all(bytes).and_then(|_| w.flush()) {
            log::debug!("pty write failed: {e}");
        }
    }

    pub fn resize(&self, size: GridSize) {
        let size = GridSize::new(size.cols, size.rows);
        let size = GridSize {
            cell_width: size.cell_width.max(1),
            cell_height: size.cell_height.max(1),
            ..size
        };
        {
            let mut cur = self.size.lock();
            if *cur == size {
                return;
            }
            *cur = size;
        }
        self.inner.lock().term.resize(Dims(size));
        if let Err(e) = self.master.lock().resize(size.pty()) {
            log::debug!("pty resize failed: {e}");
        }
    }

    /// 스크롤백을 줄 단위로 움직인다(양수는 위로)
    pub fn scroll(&self, lines: i32) {
        self.inner.lock().term.scroll_display(Scroll::Delta(lines));
    }

    pub fn scroll_to_bottom(&self) {
        self.inner.lock().term.scroll_display(Scroll::Bottom);
    }

    /// 마우스 보고 모드가 켜져 있으면 끈다. 껐으면 참.
    /// 마우스를 켠 프로그램이 끄지 못하고 죽으면 셸에 `ESC[<35;x;yM`이 글자로 쏟아진다(#24). PTY에는 쓰지 않고 화면 상태만 바꾼다
    pub fn reset_mouse_modes(&self) -> bool {
        let mut g = self.inner.lock();
        if !g
            .term
            .mode()
            .intersects(TermMode::MOUSE_MODE | TermMode::SGR_MOUSE | TermMode::UTF8_MOUSE)
        {
            return false;
        }
        let Inner { term, parser } = &mut *g;
        parser.advance(term, MOUSE_OFF);
        true
    }

    /// 동기화 출력(DEC 2026)이 끝나지 않고 기한이 지났으면 모아 둔 것을 반영한다. 반영했으면 참
    pub fn flush_expired_sync(&self) -> bool {
        let mut g = self.inner.lock();
        let expired = g
            .parser
            .sync_timeout()
            .sync_timeout()
            .is_some_and(|t| t <= Instant::now());
        if expired {
            let Inner { term, parser } = &mut *g;
            parser.stop_sync(term);
        }
        expired
    }

    /// 동기화 출력의 기한. 앱은 이때 `flush_expired_sync`를 부른다
    pub fn sync_deadline(&self) -> Option<Instant> {
        self.inner.lock().parser.sync_timeout().sync_timeout()
    }

    pub fn modes(&self) -> Modes {
        modes_of(*self.inner.lock().term.mode())
    }

    /// 화면 좌표(보이는 줄 기준)로 고르기를 시작한다
    pub fn select_start(&self, col: usize, row: usize, kind: SelectKind) {
        let mut g = self.inner.lock();
        let point = viewport_point(&g.term, col, row);
        let ty = match kind {
            SelectKind::Simple => SelectionType::Simple,
            SelectKind::Word => SelectionType::Semantic,
            SelectKind::Line => SelectionType::Lines,
        };
        g.term.selection = Some(Selection::new(ty, point, Side::Left));
    }

    pub fn select_update(&self, col: usize, row: usize) {
        let mut g = self.inner.lock();
        let point = viewport_point(&g.term, col, row);
        if let Some(sel) = g.term.selection.as_mut() {
            sel.update(point, Side::Right);
        }
    }

    pub fn select_clear(&self) {
        self.inner.lock().term.selection = None;
    }

    pub fn selection_text(&self) -> Option<String> {
        let g = self.inner.lock();
        g.term.selection_to_string().filter(|s| !s.is_empty())
    }

    /// 지금 화면을 꺼낸다
    pub fn snapshot(&self) -> Screen {
        let theme = self.theme.lock().clone();
        let g = self.inner.lock();
        snapshot_of(&g.term, &theme)
    }

    /// 색 테마를 바꾼다(설정 다시 읽기). 다음 화면부터 새 색으로 푼다
    pub fn set_theme(&self, theme: Theme) {
        *self.theme.lock() = Arc::new(theme);
    }
}

fn signal_event(sig: Signal) -> SessionEvent {
    match sig {
        Signal::Notify(n) => SessionEvent::Notification(n),
        Signal::Cwd { host, path } => SessionEvent::Cwd { host, path },
        Signal::Progress { state, percent } => SessionEvent::Progress { state, percent },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectKind {
    Simple,
    Word,
    Line,
}

fn dirs_home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn viewport_point<T>(term: &Term<T>, col: usize, row: usize) -> Point {
    let cols = term.columns();
    let rows = term.screen_lines();
    let offset = term.grid().display_offset() as i32;
    Point::new(
        Line(row.min(rows.saturating_sub(1)) as i32 - offset),
        Column(col.min(cols.saturating_sub(1))),
    )
}

fn modes_of(m: TermMode) -> Modes {
    Modes {
        app_cursor: m.contains(TermMode::APP_CURSOR),
        app_keypad: m.contains(TermMode::APP_KEYPAD),
        bracketed_paste: m.contains(TermMode::BRACKETED_PASTE),
        alt_screen: m.contains(TermMode::ALT_SCREEN),
        mouse_report: m.contains(TermMode::MOUSE_REPORT_CLICK),
        mouse_drag: m.contains(TermMode::MOUSE_DRAG),
        mouse_motion: m.contains(TermMode::MOUSE_MOTION),
        sgr_mouse: m.contains(TermMode::SGR_MOUSE),
        focus_events: m.contains(TermMode::FOCUS_IN_OUT),
        alternate_scroll: m.contains(TermMode::ALTERNATE_SCROLL),
        kitty: crate::input::KittyFlags {
            disambiguate: m.contains(TermMode::DISAMBIGUATE_ESC_CODES),
            all_as_escapes: m.contains(TermMode::REPORT_ALL_KEYS_AS_ESC),
        },
    }
}

fn snapshot_of<T: EventListener>(term: &Term<T>, theme: &Theme) -> Screen {
    let cols = term.columns();
    let rows = term.screen_lines();
    let content = term.renderable_content();
    let overrides = content.colors;
    let background = color::named(theme, overrides, NamedColor::Background);
    let foreground = color::named(theme, overrides, NamedColor::Foreground);
    let mut cells = vec![Cell::blank(foreground, background); cols * rows];
    let offset = content.display_offset as i32;
    let selection = content.selection;

    for indexed in content.display_iter {
        let row = indexed.point.line.0 + offset;
        if row < 0 || row as usize >= rows {
            continue;
        }
        let col = indexed.point.column.0;
        if col >= cols {
            continue;
        }
        let src = indexed.cell;
        let (fg, bg) = color::cell_colors(theme, overrides, src.fg, src.bg, src.flags);
        let mut flags = CellFlags::empty();
        let f = src.flags;
        if f.contains(Flags::BOLD) {
            flags.insert(CellFlags::BOLD);
        }
        if f.contains(Flags::ITALIC) {
            flags.insert(CellFlags::ITALIC);
        }
        if f.intersects(Flags::UNDERLINE | Flags::DOUBLE_UNDERLINE | Flags::DOTTED_UNDERLINE | Flags::DASHED_UNDERLINE)
        {
            flags.insert(CellFlags::UNDERLINE);
        }
        if f.contains(Flags::UNDERCURL) {
            flags.insert(CellFlags::UNDERCURL);
        }
        if f.contains(Flags::STRIKEOUT) {
            flags.insert(CellFlags::STRIKE);
        }
        if f.contains(Flags::WIDE_CHAR) {
            flags.insert(CellFlags::WIDE);
        }
        if f.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
            flags.insert(CellFlags::SPACER);
        }
        if f.contains(Flags::HIDDEN) {
            flags.insert(CellFlags::HIDDEN);
        }
        if selection.is_some_and(|s| s.contains(indexed.point)) {
            flags.insert(CellFlags::SELECTED);
        }
        let combining = src.zerowidth().map(|z| z.to_vec()).unwrap_or_default();
        cells[row as usize * cols + col] = Cell {
            ch: src.c,
            combining,
            fg,
            bg,
            flags,
        };
    }

    let vt_cursor = content.cursor;
    let cursor_row = vt_cursor.point.line.0 + offset;
    let show = content.mode.contains(TermMode::SHOW_CURSOR);
    let shape = match vt_cursor.shape {
        VtCursorShape::Block => Some(CursorShape::Block),
        VtCursorShape::Beam => Some(CursorShape::Beam),
        VtCursorShape::Underline => Some(CursorShape::Underline),
        VtCursorShape::HollowBlock => Some(CursorShape::HollowBlock),
        VtCursorShape::Hidden => None,
    };
    let in_view = cursor_row >= 0 && (cursor_row as usize) < rows;
    let col = vt_cursor.point.column.0.min(cols.saturating_sub(1));
    let wide = in_view && cells[cursor_row as usize * cols + col].flags.contains(CellFlags::WIDE);
    let cursor = Cursor {
        col,
        row: cursor_row.max(0) as usize,
        shape: shape.unwrap_or(CursorShape::Block),
        wide,
        visible: show && shape.is_some() && in_view,
    };

    Screen {
        cols,
        rows,
        cells,
        cursor,
        modes: modes_of(content.mode),
        display_offset: content.display_offset,
        history: term.grid().history_size(),
        background,
    }
}

/// PTY 없이 바이트를 VT로 해석해 화면을 얻는다. 렌더러 테스트와 b-studio 세션 터미널(바이트가 HTTP로 오는 칸)에 쓴다
pub struct Emulator {
    term: Term<NullListener>,
    parser: Processor,
    theme: Theme,
    scanner: Scanner,
}

#[derive(Clone, Copy)]
struct NullListener;
impl EventListener for NullListener {}

impl Emulator {
    pub fn new(size: GridSize, theme: Theme) -> Self {
        let config = Config {
            osc52: Osc52::Disabled,
            kitty_keyboard: true,
            ..Config::default()
        };
        Self {
            term: Term::new(config, &Dims(size), NullListener),
            parser: Processor::new(),
            theme,
            scanner: Scanner::default(),
        }
    }

    /// 바이트를 먹이고 그 안의 신호(알림·폴더·진행)를 돌려준다
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Signal> {
        let notes = self.scanner.feed(bytes);
        self.parser.advance(&mut self.term, bytes);
        notes
    }

    pub fn resize(&mut self, size: GridSize) {
        self.term.resize(Dims(size));
    }

    pub fn scroll(&mut self, lines: i32) {
        self.term.scroll_display(Scroll::Delta(lines));
    }

    pub fn modes(&self) -> Modes {
        modes_of(*self.term.mode())
    }

    pub fn snapshot(&self) -> Screen {
        snapshot_of(&self.term, &self.theme)
    }
}
