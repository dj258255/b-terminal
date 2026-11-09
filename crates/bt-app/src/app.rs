//! 창 하나, 탭 여럿. winit 이벤트를 받아 PTY에 쓰고 화면을 그린다

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bt_render::{Canvas, CursorStyle, DrawCell, DrawCursor, FontConfig, Fonts, GridView, draw_grid, draw_grid_rows};
use bt_term::{CellFlags, CursorShape, GridSize, Screen, SelectKind, Session, SessionEvent, SpawnOptions};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::ModifiersState;
use winit::window::{CursorIcon, Window, WindowId};

use crate::keymap::{Action, Keymap, SplitTarget};
use crate::keys;
use crate::mouse;
use crate::split::{Direction, Node, Rect, Slot};
use crate::tabbar;
use crate::undo::UndoStack;
use bt_core::config::NotifyOnFinish;

fn truncate_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    s.chars().take(n.saturating_sub(1)).chain(['…']).collect()
}

fn command_result(rec: &bt_term::CommandRecord) -> bt_core::proto::CommandResult {
    bt_core::proto::CommandResult {
        command: rec.command.clone(),
        output: rec.output.clone(),
        exit_code: rec.exit_code,
        duration_ms: rec.duration.as_millis() as u64,
    }
}

/// 알림 띠가 보이는 시간
const TOAST_FOR: Duration = Duration::from_millis(2500);

/// 다른 스레드(PTY 읽기)가 창 스레드를 깨우는 사건
#[derive(Debug, Clone)]
pub enum UserEvent {
    /// 어떤 탭의 화면이 바뀌었다(여러 번 와도 한 번 그린다)
    Redraw,
    Session(u64, SessionEvent),
    /// 전역 단축키가 눌렸다(단축키 번호, #95)
    Hotkey(u32),
    /// 메뉴 막대 항목을 눌렀다(항목 id). 메뉴는 macOS에만 있다
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Menu(String),
    /// 제어 소켓 요청. 답은 함께 온 통로로 보낸다
    Control(
        bt_core::proto::Request,
        std::sync::mpsc::SyncSender<bt_core::proto::Response>,
    ),
}

/// 앱 설정: 설정 파일(bt_core::Config, Ghostty 문법)을 그대로 쓴다(#13)
pub type AppConfig = bt_core::Config;

/// 설정의 글꼴 키를 렌더러 설정으로
fn font_config(c: &AppConfig, size: f32, scale: f32) -> FontConfig {
    let defaults = FontConfig::default();
    // font-family를 여러 번 쓰면 첫째가 기본 글꼴, 나머지는 기본 대체 글꼴보다 앞에 둔다(Ghostty와 같다)
    let mut fallbacks: Vec<String> = c.font_family.iter().skip(1).cloned().collect();
    fallbacks.extend(defaults.fallbacks);
    FontConfig {
        family: c.font_family.first().cloned(),
        family_bold: c.font_family_bold.clone(),
        family_italic: c.font_family_italic.clone(),
        family_bold_italic: c.font_family_bold_italic.clone(),
        adjust_cell_height: c.adjust_cell_height,
        adjust_cell_width: c.adjust_cell_width,
        adjust: bt_render::MetricAdjust {
            baseline: c.adjust_font_baseline,
            underline_position: c.adjust_underline_position,
            underline_thickness: c.adjust_underline_thickness,
            strikethrough_position: c.adjust_strikethrough_position,
            strikethrough_thickness: c.adjust_strikethrough_thickness,
            overline_position: c.adjust_overline_position,
            overline_thickness: c.adjust_overline_thickness,
            cursor_thickness: c.adjust_cursor_thickness,
            cursor_height: c.adjust_cursor_height,
            box_thickness: c.adjust_box_thickness,
            icon_height: c.adjust_icon_height,
        },
        variation: c.font_variation.clone(),
        variation_bold: c.font_variation_bold.clone(),
        variation_italic: c.font_variation_italic.clone(),
        variation_bold_italic: c.font_variation_bold_italic.clone(),
        size,
        scale,
        fallbacks,
        features: c.font_features.clone(),
        styles: [
            c.font_style.clone(),
            c.font_style_bold.clone(),
            c.font_style_italic.clone(),
            c.font_style_bold_italic.clone(),
        ],
        codepoint_map: c.font_codepoint_map.clone(),
        shaping_break_cursor: c.font_shaping_break.cursor,
    }
}

/// 칸 하나: 셸 하나와 그 상태. 제어 소켓의 "탭 id"(BT_TAB_ID)는 칸의 id다
pub struct Pane {
    pub id: u64,
    pub session: Session,
    pub title: String,
    pub exited: Option<Option<u32>>,
    /// 명령이 끝났지만 칸을 남기고 키를 기다린다(`wait-after-command`·비정상 종료, #178)
    pub held_after_exit: bool,
    /// 전경 프로세스 이름(1초마다 갱신). pid가 같으면 다시 묻지 않는다(node 스크립트는 ps를 부른다)
    pub fg_name: Option<String>,
    fg_pid: Option<i32>,
    /// 보지 않은 알림이 있다
    pub unread: bool,
    /// 보지 않은 벨이 있다(`bell-features = title`, #107): 제목 앞에 🔔
    pub bell: bool,
    /// 사람이 붙인 제목(`prompt_surface_title`·`set_surface_title`, #124). 프로그램이 정한 제목보다 먼저
    pub user_title: Option<String>,
    /// 칸을 만든 때와 크기 표시를 지울 때(#122)
    created: Instant,
    overlay_until: Option<Instant>,
    /// 셸이 OSC 7로 알린 작업 폴더
    pub cwd: Option<std::path::PathBuf>,
    /// 마지막으로 데스크톱 알림을 띄운 때(탭마다 묶는다)
    last_desktop: Option<Instant>,
    /// 셸 통합: 지금 도는 명령(글, 시작한 때)과 마지막으로 끝난 명령
    pub running: Option<(Option<String>, Instant)>,
    pub last_command: Option<bt_term::CommandRecord>,
    /// 진행 표시(OSC 9;4): (상태, 퍼센트, 받은 때). 15초 동안 새 소식이 없으면 지운다(Ghostty와 같다)
    pub progress: Option<(u8, u8, Instant)>,
}

impl Pane {
    /// 아직 살아 있는 진행 표시(상태, 퍼센트)
    fn live_progress(&self) -> Option<(u8, u8)> {
        self.progress
            .filter(|(_, _, at)| at.elapsed() < Duration::from_secs(15))
            .map(|(s, p, _)| (s, p))
    }

    /// 탭 줄·창 제목에 쓸 이름: 사람이 붙인 제목 → 셸이 보낸 제목 → 전경 프로세스 이름. 보지 않은 벨이 있으면 앞에 🔔
    fn display_title(&self) -> String {
        title_of(
            self.user_title.as_deref(),
            &self.title,
            self.fg_name.as_deref(),
            self.bell,
        )
    }
}

/// 커서 색과 커서 위 글자색(#135): 설정이 칸 색을 따르라면 커서 칸의 글자색·바탕색, 아니면 테마 색
fn cursor_colors(
    color: Option<bt_core::config::CellColor>,
    text: Option<bt_core::config::CellColor>,
    theme: (bt_core::Rgb, bt_core::Rgb),
    cell: (bt_core::Rgb, bt_core::Rgb),
) -> (bt_core::Rgb, bt_core::Rgb) {
    use bt_core::config::CellColor;
    let pick = |c: Option<CellColor>, fallback| match c {
        Some(CellColor::Foreground) => cell.0,
        Some(CellColor::Background) => cell.1,
        None => fallback,
    };
    (pick(color, theme.0), pick(text, theme.1))
}

/// 칸 제목(#124): 사람이 붙인 것 → 프로그램이 정한 것 → 전경 프로세스 이름 → "셸". 벨이 있으면 앞에 🔔
fn title_of(user: Option<&str>, program: &str, fg: Option<&str>, bell: bool) -> String {
    let name = if let Some(u) = user.filter(|u| !u.is_empty()) {
        u.to_string()
    } else if !program.is_empty() {
        program.to_string()
    } else {
        fg.unwrap_or("셸").to_string()
    };
    if bell { format!("🔔 {name}") } else { name }
}

/// 사람이 친 제목을 다듬는다: 제어 문자를 지우고 앞뒤 공백을 떼고 200자까지. 비면 없다
fn clean_user_title(text: &str) -> Option<String> {
    let t: String = bt_core::proto::printable(text).trim().chars().take(200).collect();
    let t = t.trim_end().to_string();
    (!t.is_empty()).then_some(t)
}

/// 칸을 만든 직후 이만큼은 크기 맞추기로 보고 크기 표시를 하지 않는다(`resize-overlay = after-first`, #122)
const RESIZE_OVERLAY_GRACE: Duration = Duration::from_millis(500);

/// 이 크기 바뀜에 크기 표시를 보일지. `age`: 칸을 만든 뒤 지난 시간
fn resize_overlay_due(mode: bt_core::config::ResizeOverlay, age: Duration) -> bool {
    use bt_core::config::ResizeOverlay;
    match mode {
        ResizeOverlay::Always => true,
        ResizeOverlay::Never => false,
        ResizeOverlay::AfterFirst => age >= RESIZE_OVERLAY_GRACE,
    }
}

/// 닫기 전에 물어야 할 만큼 바쁜 칸인가(#108). `integrated`: 셸 통합 표지를 받았다,
/// `running`: 셸 통합이 알린 명령이 도는 중, `fg_is_shell`: 전경 프로세스가 셸 자신인가(모르면 없다)
/// Ghostty는 셸 통합이 없으면 늘 묻는다. 여기서는 그때 전경 프로세스가 셸 자신이면 묻지 않는다(`/bin/sh` 칸이 늘 묻지 않게)
fn pane_busy(
    mode: bt_core::config::ConfirmClose,
    exited: bool,
    integrated: bool,
    running: bool,
    fg_is_shell: Option<bool>,
) -> bool {
    use bt_core::config::ConfirmClose;
    match mode {
        ConfirmClose::False => false,
        _ if exited => false,
        ConfirmClose::Always => true,
        ConfirmClose::True if integrated => running,
        ConfirmClose::True => fg_is_shell != Some(true),
    }
}

/// 줄바꿈이 든 붙여넣기를 물을 글(#109): 글이 든 줄 수와 첫 줄(제어 문자를 지우고 30칸까지)
fn paste_prompt(text: &str) -> String {
    let mut lines = text.split(['\n', '\r']).filter(|l| !l.trim().is_empty());
    let first = lines.next().unwrap_or("");
    let n = 1 + lines.count();
    let first = bt_render::truncate(bt_core::proto::printable(first).trim(), 30);
    format!("붙여넣을 글({n}줄)의 줄바꿈이 바로 실행될 수 있습니다: {first}")
}

/// 퀵 창 움직임의 한 프레임(약 120Hz)
const QUICK_FRAME: Duration = Duration::from_millis(8);

/// 퀵 창 움직임 하나(논리 점): `from`에서 `to`로, 끝나면 `hide`면 숨긴다
struct QuickAnim {
    from: (f64, f64),
    to: (f64, f64),
    start: Instant,
    dur: Duration,
    hide: bool,
}

/// 확인을 기다리는 일(#108)
#[derive(Debug, Clone, PartialEq)]
enum Pending {
    ClosePane(u64),
    /// 이 칸이 든 탭
    CloseTab(u64),
    CloseWindow,
    Quit,
    /// 줄바꿈이 든 붙여넣기(#109): 이 칸에 이 글을
    Paste {
        pane: u64,
        text: String,
    },
}

/// 창 위에 띄운 확인 막대: Enter·y면 한다, Esc·n이면 그만둔다
struct Confirm {
    pending: Pending,
    message: String,
    /// "닫기", "끝내기"
    yes: &'static str,
}

/// btctl new의 띄우기 값(#115): 키 묶음의 새 탭처럼 설정(`base`: command·working-directory)에서 시작하고,
/// 준 폴더·명령이 이긴다
fn ctl_spawn(base: SpawnOptions, cwd: Option<std::path::PathBuf>, command: &[String]) -> SpawnOptions {
    let mut opts = base;
    if cwd.is_some() {
        opts.cwd = cwd;
    }
    if let Some((prog, args)) = command.split_first() {
        opts.program = Some(prog.clone());
        opts.args = args.to_vec();
    }
    opts
}

/// 벨이 울렸을 때 할 일(#107)
#[derive(Debug, PartialEq)]
struct BellResponse {
    /// 칸 제목 앞에 🔔
    title: bool,
    /// Dock 아이콘 튀기기
    attention: bool,
    /// 시스템 경고음
    beep: bool,
    /// `bell-audio-path`의 소리 파일 재생(#165)
    audio: bool,
}

/// `watching`: 창이 앞에 있고 그 칸이 보인다. `window_focused`: 그 칸의 창이 앞에 있다.
/// `has_audio_file`: `bell-audio-path`가 정해져 있다(없으면 `audio`를 켜도 재생할 것이 없다)
fn bell_response(
    f: bt_core::config::BellFeatures,
    has_audio_file: bool,
    watching: bool,
    window_focused: bool,
) -> BellResponse {
    BellResponse {
        title: f.title && !watching,
        attention: f.attention && !window_focused,
        beep: f.system,
        audio: f.audio && has_audio_file,
    }
}

/// 벨 소리(경고음·소리 파일)를 지금 낼지: 앞 소리에서 `since_last`가 지났고(`None`은 처음),
/// 소리 파일이 아직 나고 있지 않아야 한다(#165). 쏟아지는 벨이 소리를 겹쳐 쌓지 않게 한다
fn bell_sound_due(since_last: Option<Duration>, still_playing: bool) -> bool {
    !still_playing && since_last.is_none_or(|d| d >= Duration::from_millis(100))
}

/// 명령이 끝났을 때 할 일(#161)
#[derive(Debug, PartialEq)]
struct FinishResponse {
    /// 벨이 울린 것처럼
    bell: bool,
    /// 데스크톱 알림
    notify: bool,
}

/// 끝난 명령에 대해 무엇을 할지. `duration`은 걸린 시간, `watching`은 창이 앞에 있고 그 칸이 보임.
/// `notify-on-command-finish`를 적었으면 새 키들을 따르고, 적지 않았으면 옛 `notify-command-seconds`
/// (창을 보지 않을 때 데스크톱 알림만)를 따른다
fn command_finish_response(c: &bt_core::config::Config, duration: Duration, watching: bool) -> FinishResponse {
    let none = FinishResponse {
        bell: false,
        notify: false,
    };
    match c.notify_on_command_finish {
        // 새 키를 적지 않았다: 옛 동작(창을 보지 않을 때 데스크톱 알림만)
        None => {
            let limit = c.notify_command_seconds;
            FinishResponse {
                bell: false,
                notify: limit > 0 && duration.as_secs() >= limit && !watching,
            }
        }
        Some(NotifyOnFinish::Never) => none,
        Some(mode) => {
            if (mode == NotifyOnFinish::Unfocused && watching) || duration < c.notify_on_command_finish_after {
                return none;
            }
            let a = c.notify_on_command_finish_action;
            FinishResponse {
                bell: a.bell,
                notify: a.notify,
            }
        }
    }
}

/// 탭 줄의 탭 하나: 칸들을 나눈 트리와 고른 칸(#35)
struct TabGroup {
    layout: Node,
    focus: u64,
    /// 고른 칸만 탭 전체에 크게 보인다(toggle_split_zoom)
    zoomed: bool,
}

/// 닫고 `undo-timeout` 동안 들고 있는 것(#164). 셸(Session)을 떨어뜨리지 않아 되살리면 그대로 이어진다
enum Closed {
    /// 분할의 칸 하나: 원래 트리 자리
    Pane { pane: Box<Pane>, slot: Slot },
    /// 탭 하나(칸 전부와 배치)
    Tab {
        panes: Vec<Pane>,
        layout: Node,
        index: usize,
        focus: u64,
        zoomed: bool,
    },
}

impl Closed {
    fn holds(&self, id: u64) -> bool {
        match self {
            Closed::Pane { pane, .. } => pane.id == id,
            Closed::Tab { panes, .. } => panes.iter().any(|p| p.id == id),
        }
    }
}

/// 되살린 것을 다시 닫는 일(`redo`, #164): 되살린 칸 id
#[derive(Debug, Clone, Copy, PartialEq)]
enum Redo {
    Pane(u64),
    /// 탭의 칸 하나
    Tab(u64),
}

/// btctl wait 하나: 칸, 기다리는 글, 답할 곳, 기한(#66)
struct TextWaiter {
    pane: u64,
    text: String,
    req_id: u64,
    reply: std::sync::mpsc::SyncSender<bt_core::proto::Response>,
    deadline: Instant,
}

/// btctl wait가 살피는 줄 수(화면과 최근 스크롤백)
const WAIT_LINES: usize = 500;
/// btctl read --lines의 상한과 답 글의 상한(제어 소켓 한 줄 1MiB 안)
const READ_MAX_LINES: usize = 100_000;
const READ_MAX_BYTES: usize = 900 * 1024;

/// 글이 길면 앞을 버리고 끝 `max`바이트 안쪽만 남긴다(글자 경계에서). 버렸으면 참
fn keep_tail(text: String, max: usize) -> (String, bool) {
    if text.len() <= max {
        return (text, false);
    }
    let mut cut = text.len() - max;
    while !text.is_char_boundary(cut) {
        cut += 1;
    }
    (text[cut..].to_string(), true)
}

/// 찾기 막대(#39): 찾는 칸, 친 글, 결과
struct SearchBar {
    pane: u64,
    query: String,
    status: Option<bt_term::search::SearchStatus>,
}

/// 찾기 막대 오른쪽에 보일 결과 글
fn search_status_text(query: &str, status: Option<bt_term::search::SearchStatus>) -> String {
    let Some(st) = status.filter(|_| !query.is_empty()) else {
        return String::new();
    };
    let plus = if st.capped { "+" } else { "" };
    match st.index {
        _ if st.total == 0 => "없음".into(),
        Some(i) => format!("{i}/{}{plus}", st.total),
        None => format!("{}{plus}개", st.total),
    }
}

/// 그릴 칸 하나(다시 그릴 때 잠깐 쓴다)
struct PaneView {
    id: u64,
    outer: Rect,
    inner: Rect,
    screen: Screen,
    focused: bool,
    progress: Option<(u8, u8)>,
}

struct Gfx {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    _context: softbuffer::Context<Rc<Window>>,
}

/// 창 하나의 상태(#36). 칸(셸)은 모든 창이 App에서 함께 든다
struct Win {
    gfx: Option<Gfx>,
    /// 항상 위에 띄우는 중(`toggle_window_float_on_top`, #148)
    float_on_top: bool,
    tabs: Vec<TabGroup>,
    /// 보고 있는 탭(tabs의 자리)
    active: usize,
    preedit: String,
    cursor_pos: PhysicalPosition<f64>,
    selecting: bool,
    last_click: Option<(Instant, usize, usize, u8)>,
    scroll_accum: f64,
    focused: bool,
    /// 창 안 알림 띠(글, 띄운 때). 2.5초 보인다
    toast: Option<(String, Instant)>,
    /// Cmd를 누른 채 마우스가 올라간 링크(칸 id, 링크)
    hover: Option<(u64, bt_term::links::LinkHit)>,
    /// Cmd-클릭으로 링크를 열었다: 뒤따르는 놓기를 프로그램에 보내지 않는다
    link_click: bool,
    /// 열린 찾기 막대(칸 하나에 붙는다)
    search: Option<SearchBar>,
    /// 열린 명령 팔레트(#77): 친 글, 고른 줄
    palette: Option<(String, usize)>,
    /// 퀵 터미널 창(#95): 화면 위쪽에 떠서 전역 단축키로 보였다 숨는다. 창 상태 복원에서 뺀다
    quick: bool,
    /// 퀵 터미널을 보인 때. 직후의 포커스 잃음(앱이 앞으로 오기 전)은 숨기지 않는다
    quick_shown: Option<Instant>,
    /// 퀵 창이 놓인 자리와 숨을 때 나가는 자리(#110)
    quick_place: Option<(crate::quick::Rect, (f64, f64))>,
    /// 퀵 창의 들어오고 나가는 움직임(#110)
    quick_anim: Option<QuickAnim>,
    /// 키 순서(`ctrl+a>n`)의 기다리는 앞 키들(#106)
    keyseq: crate::keymap::Sequencer,
    /// 닫기·끝내기 확인 막대(#108)
    confirm: Option<Confirm>,
    /// 칸 제목 입력 막대(#124): 그 칸과 친 글
    title_prompt: Option<(u64, String)>,
    /// 배경 그림 층(#134): (창 폭, 높이, 기본 바탕색)과 미리 합성한 픽셀
    backdrop: Option<((u32, u32, u32), Vec<u32>)>,
    /// 바뀐 것이 없어도 다음 그리기에서 창에 한 번 올린다(창이 다른 화면으로 옮겨 가면 그 화면의 색 공간으로, #97)
    force_present: bool,
    /// 들고 있는 프레임과 그 내용(바뀐 줄만 다시 그리려고, #40)
    frame: Vec<u32>,
    last_frame: Option<FrameMemo>,
    /// 커서 깜빡임: 지금 보이는가, 다음에 바꿀 때, 깜빡일 커서가 있는가
    blink_on: bool,
    blink_next: Instant,
    blink_wanted: bool,
}

impl Win {
    fn new() -> Self {
        Self {
            gfx: None,
            float_on_top: false,
            tabs: Vec::new(),
            active: 0,
            preedit: String::new(),
            cursor_pos: PhysicalPosition::new(0.0, 0.0),
            selecting: false,
            last_click: None,
            scroll_accum: 0.0,
            // 포커스 사건이 오기 전에는 보고 있지 않은 창으로 둔다. 보고 있다고 잘못 알면
            // 그 창의 OSC 52 클립보드 쓰기를 받게 된다(보안 검토, #36)
            focused: false,
            toast: None,
            hover: None,
            link_click: false,
            search: None,
            palette: None,
            quick: false,
            quick_shown: None,
            quick_place: None,
            quick_anim: None,
            keyseq: Default::default(),
            confirm: None,
            title_prompt: None,
            backdrop: None,
            force_present: false,
            frame: Vec::new(),
            last_frame: None,
            blink_on: true,
            blink_next: Instant::now(),
            blink_wanted: false,
        }
    }

    fn id(&self) -> Option<WindowId> {
        self.gfx.as_ref().map(|g| g.window.id())
    }

    fn holds(&self, pane: u64) -> bool {
        self.tabs.iter().any(|g| g.layout.contains(pane))
    }
}

pub struct App {
    config: AppConfig,
    proxy: EventLoopProxy<UserEvent>,
    redraw_pending: Arc<AtomicBool>,
    fonts: Option<Fonts>,
    font_size: f32,
    scale: f64,
    /// 모든 탭의 칸들(탭 순서와 상관없다)
    panes: Vec<Pane>,
    next_id: u64,
    mods: ModifiersState,
    clipboard: Option<arboard::Clipboard>,
    first: Option<SpawnOptions>,
    next_poll: Instant,
    /// 셸 통합 스크립트를 풀어 둔 폴더(없으면 셸 통합을 싣지 않는다)
    shell_integration_dir: Option<std::path::PathBuf>,
    /// btctl run이 기다리는 탭: 탭 id → (요청 id, 답할 통로, 기한)
    run_waiters: std::collections::HashMap<u64, (u64, std::sync::mpsc::SyncSender<bt_core::proto::Response>, Instant)>,
    /// 읽어 둔 벨 소리 파일(`bell-audio-path`, #165). 경로가 바뀌면 다시 읽는다
    bell_sound: Option<crate::bell_sound::BellSound>,
    /// 마지막으로 벨 소리 파일을 울린 때
    last_bell_audio: Option<Instant>,
    /// btctl wait가 기다리는 글(#66)
    text_waiters: Vec<TextWaiter>,
    /// 메뉴 막대(살려 두어야 메뉴가 남는다)
    #[cfg(target_os = "macos")]
    menu: Option<muda::Menu>,
    control: Option<crate::control::ControlServer>,
    keymap: Keymap,
    /// --config-file(설정을 다시 읽을 때 같은 파일을 읽는다)
    config_extra: Option<std::path::PathBuf>,
    /// 지금 창(사건을 받은 창)의 상태. 다른 창의 사건이 오면 `others`와 바꿔 끼운다(#36)
    w: Win,
    others: Vec<Win>,
    /// 마지막으로 보던 창(창 밖에서 온 btctl 요청이 쓴다)
    last_focused: Option<WindowId>,
    /// 전역 단축키(#95): 등록한 관리자와 단축키 번호 → 동작
    hotkeys: Option<global_hotkey::GlobalHotKeyManager>,
    /// 마지막 경고음(벨이 몰려도 0.1초에 한 번만, #107)
    last_beep: Option<Instant>,
    /// write_*_file로 쓴 파일들. 앱이 끝날 때 지운다(#125)
    written_files: Vec<std::path::PathBuf>,
    /// 풀어 둔 배경 그림(background-image, #134)
    bg_image: Option<Arc<bt_render::background::BgImage>>,
    hotkey_registered: Vec<global_hotkey::hotkey::HotKey>,
    hotkey_actions: std::collections::HashMap<u32, Action>,
    /// 닫고 들고 있는 칸·탭(`undo-timeout`, #164)과 되살린 것을 다시 닫는 일(`redo`)
    undo: UndoStack<Closed>,
    redo: UndoStack<Redo>,
    /// 마지막 창이 닫힌 뒤 앱을 끝낼 때(`quit-after-last-window-closed-delay`, #178). 새 창이 열리면 지운다
    quit_deadline: Option<Instant>,
}

