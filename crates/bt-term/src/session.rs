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
use bt_core::config::OscColorReportFormat;
use parking_lot::Mutex;
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

use crate::color;
use crate::notify::{Notification, PromptMark, Scanner, Signal};
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
    /// OSC 52 쓰기·읽기 허용(설정 clipboard-write·clipboard-read)
    pub clipboard_write: bool,
    pub clipboard_read: bool,
    /// 단어 고르기(두 번 누르기)의 경계 글자(설정 selection-word-chars, #126). 없으면 VT 엔진 기본
    pub word_chars: Option<String>,
    /// 새 출력이 오면 맨 아래로 스크롤한다(설정 scroll-to-bottom의 output, #147)
    pub scroll_on_output: bool,
    /// CSI 21 t(창 제목 보고)에 답할지(설정 title-report, 기본 끔, #151)
    pub title_report: bool,
    /// 색 질의(OSC 4·10·11)에 답하는 꼴(설정 osc-color-report-format, #151)
    pub osc_color_report_format: bt_core::config::OscColorReportFormat,
    /// ENQ(0x05)에 돌려보낼 글(설정 enquiry-response, 빈 값은 답하지 않음, #176)
    pub enquiry_response: String,
    /// 프로그램의 키보드 잠금(KAM, CSI 2 h)을 받을지(설정 vt-kam-allowed, 기본 끔, #176)
    pub vt_kam_allowed: bool,
    /// 프로그램의 창 크기 바꾸기 요청(CSI 8 t)을 앱에 알릴지(설정 vt-window-resize-allowed, 기본 끔, #176)
    pub vt_window_resize_allowed: bool,
    /// kitty 그림 저장 바이트 상한(설정 image-storage-limit). 없으면 기본(320000000), 0이면 그림을 쓰지 않는다
    pub image_storage_limit: Option<u32>,
}

/// 창 크기 바꾸기 요청(CSI 8 ; 행 ; 열 t)이 가리키는 칸 수를 정한다. 인자와 결과는 모두 (열, 행)이다. 0은 그 쪽을 그대로 두고,
/// 40열 × 10행보다 작으면 그 크기로 올리고(프로그램이 창을 줄여 출력을 숨기지 못하게), 화면(`max`)보다 크면 줄인다
pub fn window_resize_target(req: (u16, u16), cur: (u16, u16), max: (u16, u16)) -> (u16, u16) {
    let pick = |req: u16, cur: u16, min: u16, max: u16| if req == 0 { cur } else { req.max(min) }.min(max);
    (pick(req.0, cur.0, 40, max.0), pick(req.1, cur.1, 10, max.1))
}

/// 새 출력이 왔을 때 맨 아래로 스크롤해야 하는가(켜져 있고 위로 올라가 있을 때만)
fn scroll_wanted(on: bool, display_offset: usize) -> bool {
    on && display_offset != 0
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
    /// 프로그램이 OSC 52로 클립보드에 쓰려 한다
    ClipboardStore(String),
    /// 프로그램이 OSC 52로 클립보드를 읽으려 한다. 앱이 `reply`로 답을 만들어 PTY에 쓴다
    ClipboardLoad(ClipboardReply),
    /// 셸 통합: 명령이 실행되기 시작했다(OSC 133;C). 명령 글을 알면 함께
    CommandStarted(Option<String>),
    /// 셸 통합: 명령이 끝났다(OSC 133;D). 출력과 종료 코드
    CommandFinished(CommandRecord),
    /// 프로그램이 창 크기를 바꾸라고 요청했다(CSI 8 ; 행 ; 열 t, 설정 vt-window-resize-allowed가 켜졌을 때만). 0은 그 쪽을 그대로 둔다
    WindowResize {
        rows: u16,
        cols: u16,
    },
    /// 프로세스가 끝났다. 종료 코드를 모르면 없다
    Exited(Option<u32>),
}

pub type EventSink = Arc<dyn Fn(SessionEvent) + Send + Sync>;

/// OSC 52 읽기에 답을 만드는 함수(엔진이 준다). 클립보드 글을 넣으면 PTY에 쓸 이스케이프가 나온다
#[derive(Clone)]
pub struct ClipboardReply(Arc<dyn Fn(&str) -> String + Send + Sync>);

impl ClipboardReply {
    pub fn format(&self, text: &str) -> String {
        (self.0)(text)
    }
}

impl std::fmt::Debug for ClipboardReply {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClipboardReply")
    }
}

impl PartialEq for ClipboardReply {
    fn eq(&self, _: &Self) -> bool {
        false
    }
}

impl Eq for ClipboardReply {}

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;

/// VT 엔진이 요구하는 이벤트 받이. 응답(장치 속성 등)은 PTY에 바로 쓴다
#[derive(Clone)]
struct Listener {
    writer: Writer,
    sink: EventSink,
    size: Arc<Mutex<GridSize>>,
    theme: Arc<Theme>,
    /// 색 질의에 답하는 꼴(osc-color-report-format)
    color_format: OscColorReportFormat,
    /// 프로그램이 정한 지금 창 제목(CSI 21 t 보고용)
    title: Arc<Mutex<String>>,
}

