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

/// `custom-shader` 경로 하나(Ghostty `RepeatablePath`). `optional`이면 `?`로 시작한 경로라 없어도 오류가 아니다.
/// 상대 경로는 그 줄이 있는 설정 파일의 폴더 기준으로 풀어 둔다(`~/`는 홈)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShaderPath {
    pub path: PathBuf,
    pub optional: bool,
}

/// `custom-shader` 값 하나를 푼다. 빈 값은 호출하는 쪽이 목록을 비우는 것으로 다룬다
pub fn parse_shader_path(v: &str, base: &Path) -> ShaderPath {
    let (optional, p) = match v.strip_prefix('?') {
        Some(rest) => (true, rest),
        None => (false, v),
    };
    ShaderPath {
        path: expand_home(p, base),
        optional,
    }
}

/// `custom-shader-animation`(Ghostty `CustomShaderAnimation`): 사용자 셰이더를 움직이는 때
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomShaderAnimation {
    /// 움직이지 않고 화면이 바뀔 때만 그린다
    False,
    /// 초점이 있는 칸만 움직인다(기본)
    True,
    /// 초점과 상관없이 늘 움직인다
    Always,
}

impl CustomShaderAnimation {
    pub fn parse(v: &str) -> Option<Self> {
        match v {
            "false" => Some(Self::False),
            "true" => Some(Self::True),
            "always" => Some(Self::Always),
            _ => None,
        }
    }

    fn render(self) -> &'static str {
        match self {
            Self::False => "false",
            Self::True => "true",
            Self::Always => "always",
        }
    }
}

/// `link-previews`(Ghostty 1.2): 링크 위에 있을 때 실제 주소 띠를 언제 보일지
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkPreviews {
    /// 찾은 모든 링크(기본)
    True,
    /// 보이지 않는다
    False,
    /// OSC 8 링크만(보이는 글과 주소가 다를 수 있는 것)
    Osc8,
}

impl LinkPreviews {
    /// 이 링크에 주소 띠를 보이는가. `explicit`는 OSC 8 링크
    pub fn shows(self, explicit: bool) -> bool {
        match self {
            LinkPreviews::True => true,
            LinkPreviews::False => false,
            LinkPreviews::Osc8 => explicit,
        }
    }
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

/// `mouse-shift-capture`(Ghostty): Shift를 누른 클릭을 프로그램(마우스 보고)이 받는가
/// 프로그램은 XTSHIFTESCAPE(`CSI > 1 s`·`CSI > 0 s`)로 `True`·`False`를 덮어쓴다. `Always`·`Never`는 덮어쓰지 못한다(#327)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MouseShiftCapture {
    #[default]
    False,
    True,
    Always,
    Never,
}

impl MouseShiftCapture {
    /// Shift를 누른 클릭을 프로그램에 보내는가
    pub fn sends_shift(self) -> bool {
        matches!(self, MouseShiftCapture::True | MouseShiftCapture::Always)
    }

    /// Shift를 누른 클릭을 프로그램에 보내는가. `program`은 프로그램이 XTSHIFTESCAPE로 정한 값(#327)
    pub fn allows(self, program: Option<bool>) -> bool {
        match self {
            MouseShiftCapture::Always => true,
            MouseShiftCapture::Never => false,
            MouseShiftCapture::True | MouseShiftCapture::False => program.unwrap_or(self == MouseShiftCapture::True),
        }
    }
}

/// `middle-click-action`(Ghostty)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MiddleClickAction {
    /// 선택 클립보드 붙여넣기. macOS에는 선택 클립보드가 없어 하는 일이 없다(기본)
    #[default]
    PrimaryPaste,
    ClipboardPaste,
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

/// 글자 묶음의 폭을 세는 방식(Ghostty `grapheme-width-method`, #180)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GraphemeWidthMethod {
    /// 글자(코드포인트)마다 폭을 센다. 프로그램이 DECSET 2027을 켜면 그때부터 묶음 폭
    Legacy,
    /// 이모지 ZWJ 묶음·U+FE0F·국기·피부색 같은 글자 묶음을 한 묶음의 폭으로 센다(기본)
    #[default]
    Unicode,
}

/// 투명도가 있는 글자·그림을 섞을 색 공간(Ghostty `alpha-blending`, #405). GPU 그리기 길에서만 뜻이 있다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AlphaBlending {
    /// sRGB 값 그대로 섞는다(기본, 지금까지의 화면)
    #[default]
    Native,
    /// 선형 공간으로 섞는다. 어두운 글자는 가늘고 밝은 글자는 굵어 보인다
    Linear,
    /// 선형으로 섞되 글자 덮음 정도를 보정해 `Native`와 거의 같은 굵기로 보이게 한다
    LinearCorrected,
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

/// 복사할 때 글자를 바꾸는 규칙 하나(Ghostty `clipboard-codepoint-map`, #176). 범위는 양 끝을 포함한다
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodepointRule {
    pub range: (u32, u32),
    pub replacement: String,
}

/// OSC 52 클립보드 쓰기 한 번의 고정 상한(바이트). `clipboard-write-limit-bytes`는 이보다 크게 늘리지 못하고 줄이기만 한다(ADR-013)
pub const OSC52_WRITE_CAP: usize = 1024 * 1024;

/// OSC 52로 온 글(바이트 수 `len`)을 클립보드에 넣어도 되는가. 고정 상한과 설정(`None`은 `unlimited`) 중 작은 쪽을 넘으면 안 된다
pub fn clipboard_write_allowed(len: usize, limit: Option<usize>) -> bool {
    len <= OSC52_WRITE_CAP && limit.is_none_or(|l| len <= l)
}

/// 복사할 글에 `clipboard-codepoint-map`을 적용한다. 겹치면 나중에 적은 규칙이 이긴다
pub fn map_clipboard_text(rules: &[CodepointRule], text: &str) -> String {
    if rules.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let cp = c as u32;
        match rules.iter().rev().find(|r| r.range.0 <= cp && cp <= r.range.1) {
            Some(r) => out.push_str(&r.replacement),
            None => out.push(c),
        }
    }
    out
}

/// `U+1F600` 꼴의 코드포인트 하나(앞의 `U+`는 있어야 한다). 서로게이트와 유니코드 밖은 읽지 못한다
fn parse_codepoint(s: &str) -> Option<u32> {
    let hex = s.trim().strip_prefix("U+").or_else(|| s.trim().strip_prefix("u+"))?;
    let cp = u32::from_str_radix(hex, 16).ok()?;
    char::from_u32(cp).map(|_| cp)
}

/// `clipboard-codepoint-map`의 값 하나(`범위[,범위…]=바꿀 글`)를 규칙들로 읽는다.
/// 범위는 `U+1234`나 `U+1234-U+5678`, 바꿀 글은 `U+ABCD` 하나이거나 아무 글(빈 글이면 그 글자를 지운다)
pub fn parse_codepoint_rules(v: &str) -> Option<Vec<CodepointRule>> {
    let (key, value) = v.split_once('=')?;
    let value = value.trim();
    let replacement = if value.starts_with("U+") {
        char::from_u32(parse_codepoint(value)?)?.to_string()
    } else {
        value.to_string()
    };
    let mut rules = Vec::new();
    for part in key.split(',') {
        let range = match part.split_once('-') {
            Some((a, b)) => (parse_codepoint(a)?, parse_codepoint(b)?),
            None => {
                let c = parse_codepoint(part)?;
                (c, c)
            }
        };
        if range.0 > range.1 {
            return None;
        }
        rules.push(CodepointRule {
            range,
            replacement: replacement.clone(),
        });
    }
    Some(rules)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionAsAlt {
    None,
    Left,
    Right,
    Both,
}

/// `macos-non-native-fullscreen`(Ghostty): 전체 화면을 새 Space 없이 하는가
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NonNativeFullscreen {
    /// 운영체제 기본 전체 화면
    #[default]
    False,
    /// 새 Space 없는 전체 화면, 메뉴 막대를 숨긴다
    True,
    /// 새 Space 없는 전체 화면, 메뉴 막대를 보인다
    VisibleMenu,
    /// 새 Space 없는 전체 화면, 노치를 피한다
    PaddedNotch,
}

/// `macos-window-buttons`(Ghostty): 제목 막대의 닫기·최소화·확대 단추
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MacWindowButtons {
    #[default]
    Visible,
    Hidden,
}

/// `window-decoration`(Ghostty): 창 꾸밈(제목 막대·테두리). macOS에서 `none`만 뜻이 다르고 나머지는 모두 기본 꾸밈이다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowDecoration {
    #[default]
    Auto,
    Client,
    Server,
    None,
}

/// `macos-titlebar-style`(Ghostty): macOS 제목 막대 모양
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MacTitlebarStyle {
    Native,
    /// Ghostty 릴리스 판의 기본
    #[default]
    Transparent,
    /// b-terminal은 탭 막대를 직접 그리므로 `transparent`와 같게 동작한다
    Tabs,
    Hidden,
}

/// `window-theme`(Ghostty): 창 자체의 밝음·어두움
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WindowTheme {
    #[default]
    Auto,
    System,
    Light,
    Dark,
    Ghostty,
}

/// `macos-titlebar-proxy-icon`(Ghostty): 제목 막대의 작업 폴더 아이콘
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MacTitlebarProxyIcon {
    #[default]
    Visible,
    Hidden,
}

/// `macos-dock-drop-behavior`(Ghostty): Dock 아이콘에 폴더·파일을 놓았을 때 여는 곳(#279)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MacDockDropBehavior {
    /// 지금 창의 새 탭(창이 없으면 새 창)
    #[default]
    NewTab,
    NewWindow,
}

/// `macos-hidden`(Ghostty): Dock과 앱 전환기에서 숨기는가
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MacHidden {
    #[default]
    Never,
    Always,
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
    /// 글꼴 미세 조정 키(#163, Ghostty `adjust-*`). 값은 `10%` 또는 픽셀
    pub adjust_font_baseline: Option<Adjust>,
    pub adjust_underline_position: Option<Adjust>,
    pub adjust_underline_thickness: Option<Adjust>,
    pub adjust_strikethrough_position: Option<Adjust>,
    pub adjust_strikethrough_thickness: Option<Adjust>,
    pub adjust_overline_position: Option<Adjust>,
    pub adjust_overline_thickness: Option<Adjust>,
    pub adjust_cursor_thickness: Option<Adjust>,
    pub adjust_cursor_height: Option<Adjust>,
    pub adjust_box_thickness: Option<Adjust>,
    /// 가변 글꼴 축(#163, Ghostty `font-variation*`): (축 태그, 값). `font-variation`은 모든 꼴에, 나머지는 그 꼴에만 더한다
    pub font_variation: Vec<([u8; 4], f32)>,
    pub font_variation_bold: Vec<([u8; 4], f32)>,
    pub font_variation_italic: Vec<([u8; 4], f32)>,
    pub font_variation_bold_italic: Vec<([u8; 4], f32)>,
    /// 꼴마다 고를 글꼴 스타일 이름(#174, Ghostty `font-style*`)
    pub font_style: FontStyleName,
    pub font_style_bold: FontStyleName,
    pub font_style_italic: FontStyleName,
    pub font_style_bold_italic: FontStyleName,
    /// 글자 범위마다 정한 글꼴(#174, Ghostty `font-codepoint-map`). 뒤에 쓴 것이 앞의 것을 덮는다
    pub font_codepoint_map: Vec<CodepointFont>,
    /// 글자 두껍게(#193, Ghostty `font-thicken`·`font-thicken-strength`). 기본 끔·255
    pub font_thicken: bool,
    pub font_thicken_strength: u8,
    /// 없는 굵게·기울임 꼴을 합성할지(#193, Ghostty `font-synthetic-style`). 기본 모두 켬
    pub font_synthetic_style: SyntheticStyle,
    /// 다듬기(합자)를 끊는 자리(#174, Ghostty `font-shaping-break`)
    pub font_shaping_break: ShapingBreak,
    /// 탭 줄 제목 글꼴(#241, Ghostty `window-title-font-family`). 없으면 터미널 글꼴로 그린다
    pub window_title_font_family: Option<String>,
    /// Nerd Font 아이콘 높이 보정(#174, Ghostty `adjust-icon-height`)
    pub adjust_icon_height: Option<Adjust>,
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
    /// 새 Space 없는 전체 화면(Ghostty `macos-non-native-fullscreen`, #179). `fullscreen`의 non-native 값도 여기로 온다
    pub non_native_fullscreen: NonNativeFullscreen,
    /// 칸 크기 단위로 창 크기를 바꾼다(Ghostty `window-step-resize`, #179)
    pub window_step_resize: bool,
    /// 제목 막대 단추(Ghostty `macos-window-buttons`, #179). 새 창부터 적용
    pub macos_window_buttons: MacWindowButtons,
    /// Dock·앱 전환기에서 숨김(Ghostty `macos-hidden`, #179)
    pub macos_hidden: MacHidden,
    /// 창 꾸밈(Ghostty `window-decoration`, #192)
    pub window_decoration: WindowDecoration,
    /// macOS 제목 막대 모양(Ghostty `macos-titlebar-style`, #192). 새 창부터 적용
    pub macos_titlebar_style: MacTitlebarStyle,
    /// macOS 창 그림자(Ghostty `macos-window-shadow`, #192)
    pub macos_window_shadow: bool,
    /// 창의 밝음·어두움(Ghostty `window-theme`, #192)
    pub window_theme: WindowTheme,
    /// 제목 막대 폴더 아이콘(Ghostty `macos-titlebar-proxy-icon`, #192·#279)
    pub macos_titlebar_proxy_icon: MacTitlebarProxyIcon,
    /// Dock 아이콘에 놓은 폴더·파일을 여는 곳(Ghostty `macos-dock-drop-behavior`, #279)
    pub macos_dock_drop_behavior: MacDockDropBehavior,
    /// `theme = light:…,dark:…`처럼 밝은·어두운 테마를 따로 정했는가(`window-theme = auto`가 system처럼 된다)
    pub theme_split: bool,
    /// 첫 창 자리(Ghostty `window-position-x`·`-y`, #148). 둘 다 있어야 쓴다
    pub window_position_x: Option<i32>,
    pub window_position_y: Option<i32>,
    pub option_as_alt: OptionAsAlt,
    /// 스크롤백 줄 수 상한. `scrollback-limit-bytes`와 `-lines`에서 정해진다(`scrollback_rows`, #177)
    pub scrollback_lines: usize,
    /// `scrollback-limit-bytes`(Ghostty 1.4, 기본 50MB). 없으면 `unlimited`
    pub scrollback_limit_bytes: Option<u64>,
    /// `scrollback-limit-lines`(Ghostty 1.4, 기본 `unlimited`)
    pub scrollback_limit_lines: Option<usize>,
    /// `palette-generate`(Ghostty 1.3, 기본 끔): 16~255번 색을 기본 16색에서 만든다(#177)
    pub palette_generate: bool,
    /// `palette-harmonious`(Ghostty 1.3, 기본 끔): 밝은 테마도 바탕·글자색 순서를 맞바꾸지 않는다
    pub palette_harmonious: bool,
    /// `palette = 100=#...`로 사용자가 정한 16~255번 색. 만든 색이 덮지 않는다
    pub palette_set: Vec<(u8, Rgb)>,
    /// `background-opacity-cells`(Ghostty 1.2, 기본 끔): 색이 있는 바탕 칸도 `background-opacity`로 비친다
    pub background_opacity_cells: bool,
    /// `link-url`(기본 켬): 화면 글의 URL 자동 인식
    pub link_url: bool,
    /// `link-osc8`(Ghostty 1.4, 기본 켬): OSC 8 링크를 링크로 다룬다
    pub link_osc8: bool,
    /// `link-previews`(기본 true): Cmd를 누른 채 링크 위에 있을 때 주소 띠
    pub link_previews: LinkPreviews,
    pub copy_on_select: CopyOnSelect,
    /// 클립보드로 복사하는 글의 줄 끝 공백·탭을 지운다(Ghostty `clipboard-trim-trailing-spaces`)
    pub clipboard_trim_trailing_spaces: bool,
    /// 글을 칠 때 고른 영역을 지운다(Ghostty `selection-clear-on-typing`, 기본 켬, #162)
    pub selection_clear_on_typing: bool,
    /// 복사한 뒤 고른 영역을 지운다(Ghostty `selection-clear-on-copy`, 기본 끔, #162)
    pub selection_clear_on_copy: bool,
    /// 오른쪽 클릭 동작(Ghostty `right-click-action`). 마우스 보고 모드의 프로그램에는 항상 클릭을 보낸다
    pub right_click_action: RightClickAction,
    /// 마우스 보고 켜기(Ghostty `mouse-reporting`, 기본 켬). 끄면 프로그램이 요청해도 보내지 않는다
    pub mouse_reporting: bool,
    /// Shift를 누른 클릭을 프로그램이 받는가(Ghostty `mouse-shift-capture`, 기본 `false`)
    pub mouse_shift_capture: MouseShiftCapture,
    /// 가운데 클릭 동작(Ghostty `middle-click-action`, 기본 `primary-paste`)
    pub middle_click_action: MiddleClickAction,
    /// GPU 길 창을 화면 갱신에 맞춰 올리는가(Ghostty `window-vsync`, 기본 켬). 끄면 맞추지 않고 올린다. 새 창부터 적용된다
    pub window_vsync: bool,
    /// 두 번·세 번 누르기 간격 밀리초(Ghostty `click-repeat-interval`). 0이면 플랫폼 값
    pub click_repeat_interval: u32,
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
    /// 첫 칸에만 쓰는 명령(Ghostty `initial-command`, #178). 없으면 `command`
    pub initial_command: Option<String>,
    /// 명령이 끝나도 칸을 남기고 키를 누르면 닫는다(Ghostty `wait-after-command`, 기본 끔)
    pub wait_after_command: bool,
    /// 이 밀리초보다 짧게 끝나면 비정상으로 본다(Ghostty `abnormal-command-exit-runtime`, 기본 250)
    pub abnormal_command_exit_runtime: u32,
    /// 첫 창을 띄울까(Ghostty `initial-window`, 기본 켬)
    pub initial_window: bool,
    /// 마지막 창이 닫히면 앱을 끝낼까(Ghostty `quit-after-last-window-closed`: Linux 켬, 그 밖에 끔)
    pub quit_after_last_window_closed: bool,
    /// 끝내기 전에 기다릴 시간(Ghostty `quit-after-last-window-closed-delay`, 기본 없음)
    pub quit_after_last_window_closed_delay: Option<std::time::Duration>,
    pub working_directory: WorkingDirectory,
    /// 정해 둔 창 제목(셸이 보낸 제목보다 먼저)
    pub title: Option<String>,
    /// `key-remap = from=to`(#196): 앱 안에서만 수정 키를 바꾼다. 쓴 차례대로 쌓인다
    pub key_remap: Vec<KeyRemap>,
    /// `macos-icon`(#196). 이름만 받고 `custom`일 때만 그림을 바꾼다
    pub macos_icon: MacosIcon,
    /// `macos-custom-icon`(#196): `macos-icon = custom`일 때 쓸 그림 파일. 빈 값이면 없음
    pub macos_custom_icon: Option<PathBuf>,
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
    /// 셸 통합 기능별 켜고 끄기(Ghostty `shell-integration-features`, #194)
    pub shell_integration_features: ShellFeatures,
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
    /// OSC 9;4 진행 표시를 그린다(Ghostty `progress-style`, 기본 켬). 끄면 조용히 버린다
    pub progress_style: bool,
    /// 스크롤 막대(Ghostty `scrollbar`, 기본 system)
    pub scrollbar: Scrollbar,
    /// 크게 보는 칸을 지키는 때(Ghostty `split-preserve-zoom`, 기본 아무 때도 아님)
    pub split_preserve_zoom: SplitPreserveZoom,
    /// 앱 자신의 알림 띠(Ghostty `app-notifications`)
    pub app_notifications: AppNotifications,
    /// OSC 52로 프로그램이 클립보드에 쓰는 것(Ghostty 기본 allow). ask는 창 안 확인 막대로 묻는다(#220)
    pub clipboard_write: crate::clipboard::ClipboardAccess,
    /// OSC 52로 프로그램이 클립보드를 읽는 것(기본 deny. Ghostty 기본은 ask라 다르다, ADR-019)
    pub clipboard_read: crate::clipboard::ClipboardAccess,
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
    /// 글자·그림을 섞을 색 공간(`alpha-blending`, #405). GPU 길에서만 적용한다
    pub alpha_blending: AlphaBlending,
    /// 바탕 불투명도(0~1, Ghostty `background-opacity`). 1보다 작으면 기본 바탕색 칸으로 창 뒤가 비친다(#78)
    pub background_opacity: f32,
    /// 창 뒤를 흐리는 세기 0~255(Ghostty `background-blur`, #238). 0이면 끔. macOS는 이 값을 그대로 반지름으로 쓴다
    pub background_blur: u8,
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
    /// ENQ(0x05)를 받으면 PTY로 돌려보낼 글(Ghostty `enquiry-response`, 기본 빈 값 = 답하지 않음, #176)
    pub enquiry_response: String,
    /// `term`: 칸의 TERM 값. Ghostty 기본은 `xterm-ghostty`지만 그 terminfo가 없어 `xterm-256color`를 지킨다(ADR-016, #190)
    pub term: String,
    /// `env`: 칸을 띄울 때 환경에 더할 변수. 쓴 차례대로 풀어 둔 결과다(#190)
    pub env: Vec<(String, String)>,
    /// `input`: 칸을 띄울 때 PTY에 보낼 데이터. 쓴 차례대로 이어 붙인다(#190)
    pub input: Vec<InputSource>,
    /// `custom-shader`(여러 번, 적은 순서대로 돈다)
    pub custom_shader: Vec<ShaderPath>,
    /// `custom-shader-animation`
    pub custom_shader_animation: CustomShaderAnimation,
    /// 프로그램이 키보드 잠금 모드(KAM, `CSI 2 h`)로 사람의 키 입력을 막을 수 있는가(Ghostty `vt-kam-allowed`, 기본 끔, #176)
    pub vt_kam_allowed: bool,
    /// 프로그램이 `CSI 8 ; 행 ; 열 t`로 창 크기를 바꿀 수 있는가(Ghostty `vt-window-resize-allowed`, 기본 끔, #176)
    pub vt_window_resize_allowed: bool,
    /// kitty 그림 저장 바이트 상한. 0이면 그림 프로토콜을 쓰지 않는다(Ghostty `image-storage-limit`, 기본 320000000, #176)
    pub image_storage_limit: u32,
    /// 프로그램의 클립보드 쓰기 한 번의 바이트 상한. `None`은 `unlimited`(Ghostty `clipboard-write-limit-bytes`, 기본 64MiB, #176)
    pub clipboard_write_limit_bytes: Option<usize>,
    /// 복사할 때 글자 바꾸기(Ghostty `clipboard-codepoint-map`, 여러 번, #176)
    pub clipboard_codepoint_map: Vec<CodepointRule>,
    /// 글자 묶음의 폭을 세는 방식(Ghostty `grapheme-width-method`, #180). 새로 연 칸부터 적용
    pub grapheme_width_method: GraphemeWidthMethod,
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
    /// 닫은 탭·칸을 되살릴 수 있는 시간(Ghostty `undo-timeout`, #164). 0이면 되살리기를 끈다
    pub undo_timeout: std::time::Duration,
    /// 글자색과 바탕의 최소 명암비(1~21, Ghostty `minimum-contrast`, #123). 1이면 끔
    pub minimum_contrast: f32,
    /// 분할에서 마우스가 올라간 칸을 고른다(Ghostty `focus-follows-mouse`, #126)
    pub focus_follows_mouse: bool,
    /// 마우스 휠 배율: (줄 단위 기기, 픽셀 단위 기기). Ghostty `mouse-scroll-multiplier` 기본 3·1
    pub scroll_multiplier: (f64, f64),
    /// 단어 고르기의 경계 글자(Ghostty `selection-word-chars`)
    pub selection_word_chars: String,
    /// 암호 입력을 알아채면 그 칸이 초점일 때 보안 키보드 입력을 켠다(Ghostty `macos-auto-secure-input`, 기본 켬, #258)
    pub macos_auto_secure_input: bool,
    /// 보안 입력이 켜져 있는 동안 창 오른쪽 위에 자물쇠를 보인다(Ghostty `macos-secure-input-indication`, 기본 켬, #258)
    pub macos_secure_input_indication: bool,
    pub warnings: Vec<String>,
    /// 실제로 읽은 파일
    pub sources: Vec<PathBuf>,
    /// 읽어 낸 설정 줄(시스템 모양이 바뀔 때 파일을 다시 읽지 않고 테마를 다시 고르려고 들고 있는다, #221)
    pub entries: Vec<Entry>,
    /// 프로그램이 `CSI Pi ; Pg ; Pt ; Pl ; Pb ; Pr * y`(DECRQCRA)로 화면 영역의 체크섬을 물으면 답한다(Ghostty `vt-xt-checksum-report`, 기본 끔, #259)
    pub vt_xt_checksum_report: bool,
    /// 체크섬을 계산하는 방식(Ghostty `vt-xt-checksum-extension`, #259)
    pub vt_xt_checksum_extension: XtChecksumExtension,
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

/// `scrollbar`(Ghostty): 스크롤 막대를 보이는가
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scrollbar {
    /// 시스템 방식(기본): 스크롤하는 동안과 그 뒤 잠깐, 또는 마우스가 오른쪽 끝 위에 있을 때만 보인다
    #[default]
    System,
    /// 그리지 않는다
    Never,
}

impl Scrollbar {
    /// 막대를 그리는가. `history`는 스크롤백 줄 수, `scrolling`은 방금 스크롤했다, `hovering`은 마우스가 오른쪽 끝 위에 있다
    pub fn shows(self, history: usize, scrolling: bool, hovering: bool) -> bool {
        match self {
            Scrollbar::Never => false,
            Scrollbar::System => history > 0 && (scrolling || hovering),
        }
    }
}

/// `split-preserve-zoom`(Ghostty): 크게 보는 칸을 언제 지키는가. 기본은 아무것도 지키지 않는다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SplitPreserveZoom {
    /// `navigation`: 크게 보는 중에 `goto_split`으로 옮기면 풀지 않고 새 칸을 크게 본다
    pub navigation: bool,
}

