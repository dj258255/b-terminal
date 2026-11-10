//! 사용자 셰이더(`custom-shader`)에 넘길 값을 앱 상태에서 만드는 순수한 부분(#373, 앱 연결 #376).
//! 렌더러(bt-gpu)와 창에 기대지 않아 시계·창 없이 시험한다. 값의 규칙은 Ghostty의 `generic.zig`
//! (`updateCustomShaderUniformsFromState`·`updateCustomShaderUniformsForFrame`·`setFocus`·`animationWake`)를 따른다.
//! 앱은 GPU 프레임마다 `ShaderState::frame`으로 값을 모아 `to_uniforms`로 렌더러에 넘기고(`gpu_frame.rs`),
//! `about_to_wait`가 `shader_wake`로 움직이는 창을 깨운다.

use std::io::Read;
use std::time::Instant;

use bt_core::config::{CustomShaderAnimation, ShaderPath};
use bt_core::{Rgb, Theme};
use bt_render::CursorStyle;

/// Ghostty `draw_interval_ms`(generic.zig 1109줄): 셰이더가 움직이는 동안 그리기를 깨우는 간격
pub const DRAW_INTERVAL_MS: u64 = 8;

/// 셰이더 파일 하나의 크기 상한(1MiB). 읽기 전에 메타데이터로 먼저 본다
pub const SHADER_FILE_LIMIT: u64 = 1 << 20;

/// 셰이더 한 프레임의 유니폼 값. 필드는 `bt_gpu::ShaderUniforms`와 같은 이름·같은 타입이다(주 세션이 하나씩 옮겨 잇는다)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShaderInputs {
    /// iTime: 첫 프레임부터 초
    pub time: f32,
    /// iTimeDelta: 앞 프레임부터 초
    pub time_delta: f32,
    /// iFrame: 지금까지 그린 프레임 수
    pub frame: i32,
    /// iCurrentCursor: xy 모서리, zw 폭·높이(픽셀)
    pub current_cursor: [f32; 4],
    pub previous_cursor: [f32; 4],
    pub current_cursor_color: [f32; 4],
    pub previous_cursor_color: [f32; 4],
    /// iCurrentCursorStyle: 0 블록, 1 빈 블록, 2 막대, 3 밑줄, 4 자물쇠
    pub current_cursor_style: i32,
    pub previous_cursor_style: i32,
    /// iCursorVisible: 1 보임, 0 숨김
    pub cursor_visible: i32,
    /// iTimeCursorChange: 커서가 자리·색을 바꾼 때의 iTime
    pub time_cursor_change: f32,
    /// iTimeFocus: 초점을 얻은 때의 iTime(Ghostty: 얻을 때만 옮긴다)
    pub time_focus: f32,
    /// iFocus: 1 초점, 0 아님
    pub focus: i32,
    pub palette: [[f32; 3]; 256],
    pub background: [f32; 3],
    pub foreground: [f32; 3],
    pub cursor_color: [f32; 3],
    pub cursor_text: [f32; 3],
    pub selection_background: [f32; 3],
    pub selection_foreground: [f32; 3],
}

impl Default for ShaderInputs {
    fn default() -> Self {
        Self {
            time: 0.0,
            time_delta: 0.0,
            frame: 0,
            current_cursor: [0.0; 4],
            previous_cursor: [0.0; 4],
            current_cursor_color: [0.0; 4],
            previous_cursor_color: [0.0; 4],
            current_cursor_style: 0,
            previous_cursor_style: 0,
            cursor_visible: 0,
            time_cursor_change: 0.0,
            time_focus: 0.0,
            focus: 0,
            palette: [[0.0; 3]; 256],
            background: [0.0; 3],
            foreground: [0.0; 3],
            cursor_color: [0.0; 3],
            cursor_text: [0.0; 3],
            selection_background: [0.0; 3],
            selection_foreground: [0.0; 3],
        }
    }
}

/// 프레임마다 시간을 센다(Ghostty `updateCustomShaderUniformsForFrame`, generic.zig 2435줄)
#[derive(Debug, Default, Clone, Copy)]
pub struct ShaderClock {
    first: Option<Instant>,
    last: Option<Instant>,
    frame: i32,
}

/// 커서 자리를 만들 때 필요한 칸·커서 치수(픽셀). `bt_render::font::CellMetrics`의 같은 이름 값을 옮긴다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorMetrics {
    pub cell_w: u32,
    pub cell_h: u32,
    /// 커서 높이(`adjust-cursor-height`). 셀 높이보다 크면 셀 높이로 자른다
    pub cursor_h: u32,
    /// 막대 커서 두께
    pub beam: u32,
    /// 선 두께(밑줄 커서는 이것의 두 배)
    pub stroke: u32,
}

/// 앞 커서와 지금 커서를 들고 있는다(Ghostty의 `previous_cursor*`와 `cursor_change_time`)
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct CursorTracker {
    pub current: [f32; 4],
    pub previous: [f32; 4],
    pub current_color: [f32; 4],
    pub previous_color: [f32; 4],
    pub current_style: i32,
    pub previous_style: i32,
    pub visible: i32,
    pub change_time: f32,
}

/// 초점(Ghostty `setFocus`·`custom_shader_focused_changed`)
#[derive(Debug, Clone, Copy)]
pub struct FocusTracker {
    focus: bool,
    time_focus: f32,
    pending: bool,
}

impl Default for FocusTracker {
    /// Ghostty는 처음에 초점이 있다고 본다(`.focus = 1, // assume focused initially`, generic.zig 807줄)
    fn default() -> Self {
        Self {
            focus: true,
            time_focus: 0.0,
            pending: false,
        }
    }
}

