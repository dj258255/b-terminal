//! 창 하나, 탭 여럿. winit 이벤트를 받아 PTY에 쓰고 화면을 그린다

use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bt_render::{Canvas, DrawCell, DrawCursor, FontConfig, Fonts, GridView, draw_grid_rows};
use bt_term::{CellFlags, CellSide, CursorShape, GridSize, Screen, SelectKind, Session, SessionEvent, SpawnOptions};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::ModifiersState;
use winit::window::{CursorIcon, Window, WindowId};

mod backdrop;
mod gpu_frame;
mod gpu_pace;
mod input_latency;
mod kitty_images;
/// 사람과 AppleScript가 함께 쓰는 마우스 처리(#662). 모든 플랫폼
mod mouse_core;
/// AppleScript 명령과 상태 스냅숏(#634). macOS에만 있다
#[cfg(target_os = "macos")]
mod script;
#[cfg(target_os = "macos")]
pub use script::install as install_scripting;
mod shader_inputs;

use crate::icon;
use crate::keymap::{Action, Keymap, SplitTarget};
use crate::keys;
use crate::mouse;
use crate::pane_font::{self, SizedFonts};
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
/// 새 창의 기본 안쪽 크기(논리 점). `window-width`·`window-height`가 없을 때(`reset_window_size`도 이 크기로 되돌린다)
const DEFAULT_WINDOW_LOGICAL: (f64, f64) = (1000.0, 680.0);
/// 칸의 암호 입력을 살피는 간격(Ghostty termio/Exec.zig `TERMIOS_POLL_MS`, #258)
const SECURE_POLL: Duration = Duration::from_millis(200);
const TOAST_FOR: Duration = Duration::from_millis(2500);
/// 스크롤한 뒤 스크롤 막대를 보이는 시간
const SCROLLBAR_FOR: Duration = Duration::from_millis(1200);

/// 스크롤 막대를 누르고 있는 상태
#[derive(Debug, Clone, Copy)]
struct ScrollbarPress {
    /// 누른 칸
    pane: u64,
    /// 손잡이를 쥔 곳(손잡이 맨 위에서의 거리). 빈 곳을 눌러 쪽 단위로 옮긴 누름이면 없다
    grab: Option<u32>,
}

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
    /// Dock·Finder가 열라고 한 경로(#279). macOS에만 온다
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    OpenFile(std::path::PathBuf),
    /// Finder 서비스 "여기서 새 탭/창"이 넘긴 경로(#631). macOS에만 온다
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    OpenFolders(bool, Vec<std::path::PathBuf>),
    /// 사용자 셰이더 컴파일이 끝났다(컴파일 스레드가 보낸다, ADR-042)
    ShaderCompiled,
    /// 접근성 클라이언트(VoiceOver)의 요청(창, 요청, #560)
    Access(WindowId, crate::a11y::Request),
    /// 칸이 낸 데스크톱 알림을 눌렀다(칸 번호, #625). macOS 앱 묶음에서만 온다
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    NoteClicked(u64),
    /// AppleScript 명령(#634). vendor가 주 스레드에서 넘기고 답을 기다린다. macOS 앱 묶음에서만 온다
    #[cfg(target_os = "macos")]
    Script(
        winit::platform::macos::ScriptCommand,
        winit::platform::macos::ScriptTicket,
        std::sync::mpsc::Sender<winit::platform::macos::ScriptReply>,
    ),
}

/// 앱 설정: 설정 파일(bt_core::Config, Ghostty 문법)을 그대로 쓴다(#13)
pub type AppConfig = bt_core::Config;

/// 설정의 글꼴 키를 렌더러 설정으로
pub(crate) fn font_config(c: &AppConfig, size: f32, scale: f32) -> FontConfig {
    let defaults = FontConfig::default();
    // font-family를 여러 번 쓰면 첫째가 기본 글꼴, 나머지는 기본 대체 글꼴보다 앞에 둔다(Ghostty와 같다)
    let mut fallbacks: Vec<String> = c.font_family.iter().skip(1).cloned().collect();
    fallbacks.extend(defaults.fallbacks);
    FontConfig {
        family: c.font_family.first().cloned(),
        static_family: false,
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
        thicken: c.font_thicken,
        thicken_strength: c.font_thicken_strength,
        synthetic: c.font_synthetic_style,
    }
}

/// 읽기 전용 칸에 `btctl send`·`run`을 보냈을 때의 거절 문구(ADR-018)
const READONLY_REFUSAL: &str = "이 칸은 읽기 전용이라 입력을 받지 않습니다(btctl read·wait는 됩니다)";

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
    /// 벨이 울려 테두리를 두르고 있다(`bell-features = border`, #687). 🔔와 달리 보고 있어도 켜고,
    /// 그 칸이 키를 받는 칸이 되거나 키를 받으면 뗀다
    pub bell_border: bool,
    /// 사람이 붙인 제목(`prompt_surface_title`·`set_surface_title`, #124). 프로그램이 정한 제목보다 먼저
    pub user_title: Option<String>,
    /// 칸을 만든 때와 크기 표시를 지울 때(#122)
    created: Instant,
    overlay_until: Option<Instant>,
    /// 셸이 OSC 7로 알린 작업 폴더
    pub cwd: Option<std::path::PathBuf>,
    /// 빈 OSC 7로 폴더를 지웠고 그 뒤 새 폴더를 받지 못했다(#328)
    pub cwd_reset: bool,
    /// 마지막으로 데스크톱 알림을 띄운 때(탭마다 묶는다)
    last_desktop: Option<Instant>,
    /// 프로그램 상태(OSC 7501)로 마지막 데스크톱 알림을 띄운 때(칸마다 5초에 한 번, #618)
    last_status_note: Option<Instant>,
    /// 이 칸이 낸 데스크톱 알림의 ID(#625). 그 칸을 보거나 닫으면 알림 센터에서 지운다
    notes: Vec<String>,
    /// 셸 통합: 지금 도는 명령(글, 시작한 때)과 마지막으로 끝난 명령
    pub running: Option<(Option<String>, Instant)>,
    pub last_command: Option<bt_term::CommandRecord>,
    /// 진행 표시(OSC 9;4): (상태, 퍼센트, 받은 때). 15초 동안 새 소식이 없으면 지운다(Ghostty와 같다)
    pub progress: Option<(u8, u8, Instant)>,
    /// 스크롤 막대: 마지막으로 본 스크롤 위치(맨 아래에서 올라간 줄 수)와 막대를 보일 때까지의 기한
    scroll_seen: usize,
    scrollbar_until: Option<Instant>,
    /// `toggle_mouse_reporting`로 이 칸에 정한 마우스 보고 켬·끔(#209). 없으면 `mouse-reporting` 설정을 따른다
    mouse_reporting_over: Option<bool>,
    /// 마우스 보고: 눌린 단추(`mouse_bit`)와 마지막에 보고한 칸(움직임 중복 제거, #300)
    mouse_down: u8,
    mouse_last_cell: Option<(usize, usize)>,
    /// 스크립트 마우스(`send mouse ...`, #662)의 상태. 사람의 마우스 상태와 아무것도 나누지 않는다
    // AppleScript(macOS)만 읽는다
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    script_mouse: crate::script_mouse::ScriptMouse,
    /// PTY의 termios로 알아챈 암호 입력(#258). 자물쇠 커서를 그린다
    password: bool,
    /// 명령이 끝나도 칸을 남긴다: Dock에 놓은 파일을 실행한 칸(#279). `wait-after-command`와 같다
    wait_after_exit: bool,
    /// 이 칸의 글자 크기(pt). 칸마다 따로다(#422, Ghostty는 surface마다). 칸 격자·마우스 좌표·커서·스크롤 막대는 이 크기의 칸 크기를 쓴다.
    /// 칸을 다른 창으로 옮겨도(탭 떼어 내기) 그대로다
    pub font_size: f32,
}

impl Pane {
    /// 아직 살아 있는 진행 표시(상태, 퍼센트)
    fn live_progress(&self) -> Option<(u8, u8)> {
        self.progress
            .filter(|(_, _, at)| at.elapsed() < Duration::from_secs(15))
            .map(|(s, p, _)| (s, p))
    }

    /// 이 칸의 프로그램 상태(OSC 7501) 요약. 막대로 그릴 때는 OSC 9;4 진행 표시보다 앞선다
    fn status_summary(&self) -> Option<bt_term::program_status::Summary> {
        self.session.program_status()
    }

    /// 탭 줄·창 제목에 쓸 이름: 사람이 붙인 제목 → 셸이 보낸 제목 → 전경 프로세스 이름. 보지 않은 벨이 있으면 앞에 🔔
    fn display_title(&self) -> String {
        self.display_title_in(None)
    }

    /// 탭 제목을 덮어썼으면(`set_tab_title`, #207) 그것이 칸 제목보다 먼저다
    fn display_title_in(&self, tab_override: Option<&str>) -> String {
        title_of(
            tab_override.or(self.user_title.as_deref()),
            &self.title,
            self.fg_name.as_deref(),
            self.bell,
            self.session.readonly(),
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

/// 화면 반전(DECSCNM, #302)일 때 커서 기본 색. Ghostty는 커서 색을 따로 정하지 않았으면 (맞바뀐) 기본 글자색,
/// 커서 위 글자는 (맞바뀐) 기본 바탕색을 쓴다. 설정(cursor-color·cursor-text)으로 정한 색은 그대로다.
/// 테마에는 정했는지가 남지 않아 기본값(커서 = 글자색, 커서 글자 = 바탕색)과 같은 값이면 정하지 않은 것으로 본다
fn reversed_cursor_defaults(theme: &bt_core::Theme, reversed: bool) -> (bt_core::Rgb, bt_core::Rgb) {
    if !reversed {
        return (theme.cursor, theme.cursor_text);
    }
    let color = if theme.cursor == theme.foreground {
        theme.background
    } else {
        theme.cursor
    };
    let text = if theme.cursor_text == theme.background {
        theme.foreground
    } else {
        theme.cursor_text
    };
    (color, text)
}

/// 화면 반전(DECSCNM) 중 투명 제목 막대 영역을 칠할 색(#302). Ghostty의 제목 막대 색은 `surface.backgroundColor ?? 설정 바탕색`이라
/// reverse_colors를 따르지 않는다(OSC 11로 바꿨으면 그 색). 반전이 아니거나 제목 막대가 터미널 위에 겹치지 않으면(`top_inset` 0) 칠하지 않는다.
/// 돌려주는 값은 (x, y, 너비, 높이, 색)
fn titlebar_fill(
    top_inset: u32,
    width: u32,
    reversed: bool,
    chrome_bg: bt_core::Rgb,
) -> Option<(i32, i32, u32, u32, bt_core::Rgb)> {
    (reversed && top_inset > 0).then_some((0, 0, width, top_inset, chrome_bg))
}

/// 칸 제목(#124): 사람이 붙인 것 → 프로그램이 정한 것 → 전경 프로세스 이름 → "셸". 벨이 있으면 앞에 🔔
fn title_of(user: Option<&str>, program: &str, fg: Option<&str>, bell: bool, readonly: bool) -> String {
    let name = if let Some(u) = user.filter(|u| !u.is_empty()) {
        u.to_string()
    } else if !program.is_empty() {
        program.to_string()
    } else {
        fg.unwrap_or("셸").to_string()
    };
    // 읽기 전용 표시는 벨 표시 뒤, 이름 앞(#209)
    let lock = if readonly { "🔒 " } else { "" };
    if bell {
        format!("🔔 {lock}{name}")
    } else {
        format!("{lock}{name}")
    }
}

/// 사람이 친 제목을 다듬는다: 제어 문자를 지우고 앞뒤 공백을 떼고 200자까지. 비면 없다
/// 창이 지금 가려졌는가(#723). winit은 가림 상태가 바뀔 때만 알리므로 창을 넣을 때 실제 상태로 시작한다(Ghostty `syncSurfaceTreeOcclusionState`)
fn window_occluded(window: &Window) -> bool {
    #[cfg(target_os = "macos")]
    let visible = Some(winit::platform::macos::occlusion_visible(window));
    #[cfg(not(target_os = "macos"))]
    let visible = {
        let _ = window;
        None
    };
    crate::lifecycle::starts_occluded(visible)
}

fn clean_user_title(text: &str) -> Option<String> {
    let t: String = bt_core::proto::printable(text).trim().chars().take(200).collect();
    let t = t.trim_end().to_string();
    (!t.is_empty()).then_some(t)
}

/// 창 제목(#207): 설정 `title`(고정) → `set_window_title`로 덮어쓴 것 → 탭 제목
fn window_title_of(config_title: Option<&str>, window_override: Option<&str>, tab_title: &str) -> String {
    config_title
        .filter(|t| !t.is_empty())
        .or(window_override)
        .unwrap_or(tab_title)
        .to_string()
}

/// `copy_title_to_clipboard`가 복사할 글: 사람이 붙인 칸 제목 → 프로그램이 정한 제목. 둘 다 비면 없다
fn title_to_copy(user: Option<&str>, program: &str) -> Option<String> {
    let text = user.filter(|u| !u.is_empty()).unwrap_or(program);
    let text = bt_core::proto::printable(text);
    (!text.trim().is_empty()).then_some(text)
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
    readonly: bool,
) -> bool {
    use bt_core::config::ConfirmClose;
    // 읽기 전용 칸은 돌고 있는 프로그램이 없어도, 설정이 묻지 않기로 했어도 닫기 전에 늘 묻는다(Ghostty 주석, ADR-018)
    if readonly {
        return true;
    }
    match mode {
        ConfirmClose::False => false,
        _ if exited => false,
        ConfirmClose::Always => true,
        ConfirmClose::True if integrated => running,
        ConfirmClose::True => fg_is_shell != Some(true),
    }
}

/// 줄바꿈이 든 붙여넣기를 물을 글(#109): 글이 든 줄 수와 첫 줄(제어 문자를 지우고 30칸까지).
/// 암호 입력 중(`password`)이면 글은 한 글자도 넣지 않고 줄 수만 보인다(#508)
fn paste_prompt(text: &str, password: bool) -> String {
    let mut lines = text.split(['\n', '\r']).filter(|l| !l.trim().is_empty());
    let first = lines.next().unwrap_or("");
    let n = 1 + lines.count();
    if password {
        return format!("붙여넣을 글({n}줄)의 줄바꿈이 바로 실행될 수 있습니다. 암호 입력 중이라 내용은 숨깁니다");
    }
    let first = bt_render::truncate(bt_core::proto::printable(first).trim(), 30);
    format!("붙여넣을 글({n}줄)의 줄바꿈이 바로 실행될 수 있습니다: {first}")
}

/// 붙여넣기를 요청한 때 암호 입력으로 볼지(Ghostty #15114): 칸에 적어 둔 마지막 살핀 값과 요청 직전에 termios를 다시
/// 읽은 값 중 하나라도 참이면 참. 살핌 주기(200ms) 사이에 켜진 암호 프롬프트를 놓치지 않으려는 것이고, 못 읽으면(`None`)
/// 마지막 살핀 값만 본다
fn paste_hidden(flag: bool, rechecked: Option<bool>) -> bool {
    flag || rechecked == Some(true)
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
    /// 모든 창(`close_all_windows`, #206)
    CloseAllWindows,
    Quit,
    /// 줄바꿈이 든 붙여넣기(#109): 이 칸에 이 글을
    Paste {
        pane: u64,
        text: String,
    },
    /// 프로그램이 OSC 52로 클립보드를 읽으려 한다(`clipboard-read = ask`, #220). 막대에 보인 `text`를 그대로 이 칸에만 답한다
    ClipboardRead {
        pane: u64,
        reply: bt_term::ClipboardReply,
        text: String,
    },
    /// 프로그램이 OSC 52로 클립보드에 쓰려 한다(`clipboard-write = ask`, #220)
    ClipboardWrite {
        pane: u64,
        text: String,
    },
    /// Dock에 놓은 파일을 실행한다(#279): `cwd`(부모 폴더)에서 새 칸을 띄워 `input`을 셸에 친다
    OpenFile {
        cwd: std::path::PathBuf,
        input: String,
    },
    /// 프로그램이 kitty 클립보드 프로토콜(OSC 5522)로 클립보드를 읽으려 한다(`clipboard-read = ask`, #325). 막대에 보인 `text`를 이 칸에만 답한다
    KittyRead {
        pane: u64,
        req: bt_term::kitty_clip::ReadRequest,
        text: String,
    },
    /// 프로그램이 kitty 클립보드 프로토콜로 클립보드에 쓰려 한다(`clipboard-write = ask`, #325)
    KittyWrite {
        pane: u64,
        req: bt_term::kitty_clip::WriteRequest,
        text: String,
    },
}

impl Pending {
    /// 프로그램의 출력이 띄운 막대인가. 이런 막대는 Enter로 허용하지 않는다(ADR-019)
    fn is_program_prompt(&self) -> bool {
        // Dock에 놓은 파일도 다른 프로그램이 `open -a`로 띄울 수 있어(키보드를 치는 중에 막대가 뜬다) 같은 규칙을 쓴다(ADR-026)
        matches!(
            self,
            Pending::ClipboardRead { .. }
                | Pending::ClipboardWrite { .. }
                | Pending::OpenFile { .. }
                | Pending::KittyRead { .. }
                | Pending::KittyWrite { .. }
        )
    }
}

impl Pending {
    /// 이 확인이 `leaves` 중 한 칸에 매여 있는가. 탭을 다른 창으로 옮길 때 그 막대를 닫아야 하는지 가른다(#223).
    /// 창·앱 전체를 묻는 것(창 닫기·모두 닫기·끝내기)은 칸에 매이지 않는다
    fn binds_to(&self, leaves: &[u64]) -> bool {
        match self {
            Pending::ClosePane(id)
            | Pending::CloseTab(id)
            | Pending::Paste { pane: id, .. }
            | Pending::ClipboardRead { pane: id, .. }
            | Pending::ClipboardWrite { pane: id, .. }
            | Pending::KittyRead { pane: id, .. }
            | Pending::KittyWrite { pane: id, .. } => leaves.contains(id),
            Pending::CloseWindow | Pending::CloseAllWindows | Pending::Quit | Pending::OpenFile { .. } => false,
        }
    }
}

/// 창 위에 띄운 확인 막대: Enter·y면 한다, Esc·n이면 그만둔다
struct Confirm {
    pending: Pending,
    message: String,
    /// "닫기", "끝내기"
    yes: &'static str,
    /// 막대를 띄운 때(클립보드 막대는 이로부터 잠깐 허용 키를 받지 않는다)
    shown: Instant,
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
    /// 칸 둘레에 노란 테두리(보고 있어도 켠다, #687)
    border: bool,
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
        border: f.border,
        attention: f.attention && !window_focused,
        beep: f.system,
        audio: f.audio && has_audio_file,
    }
}

/// 벨 테두리를 지울지(#687): 창이 앞이고 그 칸이 키를 받는 칸이 될 때(Ghostty `focusDidChange`).
/// 키가 그 칸에 갈 때 지우는 것은 따로 `write_pane`이 한다(`keyDown`)
fn bell_border_cleared_on_focus(window_focused: bool, pane_is_focused: bool) -> bool {
    window_focused && pane_is_focused
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
    /// `prompt_tab_title`·`set_tab_title`로 덮어쓴 탭 제목(#207). 칸 제목보다 먼저다
    title: Option<String>,
}

/// 제목 입력 막대가 정할 곳(#207)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TitleTarget {
    /// 그 칸(id)
    Surface(u64),
    /// 그 칸이 든 탭
    Tab(u64),
    Window,
}

impl TitleTarget {
    fn label(self) -> &'static str {
        match self {
            TitleTarget::Surface(_) => "제목 › ",
            TitleTarget::Tab(_) => "탭 제목 › ",
            TitleTarget::Window => "창 제목 › ",
        }
    }
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
        title: Option<String>,
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

/// 찾기 막대에 넣을 글: 첫 줄만, 제어 문자를 지우고, 256자까지
fn search_text(raw: &str) -> String {
    raw.lines()
        .next()
        .unwrap_or("")
        .chars()
        .filter(|c| !c.is_control())
        .take(256)
        .collect()
}

/// 그릴 칸 하나(다시 그릴 때 잠깐 쓴다)
struct PaneView {
    id: u64,
    outer: Rect,
    inner: Rect,
    screen: Screen,
    focused: bool,
    progress: Option<(u8, u8)>,
    /// 벨 테두리를 두르는가(`bell-features = border`, #687)
    bell_border: bool,
    /// 이 칸의 글자 크기(pt)와 그 크기의 칸 크기: 격자·커서·그림·스크롤 막대가 이 칸 크기를 쓴다(#422)
    font_size: f32,
    metrics: bt_render::font::CellMetrics,
}

/// 창에 사용자 셰이더를 건다(#376). GPU 길이면 GLSL 오류는 곧바로 돌려주고 파이프라인은 다른 스레드에서 만든다(#381):
/// 그때 나는 드문 오류는 결과가 올 때 `poll_shaders`가 받는다. CPU 길이면 쓰지 못한다는 문구를 돌려준다
/// (둘 다 로그에도 남는다. GPU 쪽 컴파일 경고는 렌더러가 남긴다)
fn apply_shaders(g: &mut Gfx, shaders: &[bt_gpu::CustomShaderSource], wake: &bt_gpu::ShaderWaker) -> Vec<String> {
    match g.gpu.as_mut() {
        Some(gpu) => {
            gpu.set_custom_shader_waker(Some(wake.clone()));
            gpu.set_custom_shaders(shaders)
        }
        None if shaders.is_empty() => Vec::new(),
        None => {
            let e =
                "CPU 길(BT_GPU=0, 또는 GPU 길이 맞추지 못하는 설정)에서는 사용자 셰이더를 쓰지 않습니다".to_string();
            log::warn!("custom-shader: {e}");
            vec![e]
        }
    }
}

struct Gfx {
    /// 창을 만들 때 정해진 제목 막대 밑 비움 높이(논리 점, #192). 제목 막대 모양은 새 창에만 적용되므로 창마다 따로 든다
    inset_points: f32,
    window: Arc<Window>,
    /// CPU 길의 올릴 곳. GPU 길이면 없다: 한 창에는 한 가지 surface만 붙일 수 있다
    surface: Option<softbuffer::Surface<Arc<Window>, Arc<Window>>>,
    _context: Option<softbuffer::Context<Arc<Window>>>,
    /// GPU 길(ADR-036, macOS 기본 ADR-039): 창 surface에 층과 격자를 합쳐 올리는 렌더러
    gpu: Option<Box<bt_gpu::GpuRenderer>>,
    /// 창의 첫 프레임을 올리면 한 번 로그에 남긴다(실험 E10)
    first: FirstFrame,
    /// 접근성 트리(VoiceOver, #560)
    a11y: crate::a11y::WindowA11y,
}

/// `from`에서 `to`까지의 ms. `to`가 앞이면 음수(첫 프레임 로그, 실험 E12)
fn since_ms(from: Instant, to: Instant) -> f64 {
    let ms = |d: Duration| d.as_secs_f64() * 1000.0;
    if to >= from { ms(to - from) } else { -ms(from - to) }
}

/// 창을 만든 때, 첫 프레임을 올린 때, macOS가 창이 보이게 됐다고 처음 알린 때(실험 E10).
/// 둘 중 나중에 오는 쪽에서 한 번 로그에 남긴다
struct FirstFrame {
    made: Instant,
    /// 아직 첫 프레임을 올리지 못했을 때 마지막으로 다시 그리기를 시작한 때(첫 프레임이 언제 시작됐는지 남긴다)
    began: Option<Instant>,
    presented: Option<Instant>,
    visible: Option<Instant>,
}

impl FirstFrame {
    fn new(made: Instant) -> Self {
        Self {
            made,
            began: None,
            presented: None,
            visible: None,
        }
    }

    /// 다시 그리기를 시작한다. 첫 프레임을 올리기 전까지는 그때를 적어 둔다
    fn began(&mut self) {
        if self.presented.is_none() {
            self.began = Some(Instant::now());
        }
    }

    /// 프레임 하나를 창에 올렸다. 처음이면 창을 만든 뒤·앱을 띄운 뒤(·창이 보인 뒤) 걸린 시간을 남긴다
    fn presented(&mut self, path: &str) {
        if self.presented.is_some() {
            return;
        }
        let now = Instant::now();
        self.presented = Some(now);
        let ms = |d: std::time::Duration| d.as_secs_f64() * 1000.0;
        let since_start = crate::STARTED.get().map_or(0.0, |t| ms(now - *t));
        let after_visible = self.visible.map_or_else(
            || "창이 보이기 전".to_string(),
            |v| {
                // 그 프레임의 다시 그리기를 시작한 때(창이 보인 때 기준, 앞이면 음수)
                let began = self
                    .began
                    .map_or(String::new(), |b| format!("(그 프레임 시작 {:+.1}ms)", since_ms(v, b)));
                format!("창이 보인 뒤 {:.1}ms{began}", ms(now - v))
            },
        );
        log::info!(
            "첫 프레임({path}): 창을 만든 뒤 {:.1}ms, 앱을 띄운 뒤 {since_start:.1}ms, {after_visible}",
            ms(now - self.made)
        );
    }