impl App {
    pub fn new(
        config: AppConfig,
        config_extra: Option<std::path::PathBuf>,
        proxy: EventLoopProxy<UserEvent>,
        first: SpawnOptions,
    ) -> Self {
        let mut config = config;
        let font_size = config.font_size;
        let (keymap, mut kw) = Keymap::build(&config.keybinds);
        kw.extend(crate::palette::user_warnings(&config.palette_entries));
        for w in &kw {
            log::warn!("설정: {w}");
        }
        config.warnings.extend(kw);
        Self {
            config,
            proxy,
            redraw_pending: Arc::new(AtomicBool::new(false)),
            fonts: None,
            font_size,
            scale: 1.0,
            panes: Vec::new(),
            next_id: 1,
            mods: ModifiersState::empty(),
            clipboard: arboard::Clipboard::new().ok(),
            first: Some(first),
            next_poll: Instant::now(),
            #[cfg(target_os = "macos")]
            menu: None,
            shell_integration_dir: None,
            run_waiters: std::collections::HashMap::new(),
            text_waiters: Vec::new(),
            control: None,
            keymap,
            config_extra,
            w: Win::new(),
            others: Vec::new(),
            last_focused: None,
            hotkeys: None,
            last_beep: None,
            bell_sound: None,
            last_bell_audio: None,
            written_files: Vec::new(),
            bg_image: None,
            hotkey_registered: Vec::new(),
            hotkey_actions: std::collections::HashMap::new(),
            undo: UndoStack::default(),
            redo: UndoStack::default(),
            quit_deadline: None,
        }
    }

    fn load_fonts(&mut self) {
        let cfg = font_config(&self.config, self.font_size, self.scale as f32);
        match Fonts::new(&cfg) {
            Ok(f) => {
                log::info!("글꼴: {} {}pt × {}", f.family_name, self.font_size, self.scale);
                self.fonts = Some(f);
            }
            Err(e) => {
                log::error!("{e}");
                self.fonts = None;
            }
        }
    }

    /// 탭 줄 아래 칸들이 차지하는 자리. 여백은 칸마다 따로 뺀다(Ghostty처럼)
    fn content_rect(&self) -> Rect {
        let Some(g) = &self.w.gfx else { return Rect::default() };
        let size = g.window.inner_size();
        let top = self.tabbar_height();
        Rect::new(0, top as i32, size.width, size.height.saturating_sub(top))
    }

    /// 칸의 바깥 사각형에서 여백을 뺀 격자 자리
    fn inner_rect(&self, r: Rect) -> Rect {
        let px = |pt: f32| (pt as f64 * self.scale).round() as i32;
        let (left, right) = (px(self.config.padding_x.0), px(self.config.padding_x.1));
        let (top, bottom) = (px(self.config.padding_y.0), px(self.config.padding_y.1));
        let inner = Rect::new(
            r.x + left,
            r.y + top,
            (r.w as i32 - left - right).max(0) as u32,
            (r.h as i32 - top - bottom).max(0) as u32,
        );
        // 남는 자리를 네 변에 고르게(window-padding-balance, #136)
        match (&self.fonts, self.config.padding_balance) {
            (Some(f), true) => crate::padding::balance(inner, f.metrics().width, f.metrics().height),
            _ => inner,
        }
    }

    /// 칸 사이 경계선 두께(픽셀)
    fn divider_px(&self) -> u32 {
        (self.scale.round() as u32).max(1)
    }

    /// 탭 하나의 칸 배치(바깥 사각형). 크게 보기면 고른 칸만 전체를 차지한다
    fn group_layout(&self, gi: usize) -> Vec<(u64, Rect)> {
        let Some(g) = self.w.tabs.get(gi) else {
            return Vec::new();
        };
        let area = self.content_rect();
        if g.zoomed {
            return vec![(g.focus, area)];
        }
        g.layout.layout(area, self.divider_px())
    }

    /// 칸의 바깥 사각형(보이지 않으면 없다)
    fn pane_rect(&self, id: u64) -> Option<Rect> {
        let gi = self.group_of(id)?;
        self.group_layout(gi)
            .into_iter()
            .find(|(i, _)| *i == id)
            .map(|(_, r)| r)
    }

    fn pane_index(&self, id: u64) -> Option<usize> {
        self.panes.iter().position(|p| p.id == id)
    }

    /// 칸이 든 탭
    fn group_of(&self, id: u64) -> Option<usize> {
        self.w.tabs.iter().position(|g| g.layout.contains(id))
    }

    /// 보고 있는 탭의 고른 칸 id
    fn focused_id(&self) -> Option<u64> {
        self.w.tabs.get(self.w.active).map(|g| g.focus)
    }

    /// 칸이 지금 창에 보이는가(보고 있는 탭에 있고 크게 보기에 가리지 않았다)
    fn is_visible(&self, idx: usize) -> bool {
        let Some(p) = self.panes.get(idx) else { return false };
        self.w.tabs.get(self.w.active).is_some_and(|g| {
            if g.zoomed {
                g.focus == p.id
            } else {
                g.layout.contains(p.id)
            }
        })
    }

    /// 키 입력을 받는 칸인가
    fn is_focused(&self, idx: usize) -> bool {
        self.panes.get(idx).is_some_and(|p| Some(p.id) == self.focused_id())
    }

    /// 탭 줄 높이. 탭이 하나면 없다(Ghostty처럼)
    fn tabbar_height(&self) -> u32 {
        match &self.fonts {
            Some(f) if self.config.window_show_tab_bar.visible(self.w.tabs.len()) => tabbar::height_px(f),
            _ => 0,
        }
    }

    fn activate(&mut self, index: usize) {
        if index < self.w.tabs.len() && index != self.w.active {
            let before = self.focused_id();
            self.w.active = index;
            self.focus_changed(before);
        }
        self.clear_visible_unread();
        self.request_redraw();
    }

    /// 칸 하나를 고른다(그 칸의 탭으로 옮겨 간다). 크게 보기 중에 다른 칸을 고르면 크게 보기를 푼다
    fn focus_pane(&mut self, id: u64) {
        let Some(gi) = self.group_of(id) else { return };
        let before = self.focused_id();
        self.w.active = gi;
        let g = &mut self.w.tabs[gi];
        let unzoom = g.zoomed && g.focus != id;
        g.focus = id;
        if unzoom {
            g.zoomed = false;
            self.resize_all();
        }
        self.focus_changed(before);
        self.clear_visible_unread();
        self.request_redraw();
    }

    /// 고른 칸이 바뀌었다: 포커스 보고(DEC 1004)를 켠 프로그램에 나가고 들어옴을 알린다
    fn focus_changed(&mut self, before: Option<u64>) {
        let after = self.focused_id();
        // 찾기 막대는 그 칸에 붙어 있다. 다른 칸으로 옮기면 닫는다
        if self.w.search.as_ref().is_some_and(|s| Some(s.pane) != after) {
            self.end_search();
        }
        self.w.selecting = false;
        if before == after || !self.w.focused {
            return;
        }
        for (id, seq) in [(before, b"\x1b[O"), (after, b"\x1b[I")] {
            if let Some(p) = id.and_then(|id| self.pane_index(id)).map(|i| &self.panes[i]) {
                if p.session.modes().focus_events {
                    p.session.write(seq);
                }
            }
        }
    }

    /// 보이는 칸들의 안 읽음과 벨 표시를 지운다
    fn clear_visible_unread(&mut self) {
        for i in 0..self.panes.len() {
            if self.is_visible(i) {
                self.panes[i].unread = false;
                self.panes[i].bell = false;
            }
        }
    }

    /// 전경 프로세스 이름을 본다(셸이 제목을 보내지 않을 때 탭 이름으로 쓴다). 바뀐 것이 있으면 참
    fn poll_foreground(&mut self) -> bool {
        let mut changed = false;
        for t in &mut self.panes {
            let Some(pid) = t.session.foreground_pid() else {
                continue;
            };
            if t.fg_pid == Some(pid) {
                continue;
            }
            let was_other = t.fg_pid.is_some();
            t.fg_pid = Some(pid);
            t.fg_name = bt_term::process::process_name(pid);
            changed = true;
            // 다른 프로그램이 끝나 셸로 돌아왔는데 마우스 모드가 켜져 있으면 끈다(#24).
            // 셸 자신(탭이 띄운 프로세스)이 전경일 때만: 셸이 일부러 켠 경우는 드물다
            let back_to_shell = t.session.pid().map(|p| p as i32) == Some(pid);
            if back_to_shell && was_other && self.config.mouse_reset_on_shell && t.session.reset_mouse_modes() {
                log::info!("탭 {}: 셸로 돌아왔는데 마우스 모드가 켜져 있어 껐습니다", t.id);
            }
        }
        changed
    }

    /// 탭이 알림을 보냈다. 보고 있는 탭이 아니면 안 읽음으로 두고, 창을 보고 있지 않으면 데스크톱 알림을 띄운다
    fn on_notification(&mut self, idx: usize, note: bt_term::Notification) {
        let watching = self.w.focused && self.is_visible(idx);
        self.notify_tab(idx, note, !watching);
    }

    /// 안 읽음 표시와 데스크톱 알림. `desktop`이 거짓이면 데스크톱 알림은 띄우지 않는다
    fn notify_tab(&mut self, idx: usize, note: bt_term::Notification, desktop: bool) {
        let watching = self.w.focused && self.is_visible(idx);
        let t = &mut self.panes[idx];
        if !watching {
            t.unread = true;
        }
        if desktop {
            // 같은 탭의 데스크톱 알림은 3초에 한 번(출력에 알림이 몰려도 화면을 덮지 않게)
            let now = Instant::now();
            if t.last_desktop.is_none_or(|at| now - at >= Duration::from_secs(3)) {
                t.last_desktop = Some(now);
                let title = if note.title.is_empty() {
                    t.display_title()
                } else {
                    note.title.clone()
                };
                crate::desktop::notify(&title, &note.body);
            }
        }
        self.request_redraw();
    }

    /// 칸이 벨을 울렸다(`bell-features`, #107). 지금 창은 그 칸의 창이다(switch_to_pane 뒤)
    fn on_bell(&mut self, idx: usize) {
        let watching = self.w.focused && self.is_visible(idx);
        let r = bell_response(
            self.config.bell_features,
            self.config.bell_audio_path.is_some(),
            watching,
            self.w.focused,
        );
        if r.title && !self.panes[idx].bell {
            self.panes[idx].bell = true;
            self.request_redraw();
        }
        if r.attention {
            if let Some(g) = &self.w.gfx {
                g.window
                    .request_user_attention(Some(winit::window::UserAttentionType::Informational));
            }
        }
        if r.audio {
            self.play_bell_audio();
        }
        if r.beep && bell_sound_due(self.last_beep.map(|t| t.elapsed()), false) {
            self.last_beep = Some(Instant::now());
            #[cfg(target_os = "macos")]
            objc2_app_kit::NSBeep();
        }
    }

    /// 벨 소리 파일을 울린다(#165). 파일은 처음 울릴 때(또는 경로가 바뀌었을 때) 읽는다.
    /// 읽지 못하면 조용히 지나간다. 앞 소리가 아직 나거나 0.1초 안이면 건너뛴다
    fn play_bell_audio(&mut self) {
        let Some(path) = self.config.bell_audio_path.clone() else {
            return;
        };
        if self.bell_sound.as_ref().is_none_or(|s| s.path() != path) {
            self.bell_sound = crate::bell_sound::BellSound::load(&path);
            if self.bell_sound.is_none() {
                log::warn!("bell-audio-path를 읽지 못했습니다: {}", path.display());
            }
        }
        let Some(sound) = &self.bell_sound else {
            return;
        };
        if bell_sound_due(self.last_bell_audio.map(|t| t.elapsed()), sound.is_playing()) {
            self.last_bell_audio = Some(Instant::now());
            sound.play(self.config.bell_audio_volume);
        }
    }

    /// 셸 통합이 알린 명령의 끝: btctl run에 답하고, 오래 걸린 명령이면(창을 보지 않을 때) 알린다
    fn on_command_finished(&mut self, idx: usize, rec: bt_term::CommandRecord) {
        let tab_id = self.panes[idx].id;
        self.panes[idx].running = None;
        if let Some((req_id, reply, _)) = self.run_waiters.remove(&tab_id) {
            let _ = reply.send(bt_core::proto::Response::ok(
                req_id,
                serde_json::to_value(command_result(&rec)).unwrap_or_default(),
            ));
        }
        let watching = self.w.focused && self.is_visible(idx);
        let r = command_finish_response(&self.config, rec.duration, watching);
        if r.bell {
            self.on_bell(idx);
        }
        if r.notify {
            let cmd = rec.command.clone().unwrap_or_else(|| "명령".into());
            let code = rec
                .exit_code
                .map(|c| format!("종료 코드 {c}"))
                .unwrap_or_else(|| "끝남".into());
            let body = format!("{} — {code}, {}초", truncate_chars(&cmd, 60), rec.duration.as_secs());
            let title = if rec.exit_code.is_some_and(|c| c != 0) {
                "명령이 실패했습니다"
            } else {
                "명령이 끝났습니다"
            };
            // 보고 있어도(always) 띄운다
            self.notify_tab(
                idx,
                bt_term::Notification {
                    title: title.into(),
                    body,
                },
                true,
            );
        }
        self.panes[idx].last_command = Some(rec);
    }

    /// 창 오른쪽 아래에 잠깐 알림 띠를 띄운다
    fn toast(&mut self, text: String) {
        self.w.toast = Some((text, Instant::now()));
        self.request_redraw();
    }

    /// 칸의 바깥 사각형에 들어가는 격자 크기
    fn grid_size_for(&self, outer: Rect) -> GridSize {
        let Some(f) = &self.fonts else {
            return GridSize::new(80, 24);
        };
        let m = f.metrics();
        let Rect { w, h, .. } = self.inner_rect(outer);
        GridSize {
            cols: (w / m.width).max(2) as u16,
            rows: (h / m.height).max(1) as u16,
            cell_width: m.width as u16,
            cell_height: m.height as u16,
        }
    }

    fn sink(&self, id: u64) -> bt_term::EventSink {
        let proxy = self.proxy.clone();
        let pending = self.redraw_pending.clone();
        Arc::new(move |e: SessionEvent| match e {
            SessionEvent::Output => {
                // 출력이 몰려도 깨우기는 한 번만 보낸다
                if !pending.swap(true, Ordering::AcqRel) {
                    let _ = proxy.send_event(UserEvent::Redraw);
                }
            }
            other => {
                let _ = proxy.send_event(UserEvent::Session(id, other));
            }
        })
    }

    /// 셸 하나를 띄워 칸을 만든다(아직 어느 탭에도 넣지 않는다)
    fn spawn_pane(&mut self, options: SpawnOptions, size: GridSize) -> Option<Pane> {
        let id = self.next_id;
        self.next_id += 1;
        let mut options = SpawnOptions {
            scrollback: self.config.scrollback_lines,
            cursor_shape: Some(match self.config.cursor_style {
                bt_core::config::CursorStyle::Block => CursorShape::Block,
                bt_core::config::CursorStyle::Bar => CursorShape::Beam,
                bt_core::config::CursorStyle::Underline => CursorShape::Underline,
                bt_core::config::CursorStyle::BlockHollow => CursorShape::HollowBlock,
            }),
            cursor_blink: self.config.cursor_blink.unwrap_or(false),
            clipboard_write: self.config.clipboard_write,
            clipboard_read: self.config.clipboard_read,
            word_chars: Some(self.config.selection_word_chars.clone()),
            scroll_on_output: self.config.scroll_to_bottom.output,
            title_report: self.config.title_report,
            osc_color_report_format: self.config.osc_color_report_format,
            ..options
        };
        // 칸 안의 btctl이 이 터미널과 이 칸을 찾게 한다
        if let Some(c) = &self.control {
            options
                .env
                .push((bt_core::proto::ENV_SOCKET.into(), c.path.to_string_lossy().into_owned()));
        }
        options.env.push((bt_core::proto::ENV_TAB.into(), id.to_string()));
        if let Some(dir) = &self.shell_integration_dir {
            crate::shellint::apply(&mut options, dir, &self.config.shell_integration);
        }
        match Session::spawn(options, size, self.config.theme.clone(), self.sink(id)) {
            Ok(session) => Some(Pane {
                id,
                session,
                title: String::new(),
                exited: None,
                held_after_exit: false,
                fg_name: None,
                fg_pid: None,
                unread: false,
                bell: false,
                user_title: None,
                created: Instant::now(),
                overlay_until: None,
                cwd: None,
                last_desktop: None,
                progress: None,
                running: None,
                last_command: None,
            }),
            Err(e) => {
                log::error!("셸을 띄우지 못했습니다: {e}");
                None
            }
        }
    }

    /// 새 탭(칸 하나)을 연다. 돌려주는 값은 칸 id
    pub fn open_tab(&mut self, options: SpawnOptions) -> Option<u64> {
        // 탭이 하나에서 둘이 되면 탭 줄이 생긴다. 새 칸은 탭 줄을 뺀 크기로 띄운다
        let mut area = self.content_rect();
        let now = self.config.window_show_tab_bar.visible(self.w.tabs.len());
        let next = self.config.window_show_tab_bar.visible(self.w.tabs.len() + 1);
        if next && !now {
            let bar = self.fonts.as_ref().map(tabbar::height_px).unwrap_or(0);
            area.y += bar as i32;
            area.h = area.h.saturating_sub(bar);
        }
        let size = self.grid_size_for(area);
        let pane = self.spawn_pane(options, size)?;
        let id = pane.id;
        self.panes.push(pane);
        let before = self.focused_id();
        let at = self
            .config
            .window_new_tab_position
            .insert_index(self.w.active, self.w.tabs.len());
        self.w.tabs.insert(
            at,
            TabGroup {
                layout: Node::Leaf(id),
                focus: id,
                zoomed: false,
            },
        );
        self.w.active = at;
        self.focus_changed(before);
        self.resize_all();
        Some(id)
    }

    /// 고른 칸을 나눠 새 칸을 연다. `dir`이 없으면 긴 쪽으로 나눈다
    fn open_split(&mut self, dir: Option<Direction>, options: SpawnOptions) -> Option<u64> {
        let gi = self.w.active;
        let target = self.focused_id()?;
        let dir = dir.unwrap_or_else(|| match self.pane_rect(target) {
            Some(r) if r.h > r.w => Direction::Down,
            _ => Direction::Right,
        });
        // 나눈 뒤의 배치로 새 칸 크기를 미리 잰다(셸이 처음부터 맞는 크기로 뜨게)
        let mut layout = self.w.tabs[gi].layout.clone();
        let new_id = self.next_id;
        layout.split(target, new_id, dir);
        let rect = layout
            .layout(self.content_rect(), self.divider_px())
            .into_iter()
            .find(|(id, _)| *id == new_id)
            .map(|(_, r)| r)?;
        let size = self.grid_size_for(rect);
        let pane = self.spawn_pane(options, size)?;
        debug_assert_eq!(pane.id, new_id);
        self.panes.push(pane);
        let before = self.focused_id();
        let g = &mut self.w.tabs[gi];
        g.layout = layout;
        g.focus = new_id;
        g.zoomed = false;
        self.focus_changed(before);
        self.resize_all();
        Some(new_id)
    }

    /// 새 칸을 띄울 값: 설정을 따르되, 길(창·탭·분할)별 키가 켜져 있으면 고른 칸의 폴더를 이어받는다
    /// (Ghostty처럼 켜져 있으면 `working-directory`보다 앞선다)
    fn spawn_here(&self, kind: bt_core::config::SpawnKind) -> SpawnOptions {
        // OSC 7로 알려진 폴더, 없으면 프로세스에게 묻는다
        let cwd = self
            .active_pane()
            .and_then(|t| t.cwd.clone().or_else(|| t.session.cwd()));
        let mut opts = self.default_spawn();
        if cwd.is_some() && self.config.inherits_cwd(kind) {
            opts.cwd = cwd;
        }
        opts
    }

    /// 설정의 command·working-directory로 새 탭 띄우기 값을 만든다
    fn default_spawn(&self) -> SpawnOptions {
        let mut o = SpawnOptions::default();
        if let Some(cmd) = &self.config.command {
            // Ghostty처럼 셸을 거쳐 띄운다
            let (program, args) = crate::lifecycle::shell_command(cmd);
            o.program = Some(program);
            o.args = args;
        }
        o.cwd = match &self.config.working_directory {
            bt_core::config::WorkingDirectory::Home => std::env::var_os("HOME").map(std::path::PathBuf::from),
            bt_core::config::WorkingDirectory::Path(p) => Some(p.clone()).filter(|p| p.is_dir()),
            // Finder에서 띄우면 지금 폴더가 /라 집 폴더로 둔다
            bt_core::config::WorkingDirectory::Inherit => {
                std::env::current_dir().ok().filter(|p| p != std::path::Path::new("/"))
            }
        };
        o
    }