/// OSC 4·10·11·12로 프로그램이 바꾼 색. 바뀐 것만 담는다
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ColorOverrides {
    /// (번호, 색)
    pub palette: Vec<(u8, Rgb)>,
    pub foreground: Option<Rgb>,
    pub background: Option<Rgb>,
    pub cursor: Option<Rgb>,
}

/// 셰이더에 넘길 색(0~1)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShaderColors {
    pub palette: [[f32; 3]; 256],
    pub background: [f32; 3],
    pub foreground: [f32; 3],
    pub cursor_color: [f32; 3],
    pub cursor_text: [f32; 3],
    pub selection_background: [f32; 3],
    pub selection_foreground: [f32; 3],
}

/// 설정의 `custom-shader` 경로들을 읽는다. 반환은 (이름=경로, 소스) 목록(적은 순서)과 경고.
/// 한 파일이 못 읽히면 그 파일만 건너뛴다. 크기 상한은 읽기 전에 메타데이터로 보고, 없는 파일은 `?`면 조용히 건너뛴다
pub fn load_shader_files(paths: &[ShaderPath]) -> (Vec<(String, String)>, Vec<String>) {
    let mut files = Vec::new();
    let mut warnings = Vec::new();
    for sp in paths {
        let name = sp.path.display().to_string();
        let meta = match std::fs::metadata(&sp.path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && sp.optional => continue,
            Err(e) => {
                warnings.push(format!("custom-shader {name}: 읽지 못했습니다: {e}"));
                continue;
            }
        };
        if !meta.is_file() {
            warnings.push(format!("custom-shader {name}: 보통 파일이 아닙니다"));
            continue;
        }
        if meta.len() > SHADER_FILE_LIMIT {
            warnings.push(format!(
                "custom-shader {name}: {}바이트라 1MiB 상한을 넘습니다",
                meta.len()
            ));
            continue;
        }
        // 메타데이터를 본 뒤에 커졌을 수 있으니 읽는 양도 상한 +1로 자른다
        let mut bytes = Vec::new();
        let read = std::fs::File::open(&sp.path).and_then(|f| f.take(SHADER_FILE_LIMIT + 1).read_to_end(&mut bytes));
        if let Err(e) = read {
            warnings.push(format!("custom-shader {name}: 읽지 못했습니다: {e}"));
            continue;
        }
        if bytes.len() as u64 > SHADER_FILE_LIMIT {
            warnings.push(format!("custom-shader {name}: 1MiB 상한을 넘습니다"));
            continue;
        }
        match String::from_utf8(bytes) {
            Ok(src) => files.push((name, src)),
            Err(_) => warnings.push(format!("custom-shader {name}: UTF-8 글이 아닙니다")),
        }
    }
    (files, warnings)
}

impl ShaderClock {
    /// 이번 프레임의 (time, time_delta, frame). Ghostty와 같다: 첫 프레임은 time·delta가 0이고 frame은 1부터
    /// (그리기 전에 1을 더한다). 시간은 나노초를 f32로 바꿔 10^9로 나눈 값이다(`since_ns / ns_per_s`)
    pub fn frame(&mut self, now: Instant) -> (f32, f32, i32) {
        let first = *self.first.get_or_insert(now);
        let last = self.last.unwrap_or(now);
        let time = now.saturating_duration_since(first).as_nanos() as f32 / 1_000_000_000.0;
        let delta = now.saturating_duration_since(last).as_nanos() as f32 / 1_000_000_000.0;
        self.last = Some(now);
        self.frame = self.frame.wrapping_add(1);
        (time, delta, self.frame)
    }
}

/// 커서 자리 `[x, y, 폭, 높이]`(픽셀). Ghostty(generic.zig 2473~2522줄, y가 아래로 자라는 Metal 길)와 같은 꼴이다:
/// x는 커서 글리프의 왼쪽 모서리, y는 **아래쪽(+Y) 모서리**(칸 위쪽에서 잰 글리프 위쪽에 글리프 높이를 더한 값),
/// 여백(`origin`은 격자 왼쪽 위의 픽셀, 여백 포함)을 더한다. 글리프 크기는 bt-render가 그리는 모양을 따른다:
/// 블록·빈 블록·자물쇠는 칸(넓은 글자면 두 칸) 전체, 막대는 `beam` 폭, 밑줄은 `stroke*2` 두께로 아래에 붙는다.
/// 높이는 `cursor_h`(칸 높이로 자름)이고 칸 가운데에 둔다
pub fn cursor_rect(
    col: usize,
    row: usize,
    wide: bool,
    style: CursorStyle,
    origin: (i32, i32),
    m: CursorMetrics,
) -> [f32; 4] {
    let span = if wide { 2 } else { 1 };
    let cell_w = m.cell_w * span;
    let ch = m.cursor_h.min(m.cell_h);
    let x = origin.0 + (col as u32 * m.cell_w) as i32;
    let top = origin.1 + (row as u32 * m.cell_h) as i32 + ((m.cell_h - ch) / 2) as i32;
    let (w, h, dy) = match style {
        CursorStyle::Block | CursorStyle::Hollow | CursorStyle::Lock => (cell_w, ch, 0),
        CursorStyle::Beam => (m.beam.min(cell_w), ch, 0),
        CursorStyle::Underline => {
            let t = (m.stroke * 2).min(ch);
            (cell_w, t, ch - t)
        }
    };
    [x as f32, (top + dy as i32 + h as i32) as f32, w as f32, h as f32]
}

