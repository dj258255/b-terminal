//! Exports the `Term` type which is a high-level API for the Grid.

use std::ops::{Index, IndexMut, Range};
use std::sync::Arc;
use std::{cmp, mem, ptr, slice, str};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as Base64;
use bitflags::bitflags;
use log::{debug, trace};
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::event::{Event, EventListener};
use crate::grid::{Dimensions, Grid, GridIterator, Scroll};
use crate::index::{self, Boundary, Column, Direction, Line, Point, Side};
use crate::selection::{Selection, SelectionRange, SelectionType};
use crate::term::cell::{Cell, Flags, LineLength};
use crate::term::color::Colors;
use crate::vi_mode::{ViModeCursor, ViMotion};
use crate::vte::ansi::{
    self, Attr, CharsetIndex, Color, CursorShape, CursorStyle, Handler, Hyperlink, KeyboardModes,
    KeyboardModesApplyBehavior, NamedColor, NamedMode, NamedPrivateMode, PrivateMode,
    ProtectedMode, Rgb, StandardCharset,
};

pub mod cell;
pub mod color;
pub mod search;
pub mod semantic; // b-terminal(#329)

/// Minimum number of columns.
///
/// A minimum of 2 is necessary to hold fullwidth unicode characters.
pub const MIN_COLUMNS: usize = 2;

/// Minimum number of visible lines.
pub const MIN_SCREEN_LINES: usize = 1;

/// Max size of the window title stack.
const TITLE_STACK_MAX_DEPTH: usize = 4096;

/// Default semantic escape characters.
pub const SEMANTIC_ESCAPE_CHARS: &str = ",│`|:\"' ()[]{}<>\t";

/// b-terminal 패치(#180): 지역 표시 글자(U+1F1E6..=U+1F1FF, 국기를 이루는 글자)인가.
fn is_regional_indicator(c: char) -> bool {
    matches!(c as u32, 0x1F1E6..=0x1F1FF)
}

/// b-terminal 패치(#536): 피부색 수식자(U+1F3FB..=U+1F3FF)인가.
fn is_emoji_modifier(c: char) -> bool {
    matches!(c as u32, 0x1F3FB..=0x1F3FF)
}

/// b-terminal 패치(#536): 피부색 수식자를 받는 바탕 글자(Emoji_Modifier_Base)인가. Unicode 18.0 `emoji-data.txt`의 136글자와 대조했다.
/// Ghostty는 수식자가 앞 글자 속성을 따로 보므로(바탕이 아니면 새 글자) 같은 표를 쓴다
fn is_emoji_modifier_base(c: char) -> bool {
    matches!(
        c as u32,
        0x261D
            | 0x26F9
            | 0x270A..=0x270D
            | 0x1F385
            | 0x1F3C2..=0x1F3C4
            | 0x1F3C7
            | 0x1F3CA..=0x1F3CC
            | 0x1F442..=0x1F443
            | 0x1F446..=0x1F450
            | 0x1F466..=0x1F478
            | 0x1F47C
            | 0x1F481..=0x1F483
            | 0x1F485..=0x1F487
            | 0x1F48F
            | 0x1F491
            | 0x1F4AA
            | 0x1F574..=0x1F575
            | 0x1F57A
            | 0x1F590
            | 0x1F595..=0x1F596
            | 0x1F645..=0x1F647
            | 0x1F64B..=0x1F64F
            | 0x1F6A3
            | 0x1F6B4..=0x1F6B6
            | 0x1F6C0
            | 0x1F6CC
            | 0x1F90C
            | 0x1F90F
            | 0x1F918..=0x1F91F
            | 0x1F926
            | 0x1F930..=0x1F939
            | 0x1F93C..=0x1F93E
            | 0x1F977
            | 0x1F9B5..=0x1F9B6
            | 0x1F9B8..=0x1F9B9
            | 0x1F9BB
            | 0x1F9CD..=0x1F9CF
            | 0x1F9D1..=0x1F9DD
            | 0x1FAC3..=0x1FAC5
            | 0x1FAF0..=0x1FAFA
    )
}

/// b-terminal 패치(#536): Extended_Pictographic인가. 표를 따로 두지 않고 `unicode-segmentation`의 GB11
/// (ExtPict Extend* ZWJ × ExtPict)로 묻는다. 이모지 표시 선택자를 받을 글자를 가릴 때만 부른다
fn is_extended_pictographic(c: char) -> bool {
    let mut buf = String::with_capacity(12);
    buf.push(c);
    buf.push('\u{200D}');
    buf.push('\u{1F600}');
    buf.graphemes(true).nth(1).is_none()
}

/// b-terminal 패치(#536): `last` 뒤의 이모지 표시 선택자(U+FE0E·U+FE0F)가 유효한 순서인가.
/// 표를 따로 두지 않고 `unicode-width`의 문자열 폭으로 묻는다. 좁은 글자는 U+FE0F로 두 칸이 되면,
/// 넓은 글자는 U+FE0E로 한 칸이 되면 유효하다(`emoji-variation-sequences.txt`의 글자와 같다).
/// 유효하지 않으면 Ghostty처럼 그 선택자를 버린다(`x` VS16)
fn is_valid_variation(last: char) -> bool {
    let mut buf = String::with_capacity(8);
    buf.push(last);
    match last.width() {
        Some(1) => {
            buf.push('\u{FE0F}');
            buf.width() == 2
        }
        Some(2) => {
            buf.push('\u{FE0E}');
            buf.width() == 1
        }
        _ => false,
    }
}

/// b-terminal 패치(#180): 글자 묶음 폭 모드 번호(DECSET 2027).
const GRAPHEME_CLUSTER_MODE: u16 = 2027;
/// b-terminal(#311): DECLRMM(좌우 여백 모드)
const LEFT_RIGHT_MARGIN_MODE: u16 = 69;
/// b-terminal(#324): DEC 모드 40(DECCOLM 허용)
const ENABLE_MODE_3_MODE: u16 = 40;
/// b-terminal(#301): 키 입력 모드 번호. DECBKM, 키패드 NumLock 무시, Alt ESC 접두, Alt가 ESC 보냄
const BACKARROW_KEY_MODE: u16 = 67;
const IGNORE_KEYPAD_NUMLOCK_MODE: u16 = 1035;
const ALT_ESC_PREFIX_MODE: u16 = 1036;
const ALT_SENDS_ESCAPE_MODE: u16 = 1039;

/// b-terminal(#301): 키 입력 모드 번호를 비트로. 아니면 없다
fn key_input_mode_bit(mode: u16) -> Option<TermMode> {
    Some(match mode {
        BACKARROW_KEY_MODE => TermMode::BACKARROW_KEY,
        IGNORE_KEYPAD_NUMLOCK_MODE => TermMode::IGNORE_KEYPAD_NUMLOCK,
        ALT_ESC_PREFIX_MODE => TermMode::ALT_ESC_PREFIX,
        ALT_SENDS_ESCAPE_MODE => TermMode::ALT_SENDS_ESCAPE,
        _ => return None,
    })
}

/// Max size of the keyboard modes.
const KEYBOARD_MODE_STACK_MAX_DEPTH: usize = TITLE_STACK_MAX_DEPTH;

/// Default tab interval, corresponding to terminfo `it` value.
const INITIAL_TABSTOPS: usize = 8;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct TermMode: u64 { // b-terminal(#304): u32에서 넓힘
        const NONE                    = 0;
        const SHOW_CURSOR             = 1;
        const APP_CURSOR              = 1 << 1;
        const APP_KEYPAD              = 1 << 2;
        const MOUSE_REPORT_CLICK      = 1 << 3;
        const BRACKETED_PASTE         = 1 << 4;
        const SGR_MOUSE               = 1 << 5;
        const MOUSE_MOTION            = 1 << 6;
        const LINE_WRAP               = 1 << 7;
        const LINE_FEED_NEW_LINE      = 1 << 8;
        const ORIGIN                  = 1 << 9;
        const INSERT                  = 1 << 10;
        const FOCUS_IN_OUT            = 1 << 11;
        const ALT_SCREEN              = 1 << 12;
        const MOUSE_DRAG              = 1 << 13;
        const UTF8_MOUSE              = 1 << 14;
        const ALTERNATE_SCROLL        = 1 << 15;
        const VI                      = 1 << 16;
        const URGENCY_HINTS           = 1 << 17;
        const DISAMBIGUATE_ESC_CODES  = 1 << 18;
        const REPORT_EVENT_TYPES      = 1 << 19;
        const REPORT_ALTERNATE_KEYS   = 1 << 20;
        const REPORT_ALL_KEYS_AS_ESC  = 1 << 21;
        const REPORT_ASSOCIATED_TEXT  = 1 << 22;
        // b-terminal 패치(#180): DECSET 2027, 글자 묶음 폭
        const GRAPHEME_CLUSTER        = 1 << 23;
        // b-terminal(#296): DECSET 2026이 켜진 동안(DECRQM 답용), DECSET 1048을 켰다(DECRQM 답용)
        const SYNC_UPDATE             = 1 << 24;
        const SAVE_CURSOR_MODE        = 1 << 25;
        // b-terminal(#301): 키 입력 모드. DECBKM(67), 1035, 1036, 1039
        const BACKARROW_KEY           = 1 << 36;
        const IGNORE_KEYPAD_NUMLOCK   = 1 << 37;
        const ALT_ESC_PREFIX          = 1 << 38;
        const ALT_SENDS_ESCAPE        = 1 << 39;
        // b-terminal(#302): 화면 모드 40~47. 저장만 하는 것(SLOW_SCROLL·AUTOREPEAT·KEYBOARD_ACTION)도 DECRQM이 답하려고 둔다
        const REVERSE_COLORS          = 1 << 40;
        const REVERSE_WRAP            = 1 << 41;
        const REVERSE_WRAP_EXTENDED   = 1 << 42;
        const SLOW_SCROLL             = 1 << 43;
        const AUTOREPEAT              = 1 << 44;
        const KEYBOARD_ACTION         = 1 << 45;
        /// ANSI 모드 12(SRM). Ghostty 기본 켬이라 `TermMode::default()`에 들어 있다
        const SEND_RECEIVE            = 1 << 46;
        // b-terminal(#312): xterm modifyOtherKeys 상태 2(`CSI > 4 ; 2 m`). 비트 50·51은 비워 둔다
        const MODIFY_OTHER_KEYS_2     = 1 << 49;
        /// b-terminal(#311): DECLRMM(69), 좌우 여백 모드. 비트 47은 비워 둔다
        const LEFT_RIGHT_MARGIN       = 1 << 48;
        /// b-terminal(#324): 모드 40(DECCOLM 허용). 꺼져 있으면 `CSI ? 3 h·l`은 아무 일도 하지 않는다
        const ENABLE_MODE_3           = 1 << 47;
        /// b-terminal(#324): 모드 3(132열). DECRQM 답용이고 창 크기가 바뀌어도 그대로 둔다(Ghostty)
        const COLUMN_132              = 1 << 50;
        // b-terminal(#300): X10 이벤트 모드(9)와 urxvt(1015)·SGR 픽셀(1016) 형식. 비트 35는 비워 둔다
        const MOUSE_X10               = 1 << 32;
        const URXVT_MOUSE             = 1 << 33;
        const SGR_PIXELS_MOUSE        = 1 << 34;
        const MOUSE_FORMAT            = Self::SGR_MOUSE.bits() | Self::UTF8_MOUSE.bits() | Self::URXVT_MOUSE.bits() | Self::SGR_PIXELS_MOUSE.bits();
        const MOUSE_MODE              = Self::MOUSE_REPORT_CLICK.bits() | Self::MOUSE_MOTION.bits() | Self::MOUSE_DRAG.bits() | Self::MOUSE_X10.bits();
        const KITTY_KEYBOARD_PROTOCOL = Self::DISAMBIGUATE_ESC_CODES.bits()
                                      | Self::REPORT_EVENT_TYPES.bits()
                                      | Self::REPORT_ALTERNATE_KEYS.bits()
                                      | Self::REPORT_ALL_KEYS_AS_ESC.bits()
                                      | Self::REPORT_ASSOCIATED_TEXT.bits();
         const ANY                    = u64::MAX;
    }
}

impl From<KeyboardModes> for TermMode {
    fn from(value: KeyboardModes) -> Self {
        let mut mode = Self::empty();

        let disambiguate_esc_codes = value.contains(KeyboardModes::DISAMBIGUATE_ESC_CODES);
        mode.set(TermMode::DISAMBIGUATE_ESC_CODES, disambiguate_esc_codes);

        let report_event_types = value.contains(KeyboardModes::REPORT_EVENT_TYPES);
        mode.set(TermMode::REPORT_EVENT_TYPES, report_event_types);

        let report_alternate_keys = value.contains(KeyboardModes::REPORT_ALTERNATE_KEYS);
        mode.set(TermMode::REPORT_ALTERNATE_KEYS, report_alternate_keys);

        let report_all_keys_as_esc = value.contains(KeyboardModes::REPORT_ALL_KEYS_AS_ESC);
        mode.set(TermMode::REPORT_ALL_KEYS_AS_ESC, report_all_keys_as_esc);

        let report_associated_text = value.contains(KeyboardModes::REPORT_ASSOCIATED_TEXT);
        mode.set(TermMode::REPORT_ASSOCIATED_TEXT, report_associated_text);

        mode
    }
}

impl Default for TermMode {
    fn default() -> TermMode {
        TermMode::SHOW_CURSOR
            | TermMode::LINE_WRAP
            | TermMode::ALTERNATE_SCROLL
            | TermMode::URGENCY_HINTS
            // b-terminal(#301): Ghostty modes.zig의 기본 켬(1035, 1036)
            | TermMode::IGNORE_KEYPAD_NUMLOCK
            | TermMode::ALT_ESC_PREFIX
            | TermMode::SEND_RECEIVE // b-terminal(#302)
    }
}

/// Convert a terminal point to a viewport relative point.
#[inline]
pub fn point_to_viewport(display_offset: usize, point: Point) -> Option<Point<usize>> {
    let viewport_line = point.line.0 + display_offset as i32;
    usize::try_from(viewport_line).ok().map(|line| Point::new(line, point.column))
}

/// Convert a viewport relative point to a terminal point.
#[inline]
pub fn viewport_to_point(display_offset: usize, point: Point<usize>) -> Point {
    let line = Line(point.line as i32) - display_offset;
    Point::new(line, point.column)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineDamageBounds {
    /// Damaged line number.
    pub line: usize,

    /// Leftmost damaged column.
    pub left: usize,

    /// Rightmost damaged column.
    pub right: usize,
}

impl LineDamageBounds {
    #[inline]
    pub fn new(line: usize, left: usize, right: usize) -> Self {
        Self { line, left, right }
    }

    #[inline]
    pub fn undamaged(line: usize, num_cols: usize) -> Self {
        Self { line, left: num_cols, right: 0 }
    }

    #[inline]
    pub fn reset(&mut self, num_cols: usize) {
        *self = Self::undamaged(self.line, num_cols);
    }

    #[inline]
    pub fn expand(&mut self, left: usize, right: usize) {
        self.left = cmp::min(self.left, left);
        self.right = cmp::max(self.right, right);
    }

    #[inline]
    pub fn is_damaged(&self) -> bool {
        self.left <= self.right
    }
}

/// Terminal damage information collected since the last [`Term::reset_damage`] call.
#[derive(Debug)]
pub enum TermDamage<'a> {
    /// The entire terminal is damaged.
    Full,

    /// Iterator over damaged lines in the terminal.
    Partial(TermDamageIterator<'a>),
}

/// Iterator over the terminal's viewport damaged lines.
#[derive(Clone, Debug)]
pub struct TermDamageIterator<'a> {
    line_damage: slice::Iter<'a, LineDamageBounds>,
    display_offset: usize,
}

impl<'a> TermDamageIterator<'a> {
    pub fn new(line_damage: &'a [LineDamageBounds], display_offset: usize) -> Self {
        let num_lines = line_damage.len();
        // Filter out invisible damage.
        let line_damage = &line_damage[..num_lines.saturating_sub(display_offset)];
        Self { display_offset, line_damage: line_damage.iter() }
    }
}

impl Iterator for TermDamageIterator<'_> {
    type Item = LineDamageBounds;

    fn next(&mut self) -> Option<Self::Item> {
        self.line_damage.find_map(|line| {
            line.is_damaged().then_some(LineDamageBounds::new(
                line.line + self.display_offset,
                line.left,
                line.right,
            ))
        })
    }
}

/// State of the terminal damage.
struct TermDamageState {
    /// Hint whether terminal should be damaged entirely regardless of the actual damage changes.
    full: bool,

    /// Information about damage on terminal lines.
    lines: Vec<LineDamageBounds>,

    /// Old terminal cursor point.
    last_cursor: Point,
}

impl TermDamageState {
    fn new(num_cols: usize, num_lines: usize) -> Self {
        let lines =
            (0..num_lines).map(|line| LineDamageBounds::undamaged(line, num_cols)).collect();

        Self { full: true, lines, last_cursor: Default::default() }
    }

    #[inline]
    fn resize(&mut self, num_cols: usize, num_lines: usize) {
        // Reset point, so old cursor won't end up outside of the viewport.
        self.last_cursor = Default::default();
        self.full = true;

        self.lines.clear();
        self.lines.reserve(num_lines);
        for line in 0..num_lines {
            self.lines.push(LineDamageBounds::undamaged(line, num_cols));
        }
    }

    /// Damage point inside of the viewport.
    #[inline]
    fn damage_point(&mut self, point: Point<usize>) {
        self.damage_line(point.line, point.column.0, point.column.0);
    }

    /// Expand `line`'s damage to span at least `left` to `right` column.
    #[inline]
    fn damage_line(&mut self, line: usize, left: usize, right: usize) {
        self.lines[line].expand(left, right);
    }

    /// Reset information about terminal damage.
    fn reset(&mut self, num_cols: usize) {
        self.full = false;
        self.lines.iter_mut().for_each(|line| line.reset(num_cols));
    }
}

pub struct Term<T> {
    /// Terminal focus controlling the cursor shape.
    pub is_focused: bool,

    /// Cursor for keyboard selection.
    pub vi_mode_cursor: ViModeCursor,

    pub selection: Option<Selection>,

    /// Currently active grid.
    ///
    /// Tracks the screen buffer currently in use. While the alternate screen buffer is active,
    /// this will be the alternate grid. Otherwise it is the primary screen buffer.
    grid: Grid<Cell>,

    /// Currently inactive grid.
    ///
    /// Opposite of the active grid. While the alternate screen buffer is active, this will be the
    /// primary grid. Otherwise it is the alternate screen buffer.
    inactive_grid: Grid<Cell>,

    /// Index into `charsets`, pointing to what ASCII is currently being mapped to.
    active_charset: CharsetIndex,

    /// b-terminal(#295): SS2·SS3가 정한, 다음 한 글자에만 쓰는 문자 집합.
    single_shift: Option<CharsetIndex>,

    /// Tabstops.
    tabs: TabStops,

    /// Mode flags.
    mode: TermMode,

    /// Scroll region.
    ///
    /// Range going from top to bottom of the terminal, indexed from the top of the viewport.
    scroll_region: Range<Line>,

    /// b-terminal(#311): 좌우 여백(DECSLRM). 0부터 세고 두 칸 모두 여백에 든다. 69가 꺼져 있거나 정하지 않았으면 (0, 마지막 칸)
    margin_left: usize,
    margin_right: usize,

    /// Modified terminal colors.
    colors: Colors,

    /// Current style of the cursor.
    cursor_style: Option<CursorStyle>,

    /// Proxy for sending events to the event loop.
    event_proxy: T,

    /// Current title of the window.
    title: Option<String>,

    /// Stack of saved window titles. When a title is popped from this stack, the `title` for the
    /// term is set.
    title_stack: Vec<Option<String>>,

    /// b-terminal(#324): 창이 정한 격자 크기(열, 줄). `resize`가 받은 값이다. DECCOLM이 격자를 132·80열로 바꿔도 이 값은 그대로이고,
    /// 모드 40을 바꾸거나 창 크기가 바뀌면 격자가 이 값으로 돌아간다
    window_size: (usize, usize),

    /// b-terminal(#313): XTSAVE로 저장한 DEC 모드 값(모드 번호별 마지막 하나). RIS가 지운다
    saved_private_modes: std::collections::HashMap<u16, bool>,

    /// b-terminal(#313): `Some`이면 `report_private_mode`가 답을 쓰지 않고 모드의 지금 상태(켬 `Some(true)`, 끔 `Some(false)`,
    /// 모르거나 다른 곳이 쥠 `None`)를 여기에 적는다. XTSAVE가 DECRQM과 같은 표로 상태를 읽으려는 길이다
    private_mode_probe: Option<Option<bool>>,

    /// The stack for the keyboard modes.
    keyboard_mode_stack: Vec<KeyboardModes>,

    /// Currently inactive keyboard mode stack.
    inactive_keyboard_mode_stack: Vec<KeyboardModes>,

    /// Information about damaged cells.
    damage: TermDamageState,

    /// Config directly for the terminal.
    config: Config,

    /// b-terminal 패치(#180): 방금 찍은 글자 칸. 다음 글자가 같은 묶음이면 그 칸에 잇는다.
    grapheme_anchor: Option<GraphemeAnchor>,

    /// b-terminal(#294): 마지막으로 켠 보호 방식이 ISO(SPA)인가. ISO면 일반 ED·EL·ECH도 보호 칸을 건너뛴다
    /// (Ghostty `Screen.protected_mode == .iso`). 보호를 꺼도(EPA·DECSCA 0) 이 값은 남고, DECSCA 1(DEC)이 거짓으로,
    /// RIS·DECSTR이 거짓으로 되돌린다.
    protected_iso: bool,

    /// b-terminal(#327): XTSHIFTESCAPE로 프로그램이 정한 값(`None`은 정한 적 없음). Ghostty `flags.mouse_shift_capture`
    mouse_shift_capture: Option<bool>,
    /// b-terminal(#329): OSC 133 상태(커서의 글 종류, 지우기 규칙, 다시 그리기)
    semantic: semantic::SemanticState,
}

/// b-terminal 패치(#180): 한 칸의 결합 문자 목록에 이어 붙이는 글자 수 상한(join_grapheme)
const MAX_GRAPHEME_JOIN: usize = 32;

/// 방금 찍은 묶음의 첫 칸과, 그때 커서 자리. 커서가 그대로일 때만 이어 붙인다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GraphemeAnchor {
    base: Point,
    cursor: Point,
    wrap: bool,
}

/// Configuration options for the [`Term`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// The maximum amount of scrolling history.
    pub scrolling_history: usize,

    /// Default cursor style to reset the cursor to.
    pub default_cursor_style: CursorStyle,

    /// Cursor style for Vi mode.
    pub vi_mode_cursor_style: Option<CursorStyle>,

    /// The characters which terminate semantic selection.
    ///
    /// The default value is [`SEMANTIC_ESCAPE_CHARS`].
    pub semantic_escape_chars: String,

    /// Whether to enable kitty keyboard protocol.
    pub kitty_keyboard: bool,

    /// OSC52 support mode.
    pub osc52: Osc52,

    /// b-terminal 패치(#180): 글자 묶음 폭(mode 2027)의 처음 값. Ghostty의
    /// `grapheme-width-method = unicode`면 켠다(기본). 프로그램이 DECSET/DECRST 2027로 바꿀 수 있다.
    pub grapheme_cluster: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scrolling_history: 10000,
            semantic_escape_chars: SEMANTIC_ESCAPE_CHARS.to_owned(),
            default_cursor_style: Default::default(),
            vi_mode_cursor_style: Default::default(),
            kitty_keyboard: Default::default(),
            osc52: Default::default(),
            grapheme_cluster: true,
        }
    }
}

/// OSC 52 behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(rename_all = "lowercase"))]
pub enum Osc52 {
    /// The handling of the escape sequence is disabled.
    Disabled,
    /// Only copy sequence is accepted.
    ///
    /// This option is the default as a compromise between entirely
    /// disabling it (the most secure) and allowing `paste` (the less secure).
    #[default]
    OnlyCopy,
    /// Only paste sequence is accepted.
    OnlyPaste,
    /// Both are accepted.
    CopyPaste,
}

impl<T> Term<T> {
    /// b-terminal(#295): 커서가 스크롤 영역 안이면 영역의 모든 줄에 `op`를 한 번씩 적용한다.
    /// 커서 줄·칸은 그대로 돌려놓고 줄바꿈 대기는 푼다
    fn for_each_region_row(&mut self, mut op: impl FnMut(&mut Self)) {
        let line = self.grid.cursor.point.line;
        if !self.scroll_region.contains(&line) || !self.cursor_in_margins() {
            return;
        }
        for l in self.scroll_region.start.0..self.scroll_region.end.0 {
            self.grid.cursor.point.line = Line(l);
            op(self);
        }
        self.grid.cursor.point.line = line;
        self.grid.cursor.input_needs_wrap = false;
    }