/// 엔진이 만든 색 답(`rgb:RRRR/GGGG/BBBB`)을 8비트 꼴(`rgb:RR/GG/BB`)로 줄인다. 16비트는 8비트 값을 되풀이한 것이라 앞 두 자리가 원래 값이다
fn narrow_color_reply(reply: &str) -> String {
    let Some(at) = reply.find("rgb:") else {
        return reply.to_string();
    };
    let (head, rest) = reply.split_at(at + 4);
    let mut out = String::from(head);

    let mut run = 0;
    for c in rest.chars() {
        if c.is_ascii_hexdigit() && run < 4 {
            if run < 2 {
                out.push(c);
            }
            run += 1;
        } else if c.is_ascii_hexdigit() {
            out.push(c);
        } else {
            run = 0;
            out.push(c);
        }
    }
    out
}

/// 제목 보고에 넣을 제목: 제어 문자(C0·DEL·C1)를 지운다. 출력이 정한 제목이므로 답이 입력 안에서 명령이 되지 않게 한다
fn sanitize_title(title: &str) -> String {
    title.chars().filter(|c| !c.is_control()).collect()
}

impl EventListener for Listener {
    fn send_event(&self, event: VtEvent) {
        match event {
            VtEvent::PtyWrite(text) => {
                let mut w = self.writer.lock();
                let _ = w.write_all(text.as_bytes());
                let _ = w.flush();
            }
            VtEvent::Title(t) => {
                *self.title.lock() = t.clone();
                (self.sink)(SessionEvent::Title(t))
            }
            VtEvent::ResetTitle => {
                self.title.lock().clear();
                (self.sink)(SessionEvent::Title(String::new()))
            }
            VtEvent::Bell => (self.sink)(SessionEvent::Bell),
            VtEvent::TextAreaSizeRequest(format) => {
                let reply = format(self.size.lock().window());
                let mut w = self.writer.lock();
                let _ = w.write_all(reply.as_bytes());
            }
            VtEvent::ColorRequest(index, format) => {
                if self.color_format == OscColorReportFormat::None {
                    return;
                }
                let theme = &self.theme;
                let rgb = match index {
                    i if i < 256 => theme.indexed(i as u8),
                    i if i == NamedColor::Foreground as usize => theme.foreground,
                    i if i == NamedColor::Background as usize => theme.background,
                    _ => theme.cursor,
                };
                let mut reply = format(VtRgb {
                    r: rgb.r,
                    g: rgb.g,
                    b: rgb.b,
                });
                if self.color_format == OscColorReportFormat::Bit8 {
                    reply = narrow_color_reply(&reply);
                }
                let mut w = self.writer.lock();
                let _ = w.write_all(reply.as_bytes());
            }
            // OSC 52 쓰기: 앱이 클립보드에 넣는다(설정 clipboard-write, SECURITY.md)
            VtEvent::ClipboardStore(_, text) => (self.sink)(SessionEvent::ClipboardStore(text)),
            // OSC 52 읽기: 설정 clipboard-read = allow일 때만 엔진이 이 사건을 낸다. 앱이 클립보드를 읽어 답한다
            VtEvent::ClipboardLoad(_, format) => (self.sink)(SessionEvent::ClipboardLoad(ClipboardReply(format))),
            _ => {}
        }
    }
}

struct Inner {
    term: Term<Listener>,
    parser: Processor,
    /// 셸 통합이 알린 프롬프트 시작(OSC 133;A) 줄. 스크롤백 맨 위를 0으로 센 절대 줄
    prompts: std::collections::VecDeque<i64>,
    /// 찾기(#39). 찾는 중이 아니면 없다
    search: Option<crate::search::Search>,
    /// kitty 그래픽(#76): 저장한 그림과 놓인 자리
    graphics: crate::graphics::Graphics,
    /// 셀 크기(px). 그림을 칸에 맞출 때 쓴다
    cell: (u32, u32),
    /// 고르기의 움직이는 끝점(#138 adjust_selection). 고르기를 시작·늘이면 바뀐다
    sel_end: Option<Point>,
}

/// 프롬프트를 기억하는 상한(오래된 것부터 버린다)
const MAX_PROMPTS: usize = 2000;
/// 명령 하나의 출력을 모으는 상한(넘으면 앞을 버리고 끝을 남긴다)
const MAX_CAPTURE: usize = 4 * 1024 * 1024;

/// 셸 통합으로 알게 된 명령 하나
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandRecord {
    /// 셸이 알린 명령 글(OSC 633;E). 모르면 없다
    pub command: Option<String>,
    /// 명령 출력(이스케이프를 뺀 글). 상한을 넘으면 끝부분만
    pub output: String,
    pub exit_code: Option<i32>,
    pub duration: std::time::Duration,
}

impl Inner {
    /// 지금 커서 줄(절대)
    fn cursor_abs(&self) -> i64 {
        self.term.grid().history_size() as i64 + self.term.grid().cursor.point.line.0 as i64
    }

    fn graphics_command(&mut self, raw: &[u8]) -> Option<Vec<u8>> {
        let Inner {
            term,
            parser,
            graphics,
            cell,
            ..
        } = self;
        graphics_command(term, parser, graphics, *cell, raw)
    }