/// 커서 색(0~1, a 포함). Ghostty: 초점이 없으면 255, 있으면 `ceil(255 * cursor-opacity)`(generic.zig 3561~3564줄)
pub fn cursor_rgba(c: Rgb, opacity: f32, focused: bool) -> [f32; 4] {
    let a = if focused {
        (255.0 * opacity.clamp(0.0, 1.0)).ceil()
    } else {
        255.0
    };
    [c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0, a / 255.0]
}

impl CursorTracker {
    /// 화면 상태가 바뀔 때 부른다(Ghostty `updateCustomShaderUniformsFromState`, 2423~2429줄). 앞 모양은
    /// 부를 때마다 지금 모양으로 옮겨지니(모양이 같아도) 상태가 바뀌었을 때만 부른다
    pub fn set_state(&mut self, style: i32, visible: bool) {
        self.visible = visible as i32;
        self.previous_style = self.current_style;
        self.current_style = style;
    }

    /// 프레임마다 한 번(Ghostty `updateCustomShaderUniformsForFrame`, 2473~2541줄). `rect`는 `cursor_rect`의 값이고
    /// 커서 글리프가 없으면(숨김·화면 밖) `None`이라 아무것도 바꾸지 않는다. 자리나 색이 앞과 다르면 앞 값을 옮기고
    /// `change_time`을 지금 `time`으로 한다
    pub fn set_glyph(&mut self, time: f32, rect: Option<[f32; 4]>, color: [f32; 4]) {
        let Some(rect) = rect else { return };
        if rect != self.current || color != self.current_color {
            self.previous = self.current;
            self.previous_color = self.current_color;
            self.current = rect;
            self.current_color = color;
            self.change_time = time;
        }
    }
}

impl FocusTracker {
    /// 초점이 바뀌었다고 알린다(Ghostty `setFocus`). 같은 값이면 바뀐 것이 아니다
    pub fn set_focus(&mut self, focus: bool) {
        if self.focus == focus {
            return;
        }
        self.focus = focus;
        self.pending = true;
    }

    /// 프레임마다 한 번: (iFocus, iTimeFocus). Ghostty는 초점을 **얻었을 때만** 그 프레임의 time으로 옮긴다
    /// (generic.zig 2546~2554줄). 잃었을 때는 옮기지 않고, 아직 얻지 못했으면 바뀜 표시를 남겨 둔다
    pub fn frame(&mut self, time: f32) -> (i32, f32) {
        if self.pending && self.focus {
            self.time_focus = time;
            self.pending = false;
        }
        (self.focus as i32, self.time_focus)
    }
}

fn unit(c: Rgb) -> [f32; 3] {
    [c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0]
}

/// 테마와 프로그램이 OSC로 바꾼 색에서 셰이더 색을 만든다(generic.zig 2353~2421줄). 바꾼 값이 있으면 그것이 이긴다.
/// Ghostty는 커서 글자색·선택색을 설정에 정했을 때만 채우고 아니면 0으로 둔다. b-terminal 테마는 늘 값이 있어 그 값을 쓰고,
/// 선택 글자색만 정하지 않았으면(`None`) 0이다
pub fn shader_colors(theme: &Theme, ov: &ColorOverrides) -> ShaderColors {
    let mut palette = [[0.0; 3]; 256];
    for (i, slot) in palette.iter_mut().enumerate() {
        *slot = unit(theme.indexed(i as u8));
    }
    for (i, c) in &ov.palette {
        palette[*i as usize] = unit(*c);
    }
    ShaderColors {
        palette,
        background: unit(ov.background.unwrap_or(theme.background)),
        foreground: unit(ov.foreground.unwrap_or(theme.foreground)),
        cursor_color: unit(ov.cursor.unwrap_or(theme.cursor)),
        cursor_text: unit(theme.cursor_text),
        selection_background: unit(theme.selection_background),
        selection_foreground: theme.selection_foreground.map_or([0.0; 3], unit),
    }
}

impl ShaderInputs {
    /// 시간 `(time, delta, frame)`, 커서, 초점 `(focus, time_focus)`, 색을 한 프레임 값으로 모은다
    pub fn assemble(clock: (f32, f32, i32), cursor: &CursorTracker, focus: (i32, f32), colors: &ShaderColors) -> Self {
        Self {
            time: clock.0,
            time_delta: clock.1,
            frame: clock.2,
            current_cursor: cursor.current,
            previous_cursor: cursor.previous,
            current_cursor_color: cursor.current_color,
            previous_cursor_color: cursor.previous_color,
            current_cursor_style: cursor.current_style,
            previous_cursor_style: cursor.previous_style,
            cursor_visible: cursor.visible,
            time_cursor_change: cursor.change_time,
            time_focus: focus.1,
            focus: focus.0,
            palette: colors.palette,
            background: colors.background,
            foreground: colors.foreground,
            cursor_color: colors.cursor_color,
            cursor_text: colors.cursor_text,
            selection_background: colors.selection_background,
            selection_foreground: colors.selection_foreground,
        }
    }
}

/// 셰이더가 움직여야 하나(Ghostty `animationWake`, generic.zig 1136~1146줄): 셰이더가 없으면 아니고,
/// `false`는 아니고, `always`는 늘, `true`는 초점이 있을 때만. 움직이면 `DRAW_INTERVAL_MS`마다 그리기를 깨운다
pub fn shader_animates(has_shaders: bool, setting: CustomShaderAnimation, focused: bool) -> bool {
    has_shaders
        && match setting {
            CustomShaderAnimation::False => false,
            CustomShaderAnimation::Always => true,
            CustomShaderAnimation::True => focused,
        }
}

