//! 설정 파일. Ghostty와 같은 문법(`키 = 값`, `#` 주석, 같은 키 여러 번)을 쓴다.
//!
//! 읽는 순서(뒤가 앞을 덮는다):
//! 1. 기본값(JetBrains Mono 13, 바탕 #282c34, 글자 #ffffff, Tomorrow Night 팔레트)
//! 2. Ghostty 설정 파일 — `import-ghostty = true`일 때만(옮겨 오는 사람을 위한 선택, 기본 끔, ADR-006)
//! 3. b-terminal 설정 파일 `~/.config/b-terminal/config`
//!
//! 모르는 키는 버리고 경고로 모은다(`b-terminal --show-config`가 보여 준다)

use std::path::{Path, PathBuf};

use crate::theme::{Rgb, SearchColor, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorStyle {
    Block,
    Bar,
    Underline,
    BlockHollow,
}

/// `copy-on-select`(Ghostty). macOS에는 선택 클립보드가 없어 `true`와 `clipboard`는 둘 다 시스템 클립보드로 복사한다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyOnSelect {
    False,
    True,
    Clipboard,
}

impl CopyOnSelect {
    /// 고르기를 마칠 때 시스템 클립보드로 복사하는가
    pub fn copies(self) -> bool {
        self != CopyOnSelect::False
    }
}

/// `right-click-action`(Ghostty). `ContextMenu`는 받기만 하고 하는 일이 없다(b-terminal에는 오른쪽 클릭 메뉴가 없다)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightClickAction {
    ContextMenu,
    Paste,
    Copy,
    CopyOrPaste,
    Ignore,
}

/// 새 칸을 어떤 길로 여는가. 폴더 이어받기 키(`*-inherit-working-directory`)를 가른다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnKind {
    Window,
    Tab,
    Split,
}

/// `window-new-tab-position`(Ghostty): 새 탭을 둘 자리
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowNewTabPosition {
    /// 지금 탭 바로 뒤(기본)
    #[default]
    Current,
    /// 탭 목록 맨 끝
    End,
}

impl WindowNewTabPosition {
    /// 탭이 `len`개이고 `active`번째를 보고 있을 때 새 탭을 끼울 자리
    pub fn insert_index(self, active: usize, len: usize) -> usize {
        match self {
            WindowNewTabPosition::End => len,
            WindowNewTabPosition::Current if len == 0 => 0,
            WindowNewTabPosition::Current => (active + 1).min(len),
        }
    }
}

/// `window-show-tab-bar`(Ghostty): 탭 막대를 언제 보이는가
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowShowTabBar {
    /// 탭이 하나여도 보인다
    Always,
    /// 탭이 둘 이상일 때만(기본)
    #[default]
    Auto,
    /// 보이지 않는다
    Never,
}

impl WindowShowTabBar {
    /// 탭이 `tab_count`개일 때 탭 막대를 그리는가
    pub fn visible(self, tab_count: usize) -> bool {
        match self {
            WindowShowTabBar::Always => tab_count > 0,
            WindowShowTabBar::Auto => tab_count > 1,
            WindowShowTabBar::Never => false,
        }
    }
}

/// 색 질의(OSC 4·10·11)에 답하는 꼴(Ghostty `osc-color-report-format`, #151)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OscColorReportFormat {
    /// 답하지 않는다
    None,
    /// `rgb:RR/GG/BB`
    Bit8,
    /// `rgb:RRRR/GGGG/BBBB`(기본)
    #[default]
    Bit16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionAsAlt {
    None,
    Left,
    Right,
    Both,
}

/// 셀 크기 보정: 퍼센트 또는 픽셀
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Adjust {
    Percent(f32),
    Points(f32),
}

impl Adjust {
    fn parse(v: &str) -> Option<Self> {
        let v = v.trim();
        if let Some(p) = v.strip_suffix('%') {
            return p.trim().parse().ok().map(Adjust::Percent);
        }
        v.parse().ok().map(Adjust::Points)
    }

    /// 기준 크기에 보정을 더한 값
    pub fn apply(self, base: f32) -> f32 {
        match self {
            Adjust::Percent(p) => base * (1.0 + p / 100.0),
            Adjust::Points(px) => base + px,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkingDirectory {
    Home,
    Inherit,
    Path(PathBuf),
}

/// `command-palette-entry`(#150) 하나: 팔레트에 더할 사용자 항목. 동작 글은 앱이 키 묶음과 같은 해석기로 푼다
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaletteEntryConf {
    pub title: String,
    /// Ghostty 동작 글(`new_tab`, `csi:0m` 꼴)
    pub action: String,
    pub description: String,
}

/// 읽어 낸 설정
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// 앞이 기본 글꼴, 뒤는 대체 글꼴(그 뒤에 넣어 둔 기본 대체 글꼴이 붙는다)
    pub font_family: Vec<String>,
    pub font_family_bold: Option<String>,
    pub font_family_italic: Option<String>,
    pub font_family_bold_italic: Option<String>,
    /// `font-feature`(#96): 다듬기에 줄 글꼴 기능(태그, 값). `-calt`면 (calt, 0)
    pub font_features: Vec<([u8; 4], u16)>,
    pub font_size: f32,
    pub adjust_cell_height: Option<Adjust>,
    pub adjust_cell_width: Option<Adjust>,
    pub theme: Theme,
    pub cursor_style: CursorStyle,
    pub cursor_blink: Option<bool>,
    /// (왼, 오른) 포인트
    pub padding_x: (f32, f32),
    /// (위, 아래) 포인트
    pub padding_y: (f32, f32),
    /// 처음 창 크기(셀)
    pub window_width: Option<u16>,
    pub window_height: Option<u16>,
    /// 새 창을 최대화로 띄운다(Ghostty `maximize`, #148). 퀵 터미널 창에는 적용하지 않는다
    pub maximize: bool,
    /// 새 창을 전체 화면으로 띄운다(Ghostty `fullscreen`, #148)
    pub fullscreen: bool,
    /// 첫 창 자리(Ghostty `window-position-x`·`-y`, #148). 둘 다 있어야 쓴다
    pub window_position_x: Option<i32>,
    pub window_position_y: Option<i32>,
    pub option_as_alt: OptionAsAlt,
    pub scrollback_lines: usize,
    pub copy_on_select: CopyOnSelect,
    /// 클립보드로 복사하는 글의 줄 끝 공백·탭을 지운다(Ghostty `clipboard-trim-trailing-spaces`)
    pub clipboard_trim_trailing_spaces: bool,
    /// 글을 칠 때 고른 영역을 지운다(Ghostty `selection-clear-on-typing`, 기본 켬, #162)
    pub selection_clear_on_typing: bool,
    /// 복사한 뒤 고른 영역을 지운다(Ghostty `selection-clear-on-copy`, 기본 끔, #162)
    pub selection_clear_on_copy: bool,
    /// 오른쪽 클릭 동작(Ghostty `right-click-action`). 마우스 보고 모드의 프로그램에는 항상 클릭을 보낸다
    pub right_click_action: RightClickAction,
    /// 새 창·탭·분할이 고른 칸의 폴더를 이어받는가(Ghostty, 모두 기본 켬). 켜면 `working-directory`보다 앞선다
    pub window_inherit_working_directory: bool,
    pub tab_inherit_working_directory: bool,
    pub split_inherit_working_directory: bool,
    /// 받기만 한다. 글자 크기가 앱 전체 하나라 새 창은 늘 지금 크기를 쓴다(docs/configuration.md)
    pub window_inherit_font_size: bool,
    pub window_new_tab_position: WindowNewTabPosition,
    pub window_show_tab_bar: WindowShowTabBar,
    pub mouse_hide_while_typing: bool,
    pub command: Option<String>,
    pub working_directory: WorkingDirectory,
    /// 정해 둔 창 제목(셸이 보낸 제목보다 먼저)
    pub title: Option<String>,
    /// `keybind = 트리거=동작`. 앱이 해석한다
    pub keybinds: Vec<String>,
    /// `command-palette-entry`(#150): 사용자 항목. 쓴 차례대로 쌓인다
    pub palette_entries: Vec<PaletteEntryConf>,
    /// `command-palette-entry =`(빈 값)을 썼으면 기본 항목을 팔레트에서 뺀다
    pub palette_defaults_cleared: bool,
    /// 셸이 마우스 모드를 켠 채 끝났을 때 셸로 돌아오면 끈다(#24)
    pub mouse_reset_on_shell: bool,
    /// 셸 통합: detect(기본)·none·zsh·bash
    pub shell_integration: String,
    /// 셸 통합으로 안 명령이 이만큼(초) 넘게 걸려 끝나면, 창을 보고 있지 않을 때 알린다. 0이면 끔
    pub notify_command_seconds: u64,
    /// 명령이 끝났을 때 알릴 때(Ghostty `notify-on-command-finish`, #161). 적지 않았으면 None:
    /// 그때는 옛 키 `notify_command_seconds`를 따른다
    pub notify_on_command_finish: Option<NotifyOnFinish>,
    /// 명령 끝 알림 방법(Ghostty `notify-on-command-finish-action`, 기본 bell)
    pub notify_on_command_finish_action: FinishAction,
    /// 명령 끝 알림이 나가려면 이만큼 넘게 걸려야 한다(Ghostty `notify-on-command-finish-after`, 기본 5초)
    pub notify_on_command_finish_after: std::time::Duration,
    /// 프로그램의 데스크톱 알림(OSC 9·777)을 받는다(Ghostty `desktop-notifications`, 기본 켬)
    pub desktop_notifications: bool,
    /// 앱 자신의 알림 띠(Ghostty `app-notifications`)
    pub app_notifications: AppNotifications,
    /// OSC 52로 프로그램이 클립보드에 쓰는 것을 허용(Ghostty 기본 allow)
    pub clipboard_write: bool,
    /// OSC 52로 프로그램이 클립보드를 읽는 것을 허용(기본 금지. Ghostty 기본은 ask)
    pub clipboard_read: bool,
    /// 위험해 보이는 붙여넣기(괄호 붙여넣기가 꺼진 프로그램에 줄바꿈)를 묻는다(Ghostty `clipboard-paste-protection`, #109)
    pub paste_protection: bool,
    /// 괄호 붙여넣기 모드의 붙여넣기는 안전으로 본다(Ghostty `clipboard-paste-bracketed-safe`)
    pub paste_bracketed_safe: bool,
    /// 분할에서 고르지 않은 칸의 불투명도(0.15~1). 낮을수록 흐리게 덮는다(Ghostty 기본 0.7)
    pub unfocused_split_opacity: f32,
    /// 고르지 않은 칸을 덮는 색. 없으면 바탕색
    pub unfocused_split_fill: Option<Rgb>,
    /// 칸 사이 경계선 색. 없으면 바탕색과 글자색 사이
    pub split_divider_color: Option<Rgb>,
    /// macOS에서 색 맞춤을 하는가(`window-colorspace = srgb`). 기본은 끔(`display-p3`): 화면 색 공간으로 그대로 넘겨
    /// 프레임마다의 변환을 피한다(실험 E3, ADR-009)
    pub colorspace_srgb: bool,
    /// 바탕 불투명도(0~1, Ghostty `background-opacity`). 1보다 작으면 기본 바탕색 칸으로 창 뒤가 비친다(#78)
    pub background_opacity: f32,
    /// 끌 때 창·탭·분할·폴더를 적어 두고 켤 때 되살린다(Ghostty `window-save-state`, #94). never면 끔
    pub window_save_state: bool,
    /// 벨(BEL)이 울릴 때 할 일(Ghostty `bell-features`, #107)
    pub bell_features: BellFeatures,
    /// 벨 소리 파일(Ghostty `bell-audio-path`, #165). 빈 값이면 없음
    pub bell_audio_path: Option<PathBuf>,
    /// 벨 소리 크기 0.0~1.0(Ghostty `bell-audio-volume`, 기본 0.5)
    pub bell_audio_volume: f32,
    /// 프로그램이 도는 칸을 닫기 전에 묻는가(Ghostty `confirm-close-surface`, #108)
    pub confirm_close: ConfirmClose,
    /// 퀵 터미널의 자리·크기·화면·자동 숨김·움직임(Ghostty `quick-terminal-*`, #110)
    pub quick: QuickTerminal,
    /// 배경 그림(Ghostty `background-image`·`-opacity`·`-position`·`-fit`·`-repeat`, #134)
    pub background_image: Option<PathBuf>,
    pub background_image_opacity: f32,
    pub background_image_position: BgPosition,
    pub background_image_fit: BgFit,
    pub background_image_repeat: bool,
    /// 프롬프트를 눌러 커서 옮기기(Ghostty `cursor-click-to-move`, #137)
    pub cursor_click_to_move: bool,
    /// 언제 맨 아래로 스크롤하는가(Ghostty `scroll-to-bottom`, #147). 기본 `keystroke, no-output`
    pub scroll_to_bottom: ScrollToBottom,
    /// 창 제목 보고(CSI 21 t)에 답할지(Ghostty `title-report`, 기본 끔, #151)
    pub title_report: bool,
    /// 색 질의에 답하는 꼴(Ghostty `osc-color-report-format`, #151)
    pub osc_color_report_format: OscColorReportFormat,
    /// 남는 자리를 네 변에 고르게(Ghostty `window-padding-balance`, #136)
    pub padding_balance: bool,
    /// 여백 색(Ghostty `window-padding-color`)
    pub padding_color: PaddingColor,
    /// 커서 불투명도(0~1, Ghostty `cursor-opacity`, #135)
    pub cursor_opacity: f32,
    /// 커서 색·커서 위 글자색을 칸 색에서(Ghostty `cursor-color = cell-foreground` 등). 없으면 테마 색
    pub cursor_color_cell: Option<CellColor>,
    pub cursor_text_cell: Option<CellColor>,
    /// 크기 표시(Ghostty `resize-overlay`·`-position`·`-duration`, #122)
    pub resize_overlay: ResizeOverlay,
    pub resize_overlay_position: OverlayPosition,
    pub resize_overlay_duration: std::time::Duration,
    /// 글자색과 바탕의 최소 명암비(1~21, Ghostty `minimum-contrast`, #123). 1이면 끔
    pub minimum_contrast: f32,
    /// 분할에서 마우스가 올라간 칸을 고른다(Ghostty `focus-follows-mouse`, #126)
    pub focus_follows_mouse: bool,
    /// 마우스 휠 배율: (줄 단위 기기, 픽셀 단위 기기). Ghostty `mouse-scroll-multiplier` 기본 3·1
    pub scroll_multiplier: (f64, f64),
    /// 단어 고르기의 경계 글자(Ghostty `selection-word-chars`)
    pub selection_word_chars: String,
    pub warnings: Vec<String>,
    /// 실제로 읽은 파일
    pub sources: Vec<PathBuf>,
}

/// 명령 끝 알림을 언제 보낼까(Ghostty `notify-on-command-finish`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyOnFinish {
    Never,
    /// 그 칸을 보고 있지 않을 때만
    Unfocused,
    Always,
}

/// 명령 끝 알림 방법(Ghostty `notify-on-command-finish-action`). Ghostty 기본은 bell만
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinishAction {
    /// 벨이 울린 것처럼 한다(`bell-features`를 따름)
    pub bell: bool,
    /// 데스크톱 알림
    pub notify: bool,
}

impl Default for FinishAction {
    fn default() -> Self {
        Self {
            bell: true,
            notify: false,
        }
    }
}

impl FinishAction {
    /// `no-bell,notify` 같은 글을 지금 값 위에 얹는다. 모르는 이름이 있으면 아무것도 바꾸지 않고 거짓
    pub fn apply(&mut self, v: &str) -> bool {
        let mut next = *self;
        for item in v.split(',').map(str::trim).filter(|i| !i.is_empty()) {
            let (on, name) = match item.strip_prefix("no-") {
                Some(n) => (false, n),
                None => (true, item),
            };
            match name {
                "true" | "false" if item == name => {
                    let all = name == "true";
                    next = FinishAction { bell: all, notify: all };
                }
                "bell" => next.bell = on,
                "notify" => next.notify = on,
                _ => return false,
            }
        }
        *self = next;
        true
    }
}

/// 앱 자신의 알림 띠(Ghostty `app-notifications`). 둘 다 기본 켬
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppNotifications {
    /// 글을 클립보드에 복사했을 때
    pub clipboard_copy: bool,
    /// 설정을 다시 읽었을 때
    pub config_reload: bool,
}

impl Default for AppNotifications {
    fn default() -> Self {
        Self {
            clipboard_copy: true,
            config_reload: true,
        }
    }
}

impl AppNotifications {
    /// `no-clipboard-copy`, `false` 같은 글을 지금 값 위에 얹는다. 모르는 이름이 있으면 아무것도 바꾸지 않고 거짓
    pub fn apply(&mut self, v: &str) -> bool {
        let mut next = *self;
        for item in v.split(',').map(str::trim).filter(|i| !i.is_empty()) {
            let (on, name) = match item.strip_prefix("no-") {
                Some(n) => (false, n),
                None => (true, item),
            };
            match name {
                "true" | "false" if item == name => {
                    let all = name == "true";
                    next = AppNotifications {
                        clipboard_copy: all,
                        config_reload: all,
                    };
                }
                "clipboard-copy" => next.clipboard_copy = on,
                "config-reload" => next.config_reload = on,
                _ => return false,
            }
        }
        *self = next;
        true
    }
}

impl BellFeatures {
    /// `system,no-title` 같은 글을 지금 값 위에 얹는다(Ghostty처럼 적지 않은 것은 그대로).
    /// `true`·`false`는 모두 켜고 끈다. 하나라도 모르는 이름이면 아무것도 바꾸지 않고 거짓
    pub fn apply(&mut self, v: &str) -> bool {
        let mut next = *self;
        for item in v.split(',').map(str::trim).filter(|i| !i.is_empty()) {
            let (on, name) = match item.strip_prefix("no-") {
                Some(n) => (false, n),
                None => (true, item),
            };
            match name {
                "true" | "false" if item == name => {
                    let all = name == "true";
                    next = BellFeatures {
                        system: all,
                        attention: all,
                        title: all,
                        audio: all,
                    };
                }
                "system" => next.system = on,
                "attention" => next.attention = on,
                "title" => next.title = on,
                "audio" => next.audio = on,
                // Ghostty에 있지만 b-terminal은 하지 않는다(테두리 깜빡임)
                "border" => {}
                _ => return false,
            }
        }
        *self = next;
        true
    }
}

/// 배경 그림 맞추기(Ghostty `background-image-fit`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgFit {
    /// 잘리지 않게 다 보이도록
    Contain,
    /// 빈 곳 없이 덮도록(넘치는 쪽은 잘림)
    Cover,
    /// 영역에 맞춰 늘림(비율 무시)
    Stretch,
    /// 원래 크기
    None,
}