    /// 제어 소켓 요청 하나를 처리한다
    fn handle_call(&mut self, req: bt_core::proto::Request, event_loop: &ActiveEventLoop) -> bt_core::proto::Response {
        use bt_core::proto::{Call, Response, TabInfo};
        use serde_json::json;
        let id = req.id;
        // 칸을 주면 그 칸의 창, 주지 않으면 마지막으로 보던 창에서 처리한다(#36)
        let target = match &req.call {
            Call::Focus { tab } => Some(*tab),
            Call::Send { tab, .. }
            | Call::Read { tab, .. }
            | Call::Wait { tab, .. }
            | Call::Run { tab, .. }
            | Call::LastCommand { tab }
            | Call::Notify { tab, .. }
            | Call::New { tab, .. } => *tab,
            _ => None,
        };
        match target {
            Some(t) => {
                self.switch_to_pane(t);
            }
            None => {
                if let Some(w) = self.last_focused {
                    self.switch_to(w);
                }
            }
        }
        // tab을 주지 않으면 지금 보는 탭
        let pick = |app: &App, tab: Option<u64>| -> Result<usize, Response> {
            match tab {
                Some(t) => app
                    .panes
                    .iter()
                    .position(|x| x.id == t)
                    .ok_or_else(|| Response::err(id, format!("탭 {t}이 없습니다"))),
                None => app
                    .focused_id()
                    .and_then(|f| app.pane_index(f))
                    .ok_or_else(|| Response::err(id, "탭이 없습니다")),
            }
        };
        match req.call {
            Call::Ping => Response::ok(id, json!({ "app": "b-terminal", "version": env!("CARGO_PKG_VERSION") })),
            Call::QuickTerminal => {
                self.toggle_quick(event_loop);
                Response::ok(id, json!({}))
            }
            Call::List => {
                // 지금 창부터 창마다 탭 줄 순서, 탭 안에서는 왼쪽·위부터
                let order: Vec<usize> = std::iter::once(&self.w)
                    .chain(&self.others)
                    .flat_map(|w| w.tabs.iter())
                    .flat_map(|g| g.layout.leaves())
                    .filter_map(|pid| self.pane_index(pid))
                    .collect();
                let tabs: Vec<TabInfo> = order
                    .into_iter()
                    .map(|i| (i, &self.panes[i]))
                    .map(|(i, t)| TabInfo {
                        id: t.id,
                        title: t.display_title(),
                        active: self.is_focused(i),
                        unread: t.unread,
                        cwd: t.cwd.clone(),
                        process: t.fg_name.clone(),
                    })
                    .collect();
                Response::ok(id, serde_json::to_value(tabs).unwrap_or_default())
            }
            Call::New {
                cwd,
                command,
                split,
                tab,
                window,
            } => {
                let opts = ctl_spawn(self.default_spawn(), cwd.filter(|c| c.is_dir()), &command);
                // 창이 하나도 없으면 탭·분할 대신 새 창을 연다(#178)
                if window || self.windowless() {
                    return match self.open_window(event_loop, opts) {
                        Some(tab) => Response::ok(id, json!({ "tab": tab })),
                        None => Response::err(id, "창을 열지 못했습니다"),
                    };
                }
                let Some(split) = split else {
                    return match self.open_tab(opts) {
                        Some(tab) => Response::ok(id, json!({ "tab": tab })),
                        None => Response::err(id, "탭을 열지 못했습니다"),
                    };
                };
                let dir = match split.as_str() {
                    "right" => Some(Direction::Right),
                    "down" => Some(Direction::Down),
                    "left" => Some(Direction::Left),
                    "up" => Some(Direction::Up),
                    "auto" => None,
                    other => return Response::err(id, format!("나눌 방향이 아닙니다: {other}")),
                };
                // 나눌 칸을 먼저 고른다(그 칸의 탭으로 옮겨 간다)
                match pick(self, tab) {
                    Ok(i) => {
                        let pid = self.panes[i].id;
                        self.focus_pane(pid);
                    }
                    Err(r) => return r,
                }
                match self.open_split(dir, opts) {
                    Some(tab) => Response::ok(id, json!({ "tab": tab })),
                    None => Response::err(id, "칸을 나누지 못했습니다"),
                }
            }
            Call::Focus { tab } => match pick(self, Some(tab)) {
                Ok(i) => {
                    let pid = self.panes[i].id;
                    self.focus_pane(pid);
                    Response::ok(id, json!({}))
                }
                Err(r) => r,
            },
            Call::Send { tab, text } => match pick(self, tab) {
                Ok(i) => {
                    self.panes[i].session.write(text.as_bytes());
                    Response::ok(id, json!({}))
                }
                Err(r) => r,
            },
            Call::Read { tab, lines } => match pick(self, tab) {
                Ok(i) => match lines {
                    None => Response::ok(id, json!({ "text": self.panes[i].session.snapshot().text() })),
                    Some(n) => {
                        let text = self.panes[i].session.tail_text(n.clamp(1, READ_MAX_LINES));
                        let (text, truncated) = keep_tail(text, READ_MAX_BYTES);
                        Response::ok(id, json!({ "text": text, "truncated": truncated }))
                    }
                },
                Err(r) => r,
            },
            Call::Wait { tab, text, timeout_ms } => match pick(self, tab) {
                Ok(i) => {
                    if text.is_empty() || text.chars().count() > 4096 {
                        return Response::err(id, "기다릴 글은 1~4096글자여야 합니다");
                    }
                    if self.panes[i].session.tail_text(WAIT_LINES).contains(&text) {
                        return Response::ok(id, json!({ "found": true }));
                    }
                    // 답은 글이 나올 때(check_text_waiters) 보낸다
                    Response {
                        id: u64::MAX,
                        ok: true,
                        result: json!({ "pending_wait": self.panes[i].id, "text": text, "deadline_ms": timeout_ms }),
                        error: None,
                    }
                }
                Err(r) => r,
            },
            Call::Run {
                tab,
                command,
                timeout_ms,
            } => match pick(self, tab) {
                Ok(i) => {
                    let t = &self.panes[i];
                    if !t.session.has_shell_integration() {
                        return Response::err(
                            id,
                            "이 탭은 셸 통합이 없습니다(zsh·bash 탭에서 쓰세요. shell-integration 설정을 보세요)",
                        );
                    }
                    if t.running.is_some() || self.run_waiters.contains_key(&t.id) {
                        return Response::err(id, "이 탭은 다른 명령을 돌리는 중입니다");
                    }
                    if command.contains('\n') || command.contains('\r') {
                        return Response::err(id, "명령은 한 줄이어야 합니다");
                    }
                    let tab_id = t.id;
                    t.session.write(format!("{command}\r").as_bytes());
                    // 답은 명령이 끝날 때(on_command_finished) 보낸다. 여기서는 빈 답을 돌려주지 않게 표시만 남긴다
                    Response {
                        id: u64::MAX,
                        ok: true,
                        result: json!({ "pending": tab_id, "deadline_ms": timeout_ms }),
                        error: None,
                    }
                }
                Err(r) => r,
            },
            Call::LastCommand { tab } => match pick(self, tab) {
                Ok(i) => match &self.panes[i].last_command {
                    Some(rec) => Response::ok(id, serde_json::to_value(command_result(rec)).unwrap_or_default()),
                    None => Response::err(id, "이 탭에서 셸 통합으로 끝난 명령이 아직 없습니다"),
                },
                Err(r) => r,
            },
            Call::ReloadConfig => {
                self.reload_config();
                Response::ok(
                    id,
                    json!({ "warnings": self.config.warnings, "sources": self.config.sources }),
                )
            }
            Call::Notify { tab, title, body } => match pick(self, tab) {
                Ok(i) => {
                    self.on_notification(i, bt_term::Notification { title, body });
                    Response::ok(id, json!({}))
                }
                Err(r) => r,
            },
        }
    }

    /// 이 칸을 닫기 전에 물어야 하는가(`confirm-close-surface`, #108)
    fn pane_needs_confirm(&self, id: u64) -> bool {
        let Some(p) = self.pane_index(id).map(|i| &self.panes[i]) else {
            return false;
        };
        let fg_is_shell = match (p.session.foreground_pid(), p.session.pid()) {
            (Some(fg), Some(sh)) => Some(fg == sh as i32),
            _ => None,
        };
        pane_busy(
            self.config.confirm_close,
            p.exited.is_some(),
            p.session.has_shell_integration(),
            p.running.is_some(),
            fg_is_shell,
        )
    }

    /// 닫기·끝내기를 청한다. 프로그램이 도는 칸이 있으면 확인 막대를 띄우고, 없으면 바로 한다
    fn request(&mut self, pending: Pending, event_loop: &ActiveEventLoop) {
        let ids: Vec<u64> = match &pending {
            Pending::ClosePane(id) => vec![*id],
            Pending::CloseTab(id) => self
                .group_of(*id)
                .map(|gi| self.w.tabs[gi].layout.leaves())
                .unwrap_or_default(),
            Pending::CloseWindow => self.w.tabs.iter().flat_map(|g| g.layout.leaves()).collect(),
            Pending::Quit => self.panes.iter().map(|p| p.id).collect(),
            // 붙여넣기는 묻기로 정한 뒤에 ask로 온다
            Pending::Paste { .. } => Vec::new(),
        };
        let busy: Vec<u64> = ids.into_iter().filter(|&id| self.pane_needs_confirm(id)).collect();
        if busy.is_empty() {
            self.perform(pending, event_loop);
            return;
        }
        let n = busy.len();
        let (message, yes) = match &pending {
            Pending::ClosePane(id) => {
                let what = self
                    .pane_index(*id)
                    .map(|i| &self.panes[i])
                    .and_then(|p| {
                        p.running
                            .as_ref()
                            .and_then(|r| r.0.clone())
                            .or_else(|| p.fg_name.clone())
                    })
                    .map(|w| bt_render::truncate(&bt_core::proto::printable(&w), 40))
                    .unwrap_or_else(|| "프로그램".into());
                (format!("이 칸에서 {what}이(가) 돌고 있습니다. 닫을까요?"), "닫기")
            }
            Pending::CloseTab(_) => (
                format!("이 탭에서 프로그램이 도는 칸이 {n}개 있습니다. 탭을 닫을까요?"),
                "닫기",
            ),
            Pending::CloseWindow => (
                format!("이 창에서 프로그램이 도는 칸이 {n}개 있습니다. 창을 닫을까요?"),
                "닫기",
            ),
            Pending::Quit => (format!("프로그램이 도는 칸이 {n}개 있습니다. 끝낼까요?"), "끝내기"),
            Pending::Paste { .. } => return,
        };
        self.ask(pending, message, yes);
    }

    /// 확인 막대를 띄운다. 묻는 동안 팔레트는 닫는다
    fn ask(&mut self, pending: Pending, message: String, yes: &'static str) {
        self.w.palette = None;
        self.w.confirm = Some(Confirm { pending, message, yes });
        self.request_redraw();
    }

    /// 고른 글을 시스템 클립보드에 넣는다. `clipboard-trim-trailing-spaces`가 켜져 있으면 줄 끝 공백을 지운다.
    /// 고른 글이 없으면 하지 않는다. 했으면 true
    fn copy_selection(&mut self) -> bool {
        let Some(mut text) = self.active_pane().and_then(|t| t.session.selection_text()) else {
            return false;
        };
        if self.config.clipboard_trim_trailing_spaces {
            text = bt_term::trim_trailing_spaces(&text);
        }
        match self.clipboard.as_mut() {
            Some(cb) => cb.set_text(text).is_ok(),
            None => false,
        }
    }

    /// 복사 명령(단축키·메뉴·오른쪽 클릭)으로 복사한다. `selection-clear-on-copy`가 켜져 있으면 복사한 뒤 고른 영역을 지운다.
    /// `copy-on-select`의 자동 복사는 지우지 않는다(Ghostty)
    fn copy_selection_command(&mut self) -> bool {
        let copied = self.copy_selection();
        if copied && clears_selection_after_copy(self.config.selection_clear_on_copy) {
            if let Some(t) = self.active_pane() {
                t.session.select_clear();
            }
        }
        copied
    }

    /// 마우스 보고 모드가 아닌 칸의 오른쪽 클릭(`right-click-action`). `context-menu`는 메뉴가 없어 하는 일이 없다
    fn right_click(&mut self) {
        let paste = |app: &mut Self| {
            let text = app.clipboard.as_mut().and_then(|cb| cb.get_text().ok());
            if let Some(text) = text.filter(|t| !t.is_empty()) {
                app.paste(text);
            }
        };
        match self.config.right_click_action {
            bt_core::config::RightClickAction::ContextMenu | bt_core::config::RightClickAction::Ignore => {}
            bt_core::config::RightClickAction::Paste => paste(self),
            bt_core::config::RightClickAction::Copy => {
                self.copy_selection_command();
            }
            bt_core::config::RightClickAction::CopyOrPaste => {
                if !self.copy_selection_command() {
                    paste(self);
                }
            }
        }
    }

    /// 클립보드 글을 고른 칸에 붙여넣는다. 괄호 붙여넣기를 쓰지 않는 프로그램에 줄바꿈이 든 글이면 먼저 묻는다(#109)
    fn paste(&mut self, text: String) {
        let Some(t) = self.active_pane() else { return };
        let (id, bracketed) = (t.id, t.session.modes().bracketed_paste);
        if !self.config.paste_protection || bt_term::paste_is_safe(&text, bracketed, self.config.paste_bracketed_safe) {
            self.paste_into(id, &text);
            return;
        }
        let message = paste_prompt(&text);
        self.ask(Pending::Paste { pane: id, text }, message, "붙여넣기");
    }

    /// 그 칸의 지금 모드로 붙여넣기 바이트를 만들어 쓴다
    fn paste_into(&mut self, pane: u64, text: &str) {
        if let Some(t) = self.pane_index(pane).map(|i| &self.panes[i]) {
            let bytes = bt_term::encode_paste(text, t.session.modes().bracketed_paste);
            t.session.select_clear();
            t.session.write_input(&bytes, self.config.scroll_to_bottom.keystroke);
        }
    }

    fn perform(&mut self, pending: Pending, event_loop: &ActiveEventLoop) {
        match pending {
            Pending::ClosePane(id) => {
                if let Some(i) = self.pane_index(id) {
                    self.close_pane(i, event_loop, true);
                }
            }
            Pending::CloseTab(id) => {
                if let Some(gi) = self.group_of(id) {
                    self.close_tab(gi, event_loop);
                }
            }
            Pending::CloseWindow => self.close_current_window(event_loop),
            // 끌 때 창 상태는 exiting에서 적는다
            Pending::Quit => event_loop.exit(),
            Pending::Paste { pane, text } => self.paste_into(pane, &text),
        }
    }

