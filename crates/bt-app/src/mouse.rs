//! 마우스 보고(xterm 9/1000/1002/1003, 형식 1005/1006/1015/1016). vim·htop·less 같은 프로그램이 마우스를 쓸 때 켠다

use bt_core::config::{MiddleClickAction, MouseShiftCapture};
use bt_term::{CellSide, SelectKind};
use std::time::Duration;

/// 이 클릭·움직임·휠을 프로그램(마우스 보고)에 보내는가. `mouse-reporting`을 끄면 늘 아니오.
/// Shift를 누르면 `mouse-shift-capture`가 정한다: `always`는 프로그램이, `never`는 고르기가 받고, `true`·`false`는
/// 프로그램이 XTSHIFTESCAPE로 정한 값(`program`)이 있으면 그 값을, 없으면 설정 값을 따른다(#327)
pub fn reports_to_program(reporting: bool, capture: MouseShiftCapture, program: Option<bool>, shift: bool) -> bool {
    reporting && (!shift || capture.allows(program))
}

/// 끌면 사각으로 고르게 하는 수식 키인가(Ghostty `surface_mouse.zig` `isRectangleSelectState`, #433).
/// macOS는 Option(alt) 하나, 그 밖에는 Ctrl 또는 Super와 Alt를 함께
pub fn is_rectangle_select(macos: bool, alt: bool, ctrl: bool, super_key: bool) -> bool {
    if macos { alt } else { (ctrl || super_key) && alt }
}

/// 왼쪽 단추를 누른 횟수로 정하는 고르기 종류. 두 번은 낱말, 세 번은 줄, 한 번은 글자이고 사각 수식 키가 있으면 사각이다.
/// 낱말·줄 고르기는 사각을 보지 않는다(Ghostty `SelectionGesture.drag`는 한 번 누르기(cell)에서만 `rectangle`을 쓴다)
pub fn select_kind(clicks: u8, rectangle: bool) -> SelectKind {
    match clicks {
        2 => SelectKind::Word,
        3 => SelectKind::Line,
        _ if rectangle => SelectKind::Block,
        _ => SelectKind::Simple,
    }
}

/// 세 번 누르기가 줄 대신 명령 출력을 고르는가(#700, Ghostty `Surface.zig` 4133줄 `mods.ctrlOrSuper()`): macOS는 ⌘, 그 밖에는 Ctrl
pub fn triple_click_selects_output(macos: bool, ctrl: bool, super_key: bool) -> bool {
    if macos { super_key } else { ctrl }
}

/// 두 번 누르기가 링크 전체를 고를 때 찾을 링크(#701, Ghostty `linkAtPin(pin, null)`): `link` 규칙(화면 글의 URL)만 보고 OSC 8은 보지 않는다.
/// `link-url = false`면 규칙이 없어 낱말 고르기다
pub fn double_click_link_options(link_url: bool) -> Option<bt_term::links::LinkOptions> {
    link_url.then_some(bt_term::links::LinkOptions { url: true, osc8: false })
}

/// 링크 수식 키(⌘, 그 밖에는 Ctrl) 누르기의 진행(#705)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LinkPress {
    #[default]
    Idle,
    /// 마우스 보고 중에 링크를 누를 때 열었다: 짝 뗌을 삼킨다
    Swallow,
    /// 링크 위를 눌렀다(칸 id, 칸, 줄): 같은 칸에서 떼면 연다
    Pending(u64, usize, usize),
}

/// 링크 수식 키 누르기에서 할 일(#705)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkStep {
    /// 링크와 상관없다: 평소처럼 고르기·보고로
    Pass,
    /// 삼킨다(아무 데도 보내지 않는다)
    Swallow,
    /// 링크를 열고 평소 처리도 한다(뗌: 고르기를 마치고 `copy-on-select`)
    OpenPass,
    /// 링크를 열고 삼킨다
    OpenSwallow,
}

/// Ghostty `Surface.zig` 4036줄: 누름은 고르기로 보내고, 뗄 때 다른 칸으로 끌지 않았고 링크 위면 연다.
/// 다른 점 둘: 연속 누르기의 첫 번째(`clicks` 0·1, 0은 횟수에 세지 않은 누름)에서만 연다(Ghostty는 떼는 때마다 열어 ⌘+세 번 누르기에 세 번 연다).
/// 마우스 보고 중이면 지금처럼 누를 때 열고 누름·뗌을 삼킨다(프로그램에 짝 없는 누름이 가지 않게. Ghostty는 보고 중이면 링크를 보지 않는다).
/// `over`는 수식 키를 누른 채 마우스 아래 있는 링크의 (칸 id, 칸, 줄)이고 수식 키가 없으면 None
pub fn link_step(
    state: &mut LinkPress,
    pressed: bool,
    over: Option<(u64, usize, usize)>,
    reporting: bool,
    clicks: u8,
) -> LinkStep {
    let before = std::mem::take(state);
    if !pressed {
        return match before {
            LinkPress::Idle => LinkStep::Pass,
            LinkPress::Swallow => LinkStep::Swallow,
            // 누름은 고르기로 갔으니, 그사이 마우스 보고가 켜졌으면 프로그램에 뗌만 가지 않게 삼킨다
            LinkPress::Pending(id, c, r) if over == Some((id, c, r)) && clicks <= 1 => {
                if reporting {
                    LinkStep::OpenSwallow
                } else {
                    LinkStep::OpenPass
                }
            }
            LinkPress::Pending(..) if reporting => LinkStep::Swallow,
            LinkPress::Pending(..) => LinkStep::Pass,
        };
    }
    match over {
        None => LinkStep::Pass,
        Some(_) if reporting => {
            *state = LinkPress::Swallow;
            LinkStep::OpenSwallow
        }
        Some((id, c, r)) => {
            *state = LinkPress::Pending(id, c, r);
            LinkStep::Pass
        }
    }
}

