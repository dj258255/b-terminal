//! PTY 하나와 그 VT 상태(ADR-001).
//!
//! 읽기 스레드가 PTY 바이트를 받아 같은 순서로 VT 해석기에 먹인다. alacritty의 EventLoop를 쓰지 않는 것은
//! 바이트를 가로채 알림(OSC 9/99/777)을 볼 자리가 필요하기 때문이다

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use alacritty_terminal::event::{Event as VtEvent, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Config, Osc52, Term, TermMode};
use alacritty_terminal::vte::ansi::{
    CursorShape as VtCursorShape, CursorStyle as VtCursorStyle, NamedColor, Processor, Rgb as VtRgb,
};
use anyhow::Context as _;
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
    /// 엔진에 넘길 수 있는 최소 크기(2열·1줄, 셀 픽셀 1)로 올린다. 필드가 공개라 0이 들어올 수 있다(Ghostty는 0을 거절한다, Terminal.zig 5274·5302줄.
    /// `Emulator::new`는 오류를 돌려줄 수 없고 `Session::resize`는 전부터 올렸으므로 하나로 맞춰 올린다)
    fn clamped(self) -> Self {
        Self {
            cols: self.cols.max(2),
            rows: self.rows.max(1),
            cell_width: self.cell_width.max(1),
            cell_height: self.cell_height.max(1),
        }
    }

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
const MOUSE_OFF: &[u8] = b"\x1b[?9l\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1005l\x1b[?1006l\x1b[?1015l\x1b[?1016l";

/// 띄울 프로그램
#[derive(Debug, Clone, Default)]
pub struct SpawnOptions {
    /// 없으면 `$SHELL`(없으면 `/bin/zsh`)을 로그인 셸로 띄운다
    pub program: Option<String>,
    /// 트리거(설정 trigger, #928). 정규식에 맞는 출력 줄에 강조·알림·벨·탭 표시만 한다
    pub triggers: Vec<bt_core::trigger::TriggerSpec>,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// 더하거나 덮을 환경 변수
    pub env: Vec<(String, String)>,
    /// TERM 값(설정 term, #190). 비어 있으면 `xterm-256color`
    pub term: String,
    /// 칸을 띄운 직후 PTY에 보낼 바이트(설정 input을 풀어 둔 것, #190)
    pub input: Vec<u8>,
    pub scrollback: usize,
    /// 프로그램이 DECSCUSR로 바꾸기 전의 커서 모양(설정 cursor-style)
    pub cursor_shape: Option<CursorShape>,
    pub cursor_blink: bool,
    /// OSC 52 쓰기·읽기 사건을 앱에 넘길지(설정 clipboard-write·clipboard-read가 deny가 아님). 그중 allow·ask를 가르는 것은 앱이다(ADR-019)
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
    /// 처음 칸의 색 구성표가 밝음인가(기본은 어두움). `CSI ? 996 n`의 답과 모드 2031 알림의 기준(#232)
    pub color_scheme_light: bool,
    /// 글자 묶음 폭을 세는 방식(설정 grapheme-width-method, #180). `Unicode`면 mode 2027이 처음부터 켜져 있다
    pub grapheme_width_method: bt_core::config::GraphemeWidthMethod,
    /// 동아시아 모호 폭 글자를 두 칸으로 셀지(설정 `ambiguous-width = wide`, #857). 기본 끔
    pub ambiguous_wide: bool,
    /// DECRQCRA(화면 영역 체크섬 묻기)에 답할지(설정 vt-xt-checksum-report, 기본 끔, #259). 꺼져 있으면 XTCHECKSUM도 무시한다
    pub vt_xt_checksum_report: bool,
    /// 체크섬 계산 방식의 기본(설정 vt-xt-checksum-extension, #259). 프로그램이 XTCHECKSUM으로 바꾼 것은 초기화(RIS·DECSTR)에서 이 값으로 돌아온다
    pub vt_xt_checksum_extension: bt_core::config::XtChecksumExtension,
    /// kitty 클립보드 쓰기 한 번의 상한(설정 clipboard-write-limit-bytes, `None`은 unlimited). 고정 1MiB보다 크게 늘리지 못한다(#325)
    pub clipboard_write_limit_bytes: Option<usize>,
    /// 창 상태 복원(#861)이 되살릴 지난 화면의 줄들. 셸을 띄우기 전에 스크롤백 맨 아래에 놓는다(VT 파서를 거치지 않고 격자에 직접, ADR-092)
    pub restore_lines: Vec<crate::saved_screen::SavedLine>,
    /// 되살린 줄들 끝에 붙일 흐린 안내 줄(저장한 때). 줄이 없으면 쓰지 않는다
    pub restore_note: Option<String>,
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

/// 읽기 스레드가 세션 잠금을 쥔 채 그림 명령에 쓴 시간이 이만큼 쌓이면 기다리는 쪽(화면 그리기)에 잠금을 넘긴다(#557).
/// 한 번 읽은 덩어리를 다 처리할 때까지 쥐고 있으면, 무거운 그림 명령 몇 개(반투명 합성, 큰 그림 풀기)로 앱 전체가 멈춘다
const GRAPHICS_YIELD_AFTER: Duration = Duration::from_millis(4);

/// 그림 명령 하나에 든 시간을 더하고, 잠금을 넘길 때가 되었으면 센 시간을 비우고 참을 돌려준다
fn graphics_yield_due(spent: &mut Duration, took: Duration) -> bool {
    *spent += took;
    if *spent < GRAPHICS_YIELD_AFTER {
        return false;
    }
    *spent = Duration::ZERO;
    true
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
    /// 빈 OSC 7: 작업 폴더를 모른다고 되돌린다(#328)
    CwdReset,
    /// 진행 표시(OSC 9;4). 상태 0은 지움
    Progress {
        state: u8,
        percent: Option<u8>,
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
    /// 프로그램이 kitty 클립보드 프로토콜(OSC 5522)로 클립보드를 읽으려 한다(#325). 정책을 보고 답하는 것은 앱이다
    KittyClipboardRead(crate::kitty_clip::ReadRequest),
    /// 프로그램이 kitty 클립보드 프로토콜로 쓰기 거래를 끝냈다(#325). 상한은 이미 지났고 정책은 앱이 본다
    KittyClipboardWrite(crate::kitty_clip::WriteRequest),
    /// 칸의 프로그램 상태 기록(OSC 7501)이 바뀌었다(#618). 기록이 새로 `blocked`·`done`·`error`가 되었으면 그 값이 함께 온다
    ProgramStatus(Option<crate::program_status::Alert>),
    /// 트리거 `attention`(#928): 이 칸의 탭에 주의 표시(안 읽음)를 단다. 소리도 알림도 아니다
    TriggerAttention,
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

/// 시작 입력을 PTY에 쓴다. 아무도 읽지 않는 큰 입력이 칸 띄우기를 붙잡지 않게 다른 스레드에서 쓰고, 쓰는 동안
/// 잠금을 오래 쥐지 않게 작은 덩어리로 나눈다(터미널의 응답이 막히지 않게). 쓰기가 실패하면(칸이 끝남) 멈춘다
fn send_startup_input(writer: &Writer, data: Vec<u8>, readonly: Arc<std::sync::atomic::AtomicBool>) {
    let writer = writer.clone();
    let spawned = std::thread::Builder::new()
        .name("bt-startup-input".into())
        .spawn(move || write_startup_chunks(&writer, &data, &readonly));
    if let Err(e) = spawned {
        log::warn!("시작 입력을 쓸 스레드를 만들지 못했습니다: {e}");
    }
}

/// 시작 입력을 1KiB 덩어리로 쓴다. 쓰는 도중 칸을 읽기 전용으로 하면(`toggle_readonly`, ADR-018) 남은 덩어리는 쓰지 않는다
fn write_startup_chunks(writer: &Writer, data: &[u8], readonly: &std::sync::atomic::AtomicBool) {
    for chunk in data.chunks(1024) {
        if readonly.load(std::sync::atomic::Ordering::Acquire) {
            log::info!("칸을 읽기 전용으로 해 남은 시작 입력을 쓰지 않았습니다");
            break;
        }
        let mut w = writer.lock();
        if w.write_all(chunk).and_then(|()| w.flush()).is_err() {
            break;
        }
    }
}

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
            // 엔진이 사건에 실어 준 색(프로그램이 OSC 4·10·11·12·21로 바꾼 값)이 있으면 그것으로, 없으면 테마로 답한다(#272).
            // 이 사건은 읽기 스레드가 엔진을 잠근 채 보내므로 여기서 `Term::colors()`를 읽을 수 없다
            VtEvent::ColorRequest(index, changed, changed_fg, format) => {
                if self.color_format == OscColorReportFormat::None {
                    return;
                }
                let theme = &self.theme;
                let rgb = changed.map_or_else(
                    || match index {
                        i if i < 256 => theme.indexed(i as u8),
                        i if i == NamedColor::Foreground as usize => theme.foreground,
                        i if i == NamedColor::Background as usize => theme.background,
                        // 커서 색을 정하지 않았으면 지금 글자색(#286). 설정이 커서 색을 따로 정했으면 그 색
                        _ => theme.cursor_or(changed_fg.map_or(theme.foreground, |c| bt_core::Rgb::new(c.r, c.g, c.b))),
                    },
                    |c| bt_core::Rgb::new(c.r, c.g, c.b),
                );
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
            // OSC 52 읽기: 설정 clipboard-read가 deny가 아닐 때만 엔진이 이 사건을 낸다(ask면 앱이 묻는다). 앱이 클립보드를 읽어 답한다
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
    /// 명령마다 끝난 결과(종료 코드·걸린 시간, #927). 줄의 안정된 번호로 곁에 둔다
    marks: crate::command_marks::CommandMarks,
    /// 찾기(#39). 찾는 중이 아니면 없다
    search: Option<crate::search::Search>,
    /// kitty 마커(#929). 없으면 마크를 칠하지 않는다
    marker: Option<crate::marks::MarkerState>,
    /// 트리거(#928). 없으면 줄을 검사하지 않는다
    triggers: Option<crate::triggers::Triggers>,
    /// kitty 그래픽(#76): 저장한 그림과 놓인 자리
    graphics: crate::graphics::Graphics,
    /// 글리프 프로토콜(APC 25a1)의 이 칸 등록(#779). 그리지는 않는다(Ghostty 1.4.0과 같다). 전체 초기화(RIS)에서 지운다
    glossary: crate::glyph::Glossary,
    /// 셀 크기(px). 그림을 칸에 맞출 때 쓴다
    cell: (u32, u32),
    /// 픽셀 스크롤(#902)의 걸친 픽셀. 줄 단위 스크롤·크기 바뀜에서 지운다
    pixel: crate::pixel_scroll::PixelScroll,
    /// 고르기의 움직이는 끝점(#138 adjust_selection). 고르기를 시작·늘이면 바뀐다
    sel_end: Option<Point>,
    /// 명령 출력 고르기(#700)를 시작한 칸: (스크롤백 맨 위에서 센 줄, 칸). 출력이 흘러 줄이 밀려도 같은 줄을 가리킨다.
    /// 있으면 끌기가 출력 구간 단위로 늘린다. 다른 고르기를 시작하거나 지우면 없어진다
    output_click: Option<(usize, usize)>,
}

/// 격자 좌표를 스크롤백 맨 위에서 센 (줄, 칸)으로(출력이 흘러 스크롤백이 늘어도 같은 줄)
fn absolute_cell(term: &Term<Listener>, p: Point) -> (usize, usize) {
    let history = term.grid().history_size() as i32;
    ((p.line.0 + history).max(0) as usize, p.column.0)
}

/// `absolute_cell`의 반대. 지금 격자 밖이면 None
fn from_absolute_cell(term: &Term<Listener>, (row, col): (usize, usize)) -> Option<Point> {
    let history = term.grid().history_size() as i32;
    let p = Point::new(Line(row as i32 - history), Column(col));
    crate::output_select::inside(term, p).then_some(p)
}

/// 두 칸(둘 다 포함)을 글자 고르기로 고르고 끝점을 `end`로 둔다
fn set_range(g: &mut Inner, start: Point, end: Point) {
    let mut sel = Selection::new(SelectionType::Simple, start, alacritty_terminal::index::Side::Left);
    sel.update(end, alacritty_terminal::index::Side::Right);
    g.term.selection = Some(sel);
    g.sel_end = Some(end);
}

/// 절대 줄(스크롤백 맨 위가 0) `row`가 화면 맨 위에 오게 한다. 범위를 넘으면 맨 아래
fn scroll_term_to_row(term: &mut Term<Listener>, row: usize) {
    let history = term.grid().history_size();
    let offset = term.grid().display_offset();
    let want = history.saturating_sub(row);
    term.scroll_display(Scroll::Delta(want as i32 - offset as i32));
}

/// 스크롤백을 위로(양수) `delta` 픽셀 올린다(#902). 줄 높이 `ch` 단위의 `display_offset`과 걸친 픽셀로 나눠 반영한다.
/// 위치가 바뀌었으면 참. 대체 화면(스크롤백이 없다)이나 움직일 데가 없으면 거짓
fn scroll_term_pixels<T: EventListener>(
    px: &mut crate::pixel_scroll::PixelScroll,
    term: &mut Term<T>,
    delta: f64,
    ch: u32,
) -> bool {
    let history = term.grid().history_size();
    let offset = term.grid().display_offset();
    let before = (offset, px.sub(ch, history, offset));
    let Some(step) = px.plan(delta, ch, history, offset) else {
        return false;
    };
    if step.offset != offset {
        term.scroll_display(Scroll::Delta(step.offset as i32 - offset as i32));
    }
    let now = term.grid().display_offset();
    // 엔진이 줄 수를 다르게 잘랐으면 그 줄에서 걸친 픽셀은 쓰지 않는다
    px.set(if now == step.offset { step.sub } else { 0.0 }, history, now);
    before != (now, px.sub(ch, history, now))
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
    /// 읽은 바이트를 VT에 먹인다. 마우스 보고 모드(X10·버튼·움직임)를 켜거나 끄면 프로그램이 앞서 OSC 22로 정한 모양을 지운다.
    /// Ghostty가 모드를 켜고 끌 때 모양을 `default`·`text`로 되돌리는 것(stream_handler.zig)에 해당하고, b-terminal의 기본은
    /// 화살표라 둘 다 "정한 것 없음"이다(#250). 모드를 바꾼 바이트 뒤에 오는 OSC 22는 신호로 따로 오므로 남는다
    fn advance_tracking_mouse(&mut self, bytes: &[u8], shape: &std::sync::atomic::AtomicU8) {
        let before = self.term.mode().intersects(TermMode::MOUSE_MODE);
        let Inner { term, parser, .. } = self;
        parser.advance(term, bytes);
        if before != term.mode().intersects(TermMode::MOUSE_MODE) {
            shape.store(0, std::sync::atomic::Ordering::Release);
        }
    }

    /// 새 프롬프트(`133;A`)의 줄을 명령 기억에 더한다(#927). 대체 화면의 표지는 주 화면 줄이 아니라 더하지 않는다
    fn mark_prompt(&mut self) {
        if self.term.mode().contains(TermMode::ALT_SCREEN) {
            return;
        }
        self.marks.prune(crate::command_marks::oldest_id(&self.term));
        let id = crate::command_marks::stable_id(&self.term, self.term.grid().cursor.point.line);
        self.marks.prompt(id);
    }

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
        sync_graphics(&mut self.term, &mut self.graphics, self.cell);
        self.graphics.erase(visible, scrollback);
        if visible && !scrollback {
            erase_screen_graphics(&self.term, &mut self.graphics);
        }
    }
}

impl Inner {
    /// `clear_scrollback`의 엔진 몫(#863). 고른 끝점·명령 출력 고르기를 풀고 남은 줄의 프롬프트 번호를 맞춘다
    fn clear_scrollback(&mut self) -> bool {
        let Inner {
            term,
            parser,
            graphics,
            cell,
            prompts,
            ..
        } = self;
        if !clear_scrollback_op(term, parser, graphics, *cell, Some(prompts)) {
            return false;
        }
        self.sel_end = None;
        self.output_click = None;
        true
    }

    /// `clear_last_command`의 엔진 몫(#863)
    fn clear_last_command(&mut self) -> bool {
        let Inner {
            term,
            parser,
            graphics,
            cell,
            prompts,
            marks,
            ..
        } = self;
        // 지울 구간 맨 위 줄의 안정된 번호. 그 줄부터 아래는 줄이 당겨지니 기억을 버린다(지운 명령과 지금 프롬프트의 것)
        let cut = crate::output_select::last_command_block(term)
            .map(|(start, _)| crate::command_marks::stable_id(term, start));
        if !clear_last_command_op(term, parser, graphics, *cell, Some(prompts)) {
            return false;
        }
        if let Some(cut) = cut {
            marks.truncate_from(cut);
        }
        self.sel_end = None;
        self.output_click = None;
        true
    }

    /// ⌘K(`clear_screen`)의 엔진 몫(#821). 고른 끝점·명령 출력 고르기·프롬프트 기억도 함께 맞춘다
    fn clear_screen(&mut self) -> ClearScreen {
        let before = self.term.grid().history_size() as i64;
        let cursor_before = self.term.grid().cursor.point.line.0 as i64;
        let Inner {
            term,
            parser,
            graphics,
            cell,
            ..
        } = self;
        let out = clear_screen_op(term, parser, graphics, *cell);
        if out == ClearScreen::Ignored {
            return out;
        }
        self.sel_end = None;
        self.output_click = None;
        // 스크롤백이 사라지고 커서 윗줄이 떨어져 나가 절대 줄 번호가 밀린다. 남은 줄의 프롬프트만 새 번호로 옮긴다
        self.marks.clear();
        let shift = before + if out == ClearScreen::Cleared { cursor_before } else { 0 };
        let rows = self.term.screen_lines() as i64;
        let prompts = std::mem::take(&mut self.prompts);
        self.prompts = prompts
            .into_iter()
            .filter_map(|p| Some(p - shift).filter(|&n| n >= 0 && n < rows))
            .collect();
        out
    }
}

/// `Session::word_at`의 결과: 낱말 글과 시작 칸(화면 좌표)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordHit {
    pub text: String,
    pub col: usize,
    pub row: usize,
}

/// PTY 하나와 그 화면 상태
pub struct Session {
    inner: Arc<Mutex<Inner>>,
    writer: Writer,
    /// 키 입력 → 화면 지연 표(#417). 재기가 꺼져 있으면 건드리지 않는다
    latency: Arc<Mutex<crate::latency::LatencyTracker>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    size: Arc<Mutex<GridSize>>,
    theme: Arc<Mutex<Arc<Theme>>>,
    pid: Option<u32>,
    exited: Arc<Mutex<Option<Option<u32>>>>,
    /// 떨어뜨릴 때 자식에게 SIGHUP을 보내는 손잡이(#651). 읽기 스레드가 `child`를 가져가므로 복제해 둔다
    killer: Arc<Mutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>>,
    /// 셸 통합 표지(OSC 133;A)를 한 번이라도 받았다
    integrated: Arc<std::sync::atomic::AtomicBool>,
    /// 새 출력이 오면 맨 아래로 스크롤하는가(읽기 스레드가 본다)
    scroll_on_output: Arc<std::sync::atomic::AtomicBool>,
    /// 읽기 스레드가 출력을 받은 횟수(탭의 활동 표시 `tab-activity-symbol`, #910). 앱이 앞에 본 값과 달라졌으면 새 출력이 있었다
    output_seq: Arc<std::sync::atomic::AtomicU64>,
    /// 프로그램이 키보드를 잠갔다(KAM, vt-kam-allowed가 켜졌을 때만). 잠겨 있으면 사람의 입력을 PTY에 쓰지 않는다
    kam_locked: Arc<std::sync::atomic::AtomicBool>,
    /// 사람이 칸을 읽기 전용으로 했다(`toggle_readonly`, ADR-018). 켜져 있으면 `write`가 아무것도 쓰지 않는다
    readonly: Arc<std::sync::atomic::AtomicBool>,
    /// 프로그램이 색 구성표 변경 알림(모드 2031)을 켰다(#232)
    scheme_updates: Arc<std::sync::atomic::AtomicBool>,
    /// 칸에 알린 지금 색 구성표가 밝음인가(#232)
    scheme_light: Arc<std::sync::atomic::AtomicBool>,
    /// 프로그램이 OSC 22로 정한 마우스 포인터 모양의 번호(`MouseShape::to_code`, 0은 정한 것 없음)(#239)
    mouse_shape: Arc<std::sync::atomic::AtomicU8>,
    /// 프로그램이 OSC 7501로 알린 상태 기록(#618). 읽기 스레드가 쓰고 앱이 읽는다
    program_status: Arc<Mutex<crate::program_status::Records>>,
    /// 프로그램이 크기 알림(모드 2048)을 켜 두었는가(#264)
    in_band_size: Arc<std::sync::atomic::AtomicBool>,
    /// 이 칸이 보이는가. 앱이 알리기 전에는 보인다고 본다(Ghostty `Surface.visible` 기본값 true)(#303)
    visible: Arc<std::sync::atomic::AtomicBool>,
    /// 프로그램이 가시성 보고(모드 2033)를 켜 두었는가(#303)
    visibility_reports: Arc<std::sync::atomic::AtomicBool>,
    /// 프로그램이 붙여넣기 이벤트(모드 5522)를 켜 두었는가(#325)
    kitty_paste: Arc<std::sync::atomic::AtomicBool>,
    /// kitty 클립보드 프로토콜의 암호 허가. 붙여넣기 이벤트가 일회용 읽기 허가를 더하고 읽기 스레드가 쓴다(#325)
    kitty_grants: Arc<Mutex<crate::kitty_clip::Grants>>,
}

impl Session {
    /// 색 구성표(시스템 모양)가 바뀌었음을 알린다. 모드 2031이 켜져 있고 값이 달라졌을 때만 `CSI ? 997 ; 1|2 n`을
    /// PTY에 쓴다. 터미널의 답이라 읽기 전용 칸에서도 쓴다(ADR-018 결정 4). 같은 값이 또 오면 아무것도 하지 않는다(#232)
    pub fn set_color_scheme(&self, dark: bool) {
        use std::sync::atomic::Ordering::{AcqRel, Acquire};
        let light = !dark;
        let changed = self.scheme_light.swap(light, AcqRel) != light;
        if changed && self.scheme_updates.load(Acquire) {
            let mut w = self.writer.lock();
            let _ = w.write_all(scheme_report(light));
            let _ = w.flush();
        }
    }