/// b-terminal 화면의 커서 모양(터미널 상태)을 셰이더 번호로. Ghostty는 `iCurrentCursorStyle`에 터미널의 모양을 넣는다
/// (`fromTerminal(cursor.visual_style)`, generic.zig 2427줄): 초점이 없을 때의 빈 블록이나 암호 입력의 자물쇠처럼
/// 렌더러가 바꿔 그리는 모양이 아니다
pub fn screen_style_code(s: bt_term::CursorShape) -> i32 {
    match s {
        bt_term::CursorShape::Block => 0,
        bt_term::CursorShape::HollowBlock => 1,
        bt_term::CursorShape::Beam => 2,
        bt_term::CursorShape::Underline => 3,
    }
}

/// 고른 칸의 커서: 터미널 상태(모양 번호, 보임)와 이번 프레임에 그려지는 커서(자리, 색). 그려지지 않으면(숨김·깜빡임) `glyph`가 없다
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameCursor {
    pub style: i32,
    pub visible: bool,
    pub glyph: Option<([f32; 4], [f32; 4])>,
}

/// 창 하나의 셰이더 상태(#376). 셰이더를 다시 걸면 새로 만든다(시간이 0부터 다시 간다)
#[derive(Debug, Default, Clone, Copy)]
pub struct ShaderState {
    pub clock: ShaderClock,
    pub cursor: CursorTracker,
    pub focus: FocusTracker,
    /// 셰이더가 움직이는 동안 다음에 그리기를 깨울 때
    pub wake: Option<Instant>,
}

impl ShaderState {
    /// 한 프레임의 값. `dirty`는 화면 내용이 바뀐 프레임이다: Ghostty는 터미널 상태가 dirty일 때만 커서 모양·보임을 옮기고
    /// (`updateCustomShaderUniformsFromState`, 2343~2348줄) 그때마다 앞 모양을 지금 모양으로 민다.
    /// 시간·커서 자리·초점은 프레임마다(`updateCustomShaderUniformsForFrame`)
    pub fn frame(
        &mut self,
        now: Instant,
        dirty: bool,
        focused: bool,
        cursor: &FrameCursor,
        colors: &ShaderColors,
    ) -> ShaderInputs {
        let clock = self.clock.frame(now);
        if dirty {
            self.cursor.set_state(cursor.style, cursor.visible);
        }
        let (rect, color) = cursor.glyph.map_or((None, [0.0; 4]), |(r, c)| (Some(r), c));
        self.cursor.set_glyph(clock.0, rect, color);
        self.focus.set_focus(focused);
        let focus = self.focus.frame(clock.0);
        ShaderInputs::assemble(clock, &self.cursor, focus, colors)
    }
}

impl ShaderInputs {
    /// 렌더러가 받는 값으로 옮긴다(같은 이름·같은 타입)
    pub fn to_uniforms(self) -> bt_gpu::ShaderUniforms {
        bt_gpu::ShaderUniforms {
            time: self.time,
            time_delta: self.time_delta,
            frame: self.frame,
            current_cursor: self.current_cursor,
            previous_cursor: self.previous_cursor,
            current_cursor_color: self.current_cursor_color,
            previous_cursor_color: self.previous_cursor_color,
            current_cursor_style: self.current_cursor_style,
            previous_cursor_style: self.previous_cursor_style,
            cursor_visible: self.cursor_visible,
            time_cursor_change: self.time_cursor_change,
            time_focus: self.time_focus,
            focus: self.focus,
            palette: self.palette,
            background: self.background,
            foreground: self.foreground,
            cursor_color: self.cursor_color,
            cursor_text: self.cursor_text,
            selection_background: self.selection_background,
            selection_foreground: self.selection_foreground,
        }
    }
}