    #[inline]
    pub fn scroll_display(&mut self, scroll: Scroll)
    where
        T: EventListener,
    {
        let old_display_offset = self.grid.display_offset();
        self.grid.scroll_display(scroll);
        self.event_proxy.send_event(Event::MouseCursorDirty);

        // Clamp vi mode cursor to the viewport.
        let viewport_start = -(self.grid.display_offset() as i32);
        let viewport_end = viewport_start + self.bottommost_line().0;
        let vi_cursor_line = &mut self.vi_mode_cursor.point.line.0;
        *vi_cursor_line = cmp::min(viewport_end, cmp::max(viewport_start, *vi_cursor_line));
        self.vi_mode_recompute_selection();

        // Damage everything if display offset changed.
        if old_display_offset != self.grid().display_offset() {
            self.mark_fully_damaged();
        }
    }

    pub fn new<D: Dimensions>(config: Config, dimensions: &D, event_proxy: T) -> Term<T> {
        // b-terminal(#584): 크기 0은 1로 올린다(Ghostty `Terminal.init`은 0을 `InvalidValue`로 거절하지만 `new`는 오류를 돌려줄 수 없다.
        // 0열은 이 아래에서 panic이 났다. 앱은 `GridSize::new`의 최소 크기(2열·1줄)로 먼저 막는다)
        let num_cols = dimensions.columns().max(1);
        let num_lines = dimensions.screen_lines().max(1);

        let history_size = config.scrolling_history;
        let grid = Grid::new(num_lines, num_cols, history_size);
        let inactive_grid = Grid::new(num_lines, num_cols, 0);

        let tabs = TabStops::new(grid.columns());

        let scroll_region = Line(0)..Line(grid.screen_lines() as i32);

        // Initialize terminal damage, covering the entire terminal upon launch.
        let damage = TermDamageState::new(num_cols, num_lines);

        let mut mode = TermMode::default();
        mode.set(TermMode::GRAPHEME_CLUSTER, config.grapheme_cluster);

        Term {
            inactive_grid,
            scroll_region,
            margin_left: 0,
            margin_right: num_cols.saturating_sub(1),
            event_proxy,
            damage,
            config,
            grid,
            tabs,
            inactive_keyboard_mode_stack: Default::default(),
            keyboard_mode_stack: Default::default(),
            active_charset: Default::default(),
            single_shift: None,
            vi_mode_cursor: Default::default(),
            cursor_style: Default::default(),
            colors: color::Colors::default(),
            title_stack: Default::default(),
            window_size: (num_cols, num_lines),
            saved_private_modes: Default::default(),
            private_mode_probe: None,
            is_focused: Default::default(),
            selection: Default::default(),
            title: Default::default(),
            grapheme_anchor: None,
            protected_iso: false,
            mouse_shift_capture: None,
            semantic: Default::default(), // b-terminal(#329)
            mode,
        }
    }

    /// Collect the information about the changes in the lines, which
    /// could be used to minimize the amount of drawing operations.
    ///
    /// The user controlled elements, like `Vi` mode cursor and `Selection` are **not** part of the
    /// collected damage state. Those could easily be tracked by comparing their old and new
    /// value between adjacent frames.
    ///
    /// After reading damage [`reset_damage`] should be called.
    ///
    /// [`reset_damage`]: Self::reset_damage
    #[must_use]
    pub fn damage(&mut self) -> TermDamage<'_> {
        // Ensure the entire terminal is damaged after entering insert mode.
        // Leaving is handled in the ansi handler.
        if self.mode.contains(TermMode::INSERT) {
            self.mark_fully_damaged();
        }

        let previous_cursor = mem::replace(&mut self.damage.last_cursor, self.grid.cursor.point);

        if self.damage.full {
            return TermDamage::Full;
        }

        // Add information about old cursor position and new one if they are not the same, so we
        // cover everything that was produced by `Term::input`.
        if self.damage.last_cursor != previous_cursor {
            // Cursor coordinates are always inside viewport even if you have `display_offset`.
            let point = Point::new(previous_cursor.line.0 as usize, previous_cursor.column);
            self.damage.damage_point(point);
        }

        // Always damage current cursor.
        self.damage_cursor();