    /// 이 칸이 보이는지 알린다. 값이 달라졌고 모드 2033이 켜져 있을 때만 `CSI ? 999 ; 1|2 n`을 PTY에 쓴다
    /// (Ghostty `Surface.occlusionCallback`, 3437~3463줄). 터미널의 답이라 읽기 전용 칸에서도 쓴다(#303)
    pub fn set_visible(&self, visible: bool) {
        use std::sync::atomic::Ordering::{AcqRel, Acquire};
        let changed = self.visible.swap(visible, AcqRel) != visible;
        if changed && self.visibility_reports.load(Acquire) {
            write_reply(&self.writer, crate::csi_query::visibility_report(visible).as_bytes());
        }
    }

    /// 이 칸이 보이는가(알리지 않았으면 보인다고 본다)(#303)
    pub fn visible(&self) -> bool {
        self.visible.load(std::sync::atomic::Ordering::Acquire)
    }

    /// 프로그램이 크기 알림(모드 2048)을 켜 두었는가(#264)
    pub fn in_band_size_reports(&self) -> bool {
        self.in_band_size.load(std::sync::atomic::Ordering::Acquire)
    }

    /// 프로그램이 색 구성표 변경 알림(모드 2031)을 켜 두었는가
    pub fn color_scheme_updates(&self) -> bool {
        self.scheme_updates.load(std::sync::atomic::Ordering::Acquire)
    }

    /// 프로그램이 OSC 22로 정한 마우스 포인터 모양. 정한 것이 없으면 `None`(#239)
    pub fn mouse_shape(&self) -> Option<crate::MouseShape> {
        crate::MouseShape::from_code(self.mouse_shape.load(std::sync::atomic::Ordering::Acquire))
    }

    /// 프로그램이 붙여넣기 이벤트(모드 5522)를 켜 두었는가(#325)
    pub fn kitty_paste_mode(&self) -> bool {
        self.kitty_paste.load(std::sync::atomic::Ordering::Acquire)
    }

    /// 사람이 붙여넣을 때 모드 5522가 켜져 있으면 글 대신 종류 목록과 일회용 암호를 프로그램에 보낸다(#325).
    /// 보냈으면 true, 모드가 꺼져 있거나 난수를 못 얻었으면 false(그때는 아무것도 보내지 않는다)
    pub fn kitty_paste_event(&self, available: &[String]) -> bool {
        if !self.kitty_paste_mode() {
            return false;
        }
        let Some(otp) = crate::kitty_clip::generate_otp() else {
            log::warn!("붙여넣기 이벤트: 운영체제 난수를 얻지 못해 보내지 않습니다");
            return false;
        };
        // 허가는 이벤트가 프로그램에 닿기 전에 적는다. 한 번만 쓰이고 표준 클립보드 읽기에만 듣는다
        self.kitty_grants.lock().grant(&otp, crate::kitty_clip::Dir::Read, true);
        let bytes = crate::kitty_clip::paste_event(false, &otp, available);
        let mut w = self.writer.lock();
        if let Err(e) = w.write_all(&bytes).and_then(|_| w.flush()) {
            log::debug!("pty write failed: {e}");
            drop(w);
            self.kitty_grants.lock().consume(&otp, crate::kitty_clip::Dir::Read);
            return false;
        }
        true
    }

    /// 이 칸이 읽기 전용인가
    pub fn readonly(&self) -> bool {
        self.readonly.load(std::sync::atomic::Ordering::Acquire)
    }

    /// 칸을 읽기 전용으로 하거나 푼다
    pub fn set_readonly(&self, on: bool) {
        self.readonly.store(on, std::sync::atomic::Ordering::Release);
    }

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