    /// 창이 보이게 됐다는 알림(`WindowEvent::Occluded(false)`). 첫 프레임을 먼저 올렸으면 그 간격을 남긴다
    fn became_visible(&mut self) {
        if self.visible.is_some() {
            return;
        }
        let now = Instant::now();
        self.visible = Some(now);
        if let Some(p) = self.presented {
            log::info!(
                "창이 보임: 첫 프레임을 올리고 {:.1}ms 뒤",
                (now - p).as_secs_f64() * 1000.0
            );
        }
    }
}

/// 창 하나의 상태(#36). 칸(셸)은 모든 창이 App에서 함께 든다
struct Win {
    /// 이 창의 글꼴 묶음: 글자 크기마다 한 벌(#422). 칸이 쓰는 크기들과 창의 기준 크기를 든다. 같은 크기의 칸은 한 벌을 나눠 쓴다.
    /// 글꼴 파일 자료는 창끼리도 나눠 쓰고 글리프 캐시만 묶음마다 든다
    fonts: SizedFonts<Fonts>,
    /// 창의 기준 글자 크기(pt, #416·#422): 탭 줄·알림 띠·확인 막대·찾기 막대 같은 창 덧그림과 창 크기 계산(`window-step-resize`·
    /// `window-width`)이 쓴다. 이 창에서 가장 최근에 ⌘+·⌘-·⌘0·`set_font_size`로 바꾼 크기이고, 처음에는 새 창의 크기다.
    /// 칸 안은 칸마다 `Pane::font_size`다
    font_size: f32,
    /// 탭 줄 제목 글꼴(#241 `window-title-font-family`). 키가 없거나 글꼴을 못 찾았으면 없고, 그러면 터미널 글꼴로 그린다
    title_fonts: Option<Fonts>,
    /// 창을 만든 차례(`goto_window`가 이 차례로 돈다, #206). 바꿔 끼워도 변하지 않는다
    serial: u64,
    gfx: Option<Gfx>,
    /// 항상 위에 띄우는 중(`toggle_window_float_on_top`, #148)
    float_on_top: bool,
    tabs: Vec<TabGroup>,
    /// 보고 있는 탭(tabs의 자리)
    active: usize,
    preedit: String,
    cursor_pos: PhysicalPosition<f64>,
    /// 마우스가 오른쪽 끝(스크롤 막대 자리) 위에 있는 칸
    gutter_hot: Option<u64>,
    /// 스크롤 막대를 누르고 있는 중(손잡이를 끌거나 빈 곳을 눌러 쪽 단위로 옮긴 누름). 놓을 때까지 마우스는
    /// 프로그램에 보고하지 않고 글도 고르지 않는다
    scrollbar_press: Option<ScrollbarPress>,
    /// 칸 끌기 손잡이를 누르고 끄는 중(`drag-handle`). 끄는 동안 마우스는 프로그램에 보고하지 않고 글도 고르지 않는다
    pane_drag: crate::pane_drag::PaneDrag,
    /// 분할 경계선을 끄는 중과 지난 누름(두 번 누르면 칸 크기를 고르게). 끄는 동안 마우스는 프로그램에 보고하지 않고 글도 고르지 않는다
    divider: crate::divider_drag::DividerState,
    /// 탭 줄에서 왼쪽 단추를 누른 뒤 끄는 중(탭 순서 바꾸기). 끄는 동안 다른 누름은 아무 데도 가지 않는다
    tab_drag: tabbar::TabDrag,
    /// 앞 탭 줄 왼쪽 누름(시각, 탭 번호). 같은 탭을 시스템 두 번 누르기 간격 안에 또 누르면 제목 입력 막대를 연다
    tab_click: Option<(Instant, usize)>,
    /// 마우스가 위쪽 띠 안에 있는 칸과 마우스가 손잡이 위인지(`drag-handle`)
    handle_hover: Option<(u64, bool)>,
    selecting: bool,
    /// 두 번 누르기로 링크 전체를 고른 칸(#701, `PtrState::link_word`)
    link_word: Option<(usize, usize)>,
    last_click: Option<(Instant, usize, usize, u8)>,
    scroll_accum: f64,
    /// 가로 휠 누적(마우스 보고용, #300)
    scroll_accum_x: f64,
    focused: bool,
    /// 창 안 알림 띠(글, 띄운 때). 2.5초 보인다
    toast: Option<(String, Instant)>,
    /// Cmd를 누른 채 마우스가 올라간 링크(칸 id, 링크)
    hover: Option<(u64, bt_term::links::LinkHit)>,
    /// 링크 수식 키 누르기의 진행(#705, `mouse::LinkPress`)
    link_click: crate::mouse::LinkPress,
    /// 고르지 않은 칸을 누른 왼쪽 누름을 삼켰다: 짝 떼기도 삼킨다(#500)
    focus_click: mouse::FocusClickGuard,
    /// 누름을 프로그램(PTY)에 보낸 물리 키와 그 칸. 프로그램이 kitty 키보드 프로토콜 2(키 뗌 보고)를 켰을 때, 같은 칸에 짝 뗌만 보낸다.
    /// IME가 삼킨 누름, 앱 단축키가 먹은 누름, 누른 뒤 다른 칸을 고른 경우의 뗌을 홀로 보내지 않기 위해서다(#546)
    kitty_down: std::collections::HashMap<winit::keyboard::PhysicalKey, u64>,
    /// 열린 찾기 막대(칸 하나에 붙는다)
    search: Option<SearchBar>,
    /// 열린 명령 팔레트(#77): 친 글, 고른 줄
    palette: Option<(String, Option<usize>)>,
    /// 창에 마지막으로 적용한 마우스 포인터 모양(#239). 같으면 다시 적용하지 않는다
    pointer_icon: CursorIcon,
    /// 입력 중 마우스 포인터를 숨긴 상태(#250). 숨긴 동안은 Shift가 모양을 바꾸지 않는다
    pointer_hidden: bool,
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
    /// AppleScript `send key`의 키 순서와 그 칸(#650). 사람의 `keyseq`와 섞이지 않게 따로 둔다
    script_keyseq: Option<(u64, crate::keymap::Sequencer)>,
    /// 닫기·끝내기 확인 막대(#108)
    confirm: Option<Confirm>,
    /// 칸 제목 입력 막대(#124): 그 칸과 친 글
    title_prompt: Option<(TitleTarget, String)>,
    /// `set_window_title`로 덮어쓴 창 제목(#207). 설정 `title`이 있으면 그것이 먼저
    window_title: Option<String>,
    /// 배경 그림 층(#134): (창 폭, 높이, 기본 바탕색)과 미리 합성한 픽셀
    backdrop: Option<backdrop::Backdrop>,
    /// 바뀐 것이 없어도 다음 그리기에서 창에 한 번 올린다(창이 다른 화면으로 옮겨 가면 그 화면의 색 공간으로, #97)
    force_present: bool,
    /// 들고 있는 프레임과 그 내용(바뀐 줄만 다시 그리려고, #40)
    frame: Vec<u32>,
    /// GPU 길의 위층(격자 위에 겹치는 것, 곱한 알파). 아래층은 `frame`이 든다. 그린 것이 없으면 비워 둔다(#387)
    upper: Vec<u32>,
    /// 칸 바탕과 글자 사이 층(kitty 글자 아래 그림, #547)
    mid: Vec<u32>,
    /// GPU 길: 셰이더 뒤에 얹는 층의 픽셀(셰이더가 돌 때만 쓴다, #441)
    top: Vec<u32>,
    last_frame: Option<FrameMemo>,
    /// 커서 깜빡임: 지금 보이는가, 다음에 바꿀 때, 깜빡일 커서가 있는가
    blink_on: bool,
    blink_next: Instant,
    blink_wanted: bool,
    /// 이 창의 바탕이 `toggle_background_opacity`로 불투명해졌다(#209)
    opaque: bool,
    /// 제목 막대 폴더 아이콘에 마지막으로 올린 폴더(#279). 같으면 다시 올리지 않는다
    proxy_icon: Option<std::path::PathBuf>,
    /// 창이 다른 창에 가려졌거나 최소화됐다(`WindowEvent::Occluded`, #303). 알리기 전에는 보인다고 본다
    occluded: bool,
    /// kitty 애니메이션(#554): 보이는 그림의 다음 프레임을 그릴 때. 그릴 때마다 새로 정한다
    kitty_wake: Option<Instant>,
    /// GPU 길(ADR-038): 그림판 얻기가 막힐 것 같아 미룬 프레임을 다시 그릴 때
    pace_wake: Option<Instant>,
    /// 사용자 셰이더(#376): 시간·커서·초점과 애니메이션 깨우기. 셰이더를 다시 걸면 새로 만든다
    shader: shader_inputs::ShaderState,
    /// GPU 길: 앞 프레임의 층 입력. 같으면 층을 그리지도 견주지도 않는다
    layer_key: Option<gpu_pace::LayerKey>,
    /// GPU 길 로그에 남기는 누적 수(ADR-038): 미룬 프레임, 가려져 건너뛴 프레임, 층 견주기를 건너뛴 프레임
    pace_stats: gpu_pace::PaceStats,
}

impl Win {
    fn new(font_size: f32) -> Self {
        static NEXT_SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            serial: NEXT_SERIAL.fetch_add(1, Ordering::Relaxed),
            fonts: SizedFonts::default(),
            font_size,
            title_fonts: None,
            gfx: None,
            float_on_top: false,
            tabs: Vec::new(),
            active: 0,
            preedit: String::new(),
            cursor_pos: PhysicalPosition::new(0.0, 0.0),
            gutter_hot: None,
            scrollbar_press: None,
            pane_drag: crate::pane_drag::PaneDrag::default(),
            divider: crate::divider_drag::DividerState::default(),
            tab_drag: tabbar::TabDrag::default(),
            tab_click: None,
            handle_hover: None,
            selecting: false,
            link_word: None,
            last_click: None,
            scroll_accum: 0.0,
            scroll_accum_x: 0.0,
            // 포커스 사건이 오기 전에는 보고 있지 않은 창으로 둔다. 보고 있다고 잘못 알면
            // 그 창의 OSC 52 클립보드 쓰기를 받게 된다(보안 검토, #36)
            focused: false,
            toast: None,
            hover: None,
            link_click: crate::mouse::LinkPress::Idle,
            focus_click: mouse::FocusClickGuard::default(),
            kitty_down: std::collections::HashMap::new(),
            search: None,
            palette: None,
            pointer_icon: CursorIcon::Default,
            pointer_hidden: false,
            quick: false,
            quick_shown: None,
            quick_place: None,
            quick_anim: None,
            keyseq: Default::default(),
            script_keyseq: None,
            confirm: None,
            title_prompt: None,
            window_title: None,
            backdrop: None,
            force_present: false,
            frame: Vec::new(),
            upper: Vec::new(),
            mid: Vec::new(),
            top: Vec::new(),
            last_frame: None,
            blink_on: true,
            blink_next: Instant::now(),
            blink_wanted: false,
            opaque: false,
            proxy_icon: None,
            occluded: false,
            kitty_wake: None,
            pace_wake: None,
            shader: Default::default(),
            layer_key: None,
            pace_stats: Default::default(),
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
    scale: f64,
    /// 모든 탭의 칸들(탭 순서와 상관없다)
    panes: Vec<Pane>,
    next_id: u64,
    /// `key-remap`을 적용한 수정 키 상태(키 묶음·PTY 인코딩·마우스가 쓴다, #196)
    mods: ModifiersState,
    /// 바꾸기 전, 좌우를 나눈 수정 키 상태
    raw_mods: bt_core::config::SidedMods,
    clipboard: Option<arboard::Clipboard>,
    /// 선택 클립보드(`copy-on-select = primary`·`both`). 앱 안 저장소라 창·탭·칸이 나눠 쓰고 다른 프로세스와는 나누지 않는다
    selection_clipboard: Option<String>,
    /// 이 앱이 마지막으로 선택 클립보드에 쓸 때 본 이름 붙은 pasteboard의 바뀜 번호(macOS, #629)
    selection_seen: Option<isize>,
    /// 시스템 설정의 스크롤 막대(앱이 앞으로 올 때마다 다시 읽는다, #630)
    system_scroller: SystemScroller,
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
    /// 같은 이벤트 고리 차례에 온 `DroppedFile`을 모은다. `about_to_wait`에서 한 번에 붙여넣는다(#566)
    drop_batch: crate::file_drop::DropBatch<WindowId>,
    /// btctl wait가 기다리는 글(#66)
    text_waiters: Vec<TextWaiter>,
    /// 메뉴 막대(살려 두어야 메뉴가 남는다)
    #[cfg(target_os = "macos")]
    menu: Option<muda::Menu>,
    control: Option<crate::control::ControlServer>,
    keymap: Keymap,
    /// --config-file(설정을 다시 읽을 때 같은 파일을 읽는다)
    config_extra: Option<std::path::PathBuf>,
    /// `--config-default-files`(거짓이면 설정을 다시 읽을 때도 기본 설정 파일을 읽지 않는다)
    config_default_files: bool,
    /// 지금 창(사건을 받은 창)의 상태. 다른 창의 사건이 오면 `others`와 바꿔 끼운다(#36)
    w: Win,
    others: Vec<Win>,
    /// 마지막으로 보던 창(창 밖에서 온 btctl 요청이 쓴다)
    last_focused: Option<WindowId>,
    /// 스크립트 `with configuration`(#666)이 새 칸 하나에만 덮어쓸 값. `run_script`가 칸을 만드는 호출 직전에 넣고, `handle_call`의
    /// New(프로그램·폴더)와 `spawn_pane`(환경 변수·처음 입력)이 쓰며, 호출이 끝나면 쓰였든 아니든 비운다. 설정 파일의 값은 바꾸지 않는다
    spawn_overrides: Option<crate::script_config::SpawnOverrides>,
    /// 전역 단축키(#95): 등록한 관리자와 단축키 번호 → 동작
    hotkeys: Option<global_hotkey::GlobalHotKeyManager>,
    /// 마지막 경고음(벨이 몰려도 0.1초에 한 번만, #107)
    last_beep: Option<Instant>,
    /// write_*_file로 쓴 파일들. 앱이 끝날 때 지운다(#125)
    written_files: Vec<std::path::PathBuf>,
    /// 풀어 둔 배경 그림(background-image, #134)
    bg_image: Option<Arc<bt_render::background::BgImage>>,
    /// 읽어 둔 사용자 셰이더(`custom-shader`, #376). GPU 길 창마다 장치가 따로라 창을 만들 때 각자 컴파일한다
    shaders: Vec<bt_gpu::CustomShaderSource>,
    hotkey_registered: Vec<global_hotkey::hotkey::HotKey>,
    hotkey_actions: std::collections::HashMap<u32, Action>,
    /// 닫고 들고 있는 칸·탭(`undo-timeout`, #164)과 되살린 것을 다시 닫는 일(`redo`)
    undo: UndoStack<Closed>,
    redo: UndoStack<Redo>,
    /// 마지막 창이 닫힌 뒤 앱을 끝낼 때(`quit-after-last-window-closed-delay`, #178). 새 창이 열리면 지운다
    quit_deadline: Option<Instant>,
    /// `set_window_theme`으로 이번 실행 동안 고른 창 테마(#209). 없으면 설정의 `window-theme`. 설정을 다시 읽어도 남는다
    theme_override: Option<bt_core::config::WindowTheme>,
    /// 지금 깔린 색 테마를 고른 시스템 모양(`theme = light:…,dark:…`, #221). 모양 사건이 창마다 와도 한 번만 처리하려고 든다
    applied_dark: bool,
    /// 칸에 마지막으로 알린 시스템 모양(`CSI ? 996 n`의 답, 모드 2031 알림, #232)
    reported_dark: bool,
    /// 보안 키보드 입력 상태(#258)
    secure: crate::secure_input::SecureInput,
    /// 다음에 칸의 암호 입력을 살필 때(200ms마다, 앱이 앞이고 창에 초점이 있을 때만)
    next_secure_poll: Instant,
    /// 앱 메뉴의 보안 키보드 입력 항목(체크 표시를 맞춘다)
    #[cfg(target_os = "macos")]
    menu_secure: Option<muda::CheckMenuItem>,
}

impl App {
    pub fn new(
        config: AppConfig,
        (config_extra, config_default_files): (Option<std::path::PathBuf>, bool),
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
            scale: 1.0,
            panes: Vec::new(),
            next_id: 1,
            mods: ModifiersState::empty(),
            raw_mods: Default::default(),
            clipboard: arboard::Clipboard::new().ok(),
            selection_clipboard: None,
            selection_seen: None,
            system_scroller: SystemScroller::read(),
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
            config_default_files,
            w: Win::new(font_size),
            others: Vec::new(),
            last_focused: None,
            spawn_overrides: None,
            hotkeys: None,
            last_beep: None,
            bell_sound: None,
            last_bell_audio: None,
            drop_batch: Default::default(),
            written_files: Vec::new(),
            bg_image: None,
            shaders: Vec::new(),
            hotkey_registered: Vec::new(),
            hotkey_actions: std::collections::HashMap::new(),
            undo: UndoStack::default(),
            redo: UndoStack::default(),
            quit_deadline: None,
            theme_override: None,
            applied_dark: crate::system_is_dark(),
            reported_dark: crate::system_is_dark(),
            secure: crate::secure_input::SecureInput::new(
                Box::new(crate::secure_input::system_call),
                crate::secure_input::global_path().is_some_and(|p| crate::secure_input::load_global(&p)),
            ),
            next_secure_poll: Instant::now(),
            #[cfg(target_os = "macos")]
            menu_secure: None,
        }
    }

    /// 이 창의 글꼴 묶음을 모두 새로 만든다(설정·화면 배율이 바뀌었을 때, 창을 처음 열 때). 글자 크기마다 한 벌이다
    fn load_fonts(&mut self) {
        self.w.fonts.clear();
        self.w.title_fonts = None;
        self.sync_fonts_with(true);
    }

    /// 창의 칸들이 쓰는 글자 크기에 글꼴 묶음을 맞춘다(#422): 이미 있는 크기는 그대로 쓰고, 없는 크기만 만들고,
    /// 쓰지 않는 크기는 버린다. 칸마다 크기가 달라도 같은 크기끼리는 한 벌을 나눠 쓴다
    fn sync_fonts(&mut self) {
        self.sync_fonts_with(false);
    }

    /// `sync_fonts`. `rebuild_title`이면(`load_fonts`: 설정·배율이 바뀌었다) 제목 글꼴도 다시 만든다. 제목 글꼴은 터미널 글꼴
    /// 크기를 따르지 않으므로(#474) 기준 크기가 바뀌어도 다시 만들지 않는다
    fn sync_fonts_with(&mut self, rebuild_title: bool) {
        let base = self.w.font_size;
        let sizes = pane_font::needed_sizes(base, self.window_pane_sizes());
        let (config, scale) = (&self.config, self.scale as f32);
        let mut title: Option<Option<Fonts>> = None;
        let mut warnings: Vec<String> = Vec::new();
        self.w.fonts.sync(base, &sizes, |size| {
            let want_title = rebuild_title && pane_font::size_key(size) == pane_font::size_key(base);
            let (terminal, t, warn) = build_fonts(config, scale, size, want_title)?;
            warnings.extend(warn);
            if want_title {
                title = Some(t);
            }
            Some(terminal)
        });
        for w in warnings {
            // 같은 경고를 글꼴을 다시 읽을 때마다 쌓지 않는다
            if !self.config.warnings.contains(&w) {
                self.config.warnings.push(w);
            }
        }
        if self.w.fonts.chrome().is_none() {
            // 기준 글꼴을 읽지 못했다: 창은 글을 그리지 못한다(처음 창을 여는 쪽이 앱을 끝낸다)
            self.w.fonts.clear();
            self.w.title_fonts = None;
        } else if let Some(t) = title {
            self.w.title_fonts = t;
        }
    }

    /// 크기 `size`의 글꼴 묶음이 없으면 만든다(새 칸의 격자 크기를 재기 전에 부른다). 만들지 못하면 기준 크기를 쓴다
    fn ensure_font_size(&mut self, size: f32) {
        let (config, scale, base) = (&self.config, self.scale as f32, self.w.font_size);
        self.w
            .fonts
            .ensure(size, base, |s| build_fonts(config, scale, s, false).map(|(f, _, _)| f));
    }

    /// 창이 든 칸들의 글자 크기(탭·분할 차례)
    fn window_pane_sizes(&self) -> Vec<f32> {
        self.w
            .tabs
            .iter()
            .flat_map(|g| g.layout.leaves())
            .filter_map(|id| self.pane_index(id).map(|i| self.panes[i].font_size))
            .collect()
    }

    /// 창 덧그림(탭 줄 등)이 쓰는 기준 크기의 칸 크기
    fn chrome_metrics(&self) -> Option<bt_render::font::CellMetrics> {
        self.w.fonts.chrome().map(|f| f.metrics())
    }

    /// 칸의 글자 크기(pt)
    fn pane_font_size(&self, id: u64) -> Option<f32> {
        self.pane_index(id).map(|i| self.panes[i].font_size)
    }

    /// 글자 크기 `size`의 칸 크기. 그 크기의 묶음이 없으면 기준 크기의 것
    fn metrics_at(&self, size: f32) -> Option<bt_render::font::CellMetrics> {
        self.w
            .fonts
            .get(size)
            .or_else(|| self.w.fonts.chrome())
            .map(|f| f.metrics())
    }

    /// 칸의 칸 크기(그 칸의 글자 크기)
    fn pane_metrics(&self, id: u64) -> Option<bt_render::font::CellMetrics> {
        self.metrics_at(self.pane_font_size(id)?)
    }

    /// 칸의 격자 자리(바깥 사각형에서 여백을 뺀 곳)
    fn pane_inner(&self, id: u64) -> Option<Rect> {
        Some(self.inner_rect(self.pane_rect(id)?, self.pane_font_size(id)?))
    }

    /// 탭 줄 아래 칸들이 차지하는 자리. 여백은 칸마다 따로 뺀다(Ghostty처럼)
    fn content_rect(&self) -> Rect {
        let Some(g) = &self.w.gfx else { return Rect::default() };
        let size = g.window.inner_size();
        let top = self.top_inset() + self.tabbar_height();
        Rect::new(0, top as i32, size.width, size.height.saturating_sub(top))
    }