/// 배경 그림 자리(Ghostty `background-image-position`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgPosition {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// 여백 색(Ghostty `window-padding-color`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaddingColor {
    /// 기본 바탕색
    Background,
    /// 가장 가까운 칸의 바탕색(주 화면에서는 몇 가지 경우 위아래로 늘이지 않음)
    Extend,
    /// 따지지 않고 늘 늘인다
    ExtendAlways,
}

/// 커서 색으로 쓸 칸의 색(Ghostty `cell-foreground`·`cell-background`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellColor {
    Foreground,
    Background,
}

impl CellColor {
    fn parse(v: &str) -> Option<Self> {
        match v {
            "cell-foreground" => Some(CellColor::Foreground),
            "cell-background" => Some(CellColor::Background),
            _ => None,
        }
    }
}

/// 크기 표시를 언제 보이나(Ghostty `resize-overlay`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeOverlay {
    Always,
    Never,
    /// 칸을 만든 직후의 크기 맞추기는 빼고
    AfterFirst,
}

/// 크기 표시 자리(Ghostty `resize-overlay-position`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayPosition {
    Center,
    TopLeft,
    TopCenter,
    TopRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

/// Ghostty 기간 문법: `750ms`, `1s`, `1h30m`. 숫자 뒤에 단위(y d h m s ms us µs ns)
pub fn parse_duration(v: &str) -> Option<std::time::Duration> {
    let s: String = v.chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() {
        return None;
    }
    let mut total: u128 = 0;
    let mut rest = s.as_str();
    while !rest.is_empty() {
        let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        if digits == 0 {
            return None;
        }
        let n: u128 = rest[..digits].parse().ok()?;
        rest = &rest[digits..];
        let unit_len = rest.find(|c: char| c.is_ascii_digit()).unwrap_or(rest.len());
        let (unit, tail) = rest.split_at(unit_len);
        rest = tail;
        let ns: u128 = match unit {
            "y" => 365 * 86_400 * 1_000_000_000,
            "d" => 86_400 * 1_000_000_000,
            "h" => 3_600 * 1_000_000_000,
            "m" => 60 * 1_000_000_000,
            "s" => 1_000_000_000,
            "ms" => 1_000_000,
            "us" | "µs" => 1_000,
            "ns" => 1,
            _ => return None,
        };
        total = total.checked_add(n.checked_mul(ns)?)?;
    }
    Some(std::time::Duration::from_nanos(u64::try_from(total).ok()?))
}

/// Ghostty `selection-word-chars` 기본(공백 포함)
pub const DEFAULT_WORD_CHARS: &str = " \t'\"│`|:;,()[]{}<>$";

/// `3`, `discrete:3`, `precision:1`, `precision:0.5,discrete:3`를 지금 값 위에 얹는다. 범위는 0.01~10000
fn parse_scroll_multiplier(v: &str, cur: (f64, f64)) -> Option<(f64, f64)> {
    let mut out = cur;
    for part in v.split(',').map(str::trim) {
        let (kind, num) = match part.split_once(':') {
            Some((k, n)) => (Some(k.trim()), n.trim()),
            None => (None, part),
        };
        let n: f64 = num.parse().ok().filter(|n: &f64| n.is_finite() && *n > 0.0)?;
        let n = n.clamp(0.01, 10_000.0);
        match kind {
            None => out = (n, n),
            Some("discrete") => out.0 = n,
            Some("precision") => out.1 = n,
            Some(_) => return None,
        }
    }
    Some(out)
}

/// 설정 글의 `\t`·`\\`를 푼다(selection-word-chars)
fn unescape_word_chars(v: &str) -> String {
    let mut out = String::new();
    let mut it = v.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// 퀵 터미널이 붙는 화면 가장자리(Ghostty `quick-terminal-position`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickPosition {
    Top,
    Bottom,
    Left,
    Right,
    Center,
}

/// 퀵 터미널 크기의 한 축: 화면의 몇 %, 또는 몇 점(px)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum QuickDim {
    Percent(f64),
    Px(f64),
}

/// 퀵 터미널이 뜨는 화면(Ghostty `quick-terminal-screen`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickScreen {
    /// 운영체제가 권하는 화면(macOS `NSScreen.mainScreen`: 지금 앞에 있는 창의 화면)
    Main,
    /// 마우스가 있는 화면
    Mouse,
    /// 메뉴 막대가 있는 화면
    MenuBar,
}

/// 퀵 터미널 설정(#110)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuickTerminal {
    pub position: QuickPosition,
    /// 주축(위·아래면 높이, 왼쪽·오른쪽이면 폭, 가운데면 폭)
    pub size: QuickDim,
    /// 다른 축. 없으면 위·아래·왼쪽·오른쪽은 화면 전체, 가운데는 주축과 같은 값
    pub size2: Option<QuickDim>,
    pub screen: QuickScreen,
    /// 포커스를 잃으면 숨는다
    pub autohide: bool,
    /// 들어오고 나가는 움직임(초). 0이면 움직이지 않는다
    pub animation: f64,
}

impl Default for QuickTerminal {
    fn default() -> Self {
        Self {
            position: QuickPosition::Top,
            size: QuickDim::Percent(40.0),
            size2: None,
            screen: QuickScreen::Main,
            autohide: true,
            animation: 0.2,
        }
    }
}

/// `40%`, `300px` 하나. 붙이는 단위가 없으면 Ghostty처럼 잘못된 값이다
fn parse_quick_dim(s: &str) -> Option<QuickDim> {
    if let Some(n) = s.strip_suffix('%') {
        let p: f64 = n.parse().ok()?;
        return (p > 0.0 && p <= 100.0).then_some(QuickDim::Percent(p));
    }
    let px: f64 = s.strip_suffix("px")?.parse().ok()?;
    (px.is_finite() && px > 0.0).then_some(QuickDim::Px(px))
}

/// 닫기 확인(Ghostty `confirm-close-surface`). 기본 `true`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmClose {
    /// 묻지 않는다
    False,
    /// 프로그램이 돌 때만 묻는다(셸이 프롬프트에 있으면 묻지 않음)
    True,
    /// 셸이 살아 있으면 늘 묻는다
    Always,
}

/// 벨이 울릴 때 할 일(Ghostty `bell-features`). Ghostty 기본은 attention·title
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BellFeatures {
    /// 시스템 경고음(macOS NSBeep)
    pub system: bool,
    /// 창이 앞에 없으면 주의를 끈다(Dock 아이콘 튀기기)
    pub attention: bool,
    /// 벨이 울린 칸의 제목 앞에 🔔를 붙이고, 그 칸을 보면 뗀다
    pub title: bool,
    /// `bell-audio-path`의 소리 파일을 재생한다(#165)
    pub audio: bool,
}

impl Default for BellFeatures {
    fn default() -> Self {
        Self {
            system: false,
            attention: true,
            title: true,
            audio: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            font_family: Vec::new(),
            font_family_bold: None,
            font_family_italic: None,
            font_family_bold_italic: None,
            font_features: Vec::new(),
            font_size: 13.0,
            adjust_cell_height: None,
            adjust_cell_width: None,
            theme: Theme::ghostty_default(),
            cursor_style: CursorStyle::Block,
            cursor_blink: None,
            padding_x: (2.0, 2.0),
            padding_y: (2.0, 2.0),
            window_width: None,
            window_height: None,
            maximize: false,
            fullscreen: false,
            window_position_x: None,
            window_position_y: None,
            option_as_alt: OptionAsAlt::None,
            scrollback_lines: 10_000,
            copy_on_select: CopyOnSelect::True,
            clipboard_trim_trailing_spaces: true,
            selection_clear_on_typing: true,
            selection_clear_on_copy: false,
            right_click_action: RightClickAction::ContextMenu,
            window_inherit_working_directory: true,
            tab_inherit_working_directory: true,
            split_inherit_working_directory: true,
            window_inherit_font_size: true,
            window_new_tab_position: WindowNewTabPosition::Current,
            window_show_tab_bar: WindowShowTabBar::Auto,
            mouse_hide_while_typing: false,
            command: None,
            working_directory: WorkingDirectory::Inherit,
            title: None,
            keybinds: Vec::new(),
            palette_entries: Vec::new(),
            palette_defaults_cleared: false,
            mouse_reset_on_shell: true,
            clipboard_write: true,
            clipboard_read: false,
            paste_protection: true,
            paste_bracketed_safe: true,
            shell_integration: "detect".into(),
            notify_command_seconds: 10,
            notify_on_command_finish: None,
            notify_on_command_finish_action: FinishAction::default(),
            notify_on_command_finish_after: std::time::Duration::from_secs(5),
            desktop_notifications: true,
            app_notifications: AppNotifications::default(),
            unfocused_split_opacity: 0.7,
            unfocused_split_fill: None,
            split_divider_color: None,
            colorspace_srgb: false,
            background_opacity: 1.0,
            window_save_state: true,
            bell_features: BellFeatures::default(),
            bell_audio_path: None,
            bell_audio_volume: 0.5,
            confirm_close: ConfirmClose::True,
            quick: QuickTerminal::default(),
            background_image: None,
            background_image_opacity: 1.0,
            background_image_position: BgPosition::Center,
            background_image_fit: BgFit::Contain,
            background_image_repeat: false,
            cursor_click_to_move: true,
            scroll_to_bottom: ScrollToBottom::default(),
            title_report: false,
            osc_color_report_format: OscColorReportFormat::Bit16,
            padding_balance: false,
            padding_color: PaddingColor::Background,
            cursor_opacity: 1.0,
            cursor_color_cell: None,
            cursor_text_cell: None,
            resize_overlay: ResizeOverlay::AfterFirst,
            resize_overlay_position: OverlayPosition::Center,
            resize_overlay_duration: std::time::Duration::from_millis(750),
            minimum_contrast: 1.0,
            focus_follows_mouse: false,
            scroll_multiplier: (3.0, 1.0),
            selection_word_chars: DEFAULT_WORD_CHARS.into(),
            warnings: Vec::new(),
            sources: Vec::new(),
        }
    }
}

/// 설정 파일 한 줄
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub key: String,
    pub value: String,
    pub file: PathBuf,
    pub line: usize,
}