    /// 칸 제목 입력 막대가 떠 있으면 키를 가로챈다: Enter는 정하고, Esc는 그만두고, 글자는 붙인다(#124)
    fn title_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        use winit::keyboard::{Key, NamedKey};
        if self.w.title_prompt.is_none() || self.mods.super_key() || self.mods.control_key() {
            return false;
        }
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => self.w.title_prompt = None,
            Key::Named(NamedKey::Enter) => {
                if let Some((pane, text)) = self.w.title_prompt.take() {
                    self.set_user_title(pane, &text);
                }
            }
            Key::Named(NamedKey::Backspace) => {
                if let Some((_, t)) = self.w.title_prompt.as_mut() {
                    t.pop();
                }
            }
            _ => {
                if let (Some(text), Some((_, t))) = (event.text.as_ref(), self.w.title_prompt.as_mut()) {
                    if !text.chars().any(char::is_control) {
                        t.push_str(text);
                    }
                }
            }
        }
        self.request_redraw();
        true
    }

    /// 지금 칸의 글을 이 사용자 전용 폴더에 파일로 쓰고, 그 경로를 복사·붙여넣기·열기(#125)
    fn write_file(&mut self, source: crate::keymap::FileSource, then: crate::keymap::FileThen) {
        use crate::keymap::{FileSource, FileThen};
        let Some(p) = self.active_pane() else { return };
        let (id, kind, text) = match source {
            // 스크롤백 상한보다 넉넉히: tail_text가 맨 위에서 멈춘다
            FileSource::Scrollback => (p.id, "scrollback", p.session.tail_text(1_000_000)),
            FileSource::Screen => (p.id, "screen", p.session.snapshot().text()),
            FileSource::Selection => match p.session.selection_text().filter(|t| !t.is_empty()) {
                Some(t) => (p.id, "selection", t),
                // Ghostty처럼 고른 글이 없으면 아무것도 하지 않는다
                None => return,
            },
        };
        let path = match crate::dump::dir().and_then(|d| crate::dump::write(&d, kind, &text)) {
            Ok(path) => path,
            Err(e) => {
                log::warn!("파일로 쓰지 못했습니다: {e}");
                self.toast(format!("파일로 쓰지 못했습니다: {e}"));
                return;
            }
        };
        self.written_files.push(path.clone());
        let shown = path.to_string_lossy().into_owned();
        match then {
            FileThen::Copy => {
                if let Some(cb) = self.clipboard.as_mut() {
                    let _ = cb.set_text(shown.clone());
                }
                if self.config.app_notifications.clipboard_copy {
                    self.toast(format!("경로를 복사했습니다: {shown}"));
                }
            }
            FileThen::Paste => self.paste_into(id, &shown),
            FileThen::Open => {
                // 셸을 거치지 않고 인자로 넘긴다
                let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
                if let Err(e) = std::process::Command::new(opener).arg(&path).spawn() {
                    self.toast(format!("열지 못했습니다: {e}"));
                }
            }
        }
    }

    /// 셸 통합이 있는 칸의 프롬프트에서, 커서 줄을 누르면 그 칸까지 ←·→를 보낸다(#137).
    /// 명령이 도는 중·대체 화면·스크롤해 올라가 있으면 하지 않는다
    fn click_to_move(&mut self, idx: usize, col: usize, row: usize) {
        let Some(p) = self.panes.get(idx) else { return };
        let modes = p.session.modes();
        if !p.session.has_shell_integration() || p.running.is_some() || modes.alt_screen {
            return;
        }
        let screen = p.session.snapshot();
        if screen.display_offset != 0 || screen.cursor.row != row {
            return;
        }
        let cells: Vec<(char, bool)> = (0..screen.cols)
            .map(|c| {
                let cell = screen.cell(c, row);
                (cell.ch, cell.flags.contains(CellFlags::SPACER))
            })
            .collect();
        let bytes = crate::mouse::click_to_move(&cells, screen.cursor.col, col, modes.app_cursor);
        if !bytes.is_empty() {
            p.session.select_clear();
            p.session.write(&bytes);
        }
    }

    /// 칸에 사람이 붙인 제목을 정한다. 빈 글이면 지운다(프로그램이 정한 제목으로 돌아감)
    fn set_user_title(&mut self, pane: u64, text: &str) {
        if let Some(i) = self.pane_index(pane) {
            self.panes[i].user_title = clean_user_title(text);
            self.request_redraw_all();
        }
    }

    /// 확인 막대가 떠 있으면 키를 모두 가로챈다: Enter·y는 하고, Esc·n은 그만둔다
    fn confirm_key(&mut self, event: &winit::event::KeyEvent, event_loop: &ActiveEventLoop) -> bool {
        use winit::keyboard::{Key, NamedKey};
        if self.w.confirm.is_none() {
            return false;
        }
        let plain = !(self.mods.super_key() || self.mods.control_key() || self.mods.alt_key());
        let yes = match &event.logical_key {
            Key::Named(NamedKey::Enter) => Some(true),
            Key::Named(NamedKey::Escape) => Some(false),
            Key::Character(c) if plain && c.eq_ignore_ascii_case("y") => Some(true),
            Key::Character(c) if plain && c.eq_ignore_ascii_case("n") => Some(false),
            _ => None,
        };
        if let Some(yes) = yes {
            let c = self.w.confirm.take();
            self.request_redraw();
            if let (true, Some(c)) = (yes, c) {
                self.perform(c.pending, event_loop);
            }
        }
        true
    }

    /// 칸 하나를 닫는다. 탭의 마지막 칸이면 탭이, 마지막 탭이면 앱이 닫힌다.
    /// `undoable`이면 `undo-timeout` 동안 셸을 들고 있어 되살릴 수 있다(창이 비게 되면 들지 않는다)
    fn close_pane(&mut self, index: usize, event_loop: &ActiveEventLoop, undoable: bool) {
        if index >= self.panes.len() {
            return;
        }
        // Session을 떨어뜨리면 PTY가 닫히고 셸은 SIGHUP을 받는다. 들고 있으면 그때까지 떨어뜨리지 않는다
        let pane = self.panes.remove(index);
        let id = pane.id;
        self.fail_text_waiters(|w| w.pane == id, "글이 나오기 전에 칸이 닫혔습니다");
        if let Some((req_id, reply, _)) = self.run_waiters.remove(&id) {
            let _ = reply.send(bt_core::proto::Response::err(
                req_id,
                "명령이 끝나기 전에 칸이 닫혔습니다",
            ));
        }
        let before = self.focused_id();
        let mut held = None;
        if let Some(gi) = self.group_of(id) {
            let g = &mut self.w.tabs[gi];
            let slot = g.layout.slot_of(id);
            let was = (g.zoomed, g.focus);
            let layout = std::mem::replace(&mut g.layout, Node::Leaf(id));
            match layout.remove(id) {
                (Some(rest), next) => {
                    if g.focus == id {
                        g.focus = next.unwrap_or_else(|| rest.leaves()[0]);
                        g.zoomed = false;
                    }
                    g.layout = rest;
                    held = slot.map(|slot| Closed::Pane {
                        pane: Box::new(pane),
                        slot,
                    });
                }
                (None, _) => {
                    self.w.tabs.remove(gi);
                    if gi < self.w.active || self.w.active >= self.w.tabs.len() {
                        self.w.active = self.w.active.saturating_sub(1);
                    }
                    held = Some(Closed::Tab {
                        panes: vec![pane],
                        layout: Node::Leaf(id),
                        index: gi,
                        focus: was.1,
                        zoomed: was.0,
                    });
                }
            }
        }
        if self.w.tabs.is_empty() {
            self.close_current_window(event_loop);
            return;
        }
        if let Some(h) = held.filter(|_| undoable) {
            self.hold(h);
        }
        self.focus_changed(before);
        self.clear_visible_unread();
        // 남은 칸이 빈자리를 채운다. 탭이 하나가 되면 탭 줄도 사라진다
        self.resize_all();
    }

    /// 탭을 칸째 닫는다. 되살릴 수 있게 칸들을 들고 있는다(창의 마지막 탭이면 창이 닫히므로 들지 않는다)
    fn close_tab(&mut self, gi: usize, event_loop: &ActiveEventLoop) {
        let Some(g) = self.w.tabs.get(gi) else { return };
        if self.w.tabs.len() <= 1 || self.config.undo_timeout.is_zero() {
            for id in g.layout.leaves() {
                if let Some(i) = self.pane_index(id) {
                    self.close_pane(i, event_loop, false);
                }
            }
            return;
        }
        let (layout, focus, zoomed) = (g.layout.clone(), g.focus, g.zoomed);
        let before = self.focused_id();
        self.w.tabs.remove(gi);
        if gi < self.w.active || self.w.active >= self.w.tabs.len() {
            self.w.active = self.w.active.saturating_sub(1);
        }
        let mut panes = Vec::new();
        for id in layout.leaves() {
            self.fail_text_waiters(|w| w.pane == id, "글이 나오기 전에 탭이 닫혔습니다");
            if let Some((req_id, reply, _)) = self.run_waiters.remove(&id) {
                let _ = reply.send(bt_core::proto::Response::err(
                    req_id,
                    "명령이 끝나기 전에 탭이 닫혔습니다",
                ));
            }
            if let Some(i) = self.pane_index(id) {
                panes.push(self.panes.remove(i));
            }
        }
        self.hold(Closed::Tab {
            panes,
            layout,
            index: gi,
            focus,
            zoomed,
        });
        self.focus_changed(before);
        self.clear_visible_unread();
        self.resize_all();
    }

    /// 닫은 것을 `undo-timeout` 동안 들고 있는다. 0이면 바로 떨어뜨린다
    fn hold(&mut self, closed: Closed) {
        let now = Instant::now();
        // 기한이 지난 것은 이때 떨어뜨린다
        drop(self.undo.expire(now));
        // 넣지 못한(끈) 것은 여기서 떨어진다
        let _ = self.undo.push(now, self.config.undo_timeout, closed);
    }

    /// 닫은 것을 되살린다(`undo`). 되살릴 것이 없으면 거짓
    fn undo_close(&mut self) -> bool {
        let now = Instant::now();
        drop(self.undo.expire(now));
        let Some(closed) = self.undo.pop(now) else { return false };
        let redo = match closed {
            Closed::Pane { pane, slot } => {
                let id = pane.id;
                match self.restore_pane(*pane, &slot) {
                    Ok(()) => Redo::Pane(id),
                    // 원래 자리의 이웃이 사라졌다: 새 탭 끝에 되살린다
                    Err(pane) => {
                        self.restore_tab(vec![*pane], Node::Leaf(id), usize::MAX, id, false);
                        Redo::Tab(id)
                    }
                }
            }
            Closed::Tab {
                panes,
                layout,
                index,
                focus,
                zoomed,
            } => {
                self.restore_tab(panes, layout, index, focus, zoomed);
                Redo::Tab(focus)
            }
        };
        let _ = self.redo.push(now, self.config.undo_timeout, redo);
        self.request_redraw_all();
        true
    }

    /// 분할의 칸을 원래 트리 자리에 되살린다. 이웃이 어느 창에도 없으면 칸을 돌려준다
    fn restore_pane(&mut self, pane: Pane, slot: &Slot) -> Result<(), Box<Pane>> {
        let id = pane.id;
        if !self.switch_to_pane(slot.anchor) {
            return Err(Box::new(pane));
        }
        let Some(gi) = self.group_of(slot.anchor) else {
            return Err(Box::new(pane));
        };
        let before = self.focused_id();
        let g = &mut self.w.tabs[gi];
        if !g.layout.insert_at(slot, id) {
            return Err(Box::new(pane));
        }
        g.focus = id;
        g.zoomed = false;
        self.w.active = gi;
        self.panes.push(pane);
        self.focus_changed(before);
        self.clear_visible_unread();
        self.resize_all();
        Ok(())
    }

    /// 탭을 원래 탭 자리(`index`, 넘으면 맨 끝)에 되살린다
    fn restore_tab(&mut self, panes: Vec<Pane>, layout: Node, index: usize, focus: u64, zoomed: bool) {
        let before = self.focused_id();
        let at = index.min(self.w.tabs.len());
        self.w.tabs.insert(at, TabGroup { layout, focus, zoomed });
        self.w.active = at;
        self.panes.extend(panes);
        self.focus_changed(before);
        self.clear_visible_unread();
        self.resize_all();
    }

    /// 되살린 것을 다시 닫는다(`redo`). 할 것이 없으면 거짓
    fn redo_close(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let now = Instant::now();
        drop(self.redo.expire(now));
        let Some(redo) = self.redo.pop(now) else { return false };
        match redo {
            Redo::Pane(id) => {
                if self.switch_to_pane(id) {
                    self.request(Pending::ClosePane(id), event_loop);
                }
            }
            Redo::Tab(id) => {
                if self.switch_to_pane(id) {
                    self.request(Pending::CloseTab(id), event_loop);
                }
            }
        }
        true
    }

    /// 들고 있던 칸의 셸이 스스로 끝났다: 되살릴 이유가 없으니 그 기록을 버린다
    fn held_exited(&mut self, id: u64) -> bool {
        !self.undo.remove_where(|c| c.holds(id)).is_empty()
    }

    fn active_pane(&self) -> Option<&Pane> {
        self.focused_id()
            .and_then(|id| self.pane_index(id))
            .map(|i| &self.panes[i])
    }

    fn request_redraw(&self) {
        if let Some(g) = &self.w.gfx {
            g.window.request_redraw();
        }
    }

    fn write_active(&mut self, bytes: &[u8]) {
        let scroll = self.config.scroll_to_bottom.keystroke;
        let clear = clears_selection_on_write(self.config.selection_clear_on_typing, bytes);
        if let Some(t) = self.active_pane() {
            if clear {
                t.session.select_clear();
            }
            t.session.write_input(bytes, scroll);
        }
    }

    /// 모든 탭의 칸을 지금 배치에 맞춘다(창 크기·글꼴·분할이 바뀐 뒤)
    fn resize_all(&mut self) {
        self.apply_step_resize();
        for gi in 0..self.w.tabs.len() {
            for (id, rect) in self.group_layout(gi) {
                let size = self.grid_size_for(rect);
                if let Some(i) = self.pane_index(id) {
                    // 칸·줄 수가 바뀌면 크기 표시(resize-overlay, #122)
                    let p = &mut self.panes[i];
                    let old = p.session.size();
                    if (old.cols, old.rows) != (size.cols, size.rows)
                        && resize_overlay_due(self.config.resize_overlay, p.created.elapsed())
                    {
                        p.overlay_until = Some(Instant::now() + self.config.resize_overlay_duration);
                    }
                    p.session.resize(size);
                }
            }
        }
        self.request_redraw();
    }

    fn set_font_size(&mut self, size: f32) {
        self.font_size = size.clamp(6.0, 72.0);
        self.load_fonts();
        self.for_each_window(|app| app.resize_all());
    }

    /// 창 좌표 아래의 칸(보고 있는 탭에서)
    fn pane_at(&self, pos: PhysicalPosition<f64>) -> Option<u64> {
        self.group_layout(self.w.active)
            .into_iter()
            .find(|(_, r)| r.contains(pos.x, pos.y))
            .map(|(id, _)| id)
    }

    /// 창 좌표 → 칸 idx의 격자 칸. 칸 밖이면 가장자리 칸으로 붙인다(고르며 끌다 칸을 벗어나도)
    fn cell_in(&self, idx: usize, pos: PhysicalPosition<f64>) -> Option<(usize, usize)> {
        let m = self.fonts.as_ref()?.metrics();
        let pane = self.panes.get(idx)?;
        let r = self.inner_rect(self.pane_rect(pane.id)?);
        let size = pane.session.size();
        let col = ((pos.x - r.x as f64) / m.width as f64).floor().max(0.0) as usize;
        let row = ((pos.y - r.y as f64) / m.height as f64).floor().max(0.0) as usize;
        Some((
            col.min((size.cols as usize).saturating_sub(1)),
            row.min((size.rows as usize).saturating_sub(1)),
        ))
    }

    /// 링크를 짚는 수식 키: macOS는 Cmd, 그 밖에는 Ctrl(Ghostty와 같다)
    fn link_mod(&self) -> bool {
        if cfg!(target_os = "macos") {
            self.mods.super_key()
        } else {
            self.mods.control_key()
        }
    }

    /// 창 좌표 아래의 링크(칸의 격자 안에서만)
    fn link_under(&self, pos: PhysicalPosition<f64>) -> Option<(u64, bt_term::links::LinkHit)> {
        let id = self.pane_at(pos)?;
        let i = self.pane_index(id)?;
        if !self.inner_rect(self.pane_rect(id)?).contains(pos.x, pos.y) {
            return None;
        }
        let (c, r) = self.cell_in(i, pos)?;
        bt_term::links::link_at(&self.panes[i].session.snapshot(), c, r).map(|h| (id, h))
    }

    /// Cmd를 누른 동안 마우스 아래 링크를 다시 본다. 바뀌면 밑줄과 마우스 모양을 바꾼다
    fn update_hover(&mut self) {
        let next = if self.link_mod() {
            self.link_under(self.w.cursor_pos)
        } else {
            None
        };
        if next == self.w.hover {
            return;
        }
        if let Some(g) = &self.w.gfx {
            g.window.set_cursor(if next.is_some() {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            });
        }
        self.w.hover = next;
        self.request_redraw();
    }

    /// 링크를 연다: 열어도 되는 것(http·https, 이 컴퓨터의 파일)만
    fn open_link(&mut self, url: &str) {
        match bt_term::links::classify(url, &is_local_host) {
            Some(target) => crate::desktop::open_link(&target),
            None => {
                let shown: String = bt_core::proto::printable(url).chars().take(60).collect();
                self.toast(format!("열 수 없는 링크입니다: {shown}"));
            }
        }
    }

    /// 고른 칸에 찾기 막대를 연다. 이미 열려 있으면 그대로
    fn start_search(&mut self) {
        let Some(id) = self.focused_id() else { return };
        if self.w.search.as_ref().is_some_and(|s| s.pane == id) {
            return;
        }
        self.end_search();
        self.w.search = Some(SearchBar {
            pane: id,
            query: String::new(),
            status: None,
        });
        self.request_redraw();
    }

    /// 찾기 막대를 닫고 강조를 지운다. 화면 자리는 그대로 둔다
    fn end_search(&mut self) {
        if let Some(s) = self.w.search.take() {
            if let Some(i) = self.pane_index(s.pane) {
                self.panes[i].session.search_clear();
            }
            self.request_redraw();
        }
    }

    /// 친 글이 바뀌었다: 처음부터(맨 아래에서 위로) 다시 찾는다
    fn search_query_changed(&mut self) {
        let Some(s) = self.w.search.as_mut() else { return };
        if let Some(p) = self.panes.iter().find(|p| p.id == s.pane) {
            s.status = p.session.search(&s.query);
        }
        self.request_redraw();
    }

    fn search_step(&mut self, older: bool) {
        let Some(s) = self.w.search.as_mut() else { return };
        if let Some(p) = self.panes.iter().find(|p| p.id == s.pane) {
            s.status = p.session.search_step(older).or(s.status);
        }
        self.request_redraw();
    }

    /// 찾기 막대에 글을 더한다(친 글, IME 확정, 붙여넣기의 첫 줄)
    fn search_push(&mut self, text: &str) {
        let Some(s) = self.w.search.as_mut() else { return };
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if s.query.chars().count() + text.chars().count() > 256 {
            return;
        }
        s.query.push_str(&text);
        self.search_query_changed();
    }

    /// 찾기 막대가 열려 있으면 키를 받는다(Cmd·Ctrl 조합은 키 묶음에 맡긴다). 받았으면 참
    fn search_key(&mut self, event: &winit::event::KeyEvent) -> bool {
        use winit::keyboard::{Key, NamedKey};
        if self.w.search.is_none() || self.mods.super_key() || self.mods.control_key() {
            return false;
        }
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => self.end_search(),
            // Enter는 위(오래된 쪽)로, Shift-Enter는 아래로
            Key::Named(NamedKey::Enter) => self.search_step(!self.mods.shift_key()),
            Key::Named(NamedKey::Backspace) => {
                if let Some(s) = self.w.search.as_mut() {
                    s.query.pop();
                }
                self.search_query_changed();
            }
            _ => {
                if let Some(text) = event.text.clone() {
                    self.search_push(&text);
                }
            }
        }
        true
    }

    /// 기다리던 글이 나온 btctl wait에 답한다(출력이 바뀔 때마다)
    fn check_text_waiters(&mut self) {
        if self.text_waiters.is_empty() {
            return;
        }
        let panes = &self.panes;
        self.text_waiters.retain(|w| {
            let found = panes
                .iter()
                .find(|p| p.id == w.pane)
                .is_some_and(|p| p.session.tail_text(WAIT_LINES).contains(&w.text));
            if found {
                let _ = w.reply.send(bt_core::proto::Response::ok(
                    w.req_id,
                    serde_json::json!({ "found": true }),
                ));
            }
            !found
        });
    }

    /// 조건에 맞는 btctl wait를 오류로 끝낸다(기한, 칸 닫힘)
    fn fail_text_waiters(&mut self, pick: impl Fn(&TextWaiter) -> bool, why: &str) {
        self.text_waiters.retain(|w| {
            if pick(w) {
                let _ = w.reply.send(bt_core::proto::Response::err(w.req_id, why));
                false
            } else {
                true
            }
        });
    }

    /// 설정 파일을 기본 글 편집기로 연다(없으면 예시로 만든다)
    fn open_settings(&mut self) {
        match crate::menu::ensure_config_file() {
            Ok(path) => {
                let _ = std::process::Command::new("open").arg("-t").arg(&path).spawn();
                self.toast("설정을 고친 뒤 Cmd-Shift-, 로 다시 읽습니다".into());
            }
            Err(e) => log::warn!("설정 파일을 만들지 못했습니다: {e}"),
        }
    }

    /// 명령 팔레트가 열려 있으면 키를 받는다(Cmd·Ctrl 조합은 키 묶음에 맡긴다). 받았으면 참
    fn palette_key(&mut self, event: &winit::event::KeyEvent, event_loop: &ActiveEventLoop) -> bool {
        use winit::keyboard::{Key, NamedKey};
        let Some((query, selected)) = self.w.palette.as_mut() else {
            return false;
        };
        if self.mods.super_key() || self.mods.control_key() {
            return false;
        }
        let entries = crate::palette::entries(&self.config.palette_entries, !self.config.palette_defaults_cleared);
        let shown = crate::palette::filter(&entries, query);
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => self.w.palette = None,
            Key::Named(NamedKey::ArrowDown) => *selected = (*selected + 1).min(shown.len().saturating_sub(1)),
            Key::Named(NamedKey::ArrowUp) => *selected = selected.saturating_sub(1),
            Key::Named(NamedKey::Backspace) => {
                query.pop();
                *selected = 0;
            }
            Key::Named(NamedKey::Enter) => {
                let pick = shown.get(*selected).map(|&i| entries[i].command.clone());
                self.w.palette = None;
                match pick {
                    Some(crate::palette::Command::Action(a)) => self.handle_action(a, event_loop),
                    Some(crate::palette::Command::OpenSettings) => self.open_settings(),
                    None => {}
                }
            }
            _ => {
                if let Some(text) = event.text.as_ref().filter(|t| !t.chars().any(char::is_control)) {
                    if query.chars().count() < 64 {
                        query.push_str(text);
                        *selected = 0;
                    }
                }
            }
        }
        self.request_redraw();
        true
    }

    /// 고른 칸의 자리
    fn active_index(&self) -> Option<usize> {
        self.focused_id().and_then(|id| self.pane_index(id))
    }

    /// 이 동작을 지금 할 수 있는가(`performable:` 묶음이 키를 먹을지 정한다).
    /// 고른 글이 있어야 하는 것, 클립보드 글이 있어야 하는 것, 마우스 아래 링크가 있어야 하는 것만 가린다
    fn can_perform(&mut self, action: &Action) -> bool {
        let has_selection = |s: &Self| s.active_pane().and_then(|t| t.session.selection_text()).is_some();
        match action {
            Action::Copy | Action::AdjustSelection(_) | Action::WriteFile(crate::keymap::FileSource::Selection, _) => {
                has_selection(self)
            }
            Action::Paste => self
                .clipboard
                .as_mut()
                .and_then(|cb| cb.get_text().ok())
                .is_some_and(|t| !t.is_empty()),
            Action::CopyUrl => self.link_under(self.w.cursor_pos).is_some(),
            Action::EndSearch => self.w.search.is_some(),
            Action::Undo => self.undo.has_alive(Instant::now()),
            Action::Redo => self.redo.has_alive(Instant::now()),
            _ => true,
        }
    }

    fn handle_action(&mut self, action: Action, event_loop: &ActiveEventLoop) {
        // 창이 하나도 없을 때(#178)는 새 창을 여는 동작과 앱 단위 동작만 받는다. 나머지는 고를 칸이 없다
        if self.windowless() {
            match action {
                Action::NewTab | Action::NewWindow | Action::NewSplit(_) => {
                    let opts = self.default_spawn();
                    self.open_window(event_loop, opts);
                    return;
                }
                Action::Quit | Action::ToggleQuickTerminal | Action::ReloadConfig => {}
                _ => return,
            }
        }
        match action {
            Action::Copy => {
                self.copy_selection_command();
            }
            // 찾기 막대가 열려 있으면 첫 줄을 찾을 글에 붙인다
            Action::Paste if self.w.search.is_some() => {
                let text = self.clipboard.as_mut().and_then(|cb| cb.get_text().ok());
                if let Some(line) = text.as_deref().and_then(|t| t.lines().next()) {
                    self.search_push(line);
                }
            }
            Action::StartSearch => self.start_search(),
            Action::ToggleQuickTerminal => self.toggle_quick(event_loop),
            Action::TogglePalette => {
                self.w.palette = match self.w.palette {
                    Some(_) => None,
                    None => Some((String::new(), 0)),
                };
            }
            Action::EndSearch => self.end_search(),
            Action::NavigateSearch(older) => self.search_step(older),
            Action::Paste => {
                if let Some(text) = self.clipboard.as_mut().and_then(|cb| cb.get_text().ok()) {
                    self.paste(text);
                }
            }
            Action::FontBigger(n) => self.set_font_size(self.font_size + n),
            Action::FontSmaller(n) => self.set_font_size(self.font_size - n),
            Action::FontReset => self.set_font_size(self.config.font_size),
            // 새 탭·새 칸은 고른 칸의 폴더에서 연다
            Action::NewTab => {
                self.open_tab(self.spawn_here(bt_core::config::SpawnKind::Tab));
            }
            Action::NewWindow => {
                let opts = self.spawn_here(bt_core::config::SpawnKind::Window);
                self.open_window(event_loop, opts);
            }
            Action::NewSplit(dir) => {
                self.open_split(dir, self.spawn_here(bt_core::config::SpawnKind::Split));
            }
            Action::CloseSurface => {
                if let Some(id) = self.focused_id() {
                    // 새로 닫으면 다시 하기(redo) 기록은 비운다
                    drop(self.redo.clear());
                    self.request(Pending::ClosePane(id), event_loop);
                }
            }
            Action::CloseTab => {
                if let Some(id) = self.focused_id() {
                    drop(self.redo.clear());
                    self.request(Pending::CloseTab(id), event_loop);
                }
            }
            Action::Undo => {
                if !self.undo_close() {
                    self.toast("되살릴 것이 없습니다".into());
                }
            }
            Action::Redo => {
                if !self.redo_close(event_loop) {
                    self.toast("다시 닫을 것이 없습니다".into());
                }
            }
            Action::Quit => self.request(Pending::Quit, event_loop),
            Action::WriteFile(source, then) => self.write_file(source, then),
            Action::AdjustSelection(adj) => {
                if let Some(t) = self.active_pane() {
                    t.session.adjust_selection(adj);
                }
            }
            Action::CopyUrl => {
                // 마우스 아래 링크(화면 글의 URL·OSC 8). 없으면 아무것도 안 한다
                if let Some((_, hit)) = self.link_under(self.w.cursor_pos) {
                    let url = bt_core::proto::printable(&hit.url);
                    if let Some(cb) = self.clipboard.as_mut() {
                        let _ = cb.set_text(url.clone());
                    }
                    if self.config.app_notifications.clipboard_copy {
                        self.toast(format!("링크 주소를 복사했습니다: {url}"));
                    }
                }
            }
            Action::PromptSurfaceTitle => {
                if let Some(p) = self.active_pane() {
                    let (id, now) = (p.id, p.user_title.clone().unwrap_or_default());
                    self.w.palette = None;
                    self.w.title_prompt = Some((id, now));
                }
            }
            Action::SetSurfaceTitle(text) => {
                if let Some(id) = self.focused_id() {
                    self.set_user_title(id, &text);
                }
            }
            Action::GotoSplit(target) => {
                let area = self.content_rect();
                let divider = self.divider_px();
                let next = self.w.tabs.get(self.w.active).and_then(|g| match target {
                    SplitTarget::Previous => g.layout.cycle(g.focus, -1),
                    SplitTarget::Next => g.layout.cycle(g.focus, 1),
                    SplitTarget::Toward(d) => g.layout.neighbor(area, divider, g.focus, d),
                });
                if let Some(id) = next {
                    self.focus_pane(id);
                }
            }
            Action::ResizeSplit(dir, points) => {
                let area = self.content_rect();
                let divider = self.divider_px();
                let px = (points as f64 * self.scale).round() as u32;
                if let Some(g) = self.w.tabs.get_mut(self.w.active) {
                    let focus = g.focus;
                    // 크게 보는 중에는 경계가 보이지 않으니 밀지 않는다
                    if !g.zoomed && g.layout.resize(area, divider, focus, dir, px) {
                        self.resize_all();
                    }
                }
            }
            Action::EqualizeSplits => {
                if let Some(g) = self.w.tabs.get_mut(self.w.active) {
                    g.layout.equalize();
                    self.resize_all();
                }
            }
            Action::ToggleSplitZoom => {
                // 칸이 하나면 크게 볼 것이 없다
                if let Some(g) = self
                    .w
                    .tabs
                    .get_mut(self.w.active)
                    .filter(|g| g.layout.leaves().len() > 1)
                {
                    g.zoomed = !g.zoomed;
                    self.resize_all();
                }
            }
            Action::NextTab if !self.w.tabs.is_empty() => self.activate((self.w.active + 1) % self.w.tabs.len()),
            Action::PrevTab if !self.w.tabs.is_empty() => {
                self.activate((self.w.active + self.w.tabs.len() - 1) % self.w.tabs.len())
            }
            Action::LastTab if !self.w.tabs.is_empty() => self.activate(self.w.tabs.len() - 1),
            Action::GotoTab(n) if n <= self.w.tabs.len() => self.activate(n - 1),
            // Ghostty의 clear_screen: 화면과 스크롤백을 지운다. 셸에 Ctrl-L을 보내 프롬프트를 다시 그리게 한다
            Action::ClearScreen => self.write_active(b"\x0c"),
            Action::ScrollPageUp | Action::ScrollPageDown => {
                if let Some(t) = self.active_pane() {
                    let rows = t.session.size().rows as i32;
                    t.session.scroll(if action == Action::ScrollPageUp {
                        rows - 1
                    } else {
                        -(rows - 1)
                    });
                }
            }
            Action::ScrollLines(n) => {
                if let Some(t) = self.active_pane() {
                    t.session.scroll(-n);
                }
            }
            Action::ScrollFraction(f) => {
                if let Some(t) = self.active_pane() {
                    let rows = t.session.size().rows as f32;
                    t.session.scroll(-(rows * f).round() as i32);
                }
            }
            Action::ToggleFullscreen => {
                if let Some(g) = &self.w.gfx {
                    let on = window_is_fullscreen(&g.window);
                    self.set_window_fullscreen(&g.window, !on);
                }
            }
            Action::ToggleMaximize => {
                if let Some(g) = &self.w.gfx {
                    g.window.set_maximized(!g.window.is_maximized());
                }
            }
            Action::ToggleFloatOnTop => {
                if let Some(g) = &self.w.gfx {
                    self.w.float_on_top = !self.w.float_on_top;
                    g.window
                        .set_window_level(crate::startup::window_level(self.w.float_on_top));
                }
            }
            Action::ScrollTop => {
                if let Some(t) = self.active_pane() {
                    t.session.scroll(i32::MAX / 2);
                }
            }
            Action::ScrollBottom => {
                if let Some(t) = self.active_pane() {
                    t.session.scroll_to_bottom();
                }
            }
            Action::SelectAll => {
                if let Some(t) = self.active_pane() {
                    let size = t.session.size();
                    t.session.select_start(0, 0, SelectKind::Line);
                    t.session.select_update(size.cols as usize - 1, size.rows as usize - 1);
                }
            }
            Action::ReloadConfig => self.reload_config(),
            Action::JumpToPrompt(dir) => {
                if let Some(t) = self.active_pane() {
                    if !t.session.jump_to_prompt(dir) && !t.session.has_shell_integration() {
                        self.toast("셸 통합이 없는 탭이라 프롬프트를 모릅니다".into());
                    }
                }
            }
            Action::Text(text) => {
                let scroll = self.config.scroll_to_bottom.keystroke;
                if let Some(t) = self.active_pane() {
                    t.session.write_input(text.as_bytes(), scroll);
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    /// 설정을 다시 읽어 글꼴·색·키 묶음·여백을 바꾼다. 이미 열린 탭에도 새 색을 쓴다
    /// 배경 그림을 푼다(#134). 읽지 못하면 경고를 남기고 그림 없이 그린다. 창마다의 층은 다시 만든다
    fn load_background(&mut self) {
        self.bg_image = self
            .config
            .background_image
            .as_ref()
            .and_then(|p| match bt_render::background::load(p) {
                Ok(img) => {
                    log::info!("배경 그림: {} {}×{}", p.display(), img.width, img.height);
                    Some(Arc::new(img))
                }
                Err(e) => {
                    log::warn!("배경 그림을 읽지 못했습니다: {e}");
                    self.config.warnings.push(format!("배경 그림을 읽지 못했습니다: {e}"));
                    None
                }
            });
        for w in std::iter::once(&mut self.w).chain(&mut self.others) {
            w.backdrop = None;
            w.last_frame = None;
        }
    }

    fn reload_config(&mut self) {
        let next = AppConfig::load(self.config_extra.as_deref(), crate::system_is_dark());
        for w in &next.warnings {
            log::warn!("설정: {w}");
        }
        let mut next = next;
        let (keymap, mut kw) = Keymap::build(&next.keybinds);
        kw.extend(crate::palette::user_warnings(&next.palette_entries));
        for w in &kw {
            log::warn!("설정: {w}");
        }
        // 키 묶음 경고도 설정 경고로 함께 보인다(btctl reload-config)
        next.warnings.extend(kw);
        self.keymap = keymap;
        // 기다리던 키 순서는 옛 키 묶음의 것이라 버린다
        for w in std::iter::once(&mut self.w).chain(&mut self.others) {
            w.keyseq = Default::default();
        }
        self.font_size = next.font_size;
        for t in &self.panes {
            t.session.set_theme(next.theme.clone());
            t.session.set_scroll_on_output(next.scroll_to_bottom.output);
        }
        #[cfg(target_os = "macos")]
        for w in std::iter::once(&self.w).chain(&self.others) {
            use winit::platform::macos::{OptionAsAlt, WindowExtMacOS};
            if let Some(g) = &w.gfx {
                g.window.set_option_as_alt(match next.option_as_alt {
                    bt_core::config::OptionAsAlt::Both => OptionAsAlt::Both,
                    bt_core::config::OptionAsAlt::Left => OptionAsAlt::OnlyLeft,
                    bt_core::config::OptionAsAlt::Right => OptionAsAlt::OnlyRight,
                    bt_core::config::OptionAsAlt::None => OptionAsAlt::None,
                });
            }
        }
        self.config = next;
        apply_hidden_from_dock(self.config.macos_hidden);
        self.load_background();
        self.load_fonts();
        self.for_each_window(|app| app.resize_all());
        self.register_hotkeys();
        log::info!("설정을 다시 읽었습니다");
        let warn = self.config.warnings.len();
        // 경고가 있으면 알림을 껐어도 알린다(고칠 것이 있다는 뜻이라)
        if self.config.app_notifications.config_reload || warn > 0 {
            self.toast(if warn == 0 {
                "설정을 다시 읽었습니다".into()
            } else {
                format!("설정을 다시 읽었습니다(경고 {warn}개)")
            });
        }
    }

    /// 칸 idx의 프로그램이 마우스 보고를 켰으면 그 모드
    fn mouse_reporting(&self, idx: usize) -> Option<bt_term::Modes> {
        let m = self.panes.get(idx)?.session.modes();
        // mouse-reporting을 끄면 보내지 않는다. Shift를 누르면 mouse-shift-capture가 true·always일 때만
        // 프로그램이 받고, 아니면 고르기를 한다(xterm 관례)
        (m.mouse_report || m.mouse_drag || m.mouse_motion)
            .then_some(m)
            .filter(|_| {
                mouse::reports_to_program(
                    self.config.mouse_reporting,
                    self.config.mouse_shift_capture,
                    self.mods.shift_key(),
                )
            })
    }

    fn send_mouse(&mut self, idx: usize, button: mouse::Button, action: mouse::Action) {
        let Some(m) = self.mouse_reporting(idx) else { return };
        let Some((col, row)) = self.cell_in(idx, self.w.cursor_pos) else {
            return;
        };
        let bytes = mouse::encode(
            button,
            action,
            col,
            row,
            self.mods.shift_key(),
            self.mods.alt_key(),
            self.mods.control_key(),
            m.sgr_mouse,
        );
        self.panes[idx].session.write(&bytes);
    }

    /// 창과 그림판을 만든다(첫 창, 새 창)
    fn create_gfx(&mut self, event_loop: &ActiveEventLoop) -> Option<Gfx> {
        self.create_gfx_with(event_loop, false)
    }

    /// 창을 만든다. `quick`이면 퀵 터미널 모양(테두리 없음, 항상 위, 주 화면 맨 위에 폭 전체·높이 40%)으로 처음부터 만든다.
    /// 만든 뒤에 테두리를 바꾸면 macOS가 포커스를 잠깐 거둬 곧바로 숨는다(#95)
    fn create_gfx_with(&mut self, event_loop: &ActiveEventLoop, quick: bool) -> Option<Gfx> {
        // 화면 색 공간으로 넘길지(ADR-009). 새 그림판부터 따른다
        softbuffer::set_native_color_space(!self.config.colorspace_srgb);
        // 투명 창(#78): 창을 투명으로 만들고 그림판이 알파를 넘기게 한다
        let transparent = self.config.background_opacity < 1.0;
        softbuffer::set_transparent(transparent);
        // 실험 E4: BT_E4_NO_REUSE가 있으면 버퍼 다시 쓰기를 끈다(견주어 재려고)
        softbuffer::set_reuse_buffers(std::env::var_os("BT_E4_NO_REUSE").is_none());
        let attrs = Window::default_attributes()
            .with_title("b-terminal")
            .with_inner_size(LogicalSize::new(1000.0, 680.0))
            .with_min_inner_size(LogicalSize::new(320.0, 200.0))
            .with_transparent(transparent);
        // 퀵 창은 테두리 없이 맨 위 층에, 보이지 않게 만든다. 자리를 잡고 보이는 일은 show_quick이 한다(#110)
        let attrs = if quick {
            attrs
                .with_decorations(false)
                .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
                .with_visible(false)
        } else {
            attrs
        };
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::{OptionAsAlt, WindowAttributesExtMacOS};
            attrs.with_option_as_alt(match self.config.option_as_alt {
                bt_core::config::OptionAsAlt::Both => OptionAsAlt::Both,
                bt_core::config::OptionAsAlt::Left => OptionAsAlt::OnlyLeft,
                bt_core::config::OptionAsAlt::Right => OptionAsAlt::OnlyRight,
                bt_core::config::OptionAsAlt::None => OptionAsAlt::None,
            })
        };
        // macos-window-buttons = hidden: 새 창부터 신호등 단추를 숨긴다(퀵 창은 테두리가 없어 해당 없음, #179)
        #[cfg(target_os = "macos")]
        let attrs = if !quick && self.config.macos_window_buttons == bt_core::config::MacWindowButtons::Hidden {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs.with_titlebar_buttons_hidden(true)
        } else {
            attrs
        };
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                log::error!("창을 만들지 못했습니다: {e}");
                return None;
            }
        };
        window.set_ime_allowed(true);
        self.scale = window.scale_factor();
        let context = softbuffer::Context::new(window.clone()).ok()?;
        let surface = softbuffer::Surface::new(&context, window.clone()).ok()?;
        Some(Gfx {
            window,
            surface,
            _context: context,
        })
    }

    /// 창의 전체 화면을 켜고 끈다. `macos-non-native-fullscreen`이 켜진 macOS는 새 Space 없는 전체 화면(winit simple fullscreen)을 쓴다.
    /// 끌 때는 지금 어느 방식으로 들어가 있는지를 따른다(설정이 중간에 바뀌어도 빠져나올 수 있게)
    fn set_window_fullscreen(&self, window: &Window, on: bool) {
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowExtMacOS;
            let simple_now = window.simple_fullscreen();
            let kind = crate::startup::fullscreen_kind(self.config.non_native_fullscreen, true);
            if (on && kind == crate::startup::FullscreenKind::Simple) || (!on && simple_now) {
                window.set_simple_fullscreen(on);
                return;
            }
        }
        window.set_fullscreen(on.then_some(winit::window::Fullscreen::Borderless(None)));
    }

    /// window-step-resize: 칸 크기 단위로 창 크기가 바뀌게 한다(끄면 픽셀 단위로 되돌린다)
    fn apply_step_resize(&self) {
        let (Some(f), Some(g)) = (&self.fonts, &self.w.gfx) else {
            return;
        };
        let m = f.metrics();
        let inc = crate::startup::step_increments(self.config.window_step_resize, m.width, m.height);
        g.window
            .set_resize_increments(inc.map(|(w, h)| PhysicalSize::new(w, h)));
    }

    /// 설정의 window-width·window-height(셀)로 지금 창 크기를 맞춘다
    fn apply_initial_size(&self) {
        if let (Some(f), Some(g)) = (&self.fonts, &self.w.gfx) {
            if let (Some(cols), Some(rows)) = (self.config.window_width, self.config.window_height) {
                let m = f.metrics();
                let px = |pt: f32| (pt as f64 * self.scale).round() as u32;
                let w = m.width * cols as u32 + px(self.config.padding_x.0) + px(self.config.padding_x.1);
                let h = m.height * rows as u32 + px(self.config.padding_y.0) + px(self.config.padding_y.1);
                let _ = g.window.request_inner_size(PhysicalSize::new(w, h));
            }
        }
    }

    /// 설정의 maximize·fullscreen을 지금 창에 적용한다(#148). 퀵 창과 되살린 창에는 부르지 않는다.
    /// `first`면 window-position-x·y도 적용한다(첫 창만)
    fn apply_start_window(&self, event_loop: &ActiveEventLoop, first: bool) {
        let Some(g) = &self.w.gfx else { return };
        match crate::startup::start_mode(self.config.maximize, self.config.fullscreen, false) {
            crate::startup::StartMode::Normal => {}
            crate::startup::StartMode::Maximized => g.window.set_maximized(true),
            crate::startup::StartMode::Fullscreen => self.set_window_fullscreen(&g.window, true),
        }
        if first {
            if let Some(pos) =
                crate::startup::configured_position(self.config.window_position_x, self.config.window_position_y)
            {
                let primary = event_loop.primary_monitor();
                // macOS는 Ghostty처럼 보이는 화면 영역(메뉴 막대 아래)의 왼쪽 위가 기준이다(점 단위)
                #[cfg(target_os = "macos")]
                if let Some(m) = &primary {
                    let sf = m.scale_factor();
                    let screen = crate::quick::Rect {
                        x: m.position().x as f64 / sf,
                        y: m.position().y as f64 / sf,
                        w: m.size().width as f64 / sf,
                        h: m.size().height as f64 / sf,
                    };
                    let area = crate::quick::visible_area(screen, screen.h).unwrap_or(screen);
                    g.window.set_outer_position(winit::dpi::LogicalPosition::new(
                        area.x + pos.0 as f64,
                        area.y + pos.1 as f64,
                    ));
                    return;
                }
                // 그 밖에는 주 화면 왼쪽 위에서 잰 점 좌표를 물리 픽셀로 바꿔 놓는다
                let (origin, scale) = primary
                    .map(|m| ((m.position().x, m.position().y), m.scale_factor()))
                    .unwrap_or(((0, 0), self.scale));
                let (x, y) = crate::startup::physical_position(pos, origin, scale);
                g.window.set_outer_position(PhysicalPosition::new(x, y));
            }
        }
    }

    /// 설정의 전역 단축키(`keybind = global:…`)를 등록한다. 설정을 다시 읽으면 다시 한다(#95)
    fn register_hotkeys(&mut self) {
        use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
        if self.keymap.global.is_empty() && self.hotkeys.is_none() {
            return;
        }
        if self.hotkeys.is_none() {
            match GlobalHotKeyManager::new() {
                Ok(m) => self.hotkeys = Some(m),
                Err(e) => {
                    log::warn!("전역 단축키를 쓸 수 없습니다: {e}");
                    return;
                }
            }
            let proxy = self.proxy.clone();
            GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
                if e.state == HotKeyState::Pressed {
                    let _ = proxy.send_event(UserEvent::Hotkey(e.id));
                }
            }));
        }
        let Some(manager) = &self.hotkeys else { return };
        let _ = manager.unregister_all(&self.hotkey_registered);
        self.hotkey_registered.clear();
        self.hotkey_actions.clear();
        for (trig, action) in &self.keymap.global {
            let Some(text) = Keymap::hotkey_string(trig) else {
                log::warn!("전역 단축키로 쓸 수 없는 키입니다: {}", trig.key);
                continue;
            };
            let Ok(hk) = text.parse::<global_hotkey::hotkey::HotKey>() else {
                log::warn!("전역 단축키를 읽지 못했습니다: {text}");
                continue;
            };
            match manager.register(hk) {
                Ok(()) => {
                    log::info!("전역 단축키: {text}");
                    self.hotkey_actions.insert(hk.id(), action.clone());
                    self.hotkey_registered.push(hk);
                }
                Err(e) => log::warn!("전역 단축키 {text}를 등록하지 못했습니다(다른 앱이 쓰는 중일 수 있습니다): {e}"),
            }
        }
    }

    /// 퀵 터미널(#95): 없으면 화면 위쪽에 만들고, 보이고 있으면 숨기고, 숨어 있으면 보인다
    fn toggle_quick(&mut self, event_loop: &ActiveEventLoop) {
        let quick = std::iter::once(&self.w)
            .chain(&self.others)
            .find(|w| w.quick)
            .and_then(|w| w.id());
        let Some(id) = quick else {
            self.open_quick(event_loop);
            return;
        };
        self.switch_to(id);
        let Some(g) = &self.w.gfx else { return };
        let hiding = self.w.quick_anim.as_ref().is_some_and(|a| a.hide);
        log::debug!(
            "퀵 터미널 토글: 보임 {:?}, 숨는 중 {hiding}, 포커스 {}",
            g.window.is_visible(),
            self.w.focused
        );
        if g.window.is_visible() == Some(true) && !hiding {
            self.hide_quick();
        } else {
            self.show_quick(event_loop);
            self.last_focused = Some(id);
        }
    }

    /// 퀵 터미널 창을 만든다(보이지 않게). 자리를 잡아 보이는 일은 show_quick
    fn open_quick(&mut self, event_loop: &ActiveEventLoop) {
        let opts = self.default_spawn();
        if self.open_window_with(event_loop, opts, true).is_none() {
            return;
        }
        self.w.quick = true;
        self.show_quick(event_loop);
    }

    /// 퀵 창을 띄울 화면(논리 점): `quick-terminal-screen`. 찾지 못하면 메뉴 막대 화면
    fn quick_screen(&self, event_loop: &ActiveEventLoop) -> Option<crate::quick::Rect> {
        let logical = |m: &winit::monitor::MonitorHandle| {
            let sf = m.scale_factor();
            let (p, s) = (m.position(), m.size());
            crate::quick::Rect {
                x: p.x as f64 / sf,
                y: p.y as f64 / sf,
                w: s.width as f64 / sf,
                h: s.height as f64 / sf,
            }
        };
        let primary = event_loop
            .primary_monitor()
            .or_else(|| event_loop.available_monitors().next())?;
        let primary = logical(&primary);
        #[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
        let screens: Vec<crate::quick::Rect> = event_loop.available_monitors().map(|m| logical(&m)).collect();
        let picked = match self.config.quick.screen {
            bt_core::config::QuickScreen::MenuBar => None,
            #[cfg(target_os = "macos")]
            bt_core::config::QuickScreen::Main => crate::quick::main_screen(primary.h).and_then(|r| {
                // winit 화면 중 같은 자리의 것(점 단위가 조금 어긋나도)
                screens
                    .iter()
                    .copied()
                    .find(|s| (s.x - r.x).abs() < 2.0 && (s.y - r.y).abs() < 2.0)
            }),
            #[cfg(target_os = "macos")]
            bt_core::config::QuickScreen::Mouse => {
                crate::quick::screen_at(crate::quick::mouse_location(primary.h), &screens).map(|i| screens[i])
            }
            #[cfg(not(target_os = "macos"))]
            _ => None,
        };
        let screen = picked.unwrap_or(primary);
        // 메뉴 막대와 Dock을 뺀 영역에 놓는다(위쪽 창이 메뉴 막대에 가리지 않게)
        #[cfg(target_os = "macos")]
        let screen = crate::quick::visible_area(screen, primary.h).unwrap_or(screen);
        Some(screen)
    }

    /// 퀵 창을 고른 화면의 자리로 옮겨 보인다. 움직임이 있으면 가장자리 바깥에서 들어온다
    fn show_quick(&mut self, event_loop: &ActiveEventLoop) {
        let Some(screen) = self.quick_screen(event_loop) else {
            return;
        };
        let q = self.config.quick;
        let frame = crate::quick::frame(&q, screen);
        let hidden = crate::quick::hidden_origin(q.position, frame, screen);
        let animate = q.animation > 0.0 && q.position != bt_core::config::QuickPosition::Center;
        let Some(g) = &self.w.gfx else { return };
        let _ = g.window.request_inner_size(LogicalSize::new(frame.w, frame.h));
        let start = if animate { hidden } else { (frame.x, frame.y) };
        g.window
            .set_outer_position(winit::dpi::LogicalPosition::new(start.0, start.1));
        g.window.set_visible(true);
        g.window.focus_window();
        log::debug!("퀵 터미널: 화면 {screen:?}, 자리 {frame:?}, 움직임 {animate}");
        self.w.quick_shown = Some(Instant::now());
        self.w.quick_place = Some((frame, hidden));
        self.w.quick_anim = animate.then(|| QuickAnim {
            from: start,
            to: (frame.x, frame.y),
            start: Instant::now(),
            dur: Duration::from_secs_f64(q.animation),
            hide: false,
        });
    }

    /// 퀵 창을 숨긴다. 움직임이 있으면 가장자리 바깥으로 나간 뒤 숨는다
    fn hide_quick(&mut self) {
        let q = self.config.quick;
        let animate = q.animation > 0.0 && q.position != bt_core::config::QuickPosition::Center;
        match (animate, self.w.quick_place) {
            (true, Some((frame, hidden))) => {
                // 들어오는 중이었으면 지금 자리에서 나간다
                let from = self
                    .w
                    .quick_anim
                    .as_ref()
                    .map(|a| crate::quick::slide(a.from, a.to, a.start.elapsed().as_secs_f64() / a.dur.as_secs_f64()))
                    .unwrap_or((frame.x, frame.y));
                self.w.quick_anim = Some(QuickAnim {
                    from,
                    to: hidden,
                    start: Instant::now(),
                    dur: Duration::from_secs_f64(q.animation),
                    hide: true,
                });
            }
            _ => {
                self.w.quick_anim = None;
                if let Some(g) = &self.w.gfx {
                    g.window.set_visible(false);
                }
            }
        }
    }

    /// 모든 창의 상태(창 크기·자리, 탭, 분할, 칸의 폴더)를 적는다(#94)
    fn save_state(&self) {
        let Some(path) = crate::state::path() else { return };
        let cwd_of = |id: u64| {
            self.panes
                .iter()
                .find(|p| p.id == id)
                .and_then(|p| p.cwd.clone().or_else(|| p.session.cwd()))
        };
        let windows: Vec<crate::state::SavedWindow> = std::iter::once(&self.w)
            .chain(&self.others)
            .filter(|w| !w.tabs.is_empty() && !w.quick)
            .filter_map(|w| {
                let g = w.gfx.as_ref()?;
                let size = g.window.inner_size();
                Some(crate::state::SavedWindow {
                    size: (size.width, size.height),
                    position: g.window.outer_position().ok().map(|p| (p.x, p.y)),
                    tabs: w
                        .tabs
                        .iter()
                        .map(|t| crate::state::SavedTab {
                            layout: crate::state::SavedNode::from_node(&t.layout, &cwd_of),
                            focus: t.layout.leaves().iter().position(|&id| id == t.focus).unwrap_or(0),
                            zoomed: t.zoomed,
                        })
                        .collect(),
                    active: w.active,
                })
            })
            .collect();
        let state = crate::state::SavedState {
            version: crate::state::VERSION,
            windows,
        };
        match crate::state::save(&path, &state) {
            Ok(()) => log::info!("창 상태를 적었습니다: 창 {}개", state.windows.len()),
            Err(e) => log::warn!("창 상태를 적지 못했습니다: {e}"),
        }
    }

    /// 지난번 창 상태를 되살린다. 되살린 탭이 하나라도 있으면 참(#94)
    fn restore_state(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let Some(state) = crate::state::path().and_then(|p| crate::state::load(&p)) else {
            return false;
        };
        let mut restored = false;
        for (wi, sw) in state.windows.iter().enumerate() {
            // 첫 창은 이미 만든 창을 쓰고, 그다음부터 새 창
            if wi > 0 {
                let Some(gfx) = self.create_gfx(event_loop) else { break };
                let mut win = Win::new();
                win.focused = gfx.window.has_focus();
                win.gfx = Some(gfx);
                let old = std::mem::replace(&mut self.w, win);
                self.others.push(old);
            }
            if let Some(g) = &self.w.gfx {
                let _ = g
                    .window
                    .request_inner_size(PhysicalSize::new(sw.size.0.max(320), sw.size.1.max(200)));
                if let Some((x, y)) = sw.position {
                    g.window.set_outer_position(PhysicalPosition::new(x, y));
                }
            }
            for st in &sw.tabs {
                let size = self.grid_size_for(self.content_rect());
                let mut ids = Vec::new();
                for cwd in st.layout.panes() {
                    let mut opts = self.default_spawn();
                    if let Some(dir) = cwd.filter(|d| d.is_dir()) {
                        opts.cwd = Some(dir);
                    }
                    if let Some(p) = self.spawn_pane(opts, size) {
                        ids.push(p.id);
                        self.panes.push(p);
                    }
                }
                let Some(layout) = st.layout.to_node(&mut ids.clone().into_iter()) else {
                    // 칸을 다 띄우지 못했으면 그 탭은 버린다
                    for id in ids {
                        if let Some(i) = self.pane_index(id) {
                            self.panes.remove(i);
                        }
                    }
                    continue;
                };
                let leaves = layout.leaves();
                let focus = leaves.get(st.focus).copied().unwrap_or(leaves[0]);
                self.w.tabs.push(TabGroup {
                    layout,
                    focus,
                    zoomed: st.zoomed && leaves.len() > 1,
                });
                restored = true;
            }
            if self.w.tabs.is_empty() {
                // 칸을 하나도 띄우지 못한 창: 첫 창이 아니면 닫는다(첫 창은 기본 탭을 연다)
                if wi > 0 {
                    if let Some(prev) = self.others.pop() {
                        self.w = prev;
                    }
                }
                continue;
            }
            self.w.active = sw.active.min(self.w.tabs.len() - 1);
            self.last_focused = self.w.id();
            self.resize_all();
        }
        if restored {
            log::info!("창 상태를 되살렸습니다");
        }
        restored
    }

    /// 새 창을 열어 지금 창으로 하고 탭 하나를 띄운다(#36)
    fn open_window(&mut self, event_loop: &ActiveEventLoop, options: SpawnOptions) -> Option<u64> {
        self.open_window_with(event_loop, options, false)
    }

    fn open_window_with(&mut self, event_loop: &ActiveEventLoop, options: SpawnOptions, quick: bool) -> Option<u64> {
        let gfx = self.create_gfx_with(event_loop, quick)?;
        let id = gfx.window.id();
        let mut win = Win::new();
        win.focused = gfx.window.has_focus();
        win.gfx = Some(gfx);
        let mut old = std::mem::replace(&mut self.w, win);
        // 새 창이 앞으로 온다. 앞 창의 포커스 잃음 사건을 기다리지 않고 바로 보고 있지 않은 창으로.
        // 창이 하나도 없던 자리(창 없는 상태, #178)는 남기지 않는다
        if old.gfx.is_some() {
            old.focused = false;
            self.others.push(old);
        }
        self.quit_deadline = None;
        self.last_focused = Some(id);
        if !quick {
            self.apply_initial_size();
            self.apply_start_window(event_loop, false);
        }
        let tab = self.open_tab(options);
        if tab.is_none() {
            self.close_current_window(event_loop);
        }
        tab
    }

    /// 지금 창을 닫는다: 그 창의 칸들을 닫고, 다른 창이 있으면 그 창이 지금 창이 된다. 마지막 창이면 앱이 끝난다
    fn close_current_window(&mut self, event_loop: &ActiveEventLoop) {
        let ids: Vec<u64> = self.w.tabs.iter().flat_map(|g| g.layout.leaves()).collect();
        for id in ids {
            if let Some(i) = self.pane_index(id) {
                // Session을 떨어뜨리면 PTY가 닫히고 셸은 SIGHUP을 받는다
                self.panes.remove(i);
            }
            self.fail_text_waiters(|w| w.pane == id, "글이 나오기 전에 창이 닫혔습니다");
            if let Some((req_id, reply, _)) = self.run_waiters.remove(&id) {
                let _ = reply.send(bt_core::proto::Response::err(
                    req_id,
                    "명령이 끝나기 전에 창이 닫혔습니다",
                ));
            }
        }
        self.w.tabs.clear();
        match self.others.pop() {
            // 앞 Win을 떨어뜨리면 창이 닫힌다
            Some(next) => {
                self.w = next;
                self.request_redraw();
            }
            // 마지막 창: 앱을 끝낼지는 quit-after-last-window-closed가 정한다(#178)
            None => {
                self.w = Win::new();
                self.last_focused = None;
                self.on_last_window_closed(event_loop, false);
            }
        }
    }

    /// 창이 하나도 없는 상태인가(첫 창 없이 띄웠거나 마지막 창을 닫았는데 앱이 남아 있다, #178)
    fn windowless(&self) -> bool {
        self.w.gfx.is_none() && self.others.is_empty()
    }

    /// 마지막 창이 닫혔다(또는 첫 창 없이 띄웠다): 설정에 따라 바로 끝내거나 기한을 건다.
    /// 첫 창 없이 띄운 때(`at_start`)는 기한이 있을 때만 끝낸다(Ghostty `initial-window` 문서)
    fn on_last_window_closed(&mut self, event_loop: &ActiveEventLoop, at_start: bool) {
        use crate::lifecycle::LastWindow;
        match crate::lifecycle::last_window_closed(
            self.config.quit_after_last_window_closed,
            self.config.quit_after_last_window_closed_delay,
        ) {
            LastWindow::QuitNow if !at_start => event_loop.exit(),
            LastWindow::QuitAfter(d) => self.quit_deadline = Some(Instant::now() + d),
            LastWindow::QuitNow | LastWindow::Stay => {}
        }
    }

    /// 이 창을 지금 창으로 바꿔 끼운다. 모르는 창이면 거짓
    fn switch_to(&mut self, id: WindowId) -> bool {
        if self.w.id() == Some(id) {
            return true;
        }
        match self.others.iter().position(|w| w.id() == Some(id)) {
            Some(i) => {
                std::mem::swap(&mut self.w, &mut self.others[i]);
                true
            }
            None => false,
        }
    }

    /// 칸이 든 창을 지금 창으로
    fn switch_to_pane(&mut self, pane: u64) -> bool {
        if self.w.holds(pane) {
            return true;
        }
        match self.others.iter().position(|w| w.holds(pane)) {
            Some(i) => {
                std::mem::swap(&mut self.w, &mut self.others[i]);
                true
            }
            None => false,
        }
    }

    /// 모든 창을 다시 그리게 한다
    fn request_redraw_all(&self) {
        for w in std::iter::once(&self.w).chain(&self.others) {
            if let Some(g) = &w.gfx {
                g.window.request_redraw();
            }
        }
    }

    /// 창마다 차례로 지금 창으로 바꿔 끼워 `f`를 부른다(글꼴·설정이 바뀌어 모든 창을 맞출 때)
    fn for_each_window(&mut self, f: impl Fn(&mut Self)) {
        f(self);
        for i in 0..self.others.len() {
            std::mem::swap(&mut self.w, &mut self.others[i]);
            f(self);
            std::mem::swap(&mut self.w, &mut self.others[i]);
        }
    }

    fn redraw(&mut self) {
        self.redraw_pending.store(false, Ordering::Release);
        let theme = self.config.theme.clone();
        let toast = self.w.toast.clone().filter(|(_, at)| at.elapsed() < TOAST_FOR);
        let confirm = self.w.confirm.as_ref().map(|c| (c.message.clone(), c.yes));
        let title_prompt = self.w.title_prompt.as_ref().map(|(_, t)| t.clone());
        let now_overlay = Instant::now();
        let resize_overlays: Vec<u64> = self
            .panes
            .iter()
            .filter(|p| p.overlay_until.is_some_and(|t| t > now_overlay))
            .map(|p| p.id)
            .collect();
        let focus = self.focused_id();
        // 보이는 칸마다: 바깥 사각형, 격자 자리, 화면, 고른 칸인가, 진행 표시
        let layout = self.group_layout(self.w.active);
        let views: Vec<PaneView> = layout
            .iter()
            .filter_map(|(id, outer)| {
                let p = &self.panes[self.pane_index(*id)?];
                Some(PaneView {
                    id: *id,
                    outer: *outer,
                    inner: self.inner_rect(*outer),
                    screen: p.session.snapshot(),
                    focused: Some(*id) == focus,
                    progress: p.live_progress(),
                })
            })
            .collect();
        let split = views.len() > 1;
        let dividers = match self.w.tabs.get(self.w.active) {
            Some(g) if split => g.layout.dividers(self.content_rect(), self.divider_px()),
            _ => Vec::new(),
        };
        let divider_color = self
            .config
            .split_divider_color
            .unwrap_or_else(|| theme.background.mix(theme.foreground, 0.2));
        // 고르지 않은 칸은 바탕색으로 살짝 덮는다(Ghostty의 unfocused-split-opacity)
        let dim_alpha = ((1.0 - self.config.unfocused_split_opacity) * 255.0).round() as u8;
        let dim_fill = self.config.unfocused_split_fill.unwrap_or(theme.background);
        // 탭 막대가 없을 때(`window-show-tab-bar`): 진행 막대를 칸 위에 그린다
        let single_tab = !self.config.window_show_tab_bar.visible(self.w.tabs.len());
        let labels: Vec<tabbar::TabLabel> = if single_tab {
            Vec::new()
        } else {
            self.w
                .tabs
                .iter()
                .enumerate()
                .map(|(gi, g)| {
                    let panes: Vec<&Pane> = g
                        .layout
                        .leaves()
                        .into_iter()
                        .filter_map(|id| self.pane_index(id).map(|i| &self.panes[i]))
                        .collect();
                    let focused = panes.iter().find(|p| p.id == g.focus);
                    tabbar::TabLabel {
                        title: focused.map(|p| p.display_title()).unwrap_or_default(),
                        active: gi == self.w.active,
                        unread: panes.iter().any(|p| p.unread),
                        // 고른 칸의 진행 표시, 없으면 다른 칸의 것
                        progress: focused
                            .and_then(|p| p.live_progress())
                            .or_else(|| panes.iter().find_map(|p| p.live_progress())),
                    }
                })
                .collect()
        };
        let title = self
            .active_pane()
            .map(|t| t.display_title())
            .unwrap_or_else(|| "b-terminal".into());
        let title = self.config.title.clone().unwrap_or(title);
        let preedit = self.w.preedit.clone();
        let hover = self.w.hover.clone();
        // 명령 팔레트: (친 글, 보일 줄들(이름, 단축키), 고른 줄)
        let palette = self.w.palette.as_ref().map(|(q, sel)| {
            let entries = crate::palette::entries(&self.config.palette_entries, !self.config.palette_defaults_cleared);
            let rows: Vec<(String, String)> = crate::palette::filter(&entries, q)
                .into_iter()
                .map(|i| {
                    let e = &entries[i];
                    (e.label.to_string(), crate::palette::shortcut(&self.keymap, &e.command))
                })
                .collect();
            (q.clone(), rows, *sel)
        });
        let search = self.w.search.as_ref().map(|s| {
            (
                s.pane,
                s.query.clone(),
                search_status_text(&s.query, s.status),
                s.status.is_some_and(|st| st.total == 0),
            )
        });
        // 칸마다 렌더러 셀과 커서를 미리 만든다(앞 프레임과 견주려고)
        let blink_hidden = self.blink_wanted_for(&views) && !self.w.blink_on;
        self.w.blink_wanted = self.blink_wanted_for(&views);
        let drawn: Vec<(Vec<DrawCell>, Option<DrawCursor>, Option<String>)> = views
            .iter()
            .map(|v| {
                let screen = &v.screen;
                let mut cells = to_draw_cells(screen, &theme);
                // Cmd를 누른 채 올라간 링크에 밑줄
                if let Some((_, hit)) = hover.as_ref().filter(|(h, _)| *h == v.id) {
                    for sp in &hit.spans {
                        for c in sp.start..sp.end.min(screen.cols) {
                            if let Some(cell) = cells.get_mut(sp.row * screen.cols + c) {
                                cell.underline = true;
                            }
                        }
                    }
                }
                let hide = v.focused && blink_hidden;
                let under = cells
                    .get(screen.cursor.row * screen.cols + screen.cursor.col)
                    .map(|c| (c.fg, c.bg))
                    .unwrap_or((theme.foreground, theme.background));
                let cursor_color = cursor_colors(
                    self.config.cursor_color_cell,
                    self.config.cursor_text_cell,
                    (theme.cursor, theme.cursor_text),
                    under,
                );
                let cursor = (screen.cursor.visible && !hide).then_some(DrawCursor {
                    col: screen.cursor.col,
                    row: screen.cursor.row,
                    // 고르지 않은 칸의 커서는 속이 빈 네모(Ghostty와 같다)
                    style: match (v.focused, screen.cursor.shape) {
                        (false, _) => CursorStyle::Hollow,
                        (true, CursorShape::Block) => CursorStyle::Block,
                        (true, CursorShape::Beam) => CursorStyle::Beam,
                        (true, CursorShape::Underline) => CursorStyle::Underline,
                        (true, CursorShape::HollowBlock) => CursorStyle::Hollow,
                    },
                    wide: screen.cursor.wide,
                    color: cursor_color.0,
                    text_color: cursor_color.1,
                    opacity: self.config.cursor_opacity,
                });
                // IME 조합 글은 고른 칸에만 그린다(찾기 막대가 열려 있으면 막대에)
                let pre = (v.focused && search.is_none() && palette.is_none() && !preedit.is_empty())
                    .then(|| preedit.clone());
                (cells, cursor, pre)
            })
            .collect();
        // 여백 색(window-padding-color, #136): background가 아니면 칸마다 가장자리 칸의 색
        let pads: Vec<Option<crate::padding::PadPaint>> = views
            .iter()
            .zip(&drawn)
            .map(|(v, (cells, _, _))| {
                if self.config.padding_color == bt_core::config::PaddingColor::Background {
                    return None;
                }
                let p = self.pane_index(v.id).map(|i| &self.panes[i])?;
                Some(crate::padding::pad_paint(
                    self.config.padding_color,
                    cells,
                    v.screen.cols,
                    v.screen.rows,
                    v.screen.background,
                    p.session.modes().alt_screen,
                    &p.session.prompt_rows(),
                ))
            })
            .collect();
        let Some(gfx) = self.w.gfx.as_mut() else { return };
        let Some(fonts) = self.fonts.as_mut() else { return };
        let size = gfx.window.inner_size();
        let m = fonts.metrics();
        // 격자 밖에 그리는 모든 것. 하나라도 바뀌면 통째로 다시 그린다.
        // 격자 위에 겹쳐 그리는 것(알림 띠, 주소 띠, 찾기 막대, 진행 막대)이 있으면 늘 통째로
        let chrome = Chrome {
            size: (size.width, size.height),
            cell: (m.width, m.height),
            panes: views
                .iter()
                .map(|v| {
                    (
                        v.id,
                        v.outer,
                        v.inner,
                        v.focused,
                        v.screen.background,
                        v.screen.cols,
                        v.screen.rows,
                    )
                })
                .collect(),
            labels: labels
                .iter()
                .map(|l| (l.title.clone(), l.active, l.unread, l.progress))
                .collect(),
            overlay: toast.is_some()
                || hover.is_some()
                || search.is_some()
                || palette.is_some()
                || confirm.is_some()
                || title_prompt.is_some()
                || views.iter().any(|v| resize_overlays.contains(&v.id))
                // kitty 그림은 칸 위에 겹쳐 그린다(#76)
                || views.iter().any(|v| !v.screen.images.is_empty())
                || (single_tab && views.iter().any(|v| v.progress.is_some())),
            theme: (theme.background, theme.foreground),
            dim: (dim_alpha, dim_fill, divider_color),
        };
        let started = Instant::now();
        let n = (size.width * size.height) as usize;
        let full =
            chrome.overlay || self.w.last_frame.as_ref().is_none_or(|f| f.chrome != chrome) || self.w.frame.len() != n;
        if self.w.frame.len() != n {
            self.w.frame = vec![0; n];
        }
        let mut repainted_rows = 0;
        // 배경 그림 층(#134): 창 크기·바탕색이 바뀌면 다시 합성한다
        let key = (size.width, size.height, theme.background.to_u32());
        match self.bg_image.as_ref() {
            Some(img) if self.w.backdrop.as_ref().map(|b| b.0) != Some(key) => {
                let bg_alpha = self.config.background_opacity.clamp(0.0, 1.0);
                let a = (bg_alpha * 255.0).round() as u32;
                let mul = |c: u8| (c as u32 * a + 127) / 255;
                let bg = theme.background;
                let base = (a << 24) | (mul(bg.r) << 16) | (mul(bg.g) << 8) | mul(bg.b);
                // 1보다 크면 background-opacity보다 진하게(Ghostty와 같은 뜻)
                let alpha = (self.config.background_image_opacity * bg_alpha).min(1.0);
                let layer = bt_render::background::compose(
                    img,
                    size.width,
                    size.height,
                    self.config.background_image_fit,
                    self.config.background_image_position,
                    self.config.background_image_repeat,
                    alpha,
                    base,
                );
                self.w.backdrop = Some((key, layer));
            }
            Some(_) => {}
            None => self.w.backdrop = None,
        }
        {
            let mut canvas = Canvas::new(&mut self.w.frame, size.width, size.height);
            if let Some((_, layer)) = &self.w.backdrop {
                canvas.set_backdrop(theme.background, layer);
            }
            // 투명 창(#78·#86): 기본 바탕색은 반투명으로 칠하고, 글자 가장자리는 그 위에 알파째 섞는다
            if self.config.background_opacity < 1.0 {
                canvas.set_see_through(theme.background, (self.config.background_opacity * 255.0).round() as u8);
            }
            // background-opacity-cells(#177): 색 바탕 칸도 같은 알파로
            canvas.set_cells_alpha(bt_render::cells_alpha(
                self.config.background_opacity,
                self.config.background_opacity_cells,
            ));
            if full {
                let bg = views
                    .iter()
                    .find(|v| v.focused)
                    .map(|v| v.screen.background)
                    .unwrap_or(theme.background);
                canvas.clear(bg);
                if !labels.is_empty() {
                    tabbar::draw(&mut canvas, fonts, &theme, &labels);
                }
            }
            for ((v, (cells, cursor, pre)), pad) in views.iter().zip(&drawn).zip(&pads) {
                let screen = &v.screen;
                let grid_rect = Rect::new(
                    v.inner.x,
                    v.inner.y,
                    screen.cols as u32 * m.width,
                    screen.rows as u32 * m.height,
                );
                let view = GridView {
                    cols: screen.cols,
                    rows: screen.rows,
                    cells,
                    cursor: *cursor,
                    selection_bg: theme.selection_background,
                    selection_fg: theme.selection_foreground,
                    preedit: pre.as_deref(),
                    min_contrast: self.config.minimum_contrast,
                };
                if full {
                    canvas.fill_rect(v.outer.x, v.outer.y, v.outer.w, v.outer.h, screen.background);
                    draw_grid(&mut canvas, fonts, v.inner.x, v.inner.y, &view);
                    if let Some(p) = pad {
                        crate::padding::paint(
                            &mut canvas,
                            v.outer,
                            grid_rect,
                            m.width,
                            m.height,
                            p,
                            screen.background,
                            None,
                        );
                    }
                    // kitty 그림: 놓인 칸에, 칸 수에 맞춰 늘리거나 원래 크기로. 격자 밖은 자른다
                    let grid_clip = (
                        v.inner.x,
                        v.inner.y,
                        v.inner.x + (screen.cols as u32 * m.width) as i32,
                        v.inner.y + (screen.rows as u32 * m.height) as i32,
                    );
                    for im in &screen.images {
                        let (cw, chh) = (im.cols * m.width, im.rows * m.height);
                        let (w, h) = if im.scaled { (cw, chh) } else { (im.width, im.height) };
                        let clip = (
                            grid_clip.0,
                            grid_clip.1,
                            grid_clip.2.min(v.inner.x + (im.col as u32 * m.width + cw) as i32),
                            grid_clip.3,
                        );
                        canvas.draw_rgba(
                            v.inner.x + (im.col as u32 * m.width) as i32,
                            v.inner.y + im.row * m.height as i32,
                            w,
                            h,
                            &im.rgba,
                            im.width,
                            im.height,
                            clip,
                        );
                    }
                    if split && !v.focused && dim_alpha > 0 {
                        canvas.blend_rect(v.outer.x, v.outer.y, v.outer.w, v.outer.h, dim_fill, dim_alpha);
                    }
                    repainted_rows += screen.rows;
                } else {
                    // 앞 프레임과 다른 줄만: 칸이 다르거나, 커서·조합 글이 있던·있는 줄
                    let prev = self
                        .w
                        .last_frame
                        .as_ref()
                        .and_then(|f| f.panes.iter().find(|p| p.0 == v.id));
                    let mut dirty = dirty_rows(screen.cols, screen.rows, cells, *cursor, pre, prev);
                    if !dirty.iter().any(|d| *d) {
                        continue;
                    }
                    // 여백 색을 늘이는 중에 맨 위·맨 아래 줄이 바뀌면 위아래 띠까지 칸 전체를 다시 그린다(#136)
                    let edge = pad.is_some()
                        && (dirty.first().copied().unwrap_or(false) || dirty.last().copied().unwrap_or(false));
                    if edge {
                        dirty.iter_mut().for_each(|d| *d = true);
                    }
                    repainted_rows += dirty.iter().filter(|d| **d).count();
                    let spans = draw_grid_rows(&mut canvas, fonts, v.inner.x, v.inner.y, &view, &dirty);
                    if let Some(p) = pad {
                        crate::padding::paint(
                            &mut canvas,
                            v.outer,
                            grid_rect,
                            m.width,
                            m.height,
                            p,
                            screen.background,
                            (!edge).then_some(&dirty[..]),
                        );
                    }
                    if split && !v.focused && dim_alpha > 0 {
                        if edge {
                            canvas.blend_rect(v.outer.x, v.outer.y, v.outer.w, v.outer.h, dim_fill, dim_alpha);
                        } else {
                            for (top, bottom) in spans {
                                canvas.blend_rect(
                                    v.outer.x,
                                    top,
                                    v.outer.w,
                                    (bottom - top) as u32,
                                    dim_fill,
                                    dim_alpha,
                                );
                            }
                        }
                    }
                }
                if v.focused {
                    // IME 후보 창을 커서 옆에 띄운다
                    let px = v.inner.x + (screen.cursor.col as u32 * m.width) as i32;
                    let py = v.inner.y + (screen.cursor.row as u32 * m.height) as i32;
                    gfx.window
                        .set_ime_cursor_area(PhysicalPosition::new(px, py), PhysicalSize::new(m.width, m.height));
                }
                if !full {
                    continue;
                }
                if let Some((_, query, status, missing)) = search.as_ref().filter(|s| s.0 == v.id) {
                    tabbar::draw_search_bar(
                        &mut canvas,
                        fonts,
                        &theme,
                        v.outer,
                        &format!("{query}{preedit}"),
                        status,
                        *missing,
                    );
                }
                if resize_overlays.contains(&v.id) {
                    tabbar::draw_resize_overlay(
                        &mut canvas,
                        fonts,
                        &theme,
                        v.outer,
                        self.config.resize_overlay_position,
                        &format!("{} × {}", screen.cols, screen.rows),
                    );
                }
                // 탭 막대가 없으면 진행 막대를 칸 맨 위에 얇게(Ghostty와 같은 자리). 있으면 탭 줄에 보인다
                if let (true, Some(p)) = (single_tab, v.progress) {
                    tabbar::draw_progress(&mut canvas, &theme, v.outer.x, v.outer.y, v.outer.w, p, m.stroke * 3);
                }
            }
            if full {
                for d in &dividers {
                    canvas.fill_rect(d.x, d.y, d.w, d.h, divider_color);
                }
                // 실제 주소를 창 왼쪽 아래에 보인다(OSC 8 링크는 보이는 글과 주소가 다를 수 있다)
                if let Some((_, hit)) = &hover {
                    tabbar::draw_link_hint(&mut canvas, fonts, &theme, &bt_core::proto::printable(&hit.url));
                }
                if let Some((q, rows, sel)) = &palette {
                    tabbar::draw_palette(&mut canvas, fonts, &theme, &format!("{q}{preedit}"), rows, *sel);
                }
                if let Some((text, _)) = &toast {
                    crate::tabbar::draw_toast(&mut canvas, fonts, &theme, text);
                }
                if let Some(text) = &title_prompt {
                    tabbar::draw_prompt(
                        &mut canvas,
                        fonts,
                        &theme,
                        "제목 › ",
                        &format!("{text}{preedit}"),
                        "↩ 정하기    esc 취소    비우고 ↩면 프로그램이 정한 제목으로",
                    );
                }
                if let Some((message, yes)) = &confirm {
                    tabbar::draw_confirm(&mut canvas, fonts, &theme, message, &format!("↩ {yes}    esc 취소"));
                }
            }
        }
        let draw_ms = started.elapsed().as_secs_f64() * 1000.0;
        self.w.last_frame = Some(FrameMemo {
            chrome,
            panes: views
                .iter()
                .zip(drawn)
                .map(|(v, (c, cur, pre))| (v.id, c, cur, pre))
                .collect(),
        });
        gfx.window.set_title(&title);
        // 바뀐 것이 없으면 창에 올리지 않는다(올리기만 약 10ms, 실험 E2)
        let force_present = std::mem::take(&mut self.w.force_present);
        if !full && repainted_rows == 0 && !force_present {
            return;
        }
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        let t_resize = Instant::now();
        if gfx.surface.resize(w, h).is_err() {
            return;
        }
        let t_buffer = Instant::now();
        let Ok(mut buffer) = gfx.surface.buffer_mut() else {
            return;
        };
        let t_copy = Instant::now();
        // macOS의 softbuffer는 프레임마다 빈 버퍼를 준다(age 0). 들고 있는 프레임을 통째로 옮긴다.
        // 투명 창의 알파는 캔버스가 이미 곱한 알파로 그렸다(#86)
        buffer.copy_from_slice(&self.w.frame);
        let t_present = Instant::now();
        let _ = buffer.present();
        let ms = |a: Instant, b: Instant| (b - a).as_secs_f64() * 1000.0;
        let end = Instant::now();
        log::debug!(
            "다시 그리기: {} {}줄 그리기 {:.2}ms, 올리기까지 {:.2}ms (크기 {:.2} 버퍼 {:.2} 옮기기 {:.2} present {:.2})",
            if full { "통째" } else { "바뀐 줄" },
            repainted_rows,
            draw_ms,
            ms(started, end),
            ms(t_resize, t_buffer),
            ms(t_buffer, t_copy),
            ms(t_copy, t_present),
            ms(t_present, end)
        );
    }

    /// 고른 칸의 커서가 깜빡여야 하는가(창을 보고 있고 찾기 막대가 닫혀 있을 때만)
    fn blink_wanted_for(&self, views: &[PaneView]) -> bool {
        self.w.focused
            && self.w.search.is_none()
            && views
                .iter()
                .any(|v| v.focused && v.screen.cursor.visible && v.screen.cursor.blinking)
    }

    /// 커서를 보이게 하고 깜빡임을 처음부터(키를 치는 동안은 깜빡이지 않는다)
    fn reset_blink(&mut self) {
        self.w.blink_next = Instant::now() + BLINK;
        if !self.w.blink_on {
            self.w.blink_on = true;
            self.request_redraw();
        }
    }
}