impl SplitPreserveZoom {
    /// `navigation`·`no-navigation`·`true`·`false`를 지금 값 위에 얹는다. 모르는 이름이 있으면 아무것도 바꾸지 않고 거짓
    pub fn apply(&mut self, v: &str) -> bool {
        let mut next = *self;
        for item in v.split(',').map(str::trim).filter(|i| !i.is_empty()) {
            match item {
                "true" | "navigation" => next.navigation = true,
                "false" | "no-navigation" => next.navigation = false,
                _ => return false,
            }
        }
        *self = next;
        true
    }

    /// `goto_split`으로 칸을 옮긴 뒤에도 크게 보기가 남는가. 크게 보던 중이 아니면 거짓
    pub fn keeps_zoom_on_goto(self, zoomed: bool) -> bool {
        zoomed && self.navigation
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

/// DECRQCRA 체크섬을 어떻게 계산하는지(Ghostty `vt-xt-checksum-extension`, #259). 켜짐이 DEC 단말의 계산이고,
/// 프로그램이 XTCHECKSUM으로 바꾼 것은 초기화(RIS·DECSTR)에서 이 값으로 돌아온다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XtChecksumExtension {
    /// 합을 뒤집어 보고한다
    pub negate: bool,
    /// 칸마다 굵게·밑줄·반전·숨김·보호 표시를 더한다
    pub attributes: bool,
    /// 사각형 첫 칸이 아닌 일반 빈칸을 뺀다
    pub trim: bool,
    /// 쓴 적 없는 칸을 빈칸으로 센다
    pub undrawn: bool,
    /// DEC 8비트 값 대신 코드포인트 전체를 쓴다
    pub full: bool,
}

impl Default for XtChecksumExtension {
    fn default() -> Self {
        Self {
            negate: true,
            attributes: true,
            trim: true,
            undrawn: false,
            full: false,
        }
    }
}

impl XtChecksumExtension {
    /// `no-negate,full` 같은 글을 지금 값 위에 얹는다. 모르는 이름이 있으면 아무것도 바꾸지 않고 거짓
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
                    next = Self {
                        negate: all,
                        attributes: all,
                        trim: all,
                        undrawn: all,
                        full: all,
                    };
                }
                "negate" => next.negate = on,
                "attributes" => next.attributes = on,
                "trim" => next.trim = on,
                "undrawn" => next.undrawn = on,
                "full" => next.full = on,
                _ => return false,
            }
        }
        *self = next;
        true
    }

    /// 모든 항목을 `negate,attributes,trim,no-undrawn,no-full`처럼 적는다(Ghostty 묶음 값의 출력 모양)
    pub fn to_list(&self) -> String {
        [
            (self.negate, "negate"),
            (self.attributes, "attributes"),
            (self.trim, "trim"),
            (self.undrawn, "undrawn"),
            (self.full, "full"),
        ]
        .into_iter()
        .map(|(on, n)| if on { n.to_string() } else { format!("no-{n}") })
        .collect::<Vec<_>>()
        .join(",")
    }
}

/// 셸 통합의 기능별 켜고 끄기(Ghostty `shell-integration-features`, #194).
/// Ghostty 기본: cursor·title·path 켬, sudo·ssh-env·ssh-terminfo 끔
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellFeatures {
    /// 프롬프트에서 커서를 막대로, 명령이 도는 동안은 원래대로
    pub cursor: bool,
    /// 받기만 한다. b-terminal은 TERM이 xterm-256color라 지킬 terminfo가 없다
    pub sudo: bool,
    /// 셸이 창 제목을 정한다(프롬프트에서는 폴더, 명령이 도는 동안은 명령)
    pub title: bool,
    /// 받기만 한다. TERM을 바꿀 일이 없다
    pub ssh_env: bool,
    /// 받기만 한다. 원격 호스트에 아무것도 설치하지 않는다
    pub ssh_terminfo: bool,
    /// b-terminal 실행 파일 폴더(btctl이 있는 곳)를 PATH 끝에 더한다
    pub path: bool,
}

impl Default for ShellFeatures {
    fn default() -> Self {
        Self {
            cursor: true,
            sudo: false,
            title: true,
            ssh_env: false,
            ssh_terminfo: false,
            path: true,
        }
    }
}

impl ShellFeatures {
    /// `no-cursor,sudo` 같은 글을 지금 값 위에 얹는다. `true`·`false`는 모두 켜고 끈다.
    /// 하나라도 모르는 이름이면 아무것도 바꾸지 않고 거짓
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
                    next = ShellFeatures {
                        cursor: all,
                        sudo: all,
                        title: all,
                        ssh_env: all,
                        ssh_terminfo: all,
                        path: all,
                    };
                }
                "cursor" => next.cursor = on,
                "sudo" => next.sudo = on,
                "title" => next.title = on,
                "ssh-env" => next.ssh_env = on,
                "ssh-terminfo" => next.ssh_terminfo = on,
                "path" => next.path = on,
                _ => return false,
            }
        }
        *self = next;
        true
    }

    /// 켜진 기능 이름을 쉼표로 이은 글. 셸 스크립트에는 환경 변수 `BT_SHELL_FEATURES`로 넘기고,
    /// `--show-config`에도 쓴다(모두 꺼졌으면 빈 글이라 거기서는 `false`로 적는다)
    pub fn to_list(&self) -> String {
        let on: Vec<&str> = [
            (self.cursor, "cursor"),
            (self.sudo, "sudo"),
            (self.title, "title"),
            (self.ssh_env, "ssh-env"),
            (self.ssh_terminfo, "ssh-terminfo"),
            (self.path, "path"),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .map(|(_, n)| n)
        .collect();
        on.join(",")
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
    /// Space(데스크톱)를 옮길 때 퀵 터미널이 따라오는가(Ghostty `quick-terminal-space-behavior`, #196)
    pub space: QuickSpace,
}

/// Space를 옮길 때 퀵 터미널의 동작(Ghostty `quick-terminal-space-behavior`). 기본 `move`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickSpace {
    /// 지금 Space로 따라온다
    Move,
    /// 처음 연 Space에만 남는다
    Remain,
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
            space: QuickSpace::Move,
        }
    }
}

/// 수정 키의 종류(Ghostty `key-remap`의 이름)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModKind {
    Shift,
    Ctrl,
    Alt,
    Super,
}

/// 수정 키의 왼쪽·오른쪽
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModSide {
    Left,
    Right,
}

/// `key-remap`의 한쪽: `ctrl`처럼 좌우 모두이거나 `left_ctrl`처럼 한쪽
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModKey {
    pub kind: ModKind,
    pub side: Option<ModSide>,
}

impl ModKey {
    /// Ghostty 이름: `shift`, `ctrl`/`control`, `alt`/`option`, `super`/`cmd`/`command`. 앞에 `left_`·`right_`
    fn parse(s: &str) -> Option<Self> {
        let (side, name) = if let Some(n) = s.strip_prefix("left_") {
            (Some(ModSide::Left), n)
        } else if let Some(n) = s.strip_prefix("right_") {
            (Some(ModSide::Right), n)
        } else {
            (None, s)
        };
        let kind = match name {
            "shift" => ModKind::Shift,
            "ctrl" | "control" => ModKind::Ctrl,
            "alt" | "option" => ModKind::Alt,
            "super" | "cmd" | "command" => ModKind::Super,
            _ => return None,
        };
        Some(Self { kind, side })
    }
}

/// 지금 눌린 수정 키를 좌우 따로 적은 것(비트 8개)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SidedMods(u8);

impl SidedMods {
    fn bit(kind: ModKind, side: ModSide) -> u8 {
        let k = match kind {
            ModKind::Shift => 0,
            ModKind::Ctrl => 1,
            ModKind::Alt => 2,
            ModKind::Super => 3,
        };
        1 << (k * 2 + usize::from(side == ModSide::Right))
    }

    /// 그 종류의 좌우 비트 모두
    fn kind_mask(kind: ModKind) -> u8 {
        Self::bit(kind, ModSide::Left) | Self::bit(kind, ModSide::Right)
    }

    pub fn with(mut self, kind: ModKind, side: ModSide) -> Self {
        self.0 |= Self::bit(kind, side);
        self
    }

    /// 어느 쪽이든 눌려 있는가
    pub fn has(self, kind: ModKind) -> bool {
        self.0 & Self::kind_mask(kind) != 0
    }

    pub fn has_side(self, kind: ModKind, side: ModSide) -> bool {
        self.0 & Self::bit(kind, side) != 0
    }
}

/// `key-remap = from=to` 하나(Ghostty와 같다)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyRemap {
    pub from: ModKey,
    pub to: ModKey,
}

impl KeyRemap {
    pub fn parse(v: &str) -> Option<Self> {
        let (from, to) = v.split_once('=')?;
        Some(Self {
            from: ModKey::parse(from.trim())?,
            to: ModKey::parse(to.trim())?,
        })
    }
}

impl std::fmt::Display for ModKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.side {
            Some(ModSide::Left) => f.write_str("left_")?,
            Some(ModSide::Right) => f.write_str("right_")?,
            None => {}
        }
        f.write_str(match self.kind {
            ModKind::Shift => "shift",
            ModKind::Ctrl => "ctrl",
            ModKind::Alt => "alt",
            ModKind::Super => "super",
        })
    }
}

impl std::fmt::Display for KeyRemap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}={}", self.from, self.to)
    }
}

impl std::fmt::Display for MacosIcon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Official => "official",
            Self::Builtin => "(내장 그림 이름)",
            Self::Custom => "custom",
            Self::CustomStyle => "custom-style",
        })
    }
}

/// 누른 수정 키에 `key-remap`을 적용한다. 일치는 누른 그대로(바꾸기 전)로 보므로 이어서 바뀌지 않는다
/// (`ctrl=super`와 `alt=ctrl`이면 Alt는 Ctrl이 될 뿐 Super가 되지 않는다). `ctrl`은 좌우 모두에,
/// `left_ctrl`은 왼쪽에만 맞는다. 바뀐 키는 `to`의 종류로 눌린 것이 된다
pub fn apply_key_remap(remaps: &[KeyRemap], pressed: SidedMods) -> SidedMods {
    let mut out = pressed.0;
    let mut add = 0u8;
    for r in remaps {
        let from = match r.from.side {
            Some(side) => SidedMods::bit(r.from.kind, side),
            None => SidedMods::kind_mask(r.from.kind),
        };
        let hit = pressed.0 & from;
        if hit != 0 {
            out &= !hit;
            add |= SidedMods::bit(r.to.kind, r.to.side.unwrap_or(ModSide::Left));
        }
    }
    // 지우는 일을 다 한 뒤에 더해야 서로 바꾸기(`ctrl=alt`, `alt=ctrl`)가 서로를 지우지 않는다
    SidedMods(out | add)
}

/// `macos-icon` 값(Ghostty). 이름 그림은 b-terminal에 없어 `custom`만 그림을 바꾼다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacosIcon {
    /// `official`: 앱에 들어 있는 그림(기본)
    Official,
    /// Ghostty의 다른 내장 그림 이름(`blueprint`, `chalkboard` 등): 받기만 한다
    Builtin,
    /// `custom`: `macos-custom-icon`의 파일
    Custom,
    /// `custom-style`: 받기만 한다
    CustomStyle,
}