/// 파일을 줄 단위로 읽는다. `config-file = 경로`는 그 자리에서 읽어 넣는다(`?`로 시작하면 없어도 된다)
pub fn read_entries(path: &Path, depth: usize, warnings: &mut Vec<String>, sources: &mut Vec<PathBuf>) -> Vec<Entry> {
    // 서로 부르는 설정 파일이 끝없이 돌지 않게
    if depth > 8 {
        warnings.push(format!("{}: config-file이 너무 깊게 이어집니다", path.display()));
        return Vec::new();
    }
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    sources.push(path.to_path_buf());
    let base = path.parent().unwrap_or(Path::new("."));
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            warnings.push(format!("{}:{}: `키 = 값` 꼴이 아닙니다: {line}", path.display(), i + 1));
            continue;
        };
        let key = k.trim().to_string();
        let value = unquote(v.trim()).to_string();
        if key == "config-file" {
            let (optional, p) = match value.strip_prefix('?') {
                Some(rest) => (true, rest),
                None => (false, value.as_str()),
            };
            let target = expand_home(p, base);
            if !target.exists() {
                if !optional {
                    warnings.push(format!(
                        "{}:{}: config-file {}이 없습니다",
                        path.display(),
                        i + 1,
                        target.display()
                    ));
                }
                continue;
            }
            out.extend(read_entries(&target, depth + 1, warnings, sources));
            continue;
        }
        out.push(Entry {
            key,
            value,
            file: path.to_path_buf(),
            line: i + 1,
        });
    }
    out
}

/// `command-palette-entry` 값(Ghostty 1.2 문법)을 푼다: `title:…,action:…,description:…`.
/// 필드는 쉼표로 나누고 필드 사이 공백은 무시한다. 값에 쉼표가 있으면 큰따옴표로 감싼다
/// (`\"`·`\\`·`\n`·`\t`·`\r` 이스케이프). `title`과 `action`은 꼭 있어야 하고 `description`은 고른 것이다.
pub fn parse_palette_entry(v: &str) -> Result<PaletteEntryConf, String> {
    let mut title = None;
    let mut action = None;
    let mut description = None;
    let mut rest = v.trim_start();
    while !rest.is_empty() {
        let (name, after) = rest
            .split_once(':')
            .ok_or_else(|| format!("`이름:값` 꼴이 아닙니다: {rest}"))?;
        let slot = match name.trim() {
            "title" => &mut title,
            "action" => &mut action,
            "description" => &mut description,
            other => return Err(format!("모르는 필드입니다: {other}")),
        };
        let after = after.trim_start();
        let (value, tail) = if let Some(quoted) = after.strip_prefix('"') {
            let (value, tail) = read_quoted(quoted)?;
            // 닫는 따옴표 뒤에는 쉼표나 끝만 온다
            let tail = tail.trim_start();
            if !tail.is_empty() && !tail.starts_with(',') {
                return Err("닫는 따옴표 뒤에 쉼표가 없습니다".into());
            }
            (value, tail.strip_prefix(',').unwrap_or(tail))
        } else {
            let (raw, tail) = after.split_once(',').unwrap_or((after, ""));
            (raw.trim().to_string(), tail)
        };
        *slot = Some(value);
        rest = tail.trim_start();
    }
    let title = title.filter(|t| !t.is_empty()).ok_or("title이 없습니다")?;
    let action = action.filter(|t| !t.is_empty()).ok_or("action이 없습니다")?;
    Ok(PaletteEntryConf {
        title,
        action,
        description: description.unwrap_or_default(),
    })
}

/// 여는 큰따옴표 뒤의 글에서 닫는 큰따옴표까지 읽는다. (푼 글, 닫는 따옴표 뒤 나머지)
fn read_quoted(s: &str) -> Result<(String, &str), String> {
    let mut out = String::new();
    let mut chars = s.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, &s[i + 1..])),
            '\\' => match chars.next() {
                Some((_, 'n')) => out.push('\n'),
                Some((_, 't')) => out.push('\t'),
                Some((_, 'r')) => out.push('\r'),
                Some((_, c @ ('"' | '\\'))) => out.push(c),
                _ => return Err("알 수 없는 이스케이프입니다".into()),
            },
            c => out.push(c),
        }
    }
    Err("큰따옴표가 닫히지 않았습니다".into())
}

/// 설정 파일에 다시 쓸 글(모든 필드를 따옴표로 감싼다)
fn render_palette_entry(p: &PaletteEntryConf) -> String {
    let q = |s: &str| {
        let mut o = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => o.push_str("\\\""),
                '\\' => o.push_str("\\\\"),
                '\n' => o.push_str("\\n"),
                '\t' => o.push_str("\\t"),
                '\r' => o.push_str("\\r"),
                c => o.push(c),
            }
        }
        o.push('"');
        o
    };
    format!(
        "title:{},action:{},description:{}",
        q(&p.title),
        q(&p.action),
        q(&p.description)
    )
}

fn unquote(v: &str) -> &str {
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        &v[1..v.len() - 1]
    } else {
        v
    }
}

fn expand_home(p: &str, base: &Path) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        return home().join(rest);
    }
    let pb = PathBuf::from(p);
    if pb.is_absolute() { pb } else { base.join(pb) }
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

/// 글꼴 기능 하나(Ghostty 문법): `calt`·`+calt`는 켬, `-calt`는 끔, `calt=2`는 그 값. 태그는 네 글자(ASCII)
pub fn parse_font_feature(s: &str) -> Option<([u8; 4], u16)> {
    let (tag, val) = if let Some(t) = s.strip_prefix('-') {
        (t, 0)
    } else if let Some((t, v)) = s.split_once('=') {
        (t.trim(), v.trim().parse().ok()?)
    } else {
        (s.strip_prefix('+').unwrap_or(s), 1)
    };
    let tag: [u8; 4] = tag.as_bytes().try_into().ok()?;
    tag.iter().all(|b| b.is_ascii_graphic()).then_some((tag, val))
}

/// 맨 아래로 스크롤하는 때(Ghostty `scroll-to-bottom`, #147)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollToBottom {
    /// 키를 쳐서 PTY에 바이트를 보낼 때
    pub keystroke: bool,
    /// 새 출력이 올 때
    pub output: bool,
}

impl Default for ScrollToBottom {
    fn default() -> Self {
        Self {
            keystroke: true,
            output: false,
        }
    }
}

impl ScrollToBottom {
    /// 쉼표로 가른 목록(`keystroke`, `no-output` ...)이나 `true`/`false`. 기본값에서 시작해 차례로 덮는다.
    /// 모르는 이름이 하나라도 있으면 없다
    pub fn parse(v: &str) -> Option<Self> {
        match v.trim() {
            "true" => {
                return Some(Self {
                    keystroke: true,
                    output: true,
                });
            }
            "false" => {
                return Some(Self {
                    keystroke: false,
                    output: false,
                });
            }
            _ => {}
        }
        let mut out = Self::default();
        for tok in v.split(',') {
            let tok = tok.trim();
            let (name, on) = match tok.strip_prefix("no-") {
                Some(n) => (n, false),
                None => (tok, true),
            };
            match name {
                "keystroke" => out.keystroke = on,
                "output" => out.output = on,
                _ => return None,
            }
        }
        Some(out)
    }
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// `2` 또는 `2,4`
fn parse_pair(v: &str) -> Option<(f32, f32)> {
    let mut it = v.split(',').map(|s| s.trim().parse::<f32>());
    let a = it.next()?.ok()?;
    let b = match it.next() {
        Some(b) => b.ok()?,
        None => a,
    };
    Some((a.max(0.0), b.max(0.0)))
}

/// Ghostty 설정 파일 자리(먼저 찾은 것 하나). macOS는 Application Support도 본다
pub fn ghostty_config_paths() -> Vec<PathBuf> {
    let h = home();
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| h.join(".config"));
    let candidates = [
        xdg.join("ghostty/config"),
        xdg.join("ghostty/config.ghostty"),
        h.join("Library/Application Support/com.mitchellh.ghostty/config"),
        h.join("Library/Application Support/com.mitchellh.ghostty/config.ghostty"),
    ];
    candidates.into_iter().filter(|p| p.is_file()).collect()
}

pub fn bterminal_config_path() -> PathBuf {
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"));
    xdg.join("b-terminal/config")
}

include!(concat!(env!("OUT_DIR"), "/themes.rs"));

/// 사용자 테마 폴더. 넣어 둔 테마보다 먼저 본다(같은 이름이면 사용자 것이 이긴다)
pub fn theme_dirs() -> Vec<PathBuf> {
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"));
    vec![xdg.join("b-terminal/themes")]
}

/// 넣어 둔 테마 이름 목록(`b-terminal --list-themes`)
pub fn bundled_theme_names() -> impl Iterator<Item = &'static str> {
    BUNDLED_THEMES.iter().map(|(n, _)| *n)
}

impl Config {
    /// 이 길로 새 칸을 열 때 고른 칸의 폴더를 이어받는가
    pub fn inherits_cwd(&self, kind: SpawnKind) -> bool {
        match kind {
            SpawnKind::Window => self.window_inherit_working_directory,
            SpawnKind::Tab => self.tab_inherit_working_directory,
            SpawnKind::Split => self.split_inherit_working_directory,
        }
    }
}

impl Config {
    /// 기본값 → Ghostty 설정 → b-terminal 설정 → 덧붙인 파일 순서로 읽는다. `dark`는 `theme = light:…,dark:…`에서 고를 쪽
    pub fn load(extra: Option<&Path>, dark: bool) -> Self {
        let mut cfg = Config::default();
        let mut warnings = Vec::new();
        let mut sources = Vec::new();
        let own = read_entries(&bterminal_config_path(), 0, &mut warnings, &mut sources);
        let extra_entries = extra
            .map(|p| read_entries(p, 0, &mut warnings, &mut sources))
            .unwrap_or_default();
        let import = own.iter().chain(&extra_entries).rfind(|e| e.key == "import-ghostty");
        // Ghostty 설정은 옮겨 오는 사람만 켠다(기본 끔, ADR-006)
        let import = import.map(|e| parse_bool(&e.value) == Some(true)).unwrap_or(false);
        let mut entries = Vec::new();
        if import {
            if let Some(g) = ghostty_config_paths().into_iter().next() {
                let mut gs = Vec::new();
                entries.extend(read_entries(&g, 0, &mut warnings, &mut gs));
                // Ghostty 파일을 먼저 읽은 것으로 적는다
                gs.extend(sources);
                sources = gs;
            }
        }
        entries.extend(own);
        entries.extend(extra_entries);
        cfg.apply_all(&entries, dark, &mut warnings);
        cfg.warnings = warnings;
        cfg.sources = sources;
        cfg
    }

    /// 글로 된 설정을 읽는다(테스트와 `--show-config`)
    pub fn from_text(text: &str, dark: bool) -> Self {
        let mut cfg = Config::default();
        let mut warnings = Vec::new();
        let entries: Vec<Entry> = text
            .lines()
            .enumerate()
            .filter_map(|(i, l)| {
                let l = l.trim();
                if l.is_empty() || l.starts_with('#') {
                    return None;
                }
                let (k, v) = l.split_once('=')?;
                Some(Entry {
                    key: k.trim().into(),
                    value: unquote(v.trim()).into(),
                    file: PathBuf::from("<글>"),
                    line: i + 1,
                })
            })
            .collect();
        cfg.apply_all(&entries, dark, &mut warnings);
        cfg.warnings = warnings;
        cfg
    }

    fn apply_all(&mut self, entries: &[Entry], dark: bool, warnings: &mut Vec<String>) {
        // 테마를 먼저 깔고, 그 위에 색 키를 덮는다(Ghostty와 같은 순서). 마지막 theme 값이 이긴다
        if let Some(theme) = entries.iter().rfind(|e| e.key == "theme") {
            if !theme.value.trim().is_empty() {
                let name = pick_theme_name(&theme.value, dark);
                match load_theme(&name) {
                    Some(t) => {
                        let mut tw = Vec::new();
                        for e in &t {
                            self.apply_color(e, &mut tw);
                        }
                    }
                    None => warnings.push(format!(
                        "{}:{}: 테마 '{name}'을 찾지 못했습니다{}",
                        theme.file.display(),
                        theme.line,
                        match similar_themes(&name) {
                            v if v.is_empty() => String::new(),
                            v => format!(" (비슷한 이름: {})", v.join(", ")),
                        }
                    )),
                }
            }
        }
        // font-family는 파일마다 처음부터 다시 쌓는다(Ghostty: 빈 값이면 지움)
        let mut families_reset = false;
        for e in entries {
            if e.key == "font-family" && !families_reset {
                self.font_family.clear();
                families_reset = true;
            }
            self.apply(e, warnings);
        }
    }