/// 링크를 두 번 눌러 고른 뒤 끌 때 할 일(#701)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkDrag {
    /// 링크 고르기가 아니다: 보통 끌기
    Normal,
    /// 아직 누른 칸 안이다: 링크 고르기를 그대로 둔다(winit은 떼기 직전에 같은 자리 움직임을 보낸다. AppKit은 움직여야 끌기를 보낸다)
    Keep,
    /// 누른 칸을 벗어났다: 누른 칸의 낱말부터 낱말 단위로 끈다(Ghostty는 두 번 누르기 끌기가 늘 `dragSelectionWord`)
    ToWord,
}

pub fn link_drag(link_cell: Option<(usize, usize)>, cell: (usize, usize)) -> LinkDrag {
    match link_cell {
        None => LinkDrag::Normal,
        Some(at) if at == cell => LinkDrag::Keep,
        Some(_) => LinkDrag::ToWord,
    }
}

/// 두 번·세 번 누르기 간격. 설정이 0이면 플랫폼 값(macOS는 OS 설정, 그 밖에는 500ms)
pub fn repeat_interval(config_ms: u32, platform: Option<Duration>) -> Duration {
    if config_ms > 0 {
        Duration::from_millis(u64::from(config_ms))
    } else {
        platform.unwrap_or(Duration::from_millis(500))
    }
}

/// OS의 두 번 누르기 간격. macOS만 안다
#[cfg(target_os = "macos")]
pub fn platform_repeat_interval() -> Option<Duration> {
    let secs = objc2_app_kit::NSEvent::doubleClickInterval();
    (secs.is_finite() && secs > 0.0).then(|| Duration::from_secs_f64(secs.min(5.0)))
}

#[cfg(not(target_os = "macos"))]
pub fn platform_repeat_interval() -> Option<Duration> {
    None
}

/// 가운데 클릭이 클립보드 붙여넣기인가. `primary-paste`는 macOS에 선택 클립보드가 없어 아니오
pub fn middle_click_pastes(action: MiddleClickAction) -> bool {
    action == MiddleClickAction::ClipboardPaste
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    /// 휠 왼쪽(단추 6, 코드 66). 오른쪽 위 방향이 양수다(Ghostty `up_right` → `six`)
    WheelLeft,
    /// 휠 오른쪽(단추 7, 코드 67)
    WheelRight,
    /// 뒤로 단추(단추 8, 코드 128)
    Back,
    /// 앞으로 단추(단추 9, 코드 129)
    Forward,
    /// 버튼을 누르지 않은 움직임(1003)
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Press,
    Release,
    Motion,
}

/// 어떤 사건을 보고하는가(DECSET 9·1000·1002·1003)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventMode {
    /// 9: 왼쪽·가운데·오른쪽 누름만, 수식 키 없음
    X10,
    /// 1000: 누름과 놓음
    Normal,
    /// 1002: 단추를 누른 채 움직임까지
    Button,
    /// 1003: 모든 움직임
    Any,
}