impl MacosIcon {
    fn parse(v: &str) -> Option<Self> {
        Some(match v {
            "official" => Self::Official,
            "blueprint" | "chalkboard" | "microchip" | "glass" | "holographic" | "paper" | "retro" | "xray" => {
                Self::Builtin
            }
            "custom" => Self::Custom,
            "custom-style" => Self::CustomStyle,
            _ => return None,
        })
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
            adjust_font_baseline: None,
            adjust_underline_position: None,
            adjust_underline_thickness: None,
            adjust_strikethrough_position: None,
            adjust_strikethrough_thickness: None,
            adjust_overline_position: None,
            adjust_overline_thickness: None,
            adjust_cursor_thickness: None,
            adjust_cursor_height: None,
            adjust_box_thickness: None,
            font_variation: Vec::new(),
            font_variation_bold: Vec::new(),
            font_variation_italic: Vec::new(),
            font_variation_bold_italic: Vec::new(),
            font_style: FontStyleName::Default,
            font_style_bold: FontStyleName::Default,
            font_style_italic: FontStyleName::Default,
            font_style_bold_italic: FontStyleName::Default,
            font_codepoint_map: Vec::new(),
            font_thicken: false,
            font_thicken_strength: 255,
            font_synthetic_style: SyntheticStyle::default(),
            font_shaping_break: ShapingBreak::default(),
            window_title_font_family: None,
            adjust_icon_height: None,
            theme: Theme::ghostty_default(),
            cursor_style: CursorStyle::Block,
            cursor_blink: None,
            padding_x: (2.0, 2.0),
            padding_y: (2.0, 2.0),
            window_width: None,
            window_height: None,
            maximize: false,
            fullscreen: false,
            non_native_fullscreen: NonNativeFullscreen::False,
            window_step_resize: false,
            macos_window_buttons: MacWindowButtons::Visible,
            window_decoration: WindowDecoration::Auto,
            macos_titlebar_style: MacTitlebarStyle::Transparent,
            macos_window_shadow: true,
            window_theme: WindowTheme::Auto,
            macos_titlebar_proxy_icon: MacTitlebarProxyIcon::Visible,
            macos_dock_drop_behavior: MacDockDropBehavior::NewTab,
            theme_split: false,
            macos_hidden: MacHidden::Never,
            window_position_x: None,
            window_position_y: None,
            option_as_alt: OptionAsAlt::None,
            scrollback_lines: scrollback_rows(Some(SCROLLBACK_BYTES_DEFAULT), None),
            scrollback_limit_bytes: Some(SCROLLBACK_BYTES_DEFAULT),
            scrollback_limit_lines: None,
            palette_generate: false,
            palette_harmonious: false,
            palette_set: Vec::new(),
            background_opacity_cells: false,
            link_url: true,
            link_osc8: true,
            link_previews: LinkPreviews::True,
            copy_on_select: CopyOnSelect::True,
            clipboard_trim_trailing_spaces: true,
            selection_clear_on_typing: true,
            selection_clear_on_copy: false,
            right_click_action: RightClickAction::ContextMenu,
            mouse_reporting: true,
            mouse_shift_capture: MouseShiftCapture::False,
            middle_click_action: MiddleClickAction::PrimaryPaste,
            click_repeat_interval: 0,
            window_vsync: true,
            window_inherit_working_directory: true,
            tab_inherit_working_directory: true,
            split_inherit_working_directory: true,
            window_inherit_font_size: true,
            window_new_tab_position: WindowNewTabPosition::Current,
            window_show_tab_bar: WindowShowTabBar::Auto,
            mouse_hide_while_typing: false,
            command: None,
            initial_command: None,
            wait_after_command: false,
            abnormal_command_exit_runtime: 250,
            initial_window: true,
            quit_after_last_window_closed: cfg!(target_os = "linux"),
            quit_after_last_window_closed_delay: None,
            working_directory: WorkingDirectory::Inherit,
            title: None,
            keybinds: Vec::new(),
            palette_entries: Vec::new(),
            palette_defaults_cleared: false,
            mouse_reset_on_shell: true,
            clipboard_write: crate::clipboard::ClipboardAccess::Allow,
            clipboard_read: crate::clipboard::ClipboardAccess::Deny,
            paste_protection: true,
            paste_bracketed_safe: true,
            shell_integration: "detect".into(),
            shell_integration_features: ShellFeatures::default(),
            notify_command_seconds: 10,
            notify_on_command_finish: None,
            notify_on_command_finish_action: FinishAction::default(),
            notify_on_command_finish_after: std::time::Duration::from_secs(5),
            desktop_notifications: true,
            progress_style: true,
            scrollbar: Scrollbar::System,
            split_preserve_zoom: SplitPreserveZoom::default(),
            app_notifications: AppNotifications::default(),
            unfocused_split_opacity: 0.7,
            unfocused_split_fill: None,
            split_divider_color: None,
            colorspace_srgb: false,
            alpha_blending: AlphaBlending::Native,
            background_opacity: 1.0,
            background_blur: 0,
            window_save_state: true,
            bell_features: BellFeatures::default(),
            bell_audio_path: None,
            bell_audio_volume: 0.5,
            key_remap: Vec::new(),
            macos_icon: MacosIcon::Official,
            macos_custom_icon: None,
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
            enquiry_response: String::new(),
            term: DEFAULT_TERM.to_string(),
            env: Vec::new(),
            input: Vec::new(),
            custom_shader: Vec::new(),
            custom_shader_animation: CustomShaderAnimation::True,
            vt_kam_allowed: false,
            vt_window_resize_allowed: false,
            image_storage_limit: 320_000_000,
            clipboard_write_limit_bytes: Some(64 * 1024 * 1024),
            clipboard_codepoint_map: Vec::new(),
            grapheme_width_method: GraphemeWidthMethod::Unicode,
            padding_balance: false,
            padding_color: PaddingColor::Background,
            cursor_opacity: 1.0,
            cursor_color_cell: None,
            cursor_text_cell: None,
            resize_overlay: ResizeOverlay::AfterFirst,
            resize_overlay_position: OverlayPosition::Center,
            resize_overlay_duration: std::time::Duration::from_millis(750),
            undo_timeout: std::time::Duration::from_secs(5),
            minimum_contrast: 1.0,
            focus_follows_mouse: false,
            scroll_multiplier: (3.0, 1.0),
            selection_word_chars: DEFAULT_WORD_CHARS.into(),
            macos_auto_secure_input: true,
            macos_secure_input_indication: true,
            warnings: Vec::new(),
            sources: Vec::new(),
            entries: Vec::new(),
            vt_xt_checksum_report: false,
            vt_xt_checksum_extension: XtChecksumExtension::default(),
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

/// `term`의 기본. Ghostty는 `xterm-ghostty`지만 b-terminal에는 그 terminfo가 없어 지금 값을 지킨다(ADR-016)
pub const DEFAULT_TERM: &str = "xterm-256color";

/// `input` 전체 크기 상한(바이트). Ghostty는 `path:` 파일 하나를 10MB까지 읽는다. b-terminal은 `raw:`와 파일을 합친
/// 전체에 같은 값을 건다(ADR-016)
pub const INPUT_LIMIT_BYTES: usize = 10 * 1024 * 1024;

/// `input` 하나: 쓴 글(`raw:`) 또는 읽을 파일(`path:`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputSource {
    Raw(Vec<u8>),
    Path(PathBuf),
}

impl InputSource {
    /// `--show-config`에 적을 글(다시 읽으면 같은 값이 된다)
    fn render(&self) -> String {
        match self {
            InputSource::Path(p) => format!("path:{}", p.display()),
            InputSource::Raw(b) => {
                let mut s = String::from("raw:");
                for (i, &c) in b.iter().enumerate() {
                    match c {
                        b'\\' => s.push_str("\\\\"),
                        b'"' => s.push_str("\\\""),
                        // 줄 끝의 공백은 설정을 읽을 때 잘리므로 적어 둔다
                        b' ' if i + 1 == b.len() => s.push_str("\\x20"),
                        0x20..=0x7e => s.push(c as char),
                        _ => s.push_str(&format!("\\x{c:02x}")),
                    }
                }
                s
            }
        }
    }
}

/// `term`·환경 변수 값으로 쓸 수 있는 글인가: NUL과 제어 문자가 없어야 한다
fn valid_env_value(v: &str) -> bool {
    !v.chars().any(char::is_control)
}

/// `env` 한 줄을 풀어 맵(쓴 차례)에 반영한다(Ghostty `RepeatableStringMap`과 같은 뜻).
/// 빈 값은 맵 전체를 비우고, `KEY=`는 그 키를 지우고, `KEY=VALUE`는 넣거나 덮는다.
/// 덮어도 처음 자리를 지킨다. 키가 비었거나 `=`가 없거나 제어 문자가 있으면 오류
pub fn apply_env(map: &mut Vec<(String, String)>, v: &str) -> Result<(), String> {
    if v.is_empty() {
        map.clear();
        return Ok(());
    }
    let Some((k, val)) = v.split_once('=') else {
        return Err(format!("`키=값` 꼴이 아닙니다: {v}"));
    };
    let (k, val) = (k.trim(), val.trim());
    if k.is_empty() || !valid_env_value(k) || !valid_env_value(val) {
        return Err(format!("키가 비었거나 제어 문자가 있습니다: {v:?}"));
    }
    let pos = map.iter().position(|(mk, _)| mk == k);
    match (pos, val.is_empty()) {
        (Some(i), true) => {
            map.remove(i);
        }
        (Some(i), false) => map[i].1 = val.to_string(),
        (None, true) => {}
        (None, false) => map.push((k.to_string(), val.to_string())),
    }
    Ok(())
}

/// Zig 문자열 리터럴 이스케이프를 푼다: `\n \r \t \\ \' \"`, `\xNN`(바이트 하나), `\u{코드포인트}`(UTF-8).
/// 그 밖의 `\`와 끝에 홀로 남은 `\`는 오류다. 이스케이프가 아닌 글자는 그대로 둔다
pub fn parse_zig_string(s: &str) -> Result<Vec<u8>, String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' {
            out.push(b[i]);
            i += 1;
            continue;
        }
        i += 1;
        let Some(&c) = b.get(i) else {
            return Err("끝에 홀로 남은 `\\`가 있습니다".into());
        };
        i += 1;
        match c {
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'\\' | b'\'' | b'"' => out.push(c),
            b'x' => {
                let hex = b.get(i..i + 2).and_then(|h| std::str::from_utf8(h).ok());
                let n = hex.filter(|h| h.bytes().all(|c| c.is_ascii_hexdigit()));
                let Some(n) = n.and_then(|h| u8::from_str_radix(h, 16).ok()) else {
                    return Err("`\\x` 뒤에는 16진수 두 자리가 와야 합니다".into());
                };
                out.push(n);
                i += 2;
            }
            b'u' => {
                let Some(rest) = s.get(i..).and_then(|r| r.strip_prefix('{')) else {
                    return Err("`\\u` 뒤에는 `{코드포인트}`가 와야 합니다".into());
                };
                let Some(end) = rest.find('}') else {
                    return Err("`\\u{`가 닫히지 않았습니다".into());
                };
                let digits = &rest[..end];
                let ch = (!digits.is_empty() && digits.bytes().all(|c| c.is_ascii_hexdigit()))
                    .then(|| u32::from_str_radix(digits, 16).ok())
                    .flatten()
                    .and_then(char::from_u32);
                let Some(ch) = ch else {
                    return Err(format!("`\\u{{{digits}}}`는 글자가 아닙니다"));
                };
                let mut buf = [0u8; 4];
                out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                i += 1 + end + 1;
            }
            _ => return Err(format!("모르는 이스케이프 `\\{}`입니다", c as char)),
        }
    }
    Ok(out)
}

/// `input` 값 하나(`raw:…`, `path:…`, 접두사 없으면 `raw:`)를 푼다. `path:`의 상대 경로는 설정 파일의 폴더 기준이고
/// 파일은 칸을 띄울 때 읽는다(없는 파일은 설정을 읽을 때 알 수 없다)
pub fn parse_input(v: &str, base: &Path) -> Result<InputSource, String> {
    if let Some(p) = v.strip_prefix("path:") {
        if p.is_empty() {
            return Err("path: 뒤에 경로가 없습니다".into());
        }
        return Ok(InputSource::Path(expand_home(p, base)));
    }
    let raw = v.strip_prefix("raw:").unwrap_or(v);
    parse_zig_string(raw).map(InputSource::Raw)
}

/// `input` 목록을 PTY에 보낼 바이트로 만든다. 글(`raw:`)과 파일을 합친 전체가 `INPUT_LIMIT_BYTES`를 넘거나,
/// 파일이 없거나 보통 파일이 아니거나(폴더·장치·파이프는 끝이 없거나 읽다 멈출 수 있다) 못 읽으면 오류이고, 이때는
/// 하나도 보내지 않는다(Ghostty처럼)
pub fn resolve_input(sources: &[InputSource]) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut out: Vec<u8> = Vec::new();
    for src in sources {
        let room = INPUT_LIMIT_BYTES - out.len();
        match src {
            InputSource::Raw(b) => {
                if b.len() > room {
                    return Err(format!("input의 전체 크기가 상한 {INPUT_LIMIT_BYTES}바이트를 넘습니다"));
                }
                out.extend_from_slice(b);
            }
            InputSource::Path(p) => {
                let show = p.display();
                let meta = std::fs::metadata(p).map_err(|e| format!("{show}을 읽지 못했습니다: {e}"))?;
                if !meta.is_file() {
                    return Err(format!("{show}은 보통 파일이 아닙니다"));
                }
                if meta.len() > room as u64 {
                    return Err(format!("{show}이 상한 {INPUT_LIMIT_BYTES}바이트를 넘습니다"));
                }
                let f = std::fs::File::open(p).map_err(|e| format!("{show}을 읽지 못했습니다: {e}"))?;
                // 재는 사이에 커졌을 수 있으니 읽을 때도 한 바이트 넘게까지만 읽고 센다
                let before = out.len();
                f.take(room as u64 + 1)
                    .read_to_end(&mut out)
                    .map_err(|e| format!("{show}을 읽지 못했습니다: {e}"))?;
                if out.len() - before > room {
                    return Err(format!("{show}이 상한 {INPUT_LIMIT_BYTES}바이트를 넘습니다"));
                }
            }
        }
    }
    Ok(out)
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

/// `font-style*` 값(Ghostty `FontStyle`): `default`는 글꼴 찾기가 고른 그대로, `false`는 그 꼴을 끄고(보통 꼴로 그림),
/// 그 밖의 글은 글꼴이 알리는 스타일 이름(예 "Iosevka Heavy"의 `Heavy`)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FontStyleName {
    #[default]
    Default,
    Disabled,
    Named(String),
}

impl FontStyleName {
    pub fn parse(v: &str) -> Self {
        match v {
            "" | "default" => Self::Default,
            "false" => Self::Disabled,
            name => Self::Named(name.to_string()),
        }
    }
}

/// `font-synthetic-style`(Ghostty `FontSyntheticStyle`): 꼴마다, 글꼴에 그 꼴이 없을 때 합성할지. 기본은 모두 켬
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyntheticStyle {
    pub bold: bool,
    pub italic: bool,
    pub bold_italic: bool,
}

impl Default for SyntheticStyle {
    fn default() -> Self {
        Self {
            bold: true,
            italic: true,
            bold_italic: true,
        }
    }
}

impl SyntheticStyle {
    /// `true`·`false`나 쉼표로 가른 `bold`·`italic`·`bold-italic`(앞에 `no-`면 끔). 기본값에서 시작해 적은 것만 바꾼다.
    /// 모르는 이름이 하나라도 있으면 없다
    pub fn parse(v: &str) -> Option<Self> {
        let mut out = Self::default();
        for item in v.split(',').map(str::trim).filter(|i| !i.is_empty()) {
            let (on, name) = match item.strip_prefix("no-") {
                Some(n) => (false, n),
                None => (true, item),
            };
            match name {
                // `true`·`false`는 `no-`가 붙지 않은 것만(`no-true`는 모르는 이름)
                "true" | "false" if item == name => {
                    let on = name == "true";
                    out = Self {
                        bold: on,
                        italic: on,
                        bold_italic: on,
                    };
                }
                "bold" => out.bold = on,
                "italic" => out.italic = on,
                "bold-italic" => out.bold_italic = on,
                _ => return None,
            }
        }
        Some(out)
    }

    /// `--show-config`에 적는 값(다시 읽으면 같은 값)
    fn render(self) -> String {
        let name = |on: bool, n: &str| if on { n.to_string() } else { format!("no-{n}") };
        [
            name(self.bold, "bold"),
            name(self.italic, "italic"),
            name(self.bold_italic, "bold-italic"),
        ]
        .join(",")
    }
}

/// `font-codepoint-map` 한 칸: `start..=end` 글자는 `family` 글꼴로
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodepointFont {
    pub start: u32,
    pub end: u32,
    pub family: String,
}

/// `font-codepoint-map` 한 줄(`U+3000-U+9FFF,U+AC00=글꼴`)을 읽는다. 읽지 못하면 없음
/// Ghostty와 같은 문법: 글자는 `U+` 다음 16진수(큰 `U`만), 범위는 `U+ABCD-U+DEF0`, 한 글꼴에 여러 범위는 쉼표.
/// 글꼴 이름은 `font-family`와 같은 값이고 비어 있으면 안 된다
pub fn parse_codepoint_map(s: &str) -> Option<Vec<CodepointFont>> {
    let (key, family) = s.split_once('=')?;
    let family = family.trim_matches([' ', '\t']);
    if family.is_empty() {
        return None;
    }
    let ranges = parse_codepoint_ranges(key.trim_matches([' ', '\t']))?;
    Some(
        ranges
            .into_iter()
            .map(|(start, end)| CodepointFont {
                start,
                end,
                family: family.to_string(),
            })
            .collect(),
    )
}

fn parse_codepoint_ranges(key: &str) -> Option<Vec<(u32, u32)>> {
    let b = key.as_bytes();
    let mut i = 0;
    let ws = |i: &mut usize| {
        while *i < b.len() && matches!(b[*i], b' ' | b'\t') {
            *i += 1;
        }
    };
    let codepoint = |i: &mut usize| -> Option<u32> {
        if b.get(*i) != Some(&b'U') || b.get(*i + 1) != Some(&b'+') {
            return None;
        }
        *i += 2;
        let from = *i;
        while *i < b.len() && b[*i].is_ascii_hexdigit() {
            *i += 1;
        }
        u32::from_str_radix(key.get(from..*i)?, 16)
            .ok()
            .filter(|c| *c <= 0x10FFFF)
    };
    let mut out = Vec::new();
    while i < b.len() {
        let start = codepoint(&mut i)?;
        ws(&mut i);
        match b.get(i) {
            None => out.push((start, start)),
            Some(b',') => {
                i += 1;
                ws(&mut i);
                if i >= b.len() {
                    return None;
                }
                out.push((start, start));
            }
            Some(b'-') => {
                i += 1;
                ws(&mut i);
                let end = codepoint(&mut i)?;
                ws(&mut i);
                match b.get(i) {
                    None => {}
                    Some(b',') => {
                        i += 1;
                        ws(&mut i);
                    }
                    Some(_) => return None,
                }
                if start > end {
                    return None;
                }
                out.push((start, end));
            }
            Some(_) => return None,
        }
    }
    (!out.is_empty()).then_some(out)
}

/// `font-shaping-break`(Ghostty): 다듬기를 끊는 자리. 지금은 `cursor`(커서 칸)뿐이고 기본 켬
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapingBreak {
    pub cursor: bool,
}

impl Default for ShapingBreak {
    fn default() -> Self {
        Self { cursor: true }
    }
}

impl ShapingBreak {
    /// 읽지 못하면 아무것도 바꾸지 않고 거짓
    pub fn parse(v: &str) -> Option<Self> {
        // Ghostty처럼 기본값에서 시작해 적은 것만 바꾼다(앞서 쓴 값 위에 얹지 않는다)
        let mut out = Self::default();
        for item in v.split(',').map(str::trim).filter(|i| !i.is_empty()) {
            let (on, name) = match item.strip_prefix("no-") {
                Some(n) => (false, n),
                None => (true, item),
            };
            match name {
                "true" | "false" if item == name => out.cursor = name == "true",
                "cursor" => out.cursor = on,
                _ => return None,
            }
        }
        Some(out)
    }
}

/// `adjust-*` 값 하나를 읽어 둔다. 빈 값은 지우고, 읽지 못하면 `fail`을 부르고 안 정한 채로 둔다
fn set_adjust(slot: &mut Option<Adjust>, v: &str, fail: impl FnOnce()) {
    *slot = if v.is_empty() {
        None
    } else {
        Adjust::parse(v).or_else(|| {
            fail();
            None
        })
    };
}

/// 가변 글꼴 축 하나(Ghostty `font-variation`): `축=값`, 축은 네 글자, 값은 유한한 수. 양쪽 공백은 무시
pub fn parse_font_variation(s: &str) -> Option<([u8; 4], f32)> {
    let (tag, val) = s.split_once('=')?;
    let tag: [u8; 4] = tag.trim().as_bytes().try_into().ok()?;
    let val: f32 = val.trim().parse().ok()?;
    (tag.iter().all(|b| b.is_ascii_graphic()) && val.is_finite()).then_some((tag, val))
}

/// `font-variation*` 한 줄을 목록에 더한다. 같은 축을 다시 쓰면 뒤가 이기고, 빈 값은 목록을 비운다
fn set_variation(list: &mut Vec<([u8; 4], f32)>, v: &str, fail: impl FnOnce()) {
    if v.is_empty() {
        list.clear();
    } else if let Some(var) = parse_font_variation(v) {
        list.retain(|(t, _)| *t != var.0);
        list.push(var);
    } else {
        fail();
    }
}