/// 커서 깜빡임 간격
const BLINK: Duration = Duration::from_millis(600);

/// 칸 하나의 격자 밖 모양: id, 바깥·격자 자리, 고른 칸인가, 바탕색, 칸·줄 수
type PaneKey = (u64, Rect, Rect, bool, bt_core::Rgb, usize, usize);
/// 탭 줄 한 칸: 제목, 보고 있음, 안 읽음, 진행
type LabelKey = (String, bool, bool, Option<(u8, u8)>);
/// 앞 프레임의 칸 하나: id, 셀, 커서, 조합 글
type PaneFrame = (u64, Vec<DrawCell>, Option<DrawCursor>, Option<String>);

/// 격자 밖에 그린 것들. 앞 프레임과 같으면 격자의 바뀐 줄만 다시 그린다(#40)
#[derive(PartialEq)]
struct Chrome {
    size: (u32, u32),
    cell: (u32, u32),
    panes: Vec<PaneKey>,
    labels: Vec<LabelKey>,
    overlay: bool,
    theme: (bt_core::Rgb, bt_core::Rgb),
    dim: (u8, bt_core::Rgb, bt_core::Rgb),
}

/// 앞 프레임: 격자 밖 것들과 칸마다 (id, 셀, 커서, 조합 글)
struct FrameMemo {
    chrome: Chrome,
    panes: Vec<PaneFrame>,
}