impl EventMode {
    /// 터미널 모드 스냅숏에서 지금 이벤트 모드. 꺼져 있으면 `None`
    pub fn from_modes(m: &bt_term::Modes) -> Option<Self> {
        if m.mouse_x10 {
            Some(Self::X10)
        } else if m.mouse_motion {
            Some(Self::Any)
        } else if m.mouse_drag {
            Some(Self::Button)
        } else if m.mouse_report {
            Some(Self::Normal)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

/// 좌표를 바꾸는 데 쓰는 크기(실제 픽셀). Ghostty `renderer.Size`에 해당한다.
/// 좌표는 칸(창) 왼쪽 위가 원점인 "표면 좌표", `pad_*`는 거기서 격자 원점까지의 여백이다
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    /// 칸(창) 전체 크기
    pub screen_w: f64,
    pub screen_h: f64,
    pub cell_w: f64,
    pub cell_h: f64,
    pub pad_left: f64,
    pub pad_top: f64,
    pub cols: usize,
    pub rows: usize,
}

/// 인코딩 설정(Ghostty `mouse_encode.Options`)
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub event: EventMode,
    pub format: bt_term::MouseFormat,
    pub geometry: Geometry,
    /// 단추가 하나라도 눌려 있는가(이번 사건 포함). 칸 밖 움직임은 단추를 누른 채일 때만 보낸다
    pub any_button_pressed: bool,
}

/// 눌린 단추를 기억하는 비트. 휠은 누른 채 있지 않아 없다
pub fn button_bit(b: Button) -> Option<u8> {
    match b {
        Button::Left => Some(1),
        Button::Middle => Some(2),
        Button::Right => Some(4),
        Button::Back => Some(8),
        Button::Forward => Some(16),
        _ => None,
    }
}

/// 눌려 있는 단추 중 움직임에 실을 하나(Ghostty는 가장 먼저 찾은 것). 없으면 `None`
pub fn held_button(down: u8) -> Button {
    [
        Button::Left,
        Button::Middle,
        Button::Right,
        Button::Back,
        Button::Forward,
    ]
    .into_iter()
    .find(|&b| button_bit(b).is_some_and(|bit| down & bit != 0))
    .unwrap_or(Button::None)
}

/// 이 사건을 이벤트 모드에서 보고하는가
fn should_report(button: Button, action: Action, event: EventMode) -> bool {
    match event {
        EventMode::X10 => action == Action::Press && matches!(button, Button::Left | Button::Middle | Button::Right),
        EventMode::Normal => action != Action::Motion,
        EventMode::Button => button != Button::None,
        EventMode::Any => true,
    }
}

fn button_code(button: Button, action: Action, mods: Mods, opts: &Options) -> u32 {
    use bt_term::MouseFormat::{Sgr, SgrPixels};
    let mut acc: u32 = if button == Button::None {
        3
    } else if action == Action::Release && !matches!(opts.format, Sgr | SgrPixels) {
        // 옛 형식의 놓음은 늘 3번 단추
        3
    } else {
        match button {
            Button::Left => 0,
            Button::Middle => 1,
            Button::Right => 2,
            Button::WheelUp => 64,
            Button::WheelDown => 65,
            Button::WheelLeft => 66,
            Button::WheelRight => 67,
            Button::Back => 128,
            Button::Forward => 129,
            Button::None => 3,
        }
    };
    // X10는 수식 키를 넣지 않는다
    if opts.event != EventMode::X10 {
        if mods.shift {
            acc += 4;
        }
        if mods.alt {
            acc += 8;
        }
        if mods.ctrl {
            acc += 16;
        }
    }
    if action == Action::Motion {
        acc += 32;
    }
    acc
}

/// 표면 좌표(실제 픽셀) → 0부터 센 칸. 격자 밖은 가장자리 칸으로 당긴다
fn pos_to_cell(pos: (f64, f64), g: &Geometry) -> (usize, usize) {
    let cell = |v: f64, pad: f64, size: f64, n: usize| {
        let c = ((v - pad) / size.max(1.0)).floor().max(0.0);
        (c as usize).min(n.saturating_sub(1))
    };
    (
        cell(pos.0, g.pad_left, g.cell_w, g.cols),
        cell(pos.1, g.pad_top, g.cell_h, g.rows),
    )
}

/// 마우스 사건 하나를 바이트로(Ghostty `mouse_encode.encode`). `pos`는 표면 좌표(실제 픽셀),
/// `last_cell`은 움직임 중복 제거용으로 마지막에 보고한 칸. 보내지 않을 사건은 빈 바이트
pub fn encode(
    button: Button,
    action: Action,
    mods: Mods,
    pos: (f64, f64),
    opts: &Options,
    last_cell: &mut Option<(usize, usize)>,
) -> Vec<u8> {
    use bt_term::MouseFormat;
    if !should_report(button, action, opts.event) {
        return Vec::new();
    }
    let g = &opts.geometry;
    // 칸 밖: 놓음은 늘 보고한다. 그 밖에는 움직임 모드에서 단추를 누른 채일 때만(칸 안에서 눌러 밖으로 끄는 경우)
    let outside = pos.0 < 0.0 || pos.1 < 0.0 || pos.0 > g.screen_w || pos.1 > g.screen_h;
    if action != Action::Release
        && outside
        && (!matches!(opts.event, EventMode::Button | EventMode::Any) || !opts.any_button_pressed)
    {
        return Vec::new();
    }
    let cell = pos_to_cell(pos, g);
    // 움직임은 칸이 바뀔 때만(픽셀 좌표 형식은 늘)
    if action == Action::Motion && opts.format != MouseFormat::SgrPixels && *last_cell == Some(cell) {
        return Vec::new();
    }
    *last_cell = Some(cell);

    let code = button_code(button, action, mods, opts);
    let (x, y) = (cell.0 + 1, cell.1 + 1);
    let end = if action == Action::Release { 'm' } else { 'M' };
    match opts.format {
        MouseFormat::X10 => {
            // 좌표 하나가 한 바이트라 223까지만
            if cell.0 > 222 || cell.1 > 222 {
                return Vec::new();
            }
            vec![0x1b, b'[', b'M', (32 + code) as u8, (32 + x) as u8, (32 + y) as u8]
        }
        MouseFormat::Utf8 => {
            let mut out = b"\x1b[M".to_vec();
            for v in [32 + code as usize, 32 + x, 32 + y] {
                // 단추 코드는 32를 더해 최대 161, 좌표는 칸 수 + 32라 모두 유효한 글자 값이다
                if let Some(c) = char::from_u32(v as u32) {
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
            }
            out
        }
        MouseFormat::Sgr => format!("\x1b[<{code};{x};{y}{end}").into_bytes(),
        MouseFormat::Urxvt => format!("\x1b[{};{x};{y}M", 32 + code).into_bytes(),
        MouseFormat::SgrPixels => {
            // 격자 원점(여백을 뺀 자리)에서 잰 실제 픽셀. 격자 밖이면 음수나 칸 크기를 넘는 값도 그대로 보낸다
            let px = (pos.0 - g.pad_left).round() as i32;
            let py = (pos.1 - g.pad_top).round() as i32;
            format!("\x1b[<{code};{px};{py}{end}").into_bytes()
        }
    }
}

/// 격자 왼쪽 끝에서 잰 가로 픽셀이 칸의 어느 쪽인가(#437). Ghostty `SelectionGesture.zig` `dragSelection`(740~783줄)과 같다:
/// 칸 안 자리가 칸 너비의 60%(반올림) 이상이면 `Right`, 아니면 `Left`. 이 쪽이 글자 고르기에서 누른 칸과 끝 칸을 넣을지 정한다
/// (오른쪽으로 끌 때 누른 칸은 `Left`여야, 끝 칸은 `Right`여야 들어가고 왼쪽으로 끌 때는 반대다).
/// 격자 왼쪽 밖은 첫 칸의 왼쪽, 격자 오른쪽 밖은 마지막 칸의 오른쪽이다(Ghostty는 `max_x`로 붙인다)
pub fn cell_side(x_in_grid: f64, cell_width: u32, cols: usize) -> CellSide {
    if cell_width == 0 || cols == 0 {
        return CellSide::Left;
    }
    let cw = u64::from(cell_width);
    let max_x = cols as u64 * cw - 1;
    // NaN과 음수는 0으로 붙는다(`as`가 그렇게 바꾼다)
    let x = (x_in_grid.floor() as u64).min(max_x);
    let threshold = (f64::from(cell_width) * 0.6).round() as u64;
    if x % cw >= threshold {
        CellSide::Right
    } else {
        CellSide::Left
    }
}

/// 고르지 않은 칸을 왼쪽 단추로 누르면 칸만 고르고 누름은 삼킨다(#500, Ghostty GTK `gcMouseDown`·macOS `SurfaceView_AppKit`).
/// 오른쪽·가운데 단추는 칸을 고른 뒤 평소처럼 보낸다. 누른 자리에 칸이 없으면(경계선 등) 삼킬 것이 없다
pub fn swallow_focus_click(left: bool, pane_under: Option<u64>, focused: Option<u64>) -> bool {
    left && pane_under.is_some() && pane_under != focused
}

/// 삼킨 누름의 짝 떼기를 삼키기 위한 표시(Ghostty `suppress_left_mouse_release`). 창마다 하나
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct FocusClickGuard {
    swallow_release: bool,
}

impl FocusClickGuard {
    /// 왼쪽 단추 누름. 이전 표시는 지우고, 이 누름을 삼키면(`swallow`) 짝 떼기를 위해 새로 켠다. 삼키면 true
    pub fn press_left(&mut self, swallow: bool) -> bool {
        self.swallow_release = swallow;
        swallow
    }

    /// 왼쪽 단추 떼기. 삼킨 누름의 짝이면 표시를 끄고 true
    pub fn release_left(&mut self) -> bool {
        std::mem::take(&mut self.swallow_release)
    }

    /// 창이 포커스를 잃었다: 짝 떼기는 이 창에 오지 않을 수 있어 표시를 지운다(Ghostty도 같다)
    pub fn clear(&mut self) {
        self.swallow_release = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swallow_only_left_press_on_unfocused_pane() {
        assert!(swallow_focus_click(true, Some(2), Some(1)));
        assert!(swallow_focus_click(true, Some(2), None));
        assert!(!swallow_focus_click(true, Some(1), Some(1)), "고른 칸");
        assert!(!swallow_focus_click(true, None, Some(1)), "칸 밖");
        assert!(!swallow_focus_click(false, Some(2), Some(1)), "오른쪽·가운데");
    }

    #[test]
    fn guard_swallows_the_paired_release_once() {
        let mut g = FocusClickGuard::default();
        assert!(g.press_left(true));
        assert!(g.release_left());
        assert!(!g.release_left(), "짝 떼기는 한 번만");
        // 삼키지 않은 누름의 떼기는 넘긴다
        assert!(!g.press_left(false));
        assert!(!g.release_left());
    }

    #[test]
    fn guard_is_cleared_by_next_left_press() {
        let mut g = FocusClickGuard::default();
        assert!(g.press_left(true));
        // 떼기가 오지 않은 채 다음 누름이 온다: 그 누름은 삼키지 않으니 표시도 남지 않는다
        assert!(!g.press_left(false));
        assert!(!g.release_left());
    }

    #[test]
    fn guard_is_cleared_when_window_loses_focus() {
        let mut g = FocusClickGuard::default();
        assert!(g.press_left(true));
        g.clear();
        assert!(!g.release_left());
    }

    /// #437: 칸 안 자리 → 쪽. 칸 너비 10이면 경계는 6(60%): 0~5는 왼쪽, 6~9는 오른쪽(Ghostty `SelectionGesture.zig` 테스트의 cell_width 10과 같다)
    #[test]
    fn cell_side_splits_a_cell_at_sixty_percent() {
        for x in [0.0, 3.0, 5.0, 5.99] {
            assert_eq!(cell_side(x, 10, 80), CellSide::Left, "x={x}");
        }
        for x in [6.0, 9.0, 9.99] {
            assert_eq!(cell_side(x, 10, 80), CellSide::Right, "x={x}");
        }
        // 다음 칸도 같다
        assert_eq!(cell_side(15.0, 10, 80), CellSide::Left);
        assert_eq!(cell_side(16.0, 10, 80), CellSide::Right);
        // 경계가 반올림된다: 너비 8이면 4.8 -> 5
        assert_eq!(cell_side(4.0, 8, 80), CellSide::Left);
        assert_eq!(cell_side(5.0, 8, 80), CellSide::Right);
    }

    /// #437: 격자 밖은 가장자리 칸에 붙는다. 왼쪽 밖은 왼쪽, 오른쪽 밖은 마지막 칸의 오른쪽. 쓸 수 없는 값은 왼쪽
    #[test]
    fn cell_side_clamps_outside_the_grid() {
        assert_eq!(cell_side(-50.0, 10, 80), CellSide::Left);
        assert_eq!(cell_side(f64::NAN, 10, 80), CellSide::Left);
        assert_eq!(cell_side(800.0, 10, 80), CellSide::Right, "격자 오른쪽 밖");
        assert_eq!(cell_side(5000.0, 10, 80), CellSide::Right);
        assert_eq!(cell_side(1.0, 0, 80), CellSide::Left, "칸 너비 0");
        assert_eq!(cell_side(1.0, 10, 0), CellSide::Left, "열 0");
    }

    /// #433: 사각 고르기 수식 키. macOS는 Option 하나, 그 밖에는 Ctrl(또는 Super)+Alt(Ghostty surface_mouse.zig isRectangleSelectState)
    #[test]
    fn rectangle_select_modifiers_follow_ghostty() {
        // macOS: Option(alt)만 보고, 다른 키는 상관없다
        assert!(is_rectangle_select(true, true, false, false));
        assert!(is_rectangle_select(true, true, true, true));
        assert!(!is_rectangle_select(true, false, true, true), "Option 없이는 아니다");
        assert!(!is_rectangle_select(true, false, false, false));
        // 그 밖: Alt만으로는 아니고 Ctrl 또는 Super가 함께여야 한다
        assert!(!is_rectangle_select(false, true, false, false));
        assert!(is_rectangle_select(false, true, true, false));
        assert!(is_rectangle_select(false, true, false, true));
        assert!(!is_rectangle_select(false, false, true, true), "Alt 없이는 아니다");
    }

    /// #433: 누르기 횟수가 낱말·줄을 정하고, 한 번 누르기만 사각 여부를 따른다(낱말·줄 고르기는 사각을 보지 않는다)
    #[test]
    fn link_opens_on_release_in_the_same_cell_and_only_on_the_first_click() {
        let at = Some((7, 3, 1));
        // 누름은 고르기로 가고, 같은 칸에서 떼면 연다
        let mut st = LinkPress::Idle;
        assert_eq!(link_step(&mut st, true, at, false, 0), LinkStep::Pass);
        assert_eq!(st, LinkPress::Pending(7, 3, 1));
        assert_eq!(link_step(&mut st, false, at, false, 1), LinkStep::OpenPass);
        assert_eq!(st, LinkPress::Idle);
        // 두 번째·세 번째 누름의 뗌은 열지 않는다(고르기만)
        assert_eq!(link_step(&mut st, true, at, false, 1), LinkStep::Pass);
        assert_eq!(link_step(&mut st, false, at, false, 2), LinkStep::Pass);
        assert_eq!(link_step(&mut st, true, at, false, 2), LinkStep::Pass);
        assert_eq!(link_step(&mut st, false, at, false, 3), LinkStep::Pass);
        // 횟수에 세지 않은 누름(칸 고르기로 삼킨 누름)의 뗌은 연다
        assert_eq!(link_step(&mut st, true, at, false, 0), LinkStep::Pass);
        assert_eq!(link_step(&mut st, false, at, false, 0), LinkStep::OpenPass);
    }

    #[test]
    fn link_does_not_open_after_dragging_away_or_releasing_the_modifier() {
        let mut st = LinkPress::Idle;
        link_step(&mut st, true, Some((7, 3, 1)), false, 0);
        assert_eq!(
            link_step(&mut st, false, Some((7, 9, 1)), false, 1),
            LinkStep::Pass,
            "다른 칸에서 뗌"
        );
        link_step(&mut st, true, Some((7, 3, 1)), false, 0);
        assert_eq!(
            link_step(&mut st, false, None, false, 1),
            LinkStep::Pass,
            "수식 키를 떼고 뗌"
        );
        link_step(&mut st, true, Some((7, 3, 1)), false, 0);
        assert_eq!(
            link_step(&mut st, false, Some((8, 3, 1)), false, 1),
            LinkStep::Pass,
            "다른 칸(분할)"
        );
        assert_eq!(link_step(&mut st, true, None, false, 0), LinkStep::Pass, "링크 밖 누름");
        assert_eq!(st, LinkPress::Idle);
    }

    #[test]
    fn mouse_reporting_keeps_opening_on_press_and_swallows_both() {
        let mut st = LinkPress::Idle;
        assert_eq!(
            link_step(&mut st, true, Some((7, 3, 1)), true, 0),
            LinkStep::OpenSwallow
        );
        assert_eq!(link_step(&mut st, false, None, true, 0), LinkStep::Swallow);
        assert_eq!(st, LinkPress::Idle);
        assert_eq!(
            link_step(&mut st, false, None, true, 0),
            LinkStep::Pass,
            "짝 없는 뗌은 그대로"
        );
        // 누를 때는 보고가 꺼져 있었고(누름은 고르기로 감) 뗄 때 켜져 있으면, 프로그램은 누름을 받지 않았으므로 뗌도 보내지 않는다
        link_step(&mut st, true, Some((7, 3, 1)), false, 0);
        assert_eq!(
            link_step(&mut st, false, Some((7, 3, 1)), true, 1),
            LinkStep::OpenSwallow
        );
        link_step(&mut st, true, Some((7, 3, 1)), false, 0);
        assert_eq!(
            link_step(&mut st, false, Some((7, 9, 1)), true, 1),
            LinkStep::Swallow,
            "다른 칸이면 열지 않고 삼킨다"
        );
    }

    #[test]
    fn link_selection_survives_same_cell_motion_and_becomes_word_drag() {
        assert_eq!(link_drag(None, (3, 1)), LinkDrag::Normal);
        assert_eq!(link_drag(Some((3, 1)), (3, 1)), LinkDrag::Keep);
        assert_eq!(link_drag(Some((3, 1)), (4, 1)), LinkDrag::ToWord);
        assert_eq!(link_drag(Some((3, 1)), (3, 2)), LinkDrag::ToWord);
    }

    #[test]
    fn double_click_link_looks_only_at_url_rules() {
        assert_eq!(
            double_click_link_options(true),
            Some(bt_term::links::LinkOptions { url: true, osc8: false })
        );
        assert_eq!(double_click_link_options(false), None);
    }

    #[test]
    fn triple_click_output_needs_cmd_on_macos_and_ctrl_elsewhere() {
        assert!(triple_click_selects_output(true, false, true));
        assert!(
            !triple_click_selects_output(true, true, false),
            "macOS의 Ctrl은 아니다(Ctrl+누르기는 메뉴)"
        );
        assert!(triple_click_selects_output(false, true, false));
        assert!(!triple_click_selects_output(false, false, true));
        assert!(!triple_click_selects_output(true, false, false));
    }

    #[test]
    fn select_kind_follows_clicks_and_only_single_click_takes_the_rectangle() {
        use bt_term::SelectKind::*;
        assert_eq!(select_kind(1, false), Simple);
        assert_eq!(select_kind(1, true), Block);
        assert_eq!(select_kind(2, false), Word);
        assert_eq!(select_kind(2, true), Word);
        assert_eq!(select_kind(3, false), Line);
        assert_eq!(select_kind(3, true), Line);
    }

    #[test]
    fn report_decision() {
        use MouseShiftCapture::*;
        // 보고를 끄면 Shift와 상관없이 프로그램에 보내지 않는다
        for c in [False, True, Always, Never] {
            assert!(!reports_to_program(false, c, None, false));
            assert!(!reports_to_program(false, c, Some(true), true));
        }
        // Shift 없이는 켜 있으면 보낸다
        for c in [False, True, Always, Never] {
            assert!(reports_to_program(true, c, None, false));
            assert!(reports_to_program(true, c, Some(false), false));
        }
        // Shift를 누르면 true·always만 보낸다(기본 false는 고르기)
        assert!(!reports_to_program(true, False, None, true));
        assert!(!reports_to_program(true, Never, None, true));
        assert!(reports_to_program(true, True, None, true));
        assert!(reports_to_program(true, Always, None, true));
        // 프로그램이 XTSHIFTESCAPE로 정하면 true·false는 따르고, always·never는 따르지 않는다(#327)
        assert!(reports_to_program(true, False, Some(true), true));
        assert!(!reports_to_program(true, True, Some(false), true));
        assert!(!reports_to_program(true, Never, Some(true), true));
        assert!(reports_to_program(true, Always, Some(false), true));
    }

    #[test]
    fn repeat_interval_picks_config_then_platform() {
        let os = Some(Duration::from_millis(350));
        assert_eq!(repeat_interval(300, os), Duration::from_millis(300));
        assert_eq!(repeat_interval(0, os), Duration::from_millis(350));
        assert_eq!(repeat_interval(0, None), Duration::from_millis(500));
    }

    #[test]
    fn middle_click_actions() {
        assert!(middle_click_pastes(MiddleClickAction::ClipboardPaste));
        assert!(!middle_click_pastes(MiddleClickAction::PrimaryPaste));
        assert!(!middle_click_pastes(MiddleClickAction::Ignore));
    }

    use bt_term::MouseFormat as F;

    // 아래 값은 Ghostty src/input/mouse_encode.zig의 시험 값을 옮긴 것이다(testSize: 화면 1000x1000, 칸 1x1, 여백 없음)
    fn geo() -> Geometry {
        Geometry {
            screen_w: 1000.0,
            screen_h: 1000.0,
            cell_w: 1.0,
            cell_h: 1.0,
            pad_left: 0.0,
            pad_top: 0.0,
            cols: 1000,
            rows: 1000,
        }
    }

    fn opts(event: EventMode, format: F) -> Options {
        Options {
            event,
            format,
            geometry: geo(),
            any_button_pressed: false,
        }
    }

    fn enc(b: Button, a: Action, mods: Mods, pos: (f64, f64), o: &Options) -> Vec<u8> {
        encode(b, a, mods, pos, o, &mut None)
    }

    const NO: Mods = Mods {
        shift: false,
        alt: false,
        ctrl: false,
    };
    const ALL: Mods = Mods {
        shift: true,
        alt: true,
        ctrl: true,
    };

    #[test]
    fn should_report_per_event_mode() {
        use Action::*;
        use Button::*;
        // x10: 왼쪽·가운데·오른쪽 누름만
        for b in [Left, Middle, Right] {
            assert!(should_report(b, Press, EventMode::X10));
        }
        assert!(!should_report(Left, Release, EventMode::X10));
        assert!(!should_report(Left, Motion, EventMode::X10));
        assert!(!should_report(WheelUp, Press, EventMode::X10));
        assert!(!should_report(None, Press, EventMode::X10));
        // normal: 움직임 빼고
        assert!(should_report(Left, Press, EventMode::Normal));
        assert!(should_report(Left, Release, EventMode::Normal));
        assert!(!should_report(Left, Motion, EventMode::Normal));
        // button: 단추가 있어야
        for a in [Press, Release, Motion] {
            assert!(should_report(Left, a, EventMode::Button));
            assert!(!should_report(None, a, EventMode::Button));
            assert!(should_report(None, a, EventMode::Any));
        }
    }

    #[test]
    fn x10_press_left_ignores_modifiers() {
        let o = opts(EventMode::X10, F::X10);
        assert_eq!(
            enc(Button::Left, Action::Press, ALL, (0.0, 0.0), &o),
            [0x1b, b'[', b'M', 32, 33, 33]
        );
    }

    #[test]
    fn x10_event_mode_ignores_release_and_other_buttons() {
        let o = opts(EventMode::X10, F::X10);
        assert!(enc(Button::Left, Action::Release, NO, (0.0, 0.0), &o).is_empty());
        assert!(enc(Button::Back, Action::Press, NO, (0.0, 0.0), &o).is_empty());
    }

    #[test]
    fn normal_ignores_motion_and_button_mode_requires_a_button() {
        let o = opts(EventMode::Normal, F::Sgr);
        assert!(enc(Button::Left, Action::Motion, NO, (0.0, 0.0), &o).is_empty());
        let o = opts(EventMode::Button, F::Sgr);
        assert!(enc(Button::None, Action::Motion, NO, (0.0, 0.0), &o).is_empty());
    }

    #[test]
    fn sgr_release_keeps_button_identity() {
        let o = opts(EventMode::Any, F::Sgr);
        assert_eq!(enc(Button::Right, Action::Release, NO, (4.0, 5.0), &o), b"\x1b[<2;5;6m");
    }

    #[test]
    fn sgr_motion_with_no_button() {
        let o = opts(EventMode::Any, F::Sgr);
        assert_eq!(enc(Button::None, Action::Motion, NO, (1.0, 2.0), &o), b"\x1b[<35;2;3M");
    }

    #[test]
    fn sgr_ctrl_motion_matches_the_old_shape() {
        // 이전 구현의 시험 값: 왼쪽 + Ctrl + 움직임 = 48
        let o = opts(EventMode::Any, F::Sgr);
        let ctrl = Mods { ctrl: true, ..NO };
        assert_eq!(
            enc(Button::Left, Action::Motion, ctrl, (1.0, 1.0), &o),
            b"\x1b[<48;2;2M"
        );
    }

    #[test]
    fn urxvt_with_modifiers_and_release() {
        let o = opts(EventMode::Any, F::Urxvt);
        assert_eq!(enc(Button::Left, Action::Press, ALL, (2.0, 3.0), &o), b"\x1b[60;3;4M");
        // 놓음은 옛 형식처럼 3번 단추
        assert_eq!(enc(Button::Right, Action::Release, NO, (2.0, 3.0), &o), b"\x1b[35;3;4M");
    }

    #[test]
    fn utf8_encodes_large_coordinates() {
        let o = opts(EventMode::Any, F::Utf8);
        let out = enc(Button::Left, Action::Press, NO, (300.0, 400.0), &o);
        assert_eq!(&out[..4], [0x1b, b'[', b'M', 32]);
        let cps: Vec<u32> = std::str::from_utf8(&out[4..]).unwrap().chars().map(u32::from).collect();
        assert_eq!(cps, [333, 433]);
    }

    #[test]
    fn utf8_encodes_extended_mouse_buttons() {
        let o = opts(EventMode::Any, F::Utf8);
        assert_eq!(
            enc(Button::Back, Action::Press, NO, (2.0, 3.0), &o),
            "\x1b[M\u{A0}#$".as_bytes()
        );
    }

    #[test]
    fn x10_coordinate_limit() {
        let o = opts(EventMode::X10, F::X10);
        assert!(enc(Button::Left, Action::Press, NO, (223.0, 0.0), &o).is_empty());
        assert_eq!(enc(Button::Left, Action::Press, NO, (222.0, 0.0), &o).len(), 6);
    }

    #[test]
    fn sgr_wheel_button_mappings() {
        let o = opts(EventMode::Any, F::Sgr);
        for (b, code) in [
            (Button::WheelUp, 64),
            (Button::WheelDown, 65),
            (Button::WheelLeft, 66),
            (Button::WheelRight, 67),
            (Button::Back, 128),
            (Button::Forward, 129),
        ] {
            assert_eq!(
                enc(b, Action::Press, NO, (0.0, 0.0), &o),
                format!("\x1b[<{code};1;1M").into_bytes(),
                "{b:?}"
            );
        }
    }

    #[test]
    fn sgr_pixels_uses_terminal_space_cursor_coordinates() {
        let o = opts(EventMode::Any, F::SgrPixels);
        assert_eq!(
            enc(Button::Left, Action::Press, NO, (10.0, 20.0), &o),
            b"\x1b[<0;10;20M"
        );
        assert_eq!(
            enc(Button::Right, Action::Release, NO, (10.0, 20.0), &o),
            b"\x1b[<2;10;20m"
        );
    }

    #[test]
    fn sgr_pixels_are_measured_from_the_grid_origin_in_real_pixels() {
        // 여백 12x8 실제 픽셀, 칸 10x20: 표면 (52, 48)은 격자 원점에서 (40, 40) 픽셀이다
        let g = Geometry {
            screen_w: 800.0,
            screen_h: 600.0,
            cell_w: 10.0,
            cell_h: 20.0,
            pad_left: 12.0,
            pad_top: 8.0,
            cols: 76,
            rows: 29,
        };
        let o = Options {
            geometry: g,
            ..opts(EventMode::Any, F::SgrPixels)
        };
        assert_eq!(
            enc(Button::Left, Action::Press, NO, (52.0, 48.0), &o),
            b"\x1b[<0;40;40M"
        );
        // 여백 안은 음수 그대로(Ghostty는 터미널 좌표를 자르지 않는다)
        assert_eq!(enc(Button::Left, Action::Press, NO, (2.0, 4.0), &o), b"\x1b[<0;-10;-4M");
        // 같은 자리를 칸 형식으로 보내면 5열·3행
        let o = Options { format: F::Sgr, ..o };
        assert_eq!(enc(Button::Left, Action::Press, NO, (52.0, 48.0), &o), b"\x1b[<0;5;3M");
    }

    #[test]
    fn position_exactly_at_viewport_boundary_is_encoded_in_final_cell() {
        let g = Geometry {
            screen_w: 10.0,
            screen_h: 10.0,
            cell_w: 2.0,
            cell_h: 2.0,
            pad_left: 0.0,
            pad_top: 0.0,
            cols: 5,
            rows: 5,
        };
        let o = Options {
            geometry: g,
            ..opts(EventMode::Any, F::Sgr)
        };
        assert_eq!(enc(Button::Left, Action::Press, NO, (10.0, 10.0), &o), b"\x1b[<0;5;5M");
    }

    #[test]
    fn outside_viewport_motion_needs_a_pressed_button() {
        let o = opts(EventMode::Any, F::Sgr);
        assert!(enc(Button::Left, Action::Motion, NO, (-1.0, -1.0), &o).is_empty());
        let o = Options {
            any_button_pressed: true,
            ..o
        };
        assert_eq!(
            enc(Button::Left, Action::Motion, NO, (-1.0, -1.0), &o),
            b"\x1b[<32;1;1M"
        );
        // 움직임 모드가 아니면(normal) 칸 밖 누름을 버린다. 놓음은 어디서든 보고한다
        let o = opts(EventMode::Normal, F::Sgr);
        assert!(enc(Button::Left, Action::Press, NO, (-1.0, 5.0), &o).is_empty());
        assert_eq!(enc(Button::Left, Action::Release, NO, (-1.0, 5.0), &o), b"\x1b[<0;1;6m");
    }

    #[test]
    fn motion_is_deduped_by_last_cell_except_sgr_pixels() {
        let mut last = None;
        let go = |o: &Options, last: &mut Option<(usize, usize)>| {
            encode(Button::Left, Action::Motion, NO, (5.0, 6.0), o, last)
        };
        assert!(!go(&opts(EventMode::Any, F::Sgr), &mut last).is_empty());
        assert!(go(&opts(EventMode::Any, F::Sgr), &mut last).is_empty());
        assert!(!go(&opts(EventMode::Any, F::SgrPixels), &mut last).is_empty());
    }

    #[test]
    fn legacy_encoding_and_bounds() {
        let o = opts(EventMode::Normal, F::X10);
        assert_eq!(
            enc(Button::Left, Action::Press, NO, (0.0, 0.0), &o),
            [0x1b, b'[', b'M', 32, 33, 33]
        );
        assert_eq!(enc(Button::Left, Action::Release, NO, (0.0, 0.0), &o)[3], 35);
        assert!(enc(Button::Left, Action::Press, NO, (300.0, 0.0), &o).is_empty());
    }

    #[test]
    fn event_mode_comes_from_the_snapshot() {
        let m = |f: fn(&mut bt_term::Modes)| {
            let mut m = bt_term::Modes::default();
            f(&mut m);
            EventMode::from_modes(&m)
        };
        assert_eq!(m(|_| {}), None);
        assert_eq!(m(|m| m.mouse_x10 = true), Some(EventMode::X10));
        assert_eq!(m(|m| m.mouse_report = true), Some(EventMode::Normal));
        assert_eq!(m(|m| m.mouse_drag = true), Some(EventMode::Button));
        assert_eq!(m(|m| m.mouse_motion = true), Some(EventMode::Any));
    }
}