/// 글꼴 기능 하나(Ghostty 문법): `calt`·`+calt`는 켬, `-calt`는 끔, `calt=2`는 그 값. 태그는 네 글자(ASCII)
pub fn parse_font_feature(s: &str) -> Option<([u8; 4], u16)> {
    let s = s.trim();
    // 앞의 `+`는 켬, `-`는 끔(값을 따로 쓸 수 없다)
    let (sign, rest) = match s.as_bytes().first()? {
        b'+' => (Some(true), &s[1..]),
        b'-' => (Some(false), &s[1..]),
        _ => (None, s),
    };
    // 이름: 따옴표로 감쌌으면 그 안, 아니면 공백·`=` 앞까지(CSS `font-feature-settings`와 같은 모양)
    let (name, tail) = match rest.chars().next()? {
        q @ ('"' | '\'') => {
            let body = &rest[1..];
            let end = body.find(q)?;
            (&body[..end], &body[end + 1..])
        }
        _ => {
            let end = rest.find(|c: char| c.is_whitespace() || c == '=').unwrap_or(rest.len());
            (&rest[..end], &rest[end..])
        }
    };
    let tag: [u8; 4] = name.as_bytes().try_into().ok()?;
    if !tag.iter().all(|b| b.is_ascii_graphic()) {
        return None;
    }
    let tail = tail.trim();
    let tail = tail.strip_prefix('=').map_or(tail, str::trim);
    let val = match (sign, tail) {
        (Some(on), "") => u16::from(on),
        (Some(_), _) => return None,
        (None, "") | (None, "on") => 1,
        (None, "off") => 0,
        (None, v) => v.parse().ok()?,
    };
    Some((tag, val))
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

/// `background-blur` 값 → 세기(#238). Ghostty `BackgroundBlur.parseCLI`와 같다: `true`=20, `false`=0,
/// 0~255 정수(u8). 불리언은 Ghostty `parseBool`처럼 `1 t T true`·`0 f F false`. macOS 26 유리 값
/// (`macos-glass-regular`·`macos-glass-clear`)은 b-terminal에 유리 효과가 없어 흐림 켬(20)으로 본다
pub fn parse_background_blur(v: &str) -> Option<u8> {
    match v.trim() {
        "true" | "1" | "t" | "T" | "macos-glass-regular" | "macos-glass-clear" => Some(20),
        "false" | "0" | "f" | "F" => Some(0),
        n if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => n.parse::<u8>().ok(),
        _ => None,
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

/// 넣어 둔 테마 파일의 글(없는 이름이면 `None`). `+list-themes --color`가 배경색을 읽는다(#240)
pub fn bundled_theme_text(name: &str) -> Option<&'static str> {
    BUNDLED_THEMES.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
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

    /// 명령줄 `-e`로 띄웠을 때 Ghostty가 함께 바꾸는 것: 명령이 끝나면 앱도 끝난다
    pub fn apply_cli_command(&mut self) {
        self.quit_after_last_window_closed = true;
        self.quit_after_last_window_closed_delay = None;
    }
}

/// `scrollback-limit-bytes` 기본. 릴리스된 Ghostty(1.3까지)의 10MB를 따른다.
/// 1.4 소스는 50MB로 올렸지만 스크롤백 압축과 함께이고, b-terminal에는 압축이 없다
pub const SCROLLBACK_BYTES_DEFAULT: u64 = 10_000_000;
/// 스크롤백 줄 수의 맨 위(두 상한이 모두 `unlimited`여도 이만큼에서 멈춘다)
pub const SCROLLBACK_ROWS_MAX: usize = 1_000_000;

/// 두 상한(`scrollback-limit-bytes`, `scrollback-limit-lines`; 없으면 `unlimited`)에서 VT 엔진에 줄 스크롤백 줄 수를 정한다.
/// 엔진은 줄 수 상한만 있어 바이트는 1000바이트를 한 줄로 쳐서 줄 수로 바꾼다(100줄 이상).
/// 실제 줄 하나는 창 폭만큼 칸(24바이트)을 쥐어 이보다 크다(124칸 창에서 약 3.2KB, 실험 E17)
/// Ghostty처럼 둘 다 정했으면 먼저 닿는 쪽, 곧 작은 쪽이다
pub fn scrollback_rows(bytes: Option<u64>, lines: Option<usize>) -> usize {
    let from_bytes = match bytes {
        Some(b) => ((b / 1000).min(SCROLLBACK_ROWS_MAX as u64) as usize).max(100),
        None => SCROLLBACK_ROWS_MAX,
    };
    let from_lines = lines.map_or(SCROLLBACK_ROWS_MAX, |n| n.min(SCROLLBACK_ROWS_MAX));
    from_bytes.min(from_lines)
}

/// 한도 값(Ghostty `Limit`): 빈 값은 기본, `unlimited`는 한도 없음, 아니면 수. 읽지 못하면 없다
fn parse_limit<T: std::str::FromStr>(v: &str, default: Option<T>) -> Option<Option<T>> {
    match v.trim() {
        "" => Some(default),
        "unlimited" => Some(None),
        n => n.parse::<T>().ok().map(Some),
    }
}

impl Config {
    /// 기본값 → Ghostty 설정 → b-terminal 설정 → 덧붙인 파일 순서로 읽는다. `dark`는 `theme = light:…,dark:…`에서 고를 쪽
    pub fn load(extra: Option<&Path>, dark: bool) -> Self {
        Self::load_with(extra, dark, true)
    }

    /// `load`와 같고, `default_files`가 거짓이면 기본 설정 파일(b-terminal·Ghostty 사용자 설정)을 읽지 않는다.
    /// `--config-default-files=false`(명령줄 전용). `extra`(`--config-file`)는 그래도 읽는다
    pub fn load_with(extra: Option<&Path>, dark: bool, default_files: bool) -> Self {
        let mut cfg = Config::default();
        let mut warnings = Vec::new();
        let mut sources = Vec::new();
        let own = if default_files {
            read_entries(&bterminal_config_path(), 0, &mut warnings, &mut sources)
        } else {
            Vec::new()
        };
        let extra_entries = extra
            .map(|p| read_entries(p, 0, &mut warnings, &mut sources))
            .unwrap_or_default();
        let import = own.iter().chain(&extra_entries).rfind(|e| e.key == "import-ghostty");
        // Ghostty 설정은 옮겨 오는 사람만 켠다(기본 끔, ADR-006)
        let import = import.map(|e| parse_bool(&e.value) == Some(true)).unwrap_or(false);
        let mut entries = Vec::new();
        // Ghostty 사용자 설정도 기본 설정 파일이라 `--config-default-files=false`면 읽지 않는다
        if import && default_files {
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
        cfg.entries = entries;
        cfg
    }

    /// 시스템이 어두워졌거나 밝아졌을 때 `theme = light:…,dark:…`를 다시 고른다(#221, Ghostty `changeConditionalState`).
    /// 설정 파일은 다시 읽지 않고(테마 파일은 다시 읽는다) 처음 읽은 줄(`entries`)에 새 모양을 적용해, 설정을 다시 읽은 것과 같은 색을 얻는다.
    /// 테마가 하나뿐이면 아무것도 하지 않는다. 색이 바뀌었으면 참
    pub fn repick_theme(&mut self, dark: bool) -> bool {
        if !self.theme_split {
            return false;
        }
        let mut fresh = Config::default();
        fresh.apply_all(&self.entries, dark, &mut Vec::new());
        // 테마 파일이 정하는 색만 옮긴다. 다른 설정과 경고는 그대로 둔다
        let changed = fresh.theme != self.theme
            || fresh.palette_set != self.palette_set
            || fresh.cursor_color_cell != self.cursor_color_cell
            || fresh.cursor_text_cell != self.cursor_text_cell;
        self.theme = fresh.theme;
        self.palette_set = fresh.palette_set;
        self.cursor_color_cell = fresh.cursor_color_cell;
        self.cursor_text_cell = fresh.cursor_text_cell;
        changed
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
        cfg.entries = entries;
        cfg
    }

    fn apply_all(&mut self, entries: &[Entry], dark: bool, warnings: &mut Vec<String>) {
        // 테마를 먼저 깔고, 그 위에 색 키를 덮는다(Ghostty와 같은 순서). 마지막 theme 값이 이긴다
        if let Some(theme) = entries.iter().rfind(|e| e.key == "theme") {
            if !theme.value.trim().is_empty() {
                self.theme_split = is_split_theme(&theme.value);
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
        // 키를 다 읽은 뒤에 정한다: 바탕·글자색과 palette가 어느 순서로 와도 같은 결과(#177)
        self.theme
            .build_extended(self.palette_generate, self.palette_harmonious, &self.palette_set);
        self.scrollback_lines = scrollback_rows(self.scrollback_limit_bytes, self.scrollback_limit_lines);
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
                    // 16~255번(#177): 정한 값은 xterm 표나 만든 색 위에 얹는다
                    Some((n, c)) if n < 256 => {
                        self.palette_set.retain(|(i, _)| *i as usize != n);
                        self.palette_set.push((n as u8, c));
                    }
                    Some(_) => warnings.push(format!(
                        "{}:{}: palette 번호는 0~255입니다: {v}",
                        e.file.display(),
                        e.line
                    )),
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
            "font-codepoint-map" => {
                if v.is_empty() {
                    self.font_codepoint_map.clear();
                } else if let Some(list) = parse_codepoint_map(v) {
                    self.font_codepoint_map.extend(list);
                } else {
                    bad(warnings);
                }
            }
            "window-title-font-family" => {
                self.window_title_font_family = (!v.is_empty()).then(|| v.to_string());
            }
            "font-thicken" => match parse_bool(v) {
                Some(b) => self.font_thicken = b,
                None => bad(warnings),
            },
            "font-thicken-strength" => match v.parse::<u8>() {
                Ok(n) => self.font_thicken_strength = n,
                Err(_) => bad(warnings),
            },
            "font-synthetic-style" => match SyntheticStyle::parse(v) {
                Some(x) => self.font_synthetic_style = x,
                None => bad(warnings),
            },
            "font-style" => self.font_style = FontStyleName::parse(v),
            "font-style-bold" => self.font_style_bold = FontStyleName::parse(v),
            "font-style-italic" => self.font_style_italic = FontStyleName::parse(v),
            "font-style-bold-italic" => self.font_style_bold_italic = FontStyleName::parse(v),
            "font-shaping-break" => match ShapingBreak::parse(v) {
                Some(b) => self.font_shaping_break = b,
                None => bad(warnings),
            },
            "adjust-icon-height" => set_adjust(&mut self.adjust_icon_height, v, || bad(warnings)),
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
            "adjust-font-baseline" => set_adjust(&mut self.adjust_font_baseline, v, || bad(warnings)),
            "adjust-underline-position" => set_adjust(&mut self.adjust_underline_position, v, || bad(warnings)),
            "adjust-underline-thickness" => set_adjust(&mut self.adjust_underline_thickness, v, || bad(warnings)),
            "adjust-strikethrough-position" => set_adjust(&mut self.adjust_strikethrough_position, v, || bad(warnings)),
            "adjust-strikethrough-thickness" => {
                set_adjust(&mut self.adjust_strikethrough_thickness, v, || bad(warnings))
            }
            "adjust-overline-position" => set_adjust(&mut self.adjust_overline_position, v, || bad(warnings)),
            "adjust-overline-thickness" => set_adjust(&mut self.adjust_overline_thickness, v, || bad(warnings)),
            "adjust-cursor-thickness" => set_adjust(&mut self.adjust_cursor_thickness, v, || bad(warnings)),
            "adjust-cursor-height" => set_adjust(&mut self.adjust_cursor_height, v, || bad(warnings)),
            "adjust-box-thickness" => set_adjust(&mut self.adjust_box_thickness, v, || bad(warnings)),
            "font-variation" => set_variation(&mut self.font_variation, v, || bad(warnings)),
            "font-variation-bold" => set_variation(&mut self.font_variation_bold, v, || bad(warnings)),
            "font-variation-italic" => set_variation(&mut self.font_variation_italic, v, || bad(warnings)),
            "font-variation-bold-italic" => set_variation(&mut self.font_variation_bold_italic, v, || bad(warnings)),
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
                "true" => self.fullscreen = true,
                "non-native" => {
                    self.fullscreen = true;
                    self.non_native_fullscreen = NonNativeFullscreen::True;
                }
                "non-native-visible-menu" => {
                    self.fullscreen = true;
                    self.non_native_fullscreen = NonNativeFullscreen::VisibleMenu;
                }
                "non-native-padded-notch" => {
                    self.fullscreen = true;
                    self.non_native_fullscreen = NonNativeFullscreen::PaddedNotch;
                }
                "false" => self.fullscreen = false,
                _ => bad(warnings),
            },
            "macos-non-native-fullscreen" => match v {
                "false" => self.non_native_fullscreen = NonNativeFullscreen::False,
                "true" => self.non_native_fullscreen = NonNativeFullscreen::True,
                "visible-menu" => self.non_native_fullscreen = NonNativeFullscreen::VisibleMenu,
                "padded-notch" => self.non_native_fullscreen = NonNativeFullscreen::PaddedNotch,
                _ => bad(warnings),
            },
            "window-step-resize" => match parse_bool(v) {
                Some(b) => self.window_step_resize = b,
                None => bad(warnings),
            },
            "macos-window-buttons" => match v {
                "visible" => self.macos_window_buttons = MacWindowButtons::Visible,
                "hidden" => self.macos_window_buttons = MacWindowButtons::Hidden,
                _ => bad(warnings),
            },
            "window-decoration" => match v {
                "auto" | "true" => self.window_decoration = WindowDecoration::Auto,
                "client" => self.window_decoration = WindowDecoration::Client,
                "server" => self.window_decoration = WindowDecoration::Server,
                "none" | "false" => self.window_decoration = WindowDecoration::None,
                _ => bad(warnings),
            },
            "macos-titlebar-style" => match v {
                "native" => self.macos_titlebar_style = MacTitlebarStyle::Native,
                "transparent" => self.macos_titlebar_style = MacTitlebarStyle::Transparent,
                "tabs" => self.macos_titlebar_style = MacTitlebarStyle::Tabs,
                "hidden" => self.macos_titlebar_style = MacTitlebarStyle::Hidden,
                _ => bad(warnings),
            },
            "macos-window-shadow" => match parse_bool(v) {
                Some(b) => self.macos_window_shadow = b,
                None => bad(warnings),
            },
            "window-theme" => match v {
                "auto" => self.window_theme = WindowTheme::Auto,
                "system" => self.window_theme = WindowTheme::System,
                "light" => self.window_theme = WindowTheme::Light,
                "dark" => self.window_theme = WindowTheme::Dark,
                "ghostty" => self.window_theme = WindowTheme::Ghostty,
                _ => bad(warnings),
            },
            "macos-titlebar-proxy-icon" => match v {
                "visible" => self.macos_titlebar_proxy_icon = MacTitlebarProxyIcon::Visible,
                "hidden" => self.macos_titlebar_proxy_icon = MacTitlebarProxyIcon::Hidden,
                _ => bad(warnings),
            },
            // 옛 값 `window`는 Ghostty처럼 new-window로 읽는다
            "macos-dock-drop-behavior" => match v {
                "new-tab" => self.macos_dock_drop_behavior = MacDockDropBehavior::NewTab,
                "new-window" | "window" => self.macos_dock_drop_behavior = MacDockDropBehavior::NewWindow,
                _ => bad(warnings),
            },
            "macos-hidden" => match v {
                "never" => self.macos_hidden = MacHidden::Never,
                "always" => self.macos_hidden = MacHidden::Always,
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
            // Ghostty 1.4: 바이트 상한(옛 이름 scrollback-limit도 같은 뜻)과 줄 상한. 먼저 닿는 쪽이 이긴다(#177)
            "scrollback-limit-bytes" | "scrollback-limit" => {
                match parse_limit::<u64>(v, Some(SCROLLBACK_BYTES_DEFAULT)) {
                    Some(b) => self.scrollback_limit_bytes = b,
                    None => bad(warnings),
                }
            }
            "scrollback-limit-lines" => match parse_limit::<usize>(v, None) {
                Some(n) => self.scrollback_limit_lines = n,
                None => bad(warnings),
            },
            // b-terminal 옛 키: 정확히 그 줄 수(바이트 상한은 푼다)
            "scrollback-lines" => match v.parse::<usize>() {
                Ok(n) => {
                    self.scrollback_limit_lines = Some(n);
                    self.scrollback_limit_bytes = None;
                }
                Err(_) => bad(warnings),
            },
            "palette-generate" => match parse_bool(v) {
                Some(b) => self.palette_generate = b,
                None => bad(warnings),
            },
            "palette-harmonious" => match parse_bool(v) {
                Some(b) => self.palette_harmonious = b,
                None => bad(warnings),
            },
            "link-url" => match parse_bool(v) {
                Some(b) => self.link_url = b,
                None => bad(warnings),
            },
            "link-osc8" => match parse_bool(v) {
                Some(b) => self.link_osc8 = b,
                None => bad(warnings),
            },
            "link-previews" => match v {
                "true" => self.link_previews = LinkPreviews::True,
                "false" => self.link_previews = LinkPreviews::False,
                "osc8" => self.link_previews = LinkPreviews::Osc8,
                _ => bad(warnings),
            },
            // Ghostty도 아직 이 키를 읽지 못한다(소스 주석: "TODO: This can't currently be set!").
            // 정규식 링크는 만들지 않는다. 받은 줄은 알리고 넘긴다
            "link" => warnings.push(format!(
                "{}:{}: link는 Ghostty에서도 아직 쓸 수 없는 키라 무시합니다(정규식 링크 없음)",
                e.file.display(),
                e.line
            )),
            "background-opacity-cells" => match parse_bool(v) {
                Some(b) => self.background_opacity_cells = b,
                None => bad(warnings),
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
            "mouse-reporting" => match parse_bool(v) {
                Some(b) => self.mouse_reporting = b,
                None => bad(warnings),
            },
            "mouse-shift-capture" => match v {
                "true" => self.mouse_shift_capture = MouseShiftCapture::True,
                "false" => self.mouse_shift_capture = MouseShiftCapture::False,
                "always" => self.mouse_shift_capture = MouseShiftCapture::Always,
                "never" => self.mouse_shift_capture = MouseShiftCapture::Never,
                _ => bad(warnings),
            },
            "middle-click-action" => match v {
                "primary-paste" => self.middle_click_action = MiddleClickAction::PrimaryPaste,
                "clipboard-paste" => self.middle_click_action = MiddleClickAction::ClipboardPaste,
                "ignore" => self.middle_click_action = MiddleClickAction::Ignore,
                _ => bad(warnings),
            },
            "click-repeat-interval" => match v.parse::<u32>() {
                Ok(n) => self.click_repeat_interval = n,
                Err(_) => bad(warnings),
            },
            "window-vsync" => match parse_bool(v) {
                Some(b) => self.window_vsync = b,
                None => bad(warnings),
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
            "initial-command" => self.initial_command = Some(v.to_string()).filter(|s| !s.is_empty()),
            "wait-after-command" => match parse_bool(v) {
                Some(b) => self.wait_after_command = b,
                None => bad(warnings),
            },
            "abnormal-command-exit-runtime" => match v.parse::<u32>() {
                Ok(n) => self.abnormal_command_exit_runtime = n,
                Err(_) => bad(warnings),
            },
            "initial-window" => match parse_bool(v) {
                Some(b) => self.initial_window = b,
                None => bad(warnings),
            },
            "quit-after-last-window-closed" => match parse_bool(v) {
                Some(b) => self.quit_after_last_window_closed = b,
                None => bad(warnings),
            },
            "quit-after-last-window-closed-delay" if v.is_empty() => self.quit_after_last_window_closed_delay = None,
            "quit-after-last-window-closed-delay" => match parse_duration(v) {
                Some(d) => self.quit_after_last_window_closed_delay = Some(d),
                None => bad(warnings),
            },
            "working-directory" => {
                self.working_directory = match v {
                    "" | "inherit" => WorkingDirectory::Inherit,
                    "home" => WorkingDirectory::Home,
                    p => WorkingDirectory::Path(expand_home(p, e.file.parent().unwrap_or(Path::new(".")))),
                }
            }
            "title" => self.title = Some(v.to_string()).filter(|s| !s.is_empty()),
            "keybind" => self.keybinds.push(v.to_string()),
            // 빈 값은 지금까지 쌓은 것을 지운다(반복하는 다른 키와 같다)
            "key-remap" if v.is_empty() => self.key_remap.clear(),
            "key-remap" => match KeyRemap::parse(v) {
                Some(r) => self.key_remap.push(r),
                None => bad(warnings),
            },
            "macos-icon" => match MacosIcon::parse(v) {
                Some(i) => self.macos_icon = i,
                None => bad(warnings),
            },
            // 상대 경로는 그 설정 파일의 폴더 기준(bell-audio-path와 같게). 빈 값은 지움
            "macos-custom-icon" => {
                self.macos_custom_icon =
                    (!v.is_empty()).then(|| expand_home(v, e.file.parent().unwrap_or(Path::new("."))));
            }
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
            "shell-integration-features" => {
                if !self.shell_integration_features.apply(v) {
                    bad(warnings)
                }
            }
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
            "progress-style" => match parse_bool(v) {
                Some(b) => self.progress_style = b,
                None => bad(warnings),
            },
            "scrollbar" => match v {
                "system" => self.scrollbar = Scrollbar::System,
                "never" => self.scrollbar = Scrollbar::Never,
                _ => bad(warnings),
            },
            "split-preserve-zoom" => {
                if !self.split_preserve_zoom.apply(v) {
                    bad(warnings)
                }
            }
            "app-notifications" => {
                if !self.app_notifications.apply(v) {
                    bad(warnings)
                }
            }
            "clipboard-write" => match crate::clipboard::ClipboardAccess::parse(v) {
                Some(a) => self.clipboard_write = a,
                None => bad(warnings),
            },
            "clipboard-read" => match crate::clipboard::ClipboardAccess::parse(v) {
                Some(a) => self.clipboard_read = a,
                None => bad(warnings),
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
            "enquiry-response" => self.enquiry_response = v.to_string(),
            "term" => {
                if v.is_empty() {
                    self.term = DEFAULT_TERM.to_string();
                } else if valid_env_value(v) {
                    self.term = v.to_string();
                } else {
                    bad(warnings)
                }
            }
            "env" => {
                if let Err(why) = apply_env(&mut self.env, v) {
                    warnings.push(format!("{}:{}: env {why}", e.file.display(), e.line));
                }
            }
            "input" => {
                if v.is_empty() {
                    self.input.clear();
                } else {
                    match parse_input(v, e.file.parent().unwrap_or(Path::new("."))) {
                        Ok(src) => self.input.push(src),
                        Err(why) => warnings.push(format!("{}:{}: input {why}", e.file.display(), e.line)),
                    }
                }
            }
            "custom-shader" => {
                if v.is_empty() {
                    self.custom_shader.clear();
                } else {
                    self.custom_shader
                        .push(parse_shader_path(v, e.file.parent().unwrap_or(Path::new("."))));
                }
            }
            "custom-shader-animation" => match CustomShaderAnimation::parse(v) {
                Some(a) => self.custom_shader_animation = a,
                None => bad(warnings),
            },
            "vt-kam-allowed" => match parse_bool(v) {
                Some(b) => self.vt_kam_allowed = b,
                None => bad(warnings),
            },
            "vt-window-resize-allowed" => match parse_bool(v) {
                Some(b) => self.vt_window_resize_allowed = b,
                None => bad(warnings),
            },
            "image-storage-limit" => match v.parse::<u32>() {
                Ok(n) => self.image_storage_limit = n,
                Err(_) => bad(warnings),
            },
            "clipboard-write-limit-bytes" => match v {
                "unlimited" => self.clipboard_write_limit_bytes = None,
                _ => match v.parse::<usize>() {
                    Ok(n) => self.clipboard_write_limit_bytes = Some(n),
                    Err(_) => bad(warnings),
                },
            },
            "clipboard-codepoint-map" => {
                if v.is_empty() {
                    self.clipboard_codepoint_map.clear();
                } else {
                    match parse_codepoint_rules(v) {
                        Some(r) => self.clipboard_codepoint_map.extend(r),
                        None => bad(warnings),
                    }
                }
            }
            "osc-color-report-format" => match v {
                "none" => self.osc_color_report_format = OscColorReportFormat::None,
                "8-bit" => self.osc_color_report_format = OscColorReportFormat::Bit8,
                "16-bit" => self.osc_color_report_format = OscColorReportFormat::Bit16,
                _ => bad(warnings),
            },
            "grapheme-width-method" => match v {
                "legacy" => self.grapheme_width_method = GraphemeWidthMethod::Legacy,
                "unicode" => self.grapheme_width_method = GraphemeWidthMethod::Unicode,
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
            "undo-timeout" => match parse_duration(v) {
                Some(d) => self.undo_timeout = d,
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
            "vt-xt-checksum-report" => match parse_bool(v) {
                Some(b) => self.vt_xt_checksum_report = b,
                None => bad(warnings),
            },
            "vt-xt-checksum-extension" => {
                if !self.vt_xt_checksum_extension.apply(v) {
                    bad(warnings)
                }
            }
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
            "quick-terminal-space-behavior" => match v {
                "move" => self.quick.space = QuickSpace::Move,
                "remain" => self.quick.space = QuickSpace::Remain,
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
            // `background-blur-radius`는 Ghostty 1.1이 `background-blur`로 이름을 바꾼 옛 이름(#238)
            "background-blur" | "background-blur-radius" => match parse_background_blur(v) {
                Some(r) => self.background_blur = r,
                None => bad(warnings),
            },
            "background-opacity" => match v.parse::<f32>() {
                Ok(o) if o.is_finite() => self.background_opacity = o.clamp(0.0, 1.0),
                _ => bad(warnings),
            },
            "alpha-blending" => match v {
                "native" => self.alpha_blending = AlphaBlending::Native,
                "linear" => self.alpha_blending = AlphaBlending::Linear,
                "linear-corrected" => self.alpha_blending = AlphaBlending::LinearCorrected,
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
            "macos-auto-secure-input" => match parse_bool(v) {
                Some(b) => self.macos_auto_secure_input = b,
                None => bad(warnings),
            },
            "macos-secure-input-indication" => match parse_bool(v) {
                Some(b) => self.macos_secure_input_indication = b,
                None => bad(warnings),
            },
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
        if let Some(f) = &self.window_title_font_family {
            push("window-title-font-family", f.clone());
        }
        if self.font_thicken {
            push("font-thicken", "true".into());
        }
        if self.font_thicken_strength != 255 {
            push("font-thicken-strength", self.font_thicken_strength.to_string());
        }
        if self.font_synthetic_style != SyntheticStyle::default() {
            push("font-synthetic-style", self.font_synthetic_style.render());
        }
        push("font-size", format!("{}", self.font_size));
        // Ghostty는 `theme`도 한 키로 찍는다(테마가 정한 색은 색 키로 따로 나온다). 마지막 비지 않은 값이 이긴다
        if let Some(theme) = self.entries.iter().rfind(|e| e.key == "theme") {
            if !theme.value.trim().is_empty() {
                push("theme", theme.value.trim().to_string());
            }
        }
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
        for (i, c) in &self.palette_set {
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
        let limit = |v: Option<String>| v.unwrap_or_else(|| "unlimited".into());
        push(
            "scrollback-limit-bytes",
            limit(self.scrollback_limit_bytes.map(|b| b.to_string())),
        );
        push(
            "scrollback-limit-lines",
            limit(self.scrollback_limit_lines.map(|n| n.to_string())),
        );
        push("palette-generate", self.palette_generate.to_string());
        push("palette-harmonious", self.palette_harmonious.to_string());
        push("background-opacity-cells", self.background_opacity_cells.to_string());
        push(
            "alpha-blending",
            match self.alpha_blending {
                AlphaBlending::Native => "native",
                AlphaBlending::Linear => "linear",
                AlphaBlending::LinearCorrected => "linear-corrected",
            }
            .to_string(),
        );
        push("link-url", self.link_url.to_string());
        push("link-osc8", self.link_osc8.to_string());
        push(
            "link-previews",
            match self.link_previews {
                LinkPreviews::True => "true",
                LinkPreviews::False => "false",
                LinkPreviews::Osc8 => "osc8",
            }
            .to_string(),
        );
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
        push("mouse-reporting", self.mouse_reporting.to_string());
        push(
            "mouse-shift-capture",
            match self.mouse_shift_capture {
                MouseShiftCapture::False => "false",
                MouseShiftCapture::True => "true",
                MouseShiftCapture::Always => "always",
                MouseShiftCapture::Never => "never",
            }
            .into(),
        );
        push(
            "middle-click-action",
            match self.middle_click_action {
                MiddleClickAction::PrimaryPaste => "primary-paste",
                MiddleClickAction::ClipboardPaste => "clipboard-paste",
                MiddleClickAction::Ignore => "ignore",
            }
            .into(),
        );
        push("click-repeat-interval", self.click_repeat_interval.to_string());
        push("window-vsync", self.window_vsync.to_string());
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
        if let Some(c) = &self.initial_command {
            push("initial-command", c.clone());
        }
        push("wait-after-command", self.wait_after_command.to_string());
        push(
            "abnormal-command-exit-runtime",
            self.abnormal_command_exit_runtime.to_string(),
        );
        push("initial-window", self.initial_window.to_string());
        push(
            "quit-after-last-window-closed",
            self.quit_after_last_window_closed.to_string(),
        );
        if let Some(d) = self.quit_after_last_window_closed_delay {
            let ms = d.as_millis();
            push(
                "quit-after-last-window-closed-delay",
                if ms % 1000 == 0 {
                    format!("{}s", ms / 1000)
                } else {
                    format!("{ms}ms")
                },
            );
        }
        push("macos-auto-secure-input", self.macos_auto_secure_input.to_string());
        push(
            "macos-secure-input-indication",
            self.macos_secure_input_indication.to_string(),
        );
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
        let sf = self.shell_integration_features.to_list();
        push(
            "shell-integration-features",
            if sf.is_empty() { "false".into() } else { sf },
        );
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
            if ms.is_multiple_of(1000) {
                format!("{}s", ms / 1000)
            } else {
                format!("{ms}ms")
            }
        });
        push("desktop-notifications", self.desktop_notifications.to_string());
        push("progress-style", self.progress_style.to_string());
        push(
            "scrollbar",
            match self.scrollbar {
                Scrollbar::System => "system",
                Scrollbar::Never => "never",
            }
            .into(),
        );
        push(
            "split-preserve-zoom",
            if self.split_preserve_zoom.navigation {
                "navigation"
            } else {
                "no-navigation"
            }
            .into(),
        );
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
        push("term", self.term.clone());
        for (k, v) in &self.env {
            push("env", format!("{k}={v}"));
        }
        for src in &self.input {
            push("input", src.render());
        }
        for sh in &self.custom_shader {
            push(
                "custom-shader",
                format!("{}{}", if sh.optional { "?" } else { "" }, sh.path.display()),
            );
        }
        push("custom-shader-animation", self.custom_shader_animation.render().into());
        push(
            "window-decoration",
            match self.window_decoration {
                WindowDecoration::Auto => "auto",
                WindowDecoration::Client => "client",
                WindowDecoration::Server => "server",
                WindowDecoration::None => "none",
            }
            .into(),
        );
        push(
            "macos-titlebar-style",
            match self.macos_titlebar_style {
                MacTitlebarStyle::Native => "native",
                MacTitlebarStyle::Transparent => "transparent",
                MacTitlebarStyle::Tabs => "tabs",
                MacTitlebarStyle::Hidden => "hidden",
            }
            .into(),
        );
        push("macos-window-shadow", self.macos_window_shadow.to_string());
        push(
            "window-theme",
            match self.window_theme {
                WindowTheme::Auto => "auto",
                WindowTheme::System => "system",
                WindowTheme::Light => "light",
                WindowTheme::Dark => "dark",
                WindowTheme::Ghostty => "ghostty",
            }
            .into(),
        );
        push(
            "macos-titlebar-proxy-icon",
            match self.macos_titlebar_proxy_icon {
                MacTitlebarProxyIcon::Visible => "visible",
                MacTitlebarProxyIcon::Hidden => "hidden",
            }
            .into(),
        );
        push(
            "macos-dock-drop-behavior",
            match self.macos_dock_drop_behavior {
                MacDockDropBehavior::NewTab => "new-tab",
                MacDockDropBehavior::NewWindow => "new-window",
            }
            .into(),
        );
        for r in &self.key_remap {
            push("key-remap", r.to_string());
        }
        push("macos-icon", self.macos_icon.to_string());
        if let Some(p) = &self.macos_custom_icon {
            push("macos-custom-icon", p.display().to_string());
        }
        push(
            "quick-terminal-space-behavior",
            match self.quick.space {
                QuickSpace::Move => "move",
                QuickSpace::Remain => "remain",
            }
            .into(),
        );
        push("vt-xt-checksum-report", self.vt_xt_checksum_report.to_string());
        push("vt-xt-checksum-extension", self.vt_xt_checksum_extension.to_list());
        out.join("\n")
    }
}

/// Ghostty에 있지만 b-terminal이 일부러 쓰지 않는 키(경고하지 않는다). 화면 모양·창 장식처럼 지금 그릴 수 없는 것
const KNOWN_IGNORED: &[&str] = &[
    // 명령줄 전용(`--config-default-files`). 파일 안에서는 Ghostty처럼 오류도 경고도 없이 아무 일도 하지 않는다(#211)
    "config-default-files",
    // GTK에서만 뜻이 있는 Ghostty 키(받기만 한다, #179)
    "window-subtitle",
    "window-titlebar-background",
    "window-titlebar-foreground",
    "auto-update",
    "auto-update-channel",
    "gtk-titlebar",
    "gtk-single-instance",
    // 받기만 하는 키(#196). 이유는 docs/configuration.md "받기만 하고 쓰지 않는 Ghostty 키"
    // 이름 그림이 없어 custom만 그린다. custom-style의 그림 설정도 쓸 곳이 없다
    "macos-icon-frame",
    "macos-icon-ghost-color",
    "macos-icon-screen-color",
    // GTK·리눅스 전용
    "class",
    "x11-instance-name",
    "language",
    "gtk-custom-css",
    "gtk-horizontal-tab-scroll",
    "gtk-opengl-debug",
    "gtk-quick-terminal-layer",
    "gtk-quick-terminal-namespace",
    "gtk-tabs-inverted",
    "gtk-tabs-location",
    "gtk-titlebar-hide-when-maximized",
    "gtk-titlebar-style",
    "gtk-toolbar-style",
    "gtk-wide-tabs",
    "linux-cgroup",
    "linux-cgroup-memory-limit",
    "linux-cgroup-processes-limit",
    "linux-cgroup-hard-fail",
    "async-backend",
    "quick-terminal-keyboard-interactivity",
    // 그리기·엔진 쪽(b-terminal의 그리기와 VT 엔진이 다르다)
    "freetype-load-flags",
    "scrollback-compression",
    "drag-handle",
    // unsafe나 시스템 연동(AppleScript·단축어·보안 입력)이 필요하다
    "macos-applescript",
    "macos-shortcuts",
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

/// `light:`와 `dark:`가 둘 다 있는 테마 값인가(시스템 모양을 따라 바뀐다)
fn is_split_theme(v: &str) -> bool {
    let has = |p: &str| v.split(',').any(|part| part.trim().starts_with(p));
    has("light:") && has("dark:")
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
    #[test]
    fn config_default_files_false_reads_only_the_given_file() {
        let dir = std::env::temp_dir().join(format!("bt-cdf-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("only");
        // 파일 안의 config-default-files는 아무 일도 하지 않고 경고도 없다(Ghostty)
        std::fs::write(&f, "font-size = 21\nconfig-default-files = false\n").unwrap();
        let c = Config::load_with(Some(&f), true, false);
        assert_eq!(c.sources, vec![f.clone()], "기본 설정 파일은 읽지 않는다");
        assert_eq!(c.font_size, 21.0);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        let _ = std::fs::remove_dir_all(&dir);
        // 파일 없이도 기본값만으로 만들어진다
        let none = Config::load_with(None, true, false);
        assert!(none.sources.is_empty());
    }

    use super::*;

    #[test]
    fn scrollbar_shows_by_mode_history_and_activity() {
        // 스크롤백이 없으면 보일 것이 없다
        assert!(!Scrollbar::System.shows(0, true, true));
        // 스크롤했거나 마우스가 끝에 있으면 보인다
        assert!(Scrollbar::System.shows(10, true, false));
        assert!(Scrollbar::System.shows(10, false, true));
        // 가만히 있으면 숨는다
        assert!(!Scrollbar::System.shows(10, false, false));
        // never는 언제나 그리지 않는다
        assert!(!Scrollbar::Never.shows(10, true, true));
    }

    #[test]
    fn split_preserve_zoom_keeps_only_when_navigation_and_zoomed() {
        let on = SplitPreserveZoom { navigation: true };
        assert!(on.keeps_zoom_on_goto(true));
        assert!(!on.keeps_zoom_on_goto(false));
        assert!(!SplitPreserveZoom::default().keeps_zoom_on_goto(true));
    }

    #[test]
    fn scrollbar_progress_and_zoom_keys() {
        let d = Config::default();
        assert!(d.progress_style);
        assert_eq!(d.scrollbar, Scrollbar::System);
        assert!(!d.split_preserve_zoom.navigation);
        let c = Config::from_text(
            "progress-style = false\nscrollbar = never\nsplit-preserve-zoom = navigation\n",
            true,
        );
        assert!(!c.progress_style);
        assert_eq!(c.scrollbar, Scrollbar::Never);
        assert!(c.split_preserve_zoom.navigation);
        let r = c.render();
        assert!(r.contains("progress-style = false"));
        assert!(r.contains("scrollbar = never"));
        assert!(r.contains("split-preserve-zoom = navigation"));
        let c = Config::from_text(
            "progress-style = x\nscrollbar = always\nsplit-preserve-zoom = foo\n",
            true,
        );
        assert!(c.progress_style && c.scrollbar == Scrollbar::System && !c.split_preserve_zoom.navigation);
        assert_eq!(c.warnings.len(), 3);
        let mut z = SplitPreserveZoom::default();
        assert!(z.apply("navigation"));
        assert!(z.navigation);
        assert!(z.apply("no-navigation"));
        assert!(!z.navigation);
        assert!(!z.apply("navigation,bogus"));
        assert!(!z.navigation, "모르는 이름이 섞이면 아무것도 바꾸지 않는다");
    }

    #[test]
    fn window_vsync_key() {
        assert!(Config::default().window_vsync, "기본은 화면 갱신에 맞춘다");
        let off = Config::from_text("window-vsync = false\n", true);
        assert!(off.warnings.is_empty(), "{:?}", off.warnings);
        assert!(!off.window_vsync);
        let on = Config::from_text("window-vsync = false\nwindow-vsync = true\n", true);
        assert!(on.window_vsync && on.warnings.is_empty());
        let bad = Config::from_text("window-vsync = maybe\n", true);
        assert!(bad.window_vsync, "잘못된 값은 기본으로 둔다");
        assert_eq!(bad.warnings.len(), 1);
        assert!(off.render().contains("window-vsync = false"));
    }

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
    fn mouse_keys() {
        let d = Config::default();
        assert!(d.mouse_reporting);
        assert_eq!(d.mouse_shift_capture, MouseShiftCapture::False);
        assert_eq!(d.middle_click_action, MiddleClickAction::PrimaryPaste);
        assert_eq!(d.click_repeat_interval, 0);
        let c = Config::from_text(
            "mouse-reporting = false\nmouse-shift-capture = always\nmiddle-click-action = clipboard-paste\nclick-repeat-interval = 300\n",
            true,
        );
        assert!(!c.mouse_reporting);
        assert_eq!(c.mouse_shift_capture, MouseShiftCapture::Always);
        assert_eq!(c.middle_click_action, MiddleClickAction::ClipboardPaste);
        assert_eq!(c.click_repeat_interval, 300);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        for (v, want) in [
            ("true", MouseShiftCapture::True),
            ("false", MouseShiftCapture::False),
            ("always", MouseShiftCapture::Always),
            ("never", MouseShiftCapture::Never),
        ] {
            assert_eq!(
                Config::from_text(&format!("mouse-shift-capture = {v}\n"), true).mouse_shift_capture,
                want
            );
        }
        assert_eq!(
            Config::from_text("middle-click-action = ignore\n", true).middle_click_action,
            MiddleClickAction::Ignore
        );
        assert!(MouseShiftCapture::True.sends_shift() && MouseShiftCapture::Always.sends_shift());
        assert!(!MouseShiftCapture::False.sends_shift() && !MouseShiftCapture::Never.sends_shift());
        // XTSHIFTESCAPE(#327): always·never는 프로그램이 못 바꾸고, true·false는 프로그램이 정했으면 그 값, 아니면 설정 값
        for program in [None, Some(false), Some(true)] {
            assert!(MouseShiftCapture::Always.allows(program));
            assert!(!MouseShiftCapture::Never.allows(program));
        }
        assert!(!MouseShiftCapture::False.allows(None));
        assert!(!MouseShiftCapture::False.allows(Some(false)));
        assert!(MouseShiftCapture::False.allows(Some(true)));
        assert!(MouseShiftCapture::True.allows(None));
        assert!(MouseShiftCapture::True.allows(Some(true)));
        assert!(!MouseShiftCapture::True.allows(Some(false)));
        for bad in [
            "mouse-reporting = maybe",
            "mouse-shift-capture = sometimes",
            "middle-click-action = menu",
            "click-repeat-interval = -1",
        ] {
            assert_eq!(Config::from_text(&format!("{bad}\n"), true).warnings.len(), 1, "{bad}");
        }
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

    /// #259: DECRQCRA 체크섬 보고는 Ghostty처럼 기본이 꺼짐이고, 계산 방식은 DEC 단말의 것이다
    #[test]
    fn xt_checksum_keys_like_ghostty() {
        let d = Config::default();
        assert!(!d.vt_xt_checksum_report);
        assert_eq!(d.vt_xt_checksum_extension, XtChecksumExtension::default());
        assert_eq!(
            d.vt_xt_checksum_extension.to_list(),
            "negate,attributes,trim,no-undrawn,no-full"
        );
        let c = Config::from_text(
            "vt-xt-checksum-report = true\nvt-xt-checksum-extension = no-negate,no-attributes,no-trim,full\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert!(c.vt_xt_checksum_report);
        assert_eq!(
            c.vt_xt_checksum_extension,
            XtChecksumExtension {
                negate: false,
                attributes: false,
                trim: false,
                undrawn: false,
                full: true
            }
        );
        let out = c.render();
        assert!(out.contains("vt-xt-checksum-report = true"), "{out}");
        assert!(
            out.contains("vt-xt-checksum-extension = no-negate,no-attributes,no-trim,no-undrawn,full"),
            "{out}"
        );
        // 적지 않은 것은 그대로 두고, 뒤 줄이 앞 줄 위에 얹힌다
        let c = Config::from_text(
            "vt-xt-checksum-extension = undrawn\nvt-xt-checksum-extension = no-trim\n",
            true,
        );
        assert_eq!(
            c.vt_xt_checksum_extension,
            XtChecksumExtension {
                undrawn: true,
                trim: false,
                ..XtChecksumExtension::default()
            }
        );
        assert!(
            Config::from_text("vt-xt-checksum-extension = false\n", true).vt_xt_checksum_extension
                == XtChecksumExtension {
                    negate: false,
                    attributes: false,
                    trim: false,
                    undrawn: false,
                    full: false
                }
        );
        // 모르는 이름이 섞이면 줄 전체를 버리고 경고, 나쁜 불리언도 경고
        let c = Config::from_text(
            "vt-xt-checksum-extension = no-negate,bogus\nvt-xt-checksum-report = maybe\n",
            true,
        );
        assert_eq!(c.vt_xt_checksum_extension, XtChecksumExtension::default());
        assert!(!c.vt_xt_checksum_report);
        assert_eq!(c.warnings.len(), 2, "{:?}", c.warnings);
    }

    /// `macos-dock-drop-behavior`(#279): new-tab·new-window와 Ghostty의 옛 값 window. 틀린 값은 경고하고 기본을 지킨다
    #[test]
    fn dock_drop_behavior_parses() {
        for (v, want) in [
            ("new-tab", MacDockDropBehavior::NewTab),
            ("new-window", MacDockDropBehavior::NewWindow),
            ("window", MacDockDropBehavior::NewWindow),
        ] {
            let c = Config::from_text(&format!("macos-dock-drop-behavior = {v}\n"), true);
            assert_eq!((c.macos_dock_drop_behavior, c.warnings.len()), (want, 0), "{v}");
        }
        let c = Config::from_text("macos-dock-drop-behavior = tab\n", true);
        assert_eq!(
            (c.macos_dock_drop_behavior, c.warnings.len()),
            (MacDockDropBehavior::NewTab, 1)
        );
        let r = Config::from_text("macos-dock-drop-behavior = new-window\n", true).render();
        assert!(r.contains("macos-dock-drop-behavior = new-window"), "{r}");
        assert!(
            Config::default()
                .render()
                .contains("macos-dock-drop-behavior = new-tab")
        );
    }

    #[test]
    fn bad_values_warn_and_keep_defaults() {
        let c = Config::from_text("font-size = huge\ncursor-style = triangle\npalette = x\n", true);
        assert_eq!(c.font_size, 13.0);
        assert_eq!(c.cursor_style, CursorStyle::Block);
        assert_eq!(c.warnings.len(), 3);
    }

    /// #163: `adjust-*` 키는 `10%`·픽셀을 받고, 읽지 못하면 경고하고 안 정한 채로 둔다. 빈 값은 지운다
    #[test]
    fn font_adjust_keys() {
        let c = Config::from_text(
            "adjust-font-baseline = 2\nadjust-underline-position = -1\nadjust-underline-thickness = 50%\n\
             adjust-strikethrough-position = 3\nadjust-strikethrough-thickness = 1\n\
             adjust-overline-position = 4\nadjust-overline-thickness = 200%\n\
             adjust-cursor-thickness = 2\nadjust-cursor-height = -10%\nadjust-box-thickness = 1\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.adjust_font_baseline, Some(Adjust::Points(2.0)));
        assert_eq!(c.adjust_underline_position, Some(Adjust::Points(-1.0)));
        assert_eq!(c.adjust_underline_thickness, Some(Adjust::Percent(50.0)));
        assert_eq!(c.adjust_strikethrough_position, Some(Adjust::Points(3.0)));
        assert_eq!(c.adjust_strikethrough_thickness, Some(Adjust::Points(1.0)));
        assert_eq!(c.adjust_overline_position, Some(Adjust::Points(4.0)));
        assert_eq!(c.adjust_overline_thickness, Some(Adjust::Percent(200.0)));
        assert_eq!(c.adjust_cursor_thickness, Some(Adjust::Points(2.0)));
        assert_eq!(c.adjust_cursor_height, Some(Adjust::Percent(-10.0)));
        assert_eq!(c.adjust_box_thickness, Some(Adjust::Points(1.0)));

        let c = Config::from_text(
            "adjust-cursor-height = tall\nadjust-box-thickness = 2\nadjust-box-thickness =\n",
            true,
        );
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
        assert_eq!(c.adjust_cursor_height, None);
        assert_eq!(c.adjust_box_thickness, None, "빈 값은 지움");
    }

    /// #163: `font-variation*`은 `축=값`(축은 네 글자)을 쌓고, 같은 축을 다시 쓰면 뒤가 이기며, 빈 값은 비운다
    #[test]
    fn font_variation_keys() {
        let c = Config::from_text(
            "font-variation = wght=500\nfont-variation = slnt = -15\nfont-variation = wght=520.5\n\
             font-variation-bold = wght=700\nfont-variation-italic = ital=1\nfont-variation-bold-italic = wdth=90\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.font_variation, vec![(*b"slnt", -15.0), (*b"wght", 520.5)]);
        assert_eq!(c.font_variation_bold, vec![(*b"wght", 700.0)]);
        assert_eq!(c.font_variation_italic, vec![(*b"ital", 1.0)]);
        assert_eq!(c.font_variation_bold_italic, vec![(*b"wdth", 90.0)]);

        let c = Config::from_text(
            "font-variation = weight=500\nfont-variation = wght\nfont-variation = wght=heavy\nfont-variation = wght=inf\n\
             font-variation = wght=300\nfont-variation =\n",
            true,
        );
        assert_eq!(c.warnings.len(), 4, "{:?}", c.warnings);
        assert!(c.font_variation.is_empty(), "빈 값은 비움");
    }

    /// #174: `font-codepoint-map`은 `U+범위[,U+범위…]=글꼴`. 여러 번 쓰면 쌓이고, 잘못된 줄은 경고하고 건너뛴다
    #[test]
    fn font_codepoint_map_key() {
        let cf = |start, end, family: &str| CodepointFont {
            start,
            end,
            family: family.into(),
        };
        let c = Config::from_text(
            "font-codepoint-map = U+3000-U+9FFF=Noto Sans CJK KR\n\
             font-codepoint-map = U+E0A0,U+E0B0 - U+E0B3 = Symbols Nerd Font Mono\n\
             font-codepoint-map = u+41-U+5a=Menlo\n\
             font-codepoint-map = U+61-U+7a=Menlo\n",
            true,
        );
        // 소문자 u는 Ghostty도 받지 않는다
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
        assert_eq!(
            c.font_codepoint_map,
            vec![
                cf(0x3000, 0x9FFF, "Noto Sans CJK KR"),
                cf(0xE0A0, 0xE0A0, "Symbols Nerd Font Mono"),
                cf(0xE0B0, 0xE0B3, "Symbols Nerd Font Mono"),
                cf(0x61, 0x7a, "Menlo"),
            ]
        );
        // 읽지 못하는 꼴: = 없음, U+ 없음, 거꾸로 된 범위, 끝에 남은 쉼표, 16진수 아님
        for bad in [
            "U+3000",
            "3000=X",
            "U+9-U+1=X",
            "U+41,=X",
            "U+4G=X",
            "U+110000000=X",
            "=X",
        ] {
            assert_eq!(parse_codepoint_map(bad), None, "{bad}");
        }
        // 빈 값은 지움
        let c = Config::from_text("font-codepoint-map = U+41=Menlo\nfont-codepoint-map =\n", true);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert!(c.font_codepoint_map.is_empty());
    }

    /// #174: `font-style*`은 `default`·`false`·스타일 이름
    #[test]
    fn font_style_keys() {
        let d = Config::default();
        assert_eq!(d.font_style, FontStyleName::Default);
        assert_eq!(d.font_style_bold, FontStyleName::Default);
        let c = Config::from_text(
            "font-style = Light\nfont-style-bold = false\nfont-style-italic = Heavy Italic\n\
             font-style-bold-italic = default\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.font_style, FontStyleName::Named("Light".into()));
        assert_eq!(c.font_style_bold, FontStyleName::Disabled);
        assert_eq!(c.font_style_italic, FontStyleName::Named("Heavy Italic".into()));
        assert_eq!(c.font_style_bold_italic, FontStyleName::Default);
        // 빈 값은 default로 되돌린다
        let c = Config::from_text("font-style-bold = false\nfont-style-bold =\n", true);
        assert_eq!(c.font_style_bold, FontStyleName::Default);
    }

    /// #174: `font-shaping-break`는 기본 `cursor` 켬. `no-cursor`·`false`로 끄고, 모르는 이름은 경고하고 그대로
    #[test]
    fn font_shaping_break_key() {
        assert!(Config::default().font_shaping_break.cursor);
        for (text, want) in [
            ("no-cursor", false),
            ("false", false),
            ("cursor", true),
            ("true", true),
            ("no-cursor,cursor", true),
        ] {
            let c = Config::from_text(&format!("font-shaping-break = {text}\n"), true);
            assert!(c.warnings.is_empty(), "{text}: {:?}", c.warnings);
            assert_eq!(c.font_shaping_break.cursor, want, "{text}");
        }
        let c = Config::from_text("font-shaping-break = no-cursor\nfont-shaping-break = word\n", true);
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
        assert!(!c.font_shaping_break.cursor, "읽지 못한 줄은 앞 값을 그대로 둔다");
    }

    /// #174: `adjust-icon-height`는 다른 `adjust-*`처럼 `10%`·픽셀
    #[test]
    fn adjust_icon_height_key() {
        let c = Config::from_text("adjust-icon-height = -15%\n", true);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.adjust_icon_height, Some(Adjust::Percent(-15.0)));
        let c = Config::from_text("adjust-icon-height = 3\nadjust-icon-height = tall\n", true);
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
        assert_eq!(c.adjust_icon_height, None);
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
    fn macos_window_keys() {
        let d = Config::default();
        assert_eq!(d.non_native_fullscreen, NonNativeFullscreen::False);
        assert!(!d.window_step_resize);
        assert_eq!(d.macos_window_buttons, MacWindowButtons::Visible);
        assert_eq!(d.macos_hidden, MacHidden::Never);
        let c = Config::from_text(
            "macos-non-native-fullscreen = visible-menu\nwindow-step-resize = true\nmacos-window-buttons = hidden\nmacos-hidden = always\n",
            true,
        );
        assert_eq!(c.non_native_fullscreen, NonNativeFullscreen::VisibleMenu);
        assert!(c.window_step_resize);
        assert_eq!(c.macos_window_buttons, MacWindowButtons::Hidden);
        assert_eq!(c.macos_hidden, MacHidden::Always);
        assert!(c.warnings.is_empty());
        for (v, want) in [
            ("true", NonNativeFullscreen::True),
            ("false", NonNativeFullscreen::False),
            ("padded-notch", NonNativeFullscreen::PaddedNotch),
        ] {
            let c = Config::from_text(&format!("macos-non-native-fullscreen = {v}\n"), true);
            assert_eq!(c.non_native_fullscreen, want, "{v}");
        }
        for bad in [
            "macos-non-native-fullscreen = maybe",
            "window-step-resize = 2",
            "macos-window-buttons = gone",
            "macos-hidden = sometimes",
        ] {
            assert_eq!(Config::from_text(&format!("{bad}\n"), true).warnings.len(), 1, "{bad}");
        }
    }

    #[test]
    fn window_look_keys() {
        let d = Config::default();
        assert_eq!(d.window_decoration, WindowDecoration::Auto);
        assert_eq!(d.macos_titlebar_style, MacTitlebarStyle::Transparent);
        assert!(d.macos_window_shadow);
        assert_eq!(d.window_theme, WindowTheme::Auto);
        assert_eq!(d.macos_titlebar_proxy_icon, MacTitlebarProxyIcon::Visible);
        assert_eq!(d.macos_dock_drop_behavior, MacDockDropBehavior::NewTab);
        assert!(!d.theme_split);
        for (v, want) in [
            ("auto", WindowDecoration::Auto),
            ("true", WindowDecoration::Auto),
            ("client", WindowDecoration::Client),
            ("server", WindowDecoration::Server),
            ("none", WindowDecoration::None),
            ("false", WindowDecoration::None),
        ] {
            let c = Config::from_text(&format!("window-decoration = {v}\n"), true);
            assert_eq!((c.window_decoration, c.warnings.len()), (want, 0), "{v}");
        }
        for (v, want) in [
            ("native", MacTitlebarStyle::Native),
            ("transparent", MacTitlebarStyle::Transparent),
            ("tabs", MacTitlebarStyle::Tabs),
            ("hidden", MacTitlebarStyle::Hidden),
        ] {
            let c = Config::from_text(&format!("macos-titlebar-style = {v}\n"), true);
            assert_eq!((c.macos_titlebar_style, c.warnings.len()), (want, 0), "{v}");
        }
        for (v, want) in [
            ("auto", WindowTheme::Auto),
            ("system", WindowTheme::System),
            ("light", WindowTheme::Light),
            ("dark", WindowTheme::Dark),
            ("ghostty", WindowTheme::Ghostty),
        ] {
            let c = Config::from_text(&format!("window-theme = {v}\n"), true);
            assert_eq!((c.window_theme, c.warnings.len()), (want, 0), "{v}");
        }
        let c = Config::from_text(
            "macos-window-shadow = false\nmacos-titlebar-proxy-icon = hidden\n",
            true,
        );
        assert!(!c.macos_window_shadow);
        assert_eq!(c.macos_titlebar_proxy_icon, MacTitlebarProxyIcon::Hidden);
        assert!(c.warnings.is_empty());
        for bad in [
            "window-decoration = fancy",
            "macos-titlebar-style = glass",
            "macos-window-shadow = maybe",
            "window-theme = blue",
            "macos-titlebar-proxy-icon = gone",
        ] {
            assert_eq!(Config::from_text(&format!("{bad}\n"), true).warnings.len(), 1, "{bad}");
        }
        let r = Config::from_text("window-decoration = none\nmacos-titlebar-style = hidden\n", true).render();
        assert!(
            r.contains("window-decoration = none") && r.contains("macos-titlebar-style = hidden"),
            "{r}"
        );
    }

    #[test]
    fn split_theme_is_detected() {
        assert!(is_split_theme("light:GitHub Light Default,dark:GitHub Dark"));
        assert!(is_split_theme(" dark:A , light:B"));
        assert!(!is_split_theme("GitHub Dark"));
        assert!(!is_split_theme("light:OnlyLight"));
        assert!(Config::from_text("theme = light:nonexistent-a,dark:nonexistent-b\n", true).theme_split);
        assert!(!Config::from_text("theme = nonexistent-a\n", true).theme_split);
    }

    #[test]
    fn fullscreen_non_native_values_set_kind() {
        let c = Config::from_text("fullscreen = non-native-visible-menu\n", true);
        assert!(c.fullscreen);
        assert_eq!(c.non_native_fullscreen, NonNativeFullscreen::VisibleMenu);
        let c = Config::from_text("fullscreen = non-native\n", true);
        assert_eq!(c.non_native_fullscreen, NonNativeFullscreen::True);
        let c = Config::from_text("fullscreen = non-native-padded-notch\n", true);
        assert_eq!(c.non_native_fullscreen, NonNativeFullscreen::PaddedNotch);
        let c = Config::from_text("fullscreen = true\n", true);
        assert_eq!(
            c.non_native_fullscreen,
            NonNativeFullscreen::False,
            "native true는 그대로"
        );
    }

    #[test]
    fn receive_only_window_keys_do_not_warn() {
        let c = Config::from_text(
            "window-subtitle = working-directory\nwindow-titlebar-background = #112233\nwindow-titlebar-foreground = white\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
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
    fn vt_limit_keys_default_like_ghostty() {
        let d = Config::default();
        assert_eq!(d.enquiry_response, "");
        assert!(!d.vt_kam_allowed && !d.vt_window_resize_allowed);
        assert_eq!(d.image_storage_limit, 320_000_000);
        assert_eq!(d.clipboard_write_limit_bytes, Some(64 * 1024 * 1024));
        assert!(d.clipboard_codepoint_map.is_empty());
    }

    #[test]
    fn vt_limit_keys_parse() {
        let c = Config::from_text(
            "enquiry-response = \"b-terminal\"\nvt-kam-allowed = true\nvt-window-resize-allowed = true\n\
             image-storage-limit = 1000\nclipboard-write-limit-bytes = 1234\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.enquiry_response, "b-terminal");
        assert!(c.vt_kam_allowed && c.vt_window_resize_allowed);
        assert_eq!(c.image_storage_limit, 1000);
        assert_eq!(c.clipboard_write_limit_bytes, Some(1234));
        let c = Config::from_text(
            "clipboard-write-limit-bytes = unlimited\nimage-storage-limit = 0\n",
            true,
        );
        assert_eq!((c.clipboard_write_limit_bytes, c.image_storage_limit), (None, 0));
        // 읽지 못하는 값은 경고하고 기본을 둔다
        for bad in [
            "image-storage-limit = -1",
            "image-storage-limit = 4294967296",
            "image-storage-limit = big",
            "clipboard-write-limit-bytes = -5",
            "clipboard-write-limit-bytes = lots",
            "vt-kam-allowed = maybe",
            "vt-window-resize-allowed = maybe",
        ] {
            let c = Config::from_text(&format!("{bad}\n"), true);
            assert_eq!(c.warnings.len(), 1, "{bad}");
            let d = Config::default();
            assert_eq!(
                (
                    c.image_storage_limit,
                    c.clipboard_write_limit_bytes,
                    c.vt_kam_allowed,
                    c.vt_window_resize_allowed
                ),
                (
                    d.image_storage_limit,
                    d.clipboard_write_limit_bytes,
                    d.vt_kam_allowed,
                    d.vt_window_resize_allowed
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn clipboard_access_keys_take_ask_and_keep_defaults() {
        use crate::clipboard::ClipboardAccess::{Allow, Ask, Deny};
        let d = Config::default();
        assert_eq!((d.clipboard_read, d.clipboard_write), (Deny, Allow), "기본값은 그대로");
        let c = Config::from_text("clipboard-read = ask\nclipboard-write = ask\n", true);
        assert_eq!((c.clipboard_read, c.clipboard_write), (Ask, Ask));
        let c = Config::from_text("clipboard-read = true\nclipboard-write = false\n", true);
        assert_eq!((c.clipboard_read, c.clipboard_write), (Allow, Deny));
        let c = Config::from_text("clipboard-write = maybe\n", true);
        assert_eq!(c.clipboard_write, Allow);
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
    }

    #[test]
    fn clipboard_write_limit_only_narrows_the_osc52_cap() {
        let mb = 1024 * 1024;
        assert!(clipboard_write_allowed(mb, Some(64 * mb)));
        assert!(
            !clipboard_write_allowed(mb + 1, Some(64 * mb)),
            "고정 1MiB를 넘기지 못한다"
        );
        assert!(
            !clipboard_write_allowed(mb + 1, None),
            "unlimited여도 고정 상한은 남는다"
        );
        assert!(clipboard_write_allowed(10, Some(10)));
        assert!(!clipboard_write_allowed(11, Some(10)));
        assert!(
            !clipboard_write_allowed(1, Some(0)),
            "0은 빈 글이 아닌 쓰기를 모두 거절한다"
        );
        assert!(clipboard_write_allowed(0, Some(0)));
    }

    #[test]
    fn clipboard_codepoint_map_parses_and_applies() {
        let c = Config::from_text(
            "clipboard-codepoint-map = U+2500=U+002D\nclipboard-codepoint-map = U+2502 = U+007C\n\
             clipboard-codepoint-map = U+03A3=SUM\nclipboard-codepoint-map = U+2580-U+2590,U+25A0=#\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.clipboard_codepoint_map.len(), 5);
        let m = |t: &str| map_clipboard_text(&c.clipboard_codepoint_map, t);
        assert_eq!(m("a─│Σb"), "a-|SUMb");
        assert_eq!(m("▀▄■▒"), "###▒");
        assert_eq!(m("한글 plain"), "한글 plain");
        // 나중에 적은 규칙이 이긴다
        let c = Config::from_text(
            "clipboard-codepoint-map = U+0041-U+005A=X\nclipboard-codepoint-map = U+0042=b\n",
            true,
        );
        assert_eq!(map_clipboard_text(&c.clipboard_codepoint_map, "ABC"), "XbX");
        // 빈 값은 목록을 비운다
        let c = Config::from_text("clipboard-codepoint-map = U+0041=x\nclipboard-codepoint-map =\n", true);
        assert!(c.clipboard_codepoint_map.is_empty() && c.warnings.is_empty());
        for bad in [
            "U+zz=x",
            "U+0041",
            "=x",
            "U+0042-U+0041=x",
            "U+D800=x",
            "U+0041=U+110000",
            "U+0041=U+zz",
        ] {
            let c = Config::from_text(&format!("clipboard-codepoint-map = {bad}\n"), true);
            assert_eq!(c.warnings.len(), 1, "{bad}");
            assert!(c.clipboard_codepoint_map.is_empty(), "{bad}");
        }
    }

    #[test]
    fn grapheme_width_method_like_ghostty() {
        assert_eq!(Config::default().grapheme_width_method, GraphemeWidthMethod::Unicode);
        let c = Config::from_text("grapheme-width-method = legacy\n", true);
        assert_eq!(c.grapheme_width_method, GraphemeWidthMethod::Legacy);
        let c = Config::from_text("grapheme-width-method = unicode\n", true);
        assert_eq!(c.grapheme_width_method, GraphemeWidthMethod::Unicode);
        let c = Config::from_text("grapheme-width-method = wide\n", true);
        assert_eq!(c.warnings.len(), 1);
        assert_eq!(c.grapheme_width_method, GraphemeWidthMethod::Unicode);
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
    fn key_remap_parse_like_ghostty() {
        let c = Config::from_text(
            "key-remap = ctrl=super\nkey-remap = left_alt=right_ctrl\nkey-remap = option = cmd\nkey-remap = control=shift\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.key_remap.len(), 4);
        let k = |kind, side| ModKey { kind, side };
        assert_eq!(
            c.key_remap[0],
            KeyRemap {
                from: k(ModKind::Ctrl, None),
                to: k(ModKind::Super, None)
            }
        );
        assert_eq!(
            c.key_remap[1],
            KeyRemap {
                from: k(ModKind::Alt, Some(ModSide::Left)),
                to: k(ModKind::Ctrl, Some(ModSide::Right))
            }
        );
        assert_eq!(
            c.key_remap[2],
            KeyRemap {
                from: k(ModKind::Alt, None),
                to: k(ModKind::Super, None)
            }
        );
        // 잘못된 것: 이름, = 없음, 키가 아닌 것, 한쪽만 비었음
        for bad in [
            "ctrl",
            "ctrl=",
            "=ctrl",
            "ctrl=x",
            "a=ctrl",
            "left_=ctrl",
            "up_ctrl=alt",
            "ctrl=alt=shift",
        ] {
            let c = Config::from_text(&format!("key-remap = {bad}\n"), true);
            assert_eq!(c.warnings.len(), 1, "{bad}");
            assert!(c.key_remap.is_empty(), "{bad}");
        }
        // 빈 값은 지금까지 쌓은 것을 지운다
        let c = Config::from_text("key-remap = ctrl=alt\nkey-remap =\nkey-remap = alt=ctrl\n", true);
        assert_eq!(c.key_remap.len(), 1);
        assert!(Config::default().key_remap.is_empty());
        assert!(c.render().contains("key-remap = alt=ctrl"));
    }

    #[test]
    fn key_remap_apply() {
        use ModKind::*;
        use ModSide::*;
        let r = |s: &str| KeyRemap::parse(s).unwrap();
        let held = |v: &[(ModKind, ModSide)]| v.iter().fold(SidedMods::default(), |m, (k, s)| m.with(*k, *s));
        // 바꾸기가 없으면 그대로
        let m = held(&[(Ctrl, Left), (Shift, Right)]);
        assert_eq!(apply_key_remap(&[], m), m);
        // ctrl=super: 왼쪽·오른쪽 Ctrl 모두 Super가 된다
        let rs = [r("ctrl=super")];
        for side in [Left, Right] {
            let out = apply_key_remap(&rs, held(&[(Ctrl, side)]));
            assert!(out.has(Super) && !out.has(Ctrl), "{side:?}");
        }
        // 한쪽 이름은 그쪽만: left_ctrl=super
        let rs = [r("left_ctrl=super")];
        let out = apply_key_remap(&rs, held(&[(Ctrl, Left)]));
        assert!(out.has(Super) && !out.has(Ctrl));
        let out = apply_key_remap(&rs, held(&[(Ctrl, Right)]));
        assert!(out.has(Ctrl) && !out.has(Super), "오른쪽 Ctrl은 그대로");
        // 한쪽만 바꾸고 다른 쪽이 같이 눌려 있으면 그 종류가 남는다
        let out = apply_key_remap(&rs, held(&[(Ctrl, Left), (Ctrl, Right)]));
        assert!(out.has(Ctrl) && out.has(Super));
        // 한 방향: ctrl=super를 해도 Super는 Super
        let rs = [r("ctrl=super")];
        let out = apply_key_remap(&rs, held(&[(Super, Left)]));
        assert!(out.has(Super) && !out.has(Ctrl));
        // 이어서 바뀌지 않는다: ctrl=super, alt=ctrl이면 Alt는 Ctrl이 될 뿐 Super가 아니다
        let rs = [r("ctrl=super"), r("alt=ctrl")];
        let out = apply_key_remap(&rs, held(&[(Alt, Left)]));
        assert!(out.has(Ctrl) && !out.has(Super) && !out.has(Alt));
        // 서로 바꾸기: ctrl=alt, alt=ctrl
        let rs = [r("ctrl=alt"), r("alt=ctrl")];
        let out = apply_key_remap(&rs, held(&[(Ctrl, Left)]));
        assert!(out.has(Alt) && !out.has(Ctrl));
        let out = apply_key_remap(&rs, held(&[(Alt, Right)]));
        assert!(out.has(Ctrl) && !out.has(Alt));
        let out = apply_key_remap(&rs, held(&[(Alt, Right), (Ctrl, Left), (Shift, Left)]));
        assert!(
            out.has(Ctrl) && out.has(Alt) && out.has(Shift),
            "둘 다 눌렀으면 둘 다 남는다"
        );
        // 눌리지 않은 키에는 아무 일도 없다
        let out = apply_key_remap(&rs, held(&[(Shift, Left)]));
        assert_eq!(out, held(&[(Shift, Left)]));
    }

    #[test]
    fn macos_icon_and_receive_only_keys() {
        let d = Config::default();
        assert_eq!(d.macos_icon, MacosIcon::Official);
        assert!(d.macos_custom_icon.is_none());
        let c = Config::from_text("macos-icon = custom\nmacos-custom-icon = /tmp/i.icns\n", true);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.macos_icon, MacosIcon::Custom);
        assert_eq!(c.macos_custom_icon, Some(PathBuf::from("/tmp/i.icns")));
        assert_eq!(
            Config::from_text("macos-icon = blueprint\n", true).macos_icon,
            MacosIcon::Builtin
        );
        assert_eq!(
            Config::from_text("macos-icon = custom-style\n", true).macos_icon,
            MacosIcon::CustomStyle
        );
        assert_eq!(Config::from_text("macos-icon = nope\n", true).warnings.len(), 1);
        assert_eq!(
            Config::from_text("macos-custom-icon = /a\nmacos-custom-icon =\n", true).macos_custom_icon,
            None
        );
        // 받기만 하는 키는 경고하지 않는다
        for k in [
            "macos-icon-frame = chrome",
            "macos-icon-ghost-color = #fff",
            "macos-icon-screen-color = #000,#111",
            "class = x",
            "x11-instance-name = x",
            "language = de",
            "gtk-custom-css = a.css",
            "gtk-horizontal-tab-scroll = true",
            "gtk-opengl-debug = true",
            "gtk-quick-terminal-layer = top",
            "gtk-quick-terminal-namespace = q",
            "gtk-tabs-inverted = true",
            "gtk-tabs-location = bottom",
            "gtk-titlebar-hide-when-maximized = true",
            "gtk-titlebar-style = tabs",
            "gtk-toolbar-style = flat",
            "gtk-wide-tabs = false",
            "linux-cgroup = never",
            "linux-cgroup-memory-limit = 1",
            "linux-cgroup-processes-limit = 1",
            "linux-cgroup-hard-fail = true",
            "freetype-load-flags = light",
            "async-backend = epoll",
            "window-title-font-family = Menlo",
            "quick-terminal-keyboard-interactivity = none",
            "scrollback-compression = false",
            "macos-applescript = false",
            "macos-shortcuts = deny",
            "drag-handle = never",
            "background-blur = 20",
        ] {
            let c = Config::from_text(&format!("{k}\n"), true);
            assert!(c.warnings.is_empty(), "{k}: {:?}", c.warnings);
        }
        assert_eq!(Config::from_text("no-such-key = 1\n", true).warnings.len(), 1);
    }

    #[test]
    fn quick_terminal_space_behavior() {
        assert_eq!(Config::default().quick.space, QuickSpace::Move);
        assert_eq!(
            Config::from_text("quick-terminal-space-behavior = remain\n", true)
                .quick
                .space,
            QuickSpace::Remain
        );
        assert_eq!(
            Config::from_text("quick-terminal-space-behavior = move\n", true)
                .quick
                .space,
            QuickSpace::Move
        );
        let c = Config::from_text("quick-terminal-space-behavior = stay\n", true);
        assert_eq!(c.warnings.len(), 1);
        assert_eq!(c.quick.space, QuickSpace::Move);
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
    fn undo_timeout_like_ghostty() {
        use std::time::Duration;
        assert_eq!(
            Config::default().undo_timeout,
            Duration::from_secs(5),
            "Ghostty 기본 5초"
        );
        let c = Config::from_text("undo-timeout = 1m30s\n", true);
        assert_eq!((c.undo_timeout, c.warnings.len()), (Duration::from_secs(90), 0));
        let c = Config::from_text("undo-timeout = 0s\n", true);
        assert_eq!((c.undo_timeout, c.warnings.len()), (Duration::ZERO, 0), "0이면 끈다");
        let c = Config::from_text("undo-timeout = soon\n", true);
        assert_eq!(
            (c.undo_timeout, c.warnings.len()),
            (Duration::from_secs(5), 1),
            "틀리면 경고하고 기본"
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

        let d = Config::default();
        assert!(d.progress_style);
        assert_eq!(d.scrollbar, Scrollbar::System);
        assert!(!d.split_preserve_zoom.navigation);
        let c = Config::from_text(
            "progress-style = false\nscrollbar = never\nsplit-preserve-zoom = navigation\n",
            true,
        );
        assert!(!c.progress_style);
        assert_eq!(c.scrollbar, Scrollbar::Never);
        assert!(c.split_preserve_zoom.navigation);
        let r = c.render();
        assert!(r.contains("progress-style = false"));
        assert!(r.contains("scrollbar = never"));
        assert!(r.contains("split-preserve-zoom = navigation"));
        let c = Config::from_text(
            "progress-style = x\nscrollbar = always\nsplit-preserve-zoom = foo\n",
            true,
        );
        assert!(c.progress_style && c.scrollbar == Scrollbar::System && !c.split_preserve_zoom.navigation);
        assert_eq!(c.warnings.len(), 3);
        let mut z = SplitPreserveZoom::default();
        assert!(z.apply("navigation"));
        assert!(z.apply("no-navigation"));
        assert!(!z.navigation);
        assert!(!z.apply("navigation,bogus"));
        assert!(!z.navigation, "모르는 이름이 섞이면 아무것도 바꾸지 않는다");

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
    fn shell_integration_features_like_ghostty() {
        let d = Config::default().shell_integration_features;
        assert!(d.cursor && d.title && d.path, "Ghostty 기본은 cursor·title·path");
        assert!(!d.sudo && !d.ssh_env && !d.ssh_terminfo);
        assert_eq!(d.to_list(), "cursor,title,path");
        let c = Config::from_text("shell-integration-features = no-cursor,sudo\n", true);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(
            c.shell_integration_features,
            ShellFeatures {
                cursor: false,
                sudo: true,
                ..ShellFeatures::default()
            },
            "적지 않은 것은 그대로"
        );
        assert_eq!(c.shell_integration_features.to_list(), "sudo,title,path");
        assert!(
            c.render().contains("shell-integration-features = sudo,title,path"),
            "{}",
            c.render()
        );
        let c = Config::from_text(
            "shell-integration-features = false\nshell-integration-features = ssh-env, ssh-terminfo\n",
            true,
        );
        assert_eq!(
            c.shell_integration_features,
            ShellFeatures {
                cursor: false,
                sudo: false,
                title: false,
                ssh_env: true,
                ssh_terminfo: true,
                path: false
            }
        );
        let all = Config::from_text("shell-integration-features = true\n", true).shell_integration_features;
        assert_eq!(all.to_list(), "cursor,sudo,title,ssh-env,ssh-terminfo,path");
        let none = Config::from_text("shell-integration-features = false\n", true);
        assert_eq!(none.shell_integration_features.to_list(), "");
        assert!(none.render().contains("shell-integration-features = false"));
        // 모르는 이름이 섞이면 줄 전체를 버리고 경고
        let c = Config::from_text("shell-integration-features = no-cursor,chime\n", true);
        assert_eq!(c.shell_integration_features, ShellFeatures::default());
        assert_eq!(c.warnings.len(), 1);
        assert!(
            Config::from_text("shell-integration-features = sudo\n", true)
                .warnings
                .is_empty()
        );
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

    /// #177: 바이트 상한을 줄 수로 바꾸고, 두 상한 중 작은 쪽을 쓴다
    #[test]
    fn scrollback_rows_takes_the_smaller_limit() {
        assert_eq!(scrollback_rows(Some(50_000_000), None), 50_000);
        assert_eq!(scrollback_rows(Some(10_000_000), None), 10_000);
        assert_eq!(
            scrollback_rows(Some(10), None),
            100,
            "바이트는 100줄 밑으로 내려가지 않는다"
        );
        assert_eq!(
            scrollback_rows(Some(50_000_000), Some(2_000)),
            2_000,
            "줄 상한이 먼저 닿는다"
        );
        assert_eq!(
            scrollback_rows(Some(50_000_000), Some(900_000)),
            50_000,
            "바이트 상한이 먼저 닿는다"
        );
        assert_eq!(scrollback_rows(None, Some(0)), 0, "줄 0이면 기록하지 않는다");
        assert_eq!(scrollback_rows(None, Some(123)), 123);
        assert_eq!(
            scrollback_rows(None, None),
            SCROLLBACK_ROWS_MAX,
            "둘 다 무제한이어도 맨 위에서 멈춘다"
        );
        assert_eq!(scrollback_rows(None, Some(usize::MAX)), SCROLLBACK_ROWS_MAX);
        assert_eq!(scrollback_rows(Some(u64::MAX), None), SCROLLBACK_ROWS_MAX);
    }

    /// #177: scrollback-limit-bytes·-lines 해석. 기본은 10MB(릴리스된 Ghostty), 줄 무제한
    #[test]
    fn scrollback_limit_keys_parse_like_ghostty() {
        let d = Config::default();
        assert_eq!(d.scrollback_limit_bytes, Some(10_000_000));
        assert_eq!(d.scrollback_limit_lines, None);
        assert_eq!(d.scrollback_lines, 10_000);

        let c = Config::from_text(
            "scrollback-limit-bytes = 20000000\nscrollback-limit-lines = 5000\n",
            true,
        );
        assert_eq!(c.scrollback_limit_bytes, Some(20_000_000));
        assert_eq!(c.scrollback_limit_lines, Some(5000));
        assert_eq!(c.scrollback_lines, 5000);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);

        let c = Config::from_text(
            "scrollback-limit-bytes = unlimited\nscrollback-limit-lines = unlimited\n",
            true,
        );
        assert_eq!((c.scrollback_limit_bytes, c.scrollback_limit_lines), (None, None));
        assert_eq!(c.scrollback_lines, SCROLLBACK_ROWS_MAX);

        // 빈 값은 기본으로 돌린다
        let c = Config::from_text(
            "scrollback-limit-bytes = 1000\nscrollback-limit-lines = 7\nscrollback-limit-bytes =\nscrollback-limit-lines =\n",
            true,
        );
        assert_eq!(
            (c.scrollback_limit_bytes, c.scrollback_limit_lines),
            (Some(10_000_000), None)
        );

        let c = Config::from_text("scrollback-limit-lines = lots\nscrollback-limit-bytes = -1\n", true);
        assert_eq!(c.warnings.len(), 2, "{:?}", c.warnings);
        assert_eq!(c.scrollback_lines, 10_000, "잘못된 값은 기본 그대로");
    }

    /// #177: 옛 이름 scrollback-limit(Ghostty 1.4가 -bytes로 바꿈)과 b-terminal 키 scrollback-lines는 그대로 동작한다
    #[test]
    fn scrollback_old_keys_still_work() {
        let c = Config::from_text("scrollback-limit = 10000000\n", true);
        assert_eq!(c.scrollback_limit_bytes, Some(10_000_000));
        assert_eq!(c.scrollback_lines, 10_000);
        // scrollback-lines는 전과 같이 정확히 그 줄 수(기본 바이트 상한에 막히지 않는다)
        let c = Config::from_text("scrollback-lines = 200000\n", true);
        assert_eq!(c.scrollback_lines, 200_000);
        let c = Config::from_text("scrollback-lines = 0\n", true);
        assert_eq!(c.scrollback_lines, 0);
        // 뒤에 쓴 새 키가 이긴다
        let c = Config::from_text("scrollback-lines = 200000\nscrollback-limit-bytes = 3000000\n", true);
        assert_eq!(c.scrollback_lines, 3000);
    }

    /// #177: palette-generate·palette-harmonious 키와 16번 이후 palette 값
    #[test]
    fn palette_generate_keys_build_256_colors() {
        let d = Config::default();
        assert!(!d.palette_generate && !d.palette_harmonious && !d.background_opacity_cells);
        assert_eq!(d.theme.indexed(196), Rgb::new(255, 0, 0), "기본은 xterm 표 그대로");

        let c = Config::from_text("palette-generate = true\n", true);
        assert!(c.palette_generate);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_ne!(c.theme.indexed(196), Rgb::new(255, 0, 0));
        let bg = c.theme.background;
        let g = c.theme.indexed(16);
        assert!((g.r as i32 - bg.r as i32).abs() <= 1, "16번은 바탕");

        // 직접 정한 번호는 지켜진다. 바탕·글자색을 뒤에 써도 만든 색에 반영된다
        let c = Config::from_text(
            "palette = 20=#010203\npalette-generate = true\nbackground = #000000\nforeground = #ffffff\n",
            true,
        );
        assert_eq!(c.theme.indexed(20), Rgb::new(1, 2, 3));
        assert_eq!(c.theme.indexed(16), Rgb::new(0, 0, 0));
        assert_eq!(c.theme.indexed(231), Rgb::new(255, 255, 255));

        // 만들지 않아도 16번 이후를 정할 수 있다(나머지는 xterm)
        let c = Config::from_text("palette = 255=#123456\npalette = 300=#ffffff\n", true);
        assert_eq!(c.theme.indexed(255), Rgb::new(0x12, 0x34, 0x56));
        assert_eq!(c.theme.indexed(254), Rgb::new(228, 228, 228));
        assert_eq!(c.warnings.len(), 1, "256 이상 번호는 경고: {:?}", c.warnings);

        // 밝은 테마: 기본은 16번이 어둡고, harmonious는 바탕 그대로
        let light = "background = #ffffff\nforeground = #000000\npalette-generate = true\n";
        let c = Config::from_text(light, true);
        assert_eq!(c.theme.indexed(16), Rgb::new(0, 0, 0));
        let c = Config::from_text(&format!("{light}palette-harmonious = true\n"), true);
        assert!(c.palette_harmonious);
        assert_eq!(c.theme.indexed(16), Rgb::new(255, 255, 255));
    }

    /// #177: background-opacity-cells
    #[test]
    fn background_opacity_cells_key() {
        assert!(Config::from_text("background-opacity-cells = true\n", true).background_opacity_cells);
        assert!(!Config::from_text("background-opacity-cells = false\n", true).background_opacity_cells);
        assert_eq!(
            Config::from_text("background-opacity-cells = maybe\n", true)
                .warnings
                .len(),
            1
        );
    }

    /// #177: --show-config가 쓰는 글을 다시 읽으면 같은 값이 된다
    #[test]
    fn new_keys_round_trip_through_show_config() {
        let c = Config::from_text(
            "scrollback-limit-bytes = unlimited\nscrollback-limit-lines = 321\npalette-generate = true\npalette-harmonious = true\nbackground-opacity-cells = true\npalette = 99=#0a0b0c\n",
            true,
        );
        let text = c.render();
        let r = Config::from_text(&text, true);
        assert_eq!(r.scrollback_limit_bytes, None);
        assert_eq!(r.scrollback_limit_lines, Some(321));
        assert!(r.palette_generate && r.palette_harmonious && r.background_opacity_cells);
        assert_eq!(r.theme.indexed(99), Rgb::new(10, 11, 12));
    }

    /// #191: link-url·link-osc8·link-previews·link
    #[test]
    fn link_keys() {
        let d = Config::default();
        assert!(d.link_url && d.link_osc8, "기본은 둘 다 켬");
        assert_eq!(d.link_previews, LinkPreviews::True);
        let c = Config::from_text("link-url = false\nlink-osc8 = false\nlink-previews = osc8\n", true);
        assert!(!c.link_url && !c.link_osc8);
        assert_eq!(c.link_previews, LinkPreviews::Osc8);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(
            Config::from_text("link-previews = false\n", true).link_previews,
            LinkPreviews::False
        );
        // 잘못된 값은 경고하고 기본을 지킨다
        for bad in ["link-url = maybe", "link-osc8 = 2", "link-previews = sometimes"] {
            let c = Config::from_text(&format!("{bad}\n"), true);
            assert_eq!(c.warnings.len(), 1, "{bad}");
            assert!(
                c.link_url && c.link_osc8 && c.link_previews == LinkPreviews::True,
                "{bad}"
            );
        }
        // 다시 읽어도 같다
        let r = Config::from_text(&c.render(), true);
        assert!(!r.link_url && !r.link_osc8);
        assert_eq!(r.link_previews, LinkPreviews::Osc8);
    }

    /// #191: link는 Ghostty에서도 아직 쓸 수 없다(소스: "TODO: This can't currently be set!"). 받고, 안 된다고 알린다
    #[test]
    fn link_regex_key_is_accepted_but_not_applied() {
        let c = Config::from_text("link = regex:https://x\n", true);
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
        assert!(c.warnings[0].contains("link"), "{:?}", c.warnings);
        assert!(
            !c.warnings[0].contains("아직 모르는"),
            "모르는 키가 아니다: {:?}",
            c.warnings
        );
    }

    #[test]
    fn link_previews_decision() {
        assert!(LinkPreviews::True.shows(true) && LinkPreviews::True.shows(false));
        assert!(!LinkPreviews::False.shows(true) && !LinkPreviews::False.shows(false));
        assert!(LinkPreviews::Osc8.shows(true));
        assert!(
            !LinkPreviews::Osc8.shows(false),
            "화면 글의 URL은 글 자체가 주소라 띠를 보이지 않는다"
        );
    }

    #[test]
    fn secure_input_keys_parse_and_render() {
        let d = Config::default();
        assert!(
            d.macos_auto_secure_input && d.macos_secure_input_indication,
            "Ghostty 기본은 둘 다 켬"
        );
        let c = Config::from_text(
            "macos-auto-secure-input = false\nmacos-secure-input-indication = false\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert!(!c.macos_auto_secure_input);
        assert!(!c.macos_secure_input_indication);
        let c = Config::from_text("macos-auto-secure-input = false\n", true);
        assert!(
            !c.macos_auto_secure_input && c.macos_secure_input_indication,
            "하나만 바꾼다"
        );
        let r = c.render();
        assert!(r.contains("macos-auto-secure-input = false"), "{r}");
        assert!(r.contains("macos-secure-input-indication = true"), "{r}");
        let bad = Config::from_text("macos-auto-secure-input = maybe\n", true);
        assert_eq!(bad.warnings.len(), 1);
        assert!(bad.macos_auto_secure_input, "잘못된 값은 기본을 둔다");
    }

    #[test]
    fn background_blur_values() {
        let blur = |t: &str| Config::from_text(t, true);
        assert_eq!(Config::default().background_blur, 0);
        assert_eq!(blur("background-blur = true\n").background_blur, 20);
        assert_eq!(blur("background-blur = false\n").background_blur, 0);
        assert_eq!(blur("background-blur = 35\n").background_blur, 35);
        assert_eq!(blur("background-blur = 0\n").background_blur, 0);
        assert_eq!(blur("background-blur = 255\n").background_blur, 255);
        // Ghostty parseBool: 1·t는 true(20), 0·f는 false
        assert_eq!(blur("background-blur = 1\n").background_blur, 20);
        assert_eq!(blur("background-blur = t\n").background_blur, 20);
        // 유리 값은 흐림 켬으로 보고 경고하지 않는다
        for g in ["macos-glass-regular", "macos-glass-clear"] {
            let c = blur(&format!("background-blur = {g}\n"));
            assert_eq!(c.background_blur, 20, "{g}");
            assert!(c.warnings.is_empty(), "{g}: {:?}", c.warnings);
        }
        // 옛 이름
        assert_eq!(blur("background-blur-radius = 12\n").background_blur, 12);
        // 잘못된 값은 경고하고 이전 값을 지킨다
        for bad in ["256", "-3", "abc", "True", "1.5", ""] {
            let c = blur(&format!("background-blur = 9\nbackground-blur = {bad}\n"));
            assert_eq!(c.warnings.len(), 1, "{bad}: {:?}", c.warnings);
            assert_eq!(c.background_blur, 9, "{bad}");
        }
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
    fn alpha_blending_key() {
        assert_eq!(Config::default().alpha_blending, AlphaBlending::Native);
        for (v, want) in [
            ("native", AlphaBlending::Native),
            ("linear", AlphaBlending::Linear),
            ("linear-corrected", AlphaBlending::LinearCorrected),
        ] {
            let c = Config::from_text(&format!("alpha-blending = {v}\n"), true);
            assert_eq!(c.alpha_blending, want, "{v}");
            assert!(c.warnings.is_empty(), "{v}: {:?}", c.warnings);
        }
        // 잘못된 값은 경고하고 앞의 값을 지킨다
        let c = Config::from_text("alpha-blending = linear\nalpha-blending = srgb\n", true);
        assert_eq!(c.warnings.len(), 1);
        assert_eq!(c.alpha_blending, AlphaBlending::Linear);
        // 내보내기에 들어간다
        let shown = Config::from_text("alpha-blending = linear-corrected\n", true).render();
        assert!(shown.contains("alpha-blending = linear-corrected"), "{shown}");
        assert!(Config::default().render().contains("alpha-blending = native"));
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
    fn repick_theme_matches_a_fresh_load() {
        let dir = std::env::temp_dir().join(format!("bt-repick-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (l, d) = (dir.join("L"), dir.join("D"));
        std::fs::write(
            &l,
            "background = #fffffe\nforeground = #000001\npalette = 2=#000003\ncursor-color = #111111\n",
        )
        .unwrap();
        std::fs::write(&d, "background = #000001\nforeground = #fffffe\npalette = 2=#ffff03\n").unwrap();
        let text = format!(
            "theme = light:{},dark:{}\nforeground = #abcdef\npalette = 20=#050505\n",
            l.display(),
            d.display()
        );
        let mut c = Config::from_text(&text, true);
        assert_eq!(c.theme.background, Rgb::new(0, 0, 1));
        let light = Config::from_text(&text, false);
        assert!(c.repick_theme(false), "밝은 쪽으로 바뀐다");
        // 설정을 다시 읽은 것과 같은 결과
        assert_eq!(
            (&c.theme, &c.palette_set, c.cursor_color_cell, c.cursor_text_cell),
            (
                &light.theme,
                &light.palette_set,
                light.cursor_color_cell,
                light.cursor_text_cell
            )
        );
        assert_eq!(c.theme.background, Rgb::new(0xff, 0xff, 0xfe));
        assert_eq!(
            c.theme.foreground,
            Rgb::new(0xab, 0xcd, 0xef),
            "설정 색이 테마를 덮는다"
        );
        assert_eq!(c.theme.palette[2], Rgb::new(0, 0, 3));
        assert_eq!(c.theme.cursor, Rgb::new(0x11, 0x11, 0x11));
        assert!(c.palette_set.contains(&(20, Rgb::new(5, 5, 5))));
        assert!(c.repick_theme(true), "다시 어두운 쪽");
        assert_eq!(c.theme.background, Rgb::new(0, 0, 1));
        assert_eq!(c.theme.palette[2], Rgb::new(0xff, 0xff, 3));
        assert!(!c.repick_theme(true), "같은 모양이면 바뀐 것이 없다");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn repick_theme_does_nothing_for_a_single_theme() {
        let mut c = Config::from_text("theme = nonexistent-a\nbackground = #123456\n", true);
        let before = c.clone();
        assert!(!c.repick_theme(false));
        assert_eq!(c, before);
        let mut c = Config::from_text("background = #123456\n", true);
        assert!(!c.repick_theme(false));
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

    /// #193: font-feature는 Ghostty 문법대로 따옴표·`on`/`off`·공백 값도 읽는다
    #[test]
    fn font_feature_syntax_variants() {
        type Feat = Option<([u8; 4], u16)>;
        let cases: &[(&str, Feat)] = &[
            ("calt on", Some((*b"calt", 1))),
            ("calt off", Some((*b"calt", 0))),
            ("calt = 0", Some((*b"calt", 0))),
            ("ss01 3", Some((*b"ss01", 3))),
            ("cv01 = 4", Some((*b"cv01", 4))),
            ("\"calt\"", Some((*b"calt", 1))),
            ("\"calt\" off", Some((*b"calt", 0))),
            ("-\"liga\"", Some((*b"liga", 0))),
            ("'ss02'=2", Some((*b"ss02", 2))),
            ("+ss03", Some((*b"ss03", 1))),
            ("calt maybe", None),
            ("-calt=1", None),
            ("\"calt", None),
            ("", None),
        ];
        for (text, want) in cases {
            assert_eq!(parse_font_feature(text), *want, "{text:?}");
        }
        let c = Config::from_text("font-feature = \"calt\" off, ss01 on\n", false);
        assert_eq!(c.font_features, vec![(*b"calt", 0), (*b"ss01", 1)]);
    }

    /// #193: font-thicken은 기본 끔, 강도는 0~255(기본 255). 범위 밖·글자는 경고하고 앞 값을 둔다
    #[test]
    fn font_thicken_keys() {
        let d = Config::default();
        assert!(!d.font_thicken);
        assert_eq!(d.font_thicken_strength, 255);
        let c = Config::from_text("font-thicken = true\nfont-thicken-strength = 0\n", true);
        assert!(c.font_thicken);
        assert_eq!(c.font_thicken_strength, 0);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        let c = Config::from_text(
            "font-thicken-strength = 128\nfont-thicken-strength = 256\nfont-thicken = yes\n",
            true,
        );
        assert_eq!(c.font_thicken_strength, 128);
        assert!(!c.font_thicken);
        assert_eq!(c.warnings.len(), 2, "{:?}", c.warnings);
    }

    /// #193: font-synthetic-style은 기본 모두 켬. true/false, 쉼표 목록, `no-` 접두. 모르는 이름은 경고하고 앞 값을 둔다
    #[test]
    fn font_synthetic_style_key() {
        assert_eq!(Config::default().font_synthetic_style, SyntheticStyle::default());
        let on = SyntheticStyle::default();
        let off = SyntheticStyle {
            bold: false,
            italic: false,
            bold_italic: false,
        };
        let cases = [
            ("true", on),
            ("false", off),
            ("no-bold", SyntheticStyle { bold: false, ..on }),
            (
                "no-bold,no-italic",
                SyntheticStyle {
                    bold: false,
                    italic: false,
                    ..on
                },
            ),
            (
                "no-bold-italic",
                SyntheticStyle {
                    bold_italic: false,
                    ..on
                },
            ),
            ("false,bold", SyntheticStyle { bold: true, ..off }),
            ("", on),
        ];
        for (text, want) in cases {
            let c = Config::from_text(&format!("font-synthetic-style = {text}\n"), true);
            assert_eq!(c.font_synthetic_style, want, "{text:?}");
            assert!(c.warnings.is_empty(), "{text:?} {:?}", c.warnings);
        }
        let c = Config::from_text("font-synthetic-style = no-bold\nfont-synthetic-style = wobble\n", true);
        assert!(!c.font_synthetic_style.bold, "읽지 못한 줄은 앞 값을 그대로 둔다");
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
        // 이제 알고 있는 키라서 "모르는 설정" 경고가 아니다
        let c = Config::from_text("font-thicken = true\nfont-feature = -calt\n", true);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        // --show-config는 다시 읽어도 같은 값
        let c = Config::from_text(
            "font-thicken = true\nfont-thicken-strength = 7\nfont-synthetic-style = no-italic\n",
            true,
        );
        let again = Config::from_text(&c.render(), true);
        assert!(again.font_thicken);
        assert_eq!(again.font_thicken_strength, 7);
        assert_eq!(again.font_synthetic_style, c.font_synthetic_style);
    }

    #[test]
    fn window_title_font_family_is_read() {
        assert_eq!(Config::default().window_title_font_family, None);
        let c = Config::from_text("window-title-font-family = Helvetica Neue\n", true);
        assert_eq!(c.window_title_font_family.as_deref(), Some("Helvetica Neue"));
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        // 따옴표는 벗기고, 뒤에 쓴 값이 이기고, 빈 값은 지운다
        let c = Config::from_text(
            "window-title-font-family = \"Menlo\"\nwindow-title-font-family = Avenir\n",
            true,
        );
        assert_eq!(c.window_title_font_family.as_deref(), Some("Avenir"));
        let c = Config::from_text("window-title-font-family = Menlo\nwindow-title-font-family =\n", true);
        assert_eq!(c.window_title_font_family, None);
        // --show-config는 다시 읽어도 같은 값
        let c = Config::from_text("window-title-font-family = Helvetica Neue\n", true);
        let again = Config::from_text(&c.render(), true);
        assert_eq!(again.window_title_font_family.as_deref(), Some("Helvetica Neue"));
    }

    #[test]
    fn command_lifecycle_defaults_like_ghostty() {
        let d = Config::default();
        assert_eq!(d.initial_command, None);
        assert!(!d.wait_after_command);
        assert_eq!(d.abnormal_command_exit_runtime, 250);
        assert!(d.initial_window);
        assert_eq!(d.quit_after_last_window_closed, cfg!(target_os = "linux"));
        assert_eq!(d.quit_after_last_window_closed_delay, None);
    }

    #[test]
    fn command_lifecycle_keys_parse_and_render() {
        use std::time::Duration;
        let c = Config::from_text(
            "initial-command = htop -d 5\nwait-after-command = true\nabnormal-command-exit-runtime = 1000\n\
             initial-window = false\nquit-after-last-window-closed = true\nquit-after-last-window-closed-delay = 1m30s\n",
            true,
        );
        assert_eq!(c.warnings.len(), 0, "{:?}", c.warnings);
        assert_eq!(c.initial_command.as_deref(), Some("htop -d 5"));
        assert!(c.wait_after_command);
        assert_eq!(c.abnormal_command_exit_runtime, 1000);
        assert!(!c.initial_window);
        assert!(c.quit_after_last_window_closed);
        assert_eq!(c.quit_after_last_window_closed_delay, Some(Duration::from_secs(90)));
        let r = c.render();
        for line in [
            "initial-command = htop -d 5",
            "wait-after-command = true",
            "abnormal-command-exit-runtime = 1000",
            "initial-window = false",
            "quit-after-last-window-closed = true",
            "quit-after-last-window-closed-delay = 90s",
        ] {
            assert!(r.contains(line), "{line} 없음");
        }
        // 빈 값은 처음 상태로
        let c = Config::from_text(
            "initial-command = x\ninitial-command =\nquit-after-last-window-closed-delay = 5s\nquit-after-last-window-closed-delay =\n",
            true,
        );
        assert_eq!(c.initial_command, None);
        assert_eq!(c.quit_after_last_window_closed_delay, None);
    }

    #[test]
    fn command_lifecycle_bad_values_warn() {
        let c = Config::from_text(
            "wait-after-command = maybe\nabnormal-command-exit-runtime = -5\ninitial-window = 1\nquit-after-last-window-closed-delay = soon\n",
            true,
        );
        assert_eq!(c.warnings.len(), 4, "{:?}", c.warnings);
        assert!(!c.wait_after_command && c.abnormal_command_exit_runtime == 250 && c.initial_window);
    }

    #[test]
    fn dash_e_forces_quit_like_ghostty() {
        let mut c = Config::from_text(
            "quit-after-last-window-closed = false\nquit-after-last-window-closed-delay = 5s\n",
            true,
        );
        c.apply_cli_command();
        assert!(c.quit_after_last_window_closed);
        assert_eq!(c.quit_after_last_window_closed_delay, None);
    }

    #[test]
    fn term_defaults_to_xterm_256color_not_ghostty() {
        assert_eq!(Config::default().term, "xterm-256color");
        let c = Config::from_text("term = xterm-ghostty\n", true);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.term, "xterm-ghostty");
        // 빈 값은 기본으로 되돌린다
        let c = Config::from_text("term = screen\nterm =\n", true);
        assert_eq!(c.term, "xterm-256color");
        // 제어 문자가 든 값은 경고하고 기본을 둔다
        let c = Config::from_text("term = \"a\u{1b}b\"\n", true);
        assert_eq!(c.term, "xterm-256color");
        assert_eq!(c.warnings.len(), 1);
        assert!(c.render().contains("term = xterm-256color"));
    }

    fn env_of(text: &str) -> (Vec<(String, String)>, usize) {
        let c = Config::from_text(text, true);
        (c.env, c.warnings.len())
    }

    fn kv(k: &str, v: &str) -> (String, String) {
        (k.to_string(), v.to_string())
    }

    #[test]
    fn env_collects_and_later_keys_win() {
        assert_eq!(Config::default().env, vec![]);
        let (env, w) = env_of("env = foo=bar\nenv = bar=baz\n");
        assert_eq!((env, w), (vec![kv("foo", "bar"), kv("bar", "baz")], 0));
        // 같은 키를 또 쓰면 덮는다(자리는 처음 그대로)
        let (env, w) = env_of("env = foo=bar\nenv = bar=baz\nenv = foo=qux\n");
        assert_eq!((env, w), (vec![kv("foo", "qux"), kv("bar", "baz")], 0));
        // 값에 =가 있어도 첫 =에서 가른다. 앞뒤 공백은 자른다
        let (env, _) = env_of("env = A = x=y\n");
        assert_eq!(env, vec![kv("A", "x=y")]);
    }

    #[test]
    fn env_empty_value_removes_key_and_empty_line_clears_map() {
        let (env, w) = env_of("env = foo=bar\nenv = bar=baz\nenv = foo=\n");
        assert_eq!((env, w), (vec![kv("bar", "baz")], 0));
        let (env, w) = env_of("env = foo=bar\nenv = bar=baz\nenv =\nenv = x=1\n");
        assert_eq!((env, w), (vec![kv("x", "1")], 0));
    }

    #[test]
    fn env_bad_values_warn_and_are_skipped() {
        for bad in ["env = novalue", "env = =x", "env = \"A=b\u{0}c\""] {
            let (env, w) = env_of(&format!("env = ok=1\n{bad}\n"));
            assert_eq!(env, vec![kv("ok", "1")], "{bad}");
            assert_eq!(w, 1, "{bad}");
        }
    }

    #[test]
    fn env_renders_back_to_the_same_settings() {
        let c = Config::from_text("env = A=1\nenv = B=two words\n", true);
        let again = Config::from_text(&c.render(), true);
        assert_eq!(again.env, c.env);
    }

    #[test]
    fn zig_string_escapes_follow_zig_literals() {
        assert_eq!(parse_zig_string("hi").unwrap(), b"hi");
        assert_eq!(parse_zig_string("a\\nb\\r\\t\\\\\\'\\\"").unwrap(), b"a\nb\r\t\\'\"");
        assert_eq!(parse_zig_string("\\x1b[2J\\xff").unwrap(), b"\x1b[2J\xff");
        assert_eq!(parse_zig_string("\\u{1F600}한").unwrap(), "\u{1F600}한".as_bytes());
        assert_eq!(parse_zig_string("\\u{41}").unwrap(), b"A");
        for bad in [
            "\\q",
            "a\\",
            "\\x4",
            "\\xzz",
            "\\u{d800}",
            "\\u{110000}",
            "\\u41",
            "\\u{}",
            "\\u{41",
        ] {
            assert!(parse_zig_string(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn input_parses_raw_path_and_unprefixed() {
        assert!(Config::default().input.is_empty());
        let c = Config::from_text(
            "input = raw:echo hi\\n\ninput = \"hello\"\ninput = path:/tmp/x.txt\ninput = raw:a=b\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(
            c.input,
            vec![
                InputSource::Raw(b"echo hi\n".to_vec()),
                InputSource::Raw(b"hello".to_vec()),
                InputSource::Path(PathBuf::from("/tmp/x.txt")),
                InputSource::Raw(b"a=b".to_vec()),
            ]
        );
        // 빈 값은 목록을 비운다
        let c = Config::from_text("input = a\ninput =\ninput = b\n", true);
        assert_eq!(c.input, vec![InputSource::Raw(b"b".to_vec())]);
        // 못 푸는 이스케이프·빈 path는 경고하고 그 줄만 건너뛴다
        let c = Config::from_text("input = raw:\\q\ninput = path:\ninput = ok\n", true);
        assert_eq!(c.input, vec![InputSource::Raw(b"ok".to_vec())]);
        assert_eq!(c.warnings.len(), 2);
    }

    #[test]
    fn input_renders_back_to_the_same_settings() {
        let c = Config::from_text("input = raw:ls\\n\\x1b\\\\한\ninput = path:/tmp/a b.txt\n", true);
        let again = Config::from_text(&c.render(), true);
        assert!(again.warnings.is_empty(), "{:?}", again.warnings);
        assert_eq!(again.input, c.input);
    }

    fn input_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bt-input-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn input_relative_path_is_relative_to_the_config_file() {
        let src = parse_input("path:data.txt", Path::new("/etc/bt")).unwrap();
        assert_eq!(src, InputSource::Path(PathBuf::from("/etc/bt/data.txt")));
    }

    #[test]
    fn resolve_input_joins_raw_and_files_in_order() {
        let d = input_dir("join");
        std::fs::write(d.join("a.txt"), "FILE").unwrap();
        let out = resolve_input(&[
            InputSource::Raw(b"one-".to_vec()),
            InputSource::Path(d.join("a.txt")),
            InputSource::Raw(b"-end".to_vec()),
        ])
        .unwrap();
        assert_eq!(out, b"one-FILE-end");
        assert_eq!(resolve_input(&[]).unwrap(), b"");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn resolve_input_refuses_missing_oversized_and_non_regular_files() {
        let d = input_dir("refuse");
        // 없는 파일 하나가 있으면 아무것도 보내지 않는다
        let r = resolve_input(&[InputSource::Raw(b"x".to_vec()), InputSource::Path(d.join("none"))]);
        assert!(r.is_err());
        // 폴더는 읽지 않는다
        assert!(resolve_input(&[InputSource::Path(d.clone())]).is_err());
        // 장치 파일(끝이 없는 것)도 읽지 않는다
        #[cfg(unix)]
        assert!(resolve_input(&[InputSource::Path(PathBuf::from("/dev/zero"))]).is_err());
        // 정확히 상한이면 되고, 한 바이트 넘으면 안 된다
        let exact = d.join("exact.bin");
        std::fs::write(&exact, vec![b'a'; INPUT_LIMIT_BYTES]).unwrap();
        assert_eq!(
            resolve_input(&[InputSource::Path(exact.clone())]).unwrap().len(),
            INPUT_LIMIT_BYTES
        );
        let over = d.join("over.bin");
        std::fs::write(&over, vec![b'a'; INPUT_LIMIT_BYTES + 1]).unwrap();
        assert!(resolve_input(&[InputSource::Path(over)]).is_err());
        // 전체 합이 넘어도 안 된다
        assert!(resolve_input(&[InputSource::Path(exact), InputSource::Raw(b"x".to_vec())]).is_err());
        assert!(resolve_input(&[InputSource::Raw(vec![b'a'; INPUT_LIMIT_BYTES + 1])]).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn render_prints_the_theme_value_as_written() {
        // 못 찾는 테마라도 쓴 값 그대로(light:/dark: 꼴 포함). 값이 비면(초기화) 줄이 없다
        let r = Config::from_text("theme = light:nonexistent-a,dark:nonexistent-b\n", true).render();
        assert!(
            r.lines().any(|l| l == "theme = light:nonexistent-a,dark:nonexistent-b"),
            "{r}"
        );
        let r = Config::from_text("theme = nonexistent-a\ntheme = nonexistent-b\n", true).render();
        assert!(
            r.lines().any(|l| l == "theme = nonexistent-b") && !r.contains("nonexistent-a"),
            "{r}"
        );
        let no_theme = |r: String| !r.lines().any(|l| l.starts_with("theme = "));
        assert!(no_theme(
            Config::from_text("theme = nonexistent-a\ntheme =\n", true).render()
        ));
        assert!(no_theme(Config::default().render()));
    }

    #[test]
    fn custom_shader_defaults_and_order() {
        let c = Config::default();
        assert!(c.custom_shader.is_empty());
        assert_eq!(c.custom_shader_animation, CustomShaderAnimation::True);
        let c = Config::from_text(
            "custom-shader = /a/one.glsl\ncustom-shader = /a/two.glsl\ncustom-shader = /a/three.glsl\n",
            true,
        );
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        let names: Vec<_> = c.custom_shader.iter().map(|s| s.path.display().to_string()).collect();
        assert_eq!(names, ["/a/one.glsl", "/a/two.glsl", "/a/three.glsl"]);
        assert!(c.custom_shader.iter().all(|s| !s.optional));
    }

    #[test]
    fn custom_shader_question_mark_means_optional_and_empty_clears() {
        let c = Config::from_text("custom-shader = ?/a/maybe.glsl\ncustom-shader = /a/must.glsl\n", true);
        assert_eq!(
            c.custom_shader,
            [
                ShaderPath {
                    path: PathBuf::from("/a/maybe.glsl"),
                    optional: true
                },
                ShaderPath {
                    path: PathBuf::from("/a/must.glsl"),
                    optional: false
                },
            ]
        );
        let c = Config::from_text(
            "custom-shader = /a/x.glsl\ncustom-shader =\ncustom-shader = /a/y.glsl\n",
            true,
        );
        assert_eq!(c.custom_shader.len(), 1);
        assert_eq!(c.custom_shader[0].path, PathBuf::from("/a/y.glsl"));
        let c = Config::from_text("custom-shader = /a/x.glsl\ncustom-shader = \"\"\n", true);
        assert!(c.custom_shader.is_empty());
    }

    #[test]
    fn custom_shader_relative_path_is_relative_to_the_file_with_the_line() {
        let d = input_dir("shader-rel");
        std::fs::create_dir_all(d.join("sub")).unwrap();
        std::fs::write(
            d.join("main.conf"),
            "custom-shader = a.glsl\nconfig-file = sub/inc.conf\n",
        )
        .unwrap();
        std::fs::write(d.join("sub/inc.conf"), "custom-shader = ?b.glsl\n").unwrap();
        let mut w = Vec::new();
        let mut srcs = Vec::new();
        let entries = read_entries(&d.join("main.conf"), 0, &mut w, &mut srcs);
        let mut c = Config::default();
        c.apply_all(&entries, true, &mut w);
        assert_eq!(c.custom_shader.len(), 2, "{w:?}");
        // 줄이 있는 파일의 폴더 기준이다(포함한 파일이면 그 파일의 폴더)
        assert_eq!(c.custom_shader[0].path, d.join("a.glsl"));
        assert_eq!(c.custom_shader[1].path, d.join("sub").join("b.glsl"));
        assert!(c.custom_shader[1].optional);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn custom_shader_tilde_is_the_home_folder() {
        let sh = parse_shader_path("~/s/x.glsl", Path::new("/etc/bt"));
        assert_eq!(sh.path, home().join("s/x.glsl"));
        let sh = parse_shader_path("?~/s/x.glsl", Path::new("/etc/bt"));
        assert!(sh.optional);
        assert_eq!(sh.path, home().join("s/x.glsl"));
    }

    #[test]
    fn custom_shader_animation_values_and_bad_value_warning() {
        for (v, want) in [
            ("false", CustomShaderAnimation::False),
            ("true", CustomShaderAnimation::True),
            ("always", CustomShaderAnimation::Always),
        ] {
            let c = Config::from_text(&format!("custom-shader-animation = {v}\n"), true);
            assert!(c.warnings.is_empty(), "{v}: {:?}", c.warnings);
            assert_eq!(c.custom_shader_animation, want, "{v}");
        }
        let c = Config::from_text(
            "custom-shader-animation = always\ncustom-shader-animation = sometimes\n",
            true,
        );
        assert_eq!(c.warnings.len(), 1, "{:?}", c.warnings);
        assert_eq!(
            c.custom_shader_animation,
            CustomShaderAnimation::Always,
            "나쁜 값은 앞 값을 둔다"
        );
    }

    #[test]
    fn custom_shader_keys_show_in_render() {
        let r = Config::default().render();
        assert!(r.lines().any(|l| l == "custom-shader-animation = true"), "{r}");
        let r = Config::from_text(
            "custom-shader = /a/one.glsl\ncustom-shader = ?/a/two.glsl\ncustom-shader-animation = always\n",
            true,
        )
        .render();
        let lines: Vec<_> = r.lines().filter(|l| l.starts_with("custom-shader")).collect();
        assert_eq!(
            lines,
            [
                "custom-shader = /a/one.glsl",
                "custom-shader = ?/a/two.glsl",
                "custom-shader-animation = always"
            ]
        );
    }
}