        // NOTE: damage which changes all the content when the display offset is non-zero (e.g.
        // scrolling) is handled via full damage.
        let display_offset = self.grid().display_offset();
        TermDamage::Partial(TermDamageIterator::new(&self.damage.lines, display_offset))
    }

    /// Resets the terminal damage information.
    pub fn reset_damage(&mut self) {
        self.damage.reset(self.columns());
    }

    #[inline]
    fn mark_fully_damaged(&mut self) {
        self.damage.full = true;
    }

    /// Set new options for the [`Term`].
    pub fn set_options(&mut self, options: Config)
    where
        T: EventListener,
    {
        let old_config = mem::replace(&mut self.config, options);

        let title_event = match &self.title {
            Some(title) => Event::Title(title.clone()),
            None => Event::ResetTitle,
        };

        self.event_proxy.send_event(title_event);

        if self.mode.contains(TermMode::ALT_SCREEN) {
            self.inactive_grid.update_history(self.config.scrolling_history);
        } else {
            self.grid.update_history(self.config.scrolling_history);
        }

        if self.config.kitty_keyboard != old_config.kitty_keyboard {
            self.keyboard_mode_stack = Vec::new();
            self.inactive_keyboard_mode_stack = Vec::new();
            self.mode.remove(TermMode::KITTY_KEYBOARD_PROTOCOL);
        }

        // Damage everything on config updates.
        self.mark_fully_damaged();
    }

    /// Convert the active selection to a String.
    pub fn selection_to_string(&self) -> Option<String> {
        let selection_range = self.selection.as_ref().and_then(|s| s.to_range(self))?;
        let SelectionRange { start, end, .. } = selection_range;

        let mut res = String::new();

        match self.selection.as_ref() {
            Some(Selection { ty: SelectionType::Block, .. }) => {
                for line in (start.line.0..end.line.0).map(Line::from) {
                    // b-terminal(#786): 사각 고르기는 줄 끝 자리표시 칸 너머 글자를 가져오지 않는다(Ghostty `rectangle`)
                    res += self.line_to_string(line, start.column..end.column, false).trim_end();
                    res += "\n";
                }

                res += self.line_to_string(end.line, start.column..end.column, false).trim_end();
            },
            Some(Selection { ty: SelectionType::Lines, .. }) => {
                res = self.bounds_to_string(start, end) + "\n";
            },
            _ => {
                res = self.bounds_to_string(start, end);
            },
        }

        Some(res)
    }

    /// Convert range between two points to a String.
    pub fn bounds_to_string(&self, start: Point, end: Point) -> String {
        let mut res = String::new();

        for line in (start.line.0..=end.line.0).map(Line::from) {
            let start_col = if line == start.line { start.column } else { Column(0) };
            let end_col = if line == end.line { end.column } else { self.last_column() };

            res += &self.line_to_string(line, start_col..end_col, line == end.line);
        }

        res.strip_suffix('\n').map(str::to_owned).unwrap_or(res)
    }

    /// Convert a single line in the grid to a String.
    fn line_to_string(
        &self,
        line: Line,
        mut cols: Range<Column>,
        include_wrapped_wide: bool,
    ) -> String {
        let mut text = String::new();

        let grid_line = &self.grid[line];
        let line_length = cmp::min(grid_line.line_length(), cols.end + 1);

        // Include wide char when trailing spacer is selected.
        if grid_line[cols.start].flags.contains(Flags::WIDE_CHAR_SPACER) {
            cols.start -= 1;
        }

        let mut tab_mode = false;
        for column in (cols.start.0..line_length.0).map(Column::from) {
            let cell = &grid_line[column];

            // Skip over cells until next tab-stop once a tab was found.
            if tab_mode {
                if self.tabs[column] || cell.c != ' ' {
                    tab_mode = false;
                } else {
                    continue;
                }
            }

            if cell.c == '\t' {
                tab_mode = true;
            }

            if !cell.flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER) {
                // Push cells primary character.
                text.push(cell.c);

                // Push zero-width characters.
                for c in cell.zerowidth().into_iter().flatten() {
                    text.push(*c);
                }
            }
        }

        if cols.end >= self.columns() - 1
            && (line_length.0 == 0
                || !self.grid[line][line_length - 1].flags.contains(Flags::WRAPLINE))
        {
            text.push('\n');
        }

        // If wide char is not part of the selection, but leading spacer is, include it.
        if line_length == self.columns()
            && line_length.0 >= 2
            && grid_line[line_length - 1].flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
            && include_wrapped_wide
        {
            // b-terminal(#786): 넘어간 글자는 다음 줄 첫 칸에 있다(`line - 1`은 윗줄이라 엉뚱한 글자를 읽고, 맨 위 줄에서는 격자 밖이다).
            // 다음 줄이 없으면(아래 줄을 지운 뒤 등) 덧붙일 글자가 없다
            if line < self.bottommost_line() {
                let wide = &self.grid[line + 1i32][Column(0)];
                text.push(wide.c);
                for c in wide.zerowidth().into_iter().flatten() {
                    text.push(*c);
                }
            }
        }

        text
    }

    /// Terminal content required for rendering.
    #[inline]
    pub fn renderable_content(&self) -> RenderableContent<'_>
    where
        T: EventListener,
    {
        RenderableContent::new(self)
    }

    /// Access to the raw grid data structure.
    pub fn grid(&self) -> &Grid<Cell> {
        &self.grid
    }

    /// Mutable access to the raw grid data structure.
    pub fn grid_mut(&mut self) -> &mut Grid<Cell> {
        &mut self.grid
    }

    /// Resize terminal to new dimensions.
    pub fn resize<S: Dimensions>(&mut self, size: S) {
        // b-terminal(#584): 크기 0은 아무것도 바꾸기 전에 거절한다(Ghostty `Terminal.resize`, Terminal.zig 5274줄 `resize rejects zero
        // dimensions before mutation`). 0열은 격자 다시 흘리기에서 panic이 났다
        if size.columns() == 0 || size.screen_lines() == 0 {
            debug!("Term::resize rejected zero dimensions");
            return;
        }
        // b-terminal(#324): 창 크기를 적어 둔다. 격자가 DECCOLM으로 132·80열이어도 창이 바뀌면 창 크기로 돌아간다
        self.window_size = (size.columns(), size.screen_lines());
        self.resize_grid(size);
    }

    /// b-terminal(#324): 격자 크기만 바꾼다(`resize`가 하던 일). DECCOLM과 모드 40이 창 크기를 건드리지 않고 쓴다
    fn resize_grid<S: Dimensions>(&mut self, size: S) {
        let old_cols = self.columns();
        let old_lines = self.screen_lines();

        let num_cols = size.columns();
        let num_lines = size.screen_lines();

        if old_cols == num_cols && old_lines == num_lines {
            debug!("Term::resize dimensions unchanged");
            return;
        }

        debug!("New num_cols is {num_cols} and num_lines is {num_lines}");

        // b-terminal(#482): 줄 나누기 전에 프롬프트 줄들을 줄 수가 바뀌지 않는 빈 줄로 만든다
        let lost_wraps = self.semantic_blank_prompt_for_resize(num_cols);

        // Move vi mode cursor with the content.
        let history_size = self.history_size();
        let mut delta = num_lines as i32 - old_lines as i32;
        let min_delta = cmp::min(0, num_lines as i32 - self.grid.cursor.point.line.0 - 1);
        delta = cmp::min(cmp::max(delta, min_delta), history_size as i32);
        self.vi_mode_cursor.point.line += delta;

        let is_alt = self.mode.contains(TermMode::ALT_SCREEN);
        // b-terminal(#537): 다시 흘리기는 주 화면만, 자동 줄바꿈(DECAWM)이 켜져 있을 때만 한다
        // (Ghostty `Terminal.zig` `resize`의 `.reflow = modes.get(.wraparound)`, 대체 화면은 흘리지 않음)
        let reflow = self.mode.contains(TermMode::LINE_WRAP);
        self.grid.resize_with_lost_wraps(!is_alt && reflow, num_lines, num_cols, if is_alt { 0 } else { lost_wraps });
        self.inactive_grid.resize_with_lost_wraps(is_alt && reflow, num_lines, num_cols, if is_alt { lost_wraps } else { 0 });

        // Invalidate selection and tabs only when necessary.
        if old_cols != num_cols {
            self.selection = None;

            // Recreate tabs list.
            self.tabs.resize(num_cols);
        } else if let Some(selection) = self.selection.take() {
            let max_lines = cmp::max(num_lines, old_lines) as i32;
            let range = Line(0)..Line(max_lines);
            self.selection = selection.rotate(self, &range, -delta);
        }

        // Clamp vi cursor to viewport.
        let vi_point = self.vi_mode_cursor.point;
        let viewport_top = Line(-(self.grid.display_offset() as i32));
        let viewport_bottom = viewport_top + self.bottommost_line();
        self.vi_mode_cursor.point.line =
            cmp::max(cmp::min(vi_point.line, viewport_bottom), viewport_top);
        self.vi_mode_cursor.point.column = cmp::min(vi_point.column, self.last_column());

        // Reset scrolling region.
        self.scroll_region = Line(0)..Line(self.screen_lines() as i32);
        self.reset_margins(); // b-terminal(#311)

        // Resize damage information.
        self.damage.resize(num_cols, num_lines);

        // b-terminal(#329): 프롬프트에 있으면 프롬프트 줄을 비워 셸이 다시 그리게 한다
        self.semantic_clear_prompt_for_redraw();
    }

    /// Active terminal modes.
    #[inline]
    pub fn mode(&self) -> &TermMode {
        &self.mode
    }

    /// b-terminal 패치(#259): 스크롤 영역(DECSTBM)의 줄 범위. DECRQCRA가 원점 모드(DECOM)의 좌표를 풀 때 위 여백을 읽는다.
    #[inline]
    pub fn scroll_region(&self) -> &Range<Line> {
        &self.scroll_region
    }

    /// b-terminal(#801): 커서를 `col`칸으로 옮긴다(Ghostty `cursorHorizontalAbsolute`). 원점 모드의 여백 기준도 좌우 여백도 따지지 않고
    /// 줄바꿈 대기만 푼다. kitty 그림을 놓은 뒤 커서를 옮기는 데 쓴다(#801)
    pub fn set_cursor_column(&mut self, col: usize) {
        self.damage_cursor();
        self.grid.cursor.point.column = Column(cmp::min(col, self.last_column().0));
        self.damage_cursor();
        self.grid.cursor.input_needs_wrap = false;
    }

    /// b-terminal(#327): 커서 위치 보고에 쓰는 (줄, 칸)(0부터). 원점 모드에서는 위 여백·왼쪽 여백 기준이다
    /// (Ghostty stream_handler.zig `cursorReportPos` 854줄). DSR 6과 DECXCPR가 함께 쓴다
    fn cursor_report_pos(&self) -> (usize, usize) {
        let pos = self.grid.cursor.point;
        if self.mode.contains(TermMode::ORIGIN) {
            (
                (pos.line.0 - self.scroll_region.start.0).max(0) as usize,
                pos.column.0.saturating_sub(self.margin_left),
            )
        } else {
            (pos.line.0 as usize, pos.column.0)
        }
    }

    /// b-terminal(#327): XTSHIFTESCAPE로 프로그램이 정한 값. 정한 적이 없으면 `None`
    #[inline]
    pub fn mouse_shift_capture(&self) -> Option<bool> {
        self.mouse_shift_capture
    }

    /// b-terminal(#311): 왼쪽 여백(0부터, 이 칸을 포함)
    #[inline]
    pub fn left_margin(&self) -> usize {
        self.margin_left
    }

    /// b-terminal(#311): 오른쪽 여백(0부터, 이 칸을 포함)
    #[inline]
    pub fn right_margin(&self) -> usize {
        self.margin_right
    }

    /// b-terminal(#311): 여백 기준이 아닌 화면 좌표로 가는 이동(CUU·CUD·CNL·CPL). 원점 모드에서는 위아래 여백 안에 머문다.
    /// `goto`는 원점 모드에서 좌표에 여백을 더하므로 상대 이동에 쓰면 어긋난다
    fn move_to_screen(&mut self, line: i32, col: usize) {
        let (min_y, max_y) = if self.mode.contains(TermMode::ORIGIN) {
            (self.scroll_region.start, self.scroll_region.end - 1)
        } else {
            (Line(0), self.bottommost_line())
        };

        self.damage_cursor();
        self.grid.cursor.point.line = cmp::max(cmp::min(Line(line), max_y), min_y);
        self.grid.cursor.point.column = cmp::min(Column(col), self.last_column());
        self.damage_cursor();
        self.grid.cursor.input_needs_wrap = false;
    }

    /// b-terminal(#311): 왼쪽으로 가는 이동(CUB·BS)이 멈추는 칸. 왼쪽 여백 안이면 그 여백, 여백보다 왼쪽이면 0이다
    /// (Ghostty `cursorLeft`의 `left_margin`)
    #[inline]
    fn left_limit(&self) -> usize {
        if self.grid.cursor.point.column.0 < self.margin_left { 0 } else { self.margin_left }
    }

    /// b-terminal(#311): 좌우 여백이 화면 전체 폭인가
    #[inline]
    fn margins_full(&self) -> bool {
        self.margin_left == 0 && self.margin_right + 1 >= self.columns()
    }

    /// b-terminal(#311): 커서의 칸이 좌우 여백 안인가
    #[inline]
    fn cursor_in_margins(&self) -> bool {
        let column = self.grid.cursor.point.column.0;
        column >= self.margin_left && column <= self.margin_right
    }

    /// b-terminal(#524): 칸 하나를 현재 배경색의 빈 칸으로 만든다. 줄 끝의 이어진 줄 표시(WRAPLINE)는 남긴다
    fn blank_cell_keep_wrap(&mut self, line: Line, column: usize) {
        let bg = self.grid.cursor.template.bg;
        let cell = &mut self.grid[line][Column(column)];
        let wrap = cell.flags.contains(Flags::WRAPLINE);
        *cell = bg.into();
        cell.flags.set(Flags::WRAPLINE, wrap);
    }

    /// b-terminal(#524): `x - 1`칸과 `x`칸 사이의 경계가 넓은 글자를 가르면 그 글자의 두 칸을 함께 지운다.
    /// 칸을 지우거나 옮기는 구간 `[a, b]`의 양 끝은 `a`와 `b + 1`로 부른다. `x`는 칸 수까지 허용한다.
    /// Ghostty `Screen.splitCellBoundary`(1967줄)와 같은 세 경우다:
    /// - `x - 1`이 넓은 글자의 앞 칸이면 `x - 1`과 `x`를 지운다
    /// - `x`가 0이나 1이고 이 줄이 윗줄에서 이어진 줄이며 0번 칸이 넓은 글자면, 윗줄 끝의 앞 칸 자리 표시를 지운다
    /// - `x`가 칸 수이고 줄 끝이 앞 칸 자리 표시면 지운다
    /// 줄의 이어진 줄 표시는 건드리지 않는다(`reset_row_wrap`이 한다)
    fn split_cell_boundary(&mut self, line: Line, x: usize) {
        let columns = self.columns();
        if x >= columns {
            if self.grid[line][Column(columns - 1)].flags.contains(Flags::LEADING_WIDE_CHAR_SPACER)
            {
                self.blank_cell_keep_wrap(line, columns - 1);
            }
            return;
        }

        if x <= 1
            && line > self.topmost_line()
            && self.grid[line][Column(0)].flags.contains(Flags::WIDE_CHAR)
        {
            let above = line - 1i32;
            let last = self.grid[above][Column(columns - 1)].flags;
            if last.contains(Flags::WRAPLINE) && last.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
                self.blank_cell_keep_wrap(above, columns - 1);
                // b-terminal(#587): 위 줄이 스크롤백이면(음수 줄) 화면 손상에 적을 줄이 없다. `as usize`로 바꾸면 범위 밖이 된다
                if above.0 >= 0 {
                    self.damage.damage_line(above.0 as usize, columns - 1, columns - 1);
                }
            }
        }

        if x > 0 && self.grid[line][Column(x - 1)].flags.contains(Flags::WIDE_CHAR) {
            self.blank_cell_keep_wrap(line, x - 1);
            self.blank_cell_keep_wrap(line, x);
        }
    }

    /// b-terminal(#524): 줄이 다음 줄로 이어진다는 표시(줄 끝 칸의 WRAPLINE)를 지운다.
    /// 줄 끝이 앞 칸 자리 표시였으면 그 칸도 빈 칸으로 만든다(Ghostty `cursorResetWrap`)
    fn reset_row_wrap(&mut self, line: Line) {
        let last = Column(self.columns() - 1);
        let bg = self.grid.cursor.template.bg;
        let cell = &mut self.grid[line][last];
        if !cell.flags.contains(Flags::WRAPLINE) {
            return;
        }
        if cell.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
            *cell = bg.into();
        } else {
            cell.flags.remove(Flags::WRAPLINE);
        }
    }

    /// b-terminal(#524): EL 0·ED 0이 지우기 시작하는 칸. 커서가 넓은 글자의 뒤 칸이면 앞 칸부터다(Ghostty `eraseLine` right)
    fn erase_start_column(&self) -> usize {
        let point = self.grid.cursor.point;
        let column = point.column.0;
        if column > 0 && self.grid[point.line][point.column].flags.contains(Flags::WIDE_CHAR_SPACER)
        {
            column - 1
        } else {
            column
        }
    }

    /// b-terminal(#524): EL 1·ED 1이 지우는 끝 칸의 다음 칸. 커서가 넓은 글자의 앞 칸이면 뒤 칸까지다(Ghostty `eraseLine` left)
    fn erase_end_column(&self) -> usize {
        let point = self.grid.cursor.point;
        let wide = self.grid[point.line][point.column].flags.contains(Flags::WIDE_CHAR);
        cmp::min(point.column.0 + 1 + usize::from(wide), self.columns())
    }

    /// b-terminal(#311): `from..=to` 칸 사이에서 짝을 잃은 넓은 글자 반쪽(뒤 칸 없는 앞 칸, 앞 칸 없는 뒤 칸)을 빈 칸으로 만든다.
    /// 여백 경계에서 칸을 밀거나 지운 뒤에 쓴다(Ghostty `rowWillBeShifted`·`insertBlanks`가 경계의 넓은 글자를 지우는 것과 같은 결과)
    fn repair_wide_cells(&mut self, line: Line, from: usize, to: usize) {
        let columns = self.columns();
        let bg = self.grid.cursor.template.bg;
        for c in from..=to.min(columns - 1) {
            let flags = self.grid[line][Column(c)].flags;
            let orphan = if flags.contains(Flags::WIDE_CHAR) {
                c + 1 >= columns
                    || !self.grid[line][Column(c + 1)].flags.contains(Flags::WIDE_CHAR_SPACER)
            } else if flags.contains(Flags::WIDE_CHAR_SPACER) {
                c == 0 || !self.grid[line][Column(c - 1)].flags.contains(Flags::WIDE_CHAR)
            } else {
                false
            };
            if orphan {
                let wrap = flags.contains(Flags::WRAPLINE);
                let cell = &mut self.grid[line][Column(c)];
                *cell = bg.into();
                cell.flags.set(Flags::WRAPLINE, wrap);
            }
        }
    }

    /// b-terminal(#311): 좌우 여백 사각형 안에서만 줄을 위(`down == false`, 지움)나 아래(삽입)로 `lines`줄 민다.
    /// 여백 밖 칸과 줄 끝의 이어진 줄(WRAPLINE) 표시는 그대로 두고, 비는 칸은 현재 배경색으로 채운다.
    /// 사각형은 `origin`부터 아래 여백까지다(Ghostty `insertLines`·`deleteLines`의 left_right 갈래)
    fn scroll_rect(&mut self, origin: Line, lines: usize, down: bool) {
        self.grapheme_anchor = None;
        let columns = self.columns();
        let left = self.margin_left;
        let right = self.margin_right.min(columns - 1);
        let top = origin.0;
        let bottom = self.scroll_region.end.0;
        let height = (bottom - top).max(0) as usize;
        let count = cmp::min(lines, height);
        if count == 0 {
            return;
        }
        let count = count as i32;
        let bg = self.grid.cursor.template.bg;

        // 옮기기 전에 경계에 걸친 넓은 글자와 줄 끝의 앞 칸 자리 표시를 지운다(Ghostty `rowWillBeShifted`)
        for y in top..bottom {
            let line = Line(y);
            if right + 1 == columns || left < 2 {
                let last = &mut self.grid[line][Column(columns - 1)];
                if last.flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
                    last.flags.remove(Flags::LEADING_WIDE_CHAR_SPACER);
                }
            }
            if left > 0 && self.grid[line][Column(left)].flags.contains(Flags::WIDE_CHAR_SPACER) {
                self.grid[line][Column(left - 1)] = bg.into();
                self.grid[line][Column(left)] = bg.into();
            }
            if right + 1 < columns && self.grid[line][Column(right)].flags.contains(Flags::WIDE_CHAR) {
                self.grid[line][Column(right)] = bg.into();
                self.grid[line][Column(right + 1)] = bg.into();
            }
        }

        let copy_row = |term: &mut Self, from: i32, to: i32| {
            let cells: Vec<Cell> = term.grid[Line(from)][Column(left)..Column(right + 1)].to_vec();
            for (i, cell) in cells.into_iter().enumerate() {
                let dst = &mut term.grid[Line(to)][Column(left + i)];
                let wrap = dst.flags.contains(Flags::WRAPLINE);
                *dst = cell;
                dst.flags.set(Flags::WRAPLINE, wrap);
            }
        };
        let clear_row = |term: &mut Self, y: i32| {
            for c in left..=right {
                let dst = &mut term.grid[Line(y)][Column(c)];
                let wrap = dst.flags.contains(Flags::WRAPLINE);
                *dst = bg.into();
                dst.flags.set(Flags::WRAPLINE, wrap);
            }
        };

        if down {
            for y in (top + count..bottom).rev() {
                copy_row(self, y - count, y);
            }
            for y in top..top + count {
                clear_row(self, y);
            }
        } else {
            for y in top..bottom - count {
                copy_row(self, y + count, y);
            }
            for y in bottom - count..bottom {
                clear_row(self, y);
            }
        }

        self.selection = None;
        self.mark_fully_damaged();
    }

    /// b-terminal(#311): 좌우 여백을 화면 전체 폭으로 되돌린다
    fn reset_margins(&mut self) {
        self.margin_left = 0;
        self.margin_right = self.columns().saturating_sub(1);
    }

    /// Swap primary and alternate screen buffer.
    pub fn swap_alt(&mut self) {
        self.grapheme_anchor = None;
        if !self.mode.contains(TermMode::ALT_SCREEN) {
            // Set alt screen cursor to the current primary screen cursor.
            self.inactive_grid.cursor = self.grid.cursor.clone();

            // Drop information about the primary screens saved cursor.
            // b-terminal(#545): DECSC와 같은 저장(원점 모드, SO·SI로 고른 집합, SS2·SS3까지. Ghostty 1049의 `saveCursor`)
            self.save_cursor_state();

            // Reset alternate screen contents.
            self.inactive_grid.reset_region(..);
            self.swap_alt_grids();
        } else {
            self.swap_alt_grids();
            // b-terminal(#545): 주 화면의 커서 자리·펜은 그 화면에 남아 있다. 저장할 때의 원점 모드와 문자 집합 상태를
            // 되살린다(Ghostty 1049를 끌 때의 `restoreCursor`)
            self.mode.set(TermMode::ORIGIN, self.grid.saved_cursor.origin);
            self.active_charset = self.grid.saved_cursor.active_charset;
            self.single_shift = self.grid.saved_cursor.single_shift;
        }
    }

    /// DECSC(`save_cursor_position`)와 1049가 저장하는 것. Ghostty `saveCursor`(Terminal.zig 2703줄)
    fn save_cursor_state(&mut self) {
        self.grid.saved_cursor = self.grid.cursor.clone();
        // b-terminal(#531): 원점 모드도 함께 저장한다(Ghostty `saveCursor`의 `origin`)
        self.grid.saved_cursor.origin = self.mode.contains(TermMode::ORIGIN);
        // b-terminal(#545): SO·SI로 고른 집합과 대기 중인 한 글자 바꾸기도(Ghostty `saveCursor`의 `charset`)
        self.grid.saved_cursor.active_charset = self.active_charset;
        self.grid.saved_cursor.single_shift = self.single_shift;
    }

    /// b-terminal(#296): 대체 화면을 지우지 않고 바꾼다. 모드 47·1047의 화면 전환(Ghostty `switchScreenMode`).
    /// 커서는 바꾸기 전 화면의 것을 새 화면에 옮긴다. `clear_alt_on_leave`면(1047) 대체 화면을 나갈 때 먼저 지운다
    fn switch_screen_legacy(&mut self, to_alt: bool, clear_alt_on_leave: bool) {
        let in_alt = self.mode.contains(TermMode::ALT_SCREEN);
        if clear_alt_on_leave && !to_alt && in_alt {
            // 대체 화면의 `ClearMode::All`과 같다(`clear_screen`은 `Handler` 구현에만 있다)
            self.grapheme_anchor = None;
            self.grid.reset_region(..);
        }
        if in_alt == to_alt {
            return;
        }
        let cursor = self.grid.cursor.clone();
        self.swap_alt_grids();
        self.grid.cursor = cursor;
    }

    /// 두 화면과 키보드 모드 쌓음을 맞바꾼다(`swap_alt`의 뒷부분, b-terminal #296에서 갈랐다)
    fn swap_alt_grids(&mut self) {
        mem::swap(&mut self.keyboard_mode_stack, &mut self.inactive_keyboard_mode_stack);
        let keyboard_mode =
            self.keyboard_mode_stack.last().copied().unwrap_or(KeyboardModes::NO_MODE).into();
        self.set_keyboard_mode(keyboard_mode, KeyboardModesApplyBehavior::Replace);

        mem::swap(&mut self.grid, &mut self.inactive_grid);
        self.mode ^= TermMode::ALT_SCREEN;
        self.selection = None;
        self.mark_fully_damaged();
    }

    /// Scroll screen down.
    ///
    /// Text moves down; clear at bottom
    /// Expects origin to be in scroll range.
    #[inline]
    fn scroll_down_relative(&mut self, origin: Line, mut lines: usize) {
        self.grapheme_anchor = None;
        trace!("Scrolling down relative: origin={origin}, lines={lines}");

        lines = cmp::min(lines, (self.scroll_region.end - self.scroll_region.start).0 as usize);
        lines = cmp::min(lines, (self.scroll_region.end - origin).0 as usize);

        let region = origin..self.scroll_region.end;

        // Scroll selection.
        self.selection =
            self.selection.take().and_then(|s| s.rotate(self, &region, -(lines as i32)));

        // Scroll vi mode cursor.
        let line = &mut self.vi_mode_cursor.point.line;
        if region.start <= *line && region.end > *line {
            *line = cmp::min(*line + lines, region.end - 1);
        }

        // Scroll between origin and bottom
        self.grid.scroll_down(&region, lines);
        self.mark_fully_damaged();
    }

    /// Scroll screen up
    ///
    /// Text moves up; clear at top
    /// Expects origin to be in scroll range.
    #[inline]
    fn scroll_up_relative(&mut self, origin: Line, lines: usize) {
        self.scroll_up_relative_with(origin, lines, true);
    }

    /// b-terminal(#585): `history`가 거짓이면 스크롤백을 만들지 않는다(DL)
    fn scroll_up_relative_with(&mut self, origin: Line, mut lines: usize, history: bool) {
        self.grapheme_anchor = None;
        trace!("Scrolling up relative: origin={origin}, lines={lines}");

        lines = cmp::min(lines, (self.scroll_region.end - self.scroll_region.start).0 as usize);

        let region = origin..self.scroll_region.end;

        // Scroll selection.
        self.selection = self.selection.take().and_then(|s| s.rotate(self, &region, lines as i32));

        if history {
            self.grid.scroll_up(&region, lines);
        } else {
            self.grid.scroll_up_in_place(&region, lines);
        }

        // Scroll vi mode cursor.
        let viewport_top = Line(-(self.grid.display_offset() as i32));
        let top = if region.start == 0 { viewport_top } else { region.start };
        let line = &mut self.vi_mode_cursor.point.line;
        if (top <= *line) && region.end > *line {
            *line = cmp::max(*line - lines, top);
        }
        self.mark_fully_damaged();
    }

    /// b-terminal(#324): DECCOLM. 모드 40이 꺼져 있으면 모드 3을 끄고 아무것도 하지 않는다. 켜져 있으면 격자를 132·80열로(줄은 그대로)
    /// 바꾸고 화면을 지운 뒤 커서를 1;1로 보낸다(Ghostty Terminal.zig `deccolm` 4837줄). PTY 크기는 알리지 않는다
    fn deccolm(&mut self, wide: bool)
    where
        T: EventListener,
    {
        if !self.mode.contains(TermMode::ENABLE_MODE_3) {
            self.mode.remove(TermMode::COLUMN_132);
            return;
        }
        self.mode.set(TermMode::COLUMN_132, wide);

        let lines = self.screen_lines();
        self.resize_grid(GridDims { columns: if wide { 132 } else { 80 }, lines });

        // Clear grid.
        self.grid.reset_region(..);
        self.goto(0, 0);
        self.mark_fully_damaged();
    }

    /// b-terminal(#324): 모드 40을 켜거나 끌 때. 격자를 창이 정한 크기로 되돌린다(Ghostty stream_handler.zig 732줄)
    fn set_mode_40(&mut self, on: bool) {
        self.mode.set(TermMode::ENABLE_MODE_3, on);
        let (columns, lines) = self.window_size;
        self.resize_grid(GridDims { columns, lines });
    }

    #[inline]
    pub fn exit(&mut self)
    where
        T: EventListener,
    {
        self.event_proxy.send_event(Event::Exit);
    }

    /// Toggle the vi mode.
    #[inline]
    pub fn toggle_vi_mode(&mut self)
    where
        T: EventListener,
    {
        self.mode ^= TermMode::VI;

        if self.mode.contains(TermMode::VI) {
            let display_offset = self.grid.display_offset() as i32;
            if self.grid.cursor.point.line > self.bottommost_line() - display_offset {
                // Move cursor to top-left if terminal cursor is not visible.
                let point = Point::new(Line(-display_offset), Column(0));
                self.vi_mode_cursor = ViModeCursor::new(point);
            } else {
                // Reset vi mode cursor position to match primary cursor.
                self.vi_mode_cursor = ViModeCursor::new(self.grid.cursor.point);
            }
        }

        // Update UI about cursor blinking state changes.
        self.event_proxy.send_event(Event::CursorBlinkingChange);
    }

    /// Move vi mode cursor.
    #[inline]
    pub fn vi_motion(&mut self, motion: ViMotion)
    where
        T: EventListener,
    {
        // Require vi mode to be active.
        if !self.mode.contains(TermMode::VI) {
            return;
        }

        // Move cursor.
        self.vi_mode_cursor = self.vi_mode_cursor.motion(self, motion);
        self.vi_mode_recompute_selection();
    }

    /// Move vi cursor to a point in the grid.
    #[inline]
    pub fn vi_goto_point(&mut self, point: Point)
    where
        T: EventListener,
    {
        // Move viewport to make point visible.
        self.scroll_to_point(point);

        // Move vi cursor to the point.
        self.vi_mode_cursor.point = point;

        self.vi_mode_recompute_selection();
    }

    /// Update the active selection to match the vi mode cursor position.
    #[inline]
    fn vi_mode_recompute_selection(&mut self) {
        // Require vi mode to be active.
        if !self.mode.contains(TermMode::VI) {
            return;
        }

        // Update only if non-empty selection is present.
        if let Some(selection) = self.selection.as_mut().filter(|s| !s.is_empty()) {
            selection.update(self.vi_mode_cursor.point, Side::Left);
            selection.include_all();
        }
    }

    /// Scroll display to point if it is outside of viewport.
    pub fn scroll_to_point(&mut self, point: Point)
    where
        T: EventListener,
    {
        let display_offset = self.grid.display_offset() as i32;
        let screen_lines = self.grid.screen_lines() as i32;

        if point.line < -display_offset {
            let lines = point.line + display_offset;
            self.scroll_display(Scroll::Delta(-lines.0));
        } else if point.line >= (screen_lines - display_offset) {
            let lines = point.line + display_offset - screen_lines + 1i32;
            self.scroll_display(Scroll::Delta(-lines.0));
        }
    }

    /// Jump to the end of a wide cell.
    pub fn expand_wide(&self, mut point: Point, direction: Direction) -> Point {
        let flags = self.grid[point.line][point.column].flags;

        match direction {
            Direction::Right if flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) => {
                point.column = Column(1);
                point.line += 1;
            },
            Direction::Right if flags.contains(Flags::WIDE_CHAR) => {
                point.column = cmp::min(point.column + 1, self.last_column());
            },
            Direction::Left if flags.intersects(Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER) => {
                if flags.contains(Flags::WIDE_CHAR_SPACER) {
                    point.column -= 1;
                }

                let prev = point.sub(self, Boundary::Grid, 1);
                if self.grid[prev].flags.contains(Flags::LEADING_WIDE_CHAR_SPACER) {
                    point = prev;
                }
            },
            _ => (),
        }

        point
    }

    #[inline]
    pub fn semantic_escape_chars(&self) -> &str {
        &self.config.semantic_escape_chars
    }

    #[cfg(test)]
    pub(crate) fn set_semantic_escape_chars(&mut self, semantic_escape_chars: &str) {
        self.config.semantic_escape_chars = semantic_escape_chars.into();
    }

    /// Active terminal cursor style.
    ///
    /// While vi mode is active, this will automatically return the vi mode cursor style.
    #[inline]
    pub fn cursor_style(&self) -> CursorStyle {
        let cursor_style = self.cursor_style.unwrap_or(self.config.default_cursor_style);

        if self.mode.contains(TermMode::VI) {
            self.config.vi_mode_cursor_style.unwrap_or(cursor_style)
        } else {
            cursor_style
        }
    }

    pub fn colors(&self) -> &Colors {
        &self.colors
    }

    /// Insert a linebreak at the current cursor position.
    #[inline]
    fn wrapline(&mut self)
    where
        T: EventListener,
    {
        if !self.mode.contains(TermMode::LINE_WRAP) {
            return;
        }

        trace!("Wrapping input");

        let semantic = self.semantic_state(); // b-terminal(#329)

        // b-terminal(#311): 이어진 줄 표시는 화면 끝 칸에서 넘어갈 때만 한다. 오른쪽 여백에서 넘어가면 표시하지 않는다(Ghostty `printWrap`)
        if self.grid.cursor.point.column.0 + 1 == self.columns() {
            self.grid.cursor_cell().flags.insert(Flags::WRAPLINE);
        }

        if self.grid.cursor.point.line + 1 >= self.scroll_region.end {
            self.linefeed();
        } else {
            self.damage_cursor();
            self.grid.cursor.point.line += 1;
            // b-terminal(#338): 스크롤 없이 한 줄 내려가도 Ghostty `index`처럼 입력 줄을 이어진 프롬프트로 표시한다
            self.semantic_after_index();
        }

        // 줄이 바뀌면 왼쪽 여백으로 간다
        self.grid.cursor.point.column = Column(self.margin_left);
        self.grid.cursor.input_needs_wrap = false;
        self.damage_cursor();
        self.semantic_after_wrap(semantic); // b-terminal(#329)
    }

    /// b-terminal(#294): 소프트 리셋(DECSTR)이 보호 켬과 마지막 보호 방식을 모두 끈다(Ghostty `softReset`).
    pub fn reset_protected(&mut self) {
        self.grid.cursor.template.flags.remove(Flags::PROTECTED);
        self.protected_iso = false;
    }

    /// b-terminal(#302): 소프트 리셋(DECSTR)이 역방향 줄바꿈(?45·?1045)과 KAM(2)을 기본값(끔)으로 되돌린다
    /// (Ghostty `softReset`의 `reset_modes`, Terminal.zig 5919~5931줄).
    pub fn reset_screen_modes_soft(&mut self) {
        self.mode.remove(
            TermMode::REVERSE_WRAP
                | TermMode::REVERSE_WRAP_EXTENDED
                | TermMode::KEYBOARD_ACTION
                | TermMode::MODIFY_OTHER_KEYS_2, // b-terminal(#312): Terminal.zig 5954줄
        );
        // b-terminal(#311): 좌우 여백 모드(69)와 여백도 처음으로(Ghostty `softReset`의 reset_modes와 scrolling_region)
        self.mode.remove(TermMode::LEFT_RIGHT_MARGIN);
        self.reset_margins();
    }

    /// b-terminal(#530): 소프트 리셋(DECSTR)이 되돌리는 나머지 상태. Ghostty `softReset`(Terminal.zig 5918줄)과 같게
    /// 커서 보임·삽입·원점·자동 줄바꿈·커서키·키패드 모드를 기본값으로, 스크롤 영역을 전체로, 펜(SGR 템플릿)과 보호를
    /// 기본으로, 문자 집합 전체를 처음으로 되돌리고 저장한 커서를 지운다(다음 DECRC는 기본값으로 처음 자리).
    /// 활성 화면의 것만 건드린다(Ghostty는 `screens.active`). 마우스·괄호 붙여넣기 같은 다른 모드는 그대로 둔다.
    /// 역방향 줄바꿈·KAM·69·여백·보호 방식은 `reset_screen_modes_soft`와 `reset_protected`가 맡는다
    pub fn reset_state_soft(&mut self) {
        let defaults = TermMode::default();
        let reset = TermMode::SHOW_CURSOR
            | TermMode::INSERT
            | TermMode::ORIGIN
            | TermMode::LINE_WRAP
            | TermMode::APP_CURSOR
            | TermMode::APP_KEYPAD;
        self.mode.remove(reset);
        self.mode.insert(defaults & reset);

        self.scroll_region = Line(0)..Line(self.screen_lines() as i32);

        // 펜: 기본 칸 모양(굵기 같은 SGR 속성과 색, 보호). 활성 커서의 문자 집합 지정도 처음으로
        self.grid.cursor.template = Default::default();
        self.grid.cursor.charsets = Default::default();
        self.active_charset = Default::default();
        self.single_shift = None;

        self.grid.saved_cursor = Default::default();
        self.mark_fully_damaged();
    }

    /// b-terminal(#302): 왼쪽으로 가는 길(Backspace·CUB)이 윗줄로 넘어가야 하는가. Ghostty `cursorLeft`의 `wrap_mode`:
    /// 자동 줄바꿈(?7)이 꺼져 있으면 넘지 않고, 켜져 있으면 ?1045나 ?45 중 하나라도 켜졌을 때 넘는다
    fn reverse_wrap_active(&self) -> bool {
        self.mode.contains(TermMode::LINE_WRAP)
            && self.mode.intersects(TermMode::REVERSE_WRAP | TermMode::REVERSE_WRAP_EXTENDED)
    }

    /// b-terminal(#302): Ghostty `Terminal.cursorLeft`(2592~2696줄)의 역방향 줄바꿈 갈래.
    /// b-terminal(#311): 좌우 여백을 따른다. 왼쪽 여백은 처음 칸이 여백보다 왼쪽이면 0이고, 윗줄로 넘어가면 오른쪽 여백 칸에 놓인다
    fn reverse_wrap_left(&mut self, count: usize) {
        let extended = self.mode.contains(TermMode::REVERSE_WRAP_EXTENDED);
        let cols = self.columns();
        let mut count = count.max(1);
        self.damage_cursor();

        // 대기 중인 줄바꿈이 있으면 xterm처럼 한 걸음을 그것이 쓴다
        if self.grid.cursor.input_needs_wrap {
            count -= 1;
            self.grid.cursor.input_needs_wrap = false;
            if count == 0 {
                return;
            }
        }

        let top = self.scroll_region.start.0;
        let bottom = self.scroll_region.end.0 - 1;
        let right_margin = self.margin_right.min(cols - 1);
        let mut col = self.grid.cursor.point.column.0;
        let mut line = self.grid.cursor.point.line.0;
        let left_margin = if col < self.margin_left { 0 } else { self.margin_left };

        // 왼쪽 여백이고 확장이 아니면 위쪽 여백 위(또는 그 줄)에서는 여백의 왼쪽 위로 가서 끝낸다
        if col == left_margin && !extended && line <= top {
            self.grid.cursor.point.column = Column(left_margin);
            self.grid.cursor.point.line = Line(top);
            self.damage_cursor();
            return;
        }

        loop {
            let amount = (col - left_margin).min(count);
            count -= amount;
            col -= amount;
            if count == 0 {
                break;
            }
            if line == top {
                if !extended {
                    break;
                }
                // 맨 위에서 더 가면 아래 여백의 오른쪽 여백 칸으로 돈다
                col = right_margin;
                line = bottom;
                count -= 1;
                continue;
            }
            // xterm이 비정상 종료하는 자리: Ghostty처럼 (0,0)에서 멈춘다
            if line == 0 {
                break;
            }
            // 확장이 아니면 윗줄이 줄바꿈으로 이어진 줄일 때만 넘는다
            if !extended && !self.grid[Line(line - 1)][Column(cols - 1)].flags.contains(Flags::WRAPLINE) {
                break;
            }
            col = right_margin;
            line -= 1;
            count -= 1;
        }

        self.grid.cursor.point.column = Column(col);
        self.grid.cursor.point.line = Line(line);
        self.damage_cursor();
    }

    /// b-terminal(#294): 보호 칸을 건너뛰고 `range` 칸을 지운다
    fn clear_unprotected(&mut self, line: Line, range: Range<Column>) {
        let bg = self.grid.cursor.template.bg;
        for cell in &mut self.grid[line][range] {
            if !cell.flags.contains(Flags::PROTECTED) {
                *cell = bg.into();
            }
        }
    }

    /// b-terminal(#294): ED·DECSED 중 보호 칸을 남기는 쪽. 화면을 히스토리로 밀지 않고 제자리에서 지운다(Ghostty `clearRows`)
    fn erase_display_keeping_protected(&mut self, mode: ansi::ClearMode) {
        self.grid.cursor.input_needs_wrap = false; // b-terminal(#525): 보호 칸을 남기는 ED도 줄바꿈 대기를 푼다(Ghostty `eraseDisplay`)
        let cursor = self.grid.cursor.point;
        let columns = self.columns();
        let screen_lines = self.screen_lines() as i32;
        match mode {
            ansi::ClearMode::Above => {
                for line in 0..cursor.line.0 {
                    self.clear_unprotected(Line(line), Column(0)..Column(columns));
                }
                let end = Column(self.erase_end_column()); // b-terminal(#524)
                self.clear_unprotected(cursor.line, Column(0)..end);

                let range = Line(0)..=cursor.line;
                self.selection = self.selection.take().filter(|s| !s.intersects_range(range));
            },
            ansi::ClearMode::Below => {
                let start = Column(self.erase_start_column()); // b-terminal(#524)
                self.reset_row_wrap(cursor.line);
                self.clear_unprotected(cursor.line, start..Column(columns));
                for line in (cursor.line.0 + 1)..screen_lines {
                    self.clear_unprotected(Line(line), Column(0)..Column(columns));
                }

                let range = cursor.line..Line(screen_lines);
                self.selection = self.selection.take().filter(|s| !s.intersects_range(range));
            },
            ansi::ClearMode::All => {
                self.semantic_scroll_clear_at_prompt(); // b-terminal(#329): 프롬프트면 먼저 스크롤
                for line in 0..screen_lines {
                    self.clear_unprotected(Line(line), Column(0)..Column(columns));
                }
                self.selection = None;
            },
            // 히스토리는 보호 칸이 없다(clear_screen이 처리한다)
            ansi::ClearMode::Saved => (),
        }

        self.mark_fully_damaged();
    }

    /// b-terminal(#294): EL·DECSEL. `keep_protected`면 보호 칸을 건너뛴다
    fn erase_line(&mut self, mode: ansi::LineClearMode, keep_protected: bool) {
        self.grapheme_anchor = None;
        trace!("Clearing line: {mode:?}");

        let cursor = &self.grid.cursor;
        let bg = cursor.template.bg;
        let point = cursor.point;

        let (left, right) = match mode {
            // b-terminal(#525): 줄바꿈 대기 중이어도 커서 칸(마지막 칸)부터 지운다(Ghostty `eraseLine`). 업스트림은 건너뛰었다
            // b-terminal(#524): 넓은 글자를 반쪽만 지우지 않는다. 오른쪽·전체는 이어진 줄 표시도 푼다(Ghostty `eraseLine`)
            ansi::LineClearMode::Right => {
                let left = self.erase_start_column();
                self.reset_row_wrap(point.line);
                (Column(left), Column(self.columns()))
            },
            ansi::LineClearMode::Left => (Column(0), Column(self.erase_end_column())),
            ansi::LineClearMode::All => {
                self.reset_row_wrap(point.line);
                (Column(0), Column(self.columns()))
            },
        };
        // b-terminal(#525): EL은 어느 방식이든 줄바꿈 대기를 푼다(Ghostty `eraseLine`)
        self.grid.cursor.input_needs_wrap = false;

        self.damage.damage_line(point.line.0 as usize, left.0, right.0 - 1);

        // b-terminal(#582): EL 1은 마지막 칸까지 지워도 줄 이음 표지를 유지한다(Ghostty `eraseLine` left는
        // 줄 표지를 건드리지 않는다, Terminal.zig 17168줄 시험). 칸을 지우면 `WRAPLINE`도 함께 사라지므로 되살린다
        let keep_wrap = matches!(mode, ansi::LineClearMode::Left)
            && self.grid[point.line][Column(self.columns() - 1)].flags.contains(Flags::WRAPLINE);

        if keep_protected {
            self.clear_unprotected(point.line, left..right);
        } else {
            let row = &mut self.grid[point.line];
            for cell in &mut row[left..right] {
                *cell = bg.into();
            }
        }

        if keep_wrap {
            let last = Column(self.columns() - 1);
            self.grid[point.line][last].flags.insert(Flags::WRAPLINE);
        }

        let range = self.grid.cursor.point.line..=self.grid.cursor.point.line;
        self.selection = self.selection.take().filter(|s| !s.intersects_range(range));
    }

    /// Write `c` to the cell at the cursor position.
    #[inline(always)]
    fn write_at_cursor(&mut self, c: char) {
        let c = self.grid.cursor.charsets[self.active_charset].map(c);
        let fg = self.grid.cursor.template.fg;
        let bg = self.grid.cursor.template.bg;
        let flags = self.grid.cursor.template.flags;
        let extra = self.grid.cursor.template.extra.clone();

        let input_content = self.semantic.content == semantic::SemanticContent::Input; // b-terminal(#338)
        let prompt_content = self.semantic.content == semantic::SemanticContent::Prompt; // b-terminal(#700)
        let mut cursor_cell = self.grid.cursor_cell();

        // Clear all related cells when overwriting a fullwidth cell.
        if cursor_cell.flags.intersects(Flags::WIDE_CHAR | Flags::WIDE_CHAR_SPACER) {
            // Remove wide char and spacer.
            let wide = cursor_cell.flags.contains(Flags::WIDE_CHAR);
            let point = self.grid.cursor.point;
            // b-terminal(#580): 짝이 되는 칸은 지금 펜(배경색 포함)의 빈 칸이 된다(Ghostty `writeCell`의
            // `clearCells(.., blankCell())`, Terminal.zig 2310~2349줄). 옛 굵게·배경색을 남기지 않는다.
            let bg = self.grid.cursor.template.bg;
            if wide && point.column < self.last_column() {
                self.blank_wide_partner(point.line, point.column + 1, bg);
            } else if point.column > 0 {
                self.blank_wide_partner(point.line, point.column - 1, bg);
            }

            // Remove leading spacers.
            if point.column <= 1 && point.line != self.topmost_line() {
                let column = self.last_column();
                self.grid[point.line - 1i32][column].flags.remove(Flags::LEADING_WIDE_CHAR_SPACER);
            }

            cursor_cell = self.grid.cursor_cell();
        }

        cursor_cell.c = c;
        cursor_cell.fg = fg;
        cursor_cell.bg = bg;
        // b-terminal(#314): 쓴 칸 표시. b-terminal(#329): 0번 칸에 둔 줄의 프롬프트 표시는 덮어써도 남긴다
        // b-terminal(#582): 줄 이음 표지(`WRAPLINE`)는 줄의 것이라 칸을 덮어써도 남긴다(Ghostty는 칸을 쓸 때 `row.wrap`을
        // 건드리지 않는다. 지우는 것은 EL 0·2, ECH, DL 등 줄 단위 동작뿐이다)
        cursor_cell.flags = flags
            | Flags::WRITTEN
            | (cursor_cell.flags & (Flags::ROW_PROMPT | Flags::ROW_PROMPT_CONTINUATION | Flags::WRAPLINE));
        // b-terminal(#338): 커서의 글 종류가 입력이면 입력 칸으로 쓴다(Ghostty `printCell`의 `semantic_content`)
        if input_content {
            cursor_cell.flags.insert(Flags::SEMANTIC_INPUT);
        }
        // b-terminal(#700): 프롬프트 글이면 프롬프트 칸으로 쓴다(Ghostty `printCell`의 `semantic_content`)
        if prompt_content {
            cursor_cell.flags.insert(Flags::SEMANTIC_PROMPT);
        }
        cursor_cell.extra = extra;
    }

    #[inline]
    fn damage_cursor(&mut self) {
        // The normal cursor coordinates are always in viewport.
        let point =
            Point::new(self.grid.cursor.point.line.0 as usize, self.grid.cursor.point.column);
        self.damage.damage_point(point);
    }

    /// b-terminal(#580): 넓은 글자의 짝 칸을 배경색 `bg`의 빈 칸으로 만든다. 0번 칸의 프롬프트 표시는 남긴다.
    fn blank_wide_partner(&mut self, line: Line, column: Column, bg: Color) {
        let cell = &mut self.grid[line][column];
        let keep = cell.flags & (Flags::ROW_PROMPT | Flags::ROW_PROMPT_CONTINUATION);
        *cell = bg.into();
        cell.flags = keep;
    }

    /// b-terminal 패치(#180): `c`가 방금 찍은 글자와 한 글자 묶음(extended grapheme cluster)이면
    /// 그 칸에 이어 붙이고 `true`. 커서가 방금 찍은 자리 그대로일 때만 본다(CR·커서 이동·지우기·스크롤 뒤에는
    /// 앞 칸과 이어지지 않는다). 이어 붙인 결과가 U+FE0F로 두 칸이 되는 글자(❤️)면 뒤 칸을 채운다.
    fn join_grapheme(&mut self, c: char) -> bool
    where
        T: EventListener,
    {
        // 라틴 글자·ASCII는 묶음을 이루지 않는다(빠른 길)
        if (c as u32) < 0x300 {
            return false;
        }
        let Some(anchor) = self.grapheme_anchor else { return false };
        if anchor.cursor != self.grid.cursor.point || anchor.wrap != self.grid.cursor.input_needs_wrap
        {
            return false;
        }

        let cell = &self.grid[anchor.base];
        if cell.c == ' ' || cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
            return false;
        }
        // b-terminal 패치(#180): 한 칸에 이어 붙이는 글자 수 상한. 이어 붙일 때마다 묶음 전체를 다시 나누므로,
        // 상한이 없으면 결합 문자 n개에 n² 일이 든다(찍힌 글 하나로 엔진을 붙잡는다). 넘으면 원래 동작으로 돌아간다
        if cell.zerowidth().map_or(0, |z| z.len()) >= MAX_GRAPHEME_JOIN {
            return false;
        }
        let mut text = String::with_capacity(16);
        text.push(cell.c);
        if let Some(zerowidth) = cell.zerowidth() {
            text.extend(zerowidth);
        }
        text.push(c);
        // 두 글자 묶음 이상이면 새 묶음의 시작이다
        if text.graphemes(true).nth(1).is_some() {
            return false;
        }

        let was_wide = cell.flags.contains(Flags::WIDE_CHAR);
        let last = cell.zerowidth().and_then(|z| z.last().copied()).unwrap_or(cell.c);
        // b-terminal(#536): 피부색 수식자는 앞 글자가 수식자를 받는 바탕일 때만 붙는다. 아니면 새 글자다(Ghostty 6851줄)
        if is_emoji_modifier(c) && !is_emoji_modifier_base(last) {
            return false;
        }

        // b-terminal(#536): 붙은 글자가 폭에 미치는 영향(Ghostty `graphemeWidthEffect`)
        enum Effect {
            Ignore,
            Wide,
            Narrow,
            NoChange,
        }
        let effect = if c == '\u{FE0F}' || c == '\u{FE0E}' {
            if !is_valid_variation(last) {
                Effect::Ignore
            } else if c == '\u{FE0F}' && !was_wide {
                Effect::Wide
            } else if c == '\u{FE0E}' && was_wide {
                Effect::Narrow
            } else {
                Effect::NoChange
            }
        } else if !was_wide && (text.width() == 2 || c.width().is_some_and(|w| w > 0)) {
            // b-terminal(#581): 묶음에 덧붙는 글자가 폭을 가지면(묶음 안에서 폭 0이 아니면) 묶음은 폭 2다
            // (Ghostty `graphemeWidthEffect`, grapheme.zig 66줄: `!width_zero_in_grapheme`면 `.wide`). 묶음 전체의
            // 합산 폭만 보면 ❤+ZWJ+👍처럼 ZWJ 뒤의 넓은 이모지를 놓친다
            Effect::Wide
        } else {
            Effect::NoChange
        };

        let base = anchor.base;
        let columns = self.columns();
        let margin_limit = self.margin_right.min(columns - 1) + 1;
        let mut right_limit =
            if self.grid.cursor.point.column.0 > self.margin_right { columns } else { margin_limit };
        let mut new_base = base;
        match effect {
            Effect::Ignore => return true,
            Effect::NoChange => (),
            Effect::Narrow => {
                // 넓은 글자가 좁아진다. 뒤 칸은 빈 좁은 칸이 되고 커서는 좁아진 글자 다음 칸으로 온다(Ghostty `.narrow`)
                self.grid[base].flags.remove(Flags::WIDE_CHAR);
                if base.column.0 + 1 < columns {
                    self.grid[base.line][base.column + 1].flags.remove(Flags::WIDE_CHAR_SPACER);
                }
                self.grid.cursor.input_needs_wrap = false;
                self.grid.cursor.point.column = Column((base.column.0 + 1).min(right_limit - 1));
            },
            Effect::Wide if base.column.0 + 1 >= right_limit => {
                // 자리가 없다. 자동 줄바꿈이 꺼져 있으면 아무것도 하지 않고 붙이지도 않는다(Ghostty 2019줄)
                if !self.mode.contains(TermMode::LINE_WRAP) {
                    return true;
                }
                let base_char = cell.c;
                let mut moved: Vec<char> = cell.zerowidth().map(|z| z.to_vec()).unwrap_or_default();
                moved.push(c);
                // 옛 칸은 앞 칸 자리 표시(화면 끝) 또는 빈 칸(여백)이 되고 글자는 다음 줄 왼쪽 여백으로 옮겨 넓어진다.
                // 문자 집합은 다시 거치지 않는다(Ghostty 8156줄)
                self.grid.cursor.point = base;
                if right_limit == columns {
                    self.grid.cursor.template.flags.insert(Flags::LEADING_WIDE_CHAR_SPACER);
                }
                self.write_at_cursor(' ');
                self.grid.cursor.template.flags.remove(Flags::LEADING_WIDE_CHAR_SPACER);
                self.wrapline();
                right_limit = margin_limit;
                new_base = self.grid.cursor.point;
                self.grid.cursor.template.flags.insert(Flags::WIDE_CHAR);
                self.write_at_cursor(' ');
                self.grid.cursor.template.flags.remove(Flags::WIDE_CHAR);
                let moved_cell = &mut self.grid[new_base];
                moved_cell.c = base_char;
                for ch in moved {
                    moved_cell.push_zerowidth(ch);
                }
                self.finish_widen(new_base, right_limit);
                self.set_grapheme_anchor(new_base);
                return true;
            },
            Effect::Wide => {
                self.grid[base].flags.insert(Flags::WIDE_CHAR);
                self.finish_widen(base, right_limit);
            },
        }
        self.grid[new_base].push_zerowidth(c);
        self.set_grapheme_anchor(new_base);
        true
    }

    /// b-terminal(#536): 넓어진 글자(`base`)의 뒤 칸을 쓰고 커서를 그 다음으로 옮긴다. 줄 끝이면 줄바꿈 대기다
    fn finish_widen(&mut self, base: Point, right_limit: usize) {
        self.grid.cursor.point = Point::new(base.line, base.column + 1);
        self.grid.cursor.input_needs_wrap = false;
        self.grid.cursor.template.flags.insert(Flags::WIDE_CHAR_SPACER);
        self.write_at_cursor(' ');
        self.grid.cursor.template.flags.remove(Flags::WIDE_CHAR_SPACER);
        if self.grid.cursor.point.column + 1 < right_limit {
            self.grid.cursor.point.column += 1;
        } else {
            self.grid.cursor.input_needs_wrap = true;
        }
    }

    fn set_grapheme_anchor(&mut self, base: Point) {
        self.grapheme_anchor = Some(GraphemeAnchor {
            base,
            cursor: self.grid.cursor.point,
            wrap: self.grid.cursor.input_needs_wrap,
        });
    }

    #[inline]
    fn set_keyboard_mode(&mut self, mode: TermMode, apply: KeyboardModesApplyBehavior) {
        let active_mode = self.mode & TermMode::KITTY_KEYBOARD_PROTOCOL;
        self.mode &= !TermMode::KITTY_KEYBOARD_PROTOCOL;
        let new_mode = match apply {
            KeyboardModesApplyBehavior::Replace => mode,
            KeyboardModesApplyBehavior::Union => active_mode.union(mode),
            KeyboardModesApplyBehavior::Difference => active_mode.difference(mode),
        };
        trace!("Setting keyboard mode to {new_mode:?}");
        self.mode |= new_mode;
    }
}