    fn apply_color(&mut self, e: &Entry, warnings: &mut Vec<String>) -> bool {
        let v = e.value.trim();
        let color = || Rgb::parse_hex(v).or_else(|| named_color(v));
        let t = &mut self.theme;
        match e.key.as_str() {
            "background" => t.background = color().unwrap_or(t.background),
            "foreground" => t.foreground = color().unwrap_or(t.foreground),
            // cell-foreground·cell-background(Ghostty 1.2): 커서 칸의 글자색·바탕색을 따른다(#135)
            "cursor-color" => match CellColor::parse(v) {
                Some(c) => self.cursor_color_cell = Some(c),
                None => {
                    self.cursor_color_cell = None;
                    t.cursor = color().unwrap_or(t.cursor)
                }
            },
            "cursor-text" => match CellColor::parse(v) {
                Some(c) => self.cursor_text_cell = Some(c),
                None => {
                    self.cursor_text_cell = None;
                    t.cursor_text = color().unwrap_or(t.cursor_text)
                }
            },
            "selection-background" => t.selection_background = color().unwrap_or(t.selection_background),
            "selection-foreground" => t.selection_foreground = color(),
            "palette" => {
                let parsed = v.split_once('=').and_then(|(n, c)| {
                    Some((
                        n.trim().parse::<usize>().ok()?,
                        Rgb::parse_hex(c).or_else(|| named_color(c.trim()))?,
                    ))
                });
                match parsed {
                    Some((n, c)) if n < 16 => t.palette[n] = c,
                    // 16~255번은 xterm 표를 그대로 쓴다(바꾼 값은 아직 받지 않는다)
                    Some(_) => {}
                    None => warnings.push(format!(
                        "{}:{}: palette 값을 읽지 못했습니다: {v}",
                        e.file.display(),
                        e.line
                    )),
                }
            }
            "bold-is-bright" => t.bold_is_bright = parse_bool(v).unwrap_or(t.bold_is_bright),
            // 색이면 굵은 기본 글자색을 그 색으로, `bright`면 굵은 0~7번 색을 밝은 색으로. 빈 값이면 기본(#162)
            "bold-color" => {
                if v.is_empty() {
                    t.bold_color = None;
                    t.bold_is_bright = false;
                } else if v == "bright" {
                    t.bold_color = None;
                    t.bold_is_bright = true;
                } else if let Some(c) = color() {
                    t.bold_color = Some(c);
                } else {
                    warnings.push(format!(
                        "{}:{}: bold-color 값을 읽지 못했습니다: {v}",
                        e.file.display(),
                        e.line
                    ));
                }
            }
            "faint-opacity" => match v.parse::<f32>() {
                Ok(o) if o.is_finite() => t.faint_opacity = o.clamp(0.0, 1.0),
                _ => warnings.push(format!(
                    "{}:{}: faint-opacity 값을 읽지 못했습니다: {v}",
                    e.file.display(),
                    e.line
                )),
            },
            "search-foreground" | "search-background" | "search-selected-foreground" | "search-selected-background" => {
                match parse_search_color(v) {
                    Some(c) => match e.key.as_str() {
                        "search-foreground" => t.search.foreground = c,
                        "search-background" => t.search.background = c,
                        "search-selected-foreground" => t.search.selected_foreground = c,
                        _ => t.search.selected_background = c,
                    },
                    None => warnings.push(format!(
                        "{}:{}: {} 값을 읽지 못했습니다: {v}",
                        e.file.display(),
                        e.line,
                        e.key
                    )),
                }
            }
            _ => return false,
        }
        true
    }

    fn apply(&mut self, e: &Entry, warnings: &mut Vec<String>) {
        if self.apply_color(e, warnings) {
            return;
        }
        let v = e.value.trim();
        let bad = |warnings: &mut Vec<String>| {
            warnings.push(format!(
                "{}:{}: {} 값을 읽지 못했습니다: {v}",
                e.file.display(),
                e.line,
                e.key
            ))
        };
        match e.key.as_str() {
            "theme" | "import-ghostty" => {}
            "font-family" => {
                if v.is_empty() {
                    self.font_family.clear();
                } else {
                    self.font_family.push(v.to_string());
                }
            }
            "font-feature" => {
                if v.is_empty() {
                    self.font_features.clear();
                } else {
                    // 쉼표로 여럿을 한 줄에 써도 된다(Ghostty)
                    for f in v.split(',').map(str::trim).filter(|f| !f.is_empty()) {
                        match parse_font_feature(f) {
                            Some(feat) => {
                                self.font_features.retain(|(t, _)| *t != feat.0);
                                self.font_features.push(feat);
                            }
                            None => bad(warnings),
                        }
                    }
                }
            }
            "font-family-bold" => self.font_family_bold = Some(v.to_string()).filter(|s| !s.is_empty()),
            "font-family-italic" => self.font_family_italic = Some(v.to_string()).filter(|s| !s.is_empty()),
            "font-family-bold-italic" => self.font_family_bold_italic = Some(v.to_string()).filter(|s| !s.is_empty()),
            "font-size" => match v.parse::<f32>() {
                Ok(s) if (4.0..=200.0).contains(&s) => self.font_size = s,
                _ => bad(warnings),
            },
            "adjust-cell-height" => {
                self.adjust_cell_height = if v.is_empty() {
                    None
                } else {
                    Adjust::parse(v).or_else(|| {
                        bad(warnings);
                        None
                    })
                }
            }
            "adjust-cell-width" => {
                self.adjust_cell_width = if v.is_empty() {
                    None
                } else {
                    Adjust::parse(v).or_else(|| {
                        bad(warnings);
                        None
                    })
                }
            }
            "cursor-style" => match v {
                "block" => self.cursor_style = CursorStyle::Block,
                "bar" => self.cursor_style = CursorStyle::Bar,
                "underline" => self.cursor_style = CursorStyle::Underline,
                "block_hollow" => self.cursor_style = CursorStyle::BlockHollow,
                _ => bad(warnings),
            },
            "cursor-style-blink" => self.cursor_blink = if v.is_empty() { None } else { parse_bool(v) },
            "window-padding-x" => match parse_pair(v) {
                Some(p) => self.padding_x = p,
                None => bad(warnings),
            },
            "window-padding-y" => match parse_pair(v) {
                Some(p) => self.padding_y = p,
                None => bad(warnings),
            },
            "window-width" => self.window_width = v.parse::<u16>().ok().filter(|n| *n >= 10),
            "window-height" => self.window_height = v.parse::<u16>().ok().filter(|n| *n >= 4),
            "maximize" => match parse_bool(v) {
                Some(b) => self.maximize = b,
                None => bad(warnings),
            },
            // Ghostty는 non-native 값도 받는다. 여기서는 모두 같은 전체 화면으로 띄운다
            "fullscreen" => match v {
                "true" | "non-native" | "non-native-visible-menu" | "non-native-padded-notch" => self.fullscreen = true,
                "false" => self.fullscreen = false,
                _ => bad(warnings),
            },
            "window-position-x" => match v.parse::<i32>() {
                Ok(n) => self.window_position_x = Some(n),
                Err(_) => bad(warnings),
            },
            "window-position-y" => match v.parse::<i32>() {
                Ok(n) => self.window_position_y = Some(n),
                Err(_) => bad(warnings),
            },
            "macos-option-as-alt" => match v {
                "true" => self.option_as_alt = OptionAsAlt::Both,
                "false" => self.option_as_alt = OptionAsAlt::None,
                "left" => self.option_as_alt = OptionAsAlt::Left,
                "right" => self.option_as_alt = OptionAsAlt::Right,
                _ => bad(warnings),
            },
            // Ghostty는 바이트 수다. 줄 하나를 대략 1KB로 보고 줄 수로 바꾼다
            "scrollback-limit" => match v.parse::<u64>() {
                Ok(bytes) => self.scrollback_lines = ((bytes / 1000) as usize).clamp(100, 200_000),
                Err(_) => bad(warnings),
            },
            // b-terminal 키: 줄 수로 바로
            "scrollback-lines" => match v.parse::<usize>() {
                Ok(n) => self.scrollback_lines = n.clamp(0, 1_000_000),
                Err(_) => bad(warnings),
            },
            // Ghostty 1.4의 값 이름(none·primary·both)도 받는다. primary(선택 클립보드만)는 macOS에서 하는 일이 없다
            "copy-on-select" => match v {
                "true" => self.copy_on_select = CopyOnSelect::True,
                "clipboard" | "both" => self.copy_on_select = CopyOnSelect::Clipboard,
                "false" | "none" => self.copy_on_select = CopyOnSelect::False,
                "primary" if cfg!(target_os = "macos") => self.copy_on_select = CopyOnSelect::False,
                "primary" => self.copy_on_select = CopyOnSelect::True,
                _ => bad(warnings),
            },
            "selection-clear-on-typing" => match parse_bool(v) {
                Some(b) => self.selection_clear_on_typing = b,
                None => bad(warnings),
            },
            "selection-clear-on-copy" => match parse_bool(v) {
                Some(b) => self.selection_clear_on_copy = b,
                None => bad(warnings),
            },
            "clipboard-trim-trailing-spaces" => match parse_bool(v) {
                Some(b) => self.clipboard_trim_trailing_spaces = b,
                None => bad(warnings),
            },
            "right-click-action" => match v {
                "context-menu" => self.right_click_action = RightClickAction::ContextMenu,
                "paste" => self.right_click_action = RightClickAction::Paste,
                "copy" => self.right_click_action = RightClickAction::Copy,
                "copy-or-paste" => self.right_click_action = RightClickAction::CopyOrPaste,
                "ignore" => self.right_click_action = RightClickAction::Ignore,
                _ => bad(warnings),
            },
            "window-inherit-working-directory" => match parse_bool(v) {
                Some(b) => self.window_inherit_working_directory = b,
                None => bad(warnings),
            },
            "tab-inherit-working-directory" => match parse_bool(v) {
                Some(b) => self.tab_inherit_working_directory = b,
                None => bad(warnings),
            },
            "split-inherit-working-directory" => match parse_bool(v) {
                Some(b) => self.split_inherit_working_directory = b,
                None => bad(warnings),
            },
            "window-inherit-font-size" => match parse_bool(v) {
                Some(b) => self.window_inherit_font_size = b,
                None => bad(warnings),
            },
            "window-new-tab-position" => match v {
                "current" => self.window_new_tab_position = WindowNewTabPosition::Current,
                "end" => self.window_new_tab_position = WindowNewTabPosition::End,
                _ => bad(warnings),
            },
            "window-show-tab-bar" => match v {
                "always" => self.window_show_tab_bar = WindowShowTabBar::Always,
                "auto" => self.window_show_tab_bar = WindowShowTabBar::Auto,
                "never" => self.window_show_tab_bar = WindowShowTabBar::Never,
                _ => bad(warnings),
            },
            "mouse-hide-while-typing" => {
                self.mouse_hide_while_typing = parse_bool(v).unwrap_or(self.mouse_hide_while_typing)
            }
            "command" => self.command = Some(v.to_string()).filter(|s| !s.is_empty()),
            "working-directory" => {
                self.working_directory = match v {
                    "" | "inherit" => WorkingDirectory::Inherit,
                    "home" => WorkingDirectory::Home,
                    p => WorkingDirectory::Path(expand_home(p, e.file.parent().unwrap_or(Path::new(".")))),
                }
            }
            "title" => self.title = Some(v.to_string()).filter(|s| !s.is_empty()),
            "keybind" => self.keybinds.push(v.to_string()),
            "command-palette-entry" => {
                if v.is_empty() {
                    self.palette_entries.clear();
                    self.palette_defaults_cleared = true;
                } else {
                    match parse_palette_entry(v) {
                        Ok(entry) => self.palette_entries.push(entry),
                        Err(why) => warnings.push(format!(
                            "{}:{}: command-palette-entry 값을 읽지 못했습니다: {why}",
                            e.file.display(),
                            e.line
                        )),
                    }
                }
            }
            "shell-integration" => match v {
                "detect" | "none" | "zsh" | "bash" => self.shell_integration = v.to_string(),
                // fish·elvish는 아직 스크립트가 없다
                "fish" | "elvish" => {
                    self.shell_integration = "none".into();
                    warnings.push(format!("{}:{}: {v} 셸 통합은 아직 없습니다", e.file.display(), e.line));
                }
                _ => bad(warnings),
            },
            "notify-command-seconds" => match v.parse::<u64>() {
                Ok(n) => self.notify_command_seconds = n,
                Err(_) => bad(warnings),
            },
            "notify-on-command-finish" => match v {
                "never" => self.notify_on_command_finish = Some(NotifyOnFinish::Never),
                "unfocused" => self.notify_on_command_finish = Some(NotifyOnFinish::Unfocused),
                "always" => self.notify_on_command_finish = Some(NotifyOnFinish::Always),
                _ => bad(warnings),
            },
            "notify-on-command-finish-action" => {
                if !self.notify_on_command_finish_action.apply(v) {
                    bad(warnings)
                }
            }
            "notify-on-command-finish-after" => match parse_duration(v) {
                Some(d) => self.notify_on_command_finish_after = d,
                None => bad(warnings),
            },
            "desktop-notifications" => match v {
                "true" => self.desktop_notifications = true,
                "false" => self.desktop_notifications = false,
                _ => bad(warnings),
            },
            "app-notifications" => {
                if !self.app_notifications.apply(v) {
                    bad(warnings)
                }
            }
            "clipboard-write" => match v {
                "allow" | "true" => self.clipboard_write = true,
                "deny" | "false" => self.clipboard_write = false,
                _ => bad(warnings),
            },
            // ask는 아직 물을 창이 없어 금지로 본다
            "clipboard-read" => match v {
                "allow" | "true" => self.clipboard_read = true,
                "deny" | "false" | "ask" => self.clipboard_read = false,
                _ => bad(warnings),
            },
            "clipboard-paste-protection" => match parse_bool(v) {
                Some(b) => self.paste_protection = b,
                None => bad(warnings),
            },
            "clipboard-paste-bracketed-safe" => match parse_bool(v) {
                Some(b) => self.paste_bracketed_safe = b,
                None => bad(warnings),
            },
            "mouse-reset-on-shell" => self.mouse_reset_on_shell = parse_bool(v).unwrap_or(true),
            "window-save-state" => match v {
                "default" | "always" => self.window_save_state = true,
                "never" => self.window_save_state = false,
                _ => bad(warnings),
            },
            // 상대 경로는 그 설정 파일의 폴더 기준(working-directory와 같게)
            "background-image" => {
                self.background_image =
                    (!v.is_empty()).then(|| expand_home(v, e.file.parent().unwrap_or(Path::new("."))));
            }
            "background-image-opacity" => match v.parse::<f32>() {
                Ok(o) if o.is_finite() && o >= 0.0 => self.background_image_opacity = o,
                _ => bad(warnings),
            },
            "background-image-position" => {
                use BgPosition::*;
                match v {
                    "top-left" => self.background_image_position = TopLeft,
                    "top-center" => self.background_image_position = TopCenter,
                    "top-right" => self.background_image_position = TopRight,
                    "center-left" => self.background_image_position = CenterLeft,
                    "center" => self.background_image_position = Center,
                    "center-right" => self.background_image_position = CenterRight,
                    "bottom-left" => self.background_image_position = BottomLeft,
                    "bottom-center" => self.background_image_position = BottomCenter,
                    "bottom-right" => self.background_image_position = BottomRight,
                    _ => bad(warnings),
                }
            }
            "background-image-fit" => match v {
                "contain" => self.background_image_fit = BgFit::Contain,
                "cover" => self.background_image_fit = BgFit::Cover,
                "stretch" => self.background_image_fit = BgFit::Stretch,
                "none" => self.background_image_fit = BgFit::None,
                _ => bad(warnings),
            },
            "background-image-repeat" => match parse_bool(v) {
                Some(b) => self.background_image_repeat = b,
                None => bad(warnings),
            },
            "scroll-to-bottom" => match ScrollToBottom::parse(v) {
                Some(m) => self.scroll_to_bottom = m,
                None => bad(warnings),
            },
            "title-report" => match parse_bool(v) {
                Some(b) => self.title_report = b,
                None => bad(warnings),
            },
            "osc-color-report-format" => match v {
                "none" => self.osc_color_report_format = OscColorReportFormat::None,
                "8-bit" => self.osc_color_report_format = OscColorReportFormat::Bit8,
                "16-bit" => self.osc_color_report_format = OscColorReportFormat::Bit16,
                _ => bad(warnings),
            },
            "cursor-click-to-move" => match parse_bool(v) {
                Some(b) => self.cursor_click_to_move = b,
                None => bad(warnings),
            },
            "window-padding-balance" => match parse_bool(v) {
                Some(b) => self.padding_balance = b,
                None => bad(warnings),
            },
            "window-padding-color" => match v {
                "background" => self.padding_color = PaddingColor::Background,
                "extend" => self.padding_color = PaddingColor::Extend,
                "extend-always" => self.padding_color = PaddingColor::ExtendAlways,
                _ => bad(warnings),
            },
            "cursor-opacity" => match v.parse::<f32>() {
                Ok(o) if o.is_finite() => self.cursor_opacity = o.clamp(0.0, 1.0),
                _ => bad(warnings),
            },
            "resize-overlay" => match v {
                "always" => self.resize_overlay = ResizeOverlay::Always,
                "never" => self.resize_overlay = ResizeOverlay::Never,
                "after-first" => self.resize_overlay = ResizeOverlay::AfterFirst,
                _ => bad(warnings),
            },
            "resize-overlay-position" => match v {
                "center" => self.resize_overlay_position = OverlayPosition::Center,
                "top-left" => self.resize_overlay_position = OverlayPosition::TopLeft,
                "top-center" => self.resize_overlay_position = OverlayPosition::TopCenter,
                "top-right" => self.resize_overlay_position = OverlayPosition::TopRight,
                "bottom-left" => self.resize_overlay_position = OverlayPosition::BottomLeft,
                "bottom-center" => self.resize_overlay_position = OverlayPosition::BottomCenter,
                "bottom-right" => self.resize_overlay_position = OverlayPosition::BottomRight,
                _ => bad(warnings),
            },
            "resize-overlay-duration" => match parse_duration(v) {
                Some(d) => self.resize_overlay_duration = d,
                None => bad(warnings),
            },
            "minimum-contrast" => match v.parse::<f32>() {
                Ok(c) if c.is_finite() => self.minimum_contrast = c.clamp(1.0, 21.0),
                _ => bad(warnings),
            },
            "focus-follows-mouse" => match parse_bool(v) {
                Some(b) => self.focus_follows_mouse = b,
                None => bad(warnings),
            },
            "mouse-scroll-multiplier" => match parse_scroll_multiplier(v, self.scroll_multiplier) {
                Some(m) => self.scroll_multiplier = m,
                None => bad(warnings),
            },
            // 공백도 경계 글자라 다듬지 않은 값을 쓴다(큰따옴표는 이미 벗김)
            "selection-word-chars" => self.selection_word_chars = unescape_word_chars(&e.value),
            "quick-terminal-position" => match v {
                "top" => self.quick.position = QuickPosition::Top,
                "bottom" => self.quick.position = QuickPosition::Bottom,
                "left" => self.quick.position = QuickPosition::Left,
                "right" => self.quick.position = QuickPosition::Right,
                "center" => self.quick.position = QuickPosition::Center,
                _ => bad(warnings),
            },
            "quick-terminal-size" => {
                let mut parts = v.split(',').map(str::trim);
                let first = parts.next().and_then(parse_quick_dim);
                let second = parts.next().map(parse_quick_dim);
                match (first, second, parts.next()) {
                    (Some(a), None, None) => (self.quick.size, self.quick.size2) = (a, None),
                    (Some(a), Some(Some(b)), None) => (self.quick.size, self.quick.size2) = (a, Some(b)),
                    _ => bad(warnings),
                }
            }
            "quick-terminal-screen" => match v {
                "main" => self.quick.screen = QuickScreen::Main,
                "mouse" => self.quick.screen = QuickScreen::Mouse,
                "macos-menu-bar" => self.quick.screen = QuickScreen::MenuBar,
                _ => bad(warnings),
            },
            "quick-terminal-autohide" => match parse_bool(v) {
                Some(b) => self.quick.autohide = b,
                None => bad(warnings),
            },
            "quick-terminal-animation-duration" => match v.parse::<f64>() {
                Ok(d) if d.is_finite() && d >= 0.0 => self.quick.animation = d.min(5.0),
                _ => bad(warnings),
            },
            "confirm-close-surface" => match v {
                "true" => self.confirm_close = ConfirmClose::True,
                "false" => self.confirm_close = ConfirmClose::False,
                "always" => self.confirm_close = ConfirmClose::Always,
                _ => bad(warnings),
            },
            "bell-features" => {
                if !self.bell_features.apply(v) {
                    bad(warnings)
                }
            }
            // 상대 경로는 그 설정 파일의 폴더 기준(background-image와 같게). 빈 값은 지움
            "bell-audio-path" => {
                self.bell_audio_path =
                    (!v.is_empty()).then(|| expand_home(v, e.file.parent().unwrap_or(Path::new("."))));
            }
            // Ghostty처럼 0.0(무음)~1.0(최대). 범위 밖은 자른다
            "bell-audio-volume" => match v.parse::<f32>() {
                Ok(o) if o.is_finite() => self.bell_audio_volume = o.clamp(0.0, 1.0),
                _ => bad(warnings),
            },
            "background-opacity" => match v.parse::<f32>() {
                Ok(o) if o.is_finite() => self.background_opacity = o.clamp(0.0, 1.0),
                _ => bad(warnings),
            },
            "window-colorspace" => match v {
                "srgb" => self.colorspace_srgb = true,
                "display-p3" => self.colorspace_srgb = false,
                _ => bad(warnings),
            },
            "unfocused-split-opacity" => match v.parse::<f32>() {
                // Ghostty와 같이 0.15~1로 묶는다
                Ok(o) if o.is_finite() => self.unfocused_split_opacity = o.clamp(0.15, 1.0),
                _ => bad(warnings),
            },
            "unfocused-split-fill" => {
                self.unfocused_split_fill = if v.is_empty() {
                    None
                } else {
                    Rgb::parse_hex(v).or_else(|| named_color(v)).or_else(|| {
                        bad(warnings);
                        None
                    })
                }
            }
            "split-divider-color" => {
                self.split_divider_color = if v.is_empty() {
                    None
                } else {
                    Rgb::parse_hex(v).or_else(|| named_color(v)).or_else(|| {
                        bad(warnings);
                        None
                    })
                }
            }
            other => {
                // Ghostty에만 있는 키는 조용히 넘기지 않고 알린다
                if !KNOWN_IGNORED.contains(&other) {
                    warnings.push(format!(
                        "{}:{}: b-terminal이 아직 모르는 설정입니다: {other}",
                        e.file.display(),
                        e.line
                    ));
                }
            }
        }
    }