    /// 화면 지우기에 맞춰 놓인 그림을 지운다
    fn graphics_erase(&mut self, visible: bool, scrollback: bool) {
        self.graphics.erase(visible, scrollback);
    }
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
    /// 셸 통합 표지(OSC 133;A)를 한 번이라도 받았다
    integrated: Arc<std::sync::atomic::AtomicBool>,
    /// 새 출력이 오면 맨 아래로 스크롤하는가(읽기 스레드가 본다)
    scroll_on_output: Arc<std::sync::atomic::AtomicBool>,
    /// 프로그램이 키보드를 잠갔다(KAM, vt-kam-allowed가 켜졌을 때만). 잠겨 있으면 사람의 입력을 PTY에 쓰지 않는다
    kam_locked: Arc<std::sync::atomic::AtomicBool>,
}

impl Session {
    /// 프로그램이 키보드 잠금(KAM)을 걸어 두었는가
    pub fn keyboard_locked(&self) -> bool {
        self.kam_locked.load(std::sync::atomic::Ordering::Acquire)
    }

    /// 이 탭의 셸이 셸 통합 표지를 보내는가(btctl run·프롬프트 이동·끝난 명령 알림이 쓴다)
    pub fn has_shell_integration(&self) -> bool {
        self.integrated.load(std::sync::atomic::Ordering::Acquire)
    }

    /// 지금 화면에 보이는 프롬프트 시작 줄들(화면 맨 위가 0, #136 여백 색)
    pub fn prompt_rows(&self) -> Vec<usize> {
        let g = self.inner.lock();
        let history = g.term.grid().history_size() as i64;
        let offset = g.term.grid().display_offset() as i64;
        let top = history - offset;
        let rows = g.term.screen_lines() as i64;
        g.prompts
            .iter()
            .filter(|&&p| p >= top && p < top + rows)
            .map(|&p| (p - top) as usize)
            .collect()
    }

    /// 앞(-1)·뒤(+1) 프롬프트로 스크롤한다. 옮겼으면 참
    pub fn jump_to_prompt(&self, dir: i32) -> bool {
        let mut g = self.inner.lock();
        let history = g.term.grid().history_size() as i64;
        let offset = g.term.grid().display_offset() as i64;
        // 지금 화면 맨 위 줄(절대)
        let top = history - offset;
        let target = if dir < 0 {
            g.prompts.iter().rev().find(|&&p| p < top).copied()
        } else {
            g.prompts.iter().find(|&&p| p > top).copied()
        };
        let Some(target) = target else {
            if dir > 0 && offset > 0 {
                g.term.scroll_display(Scroll::Bottom);
                return true;
            }
            return false;
        };
        // 그 줄이 화면 맨 위에 오게(스크롤백 밖이면 맨 아래)
        let new_offset = (history - target).clamp(0, history);
        g.term.scroll_display(Scroll::Delta((new_offset - offset) as i32));
        true
    }

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
        let title = Arc::new(Mutex::new(String::new()));
        let size_cell = Arc::new(Mutex::new(size));
        let listener = Listener {
            writer: writer.clone(),
            sink: sink.clone(),
            size: size_cell.clone(),
            theme: theme.clone(),
            color_format: options.osc_color_report_format,
            title: title.clone(),
        };
        let config = Config {
            scrolling_history: if options.scrollback == 0 {
                10_000
            } else {
                options.scrollback
            },
            osc52: match (options.clipboard_write, options.clipboard_read) {
                (true, true) => Osc52::CopyPaste,
                (true, false) => Osc52::OnlyCopy,
                (false, true) => Osc52::OnlyPaste,
                (false, false) => Osc52::Disabled,
            },
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
            semantic_escape_chars: options
                .word_chars
                .clone()
                .unwrap_or_else(|| Config::default().semantic_escape_chars),
            ..Config::default()
        };
        let term = Term::new(config, &Dims(size), listener);
        let inner = Arc::new(Mutex::new(Inner {
            term,
            parser: Processor::new(),
            prompts: std::collections::VecDeque::new(),
            search: None,
            graphics: crate::graphics::Graphics::with_limit(
                options
                    .image_storage_limit
                    .map_or(crate::graphics::DEFAULT_STORE, |n| n as usize),
            ),
            cell: (size.cell_width.max(1) as u32, size.cell_height.max(1) as u32),
            sel_end: None,
        }));
        let integrated = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread_integrated = integrated.clone();
        let scroll_on_output = Arc::new(std::sync::atomic::AtomicBool::new(options.scroll_on_output));
        let thread_scroll_on_output = scroll_on_output.clone();
        let kam_locked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread_kam_locked = kam_locked.clone();
        let exited = Arc::new(Mutex::new(None));