impl<T> Dimensions for Term<T> {
    #[inline]
    fn columns(&self) -> usize {
        self.grid.columns()
    }

    #[inline]
    fn screen_lines(&self) -> usize {
        self.grid.screen_lines()
    }

    #[inline]
    fn total_lines(&self) -> usize {
        self.grid.total_lines()
    }
}

impl<T: EventListener> Handler for Term<T> {
    /// A character to be displayed.
    #[inline(never)]
    fn input(&mut self, c: char) {
        // b-terminal(#295): SS2·SS3는 다음 한 글자에만 쓴다. 글자 묶음에 붙는 글자와 폭 0 글자도 한 글자로 센다(Ghostty Terminal.zig 1911줄)
        if let Some(index) = self.single_shift.take() {
            let locked = mem::replace(&mut self.active_charset, index);
            self.input(c);
            self.active_charset = locked;
            return;
        }

        // b-terminal 패치(#180): 글자 묶음 폭. 앞 글자와 한 묶음이면 새 칸 없이 그 칸에 잇는다.
        let grapheme = self.mode.contains(TermMode::GRAPHEME_CLUSTER);
        if grapheme && self.join_grapheme(c) {
            return;
        }

        // Number of cells the char will occupy.
        let width = match c.width() {
            Some(width) => width,
            None => return,
        };
        // b-terminal 패치(#180): 지역 표시 글자(국기)는 한 글자여도 두 칸(Ghostty)
        let width = if grapheme && is_regional_indicator(c) { 2 } else { width };

        // Handle zero-width characters.
        if width == 0 {
            // Get previous column.
            let mut column = self.grid.cursor.point.column;
            if !self.grid.cursor.input_needs_wrap {
                column.0 = column.saturating_sub(1);
            }

            // Put zerowidth characters over first fullwidth character cell.
            let line = self.grid.cursor.point.line;
            if self.grid[line][column].flags.contains(Flags::WIDE_CHAR_SPACER) {
                column.0 = column.saturating_sub(1);
            }

            // b-terminal(#536): 이모지 표시 선택자는 앞 글자가 Extended_Pictographic일 때만 붙인다(Ghostty 2176줄)
            if (c == '\u{FE0F}' || c == '\u{FE0E}')
                && !is_extended_pictographic(self.grid[line][column].c)
            {
                return;
            }

            self.grid[line][column].push_zerowidth(c);
            return;
        }

        // b-terminal(#311): 오른쪽 여백 밖(오른쪽)에서 쓰면 화면 끝까지 쓴다. 줄을 바꾸면 안쪽의 오른쪽 여백으로 돌아온다
        // (Ghostty `print`의 `right_limit`)
        let columns = self.columns();
        let margin_limit = self.margin_right.min(columns - 1) + 1;
        let mut right_limit =
            if self.grid.cursor.point.column.0 > self.margin_right { columns } else { margin_limit };

        // Move cursor to next line.
        if self.grid.cursor.input_needs_wrap {
            self.wrapline();
            right_limit = margin_limit;
        }

        // If in insert mode, first shift cells to the right.
        // b-terminal(#311): 오른쪽 여백까지만 민다. 여백 오른쪽이면 밀지 않는다(Ghostty `insertBlanks(width)`)
        if self.mode.contains(TermMode::INSERT) && self.grid.cursor.point.column + width < columns {
            self.insert_blank(width);
        }

        // b-terminal 패치(#180): 이 글자가 놓인 첫 칸
        let base;
        if width == 1 {
            base = self.grid.cursor.point;
            self.write_at_cursor(c);
        } else if right_limit.saturating_sub(self.margin_left) <= 1 {
            // b-terminal(#523): 넓은 글자가 들어갈 자리가 없다(1열 격자, 한 칸 폭의 여백). 뒤 칸을 쓰면 격자 밖이라 죽는다.
            // 글자를 찍지 않고 커서 칸만 빈칸으로 둔다(Ghostty `print`의 `(right_limit - left) > 1` 아닌 갈래).
            // 아래의 공통 끝에서 커서가 줄 끝이라 줄바꿈 대기가 된다
            base = self.grid.cursor.point;
            self.write_at_cursor(' ');
        } else {
            if self.grid.cursor.point.column + 1 >= right_limit {
                if self.mode.contains(TermMode::LINE_WRAP) {
                    // Insert placeholder before wide char if glyph does not fit in this row.
                    // b-terminal(#311): 앞 칸 자리 표시는 화면의 진짜 끝에서만 둔다. 오른쪽 여백에서는 일반 빈 칸이다(Ghostty print 2200줄)
                    if right_limit == columns {
                        self.grid.cursor.template.flags.insert(Flags::LEADING_WIDE_CHAR_SPACER);
                    }
                    self.write_at_cursor(' ');
                    self.grid.cursor.template.flags.remove(Flags::LEADING_WIDE_CHAR_SPACER);
                    self.wrapline();
                    right_limit = margin_limit;
                } else {
                    // Prevent out of bounds crash when linewrapping is disabled.
                    self.grid.cursor.input_needs_wrap = true;
                    self.grapheme_anchor = None;
                    return;
                }
            }

            // Write full width glyph to current cursor cell.
            base = self.grid.cursor.point;
            self.grid.cursor.template.flags.insert(Flags::WIDE_CHAR);
            self.write_at_cursor(c);
            self.grid.cursor.template.flags.remove(Flags::WIDE_CHAR);

            // Write spacer to cell following the wide glyph.
            self.grid.cursor.point.column += 1;
            self.grid.cursor.template.flags.insert(Flags::WIDE_CHAR_SPACER);
            self.write_at_cursor(' ');
            self.grid.cursor.template.flags.remove(Flags::WIDE_CHAR_SPACER);
        }

        if self.grid.cursor.point.column + 1 < right_limit {
            self.grid.cursor.point.column += 1;
        } else {
            self.grid.cursor.input_needs_wrap = true;
        }

        if grapheme {
            self.grapheme_anchor = Some(GraphemeAnchor {
                base,
                cursor: self.grid.cursor.point,
                wrap: self.grid.cursor.input_needs_wrap,
            });
        }
    }

    #[inline]
    fn decaln(&mut self) {
        trace!("Decalnning");

        for line in (0..self.screen_lines()).map(Line::from) {
            for column in 0..self.columns() {
                let cell = &mut self.grid[line][Column(column)];
                *cell = Cell::default();
                cell.c = 'E';
                cell.flags.insert(Flags::WRITTEN); // b-terminal(#314)
            }
        }

        // b-terminal(#526): 위아래·좌우 여백을 처음으로 돌리고 원점 모드를 끄고 커서를 왼쪽 위로 보낸다(Ghostty `decaln` 4586줄).
        // 69(좌우 여백 모드) 자체는 그대로 둔다
        self.scroll_region = Line(0)..Line(self.screen_lines() as i32);
        self.reset_margins();
        self.mode.remove(TermMode::ORIGIN);
        self.goto(0, 0);

        self.mark_fully_damaged();
    }

    #[inline]
    fn goto(&mut self, line: i32, col: usize) {
        let line = Line(line);
        let col = Column(col);

        trace!("Going to: line={line}, col={col}");
        // b-terminal(#311): 원점 모드에서는 위아래 여백뿐 아니라 좌우 여백 기준이다(Ghostty `setCursorPos` 3235줄)
        let (y_offset, max_y, x_offset, max_x) = if self.mode.contains(TermMode::ORIGIN) {
            (
                self.scroll_region.start,
                self.scroll_region.end - 1,
                self.margin_left,
                self.margin_right.min(self.last_column().0),
            )
        } else {
            (Line(0), self.bottommost_line(), 0, self.last_column().0)
        };

        self.damage_cursor();
        self.grid.cursor.point.line = cmp::max(cmp::min(line + y_offset, max_y), Line(0));
        self.grid.cursor.point.column = Column(cmp::min(col.saturating_add(x_offset), max_x));
        self.damage_cursor();
        self.grid.cursor.input_needs_wrap = false;
    }

    #[inline]
    fn goto_line(&mut self, line: i32) {
        trace!("Going to line: {line}");
        // 원점 모드에서 칸은 왼쪽 여백 기준으로 바꿔 넘긴다(Ghostty `setCursorRow`)
        let column = self.grid.cursor.point.column.0;
        let column = if self.mode.contains(TermMode::ORIGIN) {
            column.saturating_sub(self.margin_left)
        } else {
            column
        };
        self.goto(line, column)
    }

    #[inline]
    fn goto_col(&mut self, col: usize) {
        trace!("Going to column: {col}");
        // 원점 모드에서 줄은 위 여백 기준으로 바꿔 넘긴다(Ghostty `setCursorCol`)
        let line = self.grid.cursor.point.line.0;
        let line = if self.mode.contains(TermMode::ORIGIN) {
            (line - self.scroll_region.start.0).max(0)
        } else {
            line
        };
        self.goto(line, col)
    }

    #[inline]
    fn insert_blank(&mut self, count: usize) {
        self.grapheme_anchor = None;
        // b-terminal(#311): 대기 줄바꿈은 여백 검사보다 먼저 푼다(xterm, Ghostty `insertBlanks`)
        self.grid.cursor.input_needs_wrap = false;
        let cursor = &self.grid.cursor;
        let bg = cursor.template.bg;

        // 커서가 좌우 여백 밖이면 아무것도 안 한다
        let right = self.margin_right.min(self.columns() - 1);
        if cursor.point.column.0 < self.margin_left || cursor.point.column.0 > right {
            return;
        }

        // Ensure inserting within the margins
        let count = cmp::min(count, right + 1 - cursor.point.column.0);

        let source = cursor.point.column;
        let destination = cursor.point.column.0 + count;
        let num_cells = right + 1 - destination;

        let line = cursor.point.line;
        self.damage.damage_line(line.0 as usize, 0, self.columns() - 1);

        let row = &mut self.grid[line][..];

        for offset in (0..num_cells).rev() {
            row.swap(destination + offset, source.0 + offset);
        }

        // Cells were just moved out toward the end of the line;
        // fill in between source and dest with blanks.
        for cell in &mut row[source.0..destination] {
            *cell = bg.into();
        }
        self.repair_wide_cells(line, source.0.saturating_sub(1), right + 1);
    }

    #[inline]
    fn move_up(&mut self, lines: usize) {
        trace!("Moving up: {lines}");

        // b-terminal(#526): 커서가 위 여백 줄이나 그 아래면 위 여백에서 멈추고, 위 여백보다 위에 있으면 화면 위 끝까지 간다(Ghostty `cursorUp` 2543줄)
        let current = self.grid.cursor.point.line;
        let top = self.scroll_region.start;
        let limit = if current >= top { top } else { Line(0) };
        let line = cmp::max(current - lines, limit);
        let column = self.grid.cursor.point.column;
        self.move_to_screen(line.0, column.0)
    }

    #[inline]
    fn move_down(&mut self, lines: usize) {
        trace!("Moving down: {lines}");

        // b-terminal(#526): 커서가 아래 여백 줄이나 그 위면 아래 여백에서 멈추고, 아래 여백보다 아래에 있으면 화면 아래 끝까지 간다(Ghostty `cursorDown` 2561줄)
        let current = self.grid.cursor.point.line;
        let bottom = self.scroll_region.end - 1;
        let limit = if current <= bottom { bottom } else { self.bottommost_line() };
        let line = cmp::min(current + lines, limit);
        let column = self.grid.cursor.point.column;
        self.move_to_screen(line.0, column.0)
    }

    #[inline]
    fn move_forward(&mut self, cols: usize) {
        trace!("Moving forward: {cols}");
        // b-terminal(#311): 오른쪽 여백 안이면 그 여백까지, 여백 오른쪽이면 화면 끝까지(Ghostty `cursorRight`)
        let limit = if self.grid.cursor.point.column.0 <= self.margin_right {
            Column(self.margin_right.min(self.last_column().0))
        } else {
            self.last_column()
        };
        let last_column = cmp::min(self.grid.cursor.point.column + cols, limit);

        let cursor_line = self.grid.cursor.point.line.0 as usize;
        self.damage.damage_line(cursor_line, self.grid.cursor.point.column.0, last_column.0);

        self.grid.cursor.point.column = last_column;
        self.grid.cursor.input_needs_wrap = false;
    }