    /// 지금 설정을 Ghostty 문법으로 적는다(`--show-config`)
    pub fn render(&self) -> String {
        let t = &self.theme;
        let mut out = Vec::new();
        let mut push = |k: &str, v: String| out.push(format!("{k} = {v}"));
        if self.font_family.is_empty() {
            push("font-family", "JetBrains Mono  # 넣어 둔 기본 글꼴".into());
        }
        for f in &self.font_family {
            push("font-family", f.clone());
        }
        for (tag, val) in &self.font_features {
            let tag = String::from_utf8_lossy(tag);
            push(
                "font-feature",
                if *val == 0 {
                    format!("-{tag}")
                } else {
                    format!("{tag}={val}")
                },
            );
        }
        push("font-size", format!("{}", self.font_size));
        push("background", hex(t.background));
        push("foreground", hex(t.foreground));
        push("cursor-color", hex(t.cursor));
        push("cursor-text", hex(t.cursor_text));
        push("selection-background", hex(t.selection_background));
        if let Some(c) = t.selection_foreground {
            push("selection-foreground", hex(c));
        }
        for (i, c) in t.palette.iter().enumerate() {
            push("palette", format!("{i}={}", hex(*c)));
        }
        push("bold-is-bright", t.bold_is_bright.to_string());
        if let Some(c) = t.bold_color {
            push("bold-color", hex(c));
        }
        push("faint-opacity", format!("{}", t.faint_opacity));
        let search = |c: SearchColor| match c {
            SearchColor::Rgb(c) => hex(c),
            SearchColor::CellForeground => "cell-foreground".to_string(),
            SearchColor::CellBackground => "cell-background".to_string(),
        };
        push("search-foreground", search(t.search.foreground));
        push("search-background", search(t.search.background));
        push("search-selected-foreground", search(t.search.selected_foreground));
        push("search-selected-background", search(t.search.selected_background));
        push("selection-clear-on-typing", self.selection_clear_on_typing.to_string());
        push("selection-clear-on-copy", self.selection_clear_on_copy.to_string());
        push(
            "cursor-style",
            match self.cursor_style {
                CursorStyle::Block => "block",
                CursorStyle::Bar => "bar",
                CursorStyle::Underline => "underline",
                CursorStyle::BlockHollow => "block_hollow",
            }
            .into(),
        );
        push("window-padding-x", format!("{},{}", self.padding_x.0, self.padding_x.1));
        push("window-padding-y", format!("{},{}", self.padding_y.0, self.padding_y.1));
        push(
            "macos-option-as-alt",
            match self.option_as_alt {
                OptionAsAlt::None => "false",
                OptionAsAlt::Left => "left",
                OptionAsAlt::Right => "right",
                OptionAsAlt::Both => "true",
            }
            .into(),
        );
        push("scrollback-lines", self.scrollback_lines.to_string());
        push(
            "copy-on-select",
            match self.copy_on_select {
                CopyOnSelect::False => "false",
                CopyOnSelect::True => "true",
                CopyOnSelect::Clipboard => "clipboard",
            }
            .into(),
        );
        push(
            "clipboard-trim-trailing-spaces",
            self.clipboard_trim_trailing_spaces.to_string(),
        );
        push(
            "right-click-action",
            match self.right_click_action {
                RightClickAction::ContextMenu => "context-menu",
                RightClickAction::Paste => "paste",
                RightClickAction::Copy => "copy",
                RightClickAction::CopyOrPaste => "copy-or-paste",
                RightClickAction::Ignore => "ignore",
            }
            .into(),
        );
        push(
            "window-inherit-working-directory",
            self.window_inherit_working_directory.to_string(),
        );
        push(
            "tab-inherit-working-directory",
            self.tab_inherit_working_directory.to_string(),
        );
        push(
            "split-inherit-working-directory",
            self.split_inherit_working_directory.to_string(),
        );
        push("window-inherit-font-size", self.window_inherit_font_size.to_string());
        push(
            "window-new-tab-position",
            match self.window_new_tab_position {
                WindowNewTabPosition::Current => "current",
                WindowNewTabPosition::End => "end",
            }
            .into(),
        );
        push(
            "window-show-tab-bar",
            match self.window_show_tab_bar {
                WindowShowTabBar::Always => "always",
                WindowShowTabBar::Auto => "auto",
                WindowShowTabBar::Never => "never",
            }
            .into(),
        );
        push("mouse-hide-while-typing", self.mouse_hide_while_typing.to_string());
        for k in &self.keybinds {
            push("keybind", k.clone());
        }
        if self.palette_defaults_cleared {
            push("command-palette-entry", String::new());
        }
        for p in &self.palette_entries {
            push("command-palette-entry", render_palette_entry(p));
        }
        let b = self.bell_features;
        let bell: Vec<&str> = [
            (b.system, "system"),
            (b.attention, "attention"),
            (b.title, "title"),
            (b.audio, "audio"),
        ]
        .into_iter()
        .map(|(on, name)| if on { name } else { "" })
        .filter(|n| !n.is_empty())
        .collect();
        push("clipboard-paste-protection", self.paste_protection.to_string());
        push("clipboard-paste-bracketed-safe", self.paste_bracketed_safe.to_string());
        push(
            "confirm-close-surface",
            match self.confirm_close {
                ConfirmClose::False => "false",
                ConfirmClose::True => "true",
                ConfirmClose::Always => "always",
            }
            .into(),
        );
        if let Some(m) = self.notify_on_command_finish {
            push(
                "notify-on-command-finish",
                match m {
                    NotifyOnFinish::Never => "never",
                    NotifyOnFinish::Unfocused => "unfocused",
                    NotifyOnFinish::Always => "always",
                }
                .into(),
            );
        }
        let a = self.notify_on_command_finish_action;
        push(
            "notify-on-command-finish-action",
            format!(
                "{}bell,{}notify",
                if a.bell { "" } else { "no-" },
                if a.notify { "" } else { "no-" }
            ),
        );
        push("notify-on-command-finish-after", {
            let ms = self.notify_on_command_finish_after.as_millis();
            if ms % 1000 == 0 {
                format!("{}s", ms / 1000)
            } else {
                format!("{ms}ms")
            }
        });
        push("desktop-notifications", self.desktop_notifications.to_string());
        let n = self.app_notifications;
        push(
            "app-notifications",
            format!(
                "{}clipboard-copy,{}config-reload",
                if n.clipboard_copy { "" } else { "no-" },
                if n.config_reload { "" } else { "no-" }
            ),
        );
        push(
            "bell-features",
            if bell.is_empty() {
                "false".into()
            } else {
                bell.join(",")
            },
        );
        if let Some(p) = &self.bell_audio_path {
            push("bell-audio-path", p.display().to_string());
        }
        push("bell-audio-volume", self.bell_audio_volume.to_string());
        out.join("\n")
    }
}

/// Ghostty에 있지만 b-terminal이 일부러 쓰지 않는 키(경고하지 않는다). 화면 모양·창 장식처럼 지금 그릴 수 없는 것
const KNOWN_IGNORED: &[&str] = &[
    "font-feature",
    "font-thicken",
    "font-thicken-strength",
    "font-synthetic-style",
    "background-blur",
    "background-blur-radius",
    "window-decoration",
    "window-theme",
    "macos-titlebar-style",
    "macos-titlebar-proxy-icon",
    "macos-icon",
    "macos-window-shadow",
    "quit-after-last-window-closed",
    "shell-integration-features",
    "auto-update",
    "auto-update-channel",
    "link-url",
    "gtk-titlebar",
    "gtk-single-instance",
];

fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// X11 색 이름 몇 개(Ghostty 테마가 가끔 쓴다)
/// 찾기 강조 색 값: `#rrggbb`·이름 색·`cell-foreground`·`cell-background`(Ghostty `search-*`)
fn parse_search_color(v: &str) -> Option<SearchColor> {
    match v {
        "cell-foreground" => Some(SearchColor::CellForeground),
        "cell-background" => Some(SearchColor::CellBackground),
        _ => Rgb::parse_hex(v).or_else(|| named_color(v)).map(SearchColor::Rgb),
    }
}

fn named_color(v: &str) -> Option<Rgb> {
    Some(match v.to_ascii_lowercase().as_str() {
        "black" => Rgb::new(0, 0, 0),
        "white" => Rgb::new(255, 255, 255),
        "red" => Rgb::new(255, 0, 0),
        "green" => Rgb::new(0, 255, 0),
        "blue" => Rgb::new(0, 0, 255),
        _ => return None,
    })
}

/// `light:A,dark:B`에서 고르고, 그냥 이름이면 그대로
fn pick_theme_name(v: &str, dark: bool) -> String {
    let mut light = None;
    let mut darkn = None;
    for part in v.split(',') {
        let part = part.trim();
        if let Some(n) = part.strip_prefix("light:") {
            light = Some(n.trim().to_string());
        } else if let Some(n) = part.strip_prefix("dark:") {
            darkn = Some(n.trim().to_string());
        }
    }
    match (light, darkn) {
        (None, None) => v.trim().to_string(),
        (l, d) => if dark { d.or(l) } else { l.or(d) }.unwrap_or_default(),
    }
}

/// 테마 이름이나 경로로 테마 파일을 읽는다
fn load_theme(name: &str) -> Option<Vec<Entry>> {
    let direct = PathBuf::from(name);
    let path = if direct.is_absolute() && direct.is_file() {
        Some(direct)
    } else {
        // 이름에 경로 구분자가 있으면 폴더를 빠져나갈 수 있으므로 받지 않는다
        if name.contains('/') || name.contains("..") {
            return None;
        }
        theme_dirs().into_iter().map(|d| d.join(name)).find(|p| p.is_file())
    };
    if let Some(path) = path {
        let mut w = Vec::new();
        let mut s = Vec::new();
        return Some(read_entries(&path, 0, &mut w, &mut s));
    }
    // 넣어 둔 테마: 이름이 같은 것, 없으면 띄어쓰기·대소문자·-·_를 무시하고(`Tokyo Night` = `TokyoNight`)
    let key = theme_key(name);
    let text = BUNDLED_THEMES
        .iter()
        .find(|(n, _)| *n == name)
        .or_else(|| BUNDLED_THEMES.iter().find(|(n, _)| theme_key(n) == key))
        .map(|(_, t)| *t)?;
    Some(parse_text(text, Path::new(name)))
}

fn theme_key(n: &str) -> String {
    n.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// 못 찾은 테마 이름과 비슷한 넣어 둔 테마(경고에 보인다)
pub fn similar_themes(name: &str) -> Vec<&'static str> {
    let key = theme_key(name);
    if key.len() < 3 {
        return Vec::new();
    }
    let head: String = key.chars().take(5).collect();
    BUNDLED_THEMES
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| theme_key(n).contains(&head))
        .take(5)
        .collect()
}