        let thread_inner = inner.clone();
        let thread_exited = exited.clone();
        let thread_writer = writer.clone();
        let thread_title = title.clone();
        let title_report = options.title_report;
        let enquiry_response = options.enquiry_response.clone();
        let vt_kam_allowed = options.vt_kam_allowed;
        let vt_window_resize_allowed = options.vt_window_resize_allowed;
        std::thread::Builder::new().name("bt-pty-read".into()).spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            let mut scanner = Scanner::default();
            // 셸 통합: C(실행)부터 D(끝)까지의 출력과 그 명령
            let mut capture: Option<(Vec<u8>, Instant)> = None;
            let mut pending_cmd: Option<String> = None;
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let bytes = &buf[..n];
                        let signals = scanner.feed_at(bytes);
                        let mut events = Vec::new();
                        {
                            let mut g = thread_inner.lock();
                            let mut pos = 0;
                            // 신호가 끝난 자리까지 나눠 먹인다: 프롬프트 줄을 정확히 적고, 출력을 C와 D 사이에서 자른다
                            for (end, sig) in signals {
                                let seg = &bytes[pos..end.min(bytes.len())];
                                {
                                    let Inner { term, parser, .. } = &mut *g;
                                    parser.advance(term, seg);
                                }
                                if let Some((cap, _)) = capture.as_mut() {
                                    push_capped(cap, seg);
                                }
                                pos = end.min(bytes.len());
                                match sig {
                                    Signal::Prompt(PromptMark::PromptStart) => {
                                        thread_integrated.store(true, std::sync::atomic::Ordering::Release);
                                        let line = g.cursor_abs();
                                        if g.prompts.back() != Some(&line) {
                                            g.prompts.push_back(line);
                                            if g.prompts.len() > MAX_PROMPTS {
                                                g.prompts.pop_front();
                                            }
                                        }
                                    }
                                    Signal::Prompt(PromptMark::CommandExecuted) => {
                                        capture = Some((Vec::new(), Instant::now()));
                                        events.push(SessionEvent::CommandStarted(pending_cmd.clone()));
                                    }
                                    Signal::Prompt(PromptMark::CommandFinished(code)) => {
                                        if let Some((cap, started)) = capture.take() {
                                            events.push(SessionEvent::CommandFinished(CommandRecord {
                                                command: pending_cmd.take(),
                                                output: crate::plain::plain_text(&cap),
                                                exit_code: code,
                                                duration: started.elapsed(),
                                            }));
                                        }
                                    }
                                    Signal::Prompt(PromptMark::CommandStart) => {}
                                    Signal::CommandLine(c) => pending_cmd = Some(c),
                                    Signal::Graphics(raw) => {
                                        if let Some(reply) = g.graphics_command(&raw) {
                                            let mut w = thread_writer.lock();
                                            let _ = w.write_all(&reply);
                                            let _ = w.flush();
                                        }
                                    }
                                    Signal::Erase { visible, scrollback } => {
                                        g.graphics_erase(visible, scrollback);
                                        // 초기화(RIS)는 키보드 잠금도 푼다
                                        if visible && scrollback {
                                            thread_kam_locked.store(false, std::sync::atomic::Ordering::Release);
                                        }
                                    }
                                    // ENQ: 설정한 글이 있을 때만 답한다(ADR-013). 글은 사용자가 정한 것이다
                                    Signal::Enquiry => {
                                        if !enquiry_response.is_empty() {
                                            let mut w = thread_writer.lock();
                                            let _ = w.write_all(enquiry_response.as_bytes());
                                            let _ = w.flush();
                                        }
                                    }
                                    Signal::Kam(on) => {
                                        if vt_kam_allowed {
                                            thread_kam_locked.store(on, std::sync::atomic::Ordering::Release);
                                        }
                                    }
                                    Signal::WindowResize { rows, cols } => {
                                        if vt_window_resize_allowed {
                                            events.push(SessionEvent::WindowResize { rows, cols });
                                        }
                                    }
                                    // 제목 보고는 title-report가 켜졌을 때만 답한다(ADR-012)
                                    Signal::TitleReport => {
                                        if title_report {
                                            let reply = format!("\x1b]l{}\x1b\\", sanitize_title(&thread_title.lock()));
                                            let mut w = thread_writer.lock();
                                            let _ = w.write_all(reply.as_bytes());
                                            let _ = w.flush();
                                        }
                                    }
                                    other => events.push(signal_event(other)),
                                }
                            }
                            let rest = &bytes[pos..];
                            let Inner { term, parser, .. } = &mut *g;
                            parser.advance(term, rest);
                            if let Some((cap, _)) = capture.as_mut() {
                                push_capped(cap, rest);
                            }
                        }
                        // 새 출력이 오면 맨 아래로(scroll-to-bottom의 output, #147)
                        if thread_scroll_on_output.load(std::sync::atomic::Ordering::Relaxed) {
                            let mut g = thread_inner.lock();
                            if scroll_wanted(true, g.term.grid().display_offset()) {
                                g.term.scroll_display(Scroll::Bottom);
                            }
                        }
                        for e in events {
                            sink(e);
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
            integrated,
            scroll_on_output,
            kam_locked,
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

    /// PTY에 바이트를 그대로 쓴다. 보던 자리는 건드리지 않는다(응답·초점 알림·마우스 보고 같은 것)
    pub fn write(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut w = self.writer.lock();
        if let Err(e) = w.write_all(bytes).and_then(|_| w.flush()) {
            log::debug!("pty write failed: {e}");
        }
    }

    /// 바이트를 PTY가 아니라 화면에 바로 그린다(명령이 끝난 칸에 안내를 남길 때, #178)
    pub fn feed_local(&self, bytes: &[u8]) {
        let mut g = self.inner.lock();
        let Inner { term, parser, .. } = &mut *g;
        parser.advance(term, bytes);
    }

    /// 사용자가 친 키·붙여넣기를 PTY에 쓴다. `scroll`이면 쓰기 전에 맨 아래로 돌린다
    /// (설정 scroll-to-bottom의 keystroke, #147)
    pub fn write_input(&self, bytes: &[u8], scroll: bool) {
        if bytes.is_empty() || self.keyboard_locked() {
            return;
        }
        if scroll {
            self.scroll_to_bottom();
        }
        self.write(bytes);
    }

    /// 새 출력이 오면 맨 아래로 스크롤할지 바꾼다(설정을 다시 읽을 때)
    pub fn set_scroll_on_output(&self, on: bool) {
        self.scroll_on_output.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn resize(&self, size: GridSize) {
        // 셀 픽셀 크기는 그대로 둔다: PTY 창 크기(TIOCGWINSZ의 픽셀)와 그림 칸 맞춤이 쓴다.
        // 전에는 GridSize::new로 다시 만들어 8×16으로 바뀌었다(#76에서 고침)
        let size = GridSize {
            cols: size.cols.max(2),
            rows: size.rows.max(1),
            cell_width: size.cell_width.max(1),
            cell_height: size.cell_height.max(1),
        };
        {
            let mut cur = self.size.lock();
            if *cur == size {
                return;
            }
            *cur = size;
        }
        {
            let mut g = self.inner.lock();
            g.term.resize(Dims(size));
            g.cell = (size.cell_width as u32, size.cell_height as u32);
        }
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

    /// 찾기를 시작한다(#39): 맨 아래에서 위로 첫 맞은 곳으로 화면을 옮긴다. 빈 글이면 찾기를 끄고 없다
    pub fn search(&self, query: &str) -> Option<crate::search::SearchStatus> {
        let mut g = self.inner.lock();
        let Inner { term, search, .. } = &mut *g;
        *search = crate::search::Search::new(query);
        let s = search.as_mut()?;
        s.step(term, true);
        Some(s.status(term))
    }

    /// 다음 맞은 곳으로(`older`면 위로). 찾는 중이 아니면 없다
    pub fn search_step(&self, older: bool) -> Option<crate::search::SearchStatus> {
        let mut g = self.inner.lock();
        let Inner { term, search, .. } = &mut *g;
        let s = search.as_mut()?;
        s.step(term, older);
        Some(s.status(term))
    }

    /// 찾기를 끝낸다(강조를 지운다). 화면 자리는 그대로 둔다
    pub fn search_clear(&self) {
        self.inner.lock().search = None;
    }

    /// 맨 아래에서 위로 `lines`줄(스크롤백 포함)의 글(#66)
    pub fn tail_text(&self, lines: usize) -> String {
        tail_text(&self.inner.lock().term, lines)
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
        let Inner { term, parser, .. } = &mut *g;
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
            let Inner { term, parser, .. } = &mut *g;
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
        g.sel_end = Some(point);
    }

    pub fn select_update(&self, col: usize, row: usize) {
        let mut g = self.inner.lock();
        let point = viewport_point(&g.term, col, row);
        if let Some(sel) = g.term.selection.as_mut() {
            sel.update(point, Side::Right);
        }
        g.sel_end = Some(point);
    }

    /// 고른 것의 끝점을 키보드로 옮긴다(Ghostty `adjust_selection`, #138). 고른 것이 없으면 아무것도 안 하고 거짓.
    /// 옮긴 끝점이 화면 밖이면 그 줄이 보이게 스크롤한다
    pub fn adjust_selection(&self, adj: SelectionAdjust) -> bool {
        let mut g = self.inner.lock();
        let Some(end) = g.sel_end.filter(|_| g.term.selection.is_some()) else {
            return false;
        };
        let (cols, page) = (g.term.columns(), g.term.screen_lines());
        let (top, bottom) = (g.term.topmost_line(), g.term.bottommost_line());
        let next = adjusted_point(end, adj, cols, top, bottom, page);
        if let Some(sel) = g.term.selection.as_mut() {
            sel.update(next, Side::Right);
        }
        g.sel_end = Some(next);
        g.term.scroll_to_point(next);
        true
    }

    pub fn select_clear(&self) {
        let mut g = self.inner.lock();
        g.term.selection = None;
        g.sel_end = None;
    }

    pub fn selection_text(&self) -> Option<String> {
        let g = self.inner.lock();
        g.term.selection_to_string().filter(|s| !s.is_empty())
    }

    /// 지금 화면을 꺼낸다
    pub fn snapshot(&self) -> Screen {
        let theme = self.theme.lock().clone();
        let mut g = self.inner.lock();
        let Inner {
            term, search, graphics, ..
        } = &mut *g;
        snapshot_of(term, &theme, search.as_mut(), Some(graphics))
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
        // 셸 통합 표지와 그래픽은 읽기 스레드가 따로 다룬다
        Signal::Prompt(_)
        | Signal::CommandLine(_)
        | Signal::Graphics(_)
        | Signal::Erase { .. }
        | Signal::TitleReport
        | Signal::Enquiry
        | Signal::Kam(_)
        | Signal::WindowResize { .. } => SessionEvent::Output,
    }
}

fn push_capped(cap: &mut Vec<u8>, seg: &[u8]) {
    cap.extend_from_slice(seg);
    if cap.len() > MAX_CAPTURE {
        let cut = cap.len() - MAX_CAPTURE;
        cap.drain(..cut);
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

/// 고른 것의 끝점을 옮기는 방향(Ghostty `adjust_selection`의 인자)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionAdjust {
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    /// 화면(스크롤백 포함) 맨 위 왼쪽
    Home,
    /// 맨 아래 오른쪽
    End,
    BeginningOfLine,
    EndOfLine,
}

/// 끝점 `p`를 옮긴 자리. `top`·`bottom`은 스크롤백 맨 위·화면 맨 아래 줄, `page`는 화면 줄 수.
/// 왼쪽·오른쪽은 줄 끝에서 앞·뒤 줄로 넘어가고, 모든 결과는 범위 안으로 붙인다
fn adjusted_point(p: Point, adj: SelectionAdjust, cols: usize, top: Line, bottom: Line, page: usize) -> Point {
    use SelectionAdjust::*;
    let last = Column(cols.saturating_sub(1));
    let line = |l: i32| Line(l.clamp(top.0, bottom.0));
    match adj {
        Left if p.column.0 > 0 => Point::new(p.line, p.column - 1),
        Left if p.line > top => Point::new(p.line - 1, last),
        Left => p,
        Right if p.column < last => Point::new(p.line, p.column + 1),
        Right if p.line < bottom => Point::new(p.line + 1, Column(0)),
        Right => p,
        Up => Point::new(line(p.line.0 - 1), p.column),
        Down => Point::new(line(p.line.0 + 1), p.column),
        PageUp => Point::new(line(p.line.0 - page as i32), p.column),
        PageDown => Point::new(line(p.line.0 + page as i32), p.column),
        Home => Point::new(top, Column(0)),
        End => Point::new(bottom, last),
        BeginningOfLine => Point::new(p.line, Column(0)),
        EndOfLine => Point::new(p.line, last),
    }
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

/// 화면 맨 아래에서 위로 `lines`줄(스크롤백 포함)을 글로 꺼낸다(#66). 줄 넘김으로 접힌 줄은 이어 붙이고,
/// 줄 끝 공백과 끝의 빈 줄은 뺀다. 넓은 글자의 뒤 칸은 건너뛴다
fn tail_text<T>(term: &Term<T>, lines: usize) -> String {
    // 화면 맨 아래가 아니라 마지막 글 줄(또는 커서 줄)부터 센다. 글이 화면 위쪽에만 있을 때 아래 빈 줄을 세지 않게(#99)
    let cols = term.columns();
    let blank = |line: Line| {
        let row = &term.grid()[line];
        (0..cols).all(|c| {
            let ch = row[Column(c)].c;
            ch == ' ' || ch == '\0'
        })
    };
    let mut bottom = term.bottommost_line();
    let cursor = term.grid().cursor.point.line;
    while bottom > cursor && bottom > Line(0) && blank(bottom) {
        bottom -= 1;
    }
    let top = std::cmp::max(term.topmost_line(), bottom - (lines.max(1) as i32 - 1));
    let mut out = String::new();
    let mut line = top;
    while line <= bottom {
        let row = &term.grid()[line];
        let mut text = String::new();
        for col in 0..cols {
            let cell = &row[Column(col)];
            if cell
                .flags
                .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
            {
                continue;
            }
            text.push(cell.c);
            if let Some(z) = cell.zerowidth() {
                text.extend(z);
            }
        }
        let wrapped = row[Column(cols - 1)].flags.contains(Flags::WRAPLINE);
        if wrapped {
            out.push_str(&text);
        } else {
            out.push_str(text.trim_end());
            out.push('\n');
        }
        line += 1;
    }
    let trimmed = out.trim_end_matches(['\n', ' ']).len();
    out.truncate(trimmed);
    out
}

/// 한 화면이 담는 OSC 8 링크 수와 주소 길이의 상한
const MAX_LINKS: usize = 4096;
const MAX_LINK_URI: usize = 8192;

fn snapshot_of<T: EventListener>(
    term: &Term<T>,
    theme: &Theme,
    search: Option<&mut crate::search::Search>,
    graphics: Option<&mut crate::graphics::Graphics>,
) -> Screen {
    let cols = term.columns();
    let rows = term.screen_lines();
    let content = term.renderable_content();
    let overrides = content.colors;
    let background = color::named(theme, overrides, NamedColor::Background);
    let foreground = color::named(theme, overrides, NamedColor::Foreground);
    let mut cells = vec![Cell::blank(foreground, background); cols * rows];
    let offset = content.display_offset as i32;
    let selection = content.selection;
    // 찾기: 보이는 맞은 곳들과 지금 것. 칸을 위에서 아래로 훑으며 맞춘다
    let (matches, current) = match search {
        Some(s) => (s.visible(term), s.current.clone()),
        None => (Vec::new(), None),
    };
    let mut match_cursor = 0;
    let mut links: Vec<String> = Vec::new();
    // 주소 → 자리. 칸마다 다른 링크를 찍어도 그릴 때마다 칸 수 × 링크 수를 비교하지 않게(#54)
    let mut link_ids: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

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
        if !matches.is_empty() && crate::search::hit(&matches, &mut match_cursor, indexed.point) {
            flags.insert(CellFlags::MATCH);
            if current.as_ref().is_some_and(|m| m.contains(&indexed.point)) {
                flags.insert(CellFlags::MATCH_CURRENT);
            }
        }
        if f.contains(Flags::WRAPLINE) {
            flags.insert(CellFlags::WRAPLINE);
        }
        // OSC 8 링크: 같은 주소는 한 번만 담는다. 너무 긴 주소와 상한을 넘는 링크는 링크로 보지 않는다
        let link = src.hyperlink().and_then(|h| {
            let uri = h.uri();
            if uri.len() > MAX_LINK_URI {
                return None;
            }
            if let Some(&i) = link_ids.get(uri) {
                return Some(i);
            }
            if links.len() >= MAX_LINKS {
                return None;
            }
            let i = links.len() as u32;
            links.push(uri.to_string());
            link_ids.insert(uri.to_string(), i);
            Some(i)
        });
        let combining = src.zerowidth().map(|z| z.to_vec()).unwrap_or_default();
        cells[row as usize * cols + col] = Cell {
            ch: src.c,
            combining,
            fg,
            bg,
            flags,
            link,
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
        // 프로그램이 DECSCUSR로 정한 깜빡임, 없으면 설정(cursor-style-blink)
        blinking: term.cursor_style().blinking,
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
        links,
        images: graphics.map(|g| visible_images(term, g)).unwrap_or_default(),
    }
}

/// kitty 그래픽 명령을 처리한다: 그림을 저장·놓고, 놓은 만큼 VT 커서를 옮기고(줄을 확보), 답을 돌려준다
fn graphics_command<T: EventListener>(
    term: &mut Term<T>,
    parser: &mut Processor,
    graphics: &mut crate::graphics::Graphics,
    cell: (u32, u32),
    raw: &[u8],
) -> Option<Vec<u8>> {
    let cur = crate::graphics::Cursor {
        line: term.grid().history_size() as i64 + term.grid().cursor.point.line.0 as i64,
        col: term.grid().cursor.point.column.0,
        cell_w: cell.0,
        cell_h: cell.1,
        alt: term.mode().contains(TermMode::ALT_SCREEN),
    };
    let out = graphics.handle(raw, cur);
    if let Some((down, right)) = out.advance {
        // 그림이 차지한 줄만큼 줄을 내린다(맨 아래면 스크롤된다). 그다음 그림 오른쪽 칸으로
        let mut seq = "\n".repeat(down as usize);
        if right > 0 {
            seq.push_str(&format!("\x1b[{right}C"));
        }
        parser.advance(term, seq.as_bytes());
    }
    out.reply
}

/// 지금 보이는 줄에 걸친 kitty 그림들. 대체 화면을 나왔으면 그 화면의 그림을 지운다
fn visible_images<T>(term: &Term<T>, g: &mut crate::graphics::Graphics) -> Vec<crate::screen::ImageView> {
    let alt = term.mode().contains(TermMode::ALT_SCREEN);
    if !alt {
        g.left_alt_screen();
    }
    let top = term.grid().history_size() as i64 - term.grid().display_offset() as i64;
    let rows = term.screen_lines() as i64;
    let cols = term.columns() as i64;
    // 그릴 양의 상한(보안 검토): 최근 것부터, 같은 자리의 중복은 빼고, 보이는 칸 넓이의 합이 화면 칸 수의 4배까지
    let budget = 4 * rows * cols;
    let mut used = 0i64;
    let mut seen = std::collections::HashSet::new();
    let mut picked: Vec<&crate::graphics::Placement> = Vec::new();
    for p in g.placements.iter().rev().filter(|p| p.alt == alt) {
        let row = p.line - top;
        if row + p.rows as i64 <= 0 || row >= rows || !seen.insert((p.image, p.line, p.col, p.cols, p.rows)) {
            continue;
        }
        let visible_rows = (row + p.rows as i64).min(rows) - row.max(0);
        let visible_cols = (p.col as i64 + p.cols as i64).min(cols) - (p.col as i64).min(cols);
        let area = visible_rows.max(0) * visible_cols.max(0);
        if used + area > budget {
            break;
        }
        used += area;
        picked.push(p);
    }
    picked.reverse();
    picked
        .into_iter()
        .filter_map(|p| {
            let row = p.line - top;
            if row + p.rows as i64 <= 0 || row >= rows {
                return None;
            }
            let img = g.image(p.image)?;
            Some(crate::screen::ImageView {
                rgba: img.rgba.clone(),
                width: img.width,
                height: img.height,
                row: row as i32,
                col: p.col,
                cols: p.cols,
                rows: p.rows,
                scaled: p.scaled,
            })
        })
        .collect()
}

/// PTY 없이 바이트를 VT로 해석해 화면을 얻는다. 렌더러 테스트와 b-studio 세션 터미널(바이트가 HTTP로 오는 칸)에 쓴다
pub struct Emulator {
    term: Term<NullListener>,
    parser: Processor,
    theme: Theme,
    scanner: Scanner,
    search: Option<crate::search::Search>,
    graphics: crate::graphics::Graphics,
    cell: (u32, u32),
    /// 그래픽 명령의 답(PTY가 없으므로 모아 둔다)
    replies: Vec<u8>,
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
            search: None,
            graphics: Default::default(),
            cell: (size.cell_width.max(1) as u32, size.cell_height.max(1) as u32),
            replies: Vec::new(),
        }
    }

    /// 지금까지 모은 그래픽 명령의 답을 꺼낸다
    pub fn take_replies(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.replies)
    }

    /// 바이트를 먹이고 그 안의 신호(알림·폴더·진행)를 돌려준다
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Signal> {
        // 세션의 읽기 스레드와 같이 신호가 끝난 자리까지 나눠 먹인다(그래픽은 그 자리의 커서에 놓인다)
        let mut pos = 0;
        let mut notes = Vec::new();
        for (end, sig) in self.scanner.feed_at(bytes) {
            let end = end.min(bytes.len());
            self.parser.advance(&mut self.term, &bytes[pos..end]);
            pos = end;
            match &sig {
                Signal::Graphics(raw) => {
                    if let Some(r) =
                        graphics_command(&mut self.term, &mut self.parser, &mut self.graphics, self.cell, raw)
                    {
                        self.replies.extend(r);
                    }
                }
                Signal::Erase { visible, scrollback } => self.graphics.erase(*visible, *scrollback),
                _ => {}
            }
            notes.push(sig);
        }
        self.parser.advance(&mut self.term, &bytes[pos..]);
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

    pub fn snapshot(&mut self) -> Screen {
        snapshot_of(&self.term, &self.theme, self.search.as_mut(), Some(&mut self.graphics))
    }

    /// 찾기를 시작한다(맨 아래에서 위로 첫 맞은 곳). 빈 글이면 찾기를 끄고 없다
    pub fn search(&mut self, query: &str) -> Option<crate::search::SearchStatus> {
        self.search = crate::search::Search::new(query);
        let s = self.search.as_mut()?;
        s.step(&mut self.term, true);
        Some(s.status(&self.term))
    }

    pub fn search_step(&mut self, older: bool) -> Option<crate::search::SearchStatus> {
        let s = self.search.as_mut()?;
        s.step(&mut self.term, older);
        Some(s.status(&self.term))
    }

    pub fn search_clear(&mut self) {
        self.search = None;
    }

    pub fn tail_text(&self, lines: usize) -> String {
        tail_text(&self.term, lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjusted_point_moves_and_wraps_like_ghostty() {
        use SelectionAdjust::*;
        let (top, bottom) = (Line(-5), Line(9));
        let at = |l: i32, c: usize| Point::new(Line(l), Column(c));
        let adj = |p, a| adjusted_point(p, a, 10, top, bottom, 10);
        assert_eq!(adj(at(2, 3), Right), at(2, 4));
        assert_eq!(adj(at(2, 9), Right), at(3, 0), "줄 끝에서 다음 줄로");
        assert_eq!(adj(at(2, 0), Left), at(1, 9), "줄 처음에서 앞 줄 끝으로");
        assert_eq!(adj(at(9, 9), Right), at(9, 9), "맨 끝에서는 그대로");
        assert_eq!(adj(at(-5, 0), Left), at(-5, 0));
        assert_eq!(adj(at(2, 3), Up), at(1, 3));
        assert_eq!(adj(at(9, 3), Down), at(9, 3));
        assert_eq!(adj(at(2, 3), PageUp), at(-5, 3), "스크롤백 맨 위에서 멈춤");
        assert_eq!(adj(at(-4, 3), PageDown), at(6, 3));
        assert_eq!(adj(at(2, 3), Home), at(-5, 0));
        assert_eq!(adj(at(2, 3), End), at(9, 9));
        assert_eq!(adj(at(2, 3), BeginningOfLine), at(2, 0));
        assert_eq!(adj(at(2, 3), EndOfLine), at(2, 9));
    }
}

#[cfg(test)]
mod resize_tests {
    use super::window_resize_target;

    #[test]
    fn zero_keeps_current_and_small_sizes_are_raised() {
        let cur = (120, 40);
        let max = (300, 100);
        assert_eq!(window_resize_target((100, 30), cur, max), (100, 30));
        assert_eq!(window_resize_target((0, 30), cur, max), (120, 30));
        assert_eq!(window_resize_target((100, 0), cur, max), (100, 40));
        assert_eq!(window_resize_target((0, 0), cur, max), cur);
        // 40열 × 10행 아래는 그 크기로 올린다
        assert_eq!(window_resize_target((2, 1), cur, max), (40, 10));
        assert_eq!(window_resize_target((39, 9), cur, max), (40, 10));
        assert_eq!(window_resize_target((40, 10), cur, max), (40, 10));
        // 화면보다 크면 줄인다
        assert_eq!(window_resize_target((9999, 9999), cur, max), max);
        assert_eq!(window_resize_target((301, 50), cur, max), (300, 50));
    }
}