    #[inline]
    fn move_backward(&mut self, cols: usize) {
        trace!("Moving backward: {cols}");
        // b-terminal(#302): 역방향 줄바꿈(?45·?1045)이 켜져 있으면 윗줄로 넘어간다(Ghostty `cursorLeft`)
        if self.reverse_wrap_active() {
            self.reverse_wrap_left(cols);
            return;
        }
        let column = self.grid.cursor.point.column.saturating_sub(cols).max(self.left_limit());

        let cursor_line = self.grid.cursor.point.line.0 as usize;
        self.damage.damage_line(cursor_line, column, self.grid.cursor.point.column.0);

        self.grid.cursor.point.column = Column(column);
        self.grid.cursor.input_needs_wrap = false;
    }

    #[inline]
    fn identify_terminal(&mut self, intermediate: Option<char>) {
        // b-terminal(#264): DA1·DA2는 bt-term의 스캐너가 Ghostty와 같은 값으로 답한다(설정 clipboard-write를 본다).
        // 엔진이 따로 답하면 PTY에 답이 둘 나가므로 여기서는 답하지 않는다
        let _ = intermediate;
    }

    #[inline]
    fn report_keyboard_mode(&mut self) {
        if !self.config.kitty_keyboard {
            return;
        }

        trace!("Reporting active keyboard mode");
        let current_mode =
            self.keyboard_mode_stack.last().unwrap_or(&KeyboardModes::NO_MODE).bits();
        let text = format!("\x1b[?{current_mode}u");
        self.event_proxy.send_event(Event::PtyWrite(text));
    }

    #[inline]
    fn push_keyboard_mode(&mut self, mode: KeyboardModes) {
        if !self.config.kitty_keyboard {
            return;
        }

        trace!("Pushing `{mode:?}` keyboard mode into the stack");

        if self.keyboard_mode_stack.len() >= KEYBOARD_MODE_STACK_MAX_DEPTH {
            let removed = self.title_stack.remove(0);
            trace!(
                "Removing '{removed:?}' from bottom of keyboard mode stack that exceeds its \
                 maximum depth"
            );
        }

        self.keyboard_mode_stack.push(mode);
        self.set_keyboard_mode(mode.into(), KeyboardModesApplyBehavior::Replace);
    }

    #[inline]
    fn pop_keyboard_modes(&mut self, to_pop: u16) {
        if !self.config.kitty_keyboard {
            return;
        }

        trace!("Attempting to pop {to_pop} keyboard modes from the stack");
        let new_len = self.keyboard_mode_stack.len().saturating_sub(to_pop as usize);
        self.keyboard_mode_stack.truncate(new_len);

        // Reload active mode.
        let mode = self.keyboard_mode_stack.last().copied().unwrap_or(KeyboardModes::NO_MODE);
        self.set_keyboard_mode(mode.into(), KeyboardModesApplyBehavior::Replace);
    }

    /// b-terminal(#312): `CSI > 4 ; 2 m`만 상태 2를 켠다. 나머지는 모두 끈다(Ghostty `setModifyKeyFormat`)
    #[inline]
    fn set_modify_other_keys(&mut self, mode: ansi::ModifyOtherKeys) {
        self.mode.set(TermMode::MODIFY_OTHER_KEYS_2, mode == ansi::ModifyOtherKeys::EnableAll);
    }

    #[inline]
    fn set_keyboard_mode(&mut self, mode: KeyboardModes, apply: KeyboardModesApplyBehavior) {
        if !self.config.kitty_keyboard {
            return;
        }

        self.set_keyboard_mode(mode.into(), apply);
    }

    #[inline]
    fn device_status(&mut self, arg: usize) {
        trace!("Reporting device status: {arg}");
        match arg {
            5 => {
                let text = String::from("\x1b[0n");
                self.event_proxy.send_event(Event::PtyWrite(text));
            },
            6 => {
                let (line, column) = self.cursor_report_pos();
                let text = format!("\x1b[{};{}R", line + 1, column + 1);
                self.event_proxy.send_event(Event::PtyWrite(text));
            },
            _ => debug!("unknown device status query: {arg}"),
        };
    }

    // b-terminal(#327): DECXCPR. 쪽 번호는 늘 1이다(Ghostty stream_handler.zig 889~902줄)
    #[inline]
    fn report_cursor_position_extended(&mut self) {
        let (line, column) = self.cursor_report_pos();
        let text = format!("\x1b[?{};{};1R", line + 1, column + 1);
        self.event_proxy.send_event(Event::PtyWrite(text));
    }

    // b-terminal(#327): XTSHIFTESCAPE
    #[inline]
    fn set_mouse_shift_capture(&mut self, capture: bool) {
        self.mouse_shift_capture = Some(capture);
    }

    // b-terminal(#327): ED 22. 보이는 화면을 스크롤백으로 밀고 지운다(프롬프트의 ED 2와 같은 `scroll_clear_viewport`). Ghostty `scrollClear`는
    // 보호 칸을 가리지 않아 ISO 보호를 잠시 끈다. 대기 줄바꿈은 푼다(Terminal.zig 4459줄).
    // 그림(kitty)은 내용을 따라 스크롤백에 남고 보이는 자리에서만 사라진다(세션이 줄 번호로 쥔다)
    #[inline]
    fn clear_screen_scroll_complete(&mut self) {
        // 빈 화면은 스크롤백에 아무것도 밀지 않는다(`Grid::clear_viewport`는 빈 화면에서도 한 줄을 민다)
        let blank = (0..self.screen_lines() as i32)
            .all(|l| self.grid[Line(l)][..].iter().all(crate::grid::GridCell::is_empty));
        if !blank {
            if self.mode.contains(TermMode::ALT_SCREEN) {
                // 대체 화면은 스크롤백이 없어 지우기만 한다
                let iso = mem::replace(&mut self.protected_iso, false);
                Handler::clear_screen(self, ansi::ClearMode::All);
                self.protected_iso = iso;
            } else {
                // b-terminal(#329): ED 2는 이제 프롬프트에서만 올리므로, ED 22는 늘 올리는 길을 직접 부른다
                self.scroll_clear_viewport();
                self.selection = None;
                self.mark_fully_damaged();
            }
        }
        self.grid.cursor.input_needs_wrap = false;
    }

    #[inline]
    fn move_down_and_cr(&mut self, lines: usize) {
        trace!("Moving down and cr: {lines}");

        let line = self.grid.cursor.point.line + lines;
        let column = self.grid.cursor.point.column;
        self.move_to_screen(line.0, column.0);
        self.carriage_return();
    }

    #[inline]
    fn move_up_and_cr(&mut self, lines: usize) {
        trace!("Moving up and cr: {lines}");

        let line = self.grid.cursor.point.line - lines;
        let column = self.grid.cursor.point.column;
        self.move_to_screen(line.0, column.0);
        self.carriage_return();
    }

    /// Insert tab at cursor position.
    #[inline]
    fn put_tab(&mut self, mut count: u16) {
        // A tab after the last column is the same as a linebreak.
        if self.grid.cursor.input_needs_wrap {
            self.wrapline();
            return;
        }

        while self.grid.cursor.point.column < self.columns() && count != 0 {
            count -= 1;

            let c = self.grid.cursor.charsets[self.active_charset].map('\t');
            let cell = self.grid.cursor_cell();
            if cell.c == ' ' {
                cell.c = c;
            }

            loop {
                // b-terminal(#311): 오른쪽 여백까지만 간다. 여백 오른쪽에서는 움직이지 않는다(Ghostty `horizontalTab` 2958줄)
                if (self.grid.cursor.point.column + 1) == self.columns()
                    || self.grid.cursor.point.column.0 >= self.margin_right
                {
                    break;
                }

                self.grid.cursor.point.column += 1;

                if self.tabs[self.grid.cursor.point.column] {
                    break;
                }
            }
        }
    }

    /// Backspace.
    #[inline]
    fn backspace(&mut self) {
        trace!("Backspace");

        // b-terminal(#302): Ghostty `backspace`는 `cursorLeft(1)`이다
        if self.reverse_wrap_active() {
            self.reverse_wrap_left(1);
            return;
        }

        if self.grid.cursor.point.column > Column(self.left_limit()) {
            let line = self.grid.cursor.point.line.0 as usize;
            let column = self.grid.cursor.point.column.0;
            self.grid.cursor.point.column -= 1;
            self.grid.cursor.input_needs_wrap = false;
            self.damage.damage_line(line, column - 1, column);
        }
    }

    /// Carriage return.
    #[inline]
    fn carriage_return(&mut self) {
        trace!("Carriage return");
        // b-terminal(#311): 원점 모드이거나 왼쪽 여백 오른쪽에 있으면 왼쪽 여백으로, 아니면 0으로(Ghostty `carriageReturn` 2516줄)
        let new_col = if self.mode.contains(TermMode::ORIGIN)
            || self.grid.cursor.point.column.0 >= self.margin_left
        {
            self.margin_left
        } else {
            0
        };
        let line = self.grid.cursor.point.line.0 as usize;
        self.damage.damage_line(line, new_col, self.grid.cursor.point.column.0);
        self.grid.cursor.point.column = Column(new_col);
        self.grid.cursor.input_needs_wrap = false;
    }

    /// Linefeed.
    #[inline]
    fn linefeed(&mut self) {
        trace!("Linefeed");
        // b-terminal(#525): LF·IND는 줄바꿈 대기를 푼다(Ghostty `index` 3021줄). 아래 여백 줄에서 스크롤하지 못하고 멈춰도 푼다
        self.grid.cursor.input_needs_wrap = false;
        let next = self.grid.cursor.point.line + 1;
        if next == self.scroll_region.end {
            // b-terminal(#311): 아래 여백 줄에서는 커서가 좌우 여백 안일 때만 스크롤한다(밖이면 움직이지도 않는다).
            // 좌우 여백이 있으면 사각형만 밀고 스크롤백에 쌓지 않는다(Ghostty `index` 3017줄)
            if self.cursor_in_margins() {
                self.scroll_up(1);
            }
        } else if next < self.screen_lines() {
            self.damage_cursor();
            self.grid.cursor.point.line += 1;
            self.damage_cursor();
        }
        self.semantic_after_index(); // b-terminal(#329)
    }

    /// Set current position as a tabstop.
    #[inline]
    fn bell(&mut self) {
        trace!("Bell");
        self.event_proxy.send_event(Event::Bell);
    }

    #[inline]
    fn substitute(&mut self) {
        trace!("[unimplemented] Substitute");
    }

    /// Run LF/NL.
    ///
    /// LF/NL mode has some interesting history. According to ECMA-48 4th
    /// edition, in LINE FEED mode,
    ///
    /// > The execution of the formatter functions LINE FEED (LF), FORM FEED
    /// > (FF), LINE TABULATION (VT) cause only movement of the active position in
    /// > the direction of the line progression.
    ///
    /// In NEW LINE mode,
    ///
    /// > The execution of the formatter functions LINE FEED (LF), FORM FEED
    /// > (FF), LINE TABULATION (VT) cause movement to the line home position on
    /// > the following line, the following form, etc. In the case of LF this is
    /// > referred to as the New Line (NL) option.
    ///
    /// Additionally, ECMA-48 4th edition says that this option is deprecated.
    /// ECMA-48 5th edition only mentions this option (without explanation)
    /// saying that it's been removed.
    ///
    /// As an emulator, we need to support it since applications may still rely
    /// on it.
    #[inline]
    fn newline(&mut self) {
        self.linefeed();

        if self.mode.contains(TermMode::LINE_FEED_NEW_LINE) {
            self.carriage_return();
        }
    }

    #[inline]
    fn set_horizontal_tabstop(&mut self) {
        trace!("Setting horizontal tabstop");
        self.tabs[self.grid.cursor.point.column] = true;
    }

    // b-terminal(#532): DECST8C(`CSI ? 5 W`). 탭 정지를 `interval`칸 간격 기본으로 되돌린다(Ghostty `tabReset`, Terminal.zig 3001줄)
    #[inline]
    fn set_tabs(&mut self, interval: u16) {
        trace!("Resetting tabstops to every {interval} columns");
        self.tabs = TabStops::with_interval(self.columns(), usize::from(interval).max(1));
    }

    #[inline]
    fn scroll_up(&mut self, lines: usize) {
        let origin = self.scroll_region.start;
        // b-terminal(#311): 좌우 여백이 있으면 여백 사각형만 민다. 커서와 대기 줄바꿈은 그대로다(Ghostty `scrollUp` 3400줄)
        if self.margins_full() {
            self.scroll_up_relative(origin, lines);
        } else {
            self.scroll_rect(origin, lines, false);
        }
    }

    #[inline]
    fn scroll_down(&mut self, lines: usize) {
        let origin = self.scroll_region.start;
        if self.margins_full() {
            self.scroll_down_relative(origin, lines);
        } else {
            self.scroll_rect(origin, lines, true);
        }
    }

    #[inline]
    fn insert_blank_lines(&mut self, lines: usize) {
        trace!("Inserting blank {lines} lines");

        let origin = self.grid.cursor.point.line;
        // b-terminal(#311): 커서가 좌우 여백 밖이면 아무것도 안 한다. 한 뒤에는 커서가 왼쪽 여백으로 가고 대기 줄바꿈이 풀린다
        // (Ghostty `insertLines` 3699줄)
        if self.scroll_region.contains(&origin) && self.cursor_in_margins() {
            if self.margins_full() {
                self.scroll_down_relative(origin, lines);
            } else {
                self.scroll_rect(origin, lines, true);
            }
            self.grid.cursor.point.column = Column(self.margin_left);
            self.grid.cursor.input_needs_wrap = false;
        }
    }

    #[inline]
    fn delete_lines(&mut self, lines: usize) {
        let origin = self.grid.cursor.point.line;
        let lines = cmp::min(self.screen_lines() - origin.0 as usize, lines);

        trace!("Deleting {lines} lines");

        if lines > 0 && self.scroll_region.contains(&origin) && self.cursor_in_margins() {
            if self.margins_full() {
                // b-terminal(#585): DL은 스크롤백을 만들지 않는다(Ghostty `deleteLines`는 영역 안 줄만 옮긴다)
                self.scroll_up_relative_with(origin, lines, false);
                // b-terminal(#582): 전체 폭 영역에서 옮겨진 줄과 지워진 줄은 줄 이음 표지를 잃는다(Ghostty
                // `deleteLines` 3942~3948줄의 `wrap = false`, 지운 줄은 `reset()`). 우리는 표지를 마지막 칸의
                // `WRAPLINE`으로 들므로 커서 줄부터 영역 끝까지 푼다. 좌우 여백이 있으면 줄에 남긴다(아래 `scroll_rect`)
                for y in origin.0..self.scroll_region.end.0 {
                    self.reset_row_wrap(Line(y));
                }
            } else {
                self.scroll_rect(origin, lines, false);
            }
            self.grid.cursor.point.column = Column(self.margin_left);
            self.grid.cursor.input_needs_wrap = false;
        }
    }

    #[inline]
    fn erase_chars(&mut self, count: usize) {
        self.grapheme_anchor = None;
        let cursor = &self.grid.cursor;

        trace!("Erasing chars: count={}, col={}", count, cursor.point.column);

        let start = cursor.point.column;
        let line = cursor.point.line;
        let columns = self.columns();
        // b-terminal(#524): 지우는 구간의 끝이 넓은 글자의 앞 칸이면 뒤 칸까지 지운다. 구간 양 끝의 경계가 넓은 글자를 가르면
        // 그 글자 두 칸을 함께 지운다(Ghostty `eraseChars` 4275줄)
        let remaining = columns - start.0;
        let mut len = cmp::min(remaining, cmp::max(count, 1));
        if len != remaining && self.grid[line][start + (len - 1)].flags.contains(Flags::WIDE_CHAR) {
            len += 1;
        }
        self.split_cell_boundary(line, start.0);
        self.split_cell_boundary(line, start.0 + len);
        self.reset_row_wrap(line);
        self.grid.cursor.input_needs_wrap = false; // b-terminal(#525): ECH는 줄바꿈 대기를 푼다(Ghostty `eraseChars`의 `cursorResetWrap`)
        let end = start + len;

        // Cleared cells have current background color set.
        let bg = self.grid.cursor.template.bg;
        self.damage.damage_line(line.0 as usize, start.0, end.0);
        if self.protected_iso {
            // b-terminal(#294): ISO 보호 칸은 건너뛴다
            self.clear_unprotected(line, start..end);
            return;
        }
        let row = &mut self.grid[line];
        for cell in &mut row[start..end] {
            *cell = bg.into();
        }
    }

    #[inline]
    fn delete_chars(&mut self, count: usize) {
        self.grapheme_anchor = None;
        let columns = self.columns();
        let cursor = &self.grid.cursor;
        let bg = cursor.template.bg;

        // b-terminal(#311): 커서가 좌우 여백 밖이면 아무것도 안 하고, 안이면 오른쪽 여백까지만 당긴다(Ghostty `deleteChars`)
        let right = self.margin_right.min(columns - 1);
        let start = cursor.point.column.0;
        if start < self.margin_left || start > right {
            return;
        }

        // Ensure deleting within the margins.
        let count = cmp::min(count, right + 1 - start);

        let line = cursor.point.line;
        self.damage.damage_line(line.0 as usize, 0, self.columns() - 1);

        // b-terminal(#524): 지울 구간(`start..start + count`)과 오른쪽 여백 바깥의 경계가 넓은 글자를 가르면 그 글자 두 칸을 함께 지운다.
        // 옮기기 전에 하고, 이어진 줄 표시도 푼다(Ghostty `deleteChars` 4133줄)
        self.split_cell_boundary(line, start);
        self.split_cell_boundary(line, start + count);
        self.split_cell_boundary(line, right + 1);
        self.reset_row_wrap(line);
        self.grid.cursor.input_needs_wrap = false; // b-terminal(#525): DCH는 줄바꿈 대기를 푼다(Ghostty `deleteChars`의 `cursorResetWrap`). 여백 밖이면 위에서 이미 돌아갔다
        let row = &mut self.grid[line][..];

        for offset in 0..(right + 1 - start - count) {
            row.swap(start + offset, start + count + offset);
        }

        // Clear the last `count` cells of the margin range.
        for cell in &mut row[right + 1 - count..=right] {
            *cell = bg.into();
        }
        self.repair_wide_cells(line, start.saturating_sub(1), right + 1);
    }

    #[inline]
    fn move_backward_tabs(&mut self, count: u16) {
        trace!("Moving backward {count} tabs");

        let old_col = self.grid.cursor.point.column.0;
        for _ in 0..count {
            let mut col = self.grid.cursor.point.column;

            // b-terminal(#311): 원점 모드에서는 왼쪽 여백이 한계다. 한계 안에서 탭 자리가 없으면 한계까지 간다(Ghostty `horizontalTabBack` 2971줄)
            let left_limit = if self.mode.contains(TermMode::ORIGIN) { self.margin_left } else { 0 };
            if col.0 <= left_limit {
                break;
            }

            col = index::Column(left_limit);
            for i in (left_limit..(self.grid.cursor.point.column.0)).rev() {
                if self.tabs[index::Column(i)] {
                    col = index::Column(i);
                    break;
                }
            }
            self.grid.cursor.point.column = col;
        }

        let line = self.grid.cursor.point.line.0 as usize;
        self.damage.damage_line(line, self.grid.cursor.point.column.0, old_col);
    }

    #[inline]
    fn move_forward_tabs(&mut self, count: u16) {
        trace!("Moving forward {count} tabs");

        let num_cols = self.columns();
        let old_col = self.grid.cursor.point.column.0;
        for _ in 0..count {
            let mut col = self.grid.cursor.point.column;

            // b-terminal(#311): 오른쪽 여백까지만 간다(Ghostty `horizontalTab`)
            if col == num_cols - 1 || col.0 >= self.margin_right {
                break;
            }

            for i in col.0 + 1..=self.margin_right.min(num_cols - 1) {
                col = index::Column(i);
                if self.tabs[col] {
                    break;
                }
            }

            self.grid.cursor.point.column = col;
        }

        let line = self.grid.cursor.point.line.0 as usize;
        self.damage.damage_line(line, old_col, self.grid.cursor.point.column.0);
    }

    #[inline]
    fn save_cursor_position(&mut self) {
        trace!("Saving cursor position");

        self.save_cursor_state();
    }

    #[inline]
    fn restore_cursor_position(&mut self) {
        trace!("Restoring cursor position");

        self.damage_cursor();
        self.grid.cursor = self.grid.saved_cursor.clone();
        // b-terminal(#531): 원점 모드를 저장할 때의 값으로 되돌린다. 저장한 것이 없으면 기본 커서의 값(끔)이고
        // 처음 자리다(Ghostty `restoreCursor`의 기본값, Terminal.zig 2719줄)
        self.mode.set(TermMode::ORIGIN, self.grid.saved_cursor.origin);
        self.grid.cursor.origin = false;
        // b-terminal(#545): 저장한 것이 없으면 기본 커서의 값(G0, 대기 없음)이다(Ghostty `.charset = .{}`)
        self.active_charset = self.grid.saved_cursor.active_charset;
        self.single_shift = self.grid.saved_cursor.single_shift;
        self.grid.cursor.active_charset = Default::default();
        self.grid.cursor.single_shift = None;
        self.damage_cursor();
    }

    #[inline]
    fn clear_line(&mut self, mode: ansi::LineClearMode) {
        // b-terminal(#294): 마지막 보호 방식이 ISO면 보호 칸을 건너뛴다
        self.erase_line(mode, self.protected_iso);
    }

    // b-terminal(#294): DECSEL은 보호 방식과 상관없이 보호 칸을 남긴다
    #[inline]
    fn selective_clear_line(&mut self, mode: ansi::LineClearMode) {
        self.erase_line(mode, true);
    }

    // b-terminal(#294): DECSED
    #[inline]
    fn selective_clear_screen(&mut self, mode: ansi::ClearMode) {
        self.grapheme_anchor = None;
        trace!("Selective clearing screen: {mode:?}");
        match mode {
            // 히스토리는 보호 칸이 없어 ED 3과 같다
            ansi::ClearMode::Saved => Handler::clear_screen(self, mode),
            _ => self.erase_display_keeping_protected(mode),
        }
    }

    // b-terminal(#294): DECSCA·SPA·EPA. Ghostty `setProtectedMode`: 끔은 마지막 방식을 지우지 않는다
    #[inline]
    fn set_protected_mode(&mut self, mode: ProtectedMode) {
        trace!("Setting protected mode: {mode:?}");
        let flags = &mut self.grid.cursor.template.flags;
        match mode {
            ProtectedMode::Off => flags.remove(Flags::PROTECTED),
            ProtectedMode::Iso => {
                flags.insert(Flags::PROTECTED);
                self.protected_iso = true;
            },
            ProtectedMode::Dec => {
                flags.insert(Flags::PROTECTED);
                self.protected_iso = false;
            },
        }
    }

    /// Set the indexed color value.
    #[inline]
    fn set_color(&mut self, index: usize, color: Rgb) {
        trace!("Setting color[{index}] = {color:?}");

        // Damage terminal if the color changed and it's not the cursor.
        if index != NamedColor::Cursor as usize && self.colors[index] != Some(color) {
            self.mark_fully_damaged();
        }

        self.colors[index] = Some(color);
    }

    /// Respond to a color query escape sequence.
    #[inline]
    fn dynamic_color_sequence(&mut self, prefix: String, index: usize, terminator: &str) {
        trace!("Requested write of escape sequence for color code {prefix}: color[{index}]");

        let terminator = terminator.to_owned();
        self.event_proxy.send_event(Event::ColorRequest(
            index,
            self.colors[index], // b-terminal(#272): 프로그램이 바꾼 색(없으면 None)을 사건에 싣는다
            // b-terminal(#286): 커서 색(OSC 12)은 정하지 않았으면 지금 글자색을 따르므로 바꾼 글자색도 싣는다
            if index == NamedColor::Cursor as usize { self.colors[NamedColor::Foreground as usize] } else { None },
            Arc::new(move |color| {
                format!(
                    "\x1b]{};rgb:{1:02x}{1:02x}/{2:02x}{2:02x}/{3:02x}{3:02x}{4}",
                    prefix, color.r, color.g, color.b, terminator
                )
            }),
        ));
    }

    /// Reset the indexed color to original value.
    #[inline]
    fn reset_color(&mut self, index: usize) {
        trace!("Resetting color[{index}]");

        // Damage terminal if the color changed and it's not the cursor.
        if index != NamedColor::Cursor as usize && self.colors[index].is_some() {
            self.mark_fully_damaged();
        }

        self.colors[index] = None;
    }