/// 글로 된 설정을 줄로 나눈다(넣어 둔 테마처럼 파일이 아닌 것)
fn parse_text(text: &str, origin: &Path) -> Vec<Entry> {
    text.lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let l = l.trim();
            if l.is_empty() || l.starts_with('#') {
                return None;
            }
            let (k, v) = l.split_once('=')?;
            Some(Entry {
                key: k.trim().into(),
                value: unquote(v.trim()).into(),
                file: origin.to_path_buf(),
                line: i + 1,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inherit_and_tab_keys() {
        let d = Config::default();
        assert!(d.window_inherit_working_directory && d.tab_inherit_working_directory);
        assert!(d.split_inherit_working_directory && d.window_inherit_font_size);
        assert_eq!(d.window_new_tab_position, WindowNewTabPosition::Current);
        assert_eq!(d.window_show_tab_bar, WindowShowTabBar::Auto);
        let c = Config::from_text(
            "window-inherit-working-directory = false\ntab-inherit-working-directory = false\nsplit-inherit-working-directory = false\nwindow-inherit-font-size = false\nwindow-new-tab-position = end\nwindow-show-tab-bar = always\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert!(!c.window_inherit_working_directory && !c.tab_inherit_working_directory);
        assert!(!c.split_inherit_working_directory && !c.window_inherit_font_size);
        assert_eq!(c.window_new_tab_position, WindowNewTabPosition::End);
        assert_eq!(c.window_show_tab_bar, WindowShowTabBar::Always);
        assert_eq!(
            Config::from_text("window-show-tab-bar = never\n", true).window_show_tab_bar,
            WindowShowTabBar::Never
        );
        assert_eq!(
            Config::from_text("window-show-tab-bar = hidden\n", true).warnings.len(),
            1
        );
        assert_eq!(
            Config::from_text("window-new-tab-position = start\n", true)
                .warnings
                .len(),
            1
        );
        assert_eq!(
            Config::from_text("tab-inherit-working-directory = x\n", true)
                .warnings
                .len(),
            1
        );
    }

    #[test]
    fn inherit_cwd_per_kind() {
        let mut c = Config::default();
        assert!(
            c.inherits_cwd(SpawnKind::Window) && c.inherits_cwd(SpawnKind::Tab) && c.inherits_cwd(SpawnKind::Split)
        );
        c.tab_inherit_working_directory = false;
        assert!(!c.inherits_cwd(SpawnKind::Tab));
        assert!(c.inherits_cwd(SpawnKind::Window) && c.inherits_cwd(SpawnKind::Split));
        c.window_inherit_working_directory = false;
        c.split_inherit_working_directory = false;
        assert!(!c.inherits_cwd(SpawnKind::Window) && !c.inherits_cwd(SpawnKind::Split));
    }

    #[test]
    fn new_tab_position_index() {
        use WindowNewTabPosition::{Current, End};
        assert_eq!(Current.insert_index(0, 3), 1);
        assert_eq!(Current.insert_index(1, 3), 2);
        assert_eq!(Current.insert_index(2, 3), 3);
        assert_eq!(Current.insert_index(0, 0), 0);
        assert_eq!(End.insert_index(0, 3), 3);
        assert_eq!(End.insert_index(2, 3), 3);
        assert_eq!(End.insert_index(0, 0), 0);
    }

    #[test]
    fn tab_bar_visibility() {
        use WindowShowTabBar::{Always, Auto, Never};
        assert!(!Auto.visible(1) && Auto.visible(2));
        assert!(Always.visible(1) && Always.visible(5));
        assert!(!Never.visible(1) && !Never.visible(5));
    }

    #[test]
    fn clipboard_keys() {
        let d = Config::default();
        assert!(d.clipboard_trim_trailing_spaces);
        assert_eq!(d.copy_on_select, CopyOnSelect::True);
        assert_eq!(d.right_click_action, RightClickAction::ContextMenu);
        let c = Config::from_text(
            "clipboard-trim-trailing-spaces = false\ncopy-on-select = clipboard\nright-click-action = copy-or-paste\n",
            true,
        );
        assert!(!c.clipboard_trim_trailing_spaces);
        assert_eq!(c.copy_on_select, CopyOnSelect::Clipboard);
        assert!(c.copy_on_select.copies());
        assert_eq!(c.right_click_action, RightClickAction::CopyOrPaste);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert!(
            !Config::from_text("copy-on-select = false\n", true)
                .copy_on_select
                .copies()
        );
        for (v, want) in [
            ("context-menu", RightClickAction::ContextMenu),
            ("paste", RightClickAction::Paste),
            ("copy", RightClickAction::Copy),
            ("ignore", RightClickAction::Ignore),
        ] {
            let c = Config::from_text(&format!("right-click-action = {v}\n"), true);
            assert_eq!(c.right_click_action, want);
        }
        assert_eq!(Config::from_text("right-click-action = menu\n", true).warnings.len(), 1);
        assert_eq!(Config::from_text("copy-on-select = maybe\n", true).warnings.len(), 1);
        // Ghostty 1.4의 값 이름
        assert_eq!(
            Config::from_text("copy-on-select = none\n", true).copy_on_select,
            CopyOnSelect::False
        );
        assert_eq!(
            Config::from_text("copy-on-select = both\n", true).copy_on_select,
            CopyOnSelect::Clipboard
        );
        assert!(
            Config::from_text("copy-on-select = primary\n", true)
                .warnings
                .is_empty()
        );
        assert_eq!(
            Config::from_text("clipboard-trim-trailing-spaces = x\n", true)
                .warnings
                .len(),
            1
        );
    }

    #[test]
    fn defaults_match_ghostty() {
        let c = Config::default();
        assert_eq!(c.font_size, 13.0);
        assert_eq!(c.theme.background, Rgb::new(0x28, 0x2c, 0x34));
        assert_eq!(c.theme.foreground, Rgb::new(0xff, 0xff, 0xff));
        assert_eq!(c.theme.palette[1], Rgb::new(0xcc, 0x66, 0x66));
        assert!(!c.theme.bold_is_bright);
        assert_eq!(c.option_as_alt, OptionAsAlt::None);
    }

    #[test]
    fn reads_ghostty_syntax() {
        let c = Config::from_text(
            r##"
            # 주석
            font-family = "D2Coding"
            font-family = Apple SD Gothic Neo
            font-size = 15
            background = #101010
            foreground = eeeeee
            palette = 1=#ff0000
            cursor-style = bar
            window-padding-x = 4,8
            window-padding-y = 6
            macos-option-as-alt = left
            adjust-cell-height = 10%
            keybind = super+t=new_tab
            keybind = super+shift+comma=reload_config
            background-opacity = 0.9
            what-is-this = 1
            "##,
            true,
        );
        assert_eq!(c.font_family, vec!["D2Coding", "Apple SD Gothic Neo"]);
        assert_eq!(c.font_size, 15.0);
        assert_eq!(c.theme.background, Rgb::new(0x10, 0x10, 0x10));
        assert_eq!(c.theme.foreground, Rgb::new(0xee, 0xee, 0xee));
        assert_eq!(c.theme.palette[1], Rgb::new(255, 0, 0));
        assert_eq!(c.cursor_style, CursorStyle::Bar);
        assert_eq!(c.padding_x, (4.0, 8.0));
        assert_eq!(c.padding_y, (6.0, 6.0));
        assert_eq!(c.option_as_alt, OptionAsAlt::Left);
        assert_eq!(c.adjust_cell_height, Some(Adjust::Percent(10.0)));
        assert_eq!(c.keybinds.len(), 2);
        assert_eq!(
            c.warnings.len(),
            1,
            "background-opacity는 읽는 키, what-is-this만 경고: {:?}",
            c.warnings
        );
        assert!(c.warnings[0].contains("what-is-this"));
    }

    #[test]
    fn bad_values_warn_and_keep_defaults() {
        let c = Config::from_text("font-size = huge\ncursor-style = triangle\npalette = x\n", true);
        assert_eq!(c.font_size, 13.0);
        assert_eq!(c.cursor_style, CursorStyle::Block);
        assert_eq!(c.warnings.len(), 3);
    }

    #[test]
    fn startup_window_keys() {
        let d = Config::default();
        assert!(!d.maximize && !d.fullscreen);
        assert_eq!((d.window_position_x, d.window_position_y), (None, None));
        let c = Config::from_text(
            "maximize = true\nfullscreen = true\nwindow-position-x = 40\nwindow-position-y = -20\n",
            true,
        );
        assert!(c.maximize && c.fullscreen);
        assert_eq!((c.window_position_x, c.window_position_y), (Some(40), Some(-20)));
        assert!(c.warnings.is_empty());
        assert!(Config::from_text("fullscreen = non-native\n", true).fullscreen);
        assert_eq!(Config::from_text("maximize = yes\n", true).warnings.len(), 1);
        assert_eq!(Config::from_text("window-position-x = left\n", true).warnings.len(), 1);
    }

    #[test]
    fn window_save_state_key() {
        assert!(
            Config::default().window_save_state,
            "기본은 되살림(Ghostty macOS와 같음)"
        );
        assert!(!Config::from_text("window-save-state = never\n", true).window_save_state);
        assert!(Config::from_text("window-save-state = always\n", true).window_save_state);
        assert_eq!(Config::from_text("window-save-state = maybe\n", true).warnings.len(), 1);
    }

    #[test]
    fn background_image_keys_like_ghostty() {
        let d = Config::default();
        assert_eq!(d.background_image, None);
        assert_eq!(
            (
                d.background_image_opacity,
                d.background_image_position,
                d.background_image_fit,
                d.background_image_repeat
            ),
            (1.0, BgPosition::Center, BgFit::Contain, false)
        );
        let c = Config::from_text(
            "background-image = /tmp/a.png\nbackground-image-opacity = 1.5\nbackground-image-position = bottom-right\nbackground-image-fit = cover\nbackground-image-repeat = true\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.background_image, Some(PathBuf::from("/tmp/a.png")));
        assert_eq!(
            (
                c.background_image_opacity,
                c.background_image_position,
                c.background_image_fit,
                c.background_image_repeat
            ),
            (1.5, BgPosition::BottomRight, BgFit::Cover, true)
        );
        assert_eq!(
            Config::from_text("background-image = \n", true).background_image,
            None,
            "빈 값은 지움"
        );
        assert!(
            Config::from_text("background-image = ~/x.jpg\n", true)
                .background_image
                .is_some_and(|p| !p.starts_with("~")),
            "~를 집 폴더로"
        );
        assert_eq!(
            Config::from_text("background-image-fit = tile\n", true).warnings.len(),
            1
        );
        assert_eq!(
            Config::from_text("background-image-position = middle\n", true)
                .warnings
                .len(),
            1
        );
    }

    #[test]
    fn scroll_to_bottom_key() {
        let d = Config::default().scroll_to_bottom;
        assert_eq!(
            (d.keystroke, d.output),
            (true, false),
            "Ghostty 기본은 keystroke, no-output"
        );
        let t = |s: &str| Config::from_text(&format!("scroll-to-bottom = {s}\n"), true).scroll_to_bottom;
        assert_eq!(
            t("keystroke, output"),
            ScrollToBottom {
                keystroke: true,
                output: true
            }
        );
        assert_eq!(
            t("no-keystroke"),
            ScrollToBottom {
                keystroke: false,
                output: false
            }
        );
        assert_eq!(
            t("output"),
            ScrollToBottom {
                keystroke: true,
                output: true
            },
            "적지 않은 쪽은 기본"
        );
        assert_eq!(
            t("no-keystroke,output"),
            ScrollToBottom {
                keystroke: false,
                output: true
            }
        );
        assert_eq!(
            t("true"),
            ScrollToBottom {
                keystroke: true,
                output: true
            }
        );
        assert_eq!(
            t("false"),
            ScrollToBottom {
                keystroke: false,
                output: false
            }
        );
        for bad in ["yes", "keystrokes", "no-", ""] {
            let c = Config::from_text(&format!("scroll-to-bottom = {bad}\n"), true);
            assert_eq!((c.warnings.len(), c.scroll_to_bottom), (1, d), "{bad}");
        }
    }

    #[test]
    fn report_keys_like_ghostty() {
        let d = Config::default();
        assert!(!d.title_report);
        assert_eq!(d.osc_color_report_format, OscColorReportFormat::Bit16);
        let c = Config::from_text("title-report = true\nosc-color-report-format = 8-bit\n", true);
        assert!(c.title_report);
        assert_eq!(c.osc_color_report_format, OscColorReportFormat::Bit8);
        let c = Config::from_text("osc-color-report-format = none\n", true);
        assert_eq!(c.osc_color_report_format, OscColorReportFormat::None);
        assert_eq!(
            Config::from_text("osc-color-report-format = 32-bit\n", true)
                .warnings
                .len(),
            1
        );
        assert_eq!(Config::from_text("title-report = maybe\n", true).warnings.len(), 1);
    }

    #[test]
    fn cursor_click_to_move_key() {
        assert!(Config::default().cursor_click_to_move);
        assert!(!Config::from_text("cursor-click-to-move = false\n", true).cursor_click_to_move);
        assert_eq!(
            Config::from_text("cursor-click-to-move = yes\n", true).warnings.len(),
            1
        );
    }

    #[test]
    fn padding_keys_like_ghostty() {
        let d = Config::default();
        assert_eq!((d.padding_balance, d.padding_color), (false, PaddingColor::Background));
        let c = Config::from_text(
            "window-padding-balance = true\nwindow-padding-color = extend-always\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!((c.padding_balance, c.padding_color), (true, PaddingColor::ExtendAlways));
        assert_eq!(
            Config::from_text("window-padding-color = red\n", true).warnings.len(),
            1
        );
    }

    #[test]
    fn selection_and_search_color_keys_like_ghostty() {
        use crate::theme::SearchColor::{CellBackground, CellForeground, Rgb as C};
        let d = Config::default();
        assert!(d.selection_clear_on_typing);
        assert!(!d.selection_clear_on_copy);
        assert_eq!(d.theme.faint_opacity, 0.5);
        assert_eq!((d.theme.bold_color, d.theme.bold_is_bright), (None, false));
        let s = d.theme.search;
        assert_eq!(s.foreground, C(Rgb::new(0, 0, 0)));
        assert_eq!(s.background, C(Rgb::new(0xff, 0xe0, 0x82)));
        assert_eq!(s.selected_foreground, C(Rgb::new(0, 0, 0)));
        assert_eq!(s.selected_background, C(Rgb::new(0xf2, 0xa5, 0x7e)));

        let c = Config::from_text(
            "selection-clear-on-typing = false\nselection-clear-on-copy = true\n\
             search-foreground = cell-foreground\nsearch-background = #102030\n\
             search-selected-foreground = #ffffff\nsearch-selected-background = cell-background\n\
             faint-opacity = 0.25\nbold-color = #ff8000\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert!(!c.selection_clear_on_typing);
        assert!(c.selection_clear_on_copy);
        let s = c.theme.search;
        assert_eq!(s.foreground, CellForeground);
        assert_eq!(s.background, C(Rgb::new(0x10, 0x20, 0x30)));
        assert_eq!(s.selected_foreground, C(Rgb::new(255, 255, 255)));
        assert_eq!(s.selected_background, CellBackground);
        assert_eq!(c.theme.faint_opacity, 0.25);
        assert_eq!(c.theme.bold_color, Some(Rgb::new(0xff, 0x80, 0)));

        // bold-color = bright: 굵은 0~7번을 밝은 색으로. 색을 따로 정하지 않는다
        let c = Config::from_text("bold-color = #ff8000\nbold-color = bright\n", true);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!((c.theme.bold_color, c.theme.bold_is_bright), (None, true));
        // 값을 비우면 기본으로
        let c = Config::from_text("bold-color = bright\nbold-color =\n", true);
        assert_eq!((c.theme.bold_color, c.theme.bold_is_bright), (None, false));
        // faint-opacity는 0~1로 묶는다(Ghostty: 범위 밖은 가장 가까운 값)
        assert_eq!(Config::from_text("faint-opacity = 7\n", true).theme.faint_opacity, 1.0);
        assert_eq!(Config::from_text("faint-opacity = -1\n", true).theme.faint_opacity, 0.0);
        // 읽을 수 없는 값은 경고하고 그대로
        for bad in [
            "faint-opacity = dim",
            "search-background = nope",
            "bold-color = nope",
            "selection-clear-on-copy = x",
        ] {
            let c = Config::from_text(&format!("{bad}\n"), true);
            assert_eq!(c.warnings.len(), 1, "{bad}: {:?}", c.warnings);
        }
        let c = Config::from_text("search-background = nope\n", true);
        assert_eq!(c.theme.search, Config::default().theme.search);
    }

    #[test]
    fn cursor_keys_like_ghostty() {
        let d = Config::default();
        assert_eq!(
            (d.cursor_opacity, d.cursor_color_cell, d.cursor_text_cell),
            (1.0, None, None)
        );
        let c = Config::from_text(
            "cursor-opacity = 0.4\ncursor-color = cell-foreground\ncursor-text = cell-background\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.cursor_opacity, 0.4);
        assert_eq!(c.cursor_color_cell, Some(CellColor::Foreground));
        assert_eq!(c.cursor_text_cell, Some(CellColor::Background));
        // 나중에 색을 주면 칸 색 따르기를 끈다
        let c = Config::from_text("cursor-color = cell-foreground\ncursor-color = #ff0000\n", true);
        assert_eq!((c.cursor_color_cell, c.theme.cursor), (None, Rgb::new(255, 0, 0)));
        assert_eq!(Config::from_text("cursor-opacity = 7\n", true).cursor_opacity, 1.0);
        assert_eq!(Config::from_text("cursor-opacity = half\n", true).warnings.len(), 1);
    }

    #[test]
    fn resize_overlay_keys_and_durations_like_ghostty() {
        use std::time::Duration;
        let d = Config::default();
        assert_eq!(d.resize_overlay, ResizeOverlay::AfterFirst);
        assert_eq!(d.resize_overlay_position, OverlayPosition::Center);
        assert_eq!(d.resize_overlay_duration, Duration::from_millis(750));
        assert_eq!(parse_duration("750ms"), Some(Duration::from_millis(750)));
        assert_eq!(parse_duration("1h30m"), Some(Duration::from_secs(5400)));
        assert_eq!(
            parse_duration("2s 500ms"),
            Some(Duration::from_millis(2500)),
            "사이 공백"
        );
        assert_eq!(parse_duration("10us"), Some(Duration::from_micros(10)));
        assert_eq!(parse_duration("10µs"), Some(Duration::from_micros(10)));
        assert_eq!(parse_duration("1d"), Some(Duration::from_secs(86_400)));
        for bad in ["", "5", "ms", "1x", "1.5s", "-1s"] {
            assert_eq!(parse_duration(bad), None, "{bad}");
        }
        let c = Config::from_text(
            "resize-overlay = always\nresize-overlay-position = bottom-right\nresize-overlay-duration = 2s\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(
            (c.resize_overlay, c.resize_overlay_position, c.resize_overlay_duration),
            (
                ResizeOverlay::Always,
                OverlayPosition::BottomRight,
                Duration::from_secs(2)
            )
        );
        assert_eq!(
            Config::from_text("resize-overlay-position = middle\n", true)
                .warnings
                .len(),
            1
        );
    }

    #[test]
    fn minimum_contrast_key() {
        assert_eq!(Config::default().minimum_contrast, 1.0, "기본은 끔");
        assert_eq!(Config::from_text("minimum-contrast = 3\n", true).minimum_contrast, 3.0);
        assert_eq!(
            Config::from_text("minimum-contrast = 50\n", true).minimum_contrast,
            21.0
        );
        assert_eq!(
            Config::from_text("minimum-contrast = 0.2\n", true).minimum_contrast,
            1.0
        );
        let c = Config::from_text("minimum-contrast = high\n", true);
        assert_eq!((c.minimum_contrast, c.warnings.len()), (1.0, 1));
    }

    #[test]
    fn mouse_and_selection_keys_like_ghostty() {
        let d = Config::default();
        assert!(!d.focus_follows_mouse);
        assert_eq!(d.scroll_multiplier, (3.0, 1.0));
        assert!(d.selection_word_chars.starts_with(" \t'\""));
        let c = Config::from_text("focus-follows-mouse = true\nmouse-scroll-multiplier = 2\n", true);
        assert!(c.focus_follows_mouse);
        assert_eq!(c.scroll_multiplier, (2.0, 2.0), "접두 없는 값은 둘 다");
        let c = Config::from_text("mouse-scroll-multiplier = precision:0.5\n", true);
        assert_eq!(c.scroll_multiplier, (3.0, 0.5), "적지 않은 쪽은 그대로");
        let c = Config::from_text("mouse-scroll-multiplier = discrete:5, precision:2\n", true);
        assert_eq!(c.scroll_multiplier, (5.0, 2.0));
        let c = Config::from_text("mouse-scroll-multiplier = 99999\n", true);
        assert_eq!(c.scroll_multiplier, (10000.0, 10000.0), "상한으로 자름");
        for bad in ["abc", "fast:3", "precision:", "-1"] {
            let c = Config::from_text(&format!("mouse-scroll-multiplier = {bad}\n"), true);
            assert_eq!((c.warnings.len(), c.scroll_multiplier), (1, (3.0, 1.0)), "{bad}");
        }
        let c = Config::from_text("selection-word-chars = \" \\t:\\\\\"\n", true);
        assert_eq!(c.selection_word_chars, " \t:\\", "따옴표를 벗기고 \\t·\\\\를 푼다");
    }

    #[test]
    fn quick_terminal_keys_like_ghostty() {
        let d = Config::default().quick;
        assert_eq!(d.position, QuickPosition::Top);
        assert_eq!((d.screen, d.autohide, d.animation), (QuickScreen::Main, true, 0.2));
        let c = Config::from_text(
            "quick-terminal-position = right\nquick-terminal-size = 30%,500px\nquick-terminal-screen = mouse\nquick-terminal-autohide = false\nquick-terminal-animation-duration = 0\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        let q = c.quick;
        assert_eq!(q.position, QuickPosition::Right);
        assert_eq!((q.size, q.size2), (QuickDim::Percent(30.0), Some(QuickDim::Px(500.0))));
        assert_eq!((q.screen, q.autohide, q.animation), (QuickScreen::Mouse, false, 0.0));
        // 단위 없는 값, 셋 이상, 음수, 0%는 Ghostty처럼 잘못된 값
        for bad in ["40", "10%,20%,30%", "-5%", "0%", "abc%", "120%", "300 px"] {
            let c = Config::from_text(&format!("quick-terminal-size = {bad}\n"), true);
            assert_eq!(c.warnings.len(), 1, "{bad}");
            assert_eq!(c.quick.size, QuickDim::Percent(40.0), "{bad}: 그대로");
        }
        assert_eq!(
            Config::from_text("quick-terminal-size = 25%\n", true).quick.size,
            QuickDim::Percent(25.0)
        );
        assert_eq!(
            Config::from_text("quick-terminal-screen = left\n", true).warnings.len(),
            1
        );
    }

    #[test]
    fn paste_protection_keys() {
        let d = Config::default();
        assert!(
            d.paste_protection && d.paste_bracketed_safe,
            "Ghostty 기본은 둘 다 true"
        );
        let c = Config::from_text(
            "clipboard-paste-protection = false\nclipboard-paste-bracketed-safe = false\n",
            true,
        );
        assert!(!c.paste_protection && !c.paste_bracketed_safe);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(
            Config::from_text("clipboard-paste-protection = ask\n", true)
                .warnings
                .len(),
            1
        );
    }

    #[test]
    fn confirm_close_surface_key() {
        assert_eq!(Config::default().confirm_close, ConfirmClose::True, "Ghostty 기본");
        assert_eq!(
            Config::from_text("confirm-close-surface = always\n", true).confirm_close,
            ConfirmClose::Always
        );
        let c = Config::from_text("confirm-close-surface = false\n", true);
        assert_eq!(c.confirm_close, ConfirmClose::False);
        assert!(c.warnings.is_empty(), "알려진 키로 받는다: {:?}", c.warnings);
        assert!(c.render().contains("confirm-close-surface = false"));
        let c = Config::from_text("confirm-close-surface = maybe\n", true);
        assert_eq!((c.confirm_close, c.warnings.len()), (ConfirmClose::True, 1));
    }

    #[test]
    fn command_finish_keys_like_ghostty() {
        let d = Config::default();
        assert_eq!(d.notify_on_command_finish, None, "적지 않았으면 옛 키를 따른다");
        assert_eq!(
            d.notify_on_command_finish_action,
            FinishAction {
                bell: true,
                notify: false
            }
        );
        assert_eq!(d.notify_on_command_finish_after, std::time::Duration::from_secs(5));
        assert!(d.desktop_notifications);
        assert_eq!(
            d.app_notifications,
            AppNotifications {
                clipboard_copy: true,
                config_reload: true
            }
        );
        for (v, want) in [
            ("never", NotifyOnFinish::Never),
            ("unfocused", NotifyOnFinish::Unfocused),
            ("always", NotifyOnFinish::Always),
        ] {
            let c = Config::from_text(&format!("notify-on-command-finish = {v}\n"), true);
            assert_eq!(c.notify_on_command_finish, Some(want), "{v}");
            assert!(c.warnings.is_empty(), "{:?}", c.warnings);
            assert!(c.render().contains(&format!("notify-on-command-finish = {v}")));
        }
        let c = Config::from_text("notify-on-command-finish = sometimes\n", true);
        assert_eq!((c.notify_on_command_finish, c.warnings.len()), (None, 1));

        let c = Config::from_text("notify-on-command-finish-action = no-bell,notify\n", true);
        assert_eq!(
            c.notify_on_command_finish_action,
            FinishAction {
                bell: false,
                notify: true
            }
        );
        let c = Config::from_text("notify-on-command-finish-action = notify\n", true);
        assert_eq!(
            c.notify_on_command_finish_action,
            FinishAction {
                bell: true,
                notify: true
            },
            "적지 않은 것은 그대로"
        );
        let c = Config::from_text("notify-on-command-finish-action = false\n", true);
        assert_eq!(
            c.notify_on_command_finish_action,
            FinishAction {
                bell: false,
                notify: false
            }
        );
        let c = Config::from_text("notify-on-command-finish-action = bell,chime\n", true);
        assert_eq!(c.notify_on_command_finish_action, FinishAction::default());
        assert_eq!(c.warnings.len(), 1);

        let c = Config::from_text("notify-on-command-finish-after = 1m30s\n", true);
        assert_eq!(c.notify_on_command_finish_after, std::time::Duration::from_secs(90));
        assert!(c.render().contains("notify-on-command-finish-after = 90s"));
        let c = Config::from_text("notify-on-command-finish-after = soon\n", true);
        assert_eq!(c.notify_on_command_finish_after, std::time::Duration::from_secs(5));
        assert_eq!(c.warnings.len(), 1);

        let c = Config::from_text("desktop-notifications = false\n", true);
        assert!(!c.desktop_notifications);
        assert!(c.render().contains("desktop-notifications = false"));
        let c = Config::from_text("desktop-notifications = maybe\n", true);
        assert!(c.desktop_notifications);
        assert_eq!(c.warnings.len(), 1);

        let c = Config::from_text("app-notifications = no-clipboard-copy\n", true);
        assert_eq!(
            c.app_notifications,
            AppNotifications {
                clipboard_copy: false,
                config_reload: true
            }
        );
        let c = Config::from_text("app-notifications = false\napp-notifications = config-reload\n", true);
        assert_eq!(
            c.app_notifications,
            AppNotifications {
                clipboard_copy: false,
                config_reload: true
            }
        );
        let c = Config::from_text("app-notifications = false\n", true);
        assert_eq!(
            c.app_notifications,
            AppNotifications {
                clipboard_copy: false,
                config_reload: false
            }
        );
        let c = Config::from_text("app-notifications = true\n", true);
        assert_eq!(c.app_notifications, AppNotifications::default());
        let c = Config::from_text("app-notifications = no-clipboard-copy,toast\n", true);
        assert_eq!(c.app_notifications, AppNotifications::default());
        assert_eq!(c.warnings.len(), 1);
        // 옛 키는 그대로 받는다
        let c = Config::from_text("notify-command-seconds = 30\n", true);
        assert_eq!(c.notify_command_seconds, 30);
    }

    #[test]
    fn bell_features_like_ghostty() {
        let d = Config::default().bell_features;
        assert!(d.attention && d.title && !d.system, "Ghostty 기본은 attention·title");
        let c = Config::from_text("bell-features = system,no-title\n", true);
        assert_eq!(
            c.bell_features,
            BellFeatures {
                system: true,
                attention: true,
                title: false,
                audio: false
            },
            "적지 않은 것은 그대로"
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        let c = Config::from_text("bell-features = false\nbell-features = title\n", true);
        assert_eq!(
            c.bell_features,
            BellFeatures {
                system: false,
                attention: false,
                title: true,
                audio: false
            }
        );
        // audio는 소리 파일 재생(#165), border는 받지만 하는 일이 없다
        let c = Config::from_text("bell-features = audio, no-border\n", true);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(
            c.bell_features,
            BellFeatures {
                audio: true,
                ..BellFeatures::default()
            }
        );
        assert!(
            c.render().contains("bell-features = attention,title,audio"),
            "{}",
            c.render()
        );
        // true는 audio까지 모두 켠다, false는 모두 끈다
        let all = Config::from_text("bell-features = true\n", true).bell_features;
        assert!(all.system && all.audio && all.attention && all.title);
        let c = Config::from_text("bell-features = audio\nbell-features = no-audio\n", true);
        assert!(!c.bell_features.audio);
        // 모르는 이름이 섞이면 줄 전체를 버리고 경고
        let c = Config::from_text("bell-features = no-attention,chime\n", true);
        assert_eq!(c.bell_features, BellFeatures::default());
        assert_eq!(c.warnings.len(), 1);
        assert!(
            Config::from_text("bell-features = system\n", true)
                .render()
                .contains("bell-features = system,attention,title")
        );
    }

    #[test]
    fn bell_audio_keys_like_ghostty() {
        let d = Config::default();
        assert_eq!((d.bell_audio_path.clone(), d.bell_audio_volume), (None, 0.5));
        let c = Config::from_text("bell-audio-path = /tmp/b.wav\nbell-audio-volume = 0.25\n", true);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.bell_audio_path, Some(PathBuf::from("/tmp/b.wav")));
        assert_eq!(c.bell_audio_volume, 0.25);
        // ~/는 홈, 상대 경로는 설정 파일 폴더 기준(background-image와 같음)
        let p = Config::from_text("bell-audio-path = ~/b.wav\n", true)
            .bell_audio_path
            .unwrap();
        assert!(p.is_absolute() || std::env::var_os("HOME").is_none(), "{p:?}");
        assert!(p.ends_with("b.wav") && !p.to_string_lossy().contains('~'), "{p:?}");
        let rel = Config::from_text("bell-audio-path = sounds/b.wav\n", true)
            .bell_audio_path
            .unwrap();
        assert!(rel.ends_with("sounds/b.wav"), "{rel:?}");
        // 빈 값은 지움
        let c = Config::from_text("bell-audio-path = /tmp/b.wav\nbell-audio-path =\n", true);
        assert_eq!(c.bell_audio_path, None);
        // 볼륨은 0~1로 자른다. 숫자가 아니면 경고하고 그대로
        assert_eq!(
            Config::from_text("bell-audio-volume = 3\n", true).bell_audio_volume,
            1.0
        );
        assert_eq!(
            Config::from_text("bell-audio-volume = -1\n", true).bell_audio_volume,
            0.0
        );
        for bad in ["loud", "nan", "inf"] {
            let c = Config::from_text(&format!("bell-audio-volume = {bad}\n"), true);
            assert_eq!(c.warnings.len(), 1, "{bad}");
            assert_eq!(c.bell_audio_volume, 0.5, "{bad}");
        }
        let r = Config::from_text("bell-audio-path = /tmp/b.wav\nbell-audio-volume = 0.25\n", true).render();
        assert!(
            r.contains("bell-audio-path = /tmp/b.wav") && r.contains("bell-audio-volume = 0.25"),
            "{r}"
        );
    }

    #[test]
    fn background_opacity_key() {
        assert_eq!(Config::default().background_opacity, 1.0);
        assert_eq!(
            Config::from_text("background-opacity = 0.85\n", true).background_opacity,
            0.85
        );
        assert_eq!(
            Config::from_text("background-opacity = 3\n", true).background_opacity,
            1.0,
            "1로 묶는다"
        );
        assert_eq!(Config::from_text("background-opacity = x\n", true).warnings.len(), 1);
    }

    #[test]
    fn window_colorspace_key() {
        assert!(!Config::default().colorspace_srgb, "기본은 화면 색 공간(빠름)");
        assert!(Config::from_text("window-colorspace = srgb\n", true).colorspace_srgb);
        let c = Config::from_text("window-colorspace = rainbow\n", true);
        assert_eq!(c.warnings.len(), 1);
    }

    #[test]
    fn split_appearance_keys() {
        let c = Config::default();
        assert_eq!(c.unfocused_split_opacity, 0.7, "Ghostty 기본");
        let c = Config::from_text(
            "unfocused-split-opacity = 0.05\nunfocused-split-fill = #102030\nsplit-divider-color = white\n",
            true,
        );
        assert_eq!(c.unfocused_split_opacity, 0.15, "0.15 아래는 0.15로");
        assert_eq!(c.unfocused_split_fill, Some(Rgb::new(0x10, 0x20, 0x30)));
        assert_eq!(c.split_divider_color, Some(Rgb::new(255, 255, 255)));
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        let c = Config::from_text("unfocused-split-opacity = dim\nsplit-divider-color = nope\n", true);
        assert_eq!(c.warnings.len(), 2);
    }

    #[test]
    fn theme_light_dark_pick() {
        assert_eq!(
            pick_theme_name("light:GitHub Light Default,dark:GitHub Dark", true),
            "GitHub Dark"
        );
        assert_eq!(
            pick_theme_name("light:GitHub Light Default,dark:GitHub Dark", false),
            "GitHub Light Default"
        );
        assert_eq!(pick_theme_name("Dracula", true), "Dracula");
        assert!(
            load_theme("../../etc/passwd").is_none(),
            "경로를 빠져나가는 이름은 받지 않는다"
        );
    }

    #[test]
    fn theme_file_then_overrides() {
        let dir = std::env::temp_dir().join(format!("bt-theme-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let theme = dir.join("Mine");
        std::fs::write(
            &theme,
            "background = #000001\nforeground = #000002\npalette = 2=#000003\n",
        )
        .unwrap();
        let c = Config::from_text(&format!("theme = {}\nforeground = #abcdef\n", theme.display()), true);
        assert_eq!(c.theme.background, Rgb::new(0, 0, 1), "테마 바탕");
        assert_eq!(c.theme.foreground, Rgb::new(0xab, 0xcd, 0xef), "설정이 테마를 덮는다");
        assert_eq!(c.theme.palette[2], Rgb::new(0, 0, 3));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn config_file_include_and_cycle_guard() {
        let dir = std::env::temp_dir().join(format!("bt-cfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a"),
            "font-size = 20\nconfig-file = b\nconfig-file = ?missing\n",
        )
        .unwrap();
        std::fs::write(dir.join("b"), "background = #111111\nconfig-file = a\n").unwrap();
        let mut w = Vec::new();
        let mut s = Vec::new();
        let entries = read_entries(&dir.join("a"), 0, &mut w, &mut s);
        assert!(entries.iter().any(|e| e.key == "background"));
        assert!(
            w.iter().any(|m| m.contains("너무 깊게")),
            "서로 부르는 파일은 멈춘다: {w:?}"
        );
        assert!(!w.iter().any(|m| m.contains("missing")), "?가 붙은 파일은 없어도 된다");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bundled_themes_work_without_ghostty() {
        assert!(BUNDLED_THEMES.len() > 800, "넣어 둔 테마 {}개", BUNDLED_THEMES.len());
        let c = Config::from_text("theme = Dracula\n", true);
        assert_eq!(c.theme.background, Rgb::new(0x28, 0x2a, 0x36));
        assert_eq!(c.theme.selection_foreground, Some(Rgb::new(255, 255, 255)));
        let c = Config::from_text("theme = catppuccin mocha\n", true);
        assert_eq!(
            c.theme.background,
            Rgb::new(0x1e, 0x1e, 0x2e),
            "대소문자를 무시하고 찾는다"
        );
    }

    /// #150: command-palette-entry(Ghostty 1.2 문법)
    #[test]
    fn palette_entry_parses_like_ghostty() {
        let p = parse_palette_entry("title:Reset Font Style, action:csi:0m").unwrap();
        assert_eq!(
            (p.title.as_str(), p.action.as_str(), p.description.as_str()),
            ("Reset Font Style", "csi:0m", "")
        );
        let p = parse_palette_entry("title:\"a, b\",action:\"text:x,y\\n\" , description : 설명").unwrap();
        assert_eq!(p.title, "a, b");
        assert_eq!(p.action, "text:x,y\n");
        assert_eq!(p.description, "설명");
        assert!(parse_palette_entry("title:t").is_err(), "동작이 없다");
        assert!(parse_palette_entry("action:new_tab").is_err(), "이름이 없다");
        assert!(parse_palette_entry("title:t,bogus:1,action:new_tab").is_err());
        assert!(
            parse_palette_entry("title:\"t,action:new_tab").is_err(),
            "따옴표가 닫히지 않았다"
        );
        assert!(parse_palette_entry("t,action:new_tab").is_err());
    }

    #[test]
    fn palette_entry_stacks_clears_and_warns() {
        let c = Config::from_text(
            "command-palette-entry = title:A,action:new_tab\ncommand-palette-entry = title:B,action:quit\ncommand-palette-entry = title:x\n",
            false,
        );
        assert_eq!(c.palette_entries.len(), 2);
        assert_eq!(c.palette_entries[1].title, "B");
        assert!(!c.palette_defaults_cleared);
        assert_eq!(c.warnings.len(), 1);
        assert!(
            c.render()
                .contains("command-palette-entry = title:\"A\",action:\"new_tab\",description:\"\"")
        );
        // 빈 값: 기본 항목과 그 앞에 쌓인 것을 지운다. 뒤에 쓴 것은 남는다
        let c = Config::from_text(
            "command-palette-entry = title:A,action:new_tab\ncommand-palette-entry =\ncommand-palette-entry = title:B,action:quit\n",
            false,
        );
        assert!(c.palette_defaults_cleared);
        assert_eq!(c.palette_entries.len(), 1);
        assert_eq!(c.palette_entries[0].title, "B");
        let again = Config::from_text(&c.render(), false);
        assert_eq!(again.palette_entries, c.palette_entries);
        assert!(again.palette_defaults_cleared);
    }

    /// #96: font-feature(Ghostty 문법). 쌓이고, 같은 태그는 나중 값, 빈 값은 지움, 쉼표로 여럿
    #[test]
    fn font_feature_parses_like_ghostty() {
        assert_eq!(parse_font_feature("-calt"), Some((*b"calt", 0)));
        assert_eq!(parse_font_feature("+liga"), Some((*b"liga", 1)));
        assert_eq!(parse_font_feature("ss01"), Some((*b"ss01", 1)));
        assert_eq!(parse_font_feature("cv01=3"), Some((*b"cv01", 3)));
        assert_eq!(parse_font_feature("toolong"), None);
        assert_eq!(parse_font_feature("ca\u{1}t"), None);
        let c = Config::from_text(
            "font-feature = -calt\nfont-feature = ss01, -liga\nfont-feature = calt\nfont-feature = xx\n",
            false,
        );
        assert_eq!(c.font_features, vec![(*b"ss01", 1), (*b"liga", 0), (*b"calt", 1)]);
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
        assert!(c.render().contains("font-feature = -liga"));
        let c = Config::from_text("font-feature = -calt\nfont-feature =\n", false);
        assert!(c.font_features.is_empty());
    }
}