/// 앞 프레임과 견줘 다시 그릴 줄. 앞 프레임이 없거나 크기가 다르면 모두
fn dirty_rows(
    cols: usize,
    rows: usize,
    cells: &[DrawCell],
    cursor: Option<DrawCursor>,
    preedit: &Option<String>,
    prev: Option<&PaneFrame>,
) -> Vec<bool> {
    let Some((_, pcells, pcursor, ppre)) = prev.filter(|p| p.1.len() == cells.len()) else {
        return vec![true; rows];
    };
    let mut dirty: Vec<bool> = (0..rows)
        .map(|r| cells[r * cols..(r + 1) * cols] != pcells[r * cols..(r + 1) * cols])
        .collect();
    if cursor != *pcursor || preedit != ppre {
        for c in [cursor, *pcursor].into_iter().flatten() {
            if c.row < rows {
                dirty[c.row] = true;
            }
        }
    }
    dirty
}

/// OSC 7 폴더를 새 탭의 시작 폴더로 써도 되는가: 호스트가 이 컴퓨터(빈 값·localhost·호스트 이름)이고 실제 폴더일 때만.
/// ssh 안의 셸이 보낸 원격 경로나 터미널에 찍힌 글이 고른 경로로 새 셸을 열지 않는다
pub fn osc7_is_local(host: &str, path: &std::path::Path) -> bool {
    is_local_host(host) && path.is_absolute() && path.is_dir()
}