    /// Store data into clipboard.
    #[inline]
    fn clipboard_store(&mut self, clipboard: u8, base64: &[u8]) {
        if !matches!(self.config.osc52, Osc52::OnlyCopy | Osc52::CopyPaste) {
            debug!("Denied osc52 store");
            return;
        }

        let clipboard_type = match clipboard {
            b'c' => ClipboardType::Clipboard,
            b'p' | b's' => ClipboardType::Selection,
            _ => return,
        };

        if let Ok(bytes) = Base64.decode(base64) {
            if let Ok(text) = String::from_utf8(bytes) {
                self.event_proxy.send_event(Event::ClipboardStore(clipboard_type, text));
            }
        }
    }

    /// Load data from clipboard.
    #[inline]
    fn clipboard_load(&mut self, clipboard: u8, terminator: &str) {
        if !matches!(self.config.osc52, Osc52::OnlyPaste | Osc52::CopyPaste) {
            debug!("Denied osc52 load");
            return;
        }

        let clipboard_type = match clipboard {
            b'c' => ClipboardType::Clipboard,
            b'p' | b's' => ClipboardType::Selection,
            _ => return,
        };

        let terminator = terminator.to_owned();

        self.event_proxy.send_event(Event::ClipboardLoad(
            clipboard_type,
            Arc::new(move |text| {
                let base64 = Base64.encode(text);
                format!("\x1b]52;{};{}{}", clipboard as char, base64, terminator)
            }),
        ));
    }

    #[inline]
    fn clear_screen(&mut self, mode: ansi::ClearMode) {
        self.grapheme_anchor = None;
        trace!("Clearing screen: {mode:?}");

        // b-terminal(#294): 마지막 보호 방식이 ISO면 보호 칸을 건너뛴다(히스토리 지우기는 그대로)
        if self.protected_iso && !matches!(mode, ansi::ClearMode::Saved) {
            self.erase_display_keeping_protected(mode);
            return;
        }

        let bg = self.grid.cursor.template.bg;

        let screen_lines = self.screen_lines();

        // b-terminal(#525): ED 0·1·2는 줄바꿈 대기를 푼다(Ghostty `eraseDisplay`). 히스토리만 지우는 ED 3은 그대로다
        if !matches!(mode, ansi::ClearMode::Saved) {
            self.grid.cursor.input_needs_wrap = false;
        }

        match mode {
            ansi::ClearMode::Above => {
                let cursor = self.grid.cursor.point;

                // If clearing more than one line.
                // b-terminal(#526): 커서가 둘째 줄이어도 첫 줄을 지운다(업스트림은 `> 1`이라 남겼다). Ghostty `eraseDisplay` above는 `cursor.y > 0`
                if cursor.line > 0 {
                    // Fully clear all lines before the current line.
                    self.grid.reset_region(..cursor.line);
                }

                // Clear up to the current column in the current line.
                // b-terminal(#524): 커서가 넓은 글자의 앞 칸이면 뒤 칸까지 지운다(Ghostty `eraseLine` left)
                let end = Column(self.erase_end_column());
                for cell in &mut self.grid[cursor.line][..end] {
                    *cell = bg.into();
                }

                let range = Line(0)..=cursor.line;
                self.selection = self.selection.take().filter(|s| !s.intersects_range(range));
            },
            ansi::ClearMode::Below => {
                let cursor = self.grid.cursor.point;
                // b-terminal(#524): 커서가 넓은 글자의 뒤 칸이면 앞 칸부터 지운다. 이어진 줄 표시도 푼다(Ghostty `eraseDisplay` below는 `eraseLine` right)
                let start = self.erase_start_column();
                self.reset_row_wrap(cursor.line);
                for cell in &mut self.grid[cursor.line][Column(start)..] {
                    *cell = bg.into();
                }

                if (cursor.line.0 as usize) < screen_lines - 1 {
                    self.grid.reset_region((cursor.line + 1)..);
                }

                let range = cursor.line..Line(screen_lines as i32);
                self.selection = self.selection.take().filter(|s| !s.intersects_range(range));
            },
            ansi::ClearMode::All => {
                // b-terminal(#329): 프롬프트에서 받았을 때만 화면을 스크롤백으로 올린다(Ghostty `eraseDisplay`의 "Clearing at a prompt").
                // 그렇지 않으면 스크롤백에 쌓지 않고 지운다(alacritty 원래 동작은 언제나 올렸다)
                if !self.semantic_scroll_clear_at_prompt() {
                    self.grid.reset_region(..);
                }

                self.selection = None;
            },
            ansi::ClearMode::Saved if self.history_size() > 0 => {
                self.grid.clear_history();

                self.vi_mode_cursor.point.line =
                    self.vi_mode_cursor.point.line.grid_clamp(self, Boundary::Cursor);

                self.selection = self.selection.take().filter(|s| !s.intersects_range(..Line(0)));
            },
            // We have no history to clear.
            ansi::ClearMode::Saved => (),
        }

        self.mark_fully_damaged();
    }

    #[inline]
    fn clear_tabs(&mut self, mode: ansi::TabulationClearMode) {
        trace!("Clearing tabs: {mode:?}");
        match mode {
            ansi::TabulationClearMode::Current => {
                self.tabs[self.grid.cursor.point.column] = false;
            },
            ansi::TabulationClearMode::All => {
                self.tabs.clear_all();
            },
        }
    }

    /// Reset all important fields in the term struct.
    #[inline]
    fn reset_state(&mut self) {
        if self.mode.contains(TermMode::ALT_SCREEN) {
            mem::swap(&mut self.grid, &mut self.inactive_grid);
        }
        self.active_charset = Default::default();
        self.single_shift = None;
        self.cursor_style = None;
        self.protected_iso = false; // b-terminal(#294)
        self.mouse_shift_capture = None; // b-terminal(#327)
        self.semantic_reset(); // b-terminal(#329)
        self.grid.reset();
        self.inactive_grid.reset();
        self.scroll_region = Line(0)..Line(self.screen_lines() as i32);
        self.reset_margins(); // b-terminal(#311)
        self.tabs = TabStops::new(self.columns());
        self.title_stack = Vec::new();
        self.saved_private_modes.clear(); // b-terminal(#313)
        self.title = None;
        self.selection = None;
        self.vi_mode_cursor = Default::default();
        self.keyboard_mode_stack = Default::default();
        self.inactive_keyboard_mode_stack = Default::default();

        // Preserve vi mode across resets.
        self.mode &= TermMode::VI;
        self.mode.insert(TermMode::default());
        self.mode.set(TermMode::GRAPHEME_CLUSTER, self.config.grapheme_cluster);
        self.grapheme_anchor = None;

        self.event_proxy.send_event(Event::CursorBlinkingChange);
        self.mark_fully_damaged();
    }

    #[inline]
    fn reverse_index(&mut self) {
        trace!("Reversing index");
        // If cursor is at the top.
        if self.grid.cursor.point.line == self.scroll_region.start {
            // b-terminal(#311): 좌우 여백 밖이면 스크롤하지 않고 더 위로도 가지 않는다(Ghostty `reverseIndex` 3217줄)
            if self.cursor_in_margins() {
                self.scroll_down(1);
            }
        } else {
            self.damage_cursor();
            self.grid.cursor.point.line = cmp::max(self.grid.cursor.point.line - 1, Line(0));
            self.damage_cursor();
        }
    }

    #[inline]
    fn set_hyperlink(&mut self, hyperlink: Option<Hyperlink>) {
        trace!("Setting hyperlink: {hyperlink:?}");
        self.grid.cursor.template.set_hyperlink(hyperlink.map(|e| e.into()));
    }

    /// Set a terminal attribute.
    #[inline]
    fn terminal_attribute(&mut self, attr: Attr) {
        trace!("Setting attribute: {attr:?}");
        let cursor = &mut self.grid.cursor;
        match attr {
            Attr::Foreground(color) => cursor.template.fg = color,
            Attr::Background(color) => cursor.template.bg = color,
            Attr::UnderlineColor(color) => cursor.template.set_underline_color(color),
            Attr::Reset => {
                cursor.template.fg = Color::Named(NamedColor::Foreground);
                cursor.template.bg = Color::Named(NamedColor::Background);
                // b-terminal(#294): 보호(DECSCA)는 SGR이 아니라 SGR 0이 지우지 않는다
                cursor.template.flags &= Flags::PROTECTED;
                cursor.template.set_underline_color(None);
            },
            Attr::Reverse => cursor.template.flags.insert(Flags::INVERSE),
            Attr::CancelReverse => cursor.template.flags.remove(Flags::INVERSE),
            Attr::Bold => cursor.template.flags.insert(Flags::BOLD),
            Attr::CancelBold => cursor.template.flags.remove(Flags::BOLD),
            Attr::Dim => cursor.template.flags.insert(Flags::DIM),
            Attr::CancelBoldDim => cursor.template.flags.remove(Flags::BOLD | Flags::DIM),
            Attr::Italic => cursor.template.flags.insert(Flags::ITALIC),
            Attr::CancelItalic => cursor.template.flags.remove(Flags::ITALIC),
            Attr::Underline => {
                cursor.template.flags.remove(Flags::ALL_UNDERLINES);
                cursor.template.flags.insert(Flags::UNDERLINE);
            },
            Attr::DoubleUnderline => {
                cursor.template.flags.remove(Flags::ALL_UNDERLINES);
                cursor.template.flags.insert(Flags::DOUBLE_UNDERLINE);
            },
            Attr::Undercurl => {
                cursor.template.flags.remove(Flags::ALL_UNDERLINES);
                cursor.template.flags.insert(Flags::UNDERCURL);
            },
            Attr::DottedUnderline => {
                cursor.template.flags.remove(Flags::ALL_UNDERLINES);
                cursor.template.flags.insert(Flags::DOTTED_UNDERLINE);
            },
            Attr::DashedUnderline => {
                cursor.template.flags.remove(Flags::ALL_UNDERLINES);
                cursor.template.flags.insert(Flags::DASHED_UNDERLINE);
            },
            Attr::CancelUnderline => cursor.template.flags.remove(Flags::ALL_UNDERLINES),
            Attr::Hidden => cursor.template.flags.insert(Flags::HIDDEN),
            Attr::CancelHidden => cursor.template.flags.remove(Flags::HIDDEN),
            Attr::Strike => cursor.template.flags.insert(Flags::STRIKEOUT),
            Attr::CancelStrike => cursor.template.flags.remove(Flags::STRIKEOUT),
            // b-terminal(#277): 윗줄(SGR 53·55). Reset은 `flags = empty()`가 함께 지운다
            Attr::Overline => cursor.template.flags.insert(Flags::OVERLINE),
            Attr::CancelOverline => cursor.template.flags.remove(Flags::OVERLINE),
            // b-terminal(#287): 깜박임(SGR 5·6, Ghostty는 둘 다 blink). 25가 끈다. 그리지는 않는다
            Attr::BlinkSlow | Attr::BlinkFast => cursor.template.flags.insert(Flags::BLINK),
            Attr::CancelBlink => cursor.template.flags.remove(Flags::BLINK),
            // b-terminal(#287): 이제 vte의 모든 속성을 다뤄 `_` 갈래가 닿지 않는다(unreachable_patterns 경고). vte가 속성을 더하면 여기서 컴파일이 멈춘다
        }
    }