/// 셰이더 애니메이션 깨우기(창 하나). 움직이지 않으면 깨우지 않는다. 움직이면 깨울 때가 됐거나 처음이면 지금 그리기를 요청하고
/// 다음을 `DRAW_INTERVAL_MS` 뒤로 잡는다. 돌려주는 값: (지금 그리기를 요청할지, 다음 깨울 때)
pub fn shader_wake(next: Option<Instant>, now: Instant, animates: bool) -> (bool, Option<Instant>) {
    if !animates {
        return (false, None);
    }
    match next {
        Some(t) if t > now => (false, Some(t)),
        _ => (true, Some(now + std::time::Duration::from_millis(DRAW_INTERVAL_MS))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bt-shader-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sp(path: PathBuf, optional: bool) -> ShaderPath {
        ShaderPath { path, optional }
    }

    // ---- 시간 ----

    #[test]
    fn clock_first_frame_is_time_zero_delta_zero_frame_one() {
        let t0 = Instant::now();
        let mut c = ShaderClock::default();
        // Ghostty: 첫 프레임의 time과 delta는 0, frame은 그리기 전에 1을 더하므로 1부터
        assert_eq!(c.frame(t0), (0.0, 0.0, 1));
    }

    #[test]
    fn clock_time_is_since_first_and_delta_since_previous() {
        let t0 = Instant::now();
        let mut c = ShaderClock::default();
        c.frame(t0);
        let (t, d, f) = c.frame(t0 + Duration::from_millis(500));
        assert_eq!((t, d, f), (0.5, 0.5, 2));
        let (t, d, f) = c.frame(t0 + Duration::from_millis(750));
        assert_eq!((t, d, f), (0.75, 0.25, 3));
    }

    // ---- 커서 ----

    fn metrics() -> CursorMetrics {
        CursorMetrics {
            cell_w: 10,
            cell_h: 20,
            cursor_h: 20,
            beam: 2,
            stroke: 1,
        }
    }

    #[test]
    fn cursor_rect_block_is_the_cell_with_y_as_the_bottom_edge() {
        // 칸 (3, 2), 여백 (5, 7). 칸 왼쪽 위는 (5+30, 7+40). +Y(아래쪽) 모서리는 거기에 칸 높이를 더한 값
        let r = cursor_rect(3, 2, false, CursorStyle::Block, (5, 7), metrics());
        assert_eq!(r, [35.0, 67.0, 10.0, 20.0]);
    }

    #[test]
    fn cursor_rect_wide_char_takes_two_cells() {
        let r = cursor_rect(0, 0, true, CursorStyle::Block, (0, 0), metrics());
        assert_eq!(r, [0.0, 20.0, 20.0, 20.0]);
        let r = cursor_rect(0, 0, true, CursorStyle::Hollow, (0, 0), metrics());
        assert_eq!(r, [0.0, 20.0, 20.0, 20.0]);
    }

    #[test]
    fn cursor_rect_beam_and_underline_follow_the_drawn_shape() {
        let m = metrics();
        // 막대: 폭 beam, 높이는 커서 높이
        assert_eq!(
            cursor_rect(1, 0, false, CursorStyle::Beam, (0, 0), m),
            [10.0, 20.0, 2.0, 20.0]
        );
        // 밑줄: 두께 stroke*2, 칸 아래에 붙는다. +Y 모서리는 칸 아래
        assert_eq!(
            cursor_rect(0, 1, false, CursorStyle::Underline, (0, 0), m),
            [0.0, 40.0, 10.0, 2.0]
        );
    }

    #[test]
    fn cursor_rect_adjusted_height_is_centered_in_the_cell() {
        let m = CursorMetrics {
            cursor_h: 10,
            ..metrics()
        };
        // 높이 10은 칸(20) 가운데: 위에서 5, +Y 모서리는 5+10
        assert_eq!(
            cursor_rect(0, 0, false, CursorStyle::Block, (0, 0), m),
            [0.0, 15.0, 10.0, 10.0]
        );
        // 커서가 칸보다 높아도 칸 높이로 자른다
        let m = CursorMetrics {
            cursor_h: 99,
            ..metrics()
        };
        assert_eq!(cursor_rect(0, 0, false, CursorStyle::Block, (0, 0), m)[3], 20.0);
    }

    #[test]
    fn cursor_rgba_alpha_follows_opacity_only_when_focused() {
        let c = Rgb::new(255, 0, 51);
        assert_eq!(cursor_rgba(c, 1.0, true), [1.0, 0.0, 0.2, 1.0]);
        // ceil(255 * 0.5) = 128
        assert_eq!(cursor_rgba(c, 0.5, true)[3], 128.0 / 255.0);
        // 초점이 없으면 불투명도와 상관없이 255
        assert_eq!(cursor_rgba(c, 0.5, false)[3], 1.0);
    }

    #[test]
    fn cursor_tracker_moves_previous_and_stamps_change_time_when_the_rect_changes() {
        let mut t = CursorTracker::default();
        let red = [1.0, 0.0, 0.0, 1.0];
        let a = [0.0, 20.0, 10.0, 20.0];
        let b = [10.0, 20.0, 10.0, 20.0];
        t.set_glyph(1.5, Some(a), red);
        assert_eq!((t.current, t.previous), (a, [0.0; 4]));
        assert_eq!(t.change_time, 1.5);
        t.set_glyph(2.5, Some(b), red);
        assert_eq!((t.current, t.previous), (b, a));
        assert_eq!(t.change_time, 2.5);
    }

    #[test]
    fn cursor_tracker_same_rect_and_color_keeps_everything() {
        let mut t = CursorTracker::default();
        let red = [1.0, 0.0, 0.0, 1.0];
        let a = [0.0, 20.0, 10.0, 20.0];
        t.set_glyph(1.0, Some(a), red);
        t.set_glyph(9.0, Some(a), red);
        assert_eq!(t.change_time, 1.0);
        assert_eq!(t.previous, [0.0; 4]);
    }

    #[test]
    fn cursor_tracker_color_only_change_counts() {
        let mut t = CursorTracker::default();
        let a = [0.0, 20.0, 10.0, 20.0];
        let red = [1.0, 0.0, 0.0, 1.0];
        let blue = [0.0, 0.0, 1.0, 1.0];
        t.set_glyph(1.0, Some(a), red);
        t.set_glyph(3.0, Some(a), blue);
        assert_eq!((t.current_color, t.previous_color), (blue, red));
        assert_eq!(t.change_time, 3.0);
        assert_eq!(t.current, a);
        assert_eq!(t.previous, a);
    }

    #[test]
    fn cursor_tracker_without_a_cursor_glyph_changes_nothing() {
        let mut t = CursorTracker::default();
        let a = [0.0, 20.0, 10.0, 20.0];
        t.set_glyph(1.0, Some(a), [1.0; 4]);
        t.set_glyph(5.0, None, [0.0; 4]);
        assert_eq!(t.current, a);
        assert_eq!(t.change_time, 1.0);
    }

    #[test]
    fn cursor_tracker_state_shifts_style_each_time_and_sets_visibility() {
        let mut t = CursorTracker::default();
        t.set_state(2, true);
        assert_eq!((t.current_style, t.previous_style, t.visible), (2, 0, 1));
        t.set_state(3, false);
        assert_eq!((t.current_style, t.previous_style, t.visible), (3, 2, 0));
        // Ghostty는 상태가 바뀔 때마다 앞 모양을 지금 모양으로 옮긴다(같은 모양이어도)
        t.set_state(3, false);
        assert_eq!((t.current_style, t.previous_style), (3, 3));
    }

    // ---- 초점 ----

    #[test]
    fn focus_starts_focused_with_time_zero() {
        let mut f = FocusTracker::default();
        assert_eq!(f.frame(4.0), (1, 0.0));
    }

    #[test]
    fn focus_time_is_set_on_gain_only_not_on_loss() {
        let mut f = FocusTracker::default();
        f.set_focus(false);
        assert_eq!(f.frame(2.0), (0, 0.0), "잃을 때는 시간을 옮기지 않는다");
        assert_eq!(f.frame(3.0), (0, 0.0));
        f.set_focus(true);
        assert_eq!(f.frame(4.0), (1, 4.0), "얻을 때 그 프레임의 time");
        assert_eq!(f.frame(5.0), (1, 4.0), "한 번만");
    }

    #[test]
    fn focus_lost_then_regained_before_a_frame_still_stamps_on_the_next_frame() {
        let mut f = FocusTracker::default();
        f.set_focus(false);
        f.set_focus(true);
        assert_eq!(f.frame(6.0), (1, 6.0));
        // 같은 값을 다시 알려도 바뀐 것이 아니다
        f.set_focus(true);
        assert_eq!(f.frame(7.0), (1, 6.0));
    }

    // ---- 색 ----

    #[test]
    fn palette_covers_ansi_cube_and_gray() {
        let theme = Theme::ghostty_default();
        let c = shader_colors(&theme, &ColorOverrides::default());
        let f = |c: Rgb| [c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0];
        // 0~15는 테마 팔레트
        for i in 0..16 {
            assert_eq!(c.palette[i], f(theme.palette[i]), "{i}");
        }
        // 16~231 정육면체, 232~255 회색: 테마의 256색표(indexed)와 같다
        for i in [16usize, 17, 21, 46, 196, 231, 232, 244, 255] {
            assert_eq!(c.palette[i], f(theme.indexed(i as u8)), "{i}");
        }
        // xterm 표의 알려진 값: 16번은 검정, 231번은 흰색, 232번은 8
        assert_eq!(c.palette[16], [0.0, 0.0, 0.0]);
        assert_eq!(c.palette[231], [1.0, 1.0, 1.0]);
        assert_eq!(c.palette[232], [8.0 / 255.0; 3]);
    }

    #[test]
    fn colors_use_theme_and_osc_overrides_win() {
        let theme = Theme::ghostty_default();
        let plain = shader_colors(&theme, &ColorOverrides::default());
        let f = |c: Rgb| [c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0];
        assert_eq!(plain.background, f(theme.background));
        assert_eq!(plain.foreground, f(theme.foreground));
        assert_eq!(plain.cursor_color, f(theme.cursor));
        assert_eq!(plain.cursor_text, f(theme.cursor_text));
        assert_eq!(plain.selection_background, f(theme.selection_background));

        let ov = ColorOverrides {
            palette: vec![(1, Rgb::new(10, 20, 30))],
            foreground: Some(Rgb::new(1, 2, 3)),
            background: Some(Rgb::new(4, 5, 6)),
            cursor: Some(Rgb::new(7, 8, 9)),
        };
        let c = shader_colors(&theme, &ov);
        assert_eq!(c.palette[1], f(Rgb::new(10, 20, 30)));
        assert_eq!(c.palette[2], plain.palette[2]);
        assert_eq!(c.foreground, f(Rgb::new(1, 2, 3)));
        assert_eq!(c.background, f(Rgb::new(4, 5, 6)));
        assert_eq!(c.cursor_color, f(Rgb::new(7, 8, 9)));
    }

    #[test]
    fn selection_foreground_unset_is_zero_like_ghostty() {
        let mut theme = Theme::ghostty_default();
        theme.selection_foreground = None;
        assert_eq!(
            shader_colors(&theme, &ColorOverrides::default()).selection_foreground,
            [0.0; 3]
        );
        theme.selection_foreground = Some(Rgb::new(255, 255, 255));
        assert_eq!(
            shader_colors(&theme, &ColorOverrides::default()).selection_foreground,
            [1.0; 3]
        );
    }

    #[test]
    fn inputs_default_is_all_zero() {
        let d = ShaderInputs::default();
        assert_eq!(d.time, 0.0);
        assert_eq!(d.frame, 0);
        assert_eq!(d.focus, 0);
        assert!(d.palette.iter().all(|c| *c == [0.0; 3]));
        assert_eq!(d.current_cursor, [0.0; 4]);
    }

    #[test]
    fn inputs_assemble_from_the_parts() {
        let theme = Theme::ghostty_default();
        let colors = shader_colors(&theme, &ColorOverrides::default());
        let mut cur = CursorTracker::default();
        cur.set_state(2, true);
        cur.set_glyph(0.5, Some([1.0, 2.0, 3.0, 4.0]), [0.1, 0.2, 0.3, 1.0]);
        let i = ShaderInputs::assemble((0.5, 0.25, 7), &cur, (1, 0.125), &colors);
        assert_eq!((i.time, i.time_delta, i.frame), (0.5, 0.25, 7));
        assert_eq!(i.current_cursor, [1.0, 2.0, 3.0, 4.0]);
        assert_eq!(i.current_cursor_color, [0.1, 0.2, 0.3, 1.0]);
        assert_eq!(
            (i.current_cursor_style, i.cursor_visible, i.time_cursor_change),
            (2, 1, 0.5)
        );
        assert_eq!((i.focus, i.time_focus), (1, 0.125));
        assert_eq!(i.palette, colors.palette);
        assert_eq!(i.background, colors.background);
    }

    // ---- 애니메이션 ----

    #[test]
    fn animation_table() {
        use CustomShaderAnimation::*;
        // (셰이더 있음, 설정, 초점) -> 움직이나. Ghostty animationWake: 셰이더 없으면 null, false는 null,
        // always는 늘, true는 초점일 때만
        for (shaders, setting, focused, want) in [
            (false, False, true, false),
            (false, False, false, false),
            (false, True, true, false),
            (false, True, false, false),
            (false, Always, true, false),
            (false, Always, false, false),
            (true, False, true, false),
            (true, False, false, false),
            (true, True, true, true),
            (true, True, false, false),
            (true, Always, true, true),
            (true, Always, false, true),
        ] {
            assert_eq!(
                shader_animates(shaders, setting, focused),
                want,
                "shaders={shaders} {setting:?} focused={focused}"
            );
        }
    }

    #[test]
    fn draw_interval_is_ghosttys_8ms() {
        assert_eq!(DRAW_INTERVAL_MS, 8);
    }

    // ---- 파일 읽기 ----

    #[test]
    fn load_reads_files_in_order_with_path_names() {
        let d = tmp("order");
        std::fs::write(d.join("a.glsl"), "void mainImage(){}").unwrap();
        std::fs::write(d.join("b.glsl"), "// b").unwrap();
        let (files, warns) = load_shader_files(&[sp(d.join("b.glsl"), false), sp(d.join("a.glsl"), false)]);
        assert!(warns.is_empty(), "{warns:?}");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].0, d.join("b.glsl").display().to_string());
        assert_eq!(files[0].1, "// b");
        assert_eq!(files[1].1, "void mainImage(){}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn load_missing_file_warns_unless_optional() {
        let d = tmp("missing");
        std::fs::write(d.join("ok.glsl"), "x").unwrap();
        let (files, warns) = load_shader_files(&[
            sp(d.join("nope.glsl"), true),
            sp(d.join("nope2.glsl"), false),
            sp(d.join("ok.glsl"), false),
        ]);
        assert_eq!(files.len(), 1);
        assert_eq!(warns.len(), 1, "{warns:?}");
        assert!(warns[0].contains("nope2.glsl"), "{warns:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn load_refuses_files_over_one_mebibyte_but_takes_exactly_one() {
        let d = tmp("limit");
        std::fs::write(d.join("exact.glsl"), vec![b'a'; SHADER_FILE_LIMIT as usize]).unwrap();
        std::fs::write(d.join("over.glsl"), vec![b'a'; SHADER_FILE_LIMIT as usize + 1]).unwrap();
        let (files, warns) = load_shader_files(&[sp(d.join("exact.glsl"), false), sp(d.join("over.glsl"), false)]);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].1.len(), SHADER_FILE_LIMIT as usize);
        assert_eq!(warns.len(), 1, "{warns:?}");
        assert!(warns[0].contains("over.glsl"), "{warns:?}");
        // 상한을 넘으면 `?`여도 있는 파일이니 알린다
        let (files, warns) = load_shader_files(&[sp(d.join("over.glsl"), true)]);
        assert!(files.is_empty());
        assert_eq!(warns.len(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn load_rejects_non_utf8_and_directories() {
        let d = tmp("utf8");
        std::fs::write(d.join("bad.glsl"), [0xff, 0xfe, 0x00, 0x41]).unwrap();
        std::fs::create_dir_all(d.join("dir.glsl")).unwrap();
        let (files, warns) = load_shader_files(&[sp(d.join("bad.glsl"), false), sp(d.join("dir.glsl"), false)]);
        assert!(files.is_empty());
        assert_eq!(warns.len(), 2, "{warns:?}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn screen_cursor_shape_codes_follow_ghostty() {
        use bt_term::CursorShape as S;
        assert_eq!(
            [S::Block, S::HollowBlock, S::Beam, S::Underline].map(screen_style_code),
            [0, 1, 2, 3]
        );
    }

    fn colors() -> ShaderColors {
        shader_colors(&Theme::default(), &ColorOverrides::default())
    }

    fn cur(style: i32, glyph: Option<([f32; 4], [f32; 4])>) -> FrameCursor {
        FrameCursor {
            style,
            visible: true,
            glyph,
        }
    }

    #[test]
    fn state_frame_counts_time_and_frames_and_takes_colors() {
        let t0 = Instant::now();
        let mut s = ShaderState::default();
        let a = s.frame(t0, true, true, &cur(0, None), &colors());
        assert_eq!((a.time, a.time_delta, a.frame), (0.0, 0.0, 1));
        let b = s.frame(t0 + Duration::from_millis(500), false, true, &cur(0, None), &colors());
        assert!(
            (b.time - 0.5).abs() < 1e-6 && (b.time_delta - 0.5).abs() < 1e-6,
            "{b:?}"
        );
        assert_eq!(b.frame, 2);
        assert_eq!(b.background, colors().background);
        assert_eq!(b.palette[196], colors().palette[196]);
    }

    #[test]
    fn state_frame_moves_cursor_style_only_on_dirty_frames_like_ghostty() {
        let t0 = Instant::now();
        let mut s = ShaderState::default();
        s.frame(t0, true, true, &cur(0, None), &colors());
        // 모양이 막대로 바뀐 dirty 프레임: 앞 0, 지금 2
        let a = s.frame(t0, true, true, &cur(2, None), &colors());
        assert_eq!((a.previous_cursor_style, a.current_cursor_style), (0, 2));
        // 바뀌지 않은 프레임은 그대로 둔다
        let b = s.frame(t0, false, true, &cur(2, None), &colors());
        assert_eq!((b.previous_cursor_style, b.current_cursor_style), (0, 2));
        // 다음 dirty 프레임에서 앞 모양도 2가 된다(Ghostty는 dirty마다 민다)
        let c = s.frame(t0, true, true, &cur(2, None), &colors());
        assert_eq!((c.previous_cursor_style, c.current_cursor_style), (2, 2));
        // 보임은 dirty 프레임에서 옮긴다
        let hidden = FrameCursor {
            visible: false,
            ..cur(2, None)
        };
        assert_eq!(s.frame(t0, false, true, &hidden, &colors()).cursor_visible, 1);
        assert_eq!(s.frame(t0, true, true, &hidden, &colors()).cursor_visible, 0);
    }

    #[test]
    fn state_frame_stamps_cursor_change_and_focus_gain() {
        let t0 = Instant::now();
        let mut s = ShaderState::default();
        let r1 = ([10.0, 40.0, 8.0, 16.0], [1.0, 1.0, 1.0, 1.0]);
        let r2 = ([18.0, 40.0, 8.0, 16.0], [1.0, 1.0, 1.0, 1.0]);
        s.frame(t0, true, true, &cur(0, Some(r1)), &colors());
        let a = s.frame(t0 + Duration::from_secs(1), true, true, &cur(0, Some(r2)), &colors());
        assert_eq!((a.previous_cursor, a.current_cursor), (r1.0, r2.0));
        assert!((a.time_cursor_change - 1.0).abs() < 1e-6, "{a:?}");
        // 깜빡여 그려지지 않는 프레임은 자리를 바꾸지 않는다
        let b = s.frame(t0 + Duration::from_secs(2), false, true, &cur(0, None), &colors());
        assert_eq!((b.current_cursor, b.time_cursor_change), (r2.0, a.time_cursor_change));
        // 초점: 잃으면 0, 시각은 그대로. 다시 얻으면 그 프레임의 time
        let c = s.frame(t0 + Duration::from_secs(3), false, false, &cur(0, None), &colors());
        assert_eq!((c.focus, c.time_focus), (0, 0.0));
        let d = s.frame(t0 + Duration::from_secs(4), false, true, &cur(0, None), &colors());
        assert_eq!(d.focus, 1);
        assert!((d.time_focus - 4.0).abs() < 1e-6, "{d:?}");
    }

    #[test]
    fn to_uniforms_copies_every_field() {
        let mut i = ShaderInputs {
            time: 1.5,
            time_delta: 0.25,
            frame: 7,
            current_cursor: [1.0, 2.0, 3.0, 4.0],
            previous_cursor: [5.0, 6.0, 7.0, 8.0],
            current_cursor_color: [0.1, 0.2, 0.3, 0.4],
            previous_cursor_color: [0.5, 0.6, 0.7, 0.8],
            current_cursor_style: 2,
            previous_cursor_style: 3,
            cursor_visible: 1,
            time_cursor_change: 1.25,
            time_focus: 0.75,
            focus: 1,
            background: [0.01, 0.02, 0.03],
            foreground: [0.04, 0.05, 0.06],
            cursor_color: [0.07, 0.08, 0.09],
            cursor_text: [0.11, 0.12, 0.13],
            selection_background: [0.14, 0.15, 0.16],
            selection_foreground: [0.17, 0.18, 0.19],
            ..ShaderInputs::default()
        };
        i.palette[0] = [0.9, 0.8, 0.7];
        i.palette[255] = [0.3, 0.2, 0.1];
        let u = i.to_uniforms();
        let back = ShaderInputs {
            time: u.time,
            time_delta: u.time_delta,
            frame: u.frame,
            current_cursor: u.current_cursor,
            previous_cursor: u.previous_cursor,
            current_cursor_color: u.current_cursor_color,
            previous_cursor_color: u.previous_cursor_color,
            current_cursor_style: u.current_cursor_style,
            previous_cursor_style: u.previous_cursor_style,
            cursor_visible: u.cursor_visible,
            time_cursor_change: u.time_cursor_change,
            time_focus: u.time_focus,
            focus: u.focus,
            palette: u.palette,
            background: u.background,
            foreground: u.foreground,
            cursor_color: u.cursor_color,
            cursor_text: u.cursor_text,
            selection_background: u.selection_background,
            selection_foreground: u.selection_foreground,
        };
        assert_eq!(back, i);
    }

    #[test]
    fn wake_fires_now_then_every_8ms_and_stops_when_not_animating() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        assert_eq!(shader_wake(None, t0, false), (false, None));
        assert_eq!(
            shader_wake(Some(t0), t0, false),
            (false, None),
            "멈추면 깨우기를 지운다"
        );
        assert_eq!(shader_wake(None, t0, true), (true, Some(t0 + ms(8))), "처음이면 바로");
        assert_eq!(
            shader_wake(Some(t0 + ms(8)), t0 + ms(3), true),
            (false, Some(t0 + ms(8))),
            "때가 안 됐으면 기다린다"
        );
        assert_eq!(
            shader_wake(Some(t0 + ms(8)), t0 + ms(9), true),
            (true, Some(t0 + ms(17))),
            "때가 지났으면 지금부터 8ms"
        );
    }
}