/// 호스트 이름이 이 컴퓨터인가(빈 값·localhost·호스트 이름·그 짧은 이름). OSC 7 폴더와 file 링크가 쓴다
pub fn is_local_host(host: &str) -> bool {
    let local = gethostname::gethostname().to_string_lossy().to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    let short = local.split('.').next().unwrap_or(&local).to_string();
    host.is_empty() || host == "localhost" || host == local || host.split('.').next() == Some(short.as_str())
}

/// 칸에 바이트를 보낼 때 고른 영역을 지우나(Ghostty `selection-clear-on-typing`, #162).
/// 끄더라도 Esc는 고른 영역을 지운다(Ghostty 문서: "pressing escape")
fn clears_selection_on_write(clear_on_typing: bool, bytes: &[u8]) -> bool {
    clear_on_typing || bytes == b"\x1b"
}

/// 복사한 뒤 고른 영역을 지우나(Ghostty `selection-clear-on-copy`, #162)
fn clears_selection_after_copy(clear_on_copy: bool) -> bool {
    clear_on_copy
}

/// 화면 값을 렌더러 셀로. 찾기에 맞은 칸은 여기서 색을 바꾼다(`search-*` 색, 지금 고른 곳은 `search-selected-*`)
pub fn to_draw_cells(s: &Screen, theme: &bt_core::Theme) -> Vec<DrawCell> {
    let mut cells: Vec<DrawCell> = s
        .cells
        .iter()
        .map(|c| {
            let (fg, bg) = if c.flags.contains(CellFlags::MATCH_CURRENT) {
                theme.search.pick(true, c.fg, c.bg)
            } else if c.flags.contains(CellFlags::MATCH) {
                theme.search.pick(false, c.fg, c.bg)
            } else {
                (c.fg, c.bg)
            };
            DrawCell {
                ch: c.ch,
                fg,
                bg,
                bold: c.flags.contains(CellFlags::BOLD),
                italic: c.flags.contains(CellFlags::ITALIC),
                underline: c.flags.contains(CellFlags::UNDERLINE) || c.flags.contains(CellFlags::UNDERCURL),
                strike: c.flags.contains(CellFlags::STRIKE),
                wide: c.flags.contains(CellFlags::WIDE),
                spacer: c.flags.contains(CellFlags::SPACER),
                selected: c.flags.contains(CellFlags::SELECTED),
                emoji: c.combining.contains(&'\u{fe0f}'),
                cluster: None,
                cluster_cells: 0,
            }
        })
        .collect();
    // 이모지 묶음(ZWJ·피부색·국기, #87)
    bt_render::cluster::mark(&mut cells, s.cols, |i| {
        let c = &s.cells[i];
        bt_render::cluster::ClusterCell {
            ch: c.ch,
            combining: &c.combining,
            spacer: c.flags.contains(CellFlags::SPACER),
        }
    });
    cells
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.w.gfx.is_some() {
            return;
        }
        // initial-window = false면 글꼴 크기를 알려고 만드는 창이라 보이지 않게 만든다(#178)
        let no_window = crate::lifecycle::start_without_window(
            self.config.initial_window,
            self.first.as_ref().is_some_and(|f| f.program.is_some()),
        );
        let Some(gfx) = self.create_gfx_with(event_loop, no_window) else {
            event_loop.exit();
            return;
        };
        self.last_focused = Some(gfx.window.id());
        self.w.focused = gfx.window.has_focus();
        self.w.gfx = Some(gfx);
        #[cfg(target_os = "macos")]
        if self.menu.is_none() {
            let menu = crate::menu::build();
            menu.init_for_nsapp();
            let proxy = self.proxy.clone();
            muda::MenuEvent::set_event_handler(Some(move |e: muda::MenuEvent| {
                let _ = proxy.send_event(UserEvent::Menu(e.id.0.clone()));
            }));
            self.menu = Some(menu);
        }
        apply_hidden_from_dock(self.config.macos_hidden);
        self.load_fonts();
        if self.fonts.is_none() {
            event_loop.exit();
            return;
        }
        self.load_background();
        self.apply_initial_size();
        self.register_hotkeys();
        if self.shell_integration_dir.is_none() && self.config.shell_integration != "none" {
            match crate::shellint::install() {
                Ok(dir) => self.shell_integration_dir = Some(dir),
                Err(e) => log::warn!("셸 통합 스크립트를 풀지 못했습니다(셸 통합 없이 띄웁니다): {e}"),
            }
        }
        if self.control.is_none() {
            match crate::control::start(self.proxy.clone()) {
                Ok(c) => {
                    log::info!("제어 소켓: {}", c.path.display());
                    self.control = Some(c);
                }
                Err(e) => log::warn!("제어 소켓을 열지 못했습니다(btctl·훅이 동작하지 않습니다): {e}"),
            }
        }
        if self.panes.is_empty() {
            let mut first = self.first.take().unwrap_or_default();
            // initial-window = false: 창 없이 띄운다(#178). 글꼴 크기를 알려고 만든 창은 닫고, 새 창은 메뉴·btctl·전역 단축키로 연다
            if crate::lifecycle::start_without_window(self.config.initial_window, first.program.is_some()) {
                self.w.gfx = None;
                self.last_focused = None;
                self.on_last_window_closed(event_loop, true);
                return;
            }
            // 창 상태 복원(#94): -e·--cwd로 띄운 것이 아니고 initial-command도 없으면 지난번 창·탭·분할을 되살린다
            if self.config.window_save_state
                && first.program.is_none()
                && first.cwd.is_none()
                && self.config.initial_command.is_none()
                && self.restore_state(event_loop)
            {
                return;
            }
            // 되살린 창은 복원한 크기·자리가 이기므로 여기(복원하지 않는 경우)에서만 적용한다
            self.apply_start_window(event_loop, true);
            if first.program.is_none() {
                // 첫 칸은 initial-command, 없으면 command(#178)
                let d = match crate::lifecycle::first_command(
                    self.config.initial_command.as_deref(),
                    self.config.command.as_deref(),
                ) {
                    Some(cmd) => {
                        let mut d = self.default_spawn();
                        let (program, args) = crate::lifecycle::shell_command(cmd);
                        d.program = Some(program);
                        d.args = args;
                        d
                    }
                    None => self.default_spawn(),
                };
                first.program = d.program;
                first.args = d.args;
            }
            if first.cwd.is_none() {
                first.cwd = self.default_spawn().cwd;
            }
            self.open_tab(first);
        }
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // write_*_file로 쓴 파일은 화면 내용이라 남기지 않는다(#125)
        for f in self.written_files.drain(..) {
            let _ = std::fs::remove_file(f);
        }
        // 끌 때 창 상태를 적는다(#94). 마지막 창을 닫고 꺼진 것이면 창이 없어 파일을 지운다
        if self.config.window_save_state {
            self.save_state();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Redraw => {
                // 화면이 바뀌면 짚고 있던 링크도 바뀔 수 있다
                if self.w.hover.is_some() {
                    self.update_hover();
                }
                self.check_text_waiters();
                // 어느 창의 칸이 바뀌었는지 모르므로 모든 창(바뀐 줄이 없는 창은 올리지 않는다)
                self.request_redraw_all()
            }
            UserEvent::Hotkey(id) => {
                if let Some(a) = self.hotkey_actions.get(&id).cloned() {
                    self.handle_action(a, event_loop);
                }
            }
            UserEvent::Menu(id) => match crate::menu::action_for(&id) {
                // 메뉴는 보고 있는 창에 대한 것
                _ if self.last_focused.is_some_and(|w| !self.switch_to(w)) => {}
                Some(crate::menu::MenuCommand::Action(a)) => self.handle_action(a, event_loop),
                Some(crate::menu::MenuCommand::OpenSettings) => self.open_settings(),
                None => {}
            },
            UserEvent::Control(req, reply) => {
                let req_id = req.id;
                let timeout = match &req.call {
                    bt_core::proto::Call::Run { timeout_ms, .. } | bt_core::proto::Call::Wait { timeout_ms, .. } => {
                        Some(*timeout_ms)
                    }
                    _ => None,
                };
                let resp = self.handle_call(req, event_loop);
                // run: 명령을 넣었으면 끝날 때 답한다(기한이 지나면 about_to_wait가 오류로 답한다)
                if resp.id == u64::MAX {
                    let deadline = Instant::now() + Duration::from_millis(timeout.unwrap_or(60_000));
                    if let Some(tab_id) = resp.result.get("pending").and_then(|v| v.as_u64()) {
                        self.run_waiters.insert(tab_id, (req_id, reply, deadline));
                    } else if let Some(pane) = resp.result.get("pending_wait").and_then(|v| v.as_u64()) {
                        let text = resp.result["text"].as_str().unwrap_or_default().to_string();
                        self.text_waiters.push(TextWaiter {
                            pane,
                            text,
                            req_id,
                            reply,
                            deadline,
                        });
                    }
                } else {
                    let _ = reply.send(resp);
                }
            }
            UserEvent::Session(id, e) => {
                // 그 칸이 든 창이 지금 창이 되게(보고 있음·안 읽음·다시 그리기를 그 창 기준으로)
                self.switch_to_pane(id);
                let Some(idx) = self.panes.iter().position(|t| t.id == id) else {
                    // 들고 있던 칸의 셸이 스스로 끝났다
                    if matches!(e, SessionEvent::Exited(_)) {
                        self.held_exited(id);
                    }
                    return;
                };
                match e {
                    SessionEvent::Title(t) => {
                        self.panes[idx].title = bt_core::proto::printable(&t).chars().take(200).collect()
                    }
                    SessionEvent::Cwd { host, path } => {
                        if osc7_is_local(&host, &path) {
                            self.panes[idx].cwd = Some(path);
                        }
                    }
                    SessionEvent::Notification(note) => {
                        // 프로그램의 OSC 9·777 알림(desktop-notifications). 끄면 버린다
                        if self.config.desktop_notifications {
                            self.on_notification(idx, note)
                        }
                    }
                    SessionEvent::Exited(code) => {
                        self.panes[idx].exited = Some(code);
                        let runtime = self.panes[idx].created.elapsed();
                        let action = crate::lifecycle::exit_action(
                            self.config.wait_after_command,
                            runtime,
                            self.config.abnormal_command_exit_runtime,
                            code,
                            // Ghostty처럼 macOS는 로그인 셸을 거쳐 띄워서 어떤 종료 코드든 비정상으로 본다
                            cfg!(target_os = "macos"),
                        );
                        match action {
                            // 셸이 끝나면 그 칸을 닫는다(Ghostty 기본과 같다)
                            crate::lifecycle::ExitAction::Close => {
                                self.close_pane(idx, event_loop, false);
                                return;
                            }
                            // 칸을 남기고 안내를 그린다. 아무 키나 누르면 닫는다
                            crate::lifecycle::ExitAction::Hold(reason) => {
                                let p = &mut self.panes[idx];
                                p.held_after_exit = true;
                                p.session
                                    .feed_local(crate::lifecycle::hold_message(reason, code, runtime).as_bytes());
                            }
                        }
                    }
                    SessionEvent::Progress { state, percent } => {
                        self.panes[idx].progress = (state != 0).then(|| (state, percent, Instant::now()));
                    }
                    SessionEvent::ClipboardStore(text) => {
                        // 프로그램이 OSC 52로 보낸 글(Claude Code의 /copy 등).
                        // 보고 있는 창의 고른 칸에서 온 것만 받는다: 뒤쪽 탭이나 옆 칸에 흘러간 출력이 몰래 클립보드를 바꿔
                        // 나중의 붙여넣기에 명령을 끼워 넣지 못하게(붙여넣기 하이재킹). 받으면 알림으로 보인다
                        let watching = self.w.focused && self.is_focused(idx);
                        if watching && text.len() <= 1024 * 1024 {
                            if let Some(cb) = self.clipboard.as_mut() {
                                let _ = cb.set_text(text.clone());
                            }
                            let shown: String = bt_core::proto::printable(&text).chars().take(40).collect();
                            log::info!(
                                "탭 {}: 프로그램이 클립보드에 {}글자를 썼습니다",
                                self.panes[idx].id,
                                text.chars().count()
                            );
                            // 보고 있는 탭이라 안 읽음·데스크톱 알림으로는 보이지 않는다. 창 안 알림 띠로 늘 보인다
                            if self.config.app_notifications.clipboard_copy {
                                self.toast(format!("클립보드에 복사됨: {shown}"));
                            }
                        } else {
                            log::warn!(
                                "탭 {}: 보고 있지 않은 탭의 클립보드 쓰기(OSC 52)를 버렸습니다",
                                self.panes[idx].id
                            );
                        }
                    }
                    SessionEvent::ClipboardLoad(reply) => {
                        // clipboard-read = allow일 때만 엔진이 이 사건을 낸다
                        let text = self
                            .clipboard
                            .as_mut()
                            .and_then(|cb| cb.get_text().ok())
                            .unwrap_or_default();
                        self.panes[idx].session.write(reply.format(&text).as_bytes());
                    }
                    SessionEvent::CommandStarted(cmd) => self.panes[idx].running = Some((cmd, Instant::now())),
                    SessionEvent::CommandFinished(rec) => self.on_command_finished(idx, rec),
                    SessionEvent::Bell => self.on_bell(idx),
                    SessionEvent::Output => {}
                }
                self.request_redraw();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // 사건을 받은 창을 지금 창으로(창마다 상태를 바꿔 끼운다, #36)
        if !self.switch_to(id) {
            return;
        }
        match event {
            WindowEvent::CloseRequested => self.request(Pending::CloseWindow, event_loop),
            WindowEvent::Resized(_) => self.resize_all(),
            // 다른 화면으로 옮겨 갔을 수 있다. 다시 그리지 않고 올리기만 해 softbuffer가 그 화면의 색 공간을 고르게 한다(#97)
            WindowEvent::Moved(_) => {
                self.w.force_present = true;
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // 창을 만든 직후에도 같은 배율로 한 번 오므로 바뀌었을 때만 글꼴을 다시 읽는다
                if (scale_factor - self.scale).abs() < f64::EPSILON {
                    return;
                }
                self.scale = scale_factor;
                self.load_fonts();
                self.resize_all();
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
                self.update_hover();
            }
            WindowEvent::Focused(focused) => {
                self.w.focused = focused;
                // 퀵 터미널은 포커스를 잃으면 숨는다(Ghostty quick-terminal-autohide 기본과 같다)
                // 보인 직후(0.5초 안)의 포커스 잃음은 앱이 앞으로 오는 중일 수 있어 무시한다
                let just_shown = self
                    .w
                    .quick_shown
                    .is_some_and(|t| t.elapsed() < Duration::from_millis(500));
                if !focused && self.w.quick && self.config.quick.autohide && !just_shown {
                    log::debug!("퀵 터미널이 포커스를 잃어 숨깁니다");
                    self.hide_quick();
                }
                if focused {
                    self.last_focused = Some(id);
                }
                if focused {
                    // 창으로 돌아오면 보고 있던 탭의 안 읽음을 지운다
                    self.clear_visible_unread();
                    self.request_redraw();
                }
                if let Some(t) = self.active_pane() {
                    if t.session.modes().focus_events {
                        t.session.write(if focused { b"\x1b[I" } else { b"\x1b[O" });
                    }
                }
            }
            WindowEvent::Ime(ime) => match ime {
                Ime::Preedit(text, _) => {
                    // 조합을 시작하면 고른 영역을 지운다(Ghostty `selection-clear-on-typing`: preedit 시작도 입력으로 본다)
                    if self.config.selection_clear_on_typing
                        && !text.is_empty()
                        && self.w.preedit.is_empty()
                        && self.w.search.is_none()
                        && self.w.palette.is_none()
                    {
                        if let Some(t) = self.active_pane() {
                            t.session.select_clear();
                        }
                    }
                    self.w.preedit = text;
                    self.request_redraw();
                }
                Ime::Commit(text) => {
                    self.w.preedit.clear();
                    self.reset_blink();
                    // 확인 막대가 떠 있으면 글을 어디에도 보내지 않는다(Enter·Esc만 받는다)
                    if self.w.confirm.is_some() {
                        self.request_redraw();
                        return;
                    }
                    // 팔레트·찾기 막대가 열려 있으면 확정 글은 거기로
                    if let Some((_, t)) = self.w.title_prompt.as_mut() {
                        t.push_str(&text);
                        self.request_redraw();
                    } else if let Some((q, sel)) = self.w.palette.as_mut() {
                        q.push_str(&text);
                        *sel = 0;
                        self.request_redraw();
                    } else if self.w.search.is_some() {
                        self.search_push(&text);
                    } else {
                        self.write_active(text.as_bytes());
                    }
                }
                Ime::Enabled | Ime::Disabled => {
                    self.w.preedit.clear();
                    self.request_redraw();
                }
            },
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                // 명령이 끝나 남겨 둔 칸은 아무 키나 누르면 닫는다(wait-after-command, #178)
                if crate::lifecycle::is_dismiss_key(&event.logical_key) {
                    let held = self
                        .focused_id()
                        .and_then(|id| self.pane_index(id))
                        .filter(|&i| self.panes[i].held_after_exit);
                    if let Some(i) = held {
                        self.close_pane(i, event_loop, false);
                        self.request_redraw();
                        return;
                    }
                }
                self.reset_blink();
                if self.confirm_key(&event, event_loop)
                    || self.title_key(&event)
                    || self.palette_key(&event, event_loop)
                    || self.search_key(&event)
                {
                    return;
                }
                // 키 묶음(키 순서 포함, #106): 앞 키는 기다리고, 순서가 깨지면 쌓은 키를 그대로 보낸다
                let trigger = keys::trigger(&event, self.mods);
                let modes = self.active_pane().map(|t| t.session.modes()).unwrap_or_default();
                let meta = self.config.option_as_alt != bt_core::config::OptionAsAlt::None;
                let bytes = keys::encode(&event, self.mods, modes, meta);
                // performable: 묶음은 동작을 지금 할 수 있을 때만 키를 먹는다
                let mut seq = std::mem::take(&mut self.w.keyseq);
                let keymap = std::mem::take(&mut self.keymap);
                let out = seq.feed_with(&keymap, trigger, bytes, &mut |a| self.can_perform(a));
                self.keymap = keymap;
                self.w.keyseq = seq;
                if out.action.is_none() && self.config.mouse_hide_while_typing {
                    if let Some(g) = &self.w.gfx {
                        g.window.set_cursor_visible(false);
                    }
                }
                if !out.send.is_empty() {
                    self.write_active(&out.send);
                }
                if let Some(action) = out.action {
                    match action {
                        // all:은 글 보내기를 모든 칸에 한다. 다른 동작은 칸 하나의 일이 아니라 지금 칸·창에만 한다
                        Action::Text(text) if out.all => {
                            for p in &self.panes {
                                p.session.write(text.as_bytes());
                            }
                            self.request_redraw();
                        }
                        action => self.handle_action(action, event_loop),
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.w.cursor_pos = position;
                if self.config.mouse_hide_while_typing {
                    if let Some(g) = &self.w.gfx {
                        g.window.set_cursor_visible(true);
                    }
                }
                self.update_hover();
                // 마우스가 올라간 칸을 고른다(focus-follows-mouse, #126). 고르는 중에는 바꾸지 않는다
                if self.config.focus_follows_mouse && !self.w.selecting {
                    if let Some(id) = self.pane_at(position).filter(|id| Some(*id) != self.focused_id()) {
                        self.focus_pane(id);
                    }
                }
                let Some(idx) = self.active_index() else { return };
                if self.w.selecting {
                    if let Some((c, r)) = self.cell_in(idx, position) {
                        self.panes[idx].session.select_update(c, r);
                        self.request_redraw();
                    }
                } else if let Some(m) = self.mouse_reporting(idx) {
                    // 움직임 보고는 고른 칸 위에서만
                    if m.mouse_motion && self.pane_at(position) == Some(self.panes[idx].id) {
                        self.send_mouse(idx, mouse::Button::None, mouse::Action::Motion);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // 탭 줄을 누르면 그 탭으로
                if (self.w.cursor_pos.y as u32) < self.tabbar_height() {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        let width = self.w.gfx.as_ref().map(|g| g.window.inner_size().width).unwrap_or(0);
                        let hit = self
                            .fonts
                            .as_ref()
                            .and_then(|f| tabbar::hit(f, width, self.w.cursor_pos.x, self.w.tabs.len()));
                        if let Some(i) = hit {
                            self.activate(i);
                        }
                    }
                    return;
                }
                // Cmd-클릭: 링크를 연다. 누름과 놓기 모두 프로그램(마우스 보고)·고르기에 보내지 않는다
                if button == MouseButton::Left {
                    if state == ElementState::Released && self.w.link_click {
                        self.w.link_click = false;
                        return;
                    }
                    if state == ElementState::Pressed && self.link_mod() {
                        if let Some((_, hit)) = self.link_under(self.w.cursor_pos) {
                            self.w.link_click = true;
                            self.open_link(&hit.url);
                            return;
                        }
                    }
                }
                // 다른 칸을 누르면 그 칸을 고른다. 누름은 고른 칸이 그대로 받는다
                if state == ElementState::Pressed {
                    if let Some(id) = self
                        .pane_at(self.w.cursor_pos)
                        .filter(|id| Some(*id) != self.focused_id())
                    {
                        self.focus_pane(id);
                    }
                }
                let Some(idx) = self.active_index() else { return };
                let mb = match button {
                    MouseButton::Left => mouse::Button::Left,
                    MouseButton::Middle => mouse::Button::Middle,
                    MouseButton::Right => mouse::Button::Right,
                    _ => return,
                };
                if self.mouse_reporting(idx).is_some() {
                    let action = if state == ElementState::Pressed {
                        mouse::Action::Press
                    } else {
                        mouse::Action::Release
                    };
                    self.send_mouse(idx, mb, action);
                    return;
                }
                if button == MouseButton::Right {
                    if state == ElementState::Pressed {
                        self.right_click();
                    }
                    return;
                }
                if button == MouseButton::Middle {
                    if state == ElementState::Pressed && mouse::middle_click_pastes(self.config.middle_click_action) {
                        let text = self.clipboard.as_mut().and_then(|cb| cb.get_text().ok());
                        if let Some(text) = text.filter(|t| !t.is_empty()) {
                            self.paste(text);
                        }
                    }
                    return;
                }
                if button != MouseButton::Left {
                    return;
                }
                match state {
                    ElementState::Pressed => {
                        let Some((c, r)) = self.cell_in(idx, self.w.cursor_pos) else {
                            return;
                        };
                        let now = Instant::now();
                        let clicks = match self.w.last_click {
                            Some((t, lc, lr, n))
                                if now - t
                                    < mouse::repeat_interval(
                                        self.config.click_repeat_interval,
                                        mouse::platform_repeat_interval(),
                                    )
                                    && lc == c
                                    && lr == r =>
                            {
                                n % 3 + 1
                            }
                            _ => 1,
                        };
                        self.w.last_click = Some((now, c, r, clicks));
                        let kind = match clicks {
                            2 => SelectKind::Word,
                            3 => SelectKind::Line,
                            _ => SelectKind::Simple,
                        };
                        if let Some(t) = self.active_pane() {
                            if clicks == 1 {
                                t.session.select_clear();
                            }
                            t.session.select_start(c, r, kind);
                            if clicks > 1 {
                                t.session.select_update(c, r);
                            }
                        }
                        self.w.selecting = true;
                        self.request_redraw();
                    }
                    ElementState::Released => {
                        self.w.selecting = false;
                        // 프롬프트 줄을 끌지 않고 한 번 누르면 그 자리로 커서를 옮긴다(cursor-click-to-move, #137)
                        let released = self.cell_in(idx, self.w.cursor_pos);
                        let plain_click = matches!(
                            (self.w.last_click, released),
                            (Some((_, c, r, 1)), Some((rc, rr))) if (c, r) == (rc, rr)
                        ) && !(self.mods.super_key()
                            || self.mods.control_key()
                            || self.mods.alt_key()
                            || self.mods.shift_key());
                        if plain_click && self.config.cursor_click_to_move {
                            if let Some((col, row)) = released {
                                self.click_to_move(idx, col, row);
                            }
                        }
                        // copy-on-select(Ghostty 기본 켬): 고르기를 마치면 바로 클립보드로
                        // macOS에는 선택 클립보드가 없어 true·clipboard 모두 시스템 클립보드로 간다
                        if self.config.copy_on_select.copies() {
                            self.copy_selection();
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // 휠 배율(mouse-scroll-multiplier, #126): 줄 단위 기기(마우스 휠)와 픽셀 단위 기기(트랙패드)
                let (discrete, precision) = self.config.scroll_multiplier;
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64 * discrete,
                    MouseScrollDelta::PixelDelta(p) => {
                        let ch = self.fonts.as_ref().map(|f| f.metrics().height as f64).unwrap_or(16.0);
                        p.y / ch * precision
                    }
                };
                self.w.scroll_accum += lines;
                let whole = self.w.scroll_accum.trunc() as i32;
                if whole == 0 {
                    return;
                }
                self.w.scroll_accum -= whole as f64;
                // 휠은 마우스 아래 칸이 받는다(고르지 않은 칸도 굴려 볼 수 있게)
                let Some(idx) = self
                    .pane_at(self.w.cursor_pos)
                    .and_then(|id| self.pane_index(id))
                    .or_else(|| self.active_index())
                else {
                    return;
                };
                let modes = self.panes[idx].session.modes();
                if self.mouse_reporting(idx).is_some() {
                    let b = if whole > 0 {
                        mouse::Button::WheelUp
                    } else {
                        mouse::Button::WheelDown
                    };
                    for _ in 0..whole.unsigned_abs().min(10) {
                        self.send_mouse(idx, b, mouse::Action::Press);
                    }
                } else if modes.alt_screen && modes.alternate_scroll {
                    // 대체 화면(less·vim)에서는 휠을 위·아래 화살표로 보낸다
                    let key = if whole > 0 {
                        bt_term::NamedKey::Up
                    } else {
                        bt_term::NamedKey::Down
                    };
                    let one = bt_term::encode_named(key, bt_term::Mods::default(), modes.app_cursor);
                    let bytes = one.repeat(whole.unsigned_abs().min(10) as usize);
                    self.panes[idx].session.write(&bytes);
                } else {
                    self.panes[idx].session.scroll(whole);
                    self.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // 마지막 창을 닫고 기다리던 시간이 지났다(quit-after-last-window-closed-delay, #178)
        if self.quit_deadline.is_some_and(|d| Instant::now() >= d) {
            event_loop.exit();
            return;
        }
        // 1초마다 전경 프로세스를 본다(사이드바 이름, 에이전트 알아보기)
        let now = Instant::now();
        if now >= self.next_poll {
            // 15초 넘게 소식이 없는 진행 표시는 지운다
            let mut expired = false;
            for t in &mut self.panes {
                if t.progress
                    .is_some_and(|(_, _, at)| at.elapsed() >= Duration::from_secs(15))
                {
                    t.progress = None;
                    expired = true;
                }
            }
            for w in std::iter::once(&mut self.w).chain(self.others.iter_mut()) {
                if w.toast.as_ref().is_some_and(|(_, at)| at.elapsed() >= TOAST_FOR) {
                    w.toast = None;
                    expired = true;
                }
            }
            let late: Vec<u64> = self
                .run_waiters
                .iter()
                .filter(|(_, (_, _, d))| *d <= now)
                .map(|(k, _)| *k)
                .collect();
            for tab_id in late {
                if let Some((req_id, reply, _)) = self.run_waiters.remove(&tab_id) {
                    let _ = reply.send(bt_core::proto::Response::err(
                        req_id,
                        "기한 안에 명령이 끝나지 않았습니다(명령은 탭에서 계속 돕니다)",
                    ));
                }
            }
            self.fail_text_waiters(|w| w.deadline <= now, "기한 안에 글이 나오지 않았습니다");
            if self.poll_foreground() || expired {
                self.request_redraw_all();
            }
            self.next_poll = now + Duration::from_secs(1);
        }
        // 커서 깜빡임(창마다)
        for w in std::iter::once(&mut self.w).chain(self.others.iter_mut()) {
            if w.blink_wanted && now >= w.blink_next {
                w.blink_on = !w.blink_on;
                w.blink_next = now + BLINK;
                if let Some(g) = &w.gfx {
                    g.window.request_redraw();
                }
            }
        }
        // 기한이 지난 닫은 칸·탭은 이때 떨어뜨린다(셸이 SIGHUP을 받는다, #164)
        drop(self.undo.expire(now));
        drop(self.redo.expire(now));
        // 크기 표시가 끝난 칸은 지우고 다시 그린다(#122)
        let mut overlay_done = false;
        for p in &mut self.panes {
            if p.overlay_until.is_some_and(|t| t <= now) {
                p.overlay_until = None;
                overlay_done = true;
            }
        }
        if overlay_done {
            self.request_redraw_all();
        }
        let overlay_next = self.panes.iter().filter_map(|p| p.overlay_until).min();
        // 퀵 창 움직임(#110): 프레임마다 자리를 옮기고, 끝나면 숨길 것은 숨긴다
        let mut animating = false;
        for w in std::iter::once(&mut self.w).chain(self.others.iter_mut()) {
            let (Some(a), Some(g)) = (&w.quick_anim, &w.gfx) else {
                continue;
            };
            let t = a.start.elapsed().as_secs_f64() / a.dur.as_secs_f64().max(1e-3);
            let (x, y) = crate::quick::slide(a.from, a.to, t);
            g.window.set_outer_position(winit::dpi::LogicalPosition::new(x, y));
            if t >= 1.0 {
                if a.hide {
                    g.window.set_visible(false);
                }
                w.quick_anim = None;
            } else {
                animating = true;
            }
        }
        let anim_next = animating.then(|| now + QUICK_FRAME);
        let blink_next = std::iter::once(&self.w)
            .chain(&self.others)
            .filter(|w| w.blink_wanted)
            .map(|w| w.blink_next)
            .min();
        // 동기화 출력(DEC 2026)이 끝나지 않은 탭이 있으면 기한에 깨어 반영한다
        let deadline = self
            .panes
            .iter()
            .filter_map(|t| t.session.sync_deadline())
            .chain([self.next_poll])
            .chain(blink_next)
            .chain(anim_next)
            .chain(overlay_next)
            .chain(self.undo.next_deadline())
            .chain(self.redo.next_deadline())
            .chain(self.quit_deadline)
            .min();
        match deadline {
            Some(d) if d <= Instant::now() => {
                if self.panes.iter().any(|t| t.session.flush_expired_sync()) {
                    self.request_redraw_all();
                }
                // 움직이는 중이면 다음 프레임에 다시 깨어난다(기한이 이미 지나 잠들어 버리지 않게)
                event_loop.set_control_flow(if animating {
                    ControlFlow::WaitUntil(Instant::now() + QUICK_FRAME)
                } else {
                    ControlFlow::Wait
                });
            }
            Some(d) => event_loop.set_control_flow(ControlFlow::WaitUntil(d)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}

/// 창이 어떤 방식으로든 전체 화면인가(운영체제 전체 화면 또는 macOS의 새 Space 없는 전체 화면)
fn window_is_fullscreen(window: &Window) -> bool {
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::WindowExtMacOS;
        if window.simple_fullscreen() {
            return true;
        }
    }
    window.fullscreen().is_some()
}

/// macos-hidden: Dock과 앱 전환기에서 앱을 숨기거나(`always`: Accessory) 보인다(`never`: Regular). macOS 밖에서는 하는 일이 없다
#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
fn apply_hidden_from_dock(hidden: bt_core::config::MacHidden) {
    #[cfg(target_os = "macos")]
    {
        use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
        let Some(mtm) = objc2::MainThreadMarker::new() else {
            return;
        };
        let policy = if hidden == bt_core::config::MacHidden::Always {
            NSApplicationActivationPolicy::Accessory
        } else {
            NSApplicationActivationPolicy::Regular
        };
        let app = NSApplication::sharedApplication(mtm);
        if app.activationPolicy() != policy && !app.setActivationPolicy(policy) {
            log::warn!("Dock 숨김 설정(macos-hidden)을 적용하지 못했습니다");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_term::search::SearchStatus;

    #[test]
    fn cursor_can_take_the_cell_colors_like_ghostty() {
        use bt_core::Rgb;
        use bt_core::config::CellColor::{Background, Foreground};
        let theme = (Rgb::new(1, 1, 1), Rgb::new(2, 2, 2));
        let cell = (Rgb::new(200, 0, 0), Rgb::new(0, 0, 50));
        assert_eq!(cursor_colors(None, None, theme, cell), theme);
        // 흔한 짝: 커서는 칸 글자색, 그 위 글자는 칸 바탕색(뒤집어 보이게)
        assert_eq!(
            cursor_colors(Some(Foreground), Some(Background), theme, cell),
            (cell.0, cell.1)
        );
        assert_eq!(cursor_colors(Some(Background), None, theme, cell), (cell.1, theme.1));
    }

    #[test]
    fn selection_is_cleared_by_typing_and_copy_only_when_asked() {
        // 글을 칠 때: 기본(켬)은 지우고, 끄면 지우지 않되 Esc는 지운다
        assert!(clears_selection_on_write(true, b"a"));
        assert!(!clears_selection_on_write(false, b"a"));
        assert!(!clears_selection_on_write(false, "한".as_bytes()));
        assert!(
            clears_selection_on_write(false, b"\x1b"),
            "Esc는 설정과 상관없이 지운다"
        );
        assert!(!clears_selection_on_write(false, b"\x1b[A"), "방향키는 Esc가 아니다");
        // 복사한 뒤: 기본(끔)은 남기고, 켜면 지운다
        assert!(!clears_selection_after_copy(false));
        assert!(clears_selection_after_copy(true));
    }

    #[test]
    fn search_matches_use_search_colors_like_ghostty() {
        use bt_core::Rgb;
        use bt_core::theme::{SearchColor, SearchColors};
        use bt_term::{Emulator, GridSize};
        let mut theme = bt_core::Theme::default();
        let mut e = Emulator::new(GridSize::new(4, 1), theme.clone());
        e.feed(b"\x1b[31mabcd");
        let mut s = e.snapshot();
        s.cells[1].flags.insert(CellFlags::MATCH);
        s.cells[2].flags.insert(CellFlags::MATCH | CellFlags::MATCH_CURRENT);
        let red = theme.palette[1];

        // 기본: 맞은 곳은 검정 글자 + 노랑 바탕, 지금 고른 곳은 검정 글자 + 살구색 바탕(바탕을 섞지 않고 바꾼다)
        let cells = to_draw_cells(&s, &theme);
        assert_eq!(
            (cells[0].fg, cells[0].bg),
            (red, theme.background),
            "맞지 않은 칸은 그대로"
        );
        assert_eq!(
            (cells[1].fg, cells[1].bg),
            (Rgb::new(0, 0, 0), Rgb::new(0xff, 0xe0, 0x82))
        );
        assert_eq!(
            (cells[2].fg, cells[2].bg),
            (Rgb::new(0, 0, 0), Rgb::new(0xf2, 0xa5, 0x7e))
        );

        // 설정: cell-foreground·cell-background는 칸의 색을 따른다
        theme.search = SearchColors {
            foreground: SearchColor::CellBackground,
            background: SearchColor::CellForeground,
            selected_foreground: SearchColor::Rgb(Rgb::new(9, 9, 9)),
            selected_background: SearchColor::Rgb(Rgb::new(1, 2, 3)),
        };
        let cells = to_draw_cells(&s, &theme);
        assert_eq!((cells[1].fg, cells[1].bg), (theme.background, red));
        assert_eq!((cells[2].fg, cells[2].bg), (Rgb::new(9, 9, 9), Rgb::new(1, 2, 3)));
    }

    #[test]
    fn user_title_wins_over_program_title() {
        assert_eq!(title_of(None, "vim", Some("zsh"), false), "vim");
        assert_eq!(title_of(None, "", Some("zsh"), false), "zsh");
        assert_eq!(title_of(None, "", None, false), "셸");
        assert_eq!(
            title_of(Some("빌드"), "vim", Some("zsh"), false),
            "빌드",
            "사람이 붙인 제목이 먼저"
        );
        assert_eq!(title_of(Some("빌드"), "vim", None, true), "🔔 빌드");
        assert_eq!(
            clean_user_title("  빌드\x1b[31m 서버 \n"),
            Some("빌드 [31m 서버".into())
        );
        assert_eq!(clean_user_title("   "), None, "빈 글은 지우기");
        assert_eq!(
            clean_user_title(&"가".repeat(300)).map(|t| t.chars().count()),
            Some(200)
        );
    }

    #[test]
    fn resize_overlay_skips_the_size_set_right_after_creation() {
        use bt_core::config::ResizeOverlay::{AfterFirst, Always, Never};
        let (early, late) = (Duration::from_millis(100), Duration::from_secs(3));
        assert!(!resize_overlay_due(AfterFirst, early), "만든 직후 크기 맞추기");
        assert!(resize_overlay_due(AfterFirst, late));
        assert!(resize_overlay_due(Always, early));
        assert!(!resize_overlay_due(Never, late));
    }

    #[test]
    fn paste_prompt_counts_lines_and_strips_control_chars() {
        let p = paste_prompt("\n  \necho \x1b[31mhi\r\nrm -rf ~\n\n");
        assert!(p.contains("(2줄)"), "{p}");
        assert!(p.ends_with("echo  [31mhi"), "첫 글 줄, 제어 문자는 빈칸: {p}");
        assert!(!p.contains('\x1b'));
        let long = paste_prompt(&("x".repeat(100) + "\ny"));
        assert!(long.ends_with('…'), "{long}");
    }

    #[test]
    fn busy_panes_need_confirm_like_ghostty() {
        use bt_core::config::ConfirmClose::{Always, False, True};
        // 셸 통합이 있으면 명령이 도는지만 본다
        assert!(pane_busy(True, false, true, true, Some(true)));
        assert!(
            !pane_busy(True, false, true, false, Some(false)),
            "프롬프트에 있으면 묻지 않는다"
        );
        // 셸 통합이 없으면 전경 프로세스가 셸인지 본다. 모르면 묻는다
        assert!(!pane_busy(True, false, false, false, Some(true)));
        assert!(pane_busy(True, false, false, false, Some(false)));
        assert!(pane_busy(True, false, false, false, None));
        // always는 프롬프트에서도, false는 언제나 묻지 않음
        assert!(pane_busy(Always, false, true, false, Some(true)));
        assert!(!pane_busy(False, false, true, true, Some(false)));
        // 셸이 이미 끝난 칸은 묻지 않는다
        assert!(!pane_busy(Always, true, true, true, None));
        assert!(!pane_busy(True, true, false, false, None));
    }

    #[test]
    fn btctl_new_starts_from_config_and_given_values_win() {
        let base = SpawnOptions {
            program: Some("/bin/sh".into()),
            args: vec!["-c".into(), "exec fish".into()],
            cwd: Some("/home/me".into()),
            ..Default::default()
        };
        // 아무것도 주지 않으면 설정(command·working-directory) 그대로
        let o = ctl_spawn(base.clone(), None, &[]);
        assert_eq!(o.program.as_deref(), Some("/bin/sh"));
        assert_eq!(o.args, vec!["-c", "exec fish"]);
        assert_eq!(o.cwd.as_deref(), Some(std::path::Path::new("/home/me")));
        // 준 폴더·명령이 이긴다
        let o = ctl_spawn(base, Some("/tmp".into()), &["htop".into(), "-d".into(), "5".into()]);
        assert_eq!(o.program.as_deref(), Some("htop"));
        assert_eq!(o.args, vec!["-d", "5"]);
        assert_eq!(o.cwd.as_deref(), Some(std::path::Path::new("/tmp")));
    }

    #[test]
    fn command_finish_response_follows_new_keys_or_legacy() {
        use bt_core::config::{Config, FinishAction, NotifyOnFinish};
        let secs = Duration::from_secs;
        let r = |bell, notify| FinishResponse { bell, notify };
        let none = r(false, false);

        // 새 키를 적지 않았으면 옛 동작: 10초 넘고 보지 않을 때만 데스크톱 알림
        let legacy = Config::default();
        assert_eq!(command_finish_response(&legacy, secs(10), false), r(false, true));
        assert_eq!(command_finish_response(&legacy, secs(9), false), none);
        assert_eq!(command_finish_response(&legacy, secs(60), true), none);
        let off = Config {
            notify_command_seconds: 0,
            ..Config::default()
        };
        assert_eq!(command_finish_response(&off, secs(60), false), none);
        // 옛 키 값을 바꾸면 따른다
        let c30 = Config {
            notify_command_seconds: 30,
            ..Config::default()
        };
        assert_eq!(command_finish_response(&c30, secs(20), false), none);
        assert_eq!(command_finish_response(&c30, secs(30), false), r(false, true));

        // 새 키를 적으면 옛 키는 무시한다
        let mut c = Config {
            notify_on_command_finish: Some(NotifyOnFinish::Unfocused),
            notify_command_seconds: 0,
            ..Config::default()
        };
        // 기본 동작은 bell, 기본 기간은 5초
        assert_eq!(command_finish_response(&c, secs(5), false), r(true, false));
        assert_eq!(command_finish_response(&c, secs(4), false), none);
        assert_eq!(
            command_finish_response(&c, secs(60), true),
            none,
            "unfocused는 보고 있으면 조용"
        );
        c.notify_on_command_finish = Some(NotifyOnFinish::Always);
        assert_eq!(command_finish_response(&c, secs(60), true), r(true, false));
        c.notify_on_command_finish_action = FinishAction {
            bell: false,
            notify: true,
        };
        assert_eq!(command_finish_response(&c, secs(60), true), r(false, true));
        c.notify_on_command_finish_action = FinishAction {
            bell: false,
            notify: false,
        };
        assert_eq!(command_finish_response(&c, secs(60), false), none);
        c.notify_on_command_finish_action = FinishAction {
            bell: true,
            notify: true,
        };
        c.notify_on_command_finish_after = secs(60);
        assert_eq!(command_finish_response(&c, secs(59), false), none);
        assert_eq!(command_finish_response(&c, secs(61), false), r(true, true));
        // never는 옛 키가 켜 있어도 끈다
        c.notify_on_command_finish = Some(NotifyOnFinish::Never);
        c.notify_command_seconds = 1;
        assert_eq!(command_finish_response(&c, secs(600), false), none);
    }

    #[test]
    fn bell_marks_title_only_when_not_watching_and_bounces_only_when_unfocused() {
        use bt_core::config::BellFeatures;
        let d = BellFeatures::default();
        let r = |f, file, watching, focused| bell_response(f, file, watching, focused);
        // 보고 있으면(창이 앞, 칸이 보임) 아무것도 하지 않는다(기본에 경고음 없음)
        assert_eq!(
            r(d, false, true, true),
            BellResponse {
                title: false,
                attention: false,
                beep: false,
                audio: false
            }
        );
        // 같은 창의 다른 탭: 🔔만
        assert_eq!(
            r(d, false, false, true),
            BellResponse {
                title: true,
                attention: false,
                beep: false,
                audio: false
            }
        );
        // 창이 뒤에 있으면 🔔와 Dock
        assert_eq!(
            r(d, false, false, false),
            BellResponse {
                title: true,
                attention: true,
                beep: false,
                audio: false
            }
        );
        // 끈 것은 하지 않고, system은 보고 있어도 울린다
        let f = BellFeatures {
            system: true,
            attention: false,
            title: false,
            audio: false,
        };
        assert_eq!(
            r(f, false, true, true),
            BellResponse {
                title: false,
                attention: false,
                beep: true,
                audio: false
            }
        );
        assert!(!r(f, false, false, false).attention);
    }

    #[test]
    fn bell_audio_plays_only_when_enabled_and_file_set_even_when_watching() {
        use bt_core::config::BellFeatures;
        let on = BellFeatures {
            audio: true,
            ..BellFeatures::default()
        };
        // 소리는 보고 있어도 난다(system과 같음)
        assert!(bell_response(on, true, true, true).audio);
        assert!(bell_response(on, true, false, false).audio);
        // 파일이 없으면 켜도 재생할 것이 없다
        assert!(!bell_response(on, false, true, true).audio);
        // 끄면(기본) 파일이 있어도 재생하지 않는다
        assert!(!bell_response(BellFeatures::default(), true, true, true).audio);
        // 다른 반응은 audio와 무관하다
        assert!(!bell_response(on, true, true, true).beep);
    }

    #[test]
    fn bell_sound_is_throttled_by_interval_and_still_playing() {
        let ms = Duration::from_millis;
        // 처음이거나 간격이 지났고 앞 소리가 끝났으면 낸다
        assert!(bell_sound_due(None, false));
        assert!(bell_sound_due(Some(ms(100)), false));
        // 0.1초 안에 또 울리면 건너뛴다(벨이 쏟아질 때)
        assert!(!bell_sound_due(Some(ms(99)), false));
        assert!(!bell_sound_due(Some(ms(0)), false));
        // 앞 소리가 아직 나고 있으면 간격이 지났어도 겹치지 않는다
        assert!(!bell_sound_due(Some(ms(5000)), true));
        assert!(!bell_sound_due(None, true));
    }

    #[test]
    fn dirty_rows_are_changed_rows_and_cursor_rows() {
        let blank = DrawCell {
            ch: ' ',
            fg: bt_core::Rgb::new(255, 255, 255),
            bg: bt_core::Rgb::new(0, 0, 0),
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            wide: false,
            spacer: false,
            selected: false,
            emoji: false,
            cluster: None,
            cluster_cells: 0,
        };
        let (cols, rows) = (4, 3);
        let cells = vec![blank.clone(); cols * rows];
        let cur = |row| {
            Some(DrawCursor {
                col: 0,
                row,
                style: CursorStyle::Block,
                wide: false,
                color: blank.fg,
                text_color: blank.bg,
                opacity: 1.0,
            })
        };
        let prev: PaneFrame = (1, cells.clone(), cur(0), None);
        assert_eq!(
            dirty_rows(cols, rows, &cells, cur(0), &None, Some(&prev)),
            vec![false; 3]
        );
        let mut changed = cells.clone();
        changed[cols + 2].ch = 'x';
        assert_eq!(
            dirty_rows(cols, rows, &changed, cur(0), &None, Some(&prev)),
            vec![false, true, false]
        );
        // 커서가 옮겨 가면(깜빡여 사라져도) 있던 줄과 간 줄
        assert_eq!(
            dirty_rows(cols, rows, &cells, cur(2), &None, Some(&prev)),
            vec![true, false, true]
        );
        assert_eq!(
            dirty_rows(cols, rows, &cells, None, &None, Some(&prev)),
            vec![true, false, false]
        );
        assert_eq!(
            dirty_rows(cols, rows, &cells, cur(0), &None, None),
            vec![true; 3],
            "앞 프레임이 없으면 모두"
        );
    }

    /// 보안 검토(#36): 새 창은 포커스 사건이 오기 전까지 보고 있지 않은 창이다(클립보드 쓰기를 받지 않는다)
    #[test]
    fn new_window_state_is_not_focused_until_told() {
        assert!(!Win::new().focused);
    }

    #[test]
    fn keep_tail_cuts_at_char_boundaries() {
        assert_eq!(keep_tail("abc".into(), 10), ("abc".into(), false));
        assert_eq!(keep_tail("abcdef".into(), 3), ("def".into(), true));
        // "한"은 3바이트: 4바이트 안쪽으로 자르면 글자를 쪼개지 않고 "글"만 남는다
        assert_eq!(keep_tail("한글".into(), 4), ("글".into(), true));
    }

    #[test]
    fn search_status_text_shows_position_count_and_missing() {
        let st = |index, total, capped| Some(SearchStatus { index, total, capped });
        assert_eq!(search_status_text("err", st(Some(2), 5, false)), "2/5");
        assert_eq!(search_status_text("e", st(Some(1), 1000, true)), "1/1000+");
        assert_eq!(search_status_text("zz", st(None, 0, false)), "없음");
        assert_eq!(
            search_status_text("", st(None, 0, false)),
            "",
            "빈 글은 아무것도 보이지 않는다"
        );
        assert_eq!(search_status_text("x", None), "");
    }
}