    #[inline]
    fn set_private_mode(&mut self, mode: PrivateMode) {
        let mode = match mode {
            PrivateMode::Named(mode) => mode,
            PrivateMode::Unknown(GRAPHEME_CLUSTER_MODE) => {
                self.mode.insert(TermMode::GRAPHEME_CLUSTER);
                return;
            },
            // b-terminal(#301): 키 입력 모드
            PrivateMode::Unknown(mode) if key_input_mode_bit(mode).is_some() => {
                self.mode.insert(key_input_mode_bit(mode).unwrap_or(TermMode::NONE));
                return;
            },
            // b-terminal(#311): DECLRMM
            PrivateMode::Unknown(LEFT_RIGHT_MARGIN_MODE) => {
                self.mode.insert(TermMode::LEFT_RIGHT_MARGIN);
                return;
            },
            // b-terminal(#324): 모드 40
            PrivateMode::Unknown(ENABLE_MODE_3_MODE) => {
                self.set_mode_40(true);
                return;
            },
            PrivateMode::Unknown(mode) => {
                debug!("Ignoring unknown mode {mode} in set_private_mode");
                return;
            },
        };

        trace!("Setting private mode: {mode:?}");
        match mode {
            NamedPrivateMode::UrgencyHints => self.mode.insert(TermMode::URGENCY_HINTS),
            NamedPrivateMode::SwapScreenAndSetRestoreCursor => {
                if !self.mode.contains(TermMode::ALT_SCREEN) {
                    self.swap_alt();
                }
            },
            NamedPrivateMode::ShowCursor => self.mode.insert(TermMode::SHOW_CURSOR),
            NamedPrivateMode::CursorKeys => self.mode.insert(TermMode::APP_CURSOR),
            // Mouse protocols are mutually exclusive.
            NamedPrivateMode::ReportMouseClicks => {
                self.mode.remove(TermMode::MOUSE_MODE);
                self.mode.insert(TermMode::MOUSE_REPORT_CLICK);
                self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            NamedPrivateMode::ReportCellMouseMotion => {
                self.mode.remove(TermMode::MOUSE_MODE);
                self.mode.insert(TermMode::MOUSE_DRAG);
                self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            NamedPrivateMode::ReportAllMouseMotion => {
                self.mode.remove(TermMode::MOUSE_MODE);
                self.mode.insert(TermMode::MOUSE_MOTION);
                self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            NamedPrivateMode::ReportFocusInOut => self.mode.insert(TermMode::FOCUS_IN_OUT),
            NamedPrivateMode::BracketedPaste => self.mode.insert(TermMode::BRACKETED_PASTE),
            // Mouse encodings are mutually exclusive.
            // b-terminal(#300): 형식은 하나만, 마지막에 켠 것이 이긴다(Ghostty stream_handler.zig)
            NamedPrivateMode::SgrMouse => {
                self.mode.remove(TermMode::MOUSE_FORMAT);
                self.mode.insert(TermMode::SGR_MOUSE);
            },
            NamedPrivateMode::Utf8Mouse => {
                self.mode.remove(TermMode::MOUSE_FORMAT);
                self.mode.insert(TermMode::UTF8_MOUSE);
            },
            NamedPrivateMode::UrxvtMouse => {
                self.mode.remove(TermMode::MOUSE_FORMAT);
                self.mode.insert(TermMode::URXVT_MOUSE);
            },
            NamedPrivateMode::SgrPixelsMouse => {
                self.mode.remove(TermMode::MOUSE_FORMAT);
                self.mode.insert(TermMode::SGR_PIXELS_MOUSE);
            },
            NamedPrivateMode::X10Mouse => {
                self.mode.remove(TermMode::MOUSE_MODE);
                self.mode.insert(TermMode::MOUSE_X10);
                self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            NamedPrivateMode::AlternateScroll => self.mode.insert(TermMode::ALTERNATE_SCROLL),
            NamedPrivateMode::LineWrap => self.mode.insert(TermMode::LINE_WRAP),
            NamedPrivateMode::Origin => {
                self.mode.insert(TermMode::ORIGIN);
                self.goto(0, 0);
            },
            NamedPrivateMode::ColumnMode => self.deccolm(true),
            NamedPrivateMode::BlinkingCursor => {
                let style = self.cursor_style.get_or_insert(self.config.default_cursor_style);
                style.blinking = true;
                self.event_proxy.send_event(Event::CursorBlinkingChange);
            },
            // b-terminal(#296): 상태만 적는다. 실제 모으기는 vte의 Processor가 한다
            NamedPrivateMode::SyncUpdate => self.mode.insert(TermMode::SYNC_UPDATE),
            NamedPrivateMode::SwapScreenLegacy => self.switch_screen_legacy(true, false),
            NamedPrivateMode::SwapScreenAndClear => self.switch_screen_legacy(true, true),
            NamedPrivateMode::SaveRestoreCursor => {
                self.mode.insert(TermMode::SAVE_CURSOR_MODE);
                self.save_cursor_position();
            },
            NamedPrivateMode::KeypadApplication => self.mode.insert(TermMode::APP_KEYPAD),
            // b-terminal(#302)
            NamedPrivateMode::ReverseColors => {
                self.mode.insert(TermMode::REVERSE_COLORS);
                self.mark_fully_damaged();
            },
            NamedPrivateMode::ReverseWrap => self.mode.insert(TermMode::REVERSE_WRAP),
            NamedPrivateMode::ReverseWrapExtended => self.mode.insert(TermMode::REVERSE_WRAP_EXTENDED),
            NamedPrivateMode::SlowScroll => self.mode.insert(TermMode::SLOW_SCROLL),
            NamedPrivateMode::Autorepeat => self.mode.insert(TermMode::AUTOREPEAT),
        }
    }

    #[inline]
    fn unset_private_mode(&mut self, mode: PrivateMode) {
        let mode = match mode {
            PrivateMode::Named(mode) => mode,
            PrivateMode::Unknown(GRAPHEME_CLUSTER_MODE) => {
                self.mode.remove(TermMode::GRAPHEME_CLUSTER);
                self.grapheme_anchor = None;
                return;
            },
            // b-terminal(#301): 키 입력 모드
            PrivateMode::Unknown(mode) if key_input_mode_bit(mode).is_some() => {
                self.mode.remove(key_input_mode_bit(mode).unwrap_or(TermMode::NONE));
                return;
            },
            // b-terminal(#311): DECLRMM을 끄면 여백을 화면 전체 폭으로 되돌린다(Ghostty stream_handler.zig 700줄)
            PrivateMode::Unknown(LEFT_RIGHT_MARGIN_MODE) => {
                self.mode.remove(TermMode::LEFT_RIGHT_MARGIN);
                self.reset_margins();
                return;
            },
            // b-terminal(#324): 모드 40
            PrivateMode::Unknown(ENABLE_MODE_3_MODE) => {
                self.set_mode_40(false);
                return;
            },
            PrivateMode::Unknown(mode) => {
                debug!("Ignoring unknown mode {mode} in unset_private_mode");
                return;
            },
        };

        trace!("Unsetting private mode: {mode:?}");
        match mode {
            NamedPrivateMode::UrgencyHints => self.mode.remove(TermMode::URGENCY_HINTS),
            NamedPrivateMode::SwapScreenAndSetRestoreCursor => {
                if self.mode.contains(TermMode::ALT_SCREEN) {
                    self.swap_alt();
                }
            },
            NamedPrivateMode::ShowCursor => self.mode.remove(TermMode::SHOW_CURSOR),
            NamedPrivateMode::CursorKeys => self.mode.remove(TermMode::APP_CURSOR),
            NamedPrivateMode::ReportMouseClicks => {
                self.mode.remove(TermMode::MOUSE_REPORT_CLICK);
                self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            NamedPrivateMode::ReportCellMouseMotion => {
                self.mode.remove(TermMode::MOUSE_DRAG);
                self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            NamedPrivateMode::ReportAllMouseMotion => {
                self.mode.remove(TermMode::MOUSE_MOTION);
                self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            NamedPrivateMode::ReportFocusInOut => self.mode.remove(TermMode::FOCUS_IN_OUT),
            NamedPrivateMode::BracketedPaste => self.mode.remove(TermMode::BRACKETED_PASTE),
            // b-terminal(#300): Ghostty는 어느 형식을 끄든 X10 형식으로 돌아간다
            NamedPrivateMode::SgrMouse
            | NamedPrivateMode::Utf8Mouse
            | NamedPrivateMode::UrxvtMouse
            | NamedPrivateMode::SgrPixelsMouse => self.mode.remove(TermMode::MOUSE_FORMAT),
            NamedPrivateMode::X10Mouse => {
                self.mode.remove(TermMode::MOUSE_X10);
                self.event_proxy.send_event(Event::MouseCursorDirty);
            },
            NamedPrivateMode::AlternateScroll => self.mode.remove(TermMode::ALTERNATE_SCROLL),
            NamedPrivateMode::LineWrap => self.mode.remove(TermMode::LINE_WRAP),
            // b-terminal(#311): 끌 때도 커서를 원점으로 보낸다(Ghostty stream_handler.zig 698줄: 켜고 끌 때 `setCursorPos(1, 1)`)
            NamedPrivateMode::Origin => {
                self.mode.remove(TermMode::ORIGIN);
                self.goto(0, 0);
            },
            NamedPrivateMode::ColumnMode => self.deccolm(false),
            NamedPrivateMode::BlinkingCursor => {
                let style = self.cursor_style.get_or_insert(self.config.default_cursor_style);
                style.blinking = false;
                self.event_proxy.send_event(Event::CursorBlinkingChange);
            },
            NamedPrivateMode::SyncUpdate => self.mode.remove(TermMode::SYNC_UPDATE),
            NamedPrivateMode::SwapScreenLegacy => self.switch_screen_legacy(false, false),
            NamedPrivateMode::SwapScreenAndClear => self.switch_screen_legacy(false, true),
            NamedPrivateMode::SaveRestoreCursor => {
                self.mode.remove(TermMode::SAVE_CURSOR_MODE);
                self.restore_cursor_position();
            },
            NamedPrivateMode::KeypadApplication => self.mode.remove(TermMode::APP_KEYPAD),
            // b-terminal(#302)
            NamedPrivateMode::ReverseColors => {
                self.mode.remove(TermMode::REVERSE_COLORS);
                self.mark_fully_damaged();
            },
            NamedPrivateMode::ReverseWrap => self.mode.remove(TermMode::REVERSE_WRAP),
            NamedPrivateMode::ReverseWrapExtended => self.mode.remove(TermMode::REVERSE_WRAP_EXTENDED),
            NamedPrivateMode::SlowScroll => self.mode.remove(TermMode::SLOW_SCROLL),
            NamedPrivateMode::Autorepeat => self.mode.remove(TermMode::AUTOREPEAT),
        }
    }

    #[inline]
    fn report_private_mode(&mut self, mode: PrivateMode) {
        trace!("Reporting private mode {mode:?}");
        let state = match mode {
            PrivateMode::Named(mode) => match mode {
                NamedPrivateMode::CursorKeys => self.mode.contains(TermMode::APP_CURSOR).into(),
                NamedPrivateMode::Origin => self.mode.contains(TermMode::ORIGIN).into(),
                NamedPrivateMode::LineWrap => self.mode.contains(TermMode::LINE_WRAP).into(),
                NamedPrivateMode::BlinkingCursor => {
                    let style = self.cursor_style.get_or_insert(self.config.default_cursor_style);
                    style.blinking.into()
                },
                NamedPrivateMode::ShowCursor => self.mode.contains(TermMode::SHOW_CURSOR).into(),
                NamedPrivateMode::ReportMouseClicks => {
                    self.mode.contains(TermMode::MOUSE_REPORT_CLICK).into()
                },
                NamedPrivateMode::ReportCellMouseMotion => {
                    self.mode.contains(TermMode::MOUSE_DRAG).into()
                },
                NamedPrivateMode::ReportAllMouseMotion => {
                    self.mode.contains(TermMode::MOUSE_MOTION).into()
                },
                NamedPrivateMode::ReportFocusInOut => {
                    self.mode.contains(TermMode::FOCUS_IN_OUT).into()
                },
                NamedPrivateMode::X10Mouse => self.mode.contains(TermMode::MOUSE_X10).into(),
                NamedPrivateMode::UrxvtMouse => self.mode.contains(TermMode::URXVT_MOUSE).into(),
                NamedPrivateMode::SgrPixelsMouse => {
                    self.mode.contains(TermMode::SGR_PIXELS_MOUSE).into()
                },
                NamedPrivateMode::Utf8Mouse => self.mode.contains(TermMode::UTF8_MOUSE).into(),
                NamedPrivateMode::SgrMouse => self.mode.contains(TermMode::SGR_MOUSE).into(),
                NamedPrivateMode::AlternateScroll => {
                    self.mode.contains(TermMode::ALTERNATE_SCROLL).into()
                },
                NamedPrivateMode::UrgencyHints => {
                    self.mode.contains(TermMode::URGENCY_HINTS).into()
                },
                NamedPrivateMode::SwapScreenAndSetRestoreCursor => {
                    self.mode.contains(TermMode::ALT_SCREEN).into()
                },
                NamedPrivateMode::BracketedPaste => {
                    self.mode.contains(TermMode::BRACKETED_PASTE).into()
                },
                // b-terminal(#296): 2026은 늘 꺼짐이라 답하던 것을 실제 상태로. 47·1047은 화면, 1048은 적어 둔 표시, 66은 키패드 모드
                NamedPrivateMode::SyncUpdate => self.mode.contains(TermMode::SYNC_UPDATE).into(),
                NamedPrivateMode::SwapScreenLegacy | NamedPrivateMode::SwapScreenAndClear => {
                    self.mode.contains(TermMode::ALT_SCREEN).into()
                },
                NamedPrivateMode::SaveRestoreCursor => {
                    self.mode.contains(TermMode::SAVE_CURSOR_MODE).into()
                },
                NamedPrivateMode::KeypadApplication => self.mode.contains(TermMode::APP_KEYPAD).into(),
                NamedPrivateMode::ColumnMode => self.mode.contains(TermMode::COLUMN_132).into(), // b-terminal(#324)
                // b-terminal(#302)
                NamedPrivateMode::ReverseColors => self.mode.contains(TermMode::REVERSE_COLORS).into(),
                NamedPrivateMode::ReverseWrap => self.mode.contains(TermMode::REVERSE_WRAP).into(),
                NamedPrivateMode::ReverseWrapExtended => {
                    self.mode.contains(TermMode::REVERSE_WRAP_EXTENDED).into()
                },
                NamedPrivateMode::SlowScroll => self.mode.contains(TermMode::SLOW_SCROLL).into(),
                NamedPrivateMode::Autorepeat => self.mode.contains(TermMode::AUTOREPEAT).into(),
            },
            PrivateMode::Unknown(GRAPHEME_CLUSTER_MODE) => {
                self.mode.contains(TermMode::GRAPHEME_CLUSTER).into()
            },
            // b-terminal(#301): 키 입력 모드
            PrivateMode::Unknown(mode) if key_input_mode_bit(mode).is_some() => {
                self.mode.contains(key_input_mode_bit(mode).unwrap_or(TermMode::NONE)).into()
            },
            // b-terminal(#311)
            PrivateMode::Unknown(LEFT_RIGHT_MARGIN_MODE) => {
                self.mode.contains(TermMode::LEFT_RIGHT_MARGIN).into()
            },
            // b-terminal(#324)
            PrivateMode::Unknown(ENABLE_MODE_3_MODE) => self.mode.contains(TermMode::ENABLE_MODE_3).into(),
            // b-terminal(#264): 모드 2048(크기 알림)의 DECRQM은 bt-term의 스캐너가 답한다. 여기서 0을 답하면 답이 둘 나간다
            // b-terminal(#296): 모드 2031(색 구성표 알림)도 같다
            PrivateMode::Unknown(2048 | 2031) => return,
            // b-terminal(#303): 모드 2033(가시성 보고)도 같다
            PrivateMode::Unknown(2033) => return,
            // b-terminal(#325): 모드 5522(kitty 붙여넣기 이벤트)도 같다
            PrivateMode::Unknown(5522) => return,
            // b-terminal(#777): DECECM(117)는 Ghostty처럼 영구 리셋(4)이다(modes.zig `getReport`). 구현한 모드와 겹치지 않는다
            PrivateMode::Unknown(117) => ModeState::PermanentlyReset,
            PrivateMode::Unknown(_) => ModeState::NotSupported,
        };

        // b-terminal(#313): XTSAVE의 읽기. 답은 쓰지 않는다
        if let Some(probe) = self.private_mode_probe.as_mut() {
            *probe = match state {
                ModeState::Set => Some(true),
                ModeState::Reset => Some(false),
                ModeState::NotSupported | ModeState::PermanentlyReset => None,
            };
            return;
        }

        self.event_proxy.send_event(Event::PtyWrite(format!(
            "\x1b[?{};{}$y",
            mode.raw(),
            state as u8,
        )));
    }

    /// b-terminal(#313): XTSAVE(`CSI ? Pm s`). 모드마다 마지막 값 하나만 둔다(Ghostty modes.zig `save`).
    /// 모르는 모드와 2026(모으기는 vte의 Processor가 쥐고 값만 바꾸면 어긋남)은 저장하지 않는다
    fn save_private_mode(&mut self, mode: PrivateMode) {
        if matches!(mode, PrivateMode::Named(NamedPrivateMode::SyncUpdate)) {
            return;
        }
        self.private_mode_probe = Some(None);
        self.report_private_mode(mode);
        if let Some(Some(on)) = self.private_mode_probe.take() {
            self.saved_private_modes.insert(mode.raw(), on);
        }
    }

    /// b-terminal(#313): XTRESTORE(`CSI ? Pm r`). 저장한 값을 DECSET·DECRST와 같은 길로 다시 적용한다. 저장한 적이 없으면
    /// 모드의 기본값이다(Ghostty modes.zig는 저장 표도 모드의 기본값으로 시작한다)
    fn restore_private_mode(&mut self, mode: PrivateMode) {
        if matches!(mode, PrivateMode::Named(NamedPrivateMode::SyncUpdate)) {
            return;
        }
        // 엔진이 아는 모드만. 모르는 모드는 건너뛴다(b-terminal(#324): 3·40도 안다)
        self.private_mode_probe = Some(None);
        self.report_private_mode(mode);
        if self.private_mode_probe.take().flatten().is_none() {
            return;
        }
        let raw = mode.raw();
        let on = self.saved_private_modes.get(&raw).copied().unwrap_or_else(|| private_mode_default(raw));
        if on {
            self.set_private_mode(mode);
        } else {
            self.unset_private_mode(mode);
        }
    }

    #[inline]
    fn set_mode(&mut self, mode: ansi::Mode) {
        let mode = match mode {
            ansi::Mode::Named(mode) => mode,
            ansi::Mode::Unknown(mode) => {
                debug!("Ignoring unknown mode {mode} in set_mode");
                return;
            },
        };

        trace!("Setting public mode: {mode:?}");
        match mode {
            NamedMode::Insert => self.mode.insert(TermMode::INSERT),
            NamedMode::LineFeedNewLine => self.mode.insert(TermMode::LINE_FEED_NEW_LINE),
            // b-terminal(#302): 상태만 둔다. 키보드를 실제로 잠그는 일은 세션이 설정(vt-kam-allowed)을 보고 한다
            NamedMode::KeyboardAction => self.mode.insert(TermMode::KEYBOARD_ACTION),
            NamedMode::SendReceive => self.mode.insert(TermMode::SEND_RECEIVE),
        }
    }

    #[inline]
    fn unset_mode(&mut self, mode: ansi::Mode) {
        let mode = match mode {
            ansi::Mode::Named(mode) => mode,
            ansi::Mode::Unknown(mode) => {
                debug!("Ignoring unknown mode {mode} in unset_mode");
                return;
            },
        };

        trace!("Setting public mode: {mode:?}");
        match mode {
            NamedMode::Insert => {
                self.mode.remove(TermMode::INSERT);
                self.mark_fully_damaged();
            },
            NamedMode::LineFeedNewLine => self.mode.remove(TermMode::LINE_FEED_NEW_LINE),
            // b-terminal(#302)
            NamedMode::KeyboardAction => self.mode.remove(TermMode::KEYBOARD_ACTION),
            NamedMode::SendReceive => self.mode.remove(TermMode::SEND_RECEIVE),
        }
    }

    #[inline]
    fn report_mode(&mut self, mode: ansi::Mode) {
        trace!("Reporting mode {mode:?}");
        let state = match mode {
            ansi::Mode::Named(mode) => match mode {
                NamedMode::Insert => self.mode.contains(TermMode::INSERT).into(),
                NamedMode::LineFeedNewLine => {
                    self.mode.contains(TermMode::LINE_FEED_NEW_LINE).into()
                },
                // b-terminal(#302)
                NamedMode::KeyboardAction => self.mode.contains(TermMode::KEYBOARD_ACTION).into(),
                NamedMode::SendReceive => self.mode.contains(TermMode::SEND_RECEIVE).into(),
            },
            // b-terminal(#777): 구현하지 않았지만 Ghostty가 영구 리셋(4)으로 답하는 ANSI 모드(modes.zig `getReport`:
            // GATM·SRTM·VEM·HEM·PUM·FEAM·FETM·MATM·TTM·SATM·TSM·EBM). 구현한 모드(2·4·12·20)와 겹치지 않는다(Ghostty는 겹치면 컴파일 오류)
            ansi::Mode::Unknown(1 | 5 | 7 | 10 | 11 | 13..=19) => ModeState::PermanentlyReset,
            ansi::Mode::Unknown(_) => ModeState::NotSupported,
        };

        self.event_proxy.send_event(Event::PtyWrite(format!(
            "\x1b[{};{}$y",
            mode.raw(),
            state as u8,
        )));
    }

    #[inline]
    fn set_scrolling_region(&mut self, top: usize, bottom: Option<usize>) {
        // b-terminal(#587): Ghostty `setTopAndBottomMargin`(Terminal.zig 3335~3343줄)의 순서. 아래 경계를 줄 수로
        // 먼저 자르고 나서 `top >= bottom`을 본다. 전에는 자르기 전에 봐서 한 줄 화면의 `2;4r`이 화면 밖 영역이 됐다.
        let lines = self.screen_lines();
        let top = cmp::max(1, top);
        let bottom = cmp::min(lines, bottom.unwrap_or(lines));

        if top >= bottom {
            debug!("Invalid scrolling region: ({top};{bottom})");
            return;
        }

        // Bottom should be included in the range, but range end is not
        // usually included. One option would be to use an inclusive
        // range, but instead we just let the open range end be 1
        // higher.
        let start = Line(top as i32 - 1);
        let end = Line(bottom as i32);

        trace!("Setting scrolling region: ({start};{end})");

        let screen_lines = Line(self.screen_lines() as i32);
        self.scroll_region.start = cmp::min(start, screen_lines);
        self.scroll_region.end = cmp::min(end, screen_lines);
        self.goto(0, 0);
    }

    /// b-terminal(#311): `CSI s`. 69가 켜져 있으면 DECSLRM(0, 0)으로 여백을 되돌리고, 아니면 SCOSC(커서 저장)다
    /// (Ghostty stream_handler.zig 309~315줄)
    fn save_cursor_or_reset_margins(&mut self) {
        if self.mode.contains(TermMode::LEFT_RIGHT_MARGIN) {
            self.set_left_right_margins(0, 0);
        } else {
            self.save_cursor_position();
        }
    }

    /// b-terminal(#311): DECSLRM. 69가 꺼져 있으면 아무것도 안 한다. 왼쪽이 오른쪽보다 작아야 하고, 정하면 커서를 원점으로 보낸다
    /// (Ghostty Terminal.zig `setLeftAndRightMargin` 3346줄)
    fn set_left_right_margins(&mut self, left: usize, right: usize) {
        if !self.mode.contains(TermMode::LEFT_RIGHT_MARGIN) {
            return;
        }
        let columns = self.columns();
        let left = left.max(1);
        let right = columns.min(if right == 0 { columns } else { right });
        if left >= right {
            return;
        }
        self.margin_left = left - 1;
        self.margin_right = right - 1;
        self.goto(0, 0);
    }

    #[inline]
    fn set_keypad_application_mode(&mut self) {
        trace!("Setting keypad application mode");
        self.mode.insert(TermMode::APP_KEYPAD);
    }

    #[inline]
    fn unset_keypad_application_mode(&mut self) {
        trace!("Unsetting keypad application mode");
        self.mode.remove(TermMode::APP_KEYPAD);
    }

    #[inline]
    fn configure_charset(&mut self, index: CharsetIndex, charset: StandardCharset) {
        trace!("Configuring charset {index:?} as {charset:?}");
        self.grid.cursor.charsets[index] = charset;
    }

    #[inline]
    // b-terminal(#295)
    fn set_single_shift(&mut self, index: CharsetIndex) {
        trace!("Single shift {index:?}");
        self.single_shift = Some(index);
    }

    // b-terminal(#295): DECIC. 스크롤 영역 안 모든 줄의 커서 칸에 빈 칸을 넣는다. 커서가 영역 밖이면 아무것도 하지 않는다
    // (좌우 여백 안에서만 민다. b-terminal(#311). Ghostty Terminal.zig insertColumns)
    fn insert_columns(&mut self, count: usize) {
        self.for_each_region_row(|term| term.insert_blank(count));
    }

    // b-terminal(#295): DECDC. 스크롤 영역 안 모든 줄에서 커서 칸부터 지우고 왼쪽으로 당긴다
    fn delete_columns(&mut self, count: usize) {
        self.for_each_region_row(|term| term.delete_chars(count));
    }

    // b-terminal(#295): DECBI. 왼쪽 끝이면 열을 하나 넣고, 아니면 왼쪽으로 한 칸. 줄바꿈 대기는 풀린다
    fn back_index(&mut self) {
        self.grid.cursor.input_needs_wrap = false;
        // b-terminal(#311): 왼쪽 여백 칸이면 열을 넣고, 아니면 화면 왼쪽 끝까지 한 칸 왼쪽으로(여백을 넘어도 움직인다. Ghostty `backIndex` 4229줄)
        if self.grid.cursor.point.column.0 == self.margin_left {
            self.insert_columns(1);
        } else if self.grid.cursor.point.column.0 > 0 {
            self.damage_cursor();
            self.grid.cursor.point.column -= 1;
            self.damage_cursor();
        }
    }

    // b-terminal(#295): DECFI. 오른쪽 끝이면 왼쪽 끝 열을 지우고 당기고, 아니면 오른쪽으로 한 칸. 줄바꿈 대기는 풀린다
    fn forward_index(&mut self) {
        self.grid.cursor.input_needs_wrap = false;
        // b-terminal(#311): 오른쪽 여백 칸이면 왼쪽 여백 열을 지우고 당기며, 아니면 화면 오른쪽 끝까지 한 칸 오른쪽으로(Ghostty `forwardIndex` 4248줄)
        if self.grid.cursor.point.column.0 == self.margin_right.min(self.last_column().0) {
            let column = self.grid.cursor.point.column;
            self.grid.cursor.point.column = Column(self.margin_left);
            self.delete_columns(1);
            self.grid.cursor.point.column = column;
        } else if self.grid.cursor.point.column < self.last_column() {
            self.damage_cursor();
            self.grid.cursor.point.column += 1;
            self.damage_cursor();
        }
    }

    fn set_active_charset(&mut self, index: CharsetIndex) {
        trace!("Setting active charset {index:?}");
        self.active_charset = index;
    }

    #[inline]
    fn set_cursor_style(&mut self, style: Option<CursorStyle>) {
        trace!("Setting cursor style {style:?}");
        self.cursor_style = style;

        // Notify UI about blinking changes.
        self.event_proxy.send_event(Event::CursorBlinkingChange);
    }

    #[inline]
    fn set_cursor_shape(&mut self, shape: CursorShape) {
        trace!("Setting cursor shape {shape:?}");

        let style = self.cursor_style.get_or_insert(self.config.default_cursor_style);
        style.shape = shape;
    }

    #[inline]
    fn set_title(&mut self, title: Option<String>) {
        trace!("Setting title to '{title:?}'");

        self.title.clone_from(&title);

        let title_event = match title {
            Some(title) => Event::Title(title),
            None => Event::ResetTitle,
        };

        self.event_proxy.send_event(title_event);
    }

    #[inline]
    fn push_title(&mut self) {
        trace!("Pushing '{:?}' onto title stack", self.title);

        if self.title_stack.len() >= TITLE_STACK_MAX_DEPTH {
            let removed = self.title_stack.remove(0);
            trace!(
                "Removing '{removed:?}' from bottom of title stack that exceeds its maximum depth"
            );
        }

        self.title_stack.push(self.title.clone());
    }

    #[inline]
    fn pop_title(&mut self) {
        trace!("Attempting to pop title from stack...");

        if let Some(popped) = self.title_stack.pop() {
            trace!("Title '{popped:?}' popped from stack");
            self.set_title(popped);
        }
    }

    #[inline]
    fn text_area_size_pixels(&mut self) {
        self.event_proxy.send_event(Event::TextAreaSizeRequest(Arc::new(move |window_size| {
            let height = window_size.num_lines * window_size.cell_height;
            let width = window_size.num_cols * window_size.cell_width;
            format!("\x1b[4;{height};{width}t")
        })));
    }

    #[inline]
    fn text_area_size_chars(&mut self) {
        let text = format!("\x1b[8;{};{}t", self.screen_lines(), self.columns());
        self.event_proxy.send_event(Event::PtyWrite(text));
    }
}

/// b-terminal(#313): 저장하지 않은 모드를 XTRESTORE할 때의 값. Ghostty modes.zig의 `.default = true`인 DEC 모드
/// (7 자동 줄바꿈, 25 커서 보임, 1007 대체 스크롤, 1035, 1036)
fn private_mode_default(mode: u16) -> bool {
    matches!(mode, 7 | 25 | 1007 | 1035 | 1036)
}

/// b-terminal(#324): 줄 수와 열 수만 가진 크기(DECCOLM과 모드 40이 `resize_grid`에 넘긴다)
struct GridDims {
    columns: usize,
    lines: usize,
}

impl Dimensions for GridDims {
    fn total_lines(&self) -> usize {
        self.lines
    }

    fn screen_lines(&self) -> usize {
        self.lines
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

/// The state of the [`Mode`] and [`PrivateMode`].
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
enum ModeState {
    /// The mode is not supported.
    NotSupported = 0,
    /// The mode is currently set.
    Set = 1,
    /// The mode is currently not set.
    Reset = 2,
    /// b-terminal(#777): 영구 리셋. 바꿀 수 없는 모드(DECRPM 4)
    PermanentlyReset = 4,
}

impl From<bool> for ModeState {
    fn from(value: bool) -> Self {
        if value { Self::Set } else { Self::Reset }
    }
}

/// Terminal version for escape sequence reports.
///
/// This returns the current terminal version as a unique number based on alacritty_terminal's
/// semver version. The different versions are padded to ensure that a higher semver version will
/// always report a higher version number.
#[allow(dead_code)] // b-terminal(#264): DA2 답을 밖에서 하므로 쓰는 곳이 없다
fn version_number(mut version: &str) -> usize {
    if let Some(separator) = version.rfind('-') {
        version = &version[..separator];
    }

    let mut version_number = 0;

    let semver_versions = version.split('.');
    for (i, semver_version) in semver_versions.rev().enumerate() {
        let semver_number = semver_version.parse::<usize>().unwrap_or(0);
        version_number += usize::pow(100, i as u32) * semver_number;
    }

    version_number
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardType {
    Clipboard,
    Selection,
}

struct TabStops {
    tabs: Vec<bool>,
}

impl TabStops {
    #[inline]
    fn new(columns: usize) -> TabStops {
        TabStops { tabs: (0..columns).map(|i| i % INITIAL_TABSTOPS == 0).collect() }
    }

    /// b-terminal(#532): `interval`칸마다 탭 정지(0열 포함)
    fn with_interval(columns: usize, interval: usize) -> TabStops {
        TabStops { tabs: (0..columns).map(|i| i % interval == 0).collect() }
    }

    /// Remove all tabstops.
    #[inline]
    fn clear_all(&mut self) {
        unsafe {
            ptr::write_bytes(self.tabs.as_mut_ptr(), 0, self.tabs.len());
        }
    }

    /// Increase tabstop capacity.
    #[inline]
    fn resize(&mut self, columns: usize) {
        let mut index = self.tabs.len();
        self.tabs.resize_with(columns, || {
            let is_tabstop = index % INITIAL_TABSTOPS == 0;
            index += 1;
            is_tabstop
        });
    }
}

impl Index<Column> for TabStops {
    type Output = bool;

    fn index(&self, index: Column) -> &bool {
        &self.tabs[index.0]
    }
}

impl IndexMut<Column> for TabStops {
    fn index_mut(&mut self, index: Column) -> &mut bool {
        self.tabs.index_mut(index.0)
    }
}

/// Terminal cursor rendering information.
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct RenderableCursor {
    pub shape: CursorShape,
    pub point: Point,
}

impl RenderableCursor {
    fn new<T>(term: &Term<T>) -> Self {
        // Cursor position.
        let vi_mode = term.mode().contains(TermMode::VI);
        let mut point = if vi_mode { term.vi_mode_cursor.point } else { term.grid.cursor.point };
        if term.grid[point].flags.contains(Flags::WIDE_CHAR_SPACER) {
            point.column -= 1;
        }

        // Cursor shape.
        let shape = if !vi_mode && !term.mode().contains(TermMode::SHOW_CURSOR) {
            CursorShape::Hidden
        } else {
            term.cursor_style().shape
        };

        Self { shape, point }
    }
}

/// Visible terminal content.
///
/// This contains all content required to render the current terminal view.
pub struct RenderableContent<'a> {
    pub display_iter: GridIterator<'a, Cell>,
    pub selection: Option<SelectionRange>,
    pub cursor: RenderableCursor,
    pub display_offset: usize,
    pub colors: &'a color::Colors,
    pub mode: TermMode,
}

impl<'a> RenderableContent<'a> {
    fn new<T>(term: &'a Term<T>) -> Self {
        Self {
            display_iter: term.grid().display_iter(),
            display_offset: term.grid().display_offset(),
            cursor: RenderableCursor::new(term),
            selection: term.selection.as_ref().and_then(|s| s.to_range(term)),
            colors: &term.colors,
            mode: *term.mode(),
        }
    }
}

/// Terminal test helpers.
pub mod test {
    use super::*;

    #[cfg(feature = "serde")]
    use serde::{Deserialize, Serialize};

    use crate::event::VoidListener;

    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    pub struct TermSize {
        pub columns: usize,
        pub screen_lines: usize,
    }

    impl TermSize {
        pub fn new(columns: usize, screen_lines: usize) -> Self {
            Self { columns, screen_lines }
        }
    }

    impl Dimensions for TermSize {
        fn total_lines(&self) -> usize {
            self.screen_lines()
        }

        fn screen_lines(&self) -> usize {
            self.screen_lines
        }

        fn columns(&self) -> usize {
            self.columns
        }
    }

    /// Construct a terminal from its content as string.
    ///
    /// A `\n` will break line and `\r\n` will break line without wrapping.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use alacritty_terminal::term::test::mock_term;
    ///
    /// // Create a terminal with the following cells:
    /// //
    /// // [h][e][l][l][o] <- WRAPLINE flag set
    /// // [:][)][ ][ ][ ]
    /// // [t][e][s][t][ ]
    /// mock_term(
    ///     "\
    ///     hello\n:)\r\ntest",
    /// );
    /// ```
    pub fn mock_term(content: &str) -> Term<VoidListener> {
        let lines: Vec<&str> = content.split('\n').collect();
        let num_cols = lines
            .iter()
            .map(|line| line.chars().filter(|c| *c != '\r').map(|c| c.width().unwrap()).sum())
            .max()
            .unwrap_or(0);

        // Create terminal with the appropriate dimensions.
        let size = TermSize::new(num_cols, lines.len());
        let mut term = Term::new(Config::default(), &size, VoidListener);

        // Fill terminal with content.
        for (line, text) in lines.iter().enumerate() {
            let line = Line(line as i32);
            if !text.ends_with('\r') && line + 1 != lines.len() {
                term.grid[line][Column(num_cols - 1)].flags.insert(Flags::WRAPLINE);
            }

            let mut index = 0;
            for c in text.chars().take_while(|c| *c != '\r') {
                term.grid[line][Column(index)].c = c;

                // Handle fullwidth characters.
                let width = c.width().unwrap();
                if width == 2 {
                    term.grid[line][Column(index)].flags.insert(Flags::WIDE_CHAR);
                    term.grid[line][Column(index + 1)].flags.insert(Flags::WIDE_CHAR_SPACER);
                }

                index += width;
            }
        }

        term
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::mem;

    use crate::event::VoidListener;
    use crate::grid::{Grid, Scroll};
    use crate::index::{Column, Point, Side};
    use crate::selection::{Selection, SelectionType};
    use crate::term::cell::{Cell, Flags};
    use crate::term::test::TermSize;
    use crate::vte::ansi::{self, CharsetIndex, Handler, StandardCharset};

    #[test]
    fn scroll_display_page_up() {
        let size = TermSize::new(5, 10);
        let mut term = Term::new(Config::default(), &size, VoidListener);

        // Create 11 lines of scrollback.
        for _ in 0..20 {
            term.newline();
        }

        // Scrollable amount to top is 11.
        term.scroll_display(Scroll::PageUp);
        assert_eq!(term.vi_mode_cursor.point, Point::new(Line(-1), Column(0)));
        assert_eq!(term.grid.display_offset(), 10);

        // Scrollable amount to top is 1.
        term.scroll_display(Scroll::PageUp);
        assert_eq!(term.vi_mode_cursor.point, Point::new(Line(-2), Column(0)));
        assert_eq!(term.grid.display_offset(), 11);

        // Scrollable amount to top is 0.
        term.scroll_display(Scroll::PageUp);
        assert_eq!(term.vi_mode_cursor.point, Point::new(Line(-2), Column(0)));
        assert_eq!(term.grid.display_offset(), 11);
    }

    #[test]
    fn scroll_display_page_down() {
        let size = TermSize::new(5, 10);
        let mut term = Term::new(Config::default(), &size, VoidListener);

        // Create 11 lines of scrollback.
        for _ in 0..20 {
            term.newline();
        }

        // Change display_offset to topmost.
        term.grid_mut().scroll_display(Scroll::Top);
        term.vi_mode_cursor = ViModeCursor::new(Point::new(Line(-11), Column(0)));

        // Scrollable amount to bottom is 11.
        term.scroll_display(Scroll::PageDown);
        assert_eq!(term.vi_mode_cursor.point, Point::new(Line(-1), Column(0)));
        assert_eq!(term.grid.display_offset(), 1);

        // Scrollable amount to bottom is 1.
        term.scroll_display(Scroll::PageDown);
        assert_eq!(term.vi_mode_cursor.point, Point::new(Line(0), Column(0)));
        assert_eq!(term.grid.display_offset(), 0);

        // Scrollable amount to bottom is 0.
        term.scroll_display(Scroll::PageDown);
        assert_eq!(term.vi_mode_cursor.point, Point::new(Line(0), Column(0)));
        assert_eq!(term.grid.display_offset(), 0);
    }

    #[test]
    fn simple_selection_works() {
        let size = TermSize::new(5, 5);
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let grid = term.grid_mut();
        for i in 0..4 {
            if i == 1 {
                continue;
            }

            grid[Line(i)][Column(0)].c = '"';

            for j in 1..4 {
                grid[Line(i)][Column(j)].c = 'a';
            }

            grid[Line(i)][Column(4)].c = '"';
        }
        grid[Line(2)][Column(0)].c = ' ';
        grid[Line(2)][Column(4)].c = ' ';
        grid[Line(2)][Column(4)].flags.insert(Flags::WRAPLINE);
        grid[Line(3)][Column(0)].c = ' ';

        // Multiple lines contain an empty line.
        term.selection = Some(Selection::new(
            SelectionType::Simple,
            Point { line: Line(0), column: Column(0) },
            Side::Left,
        ));
        if let Some(s) = term.selection.as_mut() {
            s.update(Point { line: Line(2), column: Column(4) }, Side::Right);
        }
        assert_eq!(term.selection_to_string(), Some(String::from("\"aaa\"\n\n aaa ")));

        // A wrapline.
        term.selection = Some(Selection::new(
            SelectionType::Simple,
            Point { line: Line(2), column: Column(0) },
            Side::Left,
        ));
        if let Some(s) = term.selection.as_mut() {
            s.update(Point { line: Line(3), column: Column(4) }, Side::Right);
        }
        assert_eq!(term.selection_to_string(), Some(String::from(" aaa  aaa\"")));
    }

    #[test]
    fn semantic_selection_works() {
        let size = TermSize::new(5, 3);
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut grid: Grid<Cell> = Grid::new(3, 5, 0);
        for i in 0..5 {
            for j in 0..2 {
                grid[Line(j)][Column(i)].c = 'a';
                grid[Line(j)][Column(i)].flags.insert(Flags::WRITTEN); // b-terminal(#804)
            }
        }
        grid[Line(0)][Column(0)].c = '"';
        grid[Line(0)][Column(3)].c = '"';
        grid[Line(1)][Column(2)].c = '"';
        grid[Line(0)][Column(4)].flags.insert(Flags::WRAPLINE);

        let mut escape_chars = String::from("\"");

        mem::swap(&mut term.grid, &mut grid);
        mem::swap(&mut term.config.semantic_escape_chars, &mut escape_chars);

        {
            term.selection = Some(Selection::new(
                SelectionType::Semantic,
                Point { line: Line(0), column: Column(1) },
                Side::Left,
            ));
            assert_eq!(term.selection_to_string(), Some(String::from("aa")));
        }

        {
            term.selection = Some(Selection::new(
                SelectionType::Semantic,
                Point { line: Line(0), column: Column(4) },
                Side::Left,
            ));
            assert_eq!(term.selection_to_string(), Some(String::from("aaa")));
        }

        {
            term.selection = Some(Selection::new(
                SelectionType::Semantic,
                Point { line: Line(1), column: Column(1) },
                Side::Left,
            ));
            assert_eq!(term.selection_to_string(), Some(String::from("aaa")));
        }
    }

    #[test]
    fn line_selection_works() {
        let size = TermSize::new(5, 1);
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let mut grid: Grid<Cell> = Grid::new(1, 5, 0);
        for i in 0..5 {
            grid[Line(0)][Column(i)].c = 'a';
            grid[Line(0)][Column(i)].flags.insert(Flags::WRITTEN); // b-terminal(#803)
        }
        grid[Line(0)][Column(0)].c = '"';
        grid[Line(0)][Column(3)].c = '"';

        mem::swap(&mut term.grid, &mut grid);

        term.selection = Some(Selection::new(
            SelectionType::Lines,
            Point { line: Line(0), column: Column(3) },
            Side::Left,
        ));
        assert_eq!(term.selection_to_string(), Some(String::from("\"aa\"a\n")));
    }

    #[test]
    fn block_selection_works() {
        let size = TermSize::new(5, 5);
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let grid = term.grid_mut();
        for i in 1..4 {
            grid[Line(i)][Column(0)].c = '"';

            for j in 1..4 {
                grid[Line(i)][Column(j)].c = 'a';
            }

            grid[Line(i)][Column(4)].c = '"';
        }
        grid[Line(2)][Column(2)].c = ' ';
        grid[Line(2)][Column(4)].flags.insert(Flags::WRAPLINE);
        grid[Line(3)][Column(4)].c = ' ';

        term.selection = Some(Selection::new(
            SelectionType::Block,
            Point { line: Line(0), column: Column(3) },
            Side::Left,
        ));

        // The same column.
        if let Some(s) = term.selection.as_mut() {
            s.update(Point { line: Line(3), column: Column(3) }, Side::Right);
        }
        assert_eq!(term.selection_to_string(), Some(String::from("\na\na\na")));

        // The first column.
        if let Some(s) = term.selection.as_mut() {
            s.update(Point { line: Line(3), column: Column(0) }, Side::Left);
        }
        assert_eq!(term.selection_to_string(), Some(String::from("\n\"aa\n\"a\n\"aa")));

        // The last column.
        if let Some(s) = term.selection.as_mut() {
            s.update(Point { line: Line(3), column: Column(4) }, Side::Right);
        }
        assert_eq!(term.selection_to_string(), Some(String::from("\na\"\na\"\na")));
    }

    /// Check that the grid can be serialized back and forth losslessly.
    ///
    /// This test is in the term module as opposed to the grid since we want to
    /// test this property with a T=Cell.
    #[test]
    #[cfg(feature = "serde")]
    fn grid_serde() {
        let grid: Grid<Cell> = Grid::new(24, 80, 0);
        let serialized = serde_json::to_string(&grid).expect("ser");
        let deserialized = serde_json::from_str::<Grid<Cell>>(&serialized).expect("de");

        assert_eq!(deserialized, grid);
    }

    #[test]
    fn input_line_drawing_character() {
        let size = TermSize::new(7, 17);
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let cursor = Point::new(Line(0), Column(0));
        term.configure_charset(CharsetIndex::G0, StandardCharset::SpecialCharacterAndLineDrawing);
        term.input('a');

        assert_eq!(term.grid()[cursor].c, '▒');
    }

    #[test]
    fn clearing_viewport_keeps_history_position() {
        let size = TermSize::new(10, 20);
        let mut term = Term::new(Config::default(), &size, VoidListener);

        // Create 10 lines of scrollback.
        for _ in 0..29 {
            term.newline();
        }

        // Change the display area.
        term.scroll_display(Scroll::Top);

        assert_eq!(term.grid.display_offset(), 10);

        // Clear the viewport.
        term.clear_screen(ansi::ClearMode::All);

        assert_eq!(term.grid.display_offset(), 10);
    }

    #[test]
    fn clearing_viewport_with_vi_mode_keeps_history_position() {
        let size = TermSize::new(10, 20);
        let mut term = Term::new(Config::default(), &size, VoidListener);

        // Create 10 lines of scrollback.
        for _ in 0..29 {
            term.newline();
        }

        // Enable vi mode.
        term.toggle_vi_mode();

        // Change the display area and the vi cursor position.
        term.scroll_display(Scroll::Top);
        term.vi_mode_cursor.point = Point::new(Line(-5), Column(3));

        assert_eq!(term.grid.display_offset(), 10);

        // Clear the viewport.
        term.clear_screen(ansi::ClearMode::All);

        assert_eq!(term.grid.display_offset(), 10);
        assert_eq!(term.vi_mode_cursor.point, Point::new(Line(-5), Column(3)));
    }

    #[test]
    fn clearing_scrollback_resets_display_offset() {
        let size = TermSize::new(10, 20);
        let mut term = Term::new(Config::default(), &size, VoidListener);

        // Create 10 lines of scrollback.
        for _ in 0..29 {
            term.newline();
        }

        // Change the display area.
        term.scroll_display(Scroll::Top);

        assert_eq!(term.grid.display_offset(), 10);

        // Clear the scrollback buffer.
        term.clear_screen(ansi::ClearMode::Saved);

        assert_eq!(term.grid.display_offset(), 0);
    }

    #[test]
    fn clearing_scrollback_sets_vi_cursor_into_viewport() {
        let size = TermSize::new(10, 20);
        let mut term = Term::new(Config::default(), &size, VoidListener);

        // Create 10 lines of scrollback.
        for _ in 0..29 {
            term.newline();
        }

        // Enable vi mode.
        term.toggle_vi_mode();

        // Change the display area and the vi cursor position.
        term.scroll_display(Scroll::Top);
        term.vi_mode_cursor.point = Point::new(Line(-5), Column(3));

        assert_eq!(term.grid.display_offset(), 10);

        // Clear the scrollback buffer.
        term.clear_screen(ansi::ClearMode::Saved);

        assert_eq!(term.grid.display_offset(), 0);
        assert_eq!(term.vi_mode_cursor.point, Point::new(Line(0), Column(3)));
    }

    #[test]
    fn clear_saved_lines() {
        let size = TermSize::new(7, 17);
        let mut term = Term::new(Config::default(), &size, VoidListener);

        // Add one line of scrollback.
        term.grid.scroll_up(&(Line(0)..Line(1)), 1);

        // Clear the history.
        term.clear_screen(ansi::ClearMode::Saved);

        // Make sure that scrolling does not change the grid.
        let mut scrolled_grid = term.grid.clone();
        scrolled_grid.scroll_display(Scroll::Top);

        // Truncate grids for comparison.
        scrolled_grid.truncate();
        term.grid.truncate();

        assert_eq!(term.grid, scrolled_grid);
    }

    #[test]
    fn vi_cursor_keep_pos_on_scrollback_buffer() {
        let size = TermSize::new(5, 10);
        let mut term = Term::new(Config::default(), &size, VoidListener);

        // Create 11 lines of scrollback.
        for _ in 0..20 {
            term.newline();
        }

        // Enable vi mode.
        term.toggle_vi_mode();

        term.scroll_display(Scroll::Top);
        term.vi_mode_cursor.point.line = Line(-11);

        term.linefeed();
        assert_eq!(term.vi_mode_cursor.point.line, Line(-12));
    }

    #[test]
    fn grow_lines_updates_active_cursor_pos() {
        let mut size = TermSize::new(100, 10);
        let mut term = Term::new(Config::default(), &size, VoidListener);

        // Create 10 lines of scrollback.
        for _ in 0..19 {
            term.newline();
        }
        assert_eq!(term.history_size(), 10);
        assert_eq!(term.grid.cursor.point, Point::new(Line(9), Column(0)));

        // Increase visible lines.
        size.screen_lines = 30;
        term.resize(size);

        assert_eq!(term.history_size(), 0);
        assert_eq!(term.grid.cursor.point, Point::new(Line(19), Column(0)));
    }

    #[test]
    fn grow_lines_updates_inactive_cursor_pos() {
        let mut size = TermSize::new(100, 10);
        let mut term = Term::new(Config::default(), &size, VoidListener);

        // Create 10 lines of scrollback.
        for _ in 0..19 {
            term.newline();
        }
        assert_eq!(term.history_size(), 10);
        assert_eq!(term.grid.cursor.point, Point::new(Line(9), Column(0)));

        // Enter alt screen.
        term.set_private_mode(NamedPrivateMode::SwapScreenAndSetRestoreCursor.into());

        // Increase visible lines.
        size.screen_lines = 30;
        term.resize(size);

        // Leave alt screen.
        term.unset_private_mode(NamedPrivateMode::SwapScreenAndSetRestoreCursor.into());

        assert_eq!(term.history_size(), 0);
        assert_eq!(term.grid.cursor.point, Point::new(Line(19), Column(0)));
    }

    #[test]
    fn shrink_lines_updates_active_cursor_pos() {
        let mut size = TermSize::new(100, 10);
        let mut term = Term::new(Config::default(), &size, VoidListener);

        // Create 10 lines of scrollback.
        for _ in 0..19 {
            term.newline();
        }
        assert_eq!(term.history_size(), 10);
        assert_eq!(term.grid.cursor.point, Point::new(Line(9), Column(0)));

        // Increase visible lines.
        size.screen_lines = 5;
        term.resize(size);

        assert_eq!(term.history_size(), 15);
        assert_eq!(term.grid.cursor.point, Point::new(Line(4), Column(0)));
    }

    #[test]
    fn shrink_lines_updates_inactive_cursor_pos() {
        let mut size = TermSize::new(100, 10);
        let mut term = Term::new(Config::default(), &size, VoidListener);

        // Create 10 lines of scrollback.
        for _ in 0..19 {
            term.newline();
        }
        assert_eq!(term.history_size(), 10);
        assert_eq!(term.grid.cursor.point, Point::new(Line(9), Column(0)));

        // Enter alt screen.
        term.set_private_mode(NamedPrivateMode::SwapScreenAndSetRestoreCursor.into());

        // Increase visible lines.
        size.screen_lines = 5;
        term.resize(size);

        // Leave alt screen.
        term.unset_private_mode(NamedPrivateMode::SwapScreenAndSetRestoreCursor.into());

        assert_eq!(term.history_size(), 15);
        assert_eq!(term.grid.cursor.point, Point::new(Line(4), Column(0)));
    }

    #[test]
    fn damage_public_usage() {
        let size = TermSize::new(10, 10);
        let mut term = Term::new(Config::default(), &size, VoidListener);
        // Reset terminal for partial damage tests since it's initialized as fully damaged.
        term.reset_damage();

        // Test that we damage input form [`Term::input`].

        let left = term.grid.cursor.point.column.0;
        term.input('d');
        term.input('a');
        term.input('m');
        term.input('a');
        term.input('g');
        term.input('e');
        let right = term.grid.cursor.point.column.0;

        let mut damaged_lines = match term.damage() {
            TermDamage::Full => panic!("Expected partial damage, however got Full"),
            TermDamage::Partial(damaged_lines) => damaged_lines,
        };
        assert_eq!(damaged_lines.next(), Some(LineDamageBounds { line: 0, left, right }));
        assert_eq!(damaged_lines.next(), None);
        term.reset_damage();

        // Create scrollback.
        for _ in 0..20 {
            term.newline();
        }

        match term.damage() {
            TermDamage::Full => (),
            TermDamage::Partial(_) => panic!("Expected Full damage, however got Partial "),
        };
        term.reset_damage();

        term.scroll_display(Scroll::Delta(10));
        term.reset_damage();

        // No damage when scrolled into viewport.
        for idx in 0..term.columns() {
            term.goto(idx as i32, idx);
        }
        let mut damaged_lines = match term.damage() {
            TermDamage::Full => panic!("Expected partial damage, however got Full"),
            TermDamage::Partial(damaged_lines) => damaged_lines,
        };
        assert_eq!(damaged_lines.next(), None);

        // Scroll back into the viewport, so we have 2 visible lines which terminal can write
        // to.
        term.scroll_display(Scroll::Delta(-2));
        term.reset_damage();

        term.goto(0, 0);
        term.goto(1, 0);
        term.goto(2, 0);
        let display_offset = term.grid().display_offset();
        let mut damaged_lines = match term.damage() {
            TermDamage::Full => panic!("Expected partial damage, however got Full"),
            TermDamage::Partial(damaged_lines) => damaged_lines,
        };
        assert_eq!(
            damaged_lines.next(),
            Some(LineDamageBounds { line: display_offset, left: 0, right: 0 })
        );
        assert_eq!(
            damaged_lines.next(),
            Some(LineDamageBounds { line: display_offset + 1, left: 0, right: 0 })
        );
        assert_eq!(damaged_lines.next(), None);
    }

    #[test]
    fn damage_cursor_movements() {
        let size = TermSize::new(10, 10);
        let mut term = Term::new(Config::default(), &size, VoidListener);
        let num_cols = term.columns();
        // Reset terminal for partial damage tests since it's initialized as fully damaged.
        term.reset_damage();

        term.goto(1, 1);

        // NOTE While we can use `[Term::damage]` to access terminal damage information, in the
        // following tests we will be accessing `term.damage.lines` directly to avoid adding extra
        // damage information (like cursor and Vi cursor), which we're not testing.

        assert_eq!(term.damage.lines[0], LineDamageBounds { line: 0, left: 0, right: 0 });
        assert_eq!(term.damage.lines[1], LineDamageBounds { line: 1, left: 1, right: 1 });
        term.damage.reset(num_cols);

        term.move_forward(3);
        assert_eq!(term.damage.lines[1], LineDamageBounds { line: 1, left: 1, right: 4 });
        term.damage.reset(num_cols);

        term.move_backward(8);
        assert_eq!(term.damage.lines[1], LineDamageBounds { line: 1, left: 0, right: 4 });
        term.goto(5, 5);
        term.damage.reset(num_cols);

        term.backspace();
        term.backspace();
        assert_eq!(term.damage.lines[5], LineDamageBounds { line: 5, left: 3, right: 5 });
        term.damage.reset(num_cols);

        term.move_up(1);
        assert_eq!(term.damage.lines[5], LineDamageBounds { line: 5, left: 3, right: 3 });
        assert_eq!(term.damage.lines[4], LineDamageBounds { line: 4, left: 3, right: 3 });
        term.damage.reset(num_cols);

        term.move_down(1);
        term.move_down(1);
        assert_eq!(term.damage.lines[4], LineDamageBounds { line: 4, left: 3, right: 3 });
        assert_eq!(term.damage.lines[5], LineDamageBounds { line: 5, left: 3, right: 3 });
        assert_eq!(term.damage.lines[6], LineDamageBounds { line: 6, left: 3, right: 3 });
        term.damage.reset(num_cols);

        term.wrapline();
        assert_eq!(term.damage.lines[6], LineDamageBounds { line: 6, left: 3, right: 3 });
        assert_eq!(term.damage.lines[7], LineDamageBounds { line: 7, left: 0, right: 0 });
        term.move_forward(3);
        term.move_up(1);
        term.damage.reset(num_cols);

        term.linefeed();
        assert_eq!(term.damage.lines[6], LineDamageBounds { line: 6, left: 3, right: 3 });
        assert_eq!(term.damage.lines[7], LineDamageBounds { line: 7, left: 3, right: 3 });
        term.damage.reset(num_cols);

        term.carriage_return();
        assert_eq!(term.damage.lines[7], LineDamageBounds { line: 7, left: 0, right: 3 });
        term.damage.reset(num_cols);

        term.erase_chars(5);
        assert_eq!(term.damage.lines[7], LineDamageBounds { line: 7, left: 0, right: 5 });
        term.damage.reset(num_cols);

        term.delete_chars(3);
        let right = term.columns() - 1;
        assert_eq!(term.damage.lines[7], LineDamageBounds { line: 7, left: 0, right });
        term.move_forward(term.columns());
        term.damage.reset(num_cols);

        term.move_backward_tabs(1);
        assert_eq!(term.damage.lines[7], LineDamageBounds { line: 7, left: 8, right });
        term.save_cursor_position();
        term.goto(1, 1);
        term.damage.reset(num_cols);

        term.restore_cursor_position();
        assert_eq!(term.damage.lines[1], LineDamageBounds { line: 1, left: 1, right: 1 });
        assert_eq!(term.damage.lines[7], LineDamageBounds { line: 7, left: 8, right: 8 });
        term.damage.reset(num_cols);

        term.clear_line(ansi::LineClearMode::All);
        assert_eq!(term.damage.lines[7], LineDamageBounds { line: 7, left: 0, right });
        term.damage.reset(num_cols);

        term.clear_line(ansi::LineClearMode::Left);
        assert_eq!(term.damage.lines[7], LineDamageBounds { line: 7, left: 0, right: 8 });
        term.damage.reset(num_cols);

        term.clear_line(ansi::LineClearMode::Right);
        assert_eq!(term.damage.lines[7], LineDamageBounds { line: 7, left: 8, right });
        term.damage.reset(num_cols);

        term.reverse_index();
        assert_eq!(term.damage.lines[7], LineDamageBounds { line: 7, left: 8, right: 8 });
        assert_eq!(term.damage.lines[6], LineDamageBounds { line: 6, left: 8, right: 8 });
    }

    #[test]
    fn full_damage() {
        let size = TermSize::new(100, 10);
        let mut term = Term::new(Config::default(), &size, VoidListener);

        assert!(term.damage.full);
        for _ in 0..20 {
            term.newline();
        }
        term.reset_damage();

        term.clear_screen(ansi::ClearMode::Above);
        assert!(term.damage.full);
        term.reset_damage();

        term.scroll_display(Scroll::Top);
        assert!(term.damage.full);
        term.reset_damage();

        // Sequential call to scroll display without doing anything shouldn't damage.
        term.scroll_display(Scroll::Top);
        assert!(!term.damage.full);
        term.reset_damage();

        term.set_options(Config::default());
        assert!(term.damage.full);
        term.reset_damage();

        term.scroll_down_relative(Line(5), 2);
        assert!(term.damage.full);
        term.reset_damage();

        term.scroll_up_relative(Line(3), 2);
        assert!(term.damage.full);
        term.reset_damage();

        term.mode.insert(TermMode::ENABLE_MODE_3);
        term.deccolm(true);
        assert!(term.damage.full);
        term.reset_damage();

        term.decaln();
        assert!(term.damage.full);
        term.reset_damage();

        term.set_mode(NamedMode::Insert.into());
        // Just setting `Insert` mode shouldn't mark terminal as damaged.
        assert!(!term.damage.full);
        term.reset_damage();

        let color_index = 257;
        term.set_color(color_index, Rgb::default());
        assert!(term.damage.full);
        term.reset_damage();

        // Setting the same color once again shouldn't trigger full damage.
        term.set_color(color_index, Rgb::default());
        assert!(!term.damage.full);

        term.reset_color(color_index);
        assert!(term.damage.full);
        term.reset_damage();

        // We shouldn't trigger fully damage when cursor gets update.
        term.set_color(NamedColor::Cursor as usize, Rgb::default());
        assert!(!term.damage.full);

        // However requesting terminal damage should mark terminal as fully damaged in `Insert`
        // mode.
        let _ = term.damage();
        assert!(term.damage.full);
        term.reset_damage();

        term.unset_mode(NamedMode::Insert.into());
        assert!(term.damage.full);
        term.reset_damage();

        // Keep this as a last check, so we don't have to deal with restoring from alt-screen.
        term.swap_alt();
        assert!(term.damage.full);
        term.reset_damage();

        let size = TermSize::new(10, 10);
        term.resize(size);
        assert!(term.damage.full);
    }

    #[test]
    fn window_title() {
        let size = TermSize::new(7, 17);
        let mut term = Term::new(Config::default(), &size, VoidListener);

        // Title None by default.
        assert_eq!(term.title, None);

        // Title can be set.
        term.set_title(Some("Test".into()));
        assert_eq!(term.title, Some("Test".into()));

        // Title can be pushed onto stack.
        term.push_title();
        term.set_title(Some("Next".into()));
        assert_eq!(term.title, Some("Next".into()));
        assert_eq!(term.title_stack.first().unwrap(), &Some("Test".into()));

        // Title can be popped from stack and set as the window title.
        term.pop_title();
        assert_eq!(term.title, Some("Test".into()));
        assert!(term.title_stack.is_empty());

        // Title stack doesn't grow infinitely.
        for _ in 0..4097 {
            term.push_title();
        }
        assert_eq!(term.title_stack.len(), 4096);

        // Title and title stack reset when terminal state is reset.
        term.push_title();
        term.reset_state();
        assert_eq!(term.title, None);
        assert!(term.title_stack.is_empty());

        // Title stack pops back to default.
        term.title = None;
        term.push_title();
        term.set_title(Some("Test".into()));
        term.pop_title();
        assert_eq!(term.title, None);

        // Title can be reset to default.
        term.title = Some("Test".into());
        term.set_title(None);
        assert_eq!(term.title, None);
    }

    #[test]
    fn parse_cargo_version() {
        assert!(version_number(env!("CARGO_PKG_VERSION")) >= 10_01);
        assert_eq!(version_number("0.0.1-dev"), 1);
        assert_eq!(version_number("0.1.2-dev"), 1_02);
        assert_eq!(version_number("1.2.3-dev"), 1_02_03);
        assert_eq!(version_number("999.99.99"), 9_99_99_99);
    }
}