    /// 지금 보이는 `row`줄에 붙은 명령 표시(#927, 마우스를 올렸을 때 종료 코드·걸린 시간을 보인다). 화면 전체를 풀지 않는다
    pub fn command_mark_at(&self, row: usize) -> Option<crate::command_marks::RowMark> {
        let g = self.inner.lock();
        crate::command_marks::visible(&g.term, &g.marks, g.term.screen_lines())
            .into_iter()
            .find(|m| m.row == row)
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
    pub fn spawn(mut options: SpawnOptions, size: GridSize, theme: Theme, sink: EventSink) -> anyhow::Result<Self> {
        let pty = native_pty_system();
        let pair = open_pty_with_retry(|| pty.openpty(size.pty()), OPENPTY_BACKOFF, std::thread::sleep)?;

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
        cmd.env(
            "TERM",
            if options.term.is_empty() {
                "xterm-256color"
            } else {
                &options.term
            },
        );
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

        let what = format!(
            "프로그램 띄우기: {} (작업 폴더 {})",
            options.program.as_deref().unwrap_or("로그인 셸"),
            options
                .cwd
                .as_ref()
                .map_or_else(|| "홈".to_string(), |c| c.display().to_string()),
        );
        let mut child = pair.slave.spawn_command(cmd).context(what)?;
        drop(pair.slave);
        let pid = child.process_id();
        let killer = Arc::new(Mutex::new(child.clone_killer()));
        let mut reader = pair.master.try_clone_reader().context("PTY 읽기 끝 복제")?;
        let writer: Writer = Arc::new(Mutex::new(pair.master.take_writer().context("PTY 쓰기 끝 얻기")?));
        let readonly = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if !options.input.is_empty() {
            send_startup_input(&writer, std::mem::take(&mut options.input), readonly.clone());
        }

        let theme = Arc::new(theme);
        // 읽기 스레드(OSC 21 질의)도 설정을 다시 읽은 뒤의 테마를 본다
        let current_theme = Arc::new(Mutex::new(theme.clone()));
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
            grapheme_cluster: options.grapheme_width_method == bt_core::config::GraphemeWidthMethod::Unicode,
            ambiguous_wide: options.ambiguous_wide,
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
        let mut term = Term::new(config, &Dims(size), listener);
        // 지난 화면(#861): 읽기 스레드가 돌기 전, 화면이 비어 있을 때 놓는다
        if !options.restore_lines.is_empty() {
            crate::saved_screen::restore(&mut term, &options.restore_lines, options.restore_note.as_deref());
        }
        // 트리거(#928): 띄운 직후의 빈 화면부터 본다
        let triggers = new_triggers(&options.triggers).0.map(|mut t| {
            t.arm(&term);
            t
        });
        let inner = Arc::new(Mutex::new(Inner {
            term,
            parser: Processor::new(),
            prompts: std::collections::VecDeque::new(),
            marks: Default::default(),
            search: None,
            marker: None,
            triggers,
            graphics: crate::graphics::Graphics::with_limit(
                options
                    .image_storage_limit
                    .map_or(crate::graphics::DEFAULT_STORE, |n| n as usize),
            ),
            glossary: crate::glyph::Glossary::default(),
            cell: (size.cell_width.max(1) as u32, size.cell_height.max(1) as u32),
            pixel: Default::default(),
            sel_end: None,
            output_click: None,
        }));
        let integrated = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread_integrated = integrated.clone();
        let scroll_on_output = Arc::new(std::sync::atomic::AtomicBool::new(options.scroll_on_output));
        let thread_scroll_on_output = scroll_on_output.clone();
        let output_seq = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let thread_output_seq = output_seq.clone();
        let kam_locked = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread_kam_locked = kam_locked.clone();
        let exited = Arc::new(Mutex::new(None));
        let scheme_updates = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let scheme_light = Arc::new(std::sync::atomic::AtomicBool::new(options.color_scheme_light));

        let mouse_shape = Arc::new(std::sync::atomic::AtomicU8::new(0));
        let thread_mouse_shape = mouse_shape.clone();
        let program_status = Arc::new(Mutex::new(crate::program_status::Records::default()));
        let thread_status = program_status.clone();
        let thread_scheme_updates = scheme_updates.clone();
        let thread_scheme_light = scheme_light.clone();
        let in_band_size = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread_in_band_size = in_band_size.clone();
        let visible = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let thread_visible = visible.clone();
        let visibility_reports = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread_visibility_reports = visibility_reports.clone();
        let kitty_paste = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread_kitty_paste = kitty_paste.clone();
        let kitty_grants = Arc::new(Mutex::new(crate::kitty_clip::Grants::default()));
        // 쓰기 한 번의 상한은 OSC 52와 같은 규칙: 고정 1MiB와 설정 중 작은 쪽(ADR-013). 모으기 전에 본다
        let kitty_max_write = options
            .clipboard_write_limit_bytes
            .map_or(bt_core::config::OSC52_WRITE_CAP, |l| {
                l.min(bt_core::config::OSC52_WRITE_CAP)
            });
        let mut kitty_engine =
            crate::kitty_clip::Engine::new(kitty_grants.clone(), options.clipboard_write, kitty_max_write);
        let thread_kitty_grants = kitty_grants.clone();
        let thread_size = size_cell.clone();
        let da1_clipboard = options.clipboard_write;
        let thread_inner = inner.clone();
        let latency: Arc<Mutex<crate::latency::LatencyTracker>> = Arc::default();
        let thread_latency = latency.clone();
        let thread_exited = exited.clone();
        let thread_writer = writer.clone();
        let thread_title = title.clone();
        let title_report = options.title_report;
        let enquiry_response = options.enquiry_response.clone();
        let vt_kam_allowed = options.vt_kam_allowed;
        let vt_window_resize_allowed = options.vt_window_resize_allowed;
        // XTGETTCAP의 TN은 자식에게 준 TERM과 같은 값이다(#265)
        let dcs_tn = if options.term.is_empty() {
            "xterm-256color".to_string()
        } else {
            options.term.clone()
        };
        let xt_report = options.vt_xt_checksum_report;
        let thread_theme = current_theme.clone();
        let xt_default = crate::xt_checksum::Flags::from_config(&options.vt_xt_checksum_extension);
        std::thread::Builder::new().name("bt-pty-read".into()).spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            let mut scanner = Scanner::default();
            // 셸 통합: C(실행)부터 D(끝)까지의 출력과 그 명령
            let mut capture: Option<(Vec<u8>, Instant)> = None;
            let mut pending_cmd: Option<String> = None;
            // 지금 체크섬 계산 방식(XTCHECKSUM). 읽기 스레드만 쓴다
            let mut xt_flags = xt_default;
            // XTSAVE로 저장한 스캐너 모드(2031·2033·2048)의 값. 엔진이 모르는 모드라 여기서 쥔다(#313)
            let mut saved_modes: [Option<bool>; 4] = [None; 4];
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
                            let mut graphics_spent = Duration::ZERO;
                            // 신호가 끝난 자리까지 나눠 먹인다: 프롬프트 줄을 정확히 적고, 출력을 C와 D 사이에서 자른다
                            for (end, sig) in signals {
                                let seg = &bytes[pos..end.min(bytes.len())];
                                g.advance_tracking_mouse(seg, &thread_mouse_shape);
                                if let Some((cap, _)) = capture.as_mut() {
                                    push_capped(cap, seg);
                                }
                                pos = end.min(bytes.len());
                                // XTSAVE·XTRESTORE(#313): 저장은 지금 값을 적어 두고, 복원은 DECSET·DECRST와 같은 신호로 바꿔 같은 길을 탄다.
                                // 저장한 적이 없으면 꺼짐(Ghostty modes.zig 기본값)
                                let sig = match sig {
                                    Signal::ModeSave(m) => {
                                        use std::sync::atomic::Ordering::Acquire;
                                        let (slot, on) = match m {
                                            2031 => (0, thread_scheme_updates.load(Acquire)),
                                            2033 => (1, thread_visibility_reports.load(Acquire)),
                                            5522 => (3, thread_kitty_paste.load(Acquire)),
                                            _ => (2, thread_in_band_size.load(Acquire)),
                                        };
                                        saved_modes[slot] = Some(on);
                                        continue;
                                    }
                                    Signal::ModeRestore(m) => match m {
                                        2031 => Signal::ColorSchemeUpdates(saved_modes[0].unwrap_or(false)),
                                        2033 => Signal::VisibilityReports(saved_modes[1].unwrap_or(false)),
                                        5522 => Signal::KittyPasteEvents(saved_modes[3].unwrap_or(false)),
                                        _ => Signal::InBandSizeReports(saved_modes[2].unwrap_or(false)),
                                    },
                                    other => other,
                                };
                                match sig {
                                    // OSC 133 확장(#329): 줄 표시·fresh-line·지우기 규칙을 엔진에 넣는다
                                    Signal::Semantic(sp) => sp.apply(&mut g.term),
                                    Signal::Prompt(PromptMark::PromptStart) => {
                                        // 새 셸 프롬프트: 일하던 프로그램은 끝났다(OSC 7501). done·error는 남는다
                                        if thread_status.lock().drop_live() {
                                            events.push(SessionEvent::ProgramStatus(None));
                                        }
                                        thread_integrated.store(true, std::sync::atomic::Ordering::Release);
                                        g.mark_prompt();
                                        let line = g.cursor_abs();
                                        if g.prompts.back() != Some(&line) {
                                            g.prompts.push_back(line);
                                            if g.prompts.len() > MAX_PROMPTS {
                                                g.prompts.pop_front();
                                            }
                                        }
                                    }
                                    Signal::Prompt(PromptMark::CommandExecuted) => {
                                        g.marks.executed(Instant::now());
                                        capture = Some((Vec::new(), Instant::now()));
                                        events.push(SessionEvent::CommandStarted(pending_cmd.clone()));
                                    }
                                    Signal::Prompt(PromptMark::CommandFinished(code)) => {
                                        g.marks.finished(code, Instant::now());
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
                                        let started = Instant::now();
                                        let reply = g.graphics_command(&raw);
                                        if let Some(reply) = reply {
                                            let mut w = thread_writer.lock();
                                            let _ = w.write_all(&reply);
                                            let _ = w.flush();
                                        }
                                        // 무거운 명령이 이어지면 화면 그리기가 끼어들 틈을 준다. 기다리는 쪽이 없으면 곧바로 다시 쥔다(#557)
                                        if graphics_yield_due(&mut graphics_spent, started.elapsed()) {
                                            parking_lot::MutexGuard::unlocked_fair(&mut g, || {});
                                        }
                                    }
                                    // 글리프 프로토콜(#779): 등록을 검사해 칸에 두고 답한다. 터미널의 답이라 읽기 전용 칸에서도 쓴다(ADR-018 결정 4).
                                    // 답은 숫자와 고정 이름으로 다시 쓴 글이라 요청의 글이 PTY로 돌아가지 않는다(ADR-078)
                                    Signal::Glyph(raw) => {
                                        if let Some(reply) = g.glossary.handle(&raw) {
                                            let mut w = thread_writer.lock();
                                            let _ = w.write_all(&reply);
                                            let _ = w.flush();
                                        }
                                    }
                                    Signal::Erase { visible, scrollback } => {
                                        g.graphics_erase(visible, scrollback);
                                        // 초기화(RIS)는 키보드 잠금도 푼다
                                        if visible && scrollback {
                                            g.glossary.clear_all();
                                            g.marks.clear();
                                            crate::kitty_color::reset_palette(&mut g.term);
                                            xt_flags = xt_default;
                                            saved_modes = [None; 4];
                                            // 초기화는 붙여넣기 이벤트 모드를 끄고 암호 허가를 지운다(Ghostty `fullReset`)(#325)
                                            thread_kitty_paste.store(false, std::sync::atomic::Ordering::Release);
                                            thread_kitty_grants.lock().clear();
                                            thread_kam_locked.store(false, std::sync::atomic::Ordering::Release);
                                            thread_scheme_updates.store(false, std::sync::atomic::Ordering::Release);
                                            thread_in_band_size.store(false, std::sync::atomic::Ordering::Release);
                                            thread_visibility_reports
                                                .store(false, std::sync::atomic::Ordering::Release);
                                            thread_mouse_shape.store(0, std::sync::atomic::Ordering::Release);
                                            // 진행 막대도 지운다(Ghostty `fullReset` 처리, stream_terminal.zig 751줄)(#548)
                                            events.push(SessionEvent::Progress {
                                                state: 0,
                                                percent: None,
                                            });
                                            // 프로그램 상태 기록도 모두 지운다(DECSTR은 지우지 않는다)(#618)
                                            thread_status.lock().clear_all();
                                            events.push(SessionEvent::ProgramStatus(None));
                                        }
                                    }
                                    // 마우스 포인터 모양(OSC 22): 칸이 들고 있고, 보여 주는 일은 앱이 한다. 빈 이름은 되돌린다(#239)
                                    Signal::MouseShape(shape) => {
                                        let code = shape.map_or(0, |s| s.to_code());
                                        thread_mouse_shape.store(code, std::sync::atomic::Ordering::Release);
                                    }
                                    // DECRQCRA: 설정이 켜졌을 때만 그 자리의 격자로 계산해 답한다(ADR-021). 터미널의 답이라 읽기 전용 칸에서도 쓴다(ADR-018 결정 4)
                                    Signal::XtChecksumReport(req) => {
                                        if xt_report {
                                            let sum = crate::xt_checksum::compute(&g.term, req, xt_flags);
                                            let mut w = thread_writer.lock();
                                            let _ = w.write_all(&crate::xt_checksum::encode(req.id, sum));
                                            let _ = w.flush();
                                        }
                                    }
                                    // XTCHECKSUM: 보고가 꺼져 있으면 계산법도 바꾸지 않는다
                                    Signal::XtChecksumExtension(bits) => {
                                        if xt_report {
                                            xt_flags = crate::xt_checksum::Flags::from_bits(bits as u16);
                                        }
                                    }
                                    Signal::SoftReset => {
                                        xt_flags = xt_default;
                                        crate::kitty_color::soft_reset(&mut g.term);
                                    }
                                    // DCS 질의(DECRQSS, XTGETTCAP): 터미널의 상태와 정해진 목록에서 만든 답이고 요청 글은 들어가지 않는다.
                                    // 터미널의 답이라 읽기 전용 칸에서도 쓴다(ADR-018 결정 4, ADR-023)
                                    Signal::DcsQuery(query) => {
                                        let reply = match query {
                                            crate::dcs::Query::Decrqss(d) => crate::dcs::encode_decrqss(d, &g.term),
                                            crate::dcs::Query::XtGetTcap(body) => {
                                                crate::dcs::xtgettcap_reply(&body, &dcs_tn)
                                            }
                                        };
                                        if !reply.is_empty() {
                                            let mut w = thread_writer.lock();
                                            let _ = w.write_all(&reply);
                                            let _ = w.flush();
                                        }
                                    }
                                    Signal::ColorSchemeUpdates(on) => {
                                        thread_scheme_updates.store(on, std::sync::atomic::Ordering::Release);
                                    }
                                    // 색 구성표 질문: 터미널의 답이라 읽기 전용 칸에서도 쓴다(ADR-018 결정 4)
                                    Signal::ColorSchemeQuery => {
                                        let light = thread_scheme_light.load(std::sync::atomic::Ordering::Acquire);
                                        let mut w = thread_writer.lock();
                                        let _ = w.write_all(scheme_report(light));
                                        let _ = w.flush();
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
                                    // CSI 질의(#264, ADR-022): 답은 고정 바이트와 검사한 숫자뿐이다. 터미널의 답이라 읽기 전용 칸에서도 쓴다(ADR-018 결정 4)
                                    Signal::Da1 => {
                                        let reply = if da1_clipboard {
                                            crate::csi_query::DA1_CLIPBOARD
                                        } else {
                                            crate::csi_query::DA1_PLAIN
                                        };
                                        write_reply(&thread_writer, reply.as_bytes());
                                    }
                                    // DECDSR 고정 답(#513). 터미널의 답이라 읽기 전용 칸에서도 쓴다. 엔진은 `? Ps n`(6 빼고)에 답하지 않아 한 번만 나간다
                                    Signal::DecDsr(r) => {
                                        write_reply(&thread_writer, crate::csi_query::dec_status_report(r).as_bytes())
                                    }
                                    Signal::Da2 => write_reply(&thread_writer, crate::csi_query::DA2.as_bytes()),
                                    Signal::XtVersion => {
                                        write_reply(&thread_writer, crate::csi_query::xtversion().as_bytes())
                                    }
                                    Signal::SizeReport(style) => {
                                        let info = size_info(*thread_size.lock());
                                        write_reply(
                                            &thread_writer,
                                            crate::csi_query::encode_size(style, info).as_bytes(),
                                        );
                                    }
                                    // 모드 2048을 켜면 바로 한 번 보내고, 그 뒤로는 크기가 바뀔 때마다 `Session::resize`가 보낸다
                                    Signal::InBandSizeReports(on) => {
                                        thread_in_band_size.store(on, std::sync::atomic::Ordering::Release);
                                        if on {
                                            let info = size_info(*thread_size.lock());
                                            let reply = crate::csi_query::encode_size(
                                                crate::csi_query::SizeStyle::Mode2048,
                                                info,
                                            );
                                            write_reply(&thread_writer, reply.as_bytes());
                                        }
                                    }
                                    Signal::ColorSchemeModeQuery => {
                                        let on = thread_scheme_updates.load(std::sync::atomic::Ordering::Acquire);
                                        write_reply(&thread_writer, crate::csi_query::mode_2031_report(on).as_bytes());
                                    }
                                    // 모드 2033을 켜면 지금 상태를 강제로 한 번 보고하고, 그 뒤로는 `Session::set_visible`이 바뀔 때마다 보낸다
                                    // (Ghostty `stream_handler.zig` 764줄)
                                    Signal::VisibilityReports(on) => {
                                        thread_visibility_reports.store(on, std::sync::atomic::Ordering::Release);
                                        if on {
                                            let now = thread_visible.load(std::sync::atomic::Ordering::Acquire);
                                            write_reply(
                                                &thread_writer,
                                                crate::csi_query::visibility_report(now).as_bytes(),
                                            );
                                        }
                                    }
                                    Signal::KittyPasteEvents(on) => {
                                        thread_kitty_paste.store(on, std::sync::atomic::Ordering::Release);
                                    }
                                    Signal::KittyPasteQuery => {
                                        let on = thread_kitty_paste.load(std::sync::atomic::Ordering::Acquire);
                                        write_reply(&thread_writer, crate::csi_query::mode_5522_report(on).as_bytes());
                                    }
                                    // kitty 클립보드(OSC 5522): 해석·상한·거래는 엔진이, 정책과 클립보드는 앱이 한다.
                                    // 엔진의 즉석 답(EPERM·EFBIG·EINVAL)은 터미널의 답이라 읽기 전용 칸에서도 쓴다(#325)
                                    Signal::KittyClipboard { packet, bel } => {
                                        for action in kitty_engine.handle(&packet, bel) {
                                            match action {
                                                crate::kitty_clip::Action::Reply(bytes) => {
                                                    write_reply(&thread_writer, &bytes)
                                                }
                                                crate::kitty_clip::Action::Read(r) => {
                                                    events.push(SessionEvent::KittyClipboardRead(r))
                                                }
                                                crate::kitty_clip::Action::Write(w) => {
                                                    events.push(SessionEvent::KittyClipboardWrite(w))
                                                }
                                            }
                                        }
                                    }
                                    Signal::VisibilityQuery => {
                                        let on = thread_visibility_reports.load(std::sync::atomic::Ordering::Acquire);
                                        write_reply(&thread_writer, crate::csi_query::mode_2033_report(on).as_bytes());
                                    }
                                    // 보이는지 묻기(`CSI ? 998 n`): 모드 2033이 꺼져 있어도 답한다(Ghostty stream_terminal.zig
                                    // `sendVisibilityReport`). 터미널의 답이라 읽기 전용 칸에서도 쓴다(#675)
                                    Signal::VisibilityStatusQuery => {
                                        let now = thread_visible.load(std::sync::atomic::Ordering::Acquire);
                                        write_reply(
                                            &thread_writer,
                                            crate::csi_query::visibility_report(now).as_bytes(),
                                        );
                                    }
                                    Signal::InBandSizeQuery => {
                                        let on = thread_in_band_size.load(std::sync::atomic::Ordering::Acquire);
                                        write_reply(&thread_writer, crate::csi_query::mode_2048_report(on).as_bytes());
                                    }
                                    // kitty 색 프로토콜(OSC 21): 설정·되돌리기는 OSC 4·10·11·12와 같은 자리(엔진의 바꾼 색)에 넣고,
                                    // 질의의 답은 터미널의 답이라 읽기 전용 칸에서도 쓴다. osc-color-report-format은 보지 않는다
                                    // (Ghostty Config.zig: `none`은 OSC 4/10/11 질의에만 해당, #266)
                                    Signal::KittyColor { requests, bel } => {
                                        let theme = thread_theme.lock().clone();
                                        let reply = crate::kitty_color::apply(&mut g.term, &theme, &requests, bel);
                                        if let Some(reply) = reply {
                                            let mut w = thread_writer.lock();
                                            let _ = w.write_all(&reply);
                                            let _ = w.flush();
                                        }
                                    }
                                    // kitty 알림(OSC 99)의 지원 질의 답(#561)
                                    Signal::KittyNotifyReply(reply) => write_reply(&thread_writer, &reply),
                                    // 프로그램 상태(OSC 7501): 지원 질의의 답은 터미널의 답이라 읽기 전용 칸에서도 쓴다(ADR-018 결정 4).
                                    // 답에는 기록의 내용이 들어가지 않는다(#618)
                                    Signal::ProgramStatusQuery { bel } => {
                                        write_reply(&thread_writer, &crate::program_status::query_reply(bel))
                                    }
                                    Signal::ProgramStatus(report) => {
                                        let alert = thread_status.lock().apply(*report);
                                        events.push(SessionEvent::ProgramStatus(alert));
                                    }
                                    other => events.push(signal_event(other)),
                                }
                            }
                            let rest = &bytes[pos..];
                            g.advance_tracking_mouse(rest, &thread_mouse_shape);
                            if let Some((cap, _)) = capture.as_mut() {
                                push_capped(cap, rest);
                            }
                            // 이 읽기에서 쌓인 스크롤 기록을 그림 자리에 반영한다(#800). 기록이 한 읽기(64KiB)를 넘어 쌓이지 않게 한다
                            let Inner {
                                term, graphics, cell, ..
                            } = &mut *g;
                            sync_graphics(term, graphics, *cell);
                            // 트리거(#928): 이 읽기에서 새로 찍힌 줄만 본다. 하는 일은 알림·벨·탭 표시 사건뿐이고 PTY에는 쓰지 않는다
                            let Inner { term, triggers, .. } = &mut *g;
                            if let Some(t) = triggers.as_mut() {
                                events.extend(t.scan(term, Instant::now()).into_iter().map(trigger_event));
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
                        if crate::latency::enabled() {
                            thread_latency.lock().note_output(Instant::now());
                        }
                        thread_output_seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        sink(SessionEvent::Output);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            // 거두기(wait)와 끝남 표시를 `exited` 잠금 안에서 한다. `Drop`도 같은 잠금 안에서 신호를 보내므로,
            // 이미 거둔 pid가 다른 프로세스에 재사용된 뒤 신호가 가는 일이 없다(#651).
            // 막는 `wait` 대신 `try_wait`을 짧게 되풀이한다(읽기가 끝난 뒤에만 도니 비용은 작다)
            let code = loop {
                let mut slot = thread_exited.lock();
                match child.try_wait() {
                    Ok(Some(s)) => {
                        let code = Some(s.exit_code());
                        *slot = Some(code);
                        break code;
                    }
                    Ok(None) => {}
                    Err(_) => {
                        *slot = Some(None);
                        break None;
                    }
                }
                drop(slot);
                std::thread::sleep(Duration::from_millis(10));
            };
            // 붙어 있던 프로세스가 끝났다: 일하던 상태는 사라지고 done·error는 남는다(OSC 7501)
            if thread_status.lock().drop_live() {
                sink(SessionEvent::ProgramStatus(None));
            }
            sink(SessionEvent::Exited(code));
        })?;

        Ok(Self {
            inner,
            writer,
            latency,
            master: Mutex::new(pair.master),
            size: size_cell,
            theme: current_theme,
            pid,
            exited,
            killer,
            integrated,
            scroll_on_output,
            output_seq,
            kam_locked,
            readonly,
            scheme_updates,
            scheme_light,
            mouse_shape,
            program_status,
            in_band_size,
            visible,
            visibility_reports,
            kitty_paste,
            kitty_grants,
        })
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// 전경 프로세스 그룹의 pid(가볍다: ioctl 한 번)
    pub fn foreground_pid(&self) -> Option<i32> {
        self.master.lock().process_group_leader()
    }

    /// PTY 장치 경로(예: `/dev/ttys016`). 모르면 `None`(AppleScript `tty`, #672)
    pub fn tty_name(&self) -> Option<PathBuf> {
        self.master.lock().tty_name()
    }

    /// 암호를 치는 중인가(#258). PTY의 termios가 줄 단위 입력(`ICANON`)이고 되울림(`ECHO`)이 꺼졌으면 그렇다고 본다
    /// (Ghostty termio/Exec.zig의 같은 추측). 읽지 못하면 `None`
    pub fn password_input(&self) -> Option<bool> {
        #[cfg(unix)]
        {
            use nix::sys::termios::LocalFlags;
            let t = self.master.lock().get_termios()?;
            Some(is_password_mode(
                t.local_flags.contains(LocalFlags::ICANON),
                t.local_flags.contains(LocalFlags::ECHO),
            ))
        }
        #[cfg(not(unix))]
        {
            None
        }
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

    /// b-terminal(#324): VT 엔진 격자의 실제 (열, 줄). 보통 `size()`와 같지만 DECCOLM(모드 40을 켠 뒤 `CSI ? 3 h`)이면 열이 132·80이다.
    /// 마우스 좌표·전체 선택은 이 값으로 자른다
    pub fn grid_cells(&self) -> (usize, usize) {
        let g = self.inner.lock();
        (g.term.columns(), g.term.screen_lines())
    }

    /// PTY에 바이트를 그대로 쓴다. 보던 자리는 건드리지 않는다(응답·초점 알림·마우스 보고 같은 것)
    /// 읽기 전용 칸(`toggle_readonly`)에는 아무것도 쓰지 않는다. 앱이 PTY에 쓰는 길은 모두 여기를 지나므로
    /// (키·붙여넣기·IME·마우스·초점·`text:` 동작·`btctl send`) 막는 곳은 이 한 곳이다. 프로그램이 물은 것에 터미널이
    /// 답하는 쓰기(DSR·DA·색 질문·창 크기)는 `Listener`가 따로 쓰므로 막지 않는다. OSC 52 클립보드 읽기의 답은
    /// 앱이 클립보드를 읽어 이 함수로 쓰므로 읽기 전용이면 막힌다(클립보드 내용이 새지 않는 쪽)
    pub fn write(&self, bytes: &[u8]) {
        if bytes.is_empty() || self.readonly() {
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
        if bytes.is_empty() || self.keyboard_locked() || self.readonly() {
            return;
        }
        if scroll {
            self.scroll_to_bottom();
        }
        self.note_input();
        self.write(bytes);
    }

    /// 사람 입력(키·IME·붙여넣기·`btctl send`)을 PTY에 쓰기 직전의 때를 적는다(입력 → 화면 지연, #417).
    /// 이미 적혀 있으면 덮지 않는다. 재기가 꺼져 있으면(debug 로그가 아니면) 아무것도 하지 않는다
    pub fn note_input(&self) {
        if crate::latency::enabled() {
            self.latency.lock().note_input(Instant::now());
        }
    }

    /// 프레임이 칸의 화면을 뜨기 전에 부른다. 입력 뒤 출력이 이미 온 칸이면 값을 돌려준다
    pub fn latency_begin_frame(&self, now: Instant) -> Option<crate::latency::Sample> {
        if !crate::latency::enabled() {
            return None;
        }
        self.latency.lock().begin_frame(now)
    }

    /// 그 프레임이 실제로 올라갔다. 걸린 시간을 돌려주고 시각을 지운다
    pub fn latency_presented(&self, sample: &crate::latency::Sample, now: Instant) -> Option<crate::latency::Report> {
        self.latency.lock().presented(sample, now)
    }

    /// 새 출력이 오면 맨 아래로 스크롤할지 바꾼다(설정을 다시 읽을 때)
    /// 설정을 다시 읽었을 때 모호 폭 설정을 바꾼다(#857). 이미 찍힌 글은 그대로 두고 이 뒤에 찍는 글부터 따른다
    pub fn set_ambiguous_wide(&self, on: bool) {
        self.inner.lock().term.set_ambiguous_wide(on);
    }

    /// 지금까지 받은 출력 묶음의 수. 앞에 본 값과 다르면 그 사이에 새 출력이 있었다
    pub fn output_seq(&self) -> u64 {
        self.output_seq.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn set_scroll_on_output(&self, on: bool) {
        self.scroll_on_output.store(on, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn resize(&self, size: GridSize) {
        // 셀 픽셀 크기는 그대로 둔다: PTY 창 크기(TIOCGWINSZ의 픽셀)와 그림 칸 맞춤이 쓴다.
        // 전에는 GridSize::new로 다시 만들어 8×16으로 바뀌었다(#76에서 고침)
        let size = size.clamped();
        {
            let mut cur = self.size.lock();
            if *cur == size {
                return;
            }
            *cur = size;
        }
        {
            let mut g = self.inner.lock();
            // 동기 출력(?2026)을 푼다(Ghostty `Terminal.resize`, Terminal.zig 5263줄). 모아 둔 글은 크기를 바꾸기 전에 먹인다
            // (Ghostty는 글을 모으지 않아 이미 옛 크기 화면에 있다)
            let Inner {
                term,
                parser,
                graphics,
                cell,
                ..
            } = &mut *g;
            release_sync(term, parser);
            sync_graphics(term, graphics, *cell); // 옛 크기에서 일어난 스크롤을 먼저 반영한다(#800)
            g.term.resize(Dims(size));
            let Inner { term, marks, .. } = &mut *g;
            realign_marks(term, marks);
            g.cell = (size.cell_width as u32, size.cell_height as u32);
            g.pixel.reset();
        }
        if let Err(e) = self.master.lock().resize(size.pty()) {
            log::debug!("pty resize failed: {e}");
        }
        // 모드 2048: 크기가 바뀔 때마다 알린다(Ghostty termio/Termio.zig 550~595줄). 같은 크기는 위에서 이미 돌아갔다
        if self.in_band_size.load(std::sync::atomic::Ordering::Acquire) {
            let reply = crate::csi_query::encode_size(crate::csi_query::SizeStyle::Mode2048, size_info(size));
            write_reply(&self.writer, reply.as_bytes());
        }
    }

    /// 스크롤백을 줄 단위로 움직인다(양수는 위로)
    pub fn scroll(&self, lines: i32) {
        let mut g = self.inner.lock();
        g.pixel.reset();
        g.term.scroll_display(Scroll::Delta(lines));
    }

    pub fn scroll_to_bottom(&self) {
        let mut g = self.inner.lock();
        g.pixel.reset();
        g.term.scroll_display(Scroll::Bottom);
    }

    /// 스크롤백을 위로(양수) `delta` 픽셀 올린다(픽셀 스크롤, kitty `pixel_scroll`, #902). 줄보다 작은 값도 쌓인다.
    /// 위치가 바뀌었으면 참(대체 화면이나 스크롤백이 없으면 거짓)
    pub fn scroll_pixels(&self, delta: f64) -> bool {
        let mut g = self.inner.lock();
        let ch = g.cell.1;
        let Inner { term, pixel, .. } = &mut *g;
        scroll_term_pixels(pixel, term, delta, ch)
    }

    /// 지금 화면을 위로 밀어 그릴 걸친 픽셀(`Screen::pixel_shift`와 같다). 마우스 좌표를 칸으로 바꿀 때 쓴다
    pub fn pixel_shift(&self) -> u32 {
        let g = self.inner.lock();
        let grid = g.term.grid();
        g.pixel.shift(g.cell.1, grid.history_size(), grid.display_offset())
    }

    /// 걸친 픽셀을 지운다(설정에서 픽셀 스크롤을 껐을 때). 줄 위치는 그대로다
    pub fn end_pixel_scroll(&self) {
        self.inner.lock().pixel.reset();
    }

    /// 탭에 보일 가장 급한 프로그램 상태(OSC 7501, #618). 없으면 `None`
    pub fn program_status(&self) -> Option<crate::program_status::Summary> {
        self.program_status.lock().summary()
    }

    /// 사용자가 본 `done`·`error` 기록을 지운다(#618). 지운 것이 있으면 참
    pub fn clear_seen_program_status(&self) -> bool {
        self.program_status.lock().clear_seen()
    }

    /// 터미널을 초기화한다(Ghostty `reset`): VT의 RIS(`ESC c`)를 엔진에 먹인다. PTY에는 아무것도 보내지 않는다.
    /// 읽기 스레드가 RIS를 볼 때 하는 일(그림 지우기, 키보드 잠금 풀기)과 프롬프트 기억·고른 끝점도 함께 비운다
    pub fn reset(&self) {
        {
            let mut g = self.inner.lock();
            let Inner { term, parser, .. } = &mut *g;
            parser.advance(term, b"\x1bc");
            g.graphics_erase(true, true);
            g.glossary.clear_all();
            g.prompts.clear();
            g.marks.clear();
            g.sel_end = None;
        }
        self.program_status.lock().clear_all();
        self.kam_locked.store(false, std::sync::atomic::Ordering::Release);
        self.scheme_updates.store(false, std::sync::atomic::Ordering::Release);
        self.in_band_size.store(false, std::sync::atomic::Ordering::Release);
        self.visibility_reports
            .store(false, std::sync::atomic::Ordering::Release);
        self.mouse_shape.store(0, std::sync::atomic::Ordering::Release);
    }

    /// ⌘K(`clear_screen`)의 엔진 몫(#821, Ghostty `Termio.clearScreen`). `Redraw`이면 부르는 쪽이 PTY에 FF를 쓴다.
    /// `Ignored`(대체 화면)이면 아무것도 바꾸지 않는다
    pub fn clear_screen(&self) -> ClearScreen {
        self.inner.lock().clear_screen()
    }

    /// kitty `clear_terminal scrollback`(#863): 스크롤백만 지우고 보이는 화면은 그대로 둔다. 대체 화면이면 하지 않고 거짓
    pub fn clear_scrollback(&self) -> bool {
        self.inner.lock().clear_scrollback()
    }

    /// kitty `clear_terminal last_command`(#863): 지금 프롬프트 바로 위 명령(프롬프트·입력·출력)을 지우고 아래를 끌어올린다.
    /// 셸 통합이 없거나 대체 화면이거나 지울 명령이 없으면 하지 않고 거짓
    pub fn clear_last_command(&self) -> bool {
        self.inner.lock().clear_last_command()
    }

    /// 스크롤 막대를 그리고 누르는 데 쓰는 값: (보이는 줄 수, 스크롤백 줄 수, 맨 아래에서 올라간 줄 수).
    /// 대체 화면은 스크롤백이 없어 0이다. 화면 전체를 풀지 않아 마우스 움직임마다 불러도 가볍다
    pub fn scroll_metrics(&self) -> (usize, usize, usize) {
        let g = self.inner.lock();
        let grid = g.term.grid();
        (g.term.screen_lines(), grid.history_size(), grid.display_offset())
    }

    /// 스크롤백 맨 위를 0으로 센 절대 줄 `row`가 화면 맨 위에 오게 스크롤한다(Ghostty `scroll_to_row`).
    /// 맨 아래보다 아래거나 범위를 넘으면 맨 아래에서 멈춘다
    pub fn scroll_to_row(&self, row: usize) {
        let mut g = self.inner.lock();
        g.pixel.reset();
        scroll_term_to_row(&mut g.term, row);
    }

    /// 고른 곳이 보이게 스크롤한다(Ghostty `scroll_to_selection`). 시작 줄이 이미 보이면 움직이지 않고,
    /// 아니면 그 줄이 화면 맨 위에 오게 한다. 고른 것이 없으면 아무것도 하지 않고 거짓
    pub fn scroll_to_selection(&self) -> bool {
        let mut g = self.inner.lock();
        let term = &mut g.term;
        let Some(range) = term.selection.as_ref().and_then(|s| s.to_range(term)) else {
            return false;
        };
        let history = term.grid().history_size();
        let top = history - term.grid().display_offset();
        let row = (range.start.line.0 + history as i32).max(0) as usize;
        if row < top || row >= top + term.screen_lines() {
            scroll_term_to_row(term, row);
        }
        true
    }

    /// 찾기를 시작한다(#39): 바늘만 정하고 아무것도 고르지 않으며 화면도 옮기지 않는다(#824). 빈 글이면 찾기를 끄고 없다
    pub fn search(&self, query: &str) -> Option<crate::search::SearchStatus> {
        let mut g = self.inner.lock();
        let Inner { term, search, .. } = &mut *g;
        *search = crate::search::Search::new(query);
        let s = search.as_mut()?;
        Some(s.status(term))
    }

    /// 트리거를 바꾼다(설정 다시 읽기, #928). 못 읽는 규칙은 빼고 이유를 돌려준다. 규칙이 없으면 줄 검사를 하지 않는다
    pub fn set_triggers(&self, specs: &[bt_core::trigger::TriggerSpec]) -> Vec<String> {
        let (mut t, errors) = new_triggers(specs);
        let mut g = self.inner.lock();
        if let Some(t) = t.as_mut() {
            t.arm(&g.term);
        }
        g.triggers = t;
        errors
    }

    /// kitty `toggle_marker`(#929): 같은 명세면 끄고(`Ok(false)`), 아니면 바꿔 켠다(`Ok(true)`). 정규식을 못 읽으면 오류
    pub fn toggle_marker(&self, spec: &crate::marks::MarkerSpec) -> Result<bool, String> {
        crate::marks::toggle(&mut self.inner.lock().marker, spec)
    }

    /// kitty `remove_marker`: 마커가 있었으면 참
    pub fn remove_marker(&self) -> bool {
        self.inner.lock().marker.take().is_some()
    }

    pub fn has_marker(&self) -> bool {
        self.inner.lock().marker.is_some()
    }

    /// kitty `scroll_to_mark`: 이전(`prev`)·다음 마크 줄로. `mark`는 0(아무)~3. 옮겼으면 참
    pub fn scroll_to_mark(&self, prev: bool, mark: u8) -> bool {
        self.scroll_to_mark_within(prev, mark, crate::marks::SCROLL_SCAN_LINES)
    }

    /// `scroll_to_mark`에 훑을 줄 수 상한을 준다(시험용)
    pub fn scroll_to_mark_within(&self, prev: bool, mark: u8, max_lines: usize) -> bool {
        let mut g = self.inner.lock();
        let Some(m) = g.marker.as_ref() else { return false };
        let Some(n) = crate::marks::scroll_amount(&g.term, m.marker(), prev, mark, max_lines) else {
            return false;
        };
        g.pixel.reset();
        g.term.scroll_display(Scroll::Delta(n));
        true
    }

    /// 다음 맞은 곳으로(`older`면 위로). 찾는 중이 아니면 없다
    pub fn search_step(&self, older: bool) -> Option<crate::search::SearchStatus> {
        let mut g = self.inner.lock();
        let Inner { term, search, .. } = &mut *g;
        let s = search.as_mut()?;
        s.step(term, older);
        Some(s.status(term))
    }

    /// 지금 찾기 결과(몇 번째, 모두 몇 개)를 다시 센다. 찾는 중이 아니면 없다
    pub fn search_status(&self) -> Option<crate::search::SearchStatus> {
        let mut g = self.inner.lock();
        let Inner { term, search, .. } = &mut *g;
        let s = search.as_mut()?;
        Some(s.status(term))
    }

    /// 화면이 지난번 센 뒤로 바뀌었으면 다시 세어 돌려준다. 안 바뀌었거나 찾는 중이 아니면 없다(#833)
    pub fn search_refresh(&self) -> Option<crate::search::SearchStatus> {
        let mut g = self.inner.lock();
        let Inner { term, search, .. } = &mut *g;
        search.as_mut()?.refresh(term)
    }

    /// 스크롤백 전체를 센 횟수(시험용). 찾는 중이 아니면 0
    pub fn search_recounts(&self) -> usize {
        self.inner.lock().search.as_ref().map_or(0, |s| s.recounts())
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
        if !g.term.mode().intersects(TermMode::MOUSE_MODE | TermMode::MOUSE_FORMAT) {
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

    /// 커서가 셸 프롬프트에 있는가(셸 통합의 OSC 133 표시, Ghostty `cursorIsAtPrompt`와 같은 판단). 대체 화면이거나 셸 통합이
    /// 없으면 아니다. 붙여넣기 동작 `quote-urls-at-prompt`(#866)가 본다
    pub fn cursor_at_prompt(&self) -> bool {
        cursor_is_at_prompt(&self.inner.lock().term)
    }

    /// 프롬프트 클릭(#338)이 PTY에 보낼 바이트. 셸이 `cl=`·`click_events=`를 밝힌 화면에서, 프롬프트에서 눌렀을 때만 있다
    pub fn prompt_click(&self, col: usize, row: usize) -> Option<Vec<u8>> {
        crate::prompt_click::prompt_click(&self.inner.lock().term, col, row)
    }

    /// 화면 좌표 칸의 낱말과 그 시작 칸(사전 조회, #853). 고른 글은 건드리지 않는다. 칸이 비었으면 None
    pub fn word_at(&self, col: usize, row: usize) -> Option<WordHit> {
        let theme = self.theme.lock().clone();
        let g = self.inner.lock();
        let point = viewport_point(&g.term, col, row);
        let range = Selection::new(SelectionType::Semantic, point, Side::Left).to_range(&g.term)?;
        let opts = crate::copy_format::Options {
            emit: crate::copy_format::Emit::Plain,
            trim: false,
            background: None,
            foreground: None,
            codepoint_map: &[],
        };
        let text = crate::copy_format::format_range(&g.term, &theme, range.start, range.end, range.is_block, opts);
        if text.trim().is_empty() {
            return None;
        }
        // 접힌 줄의 낱말은 시작이 화면 위로 나갈 수 있다. 화면 안 첫 칸으로 맞춘다
        let offset = g.term.grid().display_offset() as i32;
        let start_row = (range.start.line.0 + offset).max(0) as usize;
        let start_col = if range.start.line.0 + offset < 0 {
            0
        } else {
            range.start.column.0
        };
        Some(WordHit {
            text,
            col: start_col,
            row: start_row,
        })
    }

    /// 프로그램이 XTSHIFTESCAPE로 정한 Shift 마우스 보고 여부. 정한 적이 없으면 `None`(#327)
    pub fn mouse_shift_capture(&self) -> Option<bool> {
        self.inner.lock().term.mouse_shift_capture()
    }

    /// 화면 좌표(보이는 줄 기준)로 고르기를 시작한다. 시작 칸은 통째로 품는다(쪽은 `Left`)
    pub fn select_start(&self, col: usize, row: usize, kind: SelectKind) {
        self.select_start_at(col, row, kind, CellSide::Left);
    }

    /// 칸 안의 어느 쪽을 눌렀는지 알고 고르기를 시작한다(#437, Ghostty `SelectionGesture.dragSelection`).
    /// 끝 칸의 쪽과 함께 글자·사각 고르기에서 두 가장자리 칸을 넣을지 정한다. 낱말·줄 고르기는 쪽을 보지 않는다
    pub fn select_start_at(&self, col: usize, row: usize, kind: SelectKind, side: CellSide) {
        let mut g = self.inner.lock();
        let point = viewport_point(&g.term, col, row);
        let ty = match kind {
            SelectKind::Simple => SelectionType::Simple,
            SelectKind::Word => SelectionType::Semantic,
            SelectKind::Line => SelectionType::Lines,
            SelectKind::Block => SelectionType::Block,
        };
        g.term.selection = Some(Selection::new(ty, point, side.to_engine()));
        g.sel_end = Some(point);
        g.output_click = None;
    }

    /// ⌘(macOS)·Ctrl+세 번 누르기(#700, Ghostty `SelectionGesture`의 `.output`): 화면 좌표 칸의 명령 출력 구간을 고른다.
    /// 그 칸이 출력이 아니거나 고를 것이 없으면 아무것도 바꾸지 않고 거짓(Ghostty도 앞서 고른 것을 그대로 둔다)
    pub fn select_output_at(&self, col: usize, row: usize) -> bool {
        let mut g = self.inner.lock();
        let point = viewport_point(&g.term, col, row);
        let Some((start, end)) = crate::output_select::select_output(&g.term, point) else {
            return false;
        };
        set_range(&mut g, start, end);
        g.output_click = Some(absolute_cell(&g.term, point));
        true
    }

    /// 끄는 중인 글자 고르기를 사각으로 바꾸거나 되돌린다(Ghostty는 끌 때마다 그 순간의 수식 키로 사각 여부를 정한다).
    /// 낱말·줄 고르기는 바꾸지 않는다(Ghostty도 낱말·줄 끌기는 사각을 보지 않는다). 고른 것이 없으면 아무것도 안 한다
    pub fn select_set_block(&self, block: bool) {
        let mut g = self.inner.lock();
        // 출력 고르기는 사각을 보지 않는다(Ghostty `dragSelectionOutput`)
        if g.output_click.is_some() {
            return;
        }
        if let Some(sel) = g.term.selection.as_mut() {
            match (sel.ty, block) {
                (SelectionType::Simple, true) => sel.ty = SelectionType::Block,
                (SelectionType::Block, false) => sel.ty = SelectionType::Simple,
                _ => {}
            }
        }
    }

    /// 끝점을 옮긴다. 시작 칸과 끝 칸을 모두 품는다: 끄는 방향이 거꾸로면 가장자리 쪽을 맞춰 두 칸이 빠지지 않게 한다.
    /// 칸 안 자리를 모르는 호출(링크 고르기, 전체 고르기, 키보드)이 쓴다
    pub fn select_update(&self, col: usize, row: usize) {
        let mut g = self.inner.lock();
        let point = viewport_point(&g.term, col, row);
        if let Some(sel) = g.term.selection.as_mut() {
            update_inclusive(sel, point);
        }
        g.sel_end = Some(point);
    }

    /// 칸 안의 어느 쪽에서 끌고 있는지 알고 끝점을 옮긴다(#437). 두 가장자리 칸을 넣을지는 누른 쪽과 끈 쪽이 정한다
    pub fn select_update_at(&self, col: usize, row: usize, side: CellSide) {
        let mut g = self.inner.lock();
        let point = viewport_point(&g.term, col, row);
        // 출력 고르기를 끄는 중이면 누른 구간에서 끈 구간까지(#700)
        if let Some(click) = g.output_click.filter(|_| g.term.selection.is_some()) {
            // 스크롤백이 지워졌거나 크기가 바뀌어 시작 칸이 격자 밖이면 `drag_output`이 None이고 고른 것을 그대로 둔다
            let click = from_absolute_cell(&g.term, click);
            if let Some((start, end)) = click.and_then(|c| crate::output_select::drag_output(&g.term, c, point)) {
                set_range(&mut g, start, end);
            }
            return;
        }
        if let Some(sel) = g.term.selection.as_mut() {
            sel.update(point, side.to_engine());
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
        let grid = g.term.grid();
        let has_text = |p: Point| crate::output_select::cell_has_text(&grid[p.line][p.column]);
        let next = adjusted_point(end, adj, cols, top, bottom, page, &has_text);
        if let Some(sel) = g.term.selection.as_mut() {
            update_inclusive(sel, next);
        }
        g.sel_end = Some(next);
        g.term.scroll_to_point(next);
        true
    }

    /// 전체 고르기(`select_all`, #805): Ghostty `Screen.selectAll`처럼 스크롤백까지 통틀어 글이 있는 첫 칸부터 마지막 칸까지 고른다
    /// (앞뒤의 빈 줄·빈칸·탭은 뺀다). 글이 하나도 없으면 아무것도 바꾸지 않고 거짓(Ghostty `Surface`도 null이면 앞서 고른 것을 그대로 둔다)
    pub fn select_all(&self) -> bool {
        let mut g = self.inner.lock();
        let Some((start, end)) = alacritty_terminal::selection::ghostty_select_all(&g.term) else {
            return false;
        };
        set_range(&mut g, start, end);
        g.output_click = None;
        true
    }

    pub fn select_clear(&self) {
        let mut g = self.inner.lock();
        g.term.selection = None;
        g.sel_end = None;
        g.output_click = None;
    }

    /// 고른 곳의 글. Ghostty `formatter.zig`(`unwrap = true`, `trim = false`)처럼 만든다(#785): 접힌 줄은 잇고, 글 없는 줄은 모았다가
    /// 뒤에 글 있는 줄이 올 때만 줄바꿈으로 내(끝의 빈 줄은 안 나온다), 직접 쓴 줄 끝 공백은 남기고 지우기·건너뜀으로 빈 칸은 버린다.
    /// `clipboard-trim-trailing-spaces`는 복사하는 쪽이 `trim_trailing_spaces`로 건다. 글이 없으면 None
    pub fn selection_text(&self) -> Option<String> {
        let theme = self.theme.lock().clone();
        let g = self.inner.lock();
        let range = g.term.selection.as_ref().and_then(|s| s.to_range(&g.term))?;
        let opts = crate::copy_format::Options {
            emit: crate::copy_format::Emit::Plain,
            trim: false,
            background: None,
            foreground: None,
            // 글 복사의 `clipboard-codepoint-map`은 앱이 줄 끝 공백을 지운 뒤에 건다. 파일 길에는 걸지 않는다(#787)
            codepoint_map: &[],
        };
        let text = crate::copy_format::format_range(&g.term, &theme, range.start, range.end, range.is_block, opts);
        Some(text).filter(|s| !s.is_empty())
    }

    /// 고른 곳을 색·꾸밈이 든 글(VT 이스케이프 또는 HTML)로 만든다(`copy_to_clipboard:vt|html|mixed`, #550).
    /// 같은 범위의 글은 `selection_text`다. `codepoint_map`은 `clipboard-codepoint-map`(복사 길만. 파일로 뽑는 길은 비워 넘긴다). `screen_colors`가 참이면 바탕·글자 기본색도 넣는다(Ghostty는 `vt`·`html`에서만,
    /// `mixed`의 HTML에서는 뺀다). 고른 것이 없으면 None
    pub fn selection_styled(
        &self,
        emit: crate::copy_format::Emit,
        trim: bool,
        screen_colors: bool,
        codepoint_map: &[bt_core::config::CodepointRule],
    ) -> Option<String> {
        let theme = self.theme.lock().clone();
        let g = self.inner.lock();
        let range = g.term.selection.as_ref().and_then(|s| s.to_range(&g.term))?;
        let colors = g.term.colors();
        let opts = crate::copy_format::Options {
            emit,
            trim,
            background: screen_colors
                .then(|| color::named(&theme, colors, alacritty_terminal::vte::ansi::NamedColor::Background)),
            foreground: screen_colors
                .then(|| color::named(&theme, colors, alacritty_terminal::vte::ansi::NamedColor::Foreground)),
            codepoint_map,
        };
        Some(crate::copy_format::format_range(
            &g.term,
            &theme,
            range.start,
            range.end,
            range.is_block,
            opts,
        ))
    }

    /// 창 상태 복원(#861)이 적을 글과 색: 기본 화면(대체 화면 제외)의 끝에서부터 `max_lines` 줄
    pub fn capture_scrollback(&self, max_lines: usize) -> Vec<crate::saved_screen::SavedLine> {
        crate::saved_screen::capture(&self.inner.lock().term, max_lines)
    }

    /// 칸 전체(`scrollback`이면 스크롤백 포함, 아니면 보이는 화면)를 VT 이스케이프 또는 HTML로 만든다
    /// (`write_screen_file`·`write_scrollback_file`의 `vt`·`html`, #768). Ghostty `writeScreenFile`처럼 끝 칸 공백을 깎지 않고
    /// 바탕·글자 기본색을 함께 넣는다
    pub fn styled_text(&self, scrollback: bool, emit: crate::copy_format::Emit) -> String {
        let theme = self.theme.lock().clone();
        let g = self.inner.lock();
        let colors = g.term.colors();
        let opts = crate::copy_format::Options {
            emit,
            trim: false,
            background: Some(color::named(
                &theme,
                colors,
                alacritty_terminal::vte::ansi::NamedColor::Background,
            )),
            foreground: Some(color::named(
                &theme,
                colors,
                alacritty_terminal::vte::ansi::NamedColor::Foreground,
            )),
            // Ghostty `writeScreenFile`은 `codepoint_map`을 주지 않는다. 파일에는 맵을 걸지 않는다
            codepoint_map: &[],
        };
        crate::copy_format::format_lines(&g.term, &theme, scrollback, opts)
    }

    /// 칸 전체(`scrollback`이면 스크롤백 포함, 아니면 보이는 화면)를 글로 만든다(`write_screen_file`·`write_scrollback_file`의 `plain`, #785).
    /// Ghostty `writeScreenFile`처럼 접힌 줄을 잇고 줄 끝 공백을 남기며(`trim = false`) 끝의 빈 줄은 내지 않는다
    pub fn plain_text(&self, scrollback: bool) -> String {
        let theme = self.theme.lock().clone();
        let g = self.inner.lock();
        let opts = crate::copy_format::Options {
            emit: crate::copy_format::Emit::Plain,
            trim: false,
            background: None,
            foreground: None,
            // 글 복사의 `clipboard-codepoint-map`은 앱이 줄 끝 공백을 지운 뒤에 건다. 파일 길에는 걸지 않는다(#787)
            codepoint_map: &[],
        };
        crate::copy_format::format_lines(&g.term, &theme, scrollback, opts)
    }

    /// (col, row) 칸의 링크를 필요한 줄만 읽어 찾는다. `links::link_at_with(&self.snapshot(), ..)`와 같은 결과이고
    /// 엔진 락은 읽는 동안만 쥔다(#924)
    pub fn link_probe(&self, col: usize, row: usize, opts: crate::links::LinkOptions) -> crate::link_probe::LinkProbe {
        let g = self.inner.lock();
        crate::link_probe::probe(&g.term, col, row, opts)
    }

    /// `link_probe`에 사용자 링크 정규식(`link-url-regex`, #929)을 준다
    pub fn link_probe_with(
        &self,
        col: usize,
        row: usize,
        opts: crate::links::LinkOptions,
        user: Option<&crate::link_regex::UserLinkRegexes>,
    ) -> crate::link_probe::LinkProbe {
        let g = self.inner.lock();
        crate::link_probe::probe_with(&g.term, col, row, opts, user)
    }

    /// 지금 화면을 꺼낸다
    pub fn snapshot(&self) -> Screen {
        let theme = self.theme.lock().clone();
        let mut g = self.inner.lock();
        let Inner {
            term,
            search,
            graphics,
            cell,
            pixel,
            marks,
            ..
        } = &mut *g;
        sync_graphics(term, graphics, *cell);
        let grid = term.grid();
        let shift = pixel.shift(cell.1, grid.history_size(), grid.display_offset());
        let mut screen = snapshot_with_shift(
            term,
            &theme,
            search.as_mut(),
            Some(graphics),
            std::time::Instant::now(),
            shift,
        );
        screen.command_marks = marks_for(term, marks, &screen);
        if let Some(m) = &g.marker {
            crate::marks::apply(&mut screen, m.marker());
        }
        if let Some(t) = &g.triggers {
            t.highlight(&mut screen);
        }
        screen
    }

    /// 마지막 `snapshot`이 본 보이는 kitty 애니메이션 가운데 가장 이른 다음 프레임 시각(#554). 보이는 애니메이션이 없으면 없다.
    /// 앱은 그 시각에 이 칸의 창을 다시 그린다. 스크롤로 밖에 나간 그림·다른 탭은 `snapshot`이 보지 않으므로 시계를 돌리지 않는다
    pub fn kitty_next_frame(&self) -> Option<std::time::Instant> {
        self.inner.lock().graphics.next_frame()
    }

    /// 색 테마를 바꾼다(설정 다시 읽기). 다음 화면부터 새 색으로 푼다
    pub fn set_theme(&self, theme: Theme) {
        *self.theme.lock() = Arc::new(theme);
    }
}

/// 색 구성표 보고(`CSI ? 997 ; 1 n` 어두움, `; 2 n` 밝음). 고정된 바이트이고 출력이 정한 글은 들어가지 않는다(#232)
fn scheme_report(light: bool) -> &'static [u8] {
    if light { b"\x1b[?997;2n" } else { b"\x1b[?997;1n" }
}

/// `clear_screen`(⌘K)의 결과(#821)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClearScreen {
    /// 대체 화면이라 아무것도 하지 않았다. 키를 소비하지 않고 프로그램에 넘긴다
    Ignored,
    /// 지웠다. PTY에는 아무것도 쓰지 않는다(프롬프트가 아니었다)
    Cleared,
    /// 프롬프트라 화면을 지웠다. 셸이 다시 그리도록 PTY에 FF(0x0C)를 써야 한다
    Redraw,
}

/// 커서가 프롬프트에 있는가(Ghostty `Terminal.cursorIsAtPrompt`, Terminal.zig 2313줄): 대체 화면이면 아니다.
/// 커서 줄에 프롬프트 표시가 있으면 맞고, 아니면 커서 칸의 글 종류가 입력·프롬프트일 때 맞다.
/// 셸 통합이 없으면 글 종류는 늘 출력(Ghostty `Screen.Cursor.semantic_content` 기본값)이라 늘 아니다
fn cursor_is_at_prompt<T>(term: &Term<T>) -> bool {
    use alacritty_terminal::term::semantic::{RowPrompt, SemanticContent};
    if term.mode().contains(TermMode::ALT_SCREEN) {
        return false;
    }
    if term.row_prompt(term.grid().cursor.point.line) != RowPrompt::None {
        return true;
    }
    matches!(
        term.semantic_content(),
        SemanticContent::Input | SemanticContent::Prompt
    )
}

/// ⌘K(`clear_screen`)의 엔진 몫(#821, Ghostty `Termio.clearScreen(history = true)`, Termio.zig 605줄).
/// 대체 화면이면 아무것도 하지 않는다. 아니면 선택을 풀고 스크롤백을 지운다(ED 3). 프롬프트가 아니면 커서 윗줄까지 지워
/// (남은 줄이 위로 올라 커서 줄이 맨 위가 된다, Ghostty `eraseActive`가 줄을 떼어 내는 것과 같다) kitty 그림을 모두 지우고 끝낸다.
/// 프롬프트이면 ED 2와 같은 지우기를 하고 `Redraw`를 돌려준다
fn clear_screen_op<T: EventListener>(
    term: &mut Term<T>,
    parser: &mut Processor,
    graphics: &mut crate::graphics::Graphics,
    cell: (u32, u32),
) -> ClearScreen {
    use alacritty_terminal::vte::ansi::{ClearMode, Handler};
    if term.mode().contains(TermMode::ALT_SCREEN) {
        return ClearScreen::Ignored;
    }
    // 동기 갱신 중에 모아 둔 바이트가 지운 뒤에 그려지지 않게 먼저 푼다
    release_sync(term, parser);
    sync_graphics(term, graphics, cell);
    term.selection = None;
    let at_prompt = cursor_is_at_prompt(term);
    if !at_prompt {
        let line = term.grid().cursor.point.line.0;
        if line > 0 {
            let region = Line(0)..Line(term.screen_lines() as i32);
            term.grid_mut().scroll_up_in_place(&region, line as usize);
            term.grid_mut().cursor.point.line = Line(0);
        }
    }
    // 스크롤백 지우기(ED 3). 끝에서 화면 전체를 다시 그리게 표시한다
    Handler::clear_screen(term, ClearMode::Saved);
    if !at_prompt {
        // Ghostty는 `kitty_images.delete(.all = true)`: 보이는 자리를 모두 지우고 쓰지 않게 된 그림 자료도 지운다(`d=A`)
        graphics_command(term, parser, graphics, cell, b"a=d,d=A");
        graphics.erase(false, true);
        return ClearScreen::Cleared;
    }
    graphics.erase(false, true);
    Handler::clear_screen(term, ClearMode::All);
    erase_screen_graphics(term, graphics);
    ClearScreen::Redraw
}

/// kitty `clear_terminal scrollback`(#863): 스크롤백(ED 3)만 지운다. 보이는 화면과 커서는 그대로다.
/// 대체 화면이면 하지 않는다. 스크롤백에 걸친 그림은 지운다. 프롬프트 번호(절대 줄)는 지운 줄 수만큼 당긴다
fn clear_scrollback_op<T: EventListener>(
    term: &mut Term<T>,
    parser: &mut Processor,
    graphics: &mut crate::graphics::Graphics,
    cell: (u32, u32),
    prompts: Option<&mut std::collections::VecDeque<i64>>,
) -> bool {
    use alacritty_terminal::vte::ansi::{ClearMode, Handler};
    if term.mode().contains(TermMode::ALT_SCREEN) {
        return false;
    }
    release_sync(term, parser);
    sync_graphics(term, graphics, cell);
    let before = term.grid().history_size() as i64;
    term.selection = None;
    Handler::clear_screen(term, ClearMode::Saved);
    graphics.erase(false, true);
    if let Some(prompts) = prompts {
        let rows = term.screen_lines() as i64;
        let old = std::mem::take(prompts);
        *prompts = old
            .into_iter()
            .filter_map(|p| Some(p - before).filter(|&n| n >= 0 && n < rows))
            .collect();
    }
    true
}

/// kitty `clear_terminal last_command`(#863, `screen.c` `erase_last_command`): 지금 프롬프트 바로 위 프롬프트 블록을 지우고
/// 아래 줄을 끌어올린다. 대체 화면·셸 통합 표시 없음·위에 명령 없음이면 거짓.
/// 줄 번호가 밀리므로 프롬프트 기억은 지운 구간을 빼고 당기며, 놓인 kitty 그림 자리는 줄을 따라가지 못해 주 화면 것을 모두 지운다
fn clear_last_command_op<T: EventListener>(
    term: &mut Term<T>,
    parser: &mut Processor,
    graphics: &mut crate::graphics::Graphics,
    cell: (u32, u32),
    prompts: Option<&mut std::collections::VecDeque<i64>>,
) -> bool {
    if term.mode().contains(TermMode::ALT_SCREEN) {
        return false;
    }
    release_sync(term, parser);
    sync_graphics(term, graphics, cell);
    let Some((start, end)) = crate::output_select::last_command_block(term) else {
        return false;
    };
    let history = term.grid().history_size() as i64;
    let (abs_start, abs_end) = (history + start.0 as i64, history + end.0 as i64);
    let (from_history, from_screen) = term.remove_lines(start, end);
    if from_history + from_screen == 0 {
        return false;
    }
    let removed = (abs_end - abs_start).max(0);
    if let Some(prompts) = prompts {
        let rows_total = term.grid().history_size() as i64 + term.screen_lines() as i64;
        let old = std::mem::take(prompts);
        *prompts = old
            .into_iter()
            .filter(|&p| p < abs_start || p >= abs_end)
            .map(|p| if p >= abs_end { p - removed } else { p })
            .filter(|&p| p >= 0 && p < rows_total)
            .collect();
    }
    graphics.lost_scrolls(false);
    true
}

/// ED 2를 엔진이 처리한 뒤의 그림 정리(#329): 스크롤백에 다 들어간 그림만 남기고 보이는 자리의 그림은 지운다
fn erase_screen_graphics<T>(term: &Term<T>, graphics: &mut crate::graphics::Graphics) {
    let alt = term.mode().contains(alacritty_terminal::term::TermMode::ALT_SCREEN);
    graphics.erase_screen(term.grid().history_size() as i64, alt);
}

/// 트리거 규칙을 컴파일한다. 규칙이 하나도 없으면 없다(검사하지 않는다)
fn new_triggers(specs: &[bt_core::trigger::TriggerSpec]) -> (Option<crate::triggers::Triggers>, Vec<String>) {
    let (t, errors) = crate::triggers::Triggers::new(specs);
    ((!t.is_empty()).then_some(t), errors)
}

/// 트리거가 낸 일을 세션 사건으로 바꾼다. 알림·벨·탭 표시뿐이고 PTY에 쓰는 사건은 없다(#928)
pub fn trigger_event(e: crate::triggers::Effect) -> SessionEvent {
    match e {
        crate::triggers::Effect::Notify { body } => SessionEvent::Notification(Notification {
            title: String::new(),
            body,
        }),
        crate::triggers::Effect::Bell => SessionEvent::Bell,
        crate::triggers::Effect::Attention => SessionEvent::TriggerAttention,
    }
}

fn signal_event(sig: Signal) -> SessionEvent {
    match sig {
        Signal::Notify(n) => SessionEvent::Notification(n),
        Signal::Cwd { host, path } => SessionEvent::Cwd { host, path },
        Signal::CwdReset => SessionEvent::CwdReset,
        Signal::Progress { state, percent } => SessionEvent::Progress { state, percent },
        // 셸 통합 표지와 그래픽은 읽기 스레드가 따로 다룬다
        Signal::Prompt(_)
        | Signal::Semantic(_)
        | Signal::CommandLine(_)
        | Signal::Graphics(_)
        | Signal::Glyph(_)
        | Signal::Erase { .. }
        | Signal::TitleReport
        | Signal::Enquiry
        | Signal::Kam(_)
        | Signal::ColorSchemeQuery
        | Signal::DecDsr(_)
        | Signal::ColorSchemeUpdates(_)
        | Signal::MouseShape(_)
        | Signal::XtChecksumReport(_)
        | Signal::XtChecksumExtension(_)
        | Signal::SoftReset
        | Signal::Da1
        | Signal::Da2
        | Signal::XtVersion
        | Signal::SizeReport(_)
        | Signal::InBandSizeReports(_)
        | Signal::InBandSizeQuery
        | Signal::ColorSchemeModeQuery
        | Signal::VisibilityReports(_)
        | Signal::VisibilityQuery
        | Signal::VisibilityStatusQuery
        | Signal::KittyClipboard { .. }
        | Signal::KittyPasteEvents(_)
        | Signal::KittyPasteQuery
        | Signal::ModeSave(_)
        | Signal::ModeRestore(_)
        | Signal::DcsQuery(_)
        | Signal::KittyColor { .. }
        | Signal::KittyNotifyReply(_)
        | Signal::ProgramStatusQuery { .. }
        | Signal::ProgramStatus(_)
        | Signal::WindowResize { .. } => SessionEvent::Output,
    }
}

/// 터미널의 답을 PTY에 쓴다
fn write_reply(writer: &Writer, bytes: &[u8]) {
    let mut w = writer.lock();
    let _ = w.write_all(bytes);
    let _ = w.flush();
}

fn size_info(size: GridSize) -> crate::csi_query::SizeInfo {
    crate::csi_query::SizeInfo {
        rows: size.rows,
        cols: size.cols,
        cell_width: u32::from(size.cell_width),
        cell_height: u32::from(size.cell_height),
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
    /// 사각(세로 덩어리) 고르기. 줄을 따라가지 않고 두 끝점이 이루는 사각 안의 칸만 고른다(#433)
    Block,
}

/// 칸 안의 왼쪽·오른쪽(Ghostty는 칸 너비의 60% 자리를 경계로 가른다, bt-app `mouse::cell_side`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellSide {
    Left,
    Right,
}

impl CellSide {
    fn to_engine(self) -> Side {
        match self {
            Self::Left => Side::Left,
            Self::Right => Side::Right,
        }
    }
}

/// 시작 칸과 끝 칸을 모두 품게 끝점을 옮긴다(#437). 엔진은 쪽으로 가장자리 칸을 넣을지 정하므로 끄는 방향이 거꾸로면
/// 시작은 `Right`, 끝은 `Left`로 해야 두 칸이 남는다. 시작점은 엔진의 고르기에서 읽는다: 끄는 중 출력으로 스크롤하면
/// 엔진이 고르기를 함께 옮기므로 따로 들고 있으면 옛 화면 줄을 가리킨다. 사각은 열만 비교한다
fn update_inclusive(sel: &mut Selection, point: Point) {
    let start = sel.start_point();
    let backwards = if sel.ty == SelectionType::Block {
        point.column < start.column
    } else {
        point < start
    };
    let (from, to) = if backwards {
        (Side::Right, Side::Left)
    } else {
        (Side::Left, Side::Right)
    };
    let mut next = Selection::new(sel.ty, start, from);
    next.update(point, to);
    *sel = next;
}

/// openpty가 실패했을 때 다시 시도하기 전에 쉬는 시간(시도 사이마다 하나). 길이가 시도 횟수 - 1이다.
/// macOS에서 여러 스레드가 동시에 `/dev/ptmx`를 열면 가끔 `Os { code: -6 }`으로 일시 실패한다(#607)
const OPENPTY_BACKOFF: [Duration; 4] = [
    Duration::from_millis(1),
    Duration::from_millis(2),
    Duration::from_millis(5),
    Duration::from_millis(10),
];

/// `open`(openpty)을 `backoff.len() + 1`번까지 시도한다. 먼저 성공하면 바로 돌려주고, 시도 사이에 `sleep`으로 쉰다.
/// 다 실패하면 시도 횟수와 마지막 오류를 담는다. 진짜 한도(PTY 수·파일 수)에 닿은 경우에도 같은 길로 가지만
/// 늘어나는 시간은 backoff 합(18ms)뿐이다. 오류에서 errno를 꺼낼 수 없어(portable-pty가 문자열로 만든다) 종류로 가르지 않는다
fn open_pty_with_retry<T>(
    mut open: impl FnMut() -> anyhow::Result<T>,
    backoff: impl AsRef<[Duration]>,
    mut sleep: impl FnMut(Duration),
) -> anyhow::Result<T> {
    let backoff = backoff.as_ref();
    let tries = backoff.len() + 1;
    let mut attempt = 0;
    loop {
        match open() {
            Ok(v) => return Ok(v),
            Err(e) if attempt >= backoff.len() => {
                let note = explain_negative_errno(&format!("{e:#}"));
                return Err(e.context(format!("PTY 열기(openpty), {tries}번 시도해도 실패{note}")));
            }
            Err(_) => {
                sleep(backoff[attempt]);
                attempt += 1;
            }
        }
    }
}

/// 오류 문구의 `code: -N`(음수 errno)을 풀어 쓴다. 양수 N은 POSIX errno지만 음수는 libc 오류 번호가 아니다.
/// macOS에서 `/dev/ptmx`를 동시에 열 때 -6이 나온다(원인은 확인하지 못함, troubleshooting 22번)
fn explain_negative_errno(msg: &str) -> String {
    let Some(rest) = msg.split("code: -").nth(1) else {
        return String::new();
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return String::new();
    }
    format!(
        " (errno -{digits}: 음수라 libc 오류 번호가 아니다. 커널이 /dev/ptmx 열기에서 돌려준 일시 실패로 보이며 양수 errno와 뜻이 같지 않다)"
    )
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
fn adjusted_point(
    p: Point,
    adj: SelectionAdjust,
    cols: usize,
    top: Line,
    bottom: Line,
    page: usize,
    has_text: &dyn Fn(Point) -> bool,
) -> Point {
    use SelectionAdjust::*;
    let last = Column(cols.saturating_sub(1));
    let row_has_text = |l: Line| (0..cols).any(|c| has_text(Point::new(l, Column(c))));
    // 글 있는 마지막 줄의 끝(Ghostty `.end`). 글이 없으면 그대로
    let end = || {
        (top.0..=bottom.0)
            .rev()
            .map(Line)
            .find(|&l| row_has_text(l))
            .map_or(p, |l| Point::new(l, last))
    };
    match adj {
        // Ghostty: 글이 있는 앞·다음 칸까지 건너뛴다(줄을 넘어감). 없으면 그대로
        Left => {
            let before = (top.0..=p.line.0).rev().flat_map(|l| {
                let from = if l == p.line.0 { p.column.0 } else { cols };
                (0..from).rev().map(move |c| Point::new(Line(l), Column(c)))
            });
            before.into_iter().find(|&q| has_text(q)).unwrap_or(p)
        }
        Right => {
            let after = (p.line.0..=bottom.0).flat_map(|l| {
                let from = if l == p.line.0 { p.column.0 + 1 } else { 0 };
                (from..cols).map(move |c| Point::new(Line(l), Column(c)))
            });
            after.into_iter().find(|&q| has_text(q)).unwrap_or(p)
        }
        // 맨 윗줄이면 줄 처음으로
        Up if p.line > top => Point::new(p.line - 1, p.column),
        Up => Point::new(p.line, Column(0)),
        // 글이 있는 다음 줄로. 없으면 그 줄 끝으로
        Down => ((p.line.0 + 1)..=bottom.0)
            .map(Line)
            .find(|&l| row_has_text(l))
            .map_or(Point::new(p.line, last), |l| Point::new(l, p.column)),
        // 한 화면만큼 옮길 수 없으면 home·end
        PageUp if p.line.0 - page as i32 >= top.0 => Point::new(p.line - page, p.column),
        PageUp => Point::new(top, Column(0)),
        PageDown if p.line.0 + page as i32 <= bottom.0 => Point::new(p.line + page, p.column),
        PageDown => end(),
        Home => Point::new(top, Column(0)),
        End => end(),
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
        mouse_x10: m.contains(TermMode::MOUSE_X10),
        mouse_format: if m.contains(TermMode::SGR_PIXELS_MOUSE) {
            crate::screen::MouseFormat::SgrPixels
        } else if m.contains(TermMode::URXVT_MOUSE) {
            crate::screen::MouseFormat::Urxvt
        } else if m.contains(TermMode::SGR_MOUSE) {
            crate::screen::MouseFormat::Sgr
        } else if m.contains(TermMode::UTF8_MOUSE) {
            crate::screen::MouseFormat::Utf8
        } else {
            crate::screen::MouseFormat::X10
        },
        focus_events: m.contains(TermMode::FOCUS_IN_OUT),
        alternate_scroll: m.contains(TermMode::ALTERNATE_SCROLL),
        kitty: crate::input::KittyFlags {
            disambiguate: m.contains(TermMode::DISAMBIGUATE_ESC_CODES),
            report_events: m.contains(TermMode::REPORT_EVENT_TYPES),
            report_alternates: m.contains(TermMode::REPORT_ALTERNATE_KEYS),
            all_as_escapes: m.contains(TermMode::REPORT_ALL_KEYS_AS_ESC),
            report_associated: m.contains(TermMode::REPORT_ASSOCIATED_TEXT),
        },
        key: crate::input::KeyModes {
            backarrow_key: m.contains(TermMode::BACKARROW_KEY),
            ignore_keypad_with_numlock: m.contains(TermMode::IGNORE_KEYPAD_NUMLOCK),
            alt_esc_prefix: m.contains(TermMode::ALT_ESC_PREFIX),
            alt_sends_escape: m.contains(TermMode::ALT_SENDS_ESCAPE),
        },
        reverse_colors: m.contains(TermMode::REVERSE_COLORS),
        modify_other_keys: m.contains(TermMode::MODIFY_OTHER_KEYS_2),
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
pub(crate) const MAX_LINKS: usize = 4096;
pub(crate) const MAX_LINK_URI: usize = 8192;

fn snapshot_of<T: EventListener>(
    term: &Term<T>,
    theme: &Theme,
    search: Option<&mut crate::search::Search>,
    graphics: Option<&mut crate::graphics::Graphics>,
    now: std::time::Instant,
) -> Screen {
    snapshot_with_shift(term, theme, search, graphics, now, 0)
}

/// `snapshot_of`에 픽셀 스크롤(#902)의 걸친 픽셀 `pixel_shift`를 준다. 0이거나 맨 아래이면 `snapshot_of`와 같다.
/// 0보다 크면 화면 맨 아래에 걸쳐 보이는 다음 줄을 `Screen::edge`에 담는다(고르기·찾기·링크 표시를 같은 규칙으로)
fn snapshot_with_shift<T: EventListener>(
    term: &Term<T>,
    theme: &Theme,
    search: Option<&mut crate::search::Search>,
    graphics: Option<&mut crate::graphics::Graphics>,
    now: std::time::Instant,
    pixel_shift: u32,
) -> Screen {
    // 맨 아래에는 다음 줄이 없다
    let pixel_shift = if term.grid().display_offset() == 0 {
        0
    } else {
        pixel_shift
    };
    let cols = term.columns();
    let rows = term.screen_lines();
    let content = term.renderable_content();
    // 화면 반전(DECSCNM, ?5): Ghostty처럼 기본 글자색·바탕색만 맞바꾼다. 칸에 정한 색(팔레트·RGB)은 그대로다.
    // 푸는 길을 하나로 두려고 테마와 OSC 10·11로 바꾼 값을 같이 맞바꾼 사본으로 아래 전부를 푼다(#302)
    let reversed = content.mode.contains(TermMode::REVERSE_COLORS);
    let chrome_background = color::named(theme, content.colors, NamedColor::Background);
    let swapped_theme;
    let swapped_colors;
    let (theme, overrides) = if reversed {
        let mut t = theme.clone();
        std::mem::swap(&mut t.foreground, &mut t.background);
        let mut c = *content.colors;
        let (fg, bg) = (c[NamedColor::Foreground], c[NamedColor::Background]);
        c[NamedColor::Foreground] = bg;
        c[NamedColor::Background] = fg;
        swapped_theme = t;
        swapped_colors = c;
        (&swapped_theme, &swapped_colors)
    } else {
        (theme, content.colors)
    };
    let background = color::named(theme, overrides, NamedColor::Background);
    let foreground = color::named(theme, overrides, NamedColor::Foreground);
    let mut cells = vec![Cell::blank(foreground, background); cols * rows];
    let offset = content.display_offset as i32;
    let selection = content.selection;
    // 찾기: 보이는 맞은 곳들과 지금 것. 칸을 위에서 아래로 훑으며 맞춘다
    let (matches, current) = match search {
        Some(s) => {
            let current = s.current(term);
            (s.visible_with_edge(term, pixel_shift > 0), current)
        }
        None => (Vec::new(), None),
    };
    let mut match_cursor = 0;
    let mut links: Vec<String> = Vec::new();
    // 주소 → 자리. 칸마다 다른 링크를 찍어도 그릴 때마다 칸 수 × 링크 수를 비교하지 않게(#54)
    let mut link_ids: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    // 칸 하나를 화면 칸으로 바꾼다. 보이는 줄과 픽셀 스크롤로 걸쳐 보이는 다음 줄(#902)이 같은 규칙을 쓴다
    let mut make_cell = |src: &alacritty_terminal::term::cell::Cell, point: alacritty_terminal::index::Point| -> Cell {
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
        if f.contains(Flags::DOUBLE_UNDERLINE) {
            flags.insert(CellFlags::DOUBLE_UNDERLINE);
        }
        if f.contains(Flags::DOTTED_UNDERLINE) {
            flags.insert(CellFlags::DOTTED_UNDERLINE);
        }
        if f.contains(Flags::DASHED_UNDERLINE) {
            flags.insert(CellFlags::DASHED_UNDERLINE);
        }
        if f.contains(Flags::STRIKEOUT) {
            flags.insert(CellFlags::STRIKE);
        }
        if f.contains(Flags::OVERLINE) {
            flags.insert(CellFlags::OVERLINE);
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
        if f.contains(Flags::DIM) {
            flags.insert(CellFlags::FAINT);
        }
        if f.contains(Flags::BLINK) {
            flags.insert(CellFlags::BLINK);
        }
        if src.c == crate::kitty_unicode::PLACEHOLDER {
            flags.insert(CellFlags::PLACEHOLDER);
        }
        if selection.is_some_and(|s| s.contains(point)) {
            flags.insert(CellFlags::SELECTED);
        }
        if !matches.is_empty() && crate::search::hit(&matches, &mut match_cursor, point) {
            flags.insert(CellFlags::MATCH);
            if current.as_ref().is_some_and(|m| m.contains(&point)) {
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
        Cell {
            ch: src.c,
            combining,
            fg,
            bg,
            flags,
            link,
            underline_color: color::underline_rgb(theme, overrides, src.underline_color(), f, bg),
        }
    };

    for indexed in content.display_iter {
        let row = indexed.point.line.0 + offset;
        if row < 0 || row as usize >= rows {
            continue;
        }
        let col = indexed.point.column.0;
        if col >= cols {
            continue;
        }
        cells[row as usize * cols + col] = make_cell(indexed.cell, indexed.point);
    }
    // 픽셀 스크롤(#902): 화면 맨 아래에 걸쳐 보이는 다음 줄. 맨 아래(offset 0)에는 다음 줄이 없다
    let mut edge: Vec<Cell> = Vec::new();
    if pixel_shift > 0 && offset > 0 {
        let line = Line(rows as i32 - offset);
        let row = &term.grid()[line];
        for col in 0..cols {
            edge.push(make_cell(
                &row[Column(col)],
                alacritty_terminal::index::Point::new(line, Column(col)),
            ));
        }
    }

    // 넓은 글자의 머리와 꼬리는 하나로 고른다(Ghostty `render.zig`의 줄 범위와 `generic.zig`의 `x_compare`).
    // 꼬리 칸은 머리 칸 자리로 견주고, 줄 끝 자리표시 칸에서 끝난 글자 고름은 다음 줄의 넓은 글자까지 넓힌다(사각은 넓히지 않는다)
    if let Some(sel) = selection {
        if !sel.is_block {
            for r in 0..rows.saturating_sub(1) {
                let (last, next) = (r * cols + cols - 1, (r + 1) * cols);
                if cells[last].flags.contains(CellFlags::SELECTED)
                    && cells[last].flags.contains(CellFlags::SPACER)
                    && !cells[last].flags.contains(CellFlags::WIDE)
                    // 앞 칸이 넓은 글자면 마지막 칸은 그 꼬리다(자리표시 칸과 같은 SPACER 표시를 쓴다)
                    && !(cols > 1 && cells[last - 1].flags.contains(CellFlags::WIDE))
                    && cells[next].flags.contains(CellFlags::WIDE)
                    && !cells[next].flags.contains(CellFlags::SELECTED)
                    && sel.end.line.0 + offset == r as i32
                {
                    cells[next].flags.insert(CellFlags::SELECTED);
                }
            }
        }
        for i in 0..cells.len() {
            if cells[i].flags.contains(CellFlags::WIDE) && i % cols + 1 < cols {
                let sel_pair =
                    cells[i].flags.contains(CellFlags::SELECTED) || cells[i + 1].flags.contains(CellFlags::SELECTED);
                if sel_pair && cells[i + 1].flags.contains(CellFlags::SPACER) {
                    cells[i].flags.insert(CellFlags::SELECTED);
                    cells[i + 1].flags.insert(CellFlags::SELECTED);
                }
            }
        }
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
    let in_grid = cursor_row >= 0 && (cursor_row as usize) < rows;
    // 픽셀 스크롤(#902): 커서가 걸쳐 보이는 다음 줄에 있으면 그 줄(`row == rows`)에 그린다
    let in_edge = !edge.is_empty() && cursor_row == rows as i32;
    let in_view = in_grid || in_edge;
    let col = vt_cursor.point.column.0.min(cols.saturating_sub(1));
    let wide = (in_grid && cells[cursor_row as usize * cols + col].flags.contains(CellFlags::WIDE))
        || (in_edge && edge[col].flags.contains(CellFlags::WIDE));
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
        pixel_shift,
        edge,
        background,
        foreground,
        cursor_color: overrides[NamedColor::Cursor].map(|c| bt_core::Rgb::new(c.r, c.g, c.b)),
        chrome_background,
        links,
        images: graphics.map(|g| visible_images(term, g, now)).unwrap_or_default(),
        command_marks: Vec::new(),
        trigger_colors: Vec::new(),
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
    // 이 명령 앞까지의 스크롤을 자리에 반영한 뒤의 줄 번호로 놓는다(#800)
    sync_graphics(term, graphics, cell);
    let cur = crate::graphics::Cursor {
        line: term.grid().history_size() as i64 + term.grid().cursor.point.line.0 as i64,
        col: term.grid().cursor.point.column.0,
        cell_w: cell.0,
        cell_h: cell.1,
        alt: term.mode().contains(TermMode::ALT_SCREEN),
        top: term.grid().history_size() as i64,
        rows: term.screen_lines() as u32,
        cols: term.columns() as u32,
    };
    let out = graphics.handle(raw, cur);
    if !graphics.placements.is_empty() {
        // 놓은 그림도 아래 커서 이동이 스크롤하는 영역 안이면 함께 움직인다(Ghostty는 자리를 더한 뒤에 index한다)
        term.set_scroll_log(true);
    }
    if let Some((rows, col)) = out.advance {
        move_cursor_after_placement(term, parser, rows, col);
    }
    sync_graphics(term, graphics, cell);
    out.reply
}

/// 그림을 놓은 뒤 커서를 옮긴다(#801, Ghostty `graphics_exec.zig` `display`의 `.after`, 403줄).
/// `rows`만큼 index(IND, 스크롤 영역을 따르고 맨 아래 줄이면 영역이 스크롤한다)하되, 믿지 않는 출력이 큰 값을 줄 수 있으므로
/// "스크롤 영역 맨 아래까지 남은 줄 + 화면 높이"를 넘지 않는다(영역 안이 아니면 화면 높이). 그다음 줄바꿈 대기를 풀고 `col`칸으로 간다
fn move_cursor_after_placement<T: EventListener>(term: &mut Term<T>, parser: &mut Processor, rows: u32, col: usize) {
    let point = term.grid().cursor.point;
    let region = term.scroll_region();
    let in_region = point.line >= region.start
        && point.line < region.end
        && point.column.0 >= term.left_margin()
        && point.column.0 <= term.right_margin();
    let before_scroll = if in_region {
        (region.end.0 - 1 - point.line.0) as usize
    } else {
        0
    };
    let n = (rows as usize).min(before_scroll.saturating_add(term.screen_lines()));
    if n > 0 {
        parser.advance(term, "\x1bD".repeat(n).as_bytes());
    }
    term.set_cursor_column(col);
}

/// 엔진이 모아 둔 스크롤 기록을 그림 자리에 반영하고, 자리가 있는 동안만 기록을 쌓게 한다(#800).
/// 자리가 없으면 줄 번호가 바뀔 일이 없어 엔진은 기록하지 않는다. 화면을 읽기 전·그림 명령 전·지우기 전·크기를 바꾸기 전과 읽기마다 부른다
fn sync_graphics<T: EventListener>(term: &mut Term<T>, graphics: &mut crate::graphics::Graphics, cell: (u32, u32)) {
    let (events, lost) = term.take_scroll_log();
    for ev in &events {
        graphics.scroll_region(ev, cell);
    }
    if lost {
        graphics.lost_scrolls(term.mode().contains(TermMode::ALT_SCREEN));
    }
    term.set_scroll_log(!graphics.placements.is_empty());
}

/// 지금 보이는 줄에 걸친 kitty 그림들. 대체 화면을 나왔으면 그 화면의 그림을 지운다
/// 보이는 화면(걸쳐 보이는 줄 포함)에 붙는 명령 표시(#927)
fn marks_for<T: EventListener>(
    term: &Term<T>,
    marks: &crate::command_marks::CommandMarks,
    screen: &Screen,
) -> Vec<crate::command_marks::RowMark> {
    crate::command_marks::visible(term, marks, screen.draw_rows())
}

/// 크기를 바꿔 줄이 다시 접힌 뒤 명령 기억의 줄 번호를 프롬프트 줄들에 맞춘다(#927)
fn realign_marks<T: EventListener>(term: &Term<T>, marks: &mut crate::command_marks::CommandMarks) {
    if !marks.is_empty() {
        marks.realign(&crate::command_marks::prompt_ids(term));
    }
}

fn visible_images<T>(
    term: &Term<T>,
    g: &mut crate::graphics::Graphics,
    now: std::time::Instant,
) -> Vec<crate::screen::ImageView> {
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
    let mut picked: Vec<crate::graphics::Placement> = Vec::new();
    // 가상 배치와 상대 배치는 자기 칸이 없다. 아래 `extra_images`가 자리표시 칸과 부모 자리에서 정한다(#553)
    for p in g.placements.iter().rev().filter(|p| p.alt == alt && p.is_pin()) {
        let row = p.line - top;
        // 같은 그림을 같은 자리·모양으로 두 번 그리지 않는다(자리 번호만 다른 겹침). 잘라내기·오프셋·z가 다르면 다른 그림이다
        if row + p.rows as i64 <= 0
            || row >= rows
            || !seen.insert((p.image, p.line, p.col, p.cols, p.rows, p.geom, p.z))
        {
            continue;
        }
        let visible_rows = (row + p.rows as i64).min(rows) - row.max(0);
        let visible_cols = (p.col as i64 + p.cols as i64).min(cols) - (p.col as i64).min(cols);
        let area = visible_rows.max(0) * visible_cols.max(0);
        if used + area > budget {
            break;
        }
        used += area;
        picked.push(p.clone());
    }
    picked.reverse();
    // 자리표시 글자가 보여 주는 가상 배치 조각과 상대 배치(#553). 그릴 양의 상한을 같이 쓴다
    let mut share = crate::kitty_unicode::Budget { used, budget };
    let extra = crate::kitty_unicode::extra_images(term, g, alt, top, &mut share);
    // 보이는 그림만 지금 시각의 프레임으로 옮긴다(보이지 않는 그림은 시계를 돌리지 않는다, #554).
    // 자리표시 조각·상대 배치로 보이는 그림도 같이 돌리고, 돌린 뒤의 프레임을 담는다
    let shown: Vec<u32> = picked
        .iter()
        .map(|p| p.image)
        .chain(extra.iter().map(|(id, _)| *id))
        .collect();
    g.tick_visible(&shown, now);
    let mut images: Vec<crate::screen::ImageView> = picked
        .into_iter()
        .filter_map(|p| {
            let row = p.line - top;
            if row + p.rows as i64 <= 0 || row >= rows {
                return None;
            }
            let img = g.image(p.image)?;
            Some(crate::screen::ImageView {
                rgba: img.frame_rgba(),
                width: img.width,
                height: img.height,
                row: row as i32,
                col: i32::try_from(p.col).unwrap_or(i32::MAX),
                cols: p.cols,
                rows: p.rows,
                geom: p.geom,
                z: p.z,
            })
        })
        .collect();
    images.extend(extra.into_iter().map(|(id, mut view)| {
        if let Some(img) = g.image(id) {
            view.rgba = img.frame_rgba();
        }
        view
    }));
    images
}

/// 동기 출력(DEC 2026)을 모으는 중이면 모아 둔 글을 반영하고 푼다(`Processor::stop_sync`). 아니면 아무것도 하지 않는다
fn release_sync<H: alacritty_terminal::vte::ansi::Handler>(term: &mut H, parser: &mut Processor) {
    if parser.sync_timeout().sync_timeout().is_some() {
        parser.stop_sync(term);
    }
}

/// PTY 없이 바이트를 VT로 해석해 화면을 얻는다. 렌더러 테스트와 b-studio 세션 터미널(바이트가 HTTP로 오는 칸)에 쓴다
pub struct Emulator {
    term: Term<NullListener>,
    parser: Processor,
    theme: Theme,
    scanner: Scanner,
    search: Option<crate::search::Search>,
    /// kitty 마커(#929)
    marker: Option<crate::marks::MarkerState>,
    /// 마지막 `snapshot`이 마크를 찾으려고 본 줄 수(시험용)
    marker_rows: usize,
    /// 트리거(#928)와 아직 꺼내지 않은 그 결과
    triggers: Option<crate::triggers::Triggers>,
    trigger_effects: Vec<crate::triggers::Effect>,
    graphics: crate::graphics::Graphics,
    /// 글리프 프로토콜의 이 칸 등록(#779)
    glossary: crate::glyph::Glossary,
    cell: (u32, u32),
    /// 픽셀 스크롤(#902)의 걸친 픽셀
    pixel: crate::pixel_scroll::PixelScroll,
    /// 그래픽 명령의 답(PTY가 없으므로 모아 둔다)
    replies: Vec<u8>,
    /// 명령마다 끝난 결과(#927). 세션의 읽기 스레드와 같은 규칙이다
    marks: crate::command_marks::CommandMarks,
}

#[derive(Clone, Copy)]
struct NullListener;
impl EventListener for NullListener {}

impl Emulator {
    pub fn new(size: GridSize, theme: Theme) -> Self {
        let size = size.clamped();
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
            marker: None,
            marker_rows: 0,
            triggers: None,
            trigger_effects: Vec::new(),
            graphics: Default::default(),
            glossary: Default::default(),
            cell: (size.cell_width as u32, size.cell_height as u32),
            pixel: Default::default(),
            replies: Vec::new(),
            marks: Default::default(),
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
                Signal::Glyph(raw) => {
                    if let Some(r) = self.glossary.handle(raw) {
                        self.replies.extend(r);
                    }
                }
                Signal::Erase { visible, scrollback } => {
                    sync_graphics(&mut self.term, &mut self.graphics, self.cell);
                    self.graphics.erase(*visible, *scrollback);
                    if *visible && !*scrollback {
                        erase_screen_graphics(&self.term, &mut self.graphics);
                    }
                    if *visible && *scrollback {
                        self.glossary.clear_all();
                        crate::kitty_color::reset_palette(&mut self.term);
                    }
                }
                Signal::SoftReset => crate::kitty_color::soft_reset(&mut self.term),
                Signal::KittyColor { requests, bel } => {
                    if let Some(r) = crate::kitty_color::apply(&mut self.term, &self.theme, requests, *bel) {
                        self.replies.extend(r);
                    }
                }
                Signal::Semantic(sp) => sp.apply(&mut self.term), // #329
                // 명령 표시(#927)
                Signal::Prompt(PromptMark::PromptStart) => {
                    if !self.term.mode().contains(TermMode::ALT_SCREEN) {
                        self.marks.prune(crate::command_marks::oldest_id(&self.term));
                        let id = crate::command_marks::stable_id(&self.term, self.term.grid().cursor.point.line);
                        self.marks.prompt(id);
                    }
                }
                Signal::Prompt(PromptMark::CommandExecuted) => self.marks.executed(Instant::now()),
                Signal::Prompt(PromptMark::CommandFinished(code)) => self.marks.finished(*code, Instant::now()),
                Signal::KittyNotifyReply(r) => self.replies.extend(r), // #561
                Signal::ProgramStatusQuery { bel } => self.replies.extend(crate::program_status::query_reply(*bel)), // #618
                _ => {}
            }
            notes.push(sig);
        }
        self.parser.advance(&mut self.term, &bytes[pos..]);
        sync_graphics(&mut self.term, &mut self.graphics, self.cell);
        if let Some(t) = self.triggers.as_mut() {
            self.trigger_effects.extend(t.scan(&self.term, Instant::now()));
        }
        notes
    }

    /// 트리거를 바꾼다(#928). 못 읽는 규칙은 빼고 이유를 돌려준다
    pub fn set_triggers(&mut self, specs: &[bt_core::trigger::TriggerSpec]) -> Vec<String> {
        let (mut t, errors) = new_triggers(specs);
        if let Some(t) = t.as_mut() {
            t.arm(&self.term);
        }
        self.triggers = t;
        errors
    }

    /// 지금까지 트리거가 낸 일(알림·벨·탭 표시)을 꺼낸다. PTY에 쓸 바이트는 없다(`take_replies`와 따로다)
    pub fn take_trigger_effects(&mut self) -> Vec<crate::triggers::Effect> {
        std::mem::take(&mut self.trigger_effects)
    }

    pub fn triggers(&self) -> Option<&crate::triggers::Triggers> {
        self.triggers.as_ref()
    }

    pub fn resize(&mut self, size: GridSize) {
        let size = size.clamped();
        release_sync(&mut self.term, &mut self.parser);
        sync_graphics(&mut self.term, &mut self.graphics, self.cell);
        self.term.resize(Dims(size));
        realign_marks(&self.term, &mut self.marks);
    }

    pub fn scroll(&mut self, lines: i32) {
        self.pixel.reset();
        self.term.scroll_display(Scroll::Delta(lines));
    }

    /// 스크롤백을 위로(양수) `delta` 픽셀 올린다(`Session::scroll_pixels`와 같다, #902)
    pub fn scroll_pixels(&mut self, delta: f64) -> bool {
        scroll_term_pixels(&mut self.pixel, &mut self.term, delta, self.cell.1)
    }

    /// 지금 걸친 픽셀(`Session::pixel_shift`와 같다)
    pub fn pixel_shift(&self) -> u32 {
        let grid = self.term.grid();
        self.pixel
            .shift(self.cell.1, grid.history_size(), grid.display_offset())
    }

    pub fn modes(&self) -> Modes {
        modes_of(*self.term.mode())
    }

    /// (col, row) 칸의 링크를 필요한 줄만 읽어 찾는다. `links::link_at_with(&self.snapshot(), ..)`와 같은 결과다(#924)
    pub fn link_probe(&self, col: usize, row: usize, opts: crate::links::LinkOptions) -> crate::link_probe::LinkProbe {
        crate::link_probe::probe(&self.term, col, row, opts)
    }

    pub fn snapshot(&mut self) -> Screen {
        sync_graphics(&mut self.term, &mut self.graphics, self.cell);
        let shift = self.pixel_shift();
        let mut screen = snapshot_with_shift(
            &self.term,
            &self.theme,
            self.search.as_mut(),
            Some(&mut self.graphics),
            std::time::Instant::now(),
            shift,
        );
        screen.command_marks = marks_for(&self.term, &self.marks, &screen);
        self.marker_rows = self
            .marker
            .as_ref()
            .map_or(0, |m| crate::marks::apply(&mut screen, m.marker()));
        if let Some(t) = &self.triggers {
            t.highlight(&mut screen);
        }
        screen
    }

    /// (col, row) 칸의 링크에 사용자 정규식(`link-url-regex`)을 더해 찾는다(#929)
    pub fn link_probe_with(
        &self,
        col: usize,
        row: usize,
        opts: crate::links::LinkOptions,
        user: Option<&crate::link_regex::UserLinkRegexes>,
    ) -> crate::link_probe::LinkProbe {
        crate::link_probe::probe_with(&self.term, col, row, opts, user)
    }

    /// kitty `toggle_marker`(#929): 같은 명세면 끄고(`Ok(false)`), 아니면 바꿔 켠다(`Ok(true)`)
    pub fn toggle_marker(&mut self, spec: &crate::marks::MarkerSpec) -> Result<bool, String> {
        crate::marks::toggle(&mut self.marker, spec)
    }

    /// kitty `remove_marker`: 마커가 있었으면 참
    pub fn remove_marker(&mut self) -> bool {
        self.marker.take().is_some()
    }

    /// kitty `scroll_to_mark`(#929). 옮겼으면 참
    pub fn scroll_to_mark(&mut self, prev: bool, mark: u8) -> bool {
        self.scroll_to_mark_within(prev, mark, crate::marks::SCROLL_SCAN_LINES)
    }

    /// `scroll_to_mark`에 훑을 줄 수 상한을 준다(시험용)
    pub fn scroll_to_mark_within(&mut self, prev: bool, mark: u8, max_lines: usize) -> bool {
        let Some(m) = self.marker.as_ref() else { return false };
        let Some(n) = crate::marks::scroll_amount(&self.term, m.marker(), prev, mark, max_lines) else {
            return false;
        };
        self.pixel.reset();
        self.term.scroll_display(Scroll::Delta(n));
        true
    }

    /// 마지막 `snapshot`이 마크를 찾으려고 본 줄 수(시험용)
    pub fn marker_rows_checked(&self) -> usize {
        self.marker_rows
    }

    /// 시각을 정해 화면을 만든다(애니메이션 프레임 고르기 시험용, #554). 시각은 `kitty_clock_start`부터의 간격으로 준다
    pub fn snapshot_at(&mut self, now: std::time::Instant) -> Screen {
        sync_graphics(&mut self.term, &mut self.graphics, self.cell);
        let mut screen = snapshot_of(
            &self.term,
            &self.theme,
            self.search.as_mut(),
            Some(&mut self.graphics),
            now,
        );
        screen.command_marks = marks_for(&self.term, &self.marks, &screen);
        if let Some(m) = &self.marker {
            crate::marks::apply(&mut screen, m.marker());
        }
        if let Some(t) = &self.triggers {
            t.highlight(&mut screen);
        }
        screen
    }

    /// kitty 애니메이션 시계의 0점(`snapshot_at`에 줄 시각의 기준)
    pub fn kitty_clock_start(&self) -> std::time::Instant {
        self.graphics.clock_start()
    }

    /// 마지막 화면이 본 가장 이른 다음 프레임 시각(`Session::kitty_next_frame`과 같다)
    pub fn kitty_next_frame(&self) -> Option<std::time::Instant> {
        self.graphics.next_frame()
    }

    /// 찾기를 시작한다(고르지 않는다). 빈 글이면 찾기를 끄고 없다
    pub fn search(&mut self, query: &str) -> Option<crate::search::SearchStatus> {
        self.search = crate::search::Search::new(query);
        let s = self.search.as_mut()?;
        Some(s.status(&self.term))
    }

    pub fn search_step(&mut self, older: bool) -> Option<crate::search::SearchStatus> {
        let s = self.search.as_mut()?;
        s.step(&mut self.term, older);
        Some(s.status(&self.term))
    }

    /// 지금 찾기 결과(몇 번째, 모두 몇 개)를 다시 센다. 출력이 와서 맞은 곳이 바뀌었을 수 있다
    pub fn search_status(&mut self) -> Option<crate::search::SearchStatus> {
        let s = self.search.as_mut()?;
        Some(s.status(&self.term))
    }

    /// 화면이 지난번 센 뒤로 바뀌었으면 다시 세어 돌려준다. 안 바뀌었거나 찾는 중이 아니면 없다(#833)
    pub fn search_refresh(&mut self) -> Option<crate::search::SearchStatus> {
        self.search.as_mut()?.refresh(&self.term)
    }

    /// 스크롤백 전체를 센 횟수(시험용). 찾는 중이 아니면 0
    pub fn search_recounts(&self) -> usize {
        self.search.as_ref().map_or(0, |s| s.recounts())
    }

    pub fn search_clear(&mut self) {
        self.search = None;
    }

    pub fn tail_text(&self, lines: usize) -> String {
        tail_text(&self.term, lines)
    }

    /// ⌘K(`clear_screen`)의 엔진 몫(#821). `Session::clear_screen`과 같다
    pub fn clear_screen(&mut self) -> ClearScreen {
        let Emulator {
            term,
            parser,
            graphics,
            cell,
            marks,
            ..
        } = self;
        let out = clear_screen_op(term, parser, graphics, *cell);
        if out != ClearScreen::Ignored {
            marks.clear();
        }
        out
    }
}

impl Emulator {
    /// kitty `clear_terminal scrollback`(#863). `Session::clear_scrollback`과 같다
    pub fn clear_scrollback(&mut self) -> bool {
        let Emulator {
            term,
            parser,
            graphics,
            cell,
            ..
        } = self;
        clear_scrollback_op(term, parser, graphics, *cell, None)
    }

    /// kitty `clear_terminal last_command`(#863). `Session::clear_last_command`와 같다
    pub fn clear_last_command(&mut self) -> bool {
        let Emulator {
            term,
            parser,
            graphics,
            cell,
            marks,
            ..
        } = self;
        let cut = crate::output_select::last_command_block(term)
            .map(|(start, _)| crate::command_marks::stable_id(term, start));
        let done = clear_last_command_op(term, parser, graphics, *cell, None);
        if let (true, Some(cut)) = (done, cut) {
            marks.truncate_from(cut);
        }
        done
    }
}

impl Session {
    /// 고른 영역이 화면 좌표 `(col, row)` 칸을 품는가(오른쪽 클릭 메뉴가 고른 것을 그대로 둘지 정한다, #278)
    pub fn selection_contains(&self, col: usize, row: usize) -> bool {
        let g = self.inner.lock();
        let point = viewport_point(&g.term, col, row);
        g.term
            .selection
            .as_ref()
            .and_then(|s| s.to_range(&g.term))
            .is_some_and(|r| r.contains(point))
    }
}

/// 칸을 닫으면 셸에 SIGHUP을 보낸다(Ghostty `termio/Exec.zig` `killPid`의 몫, #651).
/// 읽기 스레드가 PTY 주 쪽 fd를 쥐고 있어 `Session`을 떨어뜨리는 것만으로는 셸이 끊김을 받지 못한다.
/// 이미 끝난 칸에는 보내지 않는다(pid 재사용). 셸이 SIGHUP을 막아 두었으면 짧게 되풀이한 뒤 그만둔다
/// (SIGKILL로 넘어가지 않는다: 저장하던 편집기나 ssh를 강제로 끊지 않는다)
impl Drop for Session {
    fn drop(&mut self) {
        {
            let exited = self.exited.lock();
            if exited.is_some() {
                return;
            }
            // `exited` 잠금을 쥔 채 보낸다: 읽기 스레드는 이 잠금 안에서 거두므로 pid는 아직 우리 자식이다
            let _ = self.killer.lock().kill();
        }
        let exited = self.exited.clone();
        let killer = self.killer.clone();
        // 앱 메인 스레드를 막지 않는 되풀이(최대 약 3초, 100ms 간격)
        let _ = std::thread::Builder::new().name("bt-pty-hup".into()).spawn(move || {
            for _ in 0..30 {
                std::thread::sleep(Duration::from_millis(100));
                let exited = exited.lock();
                if exited.is_some() {
                    return;
                }
                let _ = killer.lock().kill();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openpty_retries_transient_failures_then_succeeds() {
        let mut calls = 0;
        let mut slept = Vec::new();
        let got = open_pty_with_retry(
            || {
                calls += 1;
                if calls < 3 {
                    anyhow::bail!(
                        "failed to openpty: Os {{ code: -6, kind: Uncategorized, message: \"Unknown error: -6\" }}"
                    )
                }
                Ok(calls)
            },
            OPENPTY_BACKOFF,
            |d| slept.push(d),
        );
        assert_eq!(got.unwrap(), 3);
        assert_eq!(
            slept,
            [Duration::from_millis(1), Duration::from_millis(2)],
            "실패한 만큼만 쉰다"
        );
    }

    #[test]
    fn openpty_succeeding_first_time_does_not_sleep() {
        let mut slept = 0;
        let got = open_pty_with_retry(|| Ok(7), OPENPTY_BACKOFF, |_| slept += 1);
        assert_eq!(got.unwrap(), 7);
        assert_eq!(slept, 0);
    }

    #[test]
    fn openpty_gives_up_after_bounded_tries_and_explains_negative_errno() {
        let mut calls = 0;
        let mut slept = Duration::ZERO;
        let err = open_pty_with_retry::<()>(
            || {
                calls += 1;
                anyhow::bail!(
                    "failed to openpty: Os {{ code: -6, kind: Uncategorized, message: \"Unknown error: -6\" }}"
                )
            },
            OPENPTY_BACKOFF,
            |d| slept += d,
        )
        .unwrap_err();
        assert_eq!(calls, OPENPTY_BACKOFF.len() + 1, "시도 횟수에 상한이 있다");
        assert!(
            slept <= Duration::from_millis(20),
            "쉬는 시간에도 상한이 있다: {slept:?}"
        );
        let msg = format!("{err:#}");
        assert!(msg.contains("5번 시도"), "시도 횟수가 오류에 없다: {msg}");
        assert!(msg.contains("errno -6: 음수"), "음수 errno 설명이 없다: {msg}");
        assert!(msg.contains("code: -6"), "원래 오류가 사라졌다: {msg}");
    }

    #[test]
    fn positive_errno_gets_no_negative_errno_note() {
        assert_eq!(
            explain_negative_errno("failed to openpty: Os { code: 24, kind: Uncategorized }"),
            ""
        );
        assert_eq!(explain_negative_errno("code: -"), "");
    }

    #[test]
    fn graphics_yield_waits_until_enough_time_piles_up() {
        let mut spent = Duration::ZERO;
        assert!(!graphics_yield_due(&mut spent, Duration::from_millis(1)));
        assert!(!graphics_yield_due(&mut spent, Duration::from_millis(2)));
        assert!(graphics_yield_due(&mut spent, Duration::from_millis(1)));
        assert_eq!(spent, Duration::ZERO, "넘긴 뒤에는 처음부터 센다");
        assert!(
            graphics_yield_due(&mut spent, Duration::from_millis(30)),
            "한 명령이 길면 바로 넘긴다"
        );
        assert!(!graphics_yield_due(&mut spent, Duration::ZERO));
    }

    /// 쓴 바이트를 모으고, 첫 덩어리를 받으면 칸을 읽기 전용으로 하는 가짜 PTY
    struct LockAfterFirst {
        got: Arc<Mutex<Vec<u8>>>,
        readonly: Arc<std::sync::atomic::AtomicBool>,
    }

    impl Write for LockAfterFirst {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.got.lock().extend_from_slice(buf);
            self.readonly.store(true, std::sync::atomic::Ordering::Release);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn startup_input_stops_once_readonly() {
        let got = Arc::new(Mutex::new(Vec::new()));
        let readonly = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let writer: Writer = Arc::new(Mutex::new(Box::new(LockAfterFirst {
            got: got.clone(),
            readonly: readonly.clone(),
        })));
        write_startup_chunks(&writer, &[b'x'; 4096], &readonly);
        assert_eq!(got.lock().len(), 1024, "읽기 전용이 켜진 뒤의 덩어리는 쓰지 않는다");

        // 처음부터 읽기 전용이면 아무것도 쓰지 않는다
        let got2 = Arc::new(Mutex::new(Vec::new()));
        let writer2: Writer = Arc::new(Mutex::new(Box::new(LockAfterFirst {
            got: got2.clone(),
            readonly: readonly.clone(),
        })));
        write_startup_chunks(&writer2, b"echo hi\n", &readonly);
        assert!(got2.lock().is_empty());
    }

    /// Ghostty `Screen.testWriteString`처럼 줄마다 글을 찍은 10×10 화면(스크롤백 없음)에서 끝점을 옮긴다
    fn adjust_on(rows: &[&str], from: (usize, usize), a: SelectionAdjust) -> (usize, usize) {
        let grid: Vec<Vec<bool>> = (0..10)
            .map(|y| {
                let line = rows.get(y).copied().unwrap_or("");
                (0..10)
                    .map(|x| line.chars().nth(x).is_some_and(|c| c != '\0'))
                    .collect()
            })
            .collect();
        let has_text = |p: Point| grid[p.line.0 as usize][p.column.0];
        let at = Point::new(Line(from.1 as i32), Column(from.0));
        let got = adjusted_point(at, a, 10, Line(0), Line(9), 10, &has_text);
        (got.column.0, got.line.0 as usize)
    }

    // 아래 시험은 Ghostty v1.4.0 `src/terminal/Selection.zig`의 "Selection: adjust …" 시험을 옮겼다. 좌표는 (x, y)
    #[test]
    fn adjust_right_like_ghostty() {
        use SelectionAdjust::*;
        let s = ["A1234", "B5678", "C1234", "D5678"];
        assert_eq!(adjust_on(&s, (3, 3), Right), (4, 3), "간단히 오른쪽");
        assert_eq!(
            adjust_on(&s, (4, 2), Right),
            (0, 3),
            "줄 끝이면 다음 줄 첫 글자로(빈 칸을 건너뜀)"
        );
        assert_eq!(adjust_on(&s, (4, 3), Right), (4, 3), "화면 끝이면 그대로");
    }

    #[test]
    fn adjust_left_like_ghostty() {
        use SelectionAdjust::*;
        let s = ["A1234", "B5678", "C1234", "D5678"];
        assert_eq!(adjust_on(&s, (3, 3), Left), (2, 3));
        assert_eq!(adjust_on(&s, (0, 3), Left), (4, 2), "줄 처음이면 앞 줄의 마지막 글자로");
        let s = ["A1234", "B5678", "C12", "D56"];
        assert_eq!(adjust_on(&s, (4, 3), Left), (2, 3), "같은 줄의 빈 칸을 건너뜀");
        assert_eq!(adjust_on(&s, (0, 3), Left), (2, 2), "앞 줄 끝의 빈 칸을 건너뜀");
    }

    #[test]
    fn adjust_up_down_like_ghostty() {
        use SelectionAdjust::*;
        let s = ["A", "B", "C", "D", "E"];
        assert_eq!(adjust_on(&s, (3, 3), Up), (3, 2));
        assert_eq!(adjust_on(&s, (3, 0), Up), (0, 0), "맨 윗줄이면 줄 처음으로");
        assert_eq!(adjust_on(&s, (3, 3), Down), (3, 4));
        assert_eq!(
            adjust_on(&s, (3, 4), Down),
            (9, 4),
            "아래에 글 있는 줄이 없으면 줄 끝으로"
        );
        let s = ["A", "B", "C"];
        assert_eq!(
            adjust_on(&s, (3, 2), Down),
            (9, 2),
            "화면이 다 차지 않아도 빈 줄로 내려가지 않음"
        );
    }

    #[test]
    fn adjust_home_end_and_line_like_ghostty() {
        use SelectionAdjust::*;
        let s = ["A", "B", "C"];
        assert_eq!(adjust_on(&s, (1, 2), Home), (0, 0));
        assert_eq!(adjust_on(&s, (1, 1), End), (9, 2), "글 있는 마지막 줄의 끝");
        let s = ["A12 B34", "C12 D34"];
        assert_eq!(adjust_on(&s, (5, 1), BeginningOfLine), (0, 1));
        assert_eq!(adjust_on(&s, (0, 1), BeginningOfLine), (0, 1));
        assert_eq!(adjust_on(&s, (1, 1), EndOfLine), (9, 1));
        assert_eq!(adjust_on(&s, (9, 1), EndOfLine), (9, 1));
    }

    #[test]
    fn adjust_page_moves_or_falls_back_to_home_end() {
        use SelectionAdjust::*;
        // Ghostty: 한 화면만큼 옮길 수 없으면 home·end
        let s = ["A", "B", "C"];
        assert_eq!(adjust_on(&s, (3, 2), PageUp), (0, 0));
        assert_eq!(adjust_on(&s, (3, 2), PageDown), (9, 2));
        // 옮길 수 있으면 열은 그대로 한 화면만큼(스크롤백 5줄 + 화면 10줄)
        let all = |_: Point| true;
        let at = |l: i32, c: usize| Point::new(Line(l), Column(c));
        assert_eq!(
            adjusted_point(at(6, 3), PageUp, 10, Line(-5), Line(9), 10, &all),
            at(-4, 3)
        );
        assert_eq!(
            adjusted_point(at(-4, 3), PageDown, 10, Line(-5), Line(9), 10, &all),
            at(6, 3)
        );
    }

    // ---- ⌘K clear_screen(#821, Ghostty 1.4.0 Termio.clearScreen) ----

    fn clear_emu(rows: u16) -> Emulator {
        Emulator::new(GridSize::new(20, rows), Theme::default())
    }

    fn row_text(e: &Emulator, row: i32) -> String {
        (0..e.term.columns())
            .map(|c| e.term.grid()[Line(row)][Column(c)].c)
            .collect::<String>()
            .trim_end()
            .to_string()
    }

    fn select_all(e: &mut Emulator) {
        let mut sel = Selection::new(SelectionType::Simple, Point::new(Line(0), Column(0)), Side::Left);
        sel.update(Point::new(Line(0), Column(3)), Side::Right);
        e.term.selection = Some(sel);
    }

    #[test]
    fn clear_screen_ignored_on_alt_screen() {
        let mut e = clear_emu(5);
        e.feed(b"one\r\ntwo\x1b[?1049h\x1b[HALT");
        select_all(&mut e);
        assert_eq!(e.clear_screen(), ClearScreen::Ignored);
        assert_eq!(row_text(&e, 0), "ALT", "대체 화면은 건드리지 않는다");
        assert!(e.term.selection.is_some(), "선택도 그대로다");
    }

    #[test]
    fn clear_screen_without_shell_integration_erases_above_cursor_and_sends_no_ff() {
        // 셸 통합이 없으면 커서 칸 종류는 출력이라 프롬프트가 아니다(Ghostty 기본값 .output)
        let mut e = clear_emu(5);
        e.feed(b"a1\r\na2\r\na3\r\na4\r\na5");
        e.feed(b"\r\n\r\nb1\r\nb2\r\nb3\r\nb4");
        assert!(e.term.grid().history_size() > 0);
        select_all(&mut e);
        assert_eq!(e.clear_screen(), ClearScreen::Cleared, "FF를 보내라고 하지 않는다");
        assert_eq!(e.term.grid().history_size(), 0, "스크롤백을 지운다");
        assert!(e.term.selection.is_none(), "선택을 푼다");
        // 커서 줄(b4)만 남고 맨 위로 올라온다
        assert_eq!(row_text(&e, 0), "b4");
        assert_eq!(row_text(&e, 1), "");
        assert_eq!(e.term.grid().cursor.point.line, Line(0));
    }

    #[test]
    fn clear_screen_running_command_keeps_cursor_row_and_sends_no_ff() {
        // 명령이 도는 중(출력 구간): 커서 줄은 남고 FF는 가지 않는다
        let mut e = clear_emu(5);
        e.feed(b"\x1b]133;A\x07$ sleep 30\x1b]133;C\x07\r\nout1\r\nout2");
        assert_eq!(e.clear_screen(), ClearScreen::Cleared);
        assert_eq!(row_text(&e, 0), "out2");
    }

    #[test]
    fn clear_screen_at_prompt_erases_screen_and_asks_for_ff() {
        let mut e = clear_emu(5);
        e.feed(b"old1\r\nold2\r\nold3\r\nold4\r\nold5\r\nold6");
        e.feed(b"\x1b]133;A\x07$ ");
        select_all(&mut e);
        assert_eq!(e.clear_screen(), ClearScreen::Redraw);
        assert!(e.term.selection.is_none());
        // Ghostty 1.4.0도 같다: 스크롤백을 지운 뒤의 ED 2가 프롬프트 규칙(`scrollClear`)으로 화면 내용을 스크롤백에 올린다
        // (Termio.zig의 "TODO: fix this"). 이 줄이 바뀌면 Ghostty가 고친 것이니 견줘 본다
        assert_eq!(e.term.grid().history_size(), 5);
        for r in 0..5 {
            assert_eq!(row_text(&e, r), "", "줄 {r}: 화면은 빈다(셸이 FF로 다시 그린다)");
        }
    }

    /// #866: `quote-urls-at-prompt`가 쓰는 프롬프트 판단. 셸 통합이 없으면 늘 아니고, OSC 133 프롬프트에서는 맞고,
    /// 명령이 도는 중(133;C 뒤)과 대체 화면에서는 아니다
    #[test]
    fn cursor_is_at_prompt_follows_osc_133_and_alt_screen() {
        let mut e = clear_emu(5);
        e.feed(b"$ ");
        assert!(!cursor_is_at_prompt(&e.term), "셸 통합이 없으면 아니다");
        e.feed(b"\r\n\x1b]133;A\x07$ ");
        assert!(cursor_is_at_prompt(&e.term));
        e.feed(b"\x1b]133;C\x07\r\noutput");
        assert!(!cursor_is_at_prompt(&e.term), "출력 중이다");
        e.feed(b"\x1b]133;A\x07$ ");
        assert!(cursor_is_at_prompt(&e.term));
        e.feed(b"\x1b[?1049h");
        assert!(!cursor_is_at_prompt(&e.term), "대체 화면이다");
    }

    #[test]
    fn clear_screen_clears_kitty_images_when_not_at_prompt() {
        let mut e = clear_emu(5);
        // 1x1 RGB 그림을 놓는다
        e.feed(b"\x1b_Ga=T,f=24,s=1,v=1,i=7,q=2;AAAA\x1b\\");
        e.feed(b"\r\ntext");
        assert!(!e.graphics.placements.is_empty());
        assert_eq!(e.clear_screen(), ClearScreen::Cleared);
        assert!(e.graphics.placements.is_empty());
        assert!(e.graphics.image(7).is_none(), "그림 자료도 지운다");
    }
}

/// 암호 입력 추측: 줄 단위 입력이고 되울림이 꺼졌다(Ghostty `mode.canonical and !mode.echo`)
#[cfg_attr(not(unix), allow(dead_code))]
fn is_password_mode(canonical: bool, echo: bool) -> bool {
    canonical && !echo
}

#[cfg(test)]
mod password_tests {
    use super::is_password_mode;

    #[test]
    fn password_mode_needs_canonical_without_echo() {
        assert!(is_password_mode(true, false));
        assert!(!is_password_mode(true, true), "되울림이 켜진 보통 줄 입력");
        assert!(!is_password_mode(false, false), "vim 같은 원시 입력");
        assert!(!is_password_mode(false, true));
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

/// 프로그램이 OSC 0/2로 보낸 제목은 탭 제목 틀에 값으로만 끼고 다시 해석되지 않는다(#910, ADR-098). 엔진에 바이트를 먹인다. PTY는 열지 않는다
#[cfg(test)]
mod title_template_tests {
    use super::*;
    use bt_core::tabtitle::{self, SpanColor, TabTitleConfig, TitleData};

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<String>>>);
    impl EventListener for Capture {
        fn send_event(&self, event: VtEvent) {
            if let VtEvent::Title(t) = event {
                self.0.lock().push(t);
            }
        }
    }

    fn title_after(bytes: &[u8]) -> String {
        let cap = Capture::default();
        let size = GridSize::new(20, 4).clamped();
        let mut term = Term::new(Config::default(), &Dims(size), cap.clone());
        let mut parser: Processor = Processor::new();
        parser.advance(&mut term, bytes);
        cap.0.lock().last().cloned().unwrap_or_default()
    }

    #[test]
    fn osc2_title_with_template_syntax_shows_literally_in_any_template() {
        let title = title_after(b"\x1b]2;{fmt.fg.red}x{index}\x07");
        assert_eq!(title, "{fmt.fg.red}x{index}");
        for template in [
            "{title}",
            "{index}: {title}",
            "{fmt.bold}{title}{fmt.nobold}",
            tabtitle::DEFAULT_TEMPLATE,
        ] {
            let cfg = TabTitleConfig {
                template: template.to_string(),
                ..TabTitleConfig::default()
            };
            let data = TitleData {
                title: &title,
                index: 3,
                ..TitleData::default()
            };
            let spans = tabtitle::render(&cfg, false, &data, false, false);
            let text: String = spans.iter().map(|s| s.text.as_str()).collect();
            assert!(text.ends_with("{fmt.fg.red}x{index}"), "{template}: {text}");
            assert!(!text.contains("\x1b"), "{template}");
            // 제목 글자에서 온 조각에는 빨강이 없다
            assert!(
                spans
                    .iter()
                    .filter(|s| s.text.contains("fmt.fg.red"))
                    .all(|s| s.fg == SpanColor::Tab),
                "{template}: {spans:?}"
            );
        }
        // 제어 문자가 낀 제목: ESC는 지우고 나머지는 값이다
        let title = title_after(b"\x1b]2;a\x1b[31mb{title}\x07");
        let data = TitleData {
            title: &title,
            ..TitleData::default()
        };
        let spans = tabtitle::render(&TabTitleConfig::default(), false, &data, false, false);
        assert_eq!(spans.len(), 1);
        assert!(!spans[0].text.contains('\x1b'));
    }
    /// 칸 제목 막대 틀(#916)도 같다: OSC 2 제목 `{fmt.fg.red}x`가 글자 그대로 나오고 막대 색을 바꾸지 못한다(엔진에 바이트를 먹인다, PTY 없음)
    #[test]
    fn osc2_title_shows_literally_in_the_window_title_bar_template() {
        let title = title_after(b"\x1b]2;{fmt.fg.red}x{window.id}\x07");
        assert_eq!(title, "{fmt.fg.red}x{window.id}");
        let tab = TabTitleConfig::default();
        for template in [
            "{title}",
            bt_core::wintitle::DEFAULT_TEMPLATE,
            "{window.id}: {title}",
            "{fmt.bold}{title}{fmt.nobold}",
        ] {
            let d = tabtitle::WindowData {
                title: &title,
                id: 4,
                ..tabtitle::WindowData::default()
            };
            let spans = tabtitle::render_window(template, &tab, &d);
            let text: String = spans.iter().map(|s| s.text.as_str()).collect();
            assert!(text.ends_with("{fmt.fg.red}x{window.id}"), "{template}: {text}");
            assert!(!text.contains('\x1b'), "{template}");
            assert!(
                spans
                    .iter()
                    .filter(|s| s.text.contains("fmt.fg.red"))
                    .all(|s| s.fg == SpanColor::Tab),
                "{template}: {spans:?}"
            );
        }
    }
}