    /// 칸의 바깥 사각형에서 여백을 뺀 격자 자리. `font_size`는 그 칸의 글자 크기다(남는 자리를 고르게 나눌 때 그 크기의 칸 크기를 쓴다)
    fn inner_rect(&self, r: Rect, font_size: f32) -> Rect {
        let px = |pt: f32| (pt as f64 * self.scale).round() as i32;
        let (left, right) = (px(self.config.padding_x.0), px(self.config.padding_x.1));
        let (top, bottom) = (px(self.config.padding_y.0), px(self.config.padding_y.1));
        // 남는 자리 나누기(window-padding-balance, #136·#675)
        let (cw, ch) = self
            .metrics_at(font_size)
            .map(|m| (m.width, m.height))
            .unwrap_or((0, 0));
        crate::padding::balance_in(r, (left, right, top, bottom), cw, ch, self.config.padding_balance)
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

    /// 마우스가 오른쪽 끝(스크롤 막대 자리) 위에 있는 칸
    fn gutter_pane(&self) -> Option<u64> {
        if self.config.scrollbar == bt_core::config::Scrollbar::Never {
            return None;
        }
        let gutter = (bt_render::scrollbar::BAR_WIDTH as f64 * self.scale * 2.0).round() as u32;
        let (x, y) = (self.w.cursor_pos.x, self.w.cursor_pos.y);
        self.group_layout(self.w.active).into_iter().find_map(|(id, r)| {
            bt_render::scrollbar::in_gutter(x, y, r.x, r.y, r.right(), r.bottom(), gutter).then_some(id)
        })
    }

    /// 칸의 스크롤 막대 트랙: (맨 위 픽셀 y, 길이 픽셀). 막대를 그리는 곳과 같다(칸 안쪽 맨 위에서 보이는 줄 수만큼)
    fn scrollbar_track(&self, id: u64, rows: usize) -> Option<(i32, u32)> {
        let m = self.pane_metrics(id)?;
        let inner = self.pane_inner(id)?;
        Some((inner.y, rows as u32 * m.height))
    }

    /// 마우스가 막대 띠 위에 있을 때 누름을 처리한다. 처리했으면 참(프로그램 보고·고르기를 하지 않는다).
    /// 스크롤백이 없거나 대체 화면이면 막대가 없으므로 거짓이고 마우스는 평소처럼 간다
    fn scrollbar_press_start(&mut self) -> bool {
        let Some(id) = self.gutter_pane() else { return false };
        let Some(idx) = self.pane_index(id) else { return false };
        let session = &self.panes[idx].session;
        if session.modes().alt_screen {
            return false;
        }
        let (rows, history, offset) = session.scroll_metrics();
        let Some((top, track)) = self.scrollbar_track(id, rows) else {
            return false;
        };
        let y = self.w.cursor_pos.y - top as f64;
        let Some(hit) = bt_render::scrollbar::hit(track, rows, history, offset, y) else {
            return false;
        };
        let grab = match hit {
            bt_render::scrollbar::Hit::Thumb { grab } => Some(grab),
            // 시스템 설정 "누른 자리로 이동"이면 그 자리로 뛰고, 이어서 끌 수 있다(손잡이 가운데를 쥔 것으로 본다)
            bt_render::scrollbar::Hit::Page { .. } if self.system_scroller.jump => {
                match bt_render::scrollbar::jump_top(track, rows, history, y) {
                    Some((row, grab)) => {
                        self.panes[idx].session.scroll_to_row(row);
                        Some(grab)
                    }
                    None => None,
                }
            }
            bt_render::scrollbar::Hit::Page { up } => {
                let n = bt_render::scrollbar::page_lines(rows) as i32;
                self.panes[idx].session.scroll(if up { n } else { -n });
                None
            }
        };
        self.w.scrollbar_press = Some(ScrollbarPress { pane: id, grab });
        self.panes[idx].scrollbar_until = Some(Instant::now() + SCROLLBAR_FOR);
        self.request_redraw();
        true
    }

    /// 손잡이를 끄는 중 포인터가 움직였다
    fn scrollbar_drag(&mut self, press: ScrollbarPress) {
        let Some(grab) = press.grab else { return };
        let Some(idx) = self.pane_index(press.pane) else { return };
        let (rows, history, _) = self.panes[idx].session.scroll_metrics();
        let Some((top, track)) = self.scrollbar_track(press.pane, rows) else {
            return;
        };
        let y = self.w.cursor_pos.y - top as f64;
        if let Some(row) = bt_render::scrollbar::drag_top(track, rows, history, y, grab) {
            self.panes[idx].session.scroll_to_row(row);
        }
        self.panes[idx].scrollbar_until = Some(Instant::now() + SCROLLBAR_FOR);
        self.request_redraw();
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

    /// 제목 막대 밑으로 콘텐츠가 들어가는 창에서 탭 줄·격자가 비켜야 할 위쪽 높이(픽셀, #192).
    /// 전체 화면이거나 꾸밈을 껐으면 제목 막대가 없어 0
    fn top_inset(&self) -> u32 {
        let Some(g) = &self.w.gfx else { return 0 };
        #[cfg(target_os = "macos")]
        let simple = {
            use winit::platform::macos::WindowExtMacOS;
            g.window.simple_fullscreen()
        };
        #[cfg(not(target_os = "macos"))]
        let simple = false;
        crate::windeco::top_inset_px(
            g.inset_points,
            self.scale,
            g.window.is_decorated(),
            g.window.fullscreen().is_some() || simple,
        )
    }

    /// 탭 줄 높이. 탭이 하나면 없다(Ghostty처럼)
    fn tabbar_height(&self) -> u32 {
        if self.config.window_show_tab_bar.visible(self.w.tabs.len()) {
            self.tabbar_px()
        } else {
            0
        }
    }

    /// 탭 줄이 보일 때의 높이(#474: 제목 글꼴이 있으면 그 기준). 글꼴을 읽지 못했으면 0
    fn tabbar_px(&self) -> u32 {
        self.w
            .fonts
            .chrome()
            .map_or(0, |term| tabbar::bar_height(term, self.w.title_fonts.as_ref()))
    }

    /// 고른 칸이 명령이 끝나 남겨 둔 칸이면 닫고 참(wait-after-command, #178·#715)
    fn close_held_pane(&mut self, event_loop: &ActiveEventLoop) -> bool {
        let held = self
            .focused_id()
            .and_then(|id| self.pane_index(id))
            .filter(|&i| self.panes[i].held_after_exit);
        let Some(i) = held else { return false };
        self.close_pane(i, event_loop, false);
        self.request_redraw();
        true
    }

    /// 확정 글을 알맞은 곳에 보낸다: 확인 막대면 버리고, 제목 입력·팔레트·찾기 막대가 열려 있으면 거기로, 아니면 고른 칸으로.
    /// `Ime::Commit`과 포커스를 잃을 때 확정하는 조합 글자(#502)가 같은 길을 쓴다
    fn commit_text(&mut self, text: &str) {
        self.w.preedit.clear();
        self.reset_blink();
        // 확인 막대가 떠 있으면 글을 어디에도 보내지 않는다(Enter·Esc만 받는다)
        if self.w.confirm.is_some() {
            self.request_redraw();
            return;
        }
        // 팔레트·찾기 막대가 열려 있으면 확정 글은 거기로
        if let Some((_, t)) = self.w.title_prompt.as_mut() {
            t.push_str(text);
            self.request_redraw();
        } else if let Some((q, sel)) = self.w.palette.as_mut() {
            q.push_str(text);
            *sel = crate::palette::selection_after_edit(q, *sel);
            self.request_redraw();
        } else if self.w.search.is_some() {
            self.search_push(text);
        } else {
            self.write_active(text.as_bytes());
        }
    }

    /// 조합 중인 글자가 있고 입력기가 한국어면 지금(옛) 칸에 확정하고 입력기가 조합을 버리게 한다(#502, Ghostty #15137).
    /// 창이 포커스를 잃을 때와 고른 칸·탭이 바뀌기 전에 부른다. 칸이 바뀌고 나면 조합 글자가 새 칸으로 따라간다.
    /// macOS 밖에서는 아무것도 하지 않는다
    fn settle_preedit(&mut self) {
        #[cfg(target_os = "macos")]
        {
            if self.w.preedit.is_empty() {
                return;
            }
            let Some(window) = self.w.gfx.as_ref().map(|g| g.window.clone()) else {
                return;
            };
            let source = winit::platform::macos::selected_keyboard_input_source(&window);
            if !ime_commits_on_focus_loss(source.as_deref()) {
                return;
            }
            let text = std::mem::take(&mut self.w.preedit);
            self.commit_text(&text);
            winit::platform::macos::discard_marked_text(&window);
            self.request_redraw();
        }
    }

    fn activate(&mut self, index: usize) {
        if index < self.w.tabs.len() && index != self.w.active {
            self.settle_preedit();
            let before = self.focused_id();
            self.w.active = index;
            self.focus_changed(before);
        }
        self.clear_visible_unread();
        self.request_redraw();
    }

    /// 칸 하나를 고른다(그 칸의 탭으로 옮겨 간다). 크게 보기 중에 다른 칸을 고르면 크게 보기를 푼다
    fn focus_pane(&mut self, id: u64) {
        self.focus_pane_keeping(id, false);
    }

    /// `focus_pane`. `keep_zoom`이면 크게 보던 중에도 풀지 않고 새 칸을 크게 본다(split-preserve-zoom = navigation)
    fn focus_pane_keeping(&mut self, id: u64, keep_zoom: bool) {
        let Some(gi) = self.group_of(id) else { return };
        if self.focused_id() != Some(id) {
            self.settle_preedit();
        }
        let before = self.focused_id();
        self.w.active = gi;
        let g = &mut self.w.tabs[gi];
        let changed = g.zoomed && g.focus != id;
        g.focus = id;
        if changed {
            // 풀지 않아도 크게 보는 칸이 바뀌었으니 크기를 다시 맞춘다
            g.zoomed = keep_zoom;
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

    /// 보이는 칸들의 안 읽음과 벨 표시를 지운다. 창이 앞이면 고른 칸의 끝남·오류 상태(OSC 7501)도 본 것으로 지운다
    fn clear_visible_unread(&mut self) {
        for i in 0..self.panes.len() {
            if self.is_visible(i) {
                self.panes[i].unread = false;
                self.panes[i].bell = false;
            }
            if bell_border_cleared_on_focus(self.w.focused, self.is_focused(i)) {
                self.panes[i].bell_border = false;
            }
            if self.w.focused && self.is_focused(i) {
                self.panes[i].session.clear_seen_program_status();
                // 그 칸을 봤으니 알림 센터의 그 칸 알림도 지운다(Ghostty `focusDidChange`, #625)
                if !self.panes[i].notes.is_empty() {
                    crate::desktop::forget(std::mem::take(&mut self.panes[i].notes));
                }
            }
        }
    }

    /// 칸의 프로그램 상태 기록(OSC 7501)이 바뀌었다(#618). 사용자가 보고 있는 칸(창이 앞이고 고른 칸)이면 끝남·오류를 본 것으로 지우고,
    /// 아니면 사용자를 부르는 바뀜(기다림·끝남·오류가 새로 됨)에 데스크톱 알림을 하나 띄운다(`desktop-notifications`, 칸마다 5초에 한 번)
    fn on_program_status(&mut self, idx: usize, alert: Option<bt_term::program_status::Alert>) {
        let watching = self.w.focused && self.is_focused(idx);
        if watching {
            self.panes[idx].session.clear_seen_program_status();
        }
        let Some(alert) = alert else { return };
        if watching || !self.config.desktop_notifications {
            return;
        }
        let title = self.panes[idx].display_title();
        let t = &mut self.panes[idx];
        if crate::status_note::due(t.last_status_note.map(|at| at.elapsed())) {
            t.last_status_note = Some(Instant::now());
            let (title, body) = crate::status_note::text(&alert, &title);
            if let Some(id) = crate::desktop::notify_pane(t.id, &title, &body) {
                crate::desktop::remember_note(&mut t.notes, id);
            }
        }
    }

    /// 칸이 낸 데스크톱 알림을 눌렀다(#625): 그 칸의 창을 앞으로 가져오고 그 탭과 칸을 고른다(Ghostty `handleUserNotification`).
    /// 칸이 이미 닫혔으면 아무것도 하지 않는다. 숨은 퀵 터미널의 칸이면 퀵 터미널을 보인다
    fn on_note_clicked(&mut self, event_loop: &ActiveEventLoop, pane: u64) {
        if !self.switch_to_pane(pane) {
            log::debug!("누른 알림의 칸 {pane}이 없습니다(이미 닫힘)");
            return;
        }
        self.focus_pane(pane);
        if self.w.quick {
            let hiding = self.w.quick_anim.as_ref().is_some_and(|a| a.hide);
            let shown = self.w.gfx.as_ref().is_some_and(|g| g.window.is_visible() == Some(true));
            if !shown || hiding {
                self.show_quick(event_loop);
            }
            if let Some(id) = self.w.id() {
                self.last_focused = Some(id);
            }
        } else if let Some(g) = &self.w.gfx {
            g.window.set_minimized(false);
            g.window.focus_window();
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
                if let Some(id) = crate::desktop::notify_pane(t.id, &title, &note.body) {
                    crate::desktop::remember_note(&mut t.notes, id);
                }
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
        if r.border && !self.panes[idx].bell_border {
            self.panes[idx].bell_border = true;
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

    /// 칸의 바깥 사각형에 들어가는 격자 크기. `font_size`는 그 칸의 글자 크기다
    fn grid_size_for(&self, outer: Rect, font_size: f32) -> GridSize {
        let Some(m) = self.metrics_at(font_size) else {
            return GridSize::new(80, 24);
        };
        grid_for(self.inner_rect(outer, font_size), &m)
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
    fn spawn_pane(&mut self, options: SpawnOptions, size: GridSize, font_size: f32) -> Option<Pane> {
        let id = self.next_id;
        self.next_id += 1;
        // 스크립트가 이 칸 하나에만 덮어쓸 값(#666). 여기서 꺼내므로 다음 칸에는 남지 않는다
        let script_overrides = self.spawn_overrides.take();
        let mut options = SpawnOptions {
            scrollback: self.config.scrollback_lines,
            cursor_shape: Some(match self.config.cursor_style {
                bt_core::config::CursorStyle::Block => CursorShape::Block,
                bt_core::config::CursorStyle::Bar => CursorShape::Beam,
                bt_core::config::CursorStyle::Underline => CursorShape::Underline,
                bt_core::config::CursorStyle::BlockHollow => CursorShape::HollowBlock,
            }),
            cursor_blink: self.config.cursor_blink.unwrap_or(false),
            clipboard_write: self.config.clipboard_write.engine_enabled(),
            clipboard_read: self.config.clipboard_read.engine_enabled(),
            clipboard_write_limit_bytes: self.config.clipboard_write_limit_bytes,
            word_chars: Some(self.config.selection_word_chars.clone()),
            scroll_on_output: self.config.scroll_to_bottom.output,
            title_report: self.config.title_report,
            osc_color_report_format: self.config.osc_color_report_format,
            enquiry_response: self.config.enquiry_response.clone(),
            vt_kam_allowed: self.config.vt_kam_allowed,
            vt_window_resize_allowed: self.config.vt_window_resize_allowed,
            vt_xt_checksum_report: self.config.vt_xt_checksum_report,
            vt_xt_checksum_extension: self.config.vt_xt_checksum_extension,
            image_storage_limit: Some(self.config.image_storage_limit),
            color_scheme_light: !self.scheme_dark(),
            grapheme_width_method: self.config.grapheme_width_method,
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
            let bin = std::env::current_exe()
                .ok()
                .and_then(|e| crate::shellint::bin_dir_of(&e));
            crate::shellint::apply(
                &mut options,
                dir,
                &self.config.shell_integration,
                &self.config.shell_integration_features,
                bin.as_deref(),
            );
        }
        options.term = self.config.term.clone();
        // 설정의 env는 b-terminal이 넣은 변수(BT_SOCKET·BT_TAB_ID·셸 통합) 뒤에 붙여 이긴다(Ghostty처럼, ADR-016)
        options.env.extend(self.config.env.iter().cloned());
        // 스크립트의 환경 변수는 설정 env 뒤에 붙여 그 위에 덮어쓴다(#666)
        if let Some(ov) = &script_overrides {
            ov.apply_env(&mut options);
        }
        // 시작 입력: 읽지 못하거나 상한을 넘으면 경고하고 하나도 보내지 않는다. 모든 새 칸에 보낸다.
        // 스크립트의 `initial input`이 있으면 설정 input 목록 대신 그 글 하나다(#666)
        match crate::script_config::startup_input(script_overrides.as_ref(), &self.config.input) {
            // 부른 쪽이 정한 시작 입력(Dock에 놓은 파일, #279)은 설정의 input 뒤에 보낸다
            Ok(mut bytes) => {
                bytes.append(&mut options.input);
                options.input = bytes;
            }
            Err(why) => log::warn!("input을 보내지 않습니다: {why}"),
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
                bell_border: false,
                user_title: None,
                created: Instant::now(),
                overlay_until: None,
                cwd: None,
                cwd_reset: false,
                last_desktop: None,
                last_status_note: None,
                notes: Vec::new(),
                progress: None,
                scroll_seen: 0,
                scrollbar_until: None,
                mouse_reporting_over: None,
                mouse_down: 0,
                mouse_last_cell: None,
                script_mouse: Default::default(),
                password: false,
                running: None,
                last_command: None,
                wait_after_exit: false,
                font_size,
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
            let bar = self.tabbar_px();
            area.y += bar as i32;
            area.h = area.h.saturating_sub(bar);
        }
        // 새 탭의 글자 크기: `window-inherit-font-size`가 켜져 있으면 고른 칸의 크기(창의 첫 탭이면 창의 크기)
        let font_size = self.new_pane_font_size(pane_font::NewPane::Tab);
        self.ensure_font_size(font_size);
        let size = self.grid_size_for(area, font_size);
        let pane = self.spawn_pane(options, size, font_size)?;
        let id = pane.id;
        self.panes.push(pane);
        self.settle_preedit();
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
                title: None,
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
        // 새 분할은 늘 고른 칸의 글자 크기를 물려받는다
        let font_size = self.new_pane_font_size(pane_font::NewPane::Split);
        self.ensure_font_size(font_size);
        let size = self.grid_size_for(rect, font_size);
        let pane = self.spawn_pane(options, size, font_size)?;
        debug_assert_eq!(pane.id, new_id);
        self.panes.push(pane);
        self.settle_preedit();
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
            .and_then(|t| inherited_cwd(t.cwd.as_deref(), t.cwd_reset, || t.session.cwd()));
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
                let mut opts = ctl_spawn(self.default_spawn(), cwd.filter(|c| c.is_dir()), &command);
                // 스크립트 `with configuration`의 명령·폴더(#666)는 설정과 btctl 인자보다 이긴다. 소켓 규약은 그대로다
                if let Some(ov) = &self.spawn_overrides {
                    ov.apply_launch(&mut opts);
                }
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
                Ok(i) if self.panes[i].session.readonly() => Response::err(id, READONLY_REFUSAL),
                Ok(i) => {
                    // 입력 → 화면 지연(#417): btctl send도 키와 같은 쪽의 시각을 적는다
                    self.panes[i].session.note_input();
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
                Ok(i) if self.panes[i].session.readonly() => Response::err(id, READONLY_REFUSAL),
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
            p.session.readonly(),
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
            Pending::CloseAllWindows | Pending::Quit => self.panes.iter().map(|p| p.id).collect(),
            // 붙여넣기는 묻기로 정한 뒤에 ask로 온다
            Pending::Paste { .. }
            | Pending::ClipboardRead { .. }
            | Pending::ClipboardWrite { .. }
            | Pending::OpenFile { .. }
            | Pending::KittyRead { .. }
            | Pending::KittyWrite { .. } => Vec::new(),
        };
        let busy: Vec<u64> = ids.into_iter().filter(|&id| self.pane_needs_confirm(id)).collect();
        if busy.is_empty() {
            self.perform(pending, event_loop);
            return;
        }
        let n = busy.len();
        // 읽기 전용 칸이 끼어 있으면 "프로그램이 돈다"고만 말하면 틀린다(#209)
        let readonly = busy
            .iter()
            .any(|&id| self.pane_index(id).is_some_and(|i| self.panes[i].session.readonly()));
        let which = if readonly {
            "읽기 전용이거나 프로그램이 도는"
        } else {
            "프로그램이 도는"
        };
        let (message, yes) = match &pending {
            Pending::ClosePane(id) if readonly => ("이 칸은 읽기 전용입니다. 닫을까요?".to_string(), "닫기"),
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
            Pending::CloseTab(_) => (format!("이 탭에서 {which} 칸이 {n}개 있습니다. 탭을 닫을까요?"), "닫기"),
            Pending::CloseWindow => (format!("이 창에서 {which} 칸이 {n}개 있습니다. 창을 닫을까요?"), "닫기"),
            Pending::CloseAllWindows => (format!("{which} 칸이 {n}개 있습니다. 모든 창을 닫을까요?"), "닫기"),
            Pending::Quit => (format!("{which} 칸이 {n}개 있습니다. 끝낼까요?"), "끝내기"),
            Pending::Paste { .. }
            | Pending::ClipboardRead { .. }
            | Pending::ClipboardWrite { .. }
            | Pending::OpenFile { .. }
            | Pending::KittyRead { .. }
            | Pending::KittyWrite { .. } => return,
        };
        self.ask(pending, message, yes);
    }

    /// 확인 막대를 띄운다. 묻는 동안 팔레트는 닫는다
    fn ask(&mut self, pending: Pending, message: String, yes: &'static str) {
        self.w.palette = None;
        // 쓰던 조합 글자는 막대가 뜨기 전에 칸에 확정한다(막대가 떠 있는 동안 확정 글은 버린다)
        self.settle_preedit();
        self.w.confirm = Some(Confirm {
            pending,
            message,
            yes,
            shown: Instant::now(),
        });
        self.sync_confirm_ime();
        self.request_redraw();
    }

    /// 확인 막대가 떠 있는 동안 지금 창의 입력기를 끈다(#721). 한국어 입력기는 Y·N 키를 조합으로 가져가 winit이
    /// `KeyboardInput`을 주지 않으므로, 끄면 그 키가 키로 와 `confirm_key`가 ⌘ 층 글자로 읽는다
    fn sync_confirm_ime(&mut self) {
        let allowed = self.w.confirm.is_none();
        if let Some(g) = self.w.gfx.as_ref() {
            g.window.set_ime_allowed(allowed);
        }
    }

    /// 복사할 고른 글. `clipboard-trim-trailing-spaces`가 켜져 있으면 줄 끝 공백을 지우고,
    /// `clipboard-codepoint-map`으로 글자를 바꾼다. 고른 글이 없으면 None
    fn selection_for_copy(&mut self) -> Option<String> {
        let mut text = self.active_pane().and_then(|t| t.session.selection_text())?;
        if self.config.clipboard_trim_trailing_spaces {
            text = bt_term::trim_trailing_spaces(&text);
        }
        Some(bt_core::config::map_clipboard_text(
            &self.config.clipboard_codepoint_map,
            &text,
        ))
    }

    /// 고른 글을 시스템 클립보드에 `format` 꼴로 넣는다(Ghostty `copySelectionToClipboards`). 고른 글이 없으면 하지 않는다. 했으면 true.
    /// `plain`·`vt`는 text/plain, `html`은 text/html, `mixed`는 글 + HTML. 앱 안 선택 클립보드(`selection_clipboard`)는 늘 글만이다
    fn copy_selection(&mut self, format: bt_term::CopyFormat) -> bool {
        let Some(text) = self.selection_for_copy() else {
            return false;
        };
        let trim = self.config.clipboard_trim_trailing_spaces;
        let pane = self.active_pane();
        let payload = crate::copy::payload(format, &text, |emit, colors| {
            pane.as_ref()
                .and_then(|t| t.session.selection_styled(emit, trim, colors))
        });
        let Some(cb) = self.clipboard.as_mut() else {
            return false;
        };
        match payload {
            crate::copy::ClipboardPayload::Text(t) => cb.set_text(t).is_ok(),
            crate::copy::ClipboardPayload::Html { html, alt } => cb.set().html(html, alt).is_ok(),
        }
    }

    /// 선택 클립보드에 쓴다. macOS는 이름 붙은 pasteboard에도 써서 다른 b-terminal과 나눈다(실패해도 앱 안 저장소는 남는다).
    /// 암호 입력 중인 칸(붙여넣기처럼 termios를 다시 읽음)이나 보안 입력이 켜져 있으면 나누지 않는다. 어느 쪽이든 그때의
    /// 바뀜 번호를 적어 두어, 읽을 때 그 뒤에 바뀐 경우에만 공유된 글을 쓴다(`pick_selection`)
    fn set_selection_clipboard(&mut self, text: String) {
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::{named_pasteboard_change_count, set_named_pasteboard_string};
            let (password, secure) = self.focused_private();
            let written = if share_selection(password, secure) {
                set_named_pasteboard_string(SELECTION_PASTEBOARD, &text)
            } else {
                None
            };
            self.selection_seen = written.or_else(|| named_pasteboard_change_count(SELECTION_PASTEBOARD));
        }
        self.selection_clipboard = Some(text);
    }

    /// 고른 칸이 암호 입력 중인가(붙여넣기처럼 termios를 다시 읽음), 보안 입력이 켜져 있는가(전역이거나 그 칸)
    fn focused_private(&self) -> (bool, bool) {
        let id = self.focused_id();
        let password = id.and_then(|i| self.pane_index(i)).is_some_and(|i| {
            let p = &self.panes[i];
            paste_hidden(p.password, p.session.password_input())
        });
        let secure = self.secure.global() || id.is_some_and(|i| self.secure.has_scoped(i));
        (password, secure)
    }

    /// 선택 클립보드의 글(macOS는 다른 b-terminal이 나중에 고른 것 포함)
    fn selection_clipboard_text(&self) -> Option<String> {
        #[cfg(target_os = "macos")]
        let (shared, count) = {
            use winit::platform::macos::{named_pasteboard_change_count, named_pasteboard_string};
            (
                named_pasteboard_string(SELECTION_PASTEBOARD, SELECTION_MAX_LEN),
                named_pasteboard_change_count(SELECTION_PASTEBOARD),
            )
        };
        #[cfg(not(target_os = "macos"))]
        let (shared, count) = (None, None);
        pick_selection(shared, count, self.selection_seen, self.selection_clipboard.as_deref())
    }

    /// `copy-on-select`: 고르기를 마칠 때 값에 따라 선택 클립보드·시스템 클립보드로 보낸다.
    /// 시스템 클립보드는 Ghostty처럼 mixed(글 + HTML)다
    fn copy_on_select_now(&mut self) {
        let (password, secure) = self.focused_private();
        let (selection, system) = auto_copy_targets(self.config.copy_on_select, password || secure);
        if selection {
            if let Some(text) = self.selection_for_copy() {
                self.set_selection_clipboard(text);
            }
        }
        if system {
            self.copy_selection(bt_term::CopyFormat::Mixed);
        }
    }

    /// 복사 명령(단축키·메뉴·오른쪽 클릭)으로 복사한다. `selection-clear-on-copy`가 켜져 있으면 복사한 뒤 고른 영역을 지운다.
    /// `copy-on-select`의 자동 복사는 지우지 않는다(Ghostty)
    fn copy_selection_command(&mut self, format: bt_term::CopyFormat) -> bool {
        let copied = self.copy_selection(format);
        if copied && clears_selection_after_copy(self.config.selection_clear_on_copy) {
            if let Some(t) = self.active_pane() {
                t.session.select_clear();
            }
        }
        copied
    }

    /// 오른쪽 클릭(또는 Ctrl+왼쪽 클릭)으로 메뉴를 띄운다(#278). 먼저 Ghostty처럼 누른 자리를 고른다(Surface.zig 4220~4246줄):
    /// 이미 고른 영역이 그 자리를 품으면 그대로, 링크 위면 링크, 아니면 단어. 고른 것은 `copy-on-select`가 켜져 있으면 복사한다
    fn context_menu_press(&mut self) {
        let Some(idx) = self.active_index() else { return };
        if let Some((c, r)) = self.cell_in(idx, self.w.cursor_pos) {
            let link = self.link_under(self.w.cursor_pos).map(|(_, hit)| hit);
            let contained = self.panes[idx].session.selection_contains(c, r);
            let session = &self.panes[idx].session;
            let picked = match crate::context_menu::pick(contained, link.is_some()) {
                crate::context_menu::Pick::Keep => false,
                crate::context_menu::Pick::Link => match link.and_then(|h| crate::context_menu::link_ends(&h.spans)) {
                    Some(((r0, c0), (r1, c1))) => {
                        session.select_start(c0, r0, SelectKind::Simple);
                        session.select_update(c1, r1);
                        true
                    }
                    None => false,
                },
                crate::context_menu::Pick::Word => {
                    session.select_start(c, r, SelectKind::Word);
                    session.select_update(c, r);
                    true
                }
            };
            if picked {
                self.copy_on_select_now();
                self.request_redraw();
            }
        }
        #[cfg(target_os = "macos")]
        {
            let (has_selection, readonly) = match self.active_pane() {
                Some(t) => (t.session.selection_text().is_some(), t.session.readonly()),
                None => return,
            };
            crate::context_menu::popup(&crate::context_menu::entries(has_selection, readonly));
        }
    }

    /// 마우스 보고 모드가 아닌 칸의 오른쪽 클릭(`right-click-action`). `context-menu`는 단어·링크를 고르고 메뉴를 띄운다(#278)
    /// `script`이면 스크립트 마우스(#662)다: 클립보드를 읽는 붙여넣기와 모달 메뉴는 하지 않고, 복사는 한다
    fn right_click(&mut self, script: bool) {
        let paste = |app: &mut Self| {
            if script {
                return;
            }
            let text = app.clipboard.as_mut().and_then(|cb| cb.get_text().ok());
            if let Some(text) = text.filter(|t| !t.is_empty()) {
                app.paste(text);
            }
        };
        match self.config.right_click_action {
            bt_core::config::RightClickAction::ContextMenu => {
                if !script {
                    self.context_menu_press()
                }
            }
            bt_core::config::RightClickAction::Ignore => {}
            bt_core::config::RightClickAction::Paste => paste(self),
            bt_core::config::RightClickAction::Copy => {
                self.copy_selection_command(bt_term::CopyFormat::Mixed);
            }
            bt_core::config::RightClickAction::CopyOrPaste => {
                if !self.copy_selection_command(bt_term::CopyFormat::Mixed) {
                    paste(self);
                }
            }
        }
    }

    /// 클립보드 글을 고른 칸에 붙여넣는다. 괄호 붙여넣기를 쓰지 않는 프로그램에 줄바꿈이 든 글이면 먼저 묻는다(#109)
    fn paste(&mut self, text: String) {
        let Some(t) = self.active_pane() else { return };
        // 읽기 전용 칸에는 붙여넣지 않는다. 확인 막대도 띄우지 않는다(#209)
        if t.session.readonly() {
            return;
        }
        // 프로그램이 붙여넣기 이벤트(모드 5522)를 켜 두었으면 글 대신 종류 목록과 일회용 암호를 보낸다. 글은 읽히지 않으므로
        // 붙여넣기 보호(줄바꿈 확인)와 괄호 붙여넣기 틀은 거치지 않는다(Ghostty `paste.zig`). 사람이 붙여넣을 때만 여기까지 온다(#325)
        if t.session.kitty_paste_mode() {
            let avail = bt_term::kitty_clip::clipboard_listing(Some(&text));
            if t.session.kitty_paste_event(&avail) {
                return;
            }
        }
        let id = t.id;
        self.paste_to(id, text);
    }

    /// 그 칸에 붙여넣는다. 붙여넣기 보호가 막으면 확인 막대를 띄우고 거짓을 돌려준다(글은 사용자가 허용해야 들어간다).
    /// 사람의 붙여넣기(`paste`)와 AppleScript `input text`(#634)가 같이 쓴다
    fn paste_to(&mut self, id: u64, text: String) -> bool {
        let Some(t) = self.pane_index(id).map(|i| &self.panes[i]) else {
            return false;
        };
        let bracketed = t.session.modes().bracketed_paste;
        // 요청한 때의 암호 상태. 막대가 떠 있는 동안 바뀌어도 막대 글은 다시 만들지 않는다
        let password = paste_hidden(t.password, t.session.password_input());
        if !self.config.paste_protection || bt_term::paste_is_safe(&text, bracketed, self.config.paste_bracketed_safe) {
            self.paste_into(id, &text);
            return true;
        }
        let message = paste_prompt(&text, password);
        self.ask(Pending::Paste { pane: id, text }, message, "붙여넣기");
        false
    }

    /// 창에 끌어다 놓은 파일 경로를 놓은 창의 지금 칸에 붙여넣는다(#566). 글은 `paste`를 거쳐
    /// 위험 바이트 바꾸기·줄바꿈 확인·읽기 전용 칸 거름을 그대로 받는다
    fn paste_dropped(&mut self, window: WindowId, text: String) {
        if !self.switch_to(window) {
            return;
        }
        if self.w.confirm.is_some() {
            self.toast("확인할 일이 있어 끌어놓은 파일을 넣지 않았습니다".into());
            return;
        }
        self.paste(text);
    }

    /// 그 칸의 지금 모드로 붙여넣기 바이트를 만들어 쓴다
    fn paste_into(&mut self, pane: u64, text: &str) {
        if let Some(t) = self
            .pane_index(pane)
            .map(|i| &self.panes[i])
            .filter(|t| !t.session.readonly())
        {
            let bytes = bt_term::encode_paste(text, t.session.modes().bracketed_paste);
            t.session.select_clear();
            t.session.write_input(&bytes, self.config.scroll_to_bottom.keystroke);
        }
    }

    /// 프로그램이 OSC 52로 보낸 글(Claude Code의 /copy 등). 보고 있는 창의 고른 칸에서 온 것만 받는다: 뒤쪽 탭이나 옆 칸에
    /// 흘러간 출력이 몰래 클립보드를 바꿔 나중의 붙여넣기에 명령을 끼워 넣지 못하게(붙여넣기 하이재킹).
    /// 크기는 고정 1MiB와 clipboard-write-limit-bytes 중 작은 쪽(ADR-013). ask면 이 판단을 지난 것만 막대로 묻는다(ADR-019)
    fn on_clipboard_store(&mut self, idx: usize, text: String) {
        use bt_core::clipboard::ClipVerdict;
        let watching = self.w.focused && self.is_focused(idx);
        let verdict = bt_core::clipboard::decide_write(
            self.config.clipboard_write,
            watching,
            text.len(),
            self.config.clipboard_write_limit_bytes,
            self.w.confirm.is_some(),
        );
        match verdict {
            ClipVerdict::Allow => {
                self.store_clipboard(idx, text);
            }
            ClipVerdict::Ask => {
                let id = self.panes[idx].id;
                let message = bt_core::clipboard::write_prompt(&text);
                self.ask(Pending::ClipboardWrite { pane: id, text }, message, "허용");
            }
            ClipVerdict::Deny => log::warn!(
                "탭 {}: 클립보드 쓰기(OSC 52)를 버렸습니다({})",
                self.panes[idx].id,
                if !watching {
                    "보고 있지 않은 탭"
                } else if !bt_core::config::clipboard_write_allowed(text.len(), self.config.clipboard_write_limit_bytes)
                {
                    "크기가 상한을 넘음"
                } else if self.config.clipboard_write == bt_core::clipboard::ClipboardAccess::Ask {
                    "확인 막대가 이미 떠 있음"
                } else {
                    "clipboard-write = deny"
                }
            ),
        }
    }

    /// 허용된 OSC 52 쓰기를 클립보드에 넣고 알림 띠로 보인다
    fn store_clipboard(&mut self, idx: usize, text: String) -> bool {
        let ok = self
            .clipboard
            .as_mut()
            .is_some_and(|cb| cb.set_text(text.clone()).is_ok());
        let shown = bt_core::clipboard::clip_preview(&text, 40);
        log::info!(
            "탭 {}: 프로그램이 클립보드에 {}글자를 썼습니다",
            self.panes[idx].id,
            text.chars().count()
        );
        // 보고 있는 탭이라 안 읽음·데스크톱 알림으로는 보이지 않는다. 창 안 알림 띠로 늘 보인다
        if self.config.app_notifications.clipboard_copy {
            self.toast(format!("클립보드에 복사됨: {shown}"));
        }
        ok
    }

    /// 프로그램이 OSC 52로 클립보드를 읽으려 한다. ask면 막대에 읽어 갈 글 앞부분을 보이고 묻는다(ADR-019)
    fn on_clipboard_load(&mut self, idx: usize, reply: bt_term::ClipboardReply) {
        use bt_core::clipboard::ClipVerdict;
        let watching = self.w.focused && self.is_focused(idx);
        let readonly = self.panes[idx].session.readonly();
        let verdict =
            bt_core::clipboard::decide_read(self.config.clipboard_read, watching, readonly, self.w.confirm.is_some());
        let read = |app: &mut Self| {
            app.clipboard
                .as_mut()
                .and_then(|cb| cb.get_text().ok())
                .unwrap_or_default()
        };
        match verdict {
            ClipVerdict::Allow => {
                let text = read(self);
                self.panes[idx].session.write(reply.format(&text).as_bytes());
            }
            ClipVerdict::Ask => {
                // 막대에 보인 글을 그대로 보낸다(묻는 사이 클립보드가 바뀌어도 본 것과 간 것이 같다)
                let text = read(self);
                let id = self.panes[idx].id;
                let message = bt_core::clipboard::read_prompt(&text);
                self.ask(Pending::ClipboardRead { pane: id, reply, text }, message, "허용");
            }
            ClipVerdict::Deny => log::warn!("탭 {}: 클립보드 읽기(OSC 52)를 거절했습니다", self.panes[idx].id),
        }
    }

    /// 허용된 읽기 답: 그 칸이 아직 있고, 보고 있는 칸이며, 읽기 전용이 아닐 때만 그 칸에 쓴다. 아니면 버린다
    fn answer_clipboard_read(&mut self, pane: u64, reply: &bt_term::ClipboardReply, text: &str) {
        let target = self
            .pane_index(pane)
            .filter(|&i| self.is_focused(i) && !self.panes[i].session.readonly());
        match target {
            Some(i) => self.panes[i].session.write(reply.format(text).as_bytes()),
            None => log::warn!("탭 {pane}: 클립보드 읽기 답을 버렸습니다(칸이 닫혔거나 보고 있지 않거나 읽기 전용)"),
        }
    }

    /// 허용된 쓰기: 그 칸이 아직 있고 보고 있는 칸일 때만 클립보드에 넣는다
    fn answer_clipboard_write(&mut self, pane: u64, text: String) {
        match self.pane_index(pane).filter(|&i| self.w.focused && self.is_focused(i)) {
            Some(i) => {
                self.store_clipboard(i, text);
            }
            None => log::warn!("탭 {pane}: 클립보드 쓰기를 버렸습니다(칸이 닫혔거나 보고 있지 않음)"),
        }
    }

    /// 그 칸에 kitty 클립보드 상태 답 하나를 쓴다(칸이 없으면 버린다)
    fn kitty_status(
        &mut self,
        pane: u64,
        op: bt_term::kitty_clip::Op,
        status: bt_term::kitty_clip::Status,
        id: &str,
        bel: bool,
    ) {
        if let Some(i) = self.pane_index(pane) {
            self.panes[i]
                .session
                .write(&bt_term::kitty_clip::status_reply(op, status, id, bel));
        }
    }

    /// 프로그램이 OSC 5522로 클립보드를 읽으려 한다(#325, ADR-030). 정책은 OSC 52의 `clipboard-read`를 그대로 따른다:
    /// deny는 EPERM, ask는 같은 확인 막대(목록만 묻거나 일회용 암호가 있거나 읽어 갈 글이 없으면 건너뜀), allow는 바로 답한다.
    /// 글 종류만 읽는다(arboard). primary 선택 영역은 이 터미널에 없으므로 ENOSYS
    fn on_kitty_read(&mut self, idx: usize, req: bt_term::kitty_clip::ReadRequest) {
        use bt_core::clipboard::ClipVerdict;
        use bt_term::kitty_clip::{Op, Status};
        let id = self.panes[idx].id;
        if self.config.clipboard_read == bt_core::clipboard::ClipboardAccess::Deny {
            log::warn!("탭 {id}: 클립보드 읽기(OSC 5522)를 거절했습니다(clipboard-read = deny)");
            return self.kitty_status(id, Op::Read, Status::Eperm, &req.id, req.bel);
        }
        if req.primary {
            return self.kitty_status(id, Op::Read, Status::Enosys, &req.id, req.bel);
        }
        let text = self
            .clipboard
            .as_mut()
            .and_then(|cb| cb.get_text().ok())
            .unwrap_or_default();
        let watching = self.w.focused && self.is_focused(idx);
        let readonly = self.panes[idx].session.readonly();
        // 읽어 갈 글이 없으면 밝힐 것이 없어 묻지 않는다
        let prompt_free = req.granted || req.prompt_exempt() || text.is_empty();
        match bt_core::clipboard::decide_kitty_read(
            self.config.clipboard_read,
            watching,
            readonly,
            self.w.confirm.is_some(),
            prompt_free,
        ) {
            ClipVerdict::Allow => {
                let bytes = bt_term::kitty_clip::answer_read(&req, Some(&text));
                self.panes[idx].session.write(&bytes);
            }
            ClipVerdict::Ask => {
                let message = bt_core::clipboard::read_prompt(&text);
                self.ask(Pending::KittyRead { pane: id, req, text }, message, "허용");
            }
            ClipVerdict::Deny => {
                log::warn!(
                    "탭 {id}: 클립보드 읽기(OSC 5522)를 거절했습니다(보고 있지 않거나 읽기 전용이거나 막대가 떠 있음)"
                );
                self.kitty_status(id, Op::Read, Status::Eperm, &req.id, req.bel);
            }
        }
    }

    /// 허용된 kitty 읽기 답: 그 칸이 아직 있고, 보고 있는 칸이며, 읽기 전용이 아닐 때만 막대에 보인 글을 답한다
    fn answer_kitty_read(&mut self, pane: u64, req: &bt_term::kitty_clip::ReadRequest, text: &str) {
        let target = self
            .pane_index(pane)
            .filter(|&i| self.is_focused(i) && !self.panes[i].session.readonly());
        match target {
            Some(i) => self.panes[i]
                .session
                .write(&bt_term::kitty_clip::answer_read(req, Some(text))),
            None => {
                log::warn!(
                    "탭 {pane}: 클립보드 읽기(OSC 5522) 답을 버렸습니다(칸이 닫혔거나 보고 있지 않거나 읽기 전용)"
                )
            }
        }
    }

    /// 프로그램이 OSC 5522 쓰기 거래를 끝냈다. 정책은 `clipboard-write`와 "보는 칸만" 규칙을 OSC 52와 똑같이 따른다.
    /// 글 종류만 쓴다(arboard): 다른 종류만 있으면 ENOSYS, UTF-8이 아니면 EINVAL, 아무것도 없으면 클립보드를 비운다
    fn on_kitty_write(&mut self, idx: usize, req: bt_term::kitty_clip::WriteRequest) {
        use bt_core::clipboard::ClipVerdict;
        use bt_term::kitty_clip::{Op, Status, WriteText};
        let id = self.panes[idx].id;
        if self.config.clipboard_write == bt_core::clipboard::ClipboardAccess::Deny {
            log::warn!("탭 {id}: 클립보드 쓰기(OSC 5522)를 거절했습니다(clipboard-write = deny)");
            return self.kitty_status(id, Op::Write, Status::Eperm, &req.id, req.bel);
        }
        if req.primary {
            return self.kitty_status(id, Op::Write, Status::Enosys, &req.id, req.bel);
        }
        let text = match bt_term::kitty_clip::write_text(&req.contents) {
            WriteText::Text(t) => t,
            WriteText::Clear => String::new(),
            WriteText::Unsupported => return self.kitty_status(id, Op::Write, Status::Enosys, &req.id, req.bel),
            WriteText::BadUtf8 => return self.kitty_status(id, Op::Write, Status::Einval, &req.id, req.bel),
        };
        let watching = self.w.focused && self.is_focused(idx);
        match bt_core::clipboard::decide_kitty_write(
            self.config.clipboard_write,
            watching,
            text.len(),
            self.config.clipboard_write_limit_bytes,
            self.w.confirm.is_some(),
            req.granted,
        ) {
            ClipVerdict::Allow => self.finish_kitty_write(idx, &req, text),
            ClipVerdict::Ask => {
                let message = bt_core::clipboard::write_prompt(&text);
                self.ask(Pending::KittyWrite { pane: id, req, text }, message, "허용");
            }
            ClipVerdict::Deny => {
                log::warn!(
                    "탭 {id}: 클립보드 쓰기(OSC 5522)를 버렸습니다(보고 있지 않거나 크기가 상한을 넘거나 막대가 떠 있음)"
                );
                self.kitty_status(id, Op::Write, Status::Eperm, &req.id, req.bel);
            }
        }
    }

    /// 클립보드에 넣고(알림 띠가 보인다) DONE이나 EIO를 답한다
    fn finish_kitty_write(&mut self, idx: usize, req: &bt_term::kitty_clip::WriteRequest, text: String) {
        use bt_term::kitty_clip::{Op, Status};
        let id = self.panes[idx].id;
        let status = if self.store_clipboard(idx, text) {
            Status::Done
        } else {
            Status::Eio
        };
        self.kitty_status(id, Op::Write, status, &req.id, req.bel);
    }

    /// 허용된 kitty 쓰기: 그 칸이 아직 있고 보고 있는 칸일 때만 클립보드에 넣는다
    fn answer_kitty_write(&mut self, pane: u64, req: &bt_term::kitty_clip::WriteRequest, text: String) {
        match self.pane_index(pane).filter(|&i| self.w.focused && self.is_focused(i)) {
            Some(i) => self.finish_kitty_write(i, req, text),
            None => {
                log::warn!("탭 {pane}: 클립보드 쓰기(OSC 5522)를 버렸습니다(칸이 닫혔거나 보고 있지 않음)");
                self.kitty_status(
                    pane,
                    bt_term::kitty_clip::Op::Write,
                    bt_term::kitty_clip::Status::Eperm,
                    &req.id,
                    req.bel,
                );
            }
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
            // 마지막 창이 닫히면 앱을 끝낼지는 quit-after-last-window-closed가 정한다(#178)
            Pending::CloseAllWindows => {
                while !self.windowless() {
                    self.close_current_window(event_loop);
                }
            }
            // 끌 때 창 상태는 exiting에서 적는다
            Pending::Quit => event_loop.exit(),
            Pending::Paste { pane, text } => self.paste_into(pane, &text),
            Pending::ClipboardRead { pane, reply, text } => self.answer_clipboard_read(pane, &reply, &text),
            Pending::ClipboardWrite { pane, text } => self.answer_clipboard_write(pane, text),
            Pending::OpenFile { cwd, input } => self.open_dropped(event_loop, cwd, Some(input)),
            Pending::KittyRead { pane, req, text } => self.answer_kitty_read(pane, &req, &text),
            Pending::KittyWrite { pane, req, text } => self.answer_kitty_write(pane, &req, text),
        }
    }

    /// 확인 막대에서 거절했다. kitty 클립보드 요청은 거절을 EPERM으로 알린다(프로그램이 기다리지 않게). 나머지는 아무 일도 없다
    fn denied(&mut self, pending: Pending) {
        use bt_term::kitty_clip::{Op, Status};
        match pending {
            Pending::KittyRead { pane, req, .. } => self.kitty_status(pane, Op::Read, Status::Eperm, &req.id, req.bel),
            Pending::KittyWrite { pane, req, .. } => {
                self.kitty_status(pane, Op::Write, Status::Eperm, &req.id, req.bel)
            }
            _ => {}
        }
    }

    /// Dock에 놓은 폴더·파일을 새 탭이나 새 창으로 연다(#279). `input`이 있으면(파일) 셸에 치고 끝나도 칸을 남긴다
    fn open_dropped(&mut self, event_loop: &ActiveEventLoop, cwd: std::path::PathBuf, input: Option<String>) {
        let mut opts = self.default_spawn();
        opts.cwd = Some(cwd);
        if let Some(text) = &input {
            opts.input = text.as_bytes().to_vec();
        }
        let tab = match crate::dock_open::placement(self.config.macos_dock_drop_behavior, self.windowless()) {
            crate::dock_open::Placement::NewTab => self.open_tab(opts),
            crate::dock_open::Placement::NewWindow => self.open_window(event_loop, opts),
        };
        // 스크립트는 곧 끝나므로 `wait-after-command`처럼 끝나도 칸을 남긴다(Ghostty `waitAfterCommand`)
        if let (Some(id), true) = (tab, input.is_some()) {
            if let Some(i) = self.pane_index(id) {
                self.panes[i].wait_after_exit = true;
            }
        }
    }

    /// Finder 서비스가 넘긴 경로(#631). 폴더(파일이면 부모 폴더)마다 새 탭이나 새 창을 띄운다. 실행하지 않는다
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn on_open_folders(&mut self, event_loop: &ActiveEventLoop, want_tab: bool, paths: Vec<std::path::PathBuf>) {
        use crate::dock_open::{Placement, kind_of, service_dirs, service_placement};
        let dirs = service_dirs(paths, kind_of);
        if dirs.is_empty() {
            log::warn!("Finder 서비스가 넘긴 경로 중 열 폴더가 없습니다");
            return;
        }
        if let Some(w) = self.last_focused {
            self.switch_to(w);
        }
        for cwd in dirs {
            let mut opts = self.default_spawn();
            opts.cwd = Some(cwd);
            match service_placement(want_tab, self.windowless()) {
                Placement::NewTab => self.open_tab(opts),
                Placement::NewWindow => self.open_window(event_loop, opts),
            };
        }
    }

    /// Dock·Finder가 열라고 한 경로(#279). 폴더는 바로 열고, 파일은 확인 막대를 거친다
    fn on_open_file(&mut self, event_loop: &ActiveEventLoop, path: std::path::PathBuf) {
        use crate::dock_open::{Plan, Refusal, kind_of, plan};
        if let Some(w) = self.last_focused {
            self.switch_to(w);
        }
        match plan(&path, kind_of(&path)) {
            Plan::Folder { cwd } => self.open_dropped(event_loop, cwd, None),
            Plan::File { cwd, input, shown } => {
                // 확인 막대는 창에 뜬다. 창이 하나도 없으면 기본 셸 창을 먼저 연다
                if self.windowless() && self.open_window(event_loop, self.default_spawn()).is_none() {
                    log::warn!("파일을 열 창을 만들지 못했습니다: {path:?}");
                    return;
                }
                if self.w.confirm.is_some() {
                    log::warn!("확인할 일이 있어 끌어놓은 파일을 열지 않았습니다: {path:?}");
                    self.toast("확인할 일이 있어 끌어놓은 파일을 열지 않았습니다".into());
                    return;
                }
                self.ask(
                    Pending::OpenFile { cwd, input },
                    format!("끌어놓은 파일을 실행할까요? {shown}"),
                    "실행",
                );
            }
            Plan::Refuse(why) => {
                // `{:?}`는 숨은 제어 문자를 \x1b처럼 보이게 한다
                log::warn!("끌어놓은 경로를 열지 않았습니다({why:?}): {path:?}");
                if !self.windowless() {
                    self.toast(
                        match why {
                            Refusal::UnsafeChars => "셸에 안전하게 넘길 수 없는 글자가 있어 열지 않았습니다",
                            Refusal::NotUtf8 => "경로를 글자로 읽을 수 없어 실행하지 않았습니다",
                            Refusal::Missing | Refusal::NotAbsolute => "열 수 없는 경로입니다",
                        }
                        .into(),
                    );
                }
            }
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
                if let Some((target, text)) = self.w.title_prompt.take() {
                    self.apply_title(target, &text);
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

    /// 프롬프트를 누르면 셸이 밝힌 방식으로 PTY에 보낸다(#137, #338). 셸이 OSC 133;A에 `cl=`나 `click_events=`를 밝힌 화면에서만이고,
    /// 무엇을 보낼지는 `Session::prompt_click`이 정한다(Ghostty `maybePromptClick`). 읽기 전용 칸과 명령이 도는 중에는 보내지 않는다
    fn click_to_move(&mut self, idx: usize, col: usize, row: usize) {
        let Some(p) = self
            .panes
            .get(idx)
            .filter(|p| !p.session.readonly() && p.running.is_none())
        else {
            return;
        };
        if let Some(bytes) = p.session.prompt_click(col, row) {
            p.session.select_clear();
            p.session.write(&bytes);
        }
    }

    /// 제목을 정할 곳(칸·탭·창)에 글을 정한다. 빈 글이면 덮어쓴 것을 지운다(#207)
    fn apply_title(&mut self, target: TitleTarget, text: &str) {
        match target {
            TitleTarget::Surface(pane) => self.set_user_title(pane, text),
            TitleTarget::Tab(pane) => {
                if let Some(gi) = self.group_of(pane) {
                    self.w.tabs[gi].title = clean_user_title(text);
                    self.request_redraw_all();
                }
            }
            TitleTarget::Window => {
                self.w.window_title = clean_user_title(text);
                self.request_redraw_all();
            }
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
        let yes = if self.w.confirm.as_ref().is_some_and(|c| c.pending.is_program_prompt()) {
            // 프로그램의 출력이 띄운 막대: Enter는 허용이 아니다. y는 막대가 뜬 지 잠깐 뒤에만(ADR-019)
            let shown_for = self.w.confirm.as_ref().map_or(Duration::ZERO, |c| c.shown.elapsed());
            let key = match &event.logical_key {
                Key::Named(NamedKey::Enter) => Some(bt_core::clipboard::AnswerKey::Enter),
                Key::Named(NamedKey::Escape) => Some(bt_core::clipboard::AnswerKey::Escape),
                Key::Character(_) => keys::answer_char(event).map(bt_core::clipboard::AnswerKey::Char),
                _ => None,
            };
            key.and_then(|k| bt_core::clipboard::clipboard_answer(k, plain, event.repeat, shown_for))
        } else {
            match &event.logical_key {
                Key::Named(NamedKey::Enter) => Some(true),
                Key::Named(NamedKey::Escape) => Some(false),
                Key::Character(_) if plain => match keys::answer_char(event).map(|c| c.to_ascii_lowercase()) {
                    Some('y') => Some(true),
                    Some('n') => Some(false),
                    _ => None,
                },
                _ => None,
            }
        };
        if let Some(yes) = yes {
            let c = self.w.confirm.take();
            self.sync_confirm_ime();
            self.request_redraw();
            match (yes, c) {
                (true, Some(c)) => self.perform(c.pending, event_loop),
                (false, Some(c)) => self.denied(c.pending),
                _ => {}
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
        // Session을 떨어뜨리면 `Drop`이 셸에 SIGHUP을 보낸다(읽기 스레드가 PTY 주 쪽을 쥐고 있어 떨어뜨리는 것만으로는 닫히지 않는다, #651). 들고 있으면 그때까지 셸이 산다
        let mut pane = self.panes.remove(index);
        let id = pane.id;
        crate::desktop::forget(std::mem::take(&mut pane.notes));
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
                    let removed = self.w.tabs.remove(gi);
                    if gi < self.w.active || self.w.active >= self.w.tabs.len() {
                        self.w.active = self.w.active.saturating_sub(1);
                    }
                    held = Some(Closed::Tab {
                        panes: vec![pane],
                        layout: Node::Leaf(id),
                        index: gi,
                        focus: was.1,
                        zoomed: was.0,
                        title: removed.title,
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
        let (layout, focus, zoomed, title) = (g.layout.clone(), g.focus, g.zoomed, g.title.clone());
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
                let mut pane = self.panes.remove(i);
                // 칸을 닫을 때처럼 알림 센터의 그 칸 알림을 지운다(되살려도 옛 알림은 돌아오지 않는다, #625)
                crate::desktop::forget(std::mem::take(&mut pane.notes));
                panes.push(pane);
            }
        }
        self.hold(Closed::Tab {
            panes,
            layout,
            index: gi,
            focus,
            zoomed,
            title,
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
                        self.restore_tab(vec![*pane], Node::Leaf(id), usize::MAX, id, false, None);
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
                title,
            } => {
                self.restore_tab(panes, layout, index, focus, zoomed, title);
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
    fn restore_tab(
        &mut self,
        panes: Vec<Pane>,
        layout: Node,
        index: usize,
        focus: u64,
        zoomed: bool,
        title: Option<String>,
    ) {
        let before = self.focused_id();
        let at = index.min(self.w.tabs.len());
        self.w.tabs.insert(
            at,
            TabGroup {
                layout,
                focus,
                zoomed,
                title,
            },
        );
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

    /// 키를 뗐다. 프로그램이 kitty 키보드 프로토콜 2(키 뗌 보고)를 켰고, 그 키의 누름을 프로그램에 보냈을 때만 뗌을 보낸다.
    /// 켜지 않았으면 뗌은 버린다(옛 바이트 그대로). 뗌은 글을 치는 것이 아니라 고른 영역을 지우거나 맨 아래로 돌리지 않는다
    fn kitty_key_release(&mut self, event: &winit::event::KeyEvent) {
        let Some(pane) = self.w.kitty_down.remove(&event.physical_key) else {
            return;
        };
        let Some(modes) = self.active_pane().filter(|t| t.id == pane).map(|t| t.session.modes()) else {
            return;
        };
        if !modes.kitty.report_events {
            return;
        }
        let meta = keys::option_counts_as_alt(self.config.option_as_alt, self.raw_mods);
        let bytes = keys::encode(event, self.mods, keys::to_state(self.raw_mods), modes, meta);
        if let Some(t) = self.active_pane() {
            t.session.write_input(&bytes, false);
        }
    }

    fn write_active(&mut self, bytes: &[u8]) {
        if let Some(id) = self.focused_id() {
            self.write_pane(id, bytes);
        }
    }

    /// 칸 하나에 키 바이트를 쓴다(고른 글 지우기·맨 아래로 돌리기는 `write_active`와 같다)
    fn write_pane(&mut self, id: u64, bytes: &[u8]) {
        // 그 칸에 키가 간다: 벨 테두리를 뗀다(Ghostty `keyDown`, #687)
        if let Some(i) = self.pane_index(id).filter(|&i| self.panes[i].bell_border) {
            self.panes[i].bell_border = false;
            self.request_redraw();
        }
        let scroll = self.config.scroll_to_bottom.keystroke;
        let clear = clears_selection_on_write(self.config.selection_clear_on_typing, bytes);
        // 읽기 전용 칸: 쓰지 않으므로 고른 글도 지우지 않는다(복사할 수 있게, #209)
        if let Some(t) = self
            .pane_index(id)
            .map(|i| &self.panes[i])
            .filter(|t| !t.session.readonly())
        {
            if clear {
                t.session.select_clear();
            }
            t.session.write_input(bytes, scroll);
        }
    }

    /// 인코딩한 키 하나를 키 묶음에 먹이고, 묶음이 먹지 않은 바이트를 칸에 쓰고, 묶음이 고른 동작을 한다.
    /// 사람이 누른 키(`script_pane`이 없다: 고른 칸에 쓴다)와 스크립트의 `send key`(그 칸에 쓴다, #650)가 같은 길을 지난다.
    /// 스크립트의 키는 동작이 나올 때만 그 칸을 고른 칸으로 만든 뒤 한다(동작은 고른 칸에 한다)
    fn feed_key(
        &mut self,
        event_loop: &ActiveEventLoop,
        trigger: Option<crate::keymap::Trigger>,
        bytes: Vec<u8>,
        physical: winit::keyboard::PhysicalKey,
        report_release: bool,
        script_pane: Option<u64>,
    ) {
        // performable: 묶음은 동작을 지금 할 수 있을 때만 키를 먹는다
        // 사람의 키 순서와 스크립트의 키 순서는 나눈다: 한쪽이 쌓은 앞 키가 다른 쪽 칸으로 새지 않게(#650)
        let mut seq = match script_pane {
            None => std::mem::take(&mut self.w.keyseq),
            Some(pane) => {
                let (seq, flush) = crate::keymap::script_sequencer(&mut self.w.script_keyseq, pane);
                if let Some((left_pane, left)) = flush {
                    self.write_pane(left_pane, &left);
                }
                seq
            }
        };
        let keymap = std::mem::take(&mut self.keymap);
        let out = seq.feed_with(&keymap, trigger, bytes, &mut |a| self.can_perform(a));
        self.keymap = keymap;
        match script_pane {
            None => self.w.keyseq = seq,
            Some(pane) => self.w.script_keyseq = Some((pane, seq)),
        }
        if out.action.is_none() && script_pane.is_none() && self.config.mouse_hide_while_typing {
            if let Some(g) = &self.w.gfx {
                g.window.set_cursor_visible(false);
            }
            self.w.pointer_hidden = true;
        }
        if !out.send.is_empty() {
            match script_pane {
                Some(pane) => self.write_pane(pane, &out.send),
                None => self.write_active(&out.send),
            }
            let down = script_pane.or_else(|| self.active_pane().map(|t| t.id));
            if let (true, Some(id)) = (report_release, down) {
                self.w.kitty_down.insert(physical, id);
            }
        }
        if let Some(action) = out.action {
            if let Some(pane) = script_pane {
                self.focus_pane(pane);
            }
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

    /// 모든 탭의 칸을 지금 배치에 맞춘다(창 크기·글꼴·분할이 바뀐 뒤)
    fn resize_all(&mut self) {
        // 칸이 늘거나 줄거나 크기가 바뀌었을 수 있다: 글꼴 묶음을 칸들이 쓰는 크기에 맞춘다
        self.sync_fonts();
        self.apply_step_resize();
        for gi in 0..self.w.tabs.len() {
            for (id, rect) in self.group_layout(gi) {
                let Some(i) = self.pane_index(id) else { continue };
                let size = self.grid_size_for(rect, self.panes[i].font_size);
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
        self.request_redraw();
    }

    /// 고른 칸의 글자 크기만 바꾼다(#422, Ghostty의 `increase_font_size`는 surface 동작이다). 같은 창의 다른 칸과 다른 창은 그대로다.
    /// 창의 기준 크기(탭 줄 등 창 덧그림)도 이 크기가 된다
    fn set_font_size(&mut self, size: f32) {
        let Some(i) = self.focused_id().and_then(|id| self.pane_index(id)) else {
            return;
        };
        let size = bt_core::config::clamp_font_size(size);
        self.panes[i].font_size = size;
        self.w.font_size = size;
        // resize_all이 글꼴 묶음을 맞춘다(새 크기만 만들고 다른 칸의 묶음은 그대로 쓴다)
        self.resize_all();
    }

    /// 고른 칸의 글자 크기(고른 칸이 없으면 창의 기준 크기)
    fn focused_font_size(&self) -> f32 {
        self.focused_id()
            .and_then(|id| self.pane_font_size(id))
            .unwrap_or(self.w.font_size)
    }

    /// 새 창이 열릴 때의 글자 크기(`window-inherit-font-size`). 직전에 고른 창은 지금 창이고 그 창에서 고른 칸의 크기다.
    /// 지금 창이 없으면(btctl 등 창 밖 요청) 마지막으로 보던 창의 고른 칸이다. 퀵 터미널은 늘 설정 `font-size`다
    fn new_window_font_size(&self, quick: bool) -> f32 {
        let size_of = |w: &Win| {
            w.tabs
                .get(w.active)
                .and_then(|g| self.pane_font_size(g.focus))
                .unwrap_or(w.font_size)
        };
        let previous = if quick {
            None
        } else if self.w.gfx.is_some() {
            Some(size_of(&self.w))
        } else {
            let last = self.last_focused;
            std::iter::once(&self.w)
                .chain(&self.others)
                .find(|w| last.is_some() && w.id() == last)
                .map(size_of)
        };
        self.config.new_window_font_size(previous)
    }

    /// 새 칸의 글자 크기. 분할은 고른 칸, 새 탭은 `window-inherit-font-size`를 따른다(`pane_font::new_pane_size`)
    fn new_pane_font_size(&self, kind: pane_font::NewPane) -> f32 {
        let focused = self.focused_id().and_then(|id| self.pane_font_size(id));
        pane_font::new_pane_size(&self.config, kind, focused, self.w.font_size)
    }

    /// 창 좌표 아래의 칸(보고 있는 탭에서)
    fn pane_at(&self, pos: PhysicalPosition<f64>) -> Option<u64> {
        self.group_layout(self.w.active)
            .into_iter()
            .find(|(_, r)| r.contains(pos.x, pos.y))
            .map(|(id, _)| id)
    }

    /// 이 탭의 칸 수와 크게 보는 중인가(손잡이를 보일지·잡을지 정한다)
    fn handle_env(&self) -> (usize, bool) {
        match self.w.tabs.get(self.w.active) {
            Some(g) => (g.layout.leaves().len(), g.zoomed),
            None => (0, false),
        }
    }

    /// 마우스가 잡을 수 있는 손잡이 위에 있으면 그 칸(`drag-handle`). 손잡이가 없거나 옮길 칸이 하나뿐이면 없다
    fn handle_under_cursor(&self) -> Option<u64> {
        let (panes, zoomed) = self.handle_env();
        if !crate::pane_drag::grabbable(self.config.drag_handle, panes, zoomed) {
            return None;
        }
        let pos = self.w.cursor_pos;
        let id = self.pane_at(pos)?;
        let outer = self.pane_rect(id)?;
        crate::pane_drag::handle_rect(outer, self.scale)
            .contains(pos.x, pos.y)
            .then_some(id)
    }

    /// 마우스 아래 칸의 위쪽 띠·손잡이 상태를 다시 읽는다. 바뀌었으면 다시 그린다
    fn update_handle_hover(&mut self) {
        let pos = self.w.cursor_pos;
        let hover = self
            .pane_at(pos)
            .and_then(|id| {
                let outer = self.pane_rect(id)?;
                crate::pane_drag::hover_band(outer, self.scale)
                    .contains(pos.x, pos.y)
                    .then(|| {
                        (
                            id,
                            crate::pane_drag::handle_rect(outer, self.scale).contains(pos.x, pos.y),
                        )
                    })
            })
            .filter(|_| self.config.drag_handle != bt_core::config::DragHandle::Never);
        if hover != self.w.handle_hover {
            self.w.handle_hover = hover;
            self.request_redraw();
        }
    }

    /// 끌던 칸을 마우스 자리에 놓으면 어디에 놓이는가
    fn drop_target_at_cursor(&self) -> Option<crate::pane_drag::Drop> {
        let from = self.w.pane_drag.from()?;
        let pos = self.w.cursor_pos;
        crate::pane_drag::drop_at(&self.group_layout(self.w.active), from, pos.x, pos.y)
    }

    /// 경계선 맞히는 띠가 경계선 밖으로 나가는 폭(픽셀). Ghostty는 선 1pt에 보이지 않는 6pt를 더해 가운데에 둔다(양쪽 3pt)
    fn divider_reach_px(&self) -> u32 {
        (3.0 * self.scale).round() as u32
    }

    /// 마우스가 올라간 분할 경계선. 크게 보는 중이면 경계선이 없다
    fn divider_under_cursor(&self) -> Option<crate::split::DividerInfo> {
        let g = self.w.tabs.get(self.w.active).filter(|g| !g.zoomed)?;
        let infos = g.layout.divider_infos(self.content_rect(), self.divider_px());
        let pos = self.w.cursor_pos;
        crate::split::hit_divider(&infos, pos.x, pos.y, self.divider_reach_px()).cloned()
    }

    /// 포인터 모양에 알릴 경계선 상태: 끄는 중이면 그 경계선의 방향, 아니면 마우스가 올라간 경계선의 방향
    fn divider_cursor(&self) -> crate::pointer::DividerCursor {
        use crate::pointer::DividerCursor;
        let axis = match self.w.divider.drag() {
            Some(d) => Some(d.axis),
            None if self.w.pane_drag.is_active() => None,
            None => self.divider_under_cursor().map(|d| d.axis),
        };
        match axis {
            Some(crate::split::Axis::Horizontal) => DividerCursor::Col,
            Some(crate::split::Axis::Vertical) => DividerCursor::Row,
            None => DividerCursor::None,
        }
    }

    /// 경계선을 눌렀다: 끌기를 시작하고, 두 번째 누름이면 칸 크기를 고르게 한다(Ghostty `onEqualize`는 그 탭의 분할 트리 전체)
    fn divider_press(&mut self, d: crate::split::DividerInfo) {
        let interval = mouse::repeat_interval(self.config.click_repeat_interval, mouse::platform_repeat_interval());
        let drag = crate::divider_drag::Drag {
            group: self.w.active,
            path: d.path,
            axis: d.axis,
        };
        let at = (self.w.cursor_pos.x, self.w.cursor_pos.y);
        if self.w.divider.press(drag, at, Instant::now(), interval) {
            if let Some(g) = self.w.tabs.get_mut(self.w.active) {
                g.layout.equalize();
            }
            self.resize_all();
        }
        self.refresh_pointer();
        self.request_redraw();
    }

    /// 경계선을 끄는 중 마우스가 움직였다: 그 경계선의 비율만 바꾸고 칸 크기를 맞춘다(키로 바꿀 때와 같은 `resize_all`).
    /// 탭이 바뀌었거나 크게 보는 중이거나 트리가 바뀌어 경계선이 없으면 끌기를 멈춘다. 단추만 누르고 움직이지 않으면 바꾸지 않는다
    fn divider_drag_to(&mut self, pos: PhysicalPosition<f64>) {
        let Some(drag) = self.w.divider.drag().cloned() else {
            return;
        };
        // Ghostty `DragGesture()` 기본 문턱 10pt. 제자리 누름·놓기는 비율을 바꾸지 않는다(두 번 눌러 고르게 한 것이 남게)
        if !self.w.divider.follows(pos.x, pos.y, 10.0 * self.scale) {
            return;
        }
        let area = self.content_rect();
        let divider = self.divider_px();
        let min_px = (10.0 * self.scale).round() as u32;
        let changed = self
            .w
            .tabs
            .get_mut(drag.group)
            .filter(|g| drag.group == self.w.active && !g.zoomed)
            .and_then(|g| {
                let info = g
                    .layout
                    .divider_infos(area, divider)
                    .into_iter()
                    .find(|i| i.path == drag.path)?;
                let ratio = crate::split::ratio_for_pointer(&info, divider, pos.x, pos.y, min_px)?;
                g.layout.set_ratio_at(&drag.path, ratio).then_some(())
            });
        match changed {
            Some(()) => self.resize_all(),
            None => {
                let alive = self.w.tabs.get(drag.group).is_some_and(|g| {
                    drag.group == self.w.active
                        && !g.zoomed
                        && g.layout
                            .divider_infos(area, divider)
                            .iter()
                            .any(|i| i.path == drag.path)
                });
                if !alive {
                    self.w.divider.end();
                    self.refresh_pointer();
                }
            }
        }
    }

    /// 끌기를 마쳤다. 옮기기면 트리를 바꾸고 PTY 크기를 맞추고 옮긴 칸을 고른다
    fn finish_pane_drag(&mut self, outcome: Option<crate::pane_drag::Outcome>) {
        if let Some(crate::pane_drag::Outcome::Move { from, to, dir }) = outcome {
            let moved = self.group_of(from) == Some(self.w.active)
                && self
                    .w
                    .tabs
                    .get_mut(self.w.active)
                    .is_some_and(|g| !g.zoomed && g.layout.move_pane(from, to, dir));
            if moved {
                self.resize_all();
                self.focus_pane(from);
            }
        }
        self.refresh_pointer();
        self.request_redraw();
    }

    /// 창 좌표 → 칸 idx의 격자 칸. 칸 밖이면 가장자리 칸으로 붙인다(고르며 끌다 칸을 벗어나도)
    fn cell_in(&self, idx: usize, pos: PhysicalPosition<f64>) -> Option<(usize, usize)> {
        let pane = self.panes.get(idx)?;
        // 그 칸의 글자 크기의 칸 크기로 잰다(#422)
        let m = self.metrics_at(pane.font_size)?;
        let r = self.pane_inner(pane.id)?;
        let (cols, rows) = pane_cells(&pane.session);
        let col = ((pos.x - r.x as f64) / m.width as f64).floor().max(0.0) as usize;
        let row = ((pos.y - r.y as f64) / m.height as f64).floor().max(0.0) as usize;
        Some((col.min(cols.saturating_sub(1)), row.min(rows.saturating_sub(1))))
    }

    /// 창 좌표가 `cell_in`이 구한 칸의 어느 쪽인가(#437). 글자 고르기가 누른 칸과 끝 칸을 넣을지 이 쪽으로 정한다
    fn cell_side_in(&self, idx: usize, pos: PhysicalPosition<f64>) -> CellSide {
        let Some(pane) = self.panes.get(idx) else {
            return CellSide::Left;
        };
        let (Some(m), Some(r)) = (self.metrics_at(pane.font_size), self.pane_inner(pane.id)) else {
            return CellSide::Left;
        };
        let (cols, _) = pane_cells(&pane.session);
        mouse::cell_side(pos.x - r.x as f64, m.width, cols)
    }

    /// 휠이 가리키는 칸의 칸 크기: 마우스 아래 칸, 없으면 고른 칸, 그것도 없으면 창의 기준 크기
    fn wheel_cell(&self) -> Option<bt_render::font::CellMetrics> {
        self.pane_at(self.w.cursor_pos)
            .or_else(|| self.focused_id())
            .and_then(|id| self.pane_metrics(id))
            .or_else(|| self.chrome_metrics())
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
        if !self.pane_inner(id)?.contains(pos.x, pos.y) {
            return None;
        }
        let (c, r) = self.cell_in(i, pos)?;
        let opts = bt_term::links::LinkOptions {
            url: self.config.link_url,
            osc8: self.config.link_osc8,
        };
        bt_term::links::link_at_with(&self.panes[i].session.snapshot(), c, r, opts).map(|h| (id, h))
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
        self.w.hover = next;
        self.refresh_pointer();
        self.request_redraw();
    }

    /// 지금 누른 수식 키가 사각 고르기 키인가(#433). macOS는 Option, 그 밖에는 Ctrl(또는 Super)+Alt
    fn rectangle_select_mods(&self) -> bool {
        mouse::is_rectangle_select(
            cfg!(target_os = "macos"),
            self.mods.alt_key(),
            self.mods.control_key(),
            self.mods.super_key(),
        )
    }

    /// 창의 마우스 포인터 모양을 맞춘다(#239): 링크 위면 손가락, 아니면 마우스 아래 칸이 OSC 22로 정한 모양, 없으면 마우스 보고 모드에 따라 I자·화살표
    fn refresh_pointer(&mut self) {
        let pos = self.w.cursor_pos;
        // 마우스가 글 영역 위에 있는 칸. 모양·Shift는 거기서만 일한다(#250)
        let over = self.pane_at(pos).and_then(|id| {
            let inside = self.pane_inner(id)?.contains(pos.x, pos.y);
            inside.then(|| self.pane_index(id)).flatten()
        });
        let program = over.and_then(|i| self.panes.get(i)?.session.mouse_shape());
        let icon = crate::pointer::decide(crate::pointer::PointerInput {
            over_link: self.w.hover.is_some(),
            program,
            shift: over.is_some() && self.mods.shift_key(),
            rectangle: over.is_some() && self.rectangle_select_mods(),
            reports_to_program: over.is_some_and(|i| self.mouse_reporting(i).is_some()),
            mouse_mode: over.is_some_and(|i| {
                self.panes
                    .get(i)
                    .is_some_and(|p| Self::mouse_mode_on(p.session.modes()))
            }),
            hidden: self.w.pointer_hidden,
            divider: self.divider_cursor(),
            handle: if self.w.pane_drag.is_active() {
                crate::pointer::HandleCursor::Dragging
            } else if self.handle_under_cursor().is_some() {
                crate::pointer::HandleCursor::Over
            } else {
                crate::pointer::HandleCursor::None
            },
        });
        if icon != self.w.pointer_icon {
            if let Some(g) = &self.w.gfx {
                g.window.set_cursor(icon);
            }
            self.w.pointer_icon = icon;
        }
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

    /// 찾을 글을 정해 찾기를 시작한다(`search:글`·`search_selection`). 막대가 없으면 열고, 있으면 글을 바꾼다.
    /// 빈 글이면 찾기만 취소하고 막대는 닫지 않는다(Ghostty). 막대가 없는데 빈 글이면 아무것도 하지 않는다
    fn search_with(&mut self, raw: &str) {
        let text = search_text(raw);
        if text.is_empty() && self.w.search.is_none() {
            return;
        }
        let Some(id) = self.focused_id() else { return };
        if self.w.search.as_ref().is_some_and(|s| s.pane != id) {
            self.end_search();
        }
        match self.w.search.as_mut() {
            Some(s) => s.query = text,
            None => {
                self.w.search = Some(SearchBar {
                    pane: id,
                    query: text,
                    status: None,
                })
            }
        }
        self.search_query_changed();
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
                // 셸을 거치지 않고 인자 배열로 연다(#209)
                let argv = crate::toggles::config_open_argv(&path, cfg!(target_os = "macos"));
                if let Some((program, args)) = argv.split_first() {
                    match std::process::Command::new(program).args(args).spawn() {
                        Ok(mut child) => {
                            // 끝난 프로세스를 거둔다(기다리는 스레드가 따로 있어야 좀비가 남지 않는다)
                            std::thread::spawn(move || {
                                let _ = child.wait();
                            });
                            self.toast("설정을 고친 뒤 Cmd-Shift-, 로 다시 읽습니다".into());
                        }
                        Err(e) => self.toast(format!("설정 파일을 열지 못했습니다: {e}")),
                    }
                }
            }
            Err(e) => {
                log::warn!("설정 파일을 만들지 못했습니다: {e}");
                self.toast(format!("설정 파일을 만들지 못했습니다: {e}"));
            }
        }
    }

    /// 지금 창의 바탕 투명도를 설정값과 불투명 사이에서 바꾼다(Ghostty `toggle_background_opacity`, #209).
    /// `background-opacity`가 1 이상이면 아무것도 하지 않는다
    fn toggle_background_opacity(&mut self) {
        if !crate::toggles::can_toggle_opacity(self.config.background_opacity) {
            return;
        }
        self.w.opaque = !self.w.opaque;
        // 배경 그림 층과 앞 프레임은 옛 투명도로 만든 것이라 다시 만든다
        self.w.backdrop = None;
        self.w.last_frame = None;
        self.toast(
            if self.w.opaque {
                "바탕 불투명"
            } else {
                "바탕 투명"
            }
            .into(),
        );
    }

    /// 지금 칸의 마우스 보고를 끄고 켠다(Ghostty `toggle_mouse_reporting`, #209)
    fn toggle_mouse_reporting(&mut self) {
        let config = self.config.mouse_reporting;
        let Some(i) = self.active_index() else { return };
        let next = crate::toggles::toggled_mouse_reporting(config, self.panes[i].mouse_reporting_over);
        self.panes[i].mouse_reporting_over = next;
        let on = crate::toggles::mouse_reporting_on(config, next);
        self.toast(
            if on {
                "마우스 보고 켬"
            } else {
                "마우스 보고 끔"
            }
            .into(),
        );
    }

    /// 지금 칸을 읽기 전용으로 하거나 푼다(Ghostty `toggle_readonly`, #209, ADR-018)
    fn toggle_readonly(&mut self) {
        let Some(i) = self.active_index() else { return };
        let on = !self.panes[i].session.readonly();
        self.panes[i].session.set_readonly(on);
        if on {
            // 쓰다 만 조합 글자와 이 칸에 걸린 붙여넣기 확인은 더는 갈 곳이 없다
            self.w.preedit.clear();
            if matches!(&self.w.confirm, Some(c) if matches!(&c.pending, Pending::Paste { pane, .. } | Pending::ClipboardRead { pane, .. } | Pending::KittyRead { pane, .. } | Pending::KittyWrite { pane, .. } if *pane == self.panes[i].id))
            {
                self.w.confirm = None;
                self.sync_confirm_ime();
            }
        }
        self.toast(
            if on {
                "읽기 전용: 이 칸에는 입력이 가지 않습니다"
            } else {
                "읽기 전용을 풀었습니다"
            }
            .into(),
        );
    }

    /// 명령 팔레트가 열려 있으면 키를 받는다(Cmd·Ctrl 조합은 키 묶음에 맡긴다). 받았으면 참
    fn palette_key(&mut self, event: &winit::event::KeyEvent, event_loop: &ActiveEventLoop) -> bool {
        use winit::keyboard::{Key, NamedKey};
        if self.w.palette.is_none() || self.mods.super_key() || self.mods.control_key() {
            return false;
        }
        let entries = self.palette_entries();
        let Some((query, selected)) = self.w.palette.as_mut() else {
            return false;
        };
        let shown = crate::palette::filter(&entries, query);
        match &event.logical_key {
            Key::Named(NamedKey::Escape) => self.w.palette = None,
            Key::Named(NamedKey::ArrowDown) => *selected = crate::palette::move_selection(*selected, shown.len(), true),
            Key::Named(NamedKey::ArrowUp) => *selected = crate::palette::move_selection(*selected, shown.len(), false),
            Key::Named(NamedKey::Backspace) => {
                query.pop();
                *selected = crate::palette::selection_after_edit(query, *selected);
            }
            Key::Named(NamedKey::Enter) => {
                let pick = crate::palette::picked(*selected, shown.len())
                    .and_then(|k| shown.get(k))
                    .map(|&i| entries[i].command.clone());
                self.w.palette = None;
                match pick {
                    Some(crate::palette::Command::Action(a)) => self.handle_action(a, event_loop),
                    Some(crate::palette::Command::OpenSettings) => self.open_settings(),
                    Some(crate::palette::Command::Focus(pane)) => self.on_note_clicked(event_loop, pane),
                    None => {}
                }
            }
            _ => {
                if let Some(text) = event.text.as_ref().filter(|t| !t.chars().any(char::is_control)) {
                    if query.chars().count() < 64 {
                        query.push_str(text);
                        *selected = crate::palette::selection_after_edit(query, *selected);
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
            Action::Copy(_)
            | Action::AdjustSelection(_)
            | Action::ScrollToSelection
            | Action::SearchSelection
            | Action::WriteFile(crate::keymap::FileSource::Selection, _) => has_selection(self),
            Action::PasteSelection => self.selection_clipboard_text().is_some(),
            Action::Paste => self
                .clipboard
                .as_mut()
                .and_then(|cb| cb.get_text().ok())
                .is_some_and(|t| !t.is_empty()),
            Action::CopyUrl => self.link_under(self.w.cursor_pos).is_some(),
            // Ghostty Surface.zig: 찾는 중이 아니면 다음·앞 찾기를 하지 않고 키를 프로그램에 넘긴다
            Action::EndSearch | Action::NavigateSearch(_) => self.w.search.is_some(),
            // Ghostty Surface.zig: 대체 화면(vim·less 등)에서는 화면 지우기를 하지 않고 키를 프로그램에 넘긴다
            Action::ClearScreen => self.active_pane().is_some_and(|t| !t.session.modes().alt_screen),
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
                Action::Quit
                | Action::ToggleQuickTerminal
                | Action::ReloadConfig
                | Action::SetWindowTheme(_)
                | Action::OpenConfig => {}
                _ => return,
            }
        }
        match action {
            Action::Copy(format) => {
                self.copy_selection_command(format);
            }
            // 찾기 막대가 열려 있으면 첫 줄을 찾을 글에 붙인다
            Action::Paste if self.w.search.is_some() => {
                let text = self.clipboard.as_mut().and_then(|cb| cb.get_text().ok());
                if let Some(line) = text.as_deref().and_then(|t| t.lines().next()) {
                    self.search_push(line);
                }
            }
            Action::PasteSelection if self.w.search.is_some() => {
                let line = self
                    .selection_clipboard_text()
                    .and_then(|t| t.lines().next().map(str::to_owned));
                if let Some(line) = line {
                    self.search_push(&line);
                }
            }
            Action::StartSearch => self.start_search(),
            Action::ToggleQuickTerminal => self.toggle_quick(event_loop),
            Action::TogglePalette => {
                self.w.palette = match self.w.palette {
                    Some(_) => None,
                    // Ghostty처럼 처음에는 아무것도 고르지 않는다(Enter가 아무 일도 하지 않음)
                    None => Some((String::new(), None)),
                };
            }
            Action::EndSearch => self.end_search(),
            Action::NavigateSearch(older) => self.search_step(older),
            Action::Paste => {
                if let Some(text) = self.clipboard.as_mut().and_then(|cb| cb.get_text().ok()) {
                    self.paste(text);
                }
            }
            // 선택 클립보드 글을 시스템 클립보드와 같은 길(붙여넣기 보호)로 붙여넣는다. 비었으면 하지 않는다
            Action::PasteSelection => {
                if let Some(text) = self.selection_clipboard_text() {
                    self.paste(text);
                }
            }
            Action::FontBigger(n) => self.set_font_size(self.focused_font_size() + n),
            Action::FontSmaller(n) => self.set_font_size(self.focused_font_size() - n),
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
            Action::CloseWindow => self.request(Pending::CloseWindow, event_loop),
            Action::CloseAllWindows => self.request(Pending::CloseAllWindows, event_loop),
            Action::GotoWindow(delta) => self.goto_window(delta),
            Action::MoveTab(delta) => self.move_tab(delta),
            Action::MoveTabToNewWindow => self.move_tab_to_new_window(event_loop),
            Action::ResetWindowSize => self.reset_window_size(),
            Action::ToggleWindowDecorations => self.toggle_window_decorations(),
            Action::ToggleVisibility => self.toggle_visibility(),
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
                    self.w.title_prompt = Some((TitleTarget::Surface(id), now));
                }
            }
            Action::PromptTabTitle => {
                if let Some(g) = self.w.tabs.get(self.w.active) {
                    let (id, now) = (g.focus, g.title.clone().unwrap_or_default());
                    self.w.palette = None;
                    self.w.title_prompt = Some((TitleTarget::Tab(id), now));
                }
            }
            Action::PromptWindowTitle => {
                let now = self.w.window_title.clone().unwrap_or_default();
                self.w.palette = None;
                self.w.title_prompt = Some((TitleTarget::Window, now));
            }
            Action::SetTabTitle(text) => {
                if let Some(id) = self.focused_id() {
                    self.apply_title(TitleTarget::Tab(id), &text);
                }
            }
            Action::SetWindowTitle(text) => self.apply_title(TitleTarget::Window, &text),
            Action::CopyTitle => {
                let text = self
                    .active_pane()
                    .and_then(|p| title_to_copy(p.user_title.as_deref(), &p.title));
                if let Some(text) = text {
                    if let Some(cb) = self.clipboard.as_mut() {
                        let _ = cb.set_text(text.clone());
                    }
                    if self.config.app_notifications.clipboard_copy {
                        self.toast(format!("제목을 복사했습니다: {text}"));
                    }
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
                    let zoomed = self.w.tabs.get(self.w.active).is_some_and(|g| g.zoomed);
                    let keep = self.config.split_preserve_zoom.keeps_zoom_on_goto(zoomed);
                    self.focus_pane_keeping(id, keep);
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
                    let (cols, rows) = pane_cells(&t.session);
                    t.session.select_start(0, 0, SelectKind::Line);
                    t.session.select_update(cols.saturating_sub(1), rows.saturating_sub(1));
                }
            }
            Action::ReloadConfig => self.reload_config(),
            Action::Reset => {
                if let Some(t) = self.active_pane() {
                    t.session.reset();
                }
            }
            Action::ScrollToRow(row) => {
                if let Some(t) = self.active_pane() {
                    t.session.scroll_to_row(row);
                }
            }
            Action::ScrollToSelection => {
                if let Some(t) = self.active_pane() {
                    t.session.scroll_to_selection();
                }
            }
            Action::Search(text) => self.search_with(&text),
            Action::SearchSelection => {
                if let Some(text) = self.active_pane().and_then(|t| t.session.selection_text()) {
                    self.search_with(&text);
                }
            }
            Action::SetFontSize(size) => self.set_font_size(size),
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
            // 키 묶음에서 온 키 테이블 동작은 키 판단에서 이미 했다. 여기는 명령 팔레트(`command-palette-entry`)에서 오는 길이다
            Action::ActivateKeyTable(_)
            | Action::ActivateKeyTableOnce(_)
            | Action::DeactivateKeyTable
            | Action::DeactivateAllKeyTables
            | Action::EndKeySequence => {
                let (_, flush) = self.w.keyseq.apply_table_action(&self.keymap, &action);
                if !flush.is_empty() {
                    self.write_active(&flush);
                }
            }
            Action::ToggleBackgroundOpacity => self.toggle_background_opacity(),
            Action::ToggleMouseReporting => self.toggle_mouse_reporting(),
            Action::ToggleReadonly => self.toggle_readonly(),
            Action::SetWindowTheme(theme) => {
                self.theme_override = theme;
                self.apply_window_look();
            }
            Action::OpenConfig => self.open_settings(),
            Action::ToggleSecureInput => self.toggle_pane_secure_input(),
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

    /// `custom-shader` 파일을 읽어 열린 GPU 길 창마다 다시 건다(#376). 읽기·컴파일 오류는 로그와 설정 경고로 남기고
    /// 그 셰이더만 건너뛴다. 창의 셰이더 시간은 0부터 다시 간다
    fn load_shaders(&mut self) {
        let (files, warnings) = shader_inputs::load_shader_files(&self.config.custom_shader);
        for w in warnings {
            log::warn!("설정: {w}");
            self.config.warnings.push(w);
        }
        self.shaders = files
            .into_iter()
            .map(|(name, glsl)| bt_gpu::CustomShaderSource { name, glsl })
            .collect();
        let wake = self.shader_waker();
        let mut errors = Vec::new();
        for w in std::iter::once(&mut self.w).chain(&mut self.others) {
            let Some(g) = w.gfx.as_mut() else { continue };
            errors.extend(apply_shaders(g, &self.shaders, &wake));
            w.shader = Default::default();
            g.window.request_redraw();
        }
        self.note_shader_errors(errors);
    }

    /// 컴파일이 끝나면 이벤트 루프를 깨우는 함수(컴파일 스레드에서 불린다, #381)
    fn shader_waker(&self) -> bt_gpu::ShaderWaker {
        let proxy = self.proxy.clone();
        std::sync::Arc::new(move || {
            let _ = proxy.send_event(UserEvent::ShaderCompiled);
        })
    }

    /// 컴파일이 끝난 창의 결과를 받아 끼우고, 그 창을 다시 그리고 셰이더 시간을 0부터 센다. 오류는 지금처럼 설정 경고로 남긴다(#381).
    /// 아직 안 끝난 창은 그대로 둔다(기다리지 않는다)
    fn poll_shaders(&mut self) {
        let mut errors = Vec::new();
        for w in std::iter::once(&mut self.w).chain(&mut self.others) {
            let Some(g) = w.gfx.as_mut() else { continue };
            let Some(gpu) = g.gpu.as_mut() else { continue };
            if let Some(e) = gpu.poll_custom_shaders() {
                errors.extend(e);
                w.shader = Default::default();
                g.window.request_redraw();
            }
        }
        self.note_shader_errors(errors);
    }

    /// 셰이더를 걸다 난 오류를 설정 경고에 더한다(`btctl reload-config`와 설정 알림에 보인다). 창마다 같은 문구는 한 번만
    fn note_shader_errors(&mut self, errors: Vec<String>) {
        for e in errors {
            let w = format!("custom-shader: {e}");
            if !self.config.warnings.contains(&w) {
                self.config.warnings.push(w);
            }
        }
    }

    fn reload_config(&mut self) {
        let next = AppConfig::load_with(
            self.config_extra.as_deref(),
            crate::system_is_dark(),
            self.config_default_files,
        );
        for w in &next.warnings {
            log::warn!("설정: {w}");
        }
        let mut next = next;
        self.applied_dark = crate::system_is_dark();
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
            w.script_keyseq = None;
        }
        // 설정의 `font-size`가 바뀌었으면 모든 칸(과 창의 기준 크기)을 새 값으로, 아니면 칸마다 고른 크기를 둔다(#416·#422)
        let (old_size, new_size) = (self.config.font_size, next.font_size);
        let current: Vec<f32> = self.panes.iter().map(|p| p.font_size).collect();
        for (p, size) in self
            .panes
            .iter_mut()
            .zip(pane_font::sizes_after_reload(old_size, new_size, &current))
        {
            p.font_size = size;
        }
        for w in std::iter::once(&mut self.w).chain(&mut self.others) {
            w.font_size = bt_core::config::font_size_after_reload(old_size, new_size, w.font_size);
        }
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
        // AppleScript를 껐다 켰으면 vendor에도 알린다(#634)
        #[cfg(target_os = "macos")]
        self.sync_scripting_enabled();
        self.apply_window_look();
        self.refresh_mods();
        apply_hidden_from_dock(self.config.macos_hidden);
        icon::apply(self.config.macos_icon, self.config.macos_custom_icon.as_deref());
        self.load_background();
        self.load_shaders();
        // 열려 있는 GPU 창은 다음 프레임부터 새 섞기 공간으로 그린다(#405)
        for w in std::iter::once(&mut self.w).chain(&mut self.others) {
            let Some(g) = w.gfx.as_mut() else { continue };
            if let Some(gpu) = g.gpu.as_mut() {
                gpu.set_alpha_blending(self.config.alpha_blending);
                g.window.request_redraw();
            }
        }
        // 설정을 다시 읽으면 시스템 글꼴을 새로 훑는다(한 번). 창마다 글꼴 묶음을 다시 만들지만 글꼴 파일은 나눠 쓴다
        bt_render::forget_system_fonts();
        self.for_each_window(|app| {
            app.load_fonts();
            app.resize_all();
        });
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

    /// 시스템 밝음·어두움이 바뀌면 `theme = light:…,dark:…`를 다시 골라 열린 모든 칸에 쓰고 창 모양을 맞춘다(#221).
    /// 설정 파일은 다시 읽지 않는다. 판단은 `appearance::should_repick`
    /// 칸의 색 구성표(`CSI ? 996 n`의 답): 창 테마가 밝음·어두움으로 고정이면 그쪽, 아니면 시스템 모양(#232)
    fn scheme_dark(&self) -> bool {
        match crate::toggles::window_theme_mode(self.config.window_theme, self.theme_override) {
            bt_core::config::WindowTheme::Light => false,
            bt_core::config::WindowTheme::Dark => true,
            _ => self.reported_dark,
        }
    }

    fn system_look_changed(&mut self, dark: bool) {
        let forced = crate::appearance::window_theme_forced(crate::toggles::window_theme_mode(
            self.config.window_theme,
            self.theme_override,
        ));
        // 모드 2031이 켜진 칸에 알린다. 테마가 하나뿐이어도 알린다(Ghostty와 같다). 같은 값은 세션이 걸러 낸다
        if crate::appearance::should_report(forced, self.reported_dark, dark) {
            self.reported_dark = dark;
            for t in &self.panes {
                t.session.set_color_scheme(dark);
            }
        }
        if !crate::appearance::should_repick(self.config.theme_split, forced, self.applied_dark, dark) {
            return;
        }
        self.applied_dark = dark;
        self.config.repick_theme(dark);
        for t in &self.panes {
            t.session.set_theme(self.config.theme.clone());
        }
        self.apply_window_look();
        self.request_redraw_all();
    }

    /// 눌린 수정 키에 `key-remap`을 적용해 `mods`를 정한다(설정을 다시 읽을 때도 한다)
    fn refresh_mods(&mut self) {
        self.mods = keys::remapped_state(&self.config.key_remap, self.raw_mods);
    }

    /// 칸의 마우스 보고 모드(X10·누름·버튼·움직임)가 켜져 있는가. Shift·`mouse-reporting`·읽기 전용과 상관없이 모드만 본다(#459)
    fn mouse_mode_on(m: bt_term::Modes) -> bool {
        m.mouse_x10 || m.mouse_report || m.mouse_drag || m.mouse_motion
    }

    /// 칸 idx의 프로그램이 마우스 보고를 켰으면 그 모드(사람의 Shift로 가린다)
    fn mouse_reporting(&self, idx: usize) -> Option<bt_term::Modes> {
        self.mouse_reporting_with(idx, self.mods.shift_key())
    }

    /// 사람의 마우스 처리에 넘길 입력과 상태. 창(`selecting`·`last_click`·휠 누적)과 칸 `idx`(`mouse_down`·`mouse_last_cell`)에 흩어진
    /// 사람의 상태를 한 `PtrState`로 모은다. 처리가 끝나면 `store_human_ptr`가 바뀐 것만 돌려놓는다
    fn human_ptr(&self, idx: Option<usize>) -> (mouse_core::PtrIn, crate::script_mouse::PtrState) {
        let input = mouse_core::PtrIn {
            pos: self.w.cursor_pos,
            mods: crate::script_mouse::PtrMods::from_state(self.mods),
            script: false,
        };
        let pane = idx.and_then(|i| self.panes.get(i));
        let st = crate::script_mouse::PtrState {
            selecting: self.w.selecting,
            link_word: self.w.link_word,
            last_click: self.w.last_click,
            down: pane.map_or(0, |p| p.mouse_down),
            last_cell: pane.and_then(|p| p.mouse_last_cell),
            accum: self.w.scroll_accum,
            accum_x: self.w.scroll_accum_x,
        };
        (input, st)
    }

    /// `human_ptr`로 모은 상태에서 처리가 바꾼 값만 사람의 상태에 돌려놓는다(바꾸지 않은 값은 쓰지 않는다)
    fn store_human_ptr(
        &mut self,
        idx: Option<usize>,
        before: &crate::script_mouse::PtrState,
        after: &crate::script_mouse::PtrState,
    ) {
        if after.selecting != before.selecting {
            self.w.selecting = after.selecting;
        }
        if after.last_click != before.last_click {
            self.w.last_click = after.last_click;
        }
        if after.link_word != before.link_word {
            self.w.link_word = after.link_word;
        }
        if after.accum != before.accum {
            self.w.scroll_accum = after.accum;
        }
        if after.accum_x != before.accum_x {
            self.w.scroll_accum_x = after.accum_x;
        }
        if let Some(p) = idx.and_then(|i| self.panes.get_mut(i)) {
            if after.down != before.down {
                p.mouse_down = after.down;
            }
            if after.last_cell != before.last_cell {
                p.mouse_last_cell = after.last_cell;
            }
        }
    }

    /// 창과 그림판을 만든다(첫 창, 새 창)
    fn create_gfx(&mut self, event_loop: &ActiveEventLoop) -> Option<Gfx> {
        self.create_gfx_with(event_loop, false)
    }

    /// 창을 만든다. `quick`이면 퀵 터미널 모양(테두리 없음, 항상 위, 주 화면 맨 위에 폭 전체·높이 40%)으로 처음부터 만든다.
    /// 만든 뒤에 테두리를 바꾸면 macOS가 포커스를 잠깐 거둬 곧바로 숨는다(#95)
    fn create_gfx_with(&mut self, event_loop: &ActiveEventLoop, quick: bool) -> Option<Gfx> {
        let made = Instant::now();
        // 화면 색 공간으로 넘길지(ADR-009). 새 그림판부터 따른다
        softbuffer::set_native_color_space(!self.config.colorspace_srgb);
        // 투명 창(#78): 창을 투명으로 만들고 그림판이 알파를 넘기게 한다
        let transparent = self.config.background_opacity < 1.0;
        softbuffer::set_transparent(transparent);
        // 실험 E4: BT_E4_NO_REUSE가 있으면 버퍼 다시 쓰기를 끈다(견주어 재려고)
        softbuffer::set_reuse_buffers(std::env::var_os("BT_E4_NO_REUSE").is_none());
        // 실험 E5: BT_E5_CGIMAGE가 있으면 예전 CGImage 길로 올린다(기본은 IOSurface, 견주어 재려고)
        softbuffer::set_iosurface(std::env::var_os("BT_E5_CGIMAGE").is_none());
        // 창 모양(#192): window-decoration·macos-titlebar-style·macos-window-shadow·window-theme
        let look = crate::windeco::window_look(
            quick,
            cfg!(target_os = "macos"),
            self.config.window_decoration,
            self.config.macos_titlebar_style,
            self.config.macos_window_buttons == bt_core::config::MacWindowButtons::Hidden,
            self.config.macos_window_shadow,
        );
        let attrs = Window::default_attributes()
            .with_title("b-terminal")
            .with_inner_size(LogicalSize::new(DEFAULT_WINDOW_LOGICAL.0, DEFAULT_WINDOW_LOGICAL.1))
            .with_min_inner_size(LogicalSize::new(320.0, 200.0))
            .with_transparent(transparent)
            .with_decorations(look.decorations)
            .with_theme(self.window_theme_choice());
        // 퀵 창은 테두리 없이 맨 위 층에, 보이지 않게 만든다. 자리를 잡고 보이는 일은 show_quick이 한다(#110)
        let attrs = if quick {
            attrs
                .with_window_level(winit::window::WindowLevel::AlwaysOnTop)
                .with_visible(false)
        } else if crate::a11y::SUPPORTED {
            // 접근성 어댑터는 창이 처음 보이기 전에 붙여야 한다(accesskit_winit의 요구, #560). 아래에서 어댑터를 붙인 바로 뒤에 보인다
            attrs.with_visible(false)
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
        // 제목 막대 모양·그림자(#192). macos-window-buttons = hidden이면 신호등 단추도 숨긴다(#179, look에 들어 있다)
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::WindowAttributesExtMacOS;
            attrs
                .with_titlebar_transparent(look.titlebar_transparent)
                .with_fullsize_content_view(look.fullsize_content)
                .with_titlebar_buttons_hidden(look.buttons_hidden)
                .with_title_hidden(look.title_hidden)
                .with_has_shadow(look.shadow)
        };
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                log::error!("창을 만들지 못했습니다: {e}");
                return None;
            }
        };
        let a11y = crate::a11y::WindowA11y::new(event_loop, &window, self.proxy.clone());
        if crate::a11y::SUPPORTED && !quick {
            window.set_visible(true);
        }
        window.set_ime_allowed(true);
        // 창 뒤 흐림(#238). 퀵 터미널 창도 이 길로 만들어진다
        crate::blur::apply(&window, self.config.background_blur, self.config.background_opacity);
        self.scale = window.scale_factor();
        // 그리기 길 고르기(ADR-036, ADR-039): macOS는 GPU 길이 기본(`BT_GPU=0`이면 CPU, `1`이면 어느 OS든 GPU).
        // 장치를 열면 wgpu surface를, 못 열면 softbuffer를 붙인다. `window-colorspace = srgb`도 GPU 길이다(#466)
        let colorspace_srgb = self.config.colorspace_srgb;
        let transparent_gpu = self.config.background_opacity < 1.0;
        let size = window.inner_size();
        let t_open = Instant::now();
        let (gpu, fallback) = crate::render_path::choose(
            std::env::var("BT_GPU").ok().as_deref(),
            crate::render_path::GPU_BY_DEFAULT,
            || {
                // `srgb`면 macOS는 창 레이어에 sRGB 색 공간을 직접 단다(#632, 추가 텍스처 없음). 달지 못하면 surface를 Display P3로 열고
                // 마지막 변환 패스를 둔다(surface가 Display P3를 안 주면 `None`, CPU 길로 간다)
                #[cfg(target_os = "macos")]
                if colorspace_srgb {
                    let layer_window = window.clone();
                    return bt_gpu::GpuRenderer::new_for_window_srgb(
                        window.clone(),
                        (size.width, size.height),
                        transparent_gpu,
                        self.config.window_vsync,
                        std::sync::Arc::new(move || winit::platform::macos::set_metal_layer_srgb(&layer_window)),
                    )
                    .map(Box::new);
                }
                if colorspace_srgb {
                    bt_gpu::GpuRenderer::new_for_window_srgb_p3(
                        window.clone(),
                        (size.width, size.height),
                        transparent_gpu,
                        self.config.window_vsync,
                    )
                } else {
                    bt_gpu::GpuRenderer::new_for_window_with(
                        window.clone(),
                        (size.width, size.height),
                        transparent_gpu,
                        self.config.window_vsync,
                    )
                }
                .map(Box::new)
            },
        );
        if let Some(why) = fallback {
            log::warn!("GPU 길을 쓰지 못해 CPU 길로 그린다: {why}");
        }
        // 글자를 섞을 색 공간(#405). GPU 길에서만 뜻이 있다: CPU 길은 늘 sRGB 값 그대로 섞는다
        let mut gpu = gpu;
        match gpu.as_mut() {
            Some(g) => g.set_alpha_blending(self.config.alpha_blending),
            None if self.config.alpha_blending != bt_core::AlphaBlending::Native => {
                log::warn!("alpha-blending: CPU 길은 native로만 섞습니다(GPU 길에서만 적용)");
            }
            None => {}
        }
        if gpu.is_some() {
            log::info!(
                "그리기 길: GPU(장치 열기 {:.1}ms)",
                t_open.elapsed().as_secs_f64() * 1000.0
            );
            let mut g = Gfx {
                inset_points: look.inset_points,
                window,
                surface: None,
                _context: None,
                gpu,
                first: FirstFrame::new(made),
                a11y,
            };
            self.note_shader_errors(apply_shaders(&mut g, &self.shaders, &self.shader_waker()));
            return Some(g);
        }
        log::info!("그리기 길: CPU");
        let context = softbuffer::Context::new(window.clone()).ok()?;
        let surface = softbuffer::Surface::new(&context, window.clone()).ok()?;
        let mut g = Gfx {
            inset_points: look.inset_points,
            window,
            surface: Some(surface),
            _context: Some(context),
            gpu: None,
            first: FirstFrame::new(made),
            a11y,
        };
        self.note_shader_errors(apply_shaders(&mut g, &self.shaders, &self.shader_waker()));
        Some(g)
    }

    /// 지금 설정과 바탕색으로 `window-theme`이 창에 강제할 모양(없으면 시스템을 따른다, #192)
    fn window_theme_choice(&self) -> Option<winit::window::Theme> {
        crate::windeco::window_theme(
            crate::toggles::window_theme_mode(self.config.window_theme, self.theme_override),
            self.config.macos_titlebar_style,
            self.config.theme_split,
            self.config.theme.background,
            cfg!(target_os = "macos"),
        )
    }

    /// 설정 다시 읽기에서 열려 있는 창에 바꿀 수 있는 창 모양을 적용한다(#192): 꾸밈 켬·끔, 그림자, 창 밝음·어두움.
    /// 제목 막대 모양(`macos-titlebar-style`)은 새 창에만 적용한다(Ghostty와 같다). 퀵 창의 꾸밈은 늘 끈 채로 둔다
    fn apply_window_look(&self) {
        let theme = self.window_theme_choice();
        let decorated = self.config.window_decoration != bt_core::config::WindowDecoration::None;
        for w in std::iter::once(&self.w).chain(&self.others) {
            let Some(g) = &w.gfx else { continue };
            g.window.set_theme(theme);
            if !w.quick {
                g.window.set_decorations(decorated);
            }
            #[cfg(target_os = "macos")]
            {
                use winit::platform::macos::WindowExtMacOS;
                g.window.set_has_shadow(self.config.macos_window_shadow);
            }
            // 설정을 다시 읽을 때의 흐림 켜기·바꾸기·끄기(#238)
            crate::blur::apply(&g.window, self.config.background_blur, self.config.background_opacity);
        }
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
        // 창 크기 단위는 창의 기준 크기의 칸 크기다(칸마다 크기가 달라도 창은 한 단위로 움직인다)
        let (Some(m), Some(g)) = (self.chrome_metrics(), &self.w.gfx) else {
            return;
        };
        let inc = crate::startup::step_increments(self.config.window_step_resize, m.width, m.height);
        g.window
            .set_resize_increments(inc.map(|(w, h)| PhysicalSize::new(w, h)));
    }

    /// 프로그램의 창 크기 바꾸기 요청(CSI 8 t, 설정 vt-window-resize-allowed가 켜졌을 때만 세션이 알린다, ADR-013).
    /// Ghostty처럼 창에 탭이 하나이고 분할이 없고 퀵 터미널·전체 화면·최대화가 아닐 때만 따른다.
    /// 40열 × 10행보다 작게는 줄이지 않고 화면보다 크게는 키우지 않는다
    fn on_window_resize_request(&mut self, idx: usize, rows: u16, cols: u16) {
        let Some(pane) = self.panes.get(idx) else { return };
        let Some(g) = &self.w.gfx else { return };
        let single = self.w.tabs.len() == 1 && self.w.tabs[0].layout.leaves() == [pane.id];
        #[cfg(target_os = "macos")]
        let simple_fullscreen = {
            use winit::platform::macos::WindowExtMacOS;
            g.window.simple_fullscreen()
        };
        #[cfg(not(target_os = "macos"))]
        let simple_fullscreen = false;
        if !single || self.w.quick || g.window.fullscreen().is_some() || simple_fullscreen || g.window.is_maximized() {
            log::debug!("창 크기 바꾸기 요청을 무시했습니다(탭·분할·퀵·전체 화면·최대화)");
            return;
        }
        // 칸이 하나뿐이라 그 칸의 글자 크기의 칸 크기다
        let Some(m) = self.metrics_at(pane.font_size) else {
            return;
        };
        let px = |pt: f32| (pt as f64 * self.scale).round() as u32;
        let (pad_w, pad_h) = (
            px(self.config.padding_x.0) + px(self.config.padding_x.1),
            px(self.config.padding_y.0) + px(self.config.padding_y.1) + self.top_inset() + self.tabbar_height(),
        );
        let size = pane.session.size();
        let max = match g.window.current_monitor() {
            Some(mon) => {
                let ms = mon.size();
                (
                    (ms.width.saturating_sub(pad_w) / m.width.max(1)).min(u16::MAX as u32) as u16,
                    (ms.height.saturating_sub(pad_h) / m.height.max(1)).min(u16::MAX as u32) as u16,
                )
            }
            None => (size.cols, size.rows),
        };
        let (cols, rows) = bt_term::window_resize_target((cols, rows), (size.cols, size.rows), max);
        let _ = g.window.request_inner_size(PhysicalSize::new(
            m.width * cols as u32 + pad_w,
            m.height * rows as u32 + pad_h,
        ));
    }

    /// 설정의 window-width·window-height(셀)로 지금 창 크기를 맞춘다
    fn apply_initial_size(&self) {
        if let (Some(m), Some(g)) = (self.chrome_metrics(), &self.w.gfx) {
            if let (Some(cols), Some(rows)) = (self.config.window_width, self.config.window_height) {
                let (w, h) = self.window_inner_for(Some((cols as u32, rows as u32)), (m.width, m.height));
                let _ = g.window.request_inner_size(PhysicalSize::new(w, h));
            }
        }
    }

    /// 새 창이 만들어질 안쪽 크기(물리 픽셀): `window-width`·`window-height`(칸)가 있으면 그것, 없으면 기본(#206)
    fn window_inner_for(&self, cells: Option<(u32, u32)>, cell: (u32, u32)) -> (u32, u32) {
        let px = |pt: f32| (pt as f64 * self.scale).round() as u32;
        crate::winact::window_inner_px(
            cells,
            cell,
            (
                px(self.config.padding_x.0) + px(self.config.padding_x.1),
                px(self.config.padding_y.0) + px(self.config.padding_y.1),
            ),
            self.top_inset() + self.tabbar_height(),
            self.scale,
            DEFAULT_WINDOW_LOGICAL,
        )
    }

    /// `reset_window_size`: 창을 새 창이 만들어질 크기로 되돌린다. 전체 화면이면 하지 않는다(Ghostty). 최대화는 풀고 되돌린다
    fn reset_window_size(&mut self) {
        let Some(m) = self.chrome_metrics() else { return };
        let cells = match (self.config.window_width, self.config.window_height) {
            (Some(c), Some(r)) => Some((c as u32, r as u32)),
            _ => None,
        };
        let (w, h) = self.window_inner_for(cells, (m.width, m.height));
        let Some(g) = &self.w.gfx else { return };
        if window_is_fullscreen(&g.window) {
            return;
        }
        if g.window.is_maximized() {
            g.window.set_maximized(false);
        }
        let _ = g.window.request_inner_size(PhysicalSize::new(w, h));
    }

    /// `toggle_window_decorations`: 지금 창의 제목 막대·테두리를 켜고 끈다. 위쪽 비움 높이는 `top_inset`이
    /// 창의 지금 꾸밈 상태(`is_decorated`)에서 그때그때 구하므로 격자만 다시 맞춘다. 퀵 창은 늘 꾸밈이 없다
    fn toggle_window_decorations(&mut self) {
        if self.w.quick {
            return;
        }
        let Some(g) = &self.w.gfx else { return };
        g.window.set_decorations(!g.window.is_decorated());
        self.resize_all();
    }

    /// `goto_window`: 만든 차례로 앞·뒤 창에 포커스를 준다(퀵 창은 건너뛴다). 포커스 사건이 오면 그 창이 지금 창이 된다
    fn goto_window(&mut self, delta: isize) {
        let serials: Vec<u64> = std::iter::once(&self.w)
            .chain(&self.others)
            .filter(|w| w.gfx.is_some() && !w.quick)
            .map(|w| w.serial)
            .collect();
        let Some(target) = crate::winact::pick_window(&serials, self.w.serial, delta) else {
            return;
        };
        let win = std::iter::once(&self.w)
            .chain(&self.others)
            .find(|w| w.serial == target);
        if let Some(g) = win.and_then(|w| w.gfx.as_ref()) {
            g.window.focus_window();
        }
    }

    /// `move_tab`: 지금 탭을 `delta`만큼 옮기고 끝을 넘으면 돌아간다
    fn move_tab(&mut self, delta: isize) {
        let len = self.w.tabs.len();
        let from = self.w.active;
        let to = crate::winact::wrap_index(from, len, delta);
        self.reorder_tab(from, to);
    }

    /// `from` 탭을 `to` 자리로 옮긴다(`move_tab`과 탭 줄 끌기가 같이 쓴다). 고른 탭은 따라간다
    fn reorder_tab(&mut self, from: usize, to: usize) {
        let len = self.w.tabs.len();
        if len < 2 || from >= len || to >= len || to == from {
            return;
        }
        let active = tabbar::active_after_move(self.w.active, from, to);
        let tab = self.w.tabs.remove(from);
        self.w.tabs.insert(to, tab);
        self.w.active = active;
        self.request_redraw();
    }

    /// `move_tab_to_new_window`: 지금 탭을 새 창으로 떼어 낸다. 탭이 하나뿐이면 아무것도 하지 않는다
    /// (AppKit의 "탭을 새 창으로 이동"도 탭이 하나면 꺼져 있다). 칸(셸)은 App이 들고 있어 탭 묶음만 옮긴다
    fn move_tab_to_new_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.w.quick || !crate::winact::can_detach_tab(self.w.tabs.len()) || self.w.active >= self.w.tabs.len() {
            return;
        }
        let Some(gfx) = self.create_gfx_with(event_loop, false) else {
            self.toast("새 창을 만들지 못했습니다".into());
            return;
        };
        let moved = self.w.tabs.remove(self.w.active);
        if self.w.active >= self.w.tabs.len() {
            self.w.active = self.w.tabs.len() - 1;
        }
        // 찾기·제목 입력 막대는 옮긴 칸에 붙어 있었다. 남는 창에서 닫는다
        let leaves = moved.layout.leaves();
        if self.w.search.as_ref().is_some_and(|s| leaves.contains(&s.pane)) {
            self.end_search();
        }
        // 칸·탭 제목 막대만 옮긴 칸에 매인다. 창 제목 막대는 남는 창의 것이다
        if self.w.title_prompt.as_ref().is_some_and(|(target, _)| match target {
            TitleTarget::Surface(p) | TitleTarget::Tab(p) => leaves.contains(p),
            TitleTarget::Window => false,
        }) {
            self.w.title_prompt = None;
        }
        // 옮긴 칸을 가리키는 확인 막대(칸 닫기·탭 닫기·붙여넣기)도 남는 창에서 닫는다(#223)
        if self.w.confirm.as_ref().is_some_and(|c| c.pending.binds_to(&leaves)) {
            self.w.confirm = None;
            self.sync_confirm_ime();
        }
        let id = gfx.window.id();
        let old_focused = self.w.focused;
        let new_focused = gfx.window.has_focus();
        let moved_focus = moved.focus;
        // 옮긴 탭의 칸들은 보던 크기 그대로 보인다(`window-inherit-font-size`와 상관없이 칸마다 크기를 그대로 둔다).
        // 새 창의 기준 크기는 옮긴 탭에서 고른 칸의 크기다
        let mut win = Win::new(self.pane_font_size(moved.focus).unwrap_or(self.w.font_size));
        win.focused = new_focused;
        win.occluded = window_occluded(&gfx.window);
        win.gfx = Some(gfx);
        win.tabs.push(moved);
        let mut old = std::mem::replace(&mut self.w, win);
        old.focused = false;
        self.load_fonts();
        // 옮긴 칸에 초점 보고(DEC 1004)를 한 번만 보낸다(#223). 창이 만들어질 때 이미 초점이 있으면 뒤따르는
        // 포커스 사건은 상태가 같아 건너뛰므로(WindowEvent::Focused) 중복되지 않는다
        let plan = crate::winact::move_focus_plan(old_focused, new_focused);
        if let Some(i) = self.pane_index(moved_focus) {
            if self.panes[i].session.modes().focus_events {
                if plan.lose {
                    self.panes[i].session.write(b"\x1b[O");
                }
                if plan.gain {
                    self.panes[i].session.write(b"\x1b[I");
                }
            }
        }
        self.others.push(old);
        self.quit_deadline = None;
        self.last_focused = Some(id);
        self.apply_initial_size();
        // 남은 창은 탭이 줄어(탭 줄이 사라질 수 있다) 격자를 다시 맞춘다
        self.for_each_window(|app| app.resize_all());
        self.clear_visible_unread();
    }

    /// `toggle_visibility`(macOS): 앱이 활성이면 모든 창을 숨기고(보고 있는 칸이 전체 화면이면 하지 않음),
    /// 아니면 숨긴 창을 되살리고 앱을 활성화한다(#718). `unhide`만으로는 앞으로 오지 않는다(실제 창에서 잼)
    fn toggle_visibility(&mut self) {
        #[cfg(target_os = "macos")]
        {
            use crate::winact::Visibility;
            let Some(mtm) = objc2::MainThreadMarker::new() else {
                return;
            };
            let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
            let fullscreen = self.w.gfx.as_ref().is_some_and(|g| window_is_fullscreen(&g.window));
            match crate::winact::visibility_plan(app.isActive(), fullscreen) {
                Some(Visibility::Hide) => app.hide(None),
                Some(Visibility::Show) => {
                    app.unhide(None);
                    // Ghostty `NSApp.activate(ignoringOtherApps: true)`. winit `focus_window`도 같은 호출을 쓴다
                    #[allow(deprecated)]
                    app.activateIgnoringOtherApps(true);
                }
                None => {}
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
        // 사람이 붙인 칸 제목만 적는다. 프로그램이 OSC로 정한 제목(`Pane::title`)은 적지 않는다(Ghostty `titleOverride`처럼)
        let title_of = |id: u64| {
            self.panes
                .iter()
                .find(|p| p.id == id)
                .and_then(|p| p.user_title.clone())
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
                            layout: crate::state::SavedNode::from_node(&t.layout, &cwd_of, &title_of),
                            focus: t.layout.leaves().iter().position(|&id| id == t.focus).unwrap_or(0),
                            zoomed: t.zoomed,
                            title: t.title.clone(),
                        })
                        .collect(),
                    active: w.active,
                    title: w.window_title.clone(),
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
                let mut win = Win::new(self.new_window_font_size(false));
                win.focused = gfx.window.has_focus();
                win.occluded = window_occluded(&gfx.window);
                win.gfx = Some(gfx);
                let old = std::mem::replace(&mut self.w, win);
                self.others.push(old);
                self.load_fonts();
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
                // 되살린 칸은 창의 기준 크기로 연다(`window-save-state`는 칸마다 크기를 적지 않는다)
                let font_size = self.w.font_size;
                self.ensure_font_size(font_size);
                let size = self.grid_size_for(self.content_rect(), font_size);
                let mut ids = Vec::new();
                for cwd in st.layout.panes() {
                    let mut opts = self.default_spawn();
                    if let Some(dir) = cwd.filter(|d| d.is_dir()) {
                        opts.cwd = Some(dir);
                    }
                    if let Some(p) = self.spawn_pane(opts, size, font_size) {
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
                    // 상태 파일은 보안 경계는 아니지만 믿지 않는다: 읽은 제목도 사람이 친 제목과 같은 규칙을 다시 거친다
                    title: st.title.as_deref().and_then(clean_user_title),
                });
                for (id, t) in leaves.iter().zip(st.layout.pane_titles()) {
                    if let Some(t) = t.as_deref().and_then(clean_user_title) {
                        self.set_user_title(*id, &t);
                    }
                }
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
            self.w.window_title = sw.title.as_deref().and_then(clean_user_title);
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
        let mut win = Win::new(self.new_window_font_size(quick));
        win.focused = gfx.window.has_focus();
        win.occluded = window_occluded(&gfx.window);
        win.gfx = Some(gfx);
        let mut old = std::mem::replace(&mut self.w, win);
        // 새 창이 앞으로 온다. 앞 창의 포커스 잃음 사건을 기다리지 않고 바로 보고 있지 않은 창으로.
        // 창이 하나도 없던 자리(창 없는 상태, #178)는 남기지 않는다
        if old.gfx.is_some() {
            old.focused = false;
            self.others.push(old);
        }
        self.load_fonts();
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
                // Session을 떨어뜨리면 `Drop`이 셸에 SIGHUP을 보낸다(#651)
                let mut pane = self.panes.remove(i);
                crate::desktop::forget(std::mem::take(&mut pane.notes));
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
                self.w = Win::new(self.config.font_size);
                self.last_focused = None;
                self.on_last_window_closed(event_loop, false);
            }
        }
    }

    /// 창이 하나도 없는 상태인가(첫 창 없이 띄웠거나 마지막 창을 닫았는데 앱이 남아 있다, #178)
    /// 창 가운데 초점이 있는 것이 있나(앱이 앞에 있다고 본다)
    fn any_window_focused(&self) -> bool {
        std::iter::once(&self.w).chain(&self.others).any(|w| w.focused)
    }

    /// 앱이 앞인지를 보안 입력 상태에 알린다(#258)
    fn update_secure_active(&mut self) {
        let active = self.any_window_focused();
        self.secure.set_active(active);
    }

    /// 칸 단위 보안 입력의 초점을 맞춘다: 초점 창이 보고 있는 탭의 고른 칸만 초점이고, 사라진 칸은 뺀다(#258)
    fn sync_secure_focus(&mut self) {
        let focused: std::collections::HashSet<u64> = std::iter::once(&self.w)
            .chain(&self.others)
            .filter(|w| w.focused)
            .filter_map(|w| w.tabs.get(w.active).map(|g| g.focus))
            .collect();
        let alive: std::collections::HashSet<u64> = self.panes.iter().map(|p| p.id).collect();
        self.secure
            .sync_scoped(|id| alive.contains(&id).then(|| focused.contains(&id)));
    }

    /// 모든 칸의 암호 입력 상태를 PTY의 termios로 살핀다. 바뀐 칸이 있으면 참. `macos-auto-secure-input`이 꺼져 있어도
    /// 자물쇠 커서는 바뀐다(Ghostty는 칸의 `password_input` 표시와 보안 입력을 따로 다룬다, #258)
    fn poll_password(&mut self) -> bool {
        let auto = self.config.macos_auto_secure_input;
        let mut changed = false;
        for p in &mut self.panes {
            let exited = p.exited.is_some();
            let polled = if exited { None } else { p.session.password_input() };
            let now = crate::secure_input::password_after_poll(p.password, exited, polled);
            if now == p.password {
                continue;
            }
            p.password = now;
            changed = true;
            if auto {
                if now {
                    self.secure.set_scoped(p.id, false);
                } else {
                    self.secure.remove_scoped(p.id);
                }
            }
        }
        changed
    }

    /// `toggle_secure_input`: 초점 칸의 칸 단위 상태를 뒤집는다. `macos-auto-secure-input`이 꺼져 있으면 아무 일도 없다
    /// (Ghostty Ghostty.App.swift `toggleSecureInput`). 앱 전체 켜짐(메뉴)과 다르다
    fn toggle_pane_secure_input(&mut self) {
        if !self.config.macos_auto_secure_input {
            return;
        }
        let Some(id) = self.focused_id() else { return };
        if self.secure.has_scoped(id) {
            self.secure.remove_scoped(id);
        } else {
            self.secure.set_scoped(id, true);
        }
        self.request_redraw_all();
    }

    /// 메뉴 `보안 키보드 입력`: 앱 전체로 켜고 끈다. 다음 실행에도 남긴다
    fn toggle_global_secure_input(&mut self) {
        let on = !self.secure.global();
        self.secure.set_global(on);
        if let Some(path) = crate::secure_input::global_path()
            && let Err(e) = crate::secure_input::save_global(&path, on)
        {
            log::warn!("보안 입력 켜짐을 남기지 못했습니다: {e}");
        }
        #[cfg(target_os = "macos")]
        if let Some(item) = &self.menu_secure {
            item.set_checked(on);
        }
        self.request_redraw_all();
    }

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

    /// 명령 팔레트의 항목: 기본·사용자 항목과 열린 모든 창의 칸마다 `Focus: 제목` 항목(Ghostty `jumpOptions`, #690)을
    /// 함께 이름순으로 고른다
    fn palette_entries(&self) -> Vec<crate::palette::Entry> {
        let home = std::env::var("HOME").ok();
        let mut v = crate::palette::entries(&self.config.palette_entries, !self.config.palette_defaults_cleared);
        v.extend(crate::palette::focus_entries(&self.focus_panes(), home.as_deref()));
        crate::palette::sort_entries(&mut v);
        v
    }

    /// 모든 창(지금 창, 다른 창)의 칸을 창·탭 차례로. 항목 상한만큼만 모은다
    fn focus_panes(&self) -> Vec<crate::palette::FocusPane> {
        let mut out = Vec::new();
        for w in std::iter::once(&self.w).chain(&self.others) {
            // 칸 제목이 비었을 때 쓰는 창 제목: 설정 `title` → 덮어쓴 창 제목. 없으면 빈 글(그러면 "Untitled")
            let window_title = window_title_of(self.config.title.as_deref(), w.window_title.as_deref(), "");
            for g in &w.tabs {
                for id in g.layout.leaves() {
                    if out.len() >= crate::palette::MAX_FOCUS {
                        return out;
                    }
                    let Some(p) = self.panes.iter().find(|p| p.id == id) else {
                        continue;
                    };
                    out.push(crate::palette::FocusPane {
                        id,
                        title_override: g.title.clone().or_else(|| w.window_title.clone()),
                        // 탭 줄에 보이는 이름(사람 제목 → 프로그램 제목 → 전경 프로세스 이름)과 같게. 🔔·🔒는 붙이지 않는다.
                        // Ghostty는 칸 제목이 비면 창 제목인데, 우리 창 제목도 이 이름에서 오므로 같은 것을 보인다
                        pane_title: title_of(p.user_title.as_deref(), &p.title, p.fg_name.as_deref(), false, false),
                        window_title: window_title.clone(),
                        cwd: p.cwd.as_ref().map(|c| c.to_string_lossy().into_owned()),
                    });
                }
            }
        }
        out
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

    /// 칸마다 보이는지(가림 상태 + 앞에 있는 탭에 든 칸인가)를 세션에 알린다. 모드 2033을 켠 프로그램은 달라진 때만 보고를 받는다
    /// (Ghostty macOS는 창의 가림 상태를 창 안 모든 칸에 알리고, 탭은 따로 창이라 앞이 아닌 탭은 안 보임이다, #303)
    fn sync_visibility(&self) {
        for w in std::iter::once(&self.w).chain(&self.others) {
            let front = w.tabs.get(w.active);
            for g in &w.tabs {
                let shown = !w.occluded && front.is_some_and(|f| std::ptr::eq(f, g));
                for id in g.layout.leaves() {
                    if let Some(p) = self.panes.iter().find(|p| p.id == id) {
                        p.session.set_visible(shown);
                    }
                }
            }
        }
    }

    /// kitty 애니메이션(#554)의 다음 프레임 때가 된 창을 다시 그리게 한다. 그린 프레임이 다음 때를 새로 정한다
    fn fire_kitty_wakes(&mut self, now: Instant) {
        for w in std::iter::once(&mut self.w).chain(self.others.iter_mut()) {
            let (fire, next) = kitty_images::wake_step(w.kitty_wake, now, w.occluded);
            w.kitty_wake = next;
            if fire {
                if let Some(g) = &w.gfx {
                    g.window.request_redraw();
                }
            }
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

    /// 접근성(#560): 지금 창의 보이는 칸들을 접근성 트리에 알린다. 화면 읽기 프로그램이 붙지 않았으면 아무것도 하지 않는다.
    /// 칸 글은 칸마다 500ms에 한 번 이하로만 다시 만들고(`a11y::Gate`), 포커스·창 제목·칸 수 변화는 바로 알린다
    fn a11y_observe(&mut self, views: &[PaneView], title: &str, focus: Option<u64>) {
        if !self.w.gfx.as_ref().is_some_and(|g| g.a11y.is_active()) {
            return;
        }
        let frames: Vec<crate::a11y::Frame<'_>> = views
            .iter()
            .map(|v| crate::a11y::Frame {
                id: v.id,
                name: self
                    .pane_index(v.id)
                    .map(|i| self.panes[i].display_title())
                    .unwrap_or_default(),
                screen: &v.screen,
                geom: Some(crate::a11y::Geom {
                    x: v.inner.x as f64,
                    y: v.inner.y as f64,
                    cell_w: v.metrics.width as f64,
                    cell_h: v.metrics.height as f64,
                }),
            })
            .collect();
        if let Some(g) = self.w.gfx.as_mut() {
            g.a11y.observe(Instant::now(), title, &frames, focus);
        }
    }

    /// 접근성(#560): 그리기를 기다리지 않고 지금 창의 화면을 떠서 접근성 트리를 맞춘다(첫 트리 요청, 미뤄 둔 갱신의 때)
    fn a11y_pull(&mut self) {
        let Some(g) = self.w.gfx.as_ref().filter(|g| g.a11y.is_active()) else {
            return;
        };
        let title = g.window.title();
        let shots: Vec<(u64, String, Screen, crate::a11y::Geom)> = self
            .group_layout(self.w.active)
            .into_iter()
            .filter_map(|(id, outer)| {
                let p = &self.panes[self.pane_index(id)?];
                let m = self.metrics_at(p.font_size)?;
                let inner = self.inner_rect(outer, p.font_size);
                let mut screen = p.session.snapshot();
                screen.crop_columns(p.session.size().cols as usize);
                let geom = crate::a11y::Geom {
                    x: inner.x as f64,
                    y: inner.y as f64,
                    cell_w: m.width as f64,
                    cell_h: m.height as f64,
                };
                Some((id, p.display_title(), screen, geom))
            })
            .collect();
        let frames: Vec<crate::a11y::Frame<'_>> = shots
            .iter()
            .map(|(id, name, screen, geom)| crate::a11y::Frame {
                id: *id,
                name: name.clone(),
                screen,
                geom: Some(*geom),
            })
            .collect();
        let focus = self.focused_id();
        if let Some(g) = self.w.gfx.as_mut() {
            g.a11y.observe(Instant::now(), &title, &frames, focus);
        }
    }

    /// 접근성(#560): 미뤄 둔 화면 글 갱신 때가 된 창마다 화면을 떠서 알린다
    fn a11y_tick(&mut self, now: Instant) {
        let due = |w: &Win| w.gfx.as_ref().and_then(|g| g.a11y.wake_at()).is_some_and(|t| t <= now);
        if due(&self.w) {
            self.a11y_pull();
        }
        for i in 0..self.others.len() {
            if due(&self.others[i]) {
                std::mem::swap(&mut self.w, &mut self.others[i]);
                self.a11y_pull();
                std::mem::swap(&mut self.w, &mut self.others[i]);
            }
        }
    }

    /// 미뤄 둔 접근성 갱신을 해야 할 가장 이른 때(이때 이벤트 고리를 깨운다)
    fn a11y_next_wake(&self) -> Option<Instant> {
        std::iter::once(&self.w)
            .chain(&self.others)
            .filter_map(|w| w.gfx.as_ref().and_then(|g| g.a11y.wake_at()))
            .min()
    }

    /// 접근성 클라이언트(VoiceOver)의 요청. 포커스(칸 고르기)만 처리하고 나머지 동작은 하지 않는다
    fn on_access(&mut self, id: WindowId, req: crate::a11y::Request) {
        use crate::a11y::Request;
        if !self.switch_to(id) {
            return;
        }
        match req {
            Request::InitialTree => {
                if let Some(g) = self.w.gfx.as_mut() {
                    g.a11y.activate();
                }
                self.a11y_pull();
            }
            Request::Deactivated => {
                if let Some(g) = self.w.gfx.as_mut() {
                    g.a11y.deactivate();
                }
            }
            Request::Focus(pane) => {
                // 트리에 내놓은 칸(지금 탭에 보이는 칸)만 받는다. 다른 탭의 칸 번호로 탭을 바꾸지 못하게
                if self.group_layout(self.w.active).iter().any(|(id, _)| *id == pane) {
                    self.focus_pane(pane);
                }
            }
            Request::Ignored => {}
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
        if let Some(g) = self.w.gfx.as_mut() {
            g.first.began();
        }
        // GPU 길: 가려진 창은 그리지 않고, 그림판 얻기가 막힐 것 같으면 미룬다(ADR-038)
        if self.gpu_frame_gate() {
            return;
        }
        // OSC 22 모양이 바뀌었거나, 탭·칸이 바뀌어 마우스 아래 칸이 달라졌을 수 있다(#239)
        self.refresh_pointer();
        let theme = self.config.theme.clone();
        let top_inset = self.top_inset();
        let toast = self.w.toast.clone().filter(|(_, at)| at.elapsed() < TOAST_FOR);
        // 보안 입력이 켜져 있는 동안 창 오른쪽 위의 자물쇠 표시(`macos-secure-input-indication`, #258)
        let secure_badge = self.secure.enabled() && self.config.macos_secure_input_indication;
        // 배지는 탭 줄 아래(탭 줄과 겹치지 않게)
        let badge_top = top_inset + self.tabbar_height();
        let confirm = self
            .w
            .confirm
            .as_ref()
            .map(|c| (c.message.clone(), c.yes, c.pending.is_program_prompt()));
        let title_prompt = self
            .w
            .title_prompt
            .as_ref()
            .map(|(target, t)| (target.label(), t.clone()));
        let now_overlay = Instant::now();
        let resize_overlays: Vec<u64> = self
            .panes
            .iter()
            .filter(|p| p.overlay_until.is_some_and(|t| t > now_overlay))
            .map(|p| p.id)
            .collect();
        let focus = self.focused_id();
        // 입력 → 화면 지연(#417): 칸의 화면을 뜨기 전의 때. 이 때까지 온 출력은 뜬 화면에 들어 있다
        let latency_at = Instant::now();
        // 보이는 칸마다: 바깥 사각형, 격자 자리, 화면, 고른 칸인가, 진행 표시
        let layout = self.group_layout(self.w.active);
        let views: Vec<PaneView> = layout
            .iter()
            .filter_map(|(id, outer)| {
                let p = &self.panes[self.pane_index(*id)?];
                Some(PaneView {
                    id: *id,
                    outer: *outer,
                    inner: self.inner_rect(*outer, p.font_size),
                    font_size: p.font_size,
                    metrics: self.metrics_at(p.font_size)?,
                    screen: {
                        // 격자가 창보다 넓으면(DECCOLM 132열, #324) 창 밖은 잘라 그린다. 좁으면 그대로 그리고 오른쪽은 바탕이다
                        let mut screen = p.session.snapshot();
                        screen.crop_columns(p.session.size().cols as usize);
                        screen
                    },
                    focused: Some(*id) == focus,
                    bell_border: p.bell_border,
                    progress: p
                        .status_summary()
                        .and_then(|s| tabbar::status_bar(s, self.config.progress_style))
                        .or(p.live_progress().filter(|_| self.config.progress_style)),
                })
            })
            .collect();
        // kitty 애니메이션(#554): 화면에 보이는 그림 가운데 가장 이른 다음 프레임 때에 이 창을 다시 그린다.
        // 보이지 않는 그림(스크롤로 밖, 다른 탭)은 화면이 보지 않으므로 여기에 없다
        self.w.kitty_wake = kitty_images::frame_wake(
            views
                .iter()
                .filter_map(|v| {
                    self.pane_index(v.id)
                        .and_then(|i| self.panes[i].session.kitty_next_frame())
                })
                .min(),
            Instant::now(),
        );
        let latency = self.latency_begin(&views, latency_at);
        // 스크롤 막대: 스크롤 위치가 바뀌면 잠깐 보인다. 마우스가 오른쪽 끝 위에 있어도 보인다
        let now_bar = Instant::now();
        for v in &views {
            if let Some(i) = self.pane_index(v.id) {
                let p = &mut self.panes[i];
                if p.scroll_seen != v.screen.display_offset {
                    p.scroll_seen = v.screen.display_offset;
                    p.scrollbar_until = Some(now_bar + SCROLLBAR_FOR);
                }
            }
        }
        let bars: Vec<(bool, bool)> = views
            .iter()
            .map(|v| {
                let scrolling = self
                    .pane_index(v.id)
                    .is_some_and(|i| self.panes[i].scrollbar_until.is_some_and(|t| t > now_bar));
                // 누르고 있는 동안은 마우스가 띠 밖으로 나가도 막대를 계속 보인다
                let hovering =
                    self.w.gutter_hot == Some(v.id) || self.w.scrollbar_press.is_some_and(|p| p.pane == v.id);
                (
                    self.config
                        .scrollbar
                        .shows(v.screen.history, scrolling, hovering, self.system_scroller.always),
                    hovering,
                )
            })
            .collect();
        let split = views.len() > 1;
        // 칸 끌기 손잡이(`drag-handle`)와 놓을 자리 미리 보기. 둘 다 위층에 그리고 `Chrome`에 들어 있어 바뀌면 통째로 다시 그린다
        let (handle_panes, handle_zoomed) = self.handle_env();
        let handles: Vec<crate::pane_drag::Look> = views
            .iter()
            .map(|v| {
                let hover = self.w.handle_hover.filter(|h| h.0 == v.id);
                crate::pane_drag::look(crate::pane_drag::HandleInput {
                    mode: self.config.drag_handle,
                    panes: handle_panes,
                    zoomed: handle_zoomed,
                    in_band: hover.is_some(),
                    over_handle: hover.is_some_and(|h| h.1),
                    dragging: self.w.pane_drag.from() == Some(v.id),
                })
            })
            .collect();
        let drop_preview: Option<(Rect, crate::split::Direction)> = self
            .w
            .pane_drag
            .target()
            .and_then(|t| views.iter().find(|v| v.id == t.pane).map(|v| (v.outer, t.dir)));
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
                        title: focused
                            .map(|p| p.display_title_in(g.title.as_deref()))
                            .unwrap_or_default(),
                        active: gi == self.w.active,
                        unread: panes.iter().any(|p| p.unread),
                        // 칸들의 프로그램 상태(OSC 7501) 가운데 가장 급한 것, 없으면 고른 칸의 진행 표시, 없으면 다른 칸의 것
                        progress: tabbar::most_urgent(panes.iter().filter_map(|p| p.status_summary()))
                            .and_then(|s| tabbar::status_bar(s, self.config.progress_style))
                            .or_else(|| {
                                focused
                                    .and_then(|p| p.live_progress())
                                    .or_else(|| panes.iter().find_map(|p| p.live_progress()))
                                    .filter(|_| self.config.progress_style)
                            }),
                    }
                })
                .collect()
        };
        let tab_title = self
            .active_pane()
            .map(|t| t.display_title_in(self.w.tabs.get(self.w.active).and_then(|g| g.title.as_deref())))
            .unwrap_or_else(|| "b-terminal".into());
        let title = window_title_of(self.config.title.as_deref(), self.w.window_title.as_deref(), &tab_title);
        // 접근성(#560): 이미 뜬 화면으로 접근성 트리를 맞춘다(클라이언트가 없으면 아무것도 하지 않는다)
        self.a11y_observe(&views, &title, focus);
        // 제목 막대 폴더 아이콘(#279): 초점 칸의 작업 폴더(OSC 7). 모르면 없앤다
        let proxy = crate::dock_open::proxy_icon_path(
            self.config.macos_titlebar_proxy_icon,
            self.active_pane().and_then(|p| p.cwd.as_deref()),
        );
        let preedit = self.w.preedit.clone();
        let hover = self.w.hover.clone();
        let link_previews = self.config.link_previews;
        // 명령 팔레트: (친 글, 보일 줄들(이름, 부제, 단축키), 고른 줄)
        let palette = self.w.palette.as_ref().map(|(q, sel)| {
            let entries = self.palette_entries();
            let rows: Vec<(String, String, String)> = crate::palette::filter(&entries, q)
                .into_iter()
                .map(|i| {
                    let e = &entries[i];
                    (
                        e.label.to_string(),
                        e.subtitle.to_string(),
                        crate::palette::shortcut(&self.keymap, &e.command),
                    )
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
                // Cmd를 누른 채 올라간 링크에 밑줄. 이미 곧은 밑줄이면 이중(#288)
                if let Some((_, hit)) = hover.as_ref().filter(|(h, _)| *h == v.id) {
                    for sp in &hit.spans {
                        for c in sp.start..sp.end.min(screen.cols) {
                            let i = sp.row * screen.cols + c;
                            if let (Some(cell), Some(src)) = (cells.get_mut(i), screen.cells.get(i)) {
                                cell.underline = link_underline(src.flags);
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
                    reversed_cursor_defaults(&theme, screen.modes.reverse_colors),
                    under,
                );
                // 암호 입력 중이면 자물쇠(#258). 모양은 Ghostty cursor.zig의 순서를 따른다
                let password = self.pane_index(v.id).is_some_and(|i| self.panes[i].password);
                let composing = v.focused && search.is_none() && palette.is_none() && !preedit.is_empty();
                let cursor = crate::secure_input::cursor_style(
                    password,
                    composing,
                    screen.cursor.visible,
                    v.focused,
                    hide,
                    screen.cursor.shape,
                )
                .map(|style| DrawCursor {
                    col: screen.cursor.col,
                    row: screen.cursor.row,
                    style,
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
        let Some(mut fonts) = self.w.fonts.parts_mut() else {
            return;
        };
        let size = gfx.window.inner_size();
        // 창 덧그림(탭 줄 등)은 기준 크기의 칸 크기다. 칸 안은 칸마다 `PaneView::metrics`다
        let m = fonts.chrome().metrics();
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
                        (v.metrics.width, v.metrics.height),
                    )
                })
                .collect(),
            labels: labels
                .iter()
                .map(|l| (l.title.clone(), l.active, l.unread, l.progress))
                .collect(),
            handles: handles.clone(),
            drop: drop_preview,
            overlay: toast.is_some()
                || secure_badge
                || hover.is_some()
                || search.is_some()
                || palette.is_some()
                || confirm.is_some()
                || title_prompt.is_some()
                || views.iter().any(|v| resize_overlays.contains(&v.id))
                // 벨 테두리는 칸 위에 겹쳐 그린다(#687)
                || views.iter().any(|v| v.bell_border)
                // kitty 그림은 칸 위에 겹쳐 그린다(#76)
                || views.iter().any(|v| !v.screen.images.is_empty())
                || bars.iter().any(|b| b.0)
                // 손잡이와 놓을 자리는 위층에 그린다. 위층을 쓰는 판단(`chrome.overlay || split`)에 빠지면 단언이 걸린다
                || crate::pane_drag::needs_upper(&handles, drop_preview.is_some())
                || (single_tab && views.iter().any(|v| v.progress.is_some())),
            shader_layers: gfx.gpu.as_ref().is_some_and(|g| g.has_custom_shaders()),
            theme: (theme.background, theme.foreground),
            dim: (dim_alpha, dim_fill, divider_color),
        };
        // GPU 길(ADR-036, macOS 기본 ADR-039): 층과 격자를 합쳐 창 surface에 올린다. 아래는 CPU 길
        if gfx.gpu.is_some() {
            self.redraw_gpu(gpu_frame::GpuFrame {
                latency,
                views: &views,
                drawn,
                pads: &pads,
                bars: &bars,
                labels: &labels,
                chrome,
                toast: &toast,
                secure_badge,
                badge_top,
                confirm: &confirm,
                title_prompt: &title_prompt,
                hover: &hover,
                link_previews,
                palette: &palette,
                search: &search,
                preedit: &preedit,
                resize_overlays: &resize_overlays,
                handles: &handles,
                drop_preview,
                dividers: &dividers,
                divider_color,
                dim_alpha,
                dim_fill,
                split,
                single_tab,
                top_inset,
                title: &title,
                proxy,
            });
            return;
        }
        let started = Instant::now();
        let n = (size.width * size.height) as usize;
        let full =
            chrome.overlay || self.w.last_frame.as_ref().is_none_or(|f| f.chrome != chrome) || self.w.frame.len() != n;
        if self.w.frame.len() != n {
            self.w.frame = vec![0; n];
        }
        let mut repainted_rows = 0;
        // 이 창이 지금 쓰는 바탕 불투명도(toggle_background_opacity로 불투명해졌으면 1, #209)
        let opacity = crate::toggles::effective_opacity(self.config.background_opacity, self.w.opaque);
        // 배경 그림 층(#134): 창 크기·바탕색이 바뀌면 다시 합성한다
        // 화면 반전(DECSCNM, #302)이면 기본 바탕색이 맞바뀐다. 창 바탕·투명 처리도 그 색을 따른다(Ghostty는 맞바뀐 바탕색을 쓴다)
        let win_bg = views
            .iter()
            .find(|v| v.focused && v.screen.modes.reverse_colors)
            .map_or(theme.background, |v| v.screen.background);
        backdrop::refresh(
            &mut self.w.backdrop,
            self.bg_image.as_deref(),
            &self.config,
            (size.width, size.height),
            win_bg,
            opacity,
        );
        {
            let mut canvas = Canvas::new(&mut self.w.frame, size.width, size.height);
            if let Some((_, layer)) = &self.w.backdrop {
                canvas.set_backdrop(win_bg, layer);
            }
            // 투명 창(#78·#86): 기본 바탕색은 반투명으로 칠하고, 글자 가장자리는 그 위에 알파째 섞는다
            if opacity < 1.0 {
                canvas.set_see_through(win_bg, (opacity * 255.0).round() as u8);
            }
            // background-opacity-cells(#177): 색 바탕 칸도 같은 알파로
            canvas.set_cells_alpha(bt_render::cells_alpha(opacity, self.config.background_opacity_cells));
            if full {
                let bg = views
                    .iter()
                    .find(|v| v.focused)
                    .map(|v| v.screen.background)
                    .unwrap_or(theme.background);
                canvas.clear(bg);
                // 화면 반전 중에도 투명 제목 막대는 반전 전 바탕색(Ghostty와 같음, #302)
                if let Some(f) = views.iter().find(|v| v.focused) {
                    if let Some((x, y, w, h, c)) = titlebar_fill(
                        top_inset,
                        size.width,
                        f.screen.modes.reverse_colors,
                        f.screen.chrome_background,
                    ) {
                        canvas.fill_rect(x, y, w, h, c);
                    }
                }
                if !labels.is_empty() {
                    tabbar::draw_with_title_font(
                        &mut canvas,
                        fonts.chrome(),
                        self.w.title_fonts.as_mut(),
                        &theme,
                        &labels,
                        top_inset as i32,
                    );
                }
            }
            for (((v, (cells, cursor, pre)), pad), bar) in views.iter().zip(&drawn).zip(&pads).zip(&bars) {
                let screen = &v.screen;
                let vm = v.metrics;
                let grid_rect = Rect::new(
                    v.inner.x,
                    v.inner.y,
                    screen.cols as u32 * vm.width,
                    screen.rows as u32 * vm.height,
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
                    // kitty 그림: 칸 바탕 아래·글자 아래는 격자를 그리면서 끼우고, 글자 위는 뒤에 그린다. 격자 밖은 자른다(#547)
                    let images = kitty_images::PaneImages::new(
                        &screen.images,
                        (v.inner.x, v.inner.y),
                        (vm.width, vm.height),
                        (screen.cols, screen.rows),
                    );
                    images.draw_grid(&mut canvas, fonts.for_size(v.font_size), &view, screen.background);
                    if let Some(p) = pad {
                        crate::padding::paint(
                            &mut canvas,
                            v.outer,
                            grid_rect,
                            vm.width,
                            vm.height,
                            p,
                            screen.background,
                            None,
                        );
                    }
                    images.paint(&mut canvas, bt_core::kitty_geom::Layer::AboveText);
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
                    let spans = draw_grid_rows(
                        &mut canvas,
                        fonts.for_size(v.font_size),
                        v.inner.x,
                        v.inner.y,
                        &view,
                        &dirty,
                    );
                    if let Some(p) = pad {
                        crate::padding::paint(
                            &mut canvas,
                            v.outer,
                            grid_rect,
                            vm.width,
                            vm.height,
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
                    let px = v.inner.x + (screen.cursor.col as u32 * vm.width) as i32;
                    let py = v.inner.y + (screen.cursor.row as u32 * vm.height) as i32;
                    gfx.window
                        .set_ime_cursor_area(PhysicalPosition::new(px, py), PhysicalSize::new(vm.width, vm.height));
                }
                if !full {
                    continue;
                }
                if let Some((_, query, status, missing)) = search.as_ref().filter(|s| s.0 == v.id) {
                    tabbar::draw_search_bar(
                        &mut canvas,
                        fonts.chrome(),
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
                        fonts.chrome(),
                        &theme,
                        v.outer,
                        self.config.resize_overlay_position,
                        &format!("{} × {}", screen.cols, screen.rows),
                    );
                }
                if bar.0 {
                    let width = (bt_render::scrollbar::BAR_WIDTH as f64 * self.scale).round() as u32;
                    bt_render::scrollbar::draw(
                        &mut canvas,
                        v.outer.right() - width as i32 - self.scale.round().max(1.0) as i32,
                        v.inner.y,
                        screen.rows as u32 * vm.height,
                        width,
                        screen.rows,
                        screen.history,
                        screen.display_offset,
                        theme.foreground,
                        if bar.1 { 170 } else { 110 },
                    );
                }
                // 탭 막대가 없으면 진행 막대를 칸 맨 위에 얇게(Ghostty와 같은 자리). 있으면 탭 줄에 보인다
                if let (true, Some(p)) = (single_tab, v.progress) {
                    tabbar::draw_progress(&mut canvas, &theme, v.outer.x, v.outer.y, v.outer.w, p, vm.stroke * 3);
                }
            }
            if full {
                for d in &dividers {
                    canvas.fill_rect(d.x, d.y, d.w, d.h, divider_color);
                }
                for (v, look) in views.iter().zip(&handles) {
                    crate::pane_drag::draw_handle(&mut canvas, v.outer, *look, theme.foreground, self.scale);
                }
                if let Some((rect, dir)) = drop_preview {
                    crate::pane_drag::draw_drop(&mut canvas, rect, dir, theme.cursor);
                }
                for v in views.iter().filter(|v| v.bell_border) {
                    crate::bell_border::draw(&mut canvas, v.outer, self.scale);
                }
                // 실제 주소를 창 왼쪽 아래에 보인다(OSC 8 링크는 보이는 글과 주소가 다를 수 있다)
                // `link-previews`가 끈 링크는 띠 없이 밑줄만 보인다
                if let Some((_, hit)) = hover.as_ref().filter(|(_, h)| link_previews.shows(h.explicit)) {
                    tabbar::draw_link_hint(
                        &mut canvas,
                        fonts.chrome(),
                        &theme,
                        &bt_core::proto::printable(&hit.url),
                    );
                }
                if let Some((q, rows, sel)) = &palette {
                    tabbar::draw_palette(
                        &mut canvas,
                        fonts.chrome(),
                        &theme,
                        &format!("{q}{preedit}"),
                        rows,
                        *sel,
                    );
                }
                if let Some((text, _)) = &toast {
                    crate::tabbar::draw_toast(&mut canvas, fonts.chrome(), &theme, text);
                }
                if secure_badge {
                    crate::tabbar::draw_secure_badge(&mut canvas, fonts.chrome(), &theme, badge_top);
                }
                if let Some((label, text)) = &title_prompt {
                    tabbar::draw_prompt(
                        &mut canvas,
                        fonts.chrome(),
                        &theme,
                        label,
                        &format!("{text}{preedit}"),
                        "↩ 정하기    esc 취소    비우고 ↩면 프로그램이 정한 제목으로",
                    );
                }
                if let Some((message, yes, from_program)) = &confirm {
                    let hint = if *from_program {
                        format!("y {yes}    n·esc 거절")
                    } else {
                        format!("↩ {yes}    esc 취소")
                    };
                    tabbar::draw_confirm(&mut canvas, fonts.chrome(), &theme, message, &hint);
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
        if self.w.proxy_icon != proxy {
            #[cfg(target_os = "macos")]
            winit::platform::macos::set_represented_path(&gfx.window, proxy.as_deref());
            self.w.proxy_icon = proxy;
        }
        // 바뀐 것이 없으면 창에 올리지 않는다(올리기만 약 10ms, 실험 E2)
        let force_present = std::mem::take(&mut self.w.force_present);
        if !full && repainted_rows == 0 && !force_present {
            return;
        }
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        let t_resize = Instant::now();
        let Some(surface) = gfx.surface.as_mut() else { return };
        if surface.resize(w, h).is_err() {
            return;
        }
        let t_buffer = Instant::now();
        let Ok(buffer) = surface.buffer_mut() else {
            return;
        };
        let t_copy = Instant::now();
        // macOS의 softbuffer는 프레임마다 빈 버퍼를 준다(age 0). 들고 있는 프레임을 통째로 올린다.
        // 투명 창의 알파는 캔버스가 이미 곱한 알파로 그렸다(#86). macOS 기본은 프레임을 IOSurface로 곧장 옮긴다(#346)
        let t_present = Instant::now();
        let presented = buffer.present_frame(&self.w.frame).is_ok();
        if presented {
            gfx.first.presented("CPU");
            self.latency_presented(&latency, "CPU");
        }
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

/// 칸 하나의 격자 밖 모양: id, 바깥·격자 자리, 고른 칸인가, 바탕색, 칸·줄 수, 칸 크기(칸마다 글자 크기가 다르다, #422)
type PaneKey = (u64, Rect, Rect, bool, bt_core::Rgb, usize, usize, (u32, u32));
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
    /// 칸마다 손잡이를 어떻게 보이는가(`drag-handle`)
    handles: Vec<crate::pane_drag::Look>,
    /// 끌어다 놓을 자리 미리 보기: 놓일 칸의 바깥 사각형과 쪽
    drop: Option<(Rect, crate::split::Direction)>,
    overlay: bool,
    /// GPU 길에서 사용자 셰이더가 돈다: 칸 밖 덧그림을 셰이더 뒤 층에 그린다(#441). 바뀌면 층을 통째로 다시 그린다
    shader_layers: bool,
    theme: (bt_core::Rgb, bt_core::Rgb),
    dim: (u8, bt_core::Rgb, bt_core::Rgb),
}

/// 앞 프레임: 격자 밖 것들과 칸마다 (id, 셀, 커서, 조합 글)
struct FrameMemo {
    chrome: Chrome,
    panes: Vec<PaneFrame>,
}

/// 칸의 (열, 줄) 수 가운데 마우스·선택이 가리킬 수 있는 것: 엔진 격자와 창 격자 중 작은 쪽(#324).
/// 격자가 창보다 넓으면 창 밖 열은 보이지 않고, 좁으면 격자 밖 열에는 칸이 없다
fn pane_cells(session: &bt_term::Session) -> (usize, usize) {
    let size = session.size();
    let (cols, rows) = session.grid_cells();
    (cols.min(size.cols as usize).max(1), rows.min(size.rows as usize).max(1))
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

/// 격자 자리(`inner`)에 들어가는 격자 크기(칸 크기 `m`). 칸마다 칸 크기가 달라 같은 자리도 칸마다 다른 열·줄 수가 된다(#422)
fn grid_for(inner: Rect, m: &bt_render::font::CellMetrics) -> GridSize {
    GridSize {
        cols: (inner.w / m.width).max(2) as u16,
        rows: (inner.h / m.height).max(1) as u16,
        cell_width: m.width as u16,
        cell_height: m.height as u16,
    }
}

/// 글꼴 묶음 하나를 만든다(글자 크기 `size`). `want_title`이면 탭 줄 제목 글꼴도 함께. 돌려주는 것: 터미널 글꼴, 제목 글꼴, 제목 글꼴 경고.
/// 글꼴 파일·시스템 글꼴 목록은 `bt_render`가 나눠 써서 크기가 다른 묶음을 더 만들어도 파일을 다시 읽지 않는다
fn build_fonts(
    config: &AppConfig,
    scale: f32,
    size: f32,
    want_title: bool,
) -> Option<(Fonts, Option<Fonts>, Option<String>)> {
    let cfg = font_config(config, size, scale);
    let built = if want_title {
        Fonts::new_with_title(&cfg, config.window_title_font_family.as_deref()).map(|set| {
            let warn = set.title_warning;
            (set.terminal, set.title, warn)
        })
    } else {
        Fonts::new(&cfg).map(|f| (f, None, None))
    };
    match built {
        Ok((terminal, title, warn)) => {
            log::info!("글꼴: {} {}pt × {}", terminal.family_name, size, scale);
            if let Some(w) = &warn {
                log::warn!("설정: {w}");
            }
            Some((terminal, title, warn))
        }
        Err(e) => {
            log::error!("{e}");
            None
        }
    }
}

/// 새 칸이 이어받을 폴더(#328). OSC 7로 알린 폴더가 먼저고, 한 번도 알린 적 없으면 프로세스에게 묻는다.
/// 빈 OSC 7로 지운 칸은 Ghostty처럼 "모른다"이므로 프로세스에게 묻지 않는다(설정의 working-directory가 쓰인다)
fn inherited_cwd(
    reported: Option<&std::path::Path>,
    reset: bool,
    process: impl FnOnce() -> Option<std::path::PathBuf>,
) -> Option<std::path::PathBuf> {
    reported
        .map(std::path::Path::to_path_buf)
        .or_else(|| if reset { None } else { process() })
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

/// 창이나 칸이 포커스를 잃을 때 조합 글자를 확정 글로 보낼 입력 소스인가(#502, Ghostty #15137).
/// 한국어 입력기는 포커스를 잃으면 `insertText`도 `unmarkText`도 부르지 않고 조합을 버린다.
/// 일본어 등은 포커스가 돌아오면 조합을 이어 가므로 확정하면 `にほn`처럼 변환 전 글이 들어간다. 건드리지 않는다
#[cfg(any(target_os = "macos", test))]
fn ime_commits_on_focus_loss(source: Option<&str>) -> bool {
    source.is_some_and(|s| s.starts_with("com.apple.inputmethod.Korean"))
}

/// macOS에서 선택 클립보드를 b-terminal 프로세스끼리 나누는 pasteboard 이름(Ghostty는 `com.mitchellh.ghostty.selection`, #629)
#[cfg(target_os = "macos")]
const SELECTION_PASTEBOARD: &str = "com.dj258255.b-terminal.selection";

/// 다른 프로세스가 쓴 선택 클립보드 글을 읽는 상한(UTF-16 단위, OSC 52와 같은 1MiB 꼴). 넘으면 없다고 본다
#[cfg(target_os = "macos")]
const SELECTION_MAX_LEN: usize = 1 << 20;

/// 고른 글을 이름 붙은 pasteboard(같은 사용자의 다른 프로세스가 읽을 수 있다)에 써도 되는가. 암호 입력 중인 칸(`password`)이나
/// 보안 입력이 켜진 때(`secure`: 전역이거나 그 칸)의 글은 앱 안 저장소에만 둔다
/// `copy-on-select`의 자동 복사가 보낼 곳(선택 클립보드, 시스템 클립보드). 고른 칸이 암호 입력 중이거나 보안 입력이면(`private`)
/// 시스템 클립보드(다른 앱과 클립보드 기록 앱이 읽음)로는 보내지 않는다. 사람이 누르는 복사(⌘C)는 막지 않는다
fn auto_copy_targets(mode: bt_core::config::CopyOnSelect, private: bool) -> (bool, bool) {
    let (selection, system) = mode.targets();
    (selection, system && !private)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn share_selection(password: bool, secure: bool) -> bool {
    !password && !secure
}

/// 선택 클립보드에서 읽을 글. 이름 붙은 pasteboard의 바뀜 번호(`count`)가 이 앱이 마지막으로 고를 때 적어 둔 번호(`seen`)와
/// 다르면 다른 b-terminal이 나중에 고른 것이니 그 글이 먼저다. 같으면(그 뒤 아무도 쓰지 않음) 앱 안 저장소가 먼저다. 그래서
/// 공유하지 않은 글(암호 칸)을 고른 뒤에도 예전 공유 글이 붙지 않는다. 먼저 고른 쪽이 비었으면 다른 쪽, 둘 다 없으면 없다
fn pick_selection(
    shared: Option<String>,
    count: Option<isize>,
    seen: Option<isize>,
    local: Option<&str>,
) -> Option<String> {
    let shared = shared.filter(|t| !t.is_empty());
    let local = local.filter(|t| !t.is_empty()).map(str::to_owned);
    if count.is_some() && count != seen {
        shared.or(local)
    } else {
        local.or(shared)
    }
}

/// 시스템 설정의 스크롤 막대 두 가지(#630). 읽지 못하면(macOS가 아니거나 앱 스레드가 아니면) 기본: 스크롤할 때만 보이고 한 쪽씩
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct SystemScroller {
    /// "스크롤 막대 보기: 항상"
    always: bool,
    /// "스크롤 막대 클릭 시: 누른 자리로 이동"
    jump: bool,
}

impl SystemScroller {
    fn read() -> Self {
        #[cfg(target_os = "macos")]
        if let Some((always, jump)) = winit::platform::macos::scroller_preferences() {
            return Self { always, jump };
        }
        Self::default()
    }
}

/// 칸의 밑줄 모양(#276). 숨김(SGR 8) 칸은 꾸밈도 그리지 않는다(Ghostty가 xterm을 따른다)
fn underline_of(f: CellFlags) -> bt_render::Underline {
    use bt_render::Underline;
    if f.contains(CellFlags::HIDDEN) {
        Underline::None
    } else if f.contains(CellFlags::UNDERCURL) {
        Underline::Curly
    } else if f.contains(CellFlags::DOUBLE_UNDERLINE) {
        Underline::Double
    } else if f.contains(CellFlags::DOTTED_UNDERLINE) {
        Underline::Dotted
    } else if f.contains(CellFlags::DASHED_UNDERLINE) {
        Underline::Dashed
    } else if f.contains(CellFlags::UNDERLINE) {
        Underline::Single
    } else {
        Underline::None
    }
}

/// Cmd를 누른 채 올라간 링크 칸의 밑줄 모양(#288). Ghostty renderer/generic.zig 3245~3258줄:
/// 글자의 밑줄이 곧은 줄이면 이중으로 그려 링크를 구분하고, 그 밖(없음·이중·물결·점·파선)은 곧은 줄.
/// 숨김(SGR 8) 칸은 Ghostty처럼 꾸밈을 그리지 않는다
fn link_underline(f: CellFlags) -> bt_render::Underline {
    use bt_render::Underline;
    match underline_of(f) {
        _ if f.contains(CellFlags::HIDDEN) => Underline::None,
        Underline::Single => Underline::Double,
        _ => Underline::Single,
    }
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
                // kitty 자리표시 글자(U+10EEEE)와 그 결합 문자는 그리지 않는다: 그림이 그 칸에 그려진다(#553)
                ch: c.draw_ch(),
                fg,
                bg,
                bold: c.flags.contains(CellFlags::BOLD),
                italic: c.flags.contains(CellFlags::ITALIC),
                underline: underline_of(c.flags),
                strike: c.flags.contains(CellFlags::STRIKE),
                wide: c.flags.contains(CellFlags::WIDE),
                spacer: c.flags.contains(CellFlags::SPACER),
                selected: c.flags.contains(CellFlags::SELECTED),
                emoji: c.draw_combining().contains(&'\u{fe0f}'),
                cluster: None,
                cluster_cells: 0,
                overline: c.flags.contains(CellFlags::OVERLINE),
                // 숨김(SGR 8) 칸은 밑줄 색도 그리지 않는다. 색은 bt-term이 팔레트·OSC 4를 반영해 RGB로 푼 것
                underline_color: c.underline_color.filter(|_| !c.flags.contains(CellFlags::HIDDEN)),
            }
        })
        .collect();
    // 이모지 묶음(ZWJ·피부색·국기, #87)
    bt_render::cluster::mark(&mut cells, s.cols, |i| {
        let c = &s.cells[i];
        bt_render::cluster::ClusterCell {
            ch: c.draw_ch(),
            combining: c.draw_combining(),
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
        self.w.occluded = window_occluded(&gfx.window);
        self.w.gfx = Some(gfx);
        #[cfg(target_os = "macos")]
        if self.menu.is_none() {
            let (menu, secure_item) = crate::menu::build(self.secure.global());
            menu.init_for_nsapp();
            self.menu_secure = Some(secure_item);
            let proxy = self.proxy.clone();
            muda::MenuEvent::set_event_handler(Some(move |e: muda::MenuEvent| {
                let _ = proxy.send_event(UserEvent::Menu(e.id.0.clone()));
            }));
            self.menu = Some(menu);
        }
        apply_hidden_from_dock(self.config.macos_hidden);
        icon::apply(self.config.macos_icon, self.config.macos_custom_icon.as_deref());
        self.load_fonts();
        if self.w.fonts.chrome().is_none() {
            event_loop.exit();
            return;
        }
        self.load_background();
        self.load_shaders();
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
        // 켜 둔 보안 입력은 풀고 나간다(#258)
        self.secure.shutdown();
        // 선택 클립보드 pasteboard는 비우고 나간다(다른 b-terminal이 떠 있으면 그 선택도 사라진다, #629)
        #[cfg(target_os = "macos")]
        winit::platform::macos::release_named_pasteboard(SELECTION_PASTEBOARD);
        // 남긴 데스크톱 알림은 칸이 없어지므로 지운다(Ghostty `applicationWillTerminate`, #625)
        crate::desktop::forget_all();
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
                Some(crate::menu::MenuCommand::ToggleSecureKeyboardEntry) => self.toggle_global_secure_input(),
                None => {}
            },
            UserEvent::OpenFile(path) => self.on_open_file(event_loop, path),
            UserEvent::OpenFolders(want_tab, paths) => self.on_open_folders(event_loop, want_tab, paths),
            UserEvent::NoteClicked(pane) => self.on_note_clicked(event_loop, pane),
            #[cfg(target_os = "macos")]
            UserEvent::Script(cmd, ticket, reply) => self.on_script(event_loop, cmd, ticket, reply),
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
                            self.panes[idx].cwd_reset = false;
                        }
                    }
                    SessionEvent::CwdReset => {
                        self.panes[idx].cwd = None;
                        self.panes[idx].cwd_reset = true;
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
                            self.config.wait_after_command || self.panes[idx].wait_after_exit,
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
                        // progress-style = false면 조용히 버린다
                        self.panes[idx].progress = tabbar::accept_progress(self.config.progress_style, state, percent)
                            .map(|(s, p)| (s, p, Instant::now()));
                    }
                    SessionEvent::ClipboardStore(text) => self.on_clipboard_store(idx, text),
                    SessionEvent::ClipboardLoad(reply) => self.on_clipboard_load(idx, reply),
                    SessionEvent::KittyClipboardRead(req) => self.on_kitty_read(idx, req),
                    SessionEvent::KittyClipboardWrite(req) => self.on_kitty_write(idx, req),
                    SessionEvent::CommandStarted(cmd) => self.panes[idx].running = Some((cmd, Instant::now())),
                    SessionEvent::CommandFinished(rec) => self.on_command_finished(idx, rec),
                    SessionEvent::Bell => self.on_bell(idx),
                    SessionEvent::WindowResize { rows, cols } => self.on_window_resize_request(idx, rows, cols),
                    SessionEvent::ProgramStatus(alert) => self.on_program_status(idx, alert),
                    SessionEvent::Output => {}
                }
                self.request_redraw();
            }
            // 사용자 셰이더 컴파일이 끝났다(ADR-042): 끝난 창의 결과를 받아 끼우고 다시 그린다
            UserEvent::ShaderCompiled => self.poll_shaders(),
            UserEvent::Access(id, req) => self.on_access(id, req),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        // 사건을 받은 창을 지금 창으로(창마다 상태를 바꿔 끼운다, #36)
        if !self.switch_to(id) {
            return;
        }
        // 접근성 어댑터에 창 사건을 먼저 알린다(accesskit_winit의 요구, #560)
        if let Some(g) = self.w.gfx.as_mut() {
            g.a11y.process_event(&g.window, &event);
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
            // 파일마다 사건이 따로 온다. 같은 차례 것을 모아 `about_to_wait`에서 한 번에 넣는다(#566)
            WindowEvent::DroppedFile(path) => {
                if let Some((w, text)) = self.drop_batch.push(id, path) {
                    self.paste_dropped(w, text);
                    self.switch_to(id);
                }
            }
            WindowEvent::Occluded(occluded) => {
                self.w.occluded = occluded;
                self.sync_visibility();
                // 보이게 되면 가려진 동안 건너뛴 내용을 그린다(GPU 길은 층을 통째로, ADR-038).
                // 요청만 하면 winit이 이벤트 고리 끝까지 미뤄 빈 창이 0~49ms 더 보였다(실험 E12): 이 자리에서 바로 그린다
                if !occluded {
                    if let Some(g) = self.w.gfx.as_mut() {
                        g.first.became_visible();
                    }
                    self.redraw();
                }
            }
            WindowEvent::ThemeChanged(theme) => self.system_look_changed(theme == winit::window::Theme::Dark),
            WindowEvent::ModifiersChanged(m) => {
                self.raw_mods = keys::sided_mods(&m);
                self.refresh_mods();
                self.update_hover();
                // Shift를 누르고 떼면 모양이 바뀐다(#250). 링크 위가 그대로면 update_hover가 다시 정하지 않는다
                self.refresh_pointer();
            }
            WindowEvent::Focused(focused) => {
                // 조합 중인 한글은 포커스 아웃(ESC[O)을 쓰기 전에 옛 칸에 확정한다(#502)
                if !focused {
                    self.settle_preedit();
                }
                let was_focused = self.w.focused;
                self.w.focused = focused;
                if !focused {
                    self.w.tab_drag.cancel();
                    self.w.focus_click.clear();
                    self.w.kitty_down.clear();
                }
                // 보안 입력: 앱이 앞이면 켜고(창 어느 하나라도 초점이면), 아니면 끈다. 앞으로 오면 바로 칸을 살핀다(#258)
                self.update_secure_active();
                if focused {
                    self.next_secure_poll = Instant::now();
                    // 시스템 설정의 스크롤 막대(보기·클릭 시)는 앱이 앞으로 올 때 다시 읽는다(#630)
                    self.system_scroller = SystemScroller::read();
                    self.request_redraw();
                }
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
                // 퀵 터미널이 키 창이 되면 Space 동작을 정한다(quick-terminal-space-behavior, #196)
                if focused && self.w.quick {
                    crate::quick::apply_space_behavior(self.config.quick.space);
                }
                if focused {
                    // 창으로 돌아오면 보고 있던 탭의 안 읽음을 지운다
                    self.clear_visible_unread();
                    self.request_redraw();
                }
                // 창 상태가 그대로면 이미 알린 것이다(탭을 새 창으로 옮길 때 미리 정해 둔 경우, #223)
                if crate::winact::focus_event_changes(was_focused, focused) {
                    if let Some(t) = self.active_pane() {
                        if t.session.modes().focus_events {
                            t.session.write(if focused { b"\x1b[I" } else { b"\x1b[O" });
                        }
                    }
                }
            }
            WindowEvent::Ime(ime) => match ime {
                Ime::Preedit(text, _) => {
                    // 읽기 전용 칸에 쓸 글은 조합 중인 것도 보이지 않는다. 고른 글도 건드리지 않는다(#209)
                    let overlay = self.w.search.is_some()
                        || self.w.palette.is_some()
                        || self.w.title_prompt.is_some()
                        || self.w.confirm.is_some();
                    if !overlay && self.active_pane().is_some_and(|t| t.session.readonly()) {
                        return;
                    }
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
                // 입력기가 확정한 글도 남은 칸을 닫는 키다(#715. 한국어 입력기의 스페이스는 여기로 온다)
                Ime::Commit(text) => {
                    if !(crate::lifecycle::is_dismiss_commit(&text) && self.close_held_pane(event_loop)) {
                        self.commit_text(&text);
                    }
                }
                Ime::Enabled | Ime::Disabled => {
                    self.w.preedit.clear();
                    self.request_redraw();
                }
            },
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    self.kitty_key_release(&event);
                    return;
                }
                // 칸을 끌던 중 Esc는 끌기를 취소한다(키는 어디에도 가지 않는다)
                if self.w.pane_drag.is_active()
                    && matches!(
                        event.logical_key,
                        winit::keyboard::Key::Named(winit::keyboard::NamedKey::Escape)
                    )
                {
                    self.w.pane_drag.cancel();
                    self.finish_pane_drag(None);
                    return;
                }
                // 명령이 끝나 남겨 둔 칸은 아무 키나 누르면 닫는다(wait-after-command, #178)
                if crate::lifecycle::is_dismiss_key(&event.logical_key) && self.close_held_pane(event_loop) {
                    return;
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
                let mut modes = self.active_pane().map(|t| t.session.modes()).unwrap_or_default();
                // IME 조합 중에 눌린 키는 modifyOtherKeys로 바꾸지 않는다(확정 글자만 PTY로 간다, #312)
                if !self.w.preedit.is_empty() {
                    modes.modify_other_keys = false;
                }
                let meta = keys::option_counts_as_alt(self.config.option_as_alt, self.raw_mods);
                let report_release = modes.kitty.report_events;
                let bytes = keys::encode(&event, self.mods, keys::to_state(self.raw_mods), modes, meta);
                self.feed_key(event_loop, trigger, bytes, event.physical_key, report_release, None);
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.w.cursor_pos = position;
                if self.config.mouse_hide_while_typing {
                    if let Some(g) = &self.w.gfx {
                        g.window.set_cursor_visible(true);
                    }
                    self.w.pointer_hidden = false;
                }
                self.update_hover();
                self.update_handle_hover();
                self.refresh_pointer();
                let hot = self.gutter_pane();
                if hot != self.w.gutter_hot {
                    self.w.gutter_hot = hot;
                    self.request_redraw();
                }
                // 탭을 누르고 있으면 끌기 시작만 살핀다(프로그램에 보고하지 않고 글도 고르지 않는다)
                if self.w.tab_drag.is_pressed() {
                    if self.w.tab_drag.moved(position.x) {
                        self.w.tab_click = None;
                        self.request_redraw();
                    }
                    return;
                }
                // 칸을 끄는 중이면 놓을 자리만 따라간다(프로그램에 보고하지 않고 글도 고르지 않고 고른 칸도 바꾸지 않는다)
                if self.w.pane_drag.is_active() {
                    let target = self.drop_target_at_cursor();
                    if self.w.pane_drag.moved(target) {
                        self.request_redraw();
                    }
                    return;
                }
                // 경계선을 끄는 중이면 비율만 따라간다(프로그램에 보고하지 않고 글도 고르지 않고 고른 칸도 바꾸지 않는다)
                if self.w.divider.is_dragging() {
                    self.divider_drag_to(position);
                    return;
                }
                // 마우스가 올라간 칸을 고른다(focus-follows-mouse, #126). 고르는 중에는 바꾸지 않는다
                if self.config.focus_follows_mouse && !self.w.selecting {
                    if let Some(id) = self.pane_at(position).filter(|id| Some(*id) != self.focused_id()) {
                        self.focus_pane(id);
                    }
                }
                // 막대를 누르고 있으면 끌기만 한다(프로그램에 보고하지 않고 글도 고르지 않는다)
                if let Some(press) = self.w.scrollbar_press {
                    self.scrollbar_drag(press);
                    return;
                }
                let Some(idx) = self.active_index() else { return };
                let over = self.pane_at(position) == Some(self.panes[idx].id);
                let (input, before) = self.human_ptr(Some(idx));
                let mut st = before.clone();
                self.pointer_moved(idx, &input, &mut st, over);
                self.store_human_ptr(Some(idx), &before, &st);
            }
            WindowEvent::CursorLeft { .. } => {
                // 창을 벗어났다: 손잡이는 숨고, 끄는 중이면 놓을 자리가 없다(단추를 놓으면 취소)
                if self.w.handle_hover.take().is_some() {
                    self.request_redraw();
                }
                if self.w.pane_drag.is_active() && self.w.pane_drag.moved(None) {
                    self.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // 칸을 끌던 중이면 왼쪽 단추를 놓을 때 끝낸다. 끄는 동안 다른 누름은 아무 데도 가지 않는다
                if self.w.pane_drag.is_active() {
                    if button == MouseButton::Left && state == ElementState::Released {
                        let target = self.drop_target_at_cursor();
                        let outcome = self.w.pane_drag.release(target);
                        self.finish_pane_drag(outcome);
                    }
                    return;
                }
                // 경계선을 끌던 중이면 왼쪽 단추를 놓을 때 끝낸다. 끄는 동안 다른 누름은 아무 데도 가지 않는다
                if self.w.divider.is_dragging() {
                    if button == MouseButton::Left && state == ElementState::Released {
                        self.w.divider.end();
                        self.refresh_pointer();
                    }
                    return;
                }
                // 탭을 누르던(끌던) 중이면 왼쪽 단추를 놓을 때 끝내고, 놓은 자리로 옮긴다. 그동안 다른 누름은 아무 데도 가지 않는다.
                // 놓기를 놓친 채 새 왼쪽 누름이 오면 옛 누름을 버리고 새 누름을 처리한다
                if self.w.tab_drag.is_pressed() {
                    if button == MouseButton::Left && state == ElementState::Released {
                        let width = self.w.gfx.as_ref().map(|g| g.window.inner_size().width).unwrap_or(0);
                        let drop = self.w.tab_drag.release(self.w.cursor_pos.x, width, self.w.tabs.len());
                        if let tabbar::TabDrop::Move { from, to } = drop {
                            self.reorder_tab(from, to);
                        }
                        return;
                    }
                    if !(button == MouseButton::Left && state == ElementState::Pressed) {
                        return;
                    }
                    self.w.tab_drag.cancel();
                }
                // 막대를 누르던 중 왼쪽 단추를 놓으면 끝낸다. 놓은 뒤 평소처럼 사라지는 시간을 다시 센다
                if let Some(press) = self.w.scrollbar_press {
                    if button == MouseButton::Left && state == ElementState::Released {
                        self.w.scrollbar_press = None;
                        if let Some(i) = self.pane_index(press.pane) {
                            self.panes[i].scrollbar_until = Some(Instant::now() + SCROLLBAR_FOR);
                        }
                        self.request_redraw();
                    }
                    return;
                }
                // 제목 막대 자리(콘텐츠가 그 밑까지 들어간 창)는 칸이 아니다
                let top_inset = self.top_inset();
                if (self.w.cursor_pos.y as u32) < top_inset {
                    return;
                }
                // 탭 줄을 누르면 그 탭으로
                if (self.w.cursor_pos.y as u32) < top_inset + self.tabbar_height() {
                    if state == ElementState::Pressed && matches!(button, MouseButton::Left | MouseButton::Middle) {
                        let width = self.w.gfx.as_ref().map(|g| g.window.inner_size().width).unwrap_or(0);
                        let hit = tabbar::hit(width, self.w.cursor_pos.x, self.w.tabs.len());
                        // 확인 막대가 떠 있으면 탭 줄의 새 동작(가운데 클릭 닫기, 두 번 눌러 제목)은 하지 않는다.
                        // 키는 확인 막대가 모두 가로채지만 마우스는 여기로 오므로, 열린 확인을 덮거나 확인 없이 닫지 않게 한다
                        let modal = self.w.confirm.is_some();
                        if let (Some(i), MouseButton::Middle, false) = (hit, button, modal) {
                            // 가운데 클릭은 그 탭을 닫는다(Ghostty). `close_tab` 키와 같은 길이라 프로그램이 돌면 확인을 지난다
                            if let Some(id) = self.w.tabs.get(i).map(|g| g.focus) {
                                drop(self.redo.clear());
                                self.request(Pending::CloseTab(id), event_loop);
                            }
                        } else if let (Some(i), MouseButton::Left) = (hit, button) {
                            self.activate(i);
                            let now = Instant::now();
                            let interval = mouse::repeat_interval(
                                self.config.click_repeat_interval,
                                mouse::platform_repeat_interval(),
                            );
                            if !modal && tabbar::is_double_click(self.w.tab_click, now, i, interval) {
                                // 두 번 누르면 그 탭의 제목 입력 막대를 연다(Ghostty는 그 자리에서 고치지만 여기는 입력 막대)
                                self.w.tab_click = None;
                                self.handle_action(Action::PromptTabTitle, event_loop);
                            } else {
                                self.w.tab_click = Some((now, i));
                                self.w.tab_drag.press(i, self.w.cursor_pos.x);
                            }
                        }
                    }
                    return;
                }
                // 경계선을 누르면 끌기를 시작한다(두 번 누르면 칸 크기를 고르게). 프로그램(마우스 보고)·고르기에는 보내지 않는다
                if state == ElementState::Pressed && button == MouseButton::Left {
                    if let Some(d) = self.divider_under_cursor() {
                        self.divider_press(d);
                        return;
                    }
                }
                // 칸 끌기 손잡이를 누르면 끌기를 시작한다. 프로그램(마우스 보고)·고르기에는 보내지 않는다
                if state == ElementState::Pressed && button == MouseButton::Left {
                    if let Some(id) = self.handle_under_cursor() {
                        if self.w.pane_drag.press(Some(id)) {
                            self.refresh_pointer();
                            self.request_redraw();
                            return;
                        }
                    }
                }
                // Cmd-클릭: 링크를 연다(#705, 마우스 보고가 꺼져 있으면 같은 칸에서 뗄 때. 누름은 고르기로 간다)
                if let Some(mb) = mouse_core::report_button(button) {
                    let (input, _) = self.human_ptr(None);
                    let mut link_click = self.w.link_click;
                    let clicks = self.w.last_click.map_or(0, |l| l.3);
                    let handled =
                        self.link_click_step(mb, state == ElementState::Pressed, &input, &mut link_click, clicks);
                    self.w.link_click = link_click;
                    if handled {
                        return;
                    }
                }
                // 막대 띠에서 왼쪽 단추를 누르면 스크롤한다(막대가 있을 때만). 프로그램·고르기에는 보내지 않는다
                if state == ElementState::Pressed && button == MouseButton::Left && self.scrollbar_press_start() {
                    return;
                }
                // 삼킨 누름의 짝 떼기는 프로그램(마우스 보고)·고르기·click-to-move 어디에도 보내지 않는다(#500)
                if button == MouseButton::Left && state == ElementState::Released && self.w.focus_click.release_left() {
                    return;
                }
                // 다른 칸을 누르면 그 칸을 고른다. 왼쪽 단추 누름은 칸을 고르는 데만 쓰고 삼킨다(#500, Ghostty).
                // 삼킨 누름은 누름 횟수에도 세지 않으니 옛 기록도 지운다. 오른쪽·가운데 단추는 고른 칸이 평소처럼 받는다.
                // 창이 뒤에 있다가 이 누름으로 앞에 오는 경우는 가르지 못한다: winit(macOS)가 windowDidBecomeKey의
                // Focused(true)를 이 누름보다 먼저 올리므로 둘이 같아 보인다(GTK처럼 고른 칸이 아니면 삼킨다)
                if state == ElementState::Pressed {
                    let under = self.pane_at(self.w.cursor_pos);
                    let focused = self.focused_id();
                    let left = button == MouseButton::Left;
                    let swallow = mouse::swallow_focus_click(left, under, focused);
                    if left {
                        self.w.focus_click.press_left(swallow);
                    }
                    if let Some(id) = under.filter(|id| Some(*id) != focused) {
                        self.focus_pane(id);
                    }
                    if swallow {
                        self.w.last_click = None;
                        return;
                    }
                }
                let Some(idx) = self.active_index() else { return };
                let Some(mb) = mouse_core::report_button(button) else {
                    return;
                };
                let (input, before) = self.human_ptr(Some(idx));
                let mut st = before.clone();
                self.pointer_button(idx, mb, state == ElementState::Pressed, &input, &mut st);
                self.store_human_ptr(Some(idx), &before, &st);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // 휠 배율(mouse-scroll-multiplier, #126): 줄 단위 기기(마우스 휠)와 픽셀 단위 기기(트랙패드)
                let (discrete, precision) = self.config.scroll_multiplier;
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64 * discrete,
                    MouseScrollDelta::PixelDelta(p) => {
                        // 마우스 아래 칸의 줄 높이(#422): 칸마다 글자 크기가 다르다
                        let ch = self.wheel_cell().map_or(16.0, |m| m.height as f64);
                        p.y / ch * precision
                    }
                };
                let xlines = match delta {
                    MouseScrollDelta::LineDelta(x, _) => x as f64 * discrete,
                    MouseScrollDelta::PixelDelta(p) => {
                        let cw = self.wheel_cell().map_or(8.0, |m| m.width as f64);
                        p.x / cw * precision
                    }
                };
                // 휠은 마우스 아래 칸이 받는다(고르지 않은 칸도 굴려 볼 수 있게)
                let idx = self
                    .pane_at(self.w.cursor_pos)
                    .and_then(|id| self.pane_index(id))
                    .or_else(|| self.active_index());
                let (input, before) = self.human_ptr(idx);
                let mut st = before.clone();
                self.pointer_wheel(idx, lines, xlines, &input, &mut st);
                self.store_human_ptr(idx, &before, &st);
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
        if let Some((window, text)) = self.drop_batch.take() {
            // 놓은 창으로 바꿔 넣고, 아래 일들이 보던 지금 창으로 되돌린다(DroppedFile 갈래와 같게)
            let back = self.w.id();
            self.paste_dropped(window, text);
            if let Some(back) = back {
                self.switch_to(back);
            }
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
        // 접근성(#560): 미뤄 둔 화면 글 갱신 때가 된 창을 갱신한다
        self.a11y_tick(now);
        // 탭을 옮기거나 창으로 옮긴 칸이 보이는 상태가 달라졌으면 알린다(#303). 같은 상태는 세션이 걸러 낸다
        self.sync_visibility();
        // 암호 입력 살피기(#258): 앱이 앞이고 창에 초점이 있을 때만 200ms마다 깨운다
        if self.any_window_focused() && now >= self.next_secure_poll {
            self.next_secure_poll = now + SECURE_POLL;
            if self.poll_password() {
                self.request_redraw_all();
            }
        }
        self.sync_secure_focus();
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
        // kitty 애니메이션(#554): 다음 프레임 때가 된 창을 다시 그린다. 가려진 창은 깨우지 않는다
        self.fire_kitty_wakes(now);
        // 미뤄 둔 GPU 프레임을 다시 그릴 때가 됐다(ADR-038)
        self.fire_pace_wakes(now);
        // 기한이 지난 닫은 칸·탭은 이때 떨어뜨린다(`Session`의 `Drop`이 셸에 SIGHUP을 보낸다, #164·#651)
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
        // 스크롤 막대를 보이던 기한이 끝나면 지우고 다시 그린다
        for p in &mut self.panes {
            if p.scrollbar_until.is_some_and(|t| t <= now) {
                p.scrollbar_until = None;
                overlay_done = true;
            }
        }
        if overlay_done {
            self.request_redraw_all();
        }
        let overlay_next = self
            .panes
            .iter()
            .flat_map(|p| [p.overlay_until, p.scrollbar_until])
            .flatten()
            .min();
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
        // 사용자 셰이더 애니메이션(#376): 움직이는 창은 8ms마다 다시 그린다(Ghostty `draw_interval_ms`). 가려진 창은 깨우지 않는다
        let mut shader_next: Option<Instant> = None;
        for w in std::iter::once(&mut self.w).chain(self.others.iter_mut()) {
            let has = w
                .gfx
                .as_ref()
                .and_then(|g| g.gpu.as_ref())
                .is_some_and(|g| g.has_custom_shaders());
            let animates =
                !w.occluded && shader_inputs::shader_animates(has, self.config.custom_shader_animation, w.focused);
            let (fire, next) = shader_inputs::shader_wake(w.shader.wake, now, animates);
            w.shader.wake = next;
            if fire {
                if let Some(g) = &w.gfx {
                    g.window.request_redraw();
                }
            }
            shader_next = shader_next.into_iter().chain(next).min();
        }
        let blink_next = std::iter::once(&self.w)
            .chain(&self.others)
            .filter(|w| w.blink_wanted)
            .map(|w| w.blink_next)
            .min();
        let kitty_next = std::iter::once(&self.w)
            .chain(&self.others)
            .filter(|w| !w.occluded)
            .filter_map(|w| w.kitty_wake)
            .min();
        // 동기화 출력(DEC 2026)이 끝나지 않은 탭이 있으면 기한에 깨어 반영한다
        let deadline = self
            .panes
            .iter()
            .filter_map(|t| t.session.sync_deadline())
            .chain([self.next_poll])
            .chain(blink_next)
            .chain(kitty_next)
            .chain(anim_next)
            .chain(shader_next)
            .chain(overlay_next)
            .chain(self.undo.next_deadline())
            .chain(self.redo.next_deadline())
            .chain(
                std::iter::once(&self.w)
                    .chain(&self.others)
                    .filter_map(|w| w.pace_wake)
                    .min(),
            )
            .chain(self.quit_deadline)
            .chain(self.a11y_next_wake())
            .chain(self.any_window_focused().then_some(self.next_secure_poll))
            .min();
        match deadline {
            Some(d) if d <= Instant::now() => {
                // 깨울 때가 방금 지났을 수 있다: 아래에서 Wait으로 잠들면 미룬 프레임이 영영 안 올라간다
                self.fire_pace_wakes(Instant::now());
                self.fire_kitty_wakes(Instant::now());
                if self.panes.iter().any(|t| t.session.flush_expired_sync()) {
                    self.request_redraw_all();
                }
                // 움직이는 중이면 다음 프레임에 다시 깨어난다(기한이 이미 지나 잠들어 버리지 않게)
                event_loop.set_control_flow(if animating {
                    ControlFlow::WaitUntil(Instant::now() + QUICK_FRAME)
                } else if let Some(t) = shader_next {
                    ControlFlow::WaitUntil(t.max(Instant::now()))
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

    #[test]
    fn ime_commits_on_focus_loss_only_for_korean() {
        for s in [
            "com.apple.inputmethod.Korean.2SetKorean",
            "com.apple.inputmethod.Korean.3SetKorean",
        ] {
            assert!(ime_commits_on_focus_loss(Some(s)), "{s}");
        }
        for s in [
            "com.apple.inputmethod.Japanese",
            "com.apple.inputmethod.TCIM.Cangjie",
            "com.apple.inputmethod.SCIM.ITABC",
            "com.apple.keylayout.ABC",
        ] {
            assert!(!ime_commits_on_focus_loss(Some(s)), "{s}");
        }
        assert!(!ime_commits_on_focus_loss(None));
    }

    /// vendor 함수가 실제로 이름 붙은 pasteboard에 쓰고 읽는다(시험 전용 이름만 쓰고 끝나면 비운다. 일반 클립보드는 건드리지 않는다).
    /// 앱 묶음 없이 돈다는 것도 이 시험이 보인다. 시스템 이름(일반 클립보드)은 거절한다
    #[cfg(target_os = "macos")]
    #[test]
    fn named_pasteboard_round_trips_and_refuses_system_names() {
        use winit::platform::macos::{
            named_pasteboard_change_count, named_pasteboard_string, release_named_pasteboard,
            set_named_pasteboard_string,
        };
        let name = format!("com.dj258255.b-terminal.test-{}", std::process::id());
        let c1 = set_named_pasteboard_string(&name, "고른 글 abc");
        assert!(c1.is_some());
        assert_eq!(named_pasteboard_string(&name, 1024).as_deref(), Some("고른 글 abc"));
        assert_eq!(named_pasteboard_change_count(&name), c1);
        let c2 = set_named_pasteboard_string(&name, "둘째");
        assert!(c2.is_some() && c2 != c1);
        assert_eq!(named_pasteboard_string(&name, 1024).as_deref(), Some("둘째"));
        // 상한(UTF-16 단위)을 넘는 글은 복사하지 않고 없다고 본다
        assert_eq!(named_pasteboard_string(&name, 1), None);
        release_named_pasteboard(&name);
        assert_eq!(set_named_pasteboard_string("Apple CFPasteboard general", "x"), None);
        assert_eq!(named_pasteboard_string("Apple CFPasteboard general", 1024), None);
        assert_eq!(named_pasteboard_change_count("Apple CFPasteboard general"), None);
        assert_eq!(set_named_pasteboard_string("", "x"), None);
    }

    /// 암호 입력 중이거나 보안 입력이 켜진 칸에서 고른 글은 다른 프로세스가 읽을 수 있는 pasteboard에 쓰지 않는다
    /// `copy-on-select`의 자동 복사: 암호 입력 중이거나 보안 입력이면 시스템 클립보드로 보내지 않는다(선택 클립보드는 앱 안에 둔다)
    #[test]
    fn auto_copy_skips_system_clipboard_when_private() {
        use bt_core::config::CopyOnSelect;
        assert_eq!(auto_copy_targets(CopyOnSelect::Clipboard, false), (false, true));
        assert_eq!(auto_copy_targets(CopyOnSelect::Clipboard, true), (false, false));
        assert_eq!(auto_copy_targets(CopyOnSelect::Both, true), (true, false));
        assert_eq!(auto_copy_targets(CopyOnSelect::Both, false), (true, true));
        assert_eq!(auto_copy_targets(CopyOnSelect::Primary, true), (true, false));
        assert_eq!(auto_copy_targets(CopyOnSelect::None, false), (false, false));
    }

    #[test]
    fn password_pane_selection_is_not_shared() {
        assert!(share_selection(false, false));
        assert!(!share_selection(true, false));
        assert!(!share_selection(false, true));
        assert!(!share_selection(true, true));
        // 붙여넣기처럼 칸의 마지막 살핀 값이 거짓이어도 termios를 다시 읽어 암호 입력이면 공유하지 않는다(살핌 주기 200ms 사이)
        assert!(!share_selection(paste_hidden(false, Some(true)), false));
        assert!(share_selection(paste_hidden(false, None), false));
    }

    /// 선택 클립보드(#629): 이 앱이 마지막으로 고른 뒤 pasteboard가 바뀌었으면(다른 b-terminal이 나중에 골랐다) 그 글,
    /// 아니면 앱 안 저장소. 암호 칸에서 방금 고른 글(공유하지 않음)이 예전에 공유된 글에 밀리지 않는다(PR #637 보안 검토)
    #[test]
    fn selection_prefers_what_changed_last() {
        let other = || Some("other".to_string());
        // 다른 쪽이 그 뒤에 썼다(바뀜 번호가 내가 적어 둔 것과 다르다)
        assert_eq!(pick_selection(other(), Some(8), Some(7), Some("mine")), other());
        // 내가 고른 뒤 아무도 쓰지 않았다: 앱 안 저장소(암호 칸에서 고른 글이면 공유되지 않은 그 글)
        assert_eq!(
            pick_selection(other(), Some(7), Some(7), Some("mine")),
            Some("mine".to_string())
        );
        // 이 앱에서 아직 고른 적이 없다: 공유된 글
        assert_eq!(pick_selection(other(), Some(3), None, None), other());
        // 공유된 글이 없거나 비었다
        assert_eq!(
            pick_selection(None, Some(8), Some(7), Some("mine")),
            Some("mine".to_string())
        );
        assert_eq!(
            pick_selection(Some(String::new()), Some(8), Some(7), Some("mine")),
            Some("mine".to_string())
        );
        // 앱 안 저장소가 비었으면 바뀌지 않았어도 공유된 글
        assert_eq!(pick_selection(other(), Some(7), Some(7), None), other());
        assert_eq!(pick_selection(None, None, None, None), None);
        assert_eq!(pick_selection(Some(String::new()), None, None, Some("")), None);
    }

    #[test]
    fn mouse_mode_counts_x10_like_ghostty() {
        // 엔진은 DECSET 9를 켜면 다른 마우스 모드를 모두 지우고 X10만 남긴다. 그래도 보고 모드다(Ghostty `mouse_event`)
        let m = |f: fn(&mut bt_term::Modes)| {
            let mut m = bt_term::Modes::default();
            f(&mut m);
            App::mouse_mode_on(m)
        };
        assert!(!m(|_| {}));
        assert!(m(|m| m.mouse_x10 = true), "X10만 켠 칸");
        assert!(m(|m| m.mouse_report = true));
        assert!(m(|m| m.mouse_drag = true));
        assert!(m(|m| m.mouse_motion = true));
    }

    #[test]
    fn inherited_cwd_forgets_process_cwd_after_empty_osc7() {
        let p = || Some(std::path::PathBuf::from("/proc/cwd"));
        let rep = std::path::Path::new("/reported");
        assert_eq!(inherited_cwd(Some(rep), false, p), Some("/reported".into()));
        assert_eq!(
            inherited_cwd(None, false, p),
            Some("/proc/cwd".into()),
            "알린 적 없으면 프로세스"
        );
        // 빈 OSC 7로 지운 칸: 프로세스 폴더를 몰래 쓰지 않는다(Ghostty는 알려진 폴더가 없으면 기본 폴더)
        assert_eq!(inherited_cwd(None, true, p), None);
        // 지운 뒤 다시 알리면 그 폴더
        assert_eq!(inherited_cwd(Some(rep), true, p), Some("/reported".into()));
    }

    #[test]
    fn hovered_link_underline_is_double_only_over_a_single_underline_like_ghostty() {
        use bt_render::Underline as U;
        let f = |flags: &[CellFlags]| flags.iter().fold(CellFlags::empty(), |a, &b| a | b);
        assert_eq!(link_underline(f(&[])), U::Single, "밑줄 없는 칸은 곧은 줄");
        assert_eq!(
            link_underline(f(&[CellFlags::UNDERLINE])),
            U::Double,
            "곧은 밑줄 칸은 이중"
        );
        for shape in [
            CellFlags::DOUBLE_UNDERLINE,
            CellFlags::UNDERCURL,
            CellFlags::DOTTED_UNDERLINE,
            CellFlags::DASHED_UNDERLINE,
        ] {
            // 엔진은 모양 밑줄에도 UNDERLINE을 함께 켠다
            assert_eq!(
                link_underline(f(&[CellFlags::UNDERLINE, shape])),
                U::Single,
                "{shape:?}"
            );
        }
        assert_eq!(
            link_underline(f(&[CellFlags::HIDDEN, CellFlags::UNDERLINE])),
            U::None,
            "숨김 칸은 링크여도 꾸밈 없음"
        );
        assert_eq!(link_underline(f(&[CellFlags::HIDDEN])), U::None);
    }
    use bt_term::search::SearchStatus;

    #[test]
    fn titlebar_keeps_the_unreversed_background_under_decscnm() {
        use bt_core::Rgb;
        let bg = Rgb::new(10, 20, 30);
        assert_eq!(titlebar_fill(28, 800, true, bg), Some((0, 0, 800, 28, bg)));
        assert_eq!(titlebar_fill(28, 800, false, bg), None, "반전이 아니면 건드리지 않는다");
        assert_eq!(
            titlebar_fill(0, 800, true, bg),
            None,
            "제목 막대가 겹치지 않으면 영향 없다"
        );
    }

    #[test]
    fn reversed_screen_swaps_only_the_default_cursor_colors() {
        use bt_core::Rgb;
        let theme = bt_core::Theme::default();
        // 반전이 아니면 테마 그대로
        assert_eq!(
            reversed_cursor_defaults(&theme, false),
            (theme.cursor, theme.cursor_text)
        );
        // 반전이면 정하지 않은 커서(= 글자색)는 바탕색, 커서 글자(= 바탕색)는 글자색(#302)
        assert_eq!(
            reversed_cursor_defaults(&theme, true),
            (theme.background, theme.foreground)
        );
        // 설정으로 정한 색은 그대로
        let set = bt_core::Theme {
            cursor: Rgb::new(9, 9, 9),
            cursor_text: Rgb::new(8, 8, 8),
            ..theme
        };
        assert_eq!(
            reversed_cursor_defaults(&set, true),
            (Rgb::new(9, 9, 9), Rgb::new(8, 8, 8))
        );
    }

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

    /// kitty 그래픽 자리표시 글자(U+10EEEE, #553)와 붙은 결합 문자는 글자로 그리지 않는다(빈 네모가 찍히지 않게). 화면 값에는 그대로 남는다
    #[test]
    fn placeholder_cells_are_drawn_blank() {
        use bt_term::{Emulator, GridSize};
        let theme = bt_core::Theme::default();
        let mut e = Emulator::new(GridSize::new(8, 1), theme.clone());
        e.feed("a\u{10EEEE}\u{0305}\u{030D}\u{10EEEE}b".as_bytes());
        let s = e.snapshot();
        let cells = to_draw_cells(&s, &theme);
        assert_eq!(cells[0].ch, 'a');
        assert_eq!((cells[1].ch, cells[2].ch, cells[3].ch), (' ', ' ', 'b'));
        assert!(
            cells.iter().all(|c| c.cluster.is_none()),
            "결합 문자를 묶음으로 그리지 않는다"
        );
        assert_eq!(
            s.row_text(0),
            "a\u{10EEEE}\u{0305}\u{030D}\u{10EEEE}b",
            "화면 읽기에는 글자가 그대로"
        );
    }

    /// Dock에 놓은 파일을 실행하는 확인(#279)은 Enter로 허용하지 않고, 어느 칸에도 매이지 않는다
    #[test]
    fn dropped_file_prompt_needs_y_and_binds_to_no_pane() {
        let p = Pending::OpenFile {
            cwd: "/tmp".into(),
            input: "'/tmp/a.sh'; exit\n".into(),
        };
        assert!(p.is_program_prompt());
        assert!(!p.binds_to(&[1, 2, 3]));
        assert!(!Pending::Quit.is_program_prompt());
    }

    /// kitty 클립보드 확인(#325)도 프로그램의 출력이 띄운 막대라 Enter로 허용하지 않고, 그 칸에 매인다
    #[test]
    fn kitty_clipboard_prompts_need_y_and_bind_to_their_pane() {
        let read = Pending::KittyRead {
            pane: 7,
            req: bt_term::kitty_clip::ReadRequest {
                primary: false,
                id: String::new(),
                mimes: vec!["text/plain".into()],
                list: false,
                pw: String::new(),
                name: String::new(),
                granted: false,
                bel: false,
            },
            text: "x".into(),
        };
        let write = Pending::KittyWrite {
            pane: 7,
            req: bt_term::kitty_clip::WriteRequest {
                primary: false,
                id: String::new(),
                pw: String::new(),
                name: String::new(),
                contents: vec![],
                granted: false,
                bel: false,
            },
            text: "x".into(),
        };
        for p in [read, write] {
            assert!(p.is_program_prompt());
            assert!(p.binds_to(&[7]) && !p.binds_to(&[1, 2]));
        }
    }

    #[test]
    fn confirm_bar_binds_to_moved_panes_only() {
        let moved = [4u64, 5];
        assert!(Pending::ClosePane(5).binds_to(&moved));
        assert!(!Pending::ClosePane(1).binds_to(&moved));
        assert!(Pending::CloseTab(4).binds_to(&moved));
        assert!(!Pending::CloseTab(2).binds_to(&moved));
        assert!(
            Pending::Paste {
                pane: 4,
                text: "a\nb".into()
            }
            .binds_to(&moved)
        );
        assert!(
            !Pending::Paste {
                pane: 9,
                text: "a\nb".into()
            }
            .binds_to(&moved)
        );
        // 창·앱 전체를 묻는 막대는 남는 창의 것이다
        // 클립보드 물음도 그 칸에 매여 있다(#220)
        assert!(
            Pending::ClipboardWrite {
                pane: 5,
                text: "x".into()
            }
            .binds_to(&moved)
        );
        assert!(
            !Pending::ClipboardWrite {
                pane: 1,
                text: "x".into()
            }
            .binds_to(&moved)
        );
        assert!(!Pending::CloseWindow.binds_to(&moved));
        assert!(!Pending::CloseAllWindows.binds_to(&moved));
        assert!(!Pending::Quit.binds_to(&moved));
    }

    #[test]
    fn saved_titles_are_cleaned_again_on_restore() {
        // 상태 파일에서 읽은 제목도 사람이 친 제목과 같은 규칙(제어 문자 지우기, 200자, 빈 글은 없음)을 거친다
        let from_file = |t: &str| Some(t.to_string()).as_deref().and_then(clean_user_title);
        let t = from_file("빌드\x1b]0;x\x07 서버").unwrap();
        assert!(!t.chars().any(char::is_control), "{t:?}");
        assert_eq!(from_file("  \n"), None);
        assert_eq!(from_file(&"가".repeat(500)).map(|t| t.chars().count()), Some(200));
    }

    #[test]
    fn user_title_wins_over_program_title() {
        assert_eq!(title_of(None, "vim", Some("zsh"), false, false), "vim");
        assert_eq!(title_of(None, "", Some("zsh"), false, false), "zsh");
        assert_eq!(title_of(None, "", None, false, false), "셸");
        assert_eq!(
            title_of(Some("빌드"), "vim", Some("zsh"), false, false),
            "빌드",
            "사람이 붙인 제목이 먼저"
        );
        assert_eq!(title_of(Some("빌드"), "vim", None, true, false), "🔔 빌드");
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
    fn title_precedence_window_and_tab() {
        // 창 제목: 설정 title > 창 덮어쓰기 > 탭 제목
        assert_eq!(window_title_of(Some("고정"), Some("창"), "탭"), "고정");
        assert_eq!(window_title_of(None, Some("창"), "탭"), "창");
        assert_eq!(window_title_of(None, None, "탭"), "탭");
        // 탭 제목을 덮어쓰면 칸 제목(사람이 붙인 것·프로그램이 정한 것)보다 먼저
        assert_eq!(title_of(Some("탭"), "vim", Some("zsh"), false, false), "탭");
        assert_eq!(title_of(None, "vim", Some("zsh"), false, false), "vim");
    }

    #[test]
    fn copy_title_text_is_decided_purely() {
        assert_eq!(title_to_copy(None, "vim"), Some("vim".into()));
        assert_eq!(
            title_to_copy(Some("빌드"), "vim"),
            Some("빌드".into()),
            "사람이 붙인 제목이 먼저"
        );
        assert_eq!(title_to_copy(None, ""), None, "제목이 없으면 하지 않는다");
        assert_eq!(title_to_copy(Some(""), ""), None);
        assert_eq!(title_to_copy(None, "  \t"), None, "공백뿐이면 비었다");
        assert_eq!(
            title_to_copy(None, "a\x1b[31mb"),
            Some("a [31mb".into()),
            "제어 문자를 지운다"
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
    fn since_ms_is_signed() {
        let a = Instant::now();
        let b = a + Duration::from_micros(7_500);
        assert!((since_ms(a, b) - 7.5).abs() < 1e-9, "뒤면 양수");
        assert!((since_ms(b, a) + 7.5).abs() < 1e-9, "앞이면 음수");
        assert_eq!(since_ms(a, a), 0.0);
    }

    #[test]
    fn paste_prompt_counts_lines_and_strips_control_chars() {
        let p = paste_prompt("\n  \necho \x1b[31mhi\r\nrm -rf ~\n\n", false);
        assert!(p.contains("(2줄)"), "{p}");
        assert!(p.ends_with("echo  [31mhi"), "첫 글 줄, 제어 문자는 빈칸: {p}");
        assert!(!p.contains('\x1b'));
        let long = paste_prompt(&("x".repeat(100) + "\ny"), false);
        assert!(long.ends_with('…'), "{long}");
    }

    #[test]
    fn paste_prompt_hides_the_text_at_a_password_prompt() {
        let p = paste_prompt("hunter2secret\nrm -rf ~\n", true);
        assert!(
            !p.contains("hunter2"),
            "암호 입력이면 글이 막대에 들어가면 안 된다: {p}"
        );
        assert!(!p.contains("rm -rf"), "{p}");
        assert!(p.contains("(2줄)"), "줄 수는 보인다: {p}");
        assert!(p.contains("암호 입력 중"), "{p}");
    }

    #[test]
    fn paste_hidden_when_either_reading_says_password() {
        assert!(!paste_hidden(false, None));
        assert!(!paste_hidden(false, Some(false)));
        assert!(paste_hidden(true, None), "읽지 못해도 마지막 살핀 값이 참이면 숨긴다");
        assert!(paste_hidden(true, Some(false)));
        assert!(
            paste_hidden(false, Some(true)),
            "200ms 사이에 켜진 것은 다시 읽은 값으로 잡는다"
        );
    }

    #[test]
    fn readonly_panes_always_ask_before_closing() {
        use bt_core::config::ConfirmClose::{Always, False, True};
        // 프롬프트에서 쉬는 칸도, 셸이 이미 끝난 칸도, confirm-close-surface = false여도 읽기 전용이면 묻는다(Ghostty 주석: always)
        assert!(pane_busy(True, false, true, false, Some(true), true));
        assert!(pane_busy(True, true, true, false, None, true));
        assert!(pane_busy(False, false, true, false, Some(true), true));
        assert!(pane_busy(Always, false, true, false, Some(true), true));
        // 읽기 전용이 아니면 그대로
        assert!(!pane_busy(True, false, true, false, Some(true), false));
    }

    #[test]
    fn readonly_marker_leads_the_title() {
        assert_eq!(title_of(None, "vim", None, false, true), "🔒 vim");
        assert_eq!(title_of(None, "vim", None, true, true), "🔔 🔒 vim");
        assert_eq!(title_of(None, "vim", None, false, false), "vim");
    }

    #[test]
    fn busy_panes_need_confirm_like_ghostty() {
        use bt_core::config::ConfirmClose::{Always, False, True};
        // 셸 통합이 있으면 명령이 도는지만 본다
        assert!(pane_busy(True, false, true, true, Some(true), false));
        assert!(
            !pane_busy(True, false, true, false, Some(false), false),
            "프롬프트에 있으면 묻지 않는다"
        );
        // 셸 통합이 없으면 전경 프로세스가 셸인지 본다. 모르면 묻는다
        assert!(!pane_busy(True, false, false, false, Some(true), false));
        assert!(pane_busy(True, false, false, false, Some(false), false));
        assert!(pane_busy(True, false, false, false, None, false));
        // always는 프롬프트에서도, false는 언제나 묻지 않음
        assert!(pane_busy(Always, false, true, false, Some(true), false));
        assert!(!pane_busy(False, false, true, true, Some(false), false));
        // 셸이 이미 끝난 칸은 묻지 않는다
        assert!(!pane_busy(Always, true, true, true, None, false));
        assert!(!pane_busy(True, true, false, false, None, false));
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
                border: false,
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
                border: false,
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
                border: false,
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
            border: false,
        };
        assert_eq!(
            r(f, false, true, true),
            BellResponse {
                title: false,
                border: false,
                attention: false,
                beep: true,
                audio: false
            }
        );
        assert!(!r(f, false, false, false).attention);
    }

    #[test]
    fn bell_border_lights_even_when_watching_and_only_when_enabled() {
        use bt_core::config::BellFeatures;
        let on = BellFeatures {
            border: true,
            ..BellFeatures::default()
        };
        // 테두리는 보고 있어도(창이 앞, 칸이 보임) 켠다. 🔔 제목과 다르다(#687)
        assert!(bell_response(on, false, true, true).border);
        assert!(bell_response(on, false, false, false).border);
        assert!(!bell_response(on, false, true, true).title);
        // 끄면(기본) 켜지 않는다
        assert!(!bell_response(BellFeatures::default(), false, false, false).border);
        // 지우는 때: 창이 앞이고 그 칸이 키를 받는 칸일 때만
        assert!(bell_border_cleared_on_focus(true, true));
        assert!(!bell_border_cleared_on_focus(true, false));
        assert!(!bell_border_cleared_on_focus(false, true));
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
            underline: bt_render::Underline::None,
            strike: false,
            wide: false,
            spacer: false,
            selected: false,
            emoji: false,
            cluster: None,
            cluster_cells: 0,
            overline: false,
            underline_color: None,
        };
        let (cols, rows) = (4, 3);
        let cells = vec![blank.clone(); cols * rows];
        let cur = |row| {
            Some(DrawCursor {
                col: 0,
                row,
                style: bt_render::CursorStyle::Block,
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

    /// 칸마다 글자 크기가 달라(#422) 같은 격자 자리도 칸마다 다른 열·줄 수가 된다. 칸 크기는 그 칸의 크기에서 온다
    #[test]
    fn the_same_area_holds_a_different_grid_per_font_size() {
        let at = |size: f32| {
            Fonts::new(&FontConfig {
                size,
                scale: 1.0,
                ..FontConfig::default()
            })
            .expect("넣어 둔 글꼴은 늘 있다")
            .metrics()
        };
        let (small, big) = (at(13.0), at(26.0));
        let inner = Rect::new(0, 0, 800, 400);
        let (a, b) = (grid_for(inner, &small), grid_for(inner, &big));
        assert_eq!((a.cell_width as u32, a.cell_height as u32), (small.width, small.height));
        assert_eq!((b.cell_width as u32, b.cell_height as u32), (big.width, big.height));
        assert_eq!((a.cols as u32, a.rows as u32), (800 / small.width, 400 / small.height));
        assert_eq!((b.cols as u32, b.rows as u32), (800 / big.width, 400 / big.height));
        assert!(a.cols > b.cols && a.rows > b.rows, "작은 글자 칸에 열·줄이 더 들어간다");
    }

    /// 보안 검토(#36): 새 창은 포커스 사건이 오기 전까지 보고 있지 않은 창이다(클립보드 쓰기를 받지 않는다)
    #[test]
    fn new_window_state_is_not_focused_until_told() {
        assert!(!Win::new(13.0).focused);
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

    #[test]
    fn search_text_is_first_line_without_controls_and_capped() {
        assert_eq!(search_text("error"), "error");
        assert_eq!(search_text("first\nsecond"), "first", "여러 줄이면 첫 줄만");
        assert_eq!(search_text("a\x1b[31mb\tc"), "a[31mbc", "제어 문자는 지운다");
        assert_eq!(search_text(""), "");
        let long = "가".repeat(300);
        assert_eq!(search_text(&long).chars().count(), 256);
    }
}
