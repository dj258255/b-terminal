//! 사용자 셰이더(Ghostty `custom-shader`, ADR-040): shadertoy 꼴 GLSL을 naga로 읽어 검증하고, 유니폼 블록의 바이트를 만든다.
//!
//! 사용자 소스 앞에 `PRELUDE`를 붙인다. Ghostty의 `shadertoy_prefix.glsl`(ghostty-org/ghostty `src/renderer/shaders/`)과
//! 유니폼 블록의 필드 순서·이름·타입이 같다. naga의 GLSL 읽기가 받는 꼴로 바꾼 곳은 ADR-040에 적었다

use std::fmt::Write as _;

/// 사용자 셰이더 하나. `name`은 오류 문구와 로그에 쓴다(보통 파일 경로)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomShaderSource {
    pub name: String,
    pub glsl: String,
}

/// 셰이더 한 프레임의 유니폼 값. 해상도(`iResolution`, `iChannelResolution[0]`)는 렌더러가 채운다.
/// `iFrameRate`·`iChannelTime`·`iMouse`·`iDate`·`iSampleRate`는 Ghostty도 쓰지 않아 0으로 둔다
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShaderUniforms {
    /// `iTime`: 첫 프레임부터 초
    pub time: f32,
    /// `iTimeDelta`: 앞 프레임부터 초
    pub time_delta: f32,
    /// `iFrame`: 지금까지 그린 프레임 수
    pub frame: i32,
    /// `iCurrentCursor`: xy 모서리, zw 폭·높이(픽셀, Ghostty 규칙)
    pub current_cursor: [f32; 4],
    /// `iPreviousCursor`
    pub previous_cursor: [f32; 4],
    /// `iCurrentCursorColor` (0~1, a 포함)
    pub current_cursor_color: [f32; 4],
    /// `iPreviousCursorColor`
    pub previous_cursor_color: [f32; 4],
    /// `iCurrentCursorStyle`: 0 블록, 1 빈 블록, 2 막대, 3 밑줄, 4 자물쇠
    pub current_cursor_style: i32,
    /// `iPreviousCursorStyle`
    pub previous_cursor_style: i32,
    /// `iCursorVisible`: 1 보임, 0 숨김
    pub cursor_visible: i32,
    /// `iTimeCursorChange`: 커서가 자리·색을 바꾼 때의 `iTime`
    pub time_cursor_change: f32,
    /// `iTimeFocus`: 초점을 얻거나 잃은 때의 `iTime`(Ghostty 원문대로)
    pub time_focus: f32,
    /// `iFocus`: 1 초점, 0 아님
    pub focus: i32,
    /// `iPalette` (0~1)
    pub palette: [[f32; 3]; 256],
    /// `iBackgroundColor`
    pub background: [f32; 3],
    /// `iForegroundColor`
    pub foreground: [f32; 3],
    /// `iCursorColor`
    pub cursor_color: [f32; 3],
    /// `iCursorText`
    pub cursor_text: [f32; 3],
    /// `iSelectionBackgroundColor`
    pub selection_background: [f32; 3],
    /// `iSelectionForegroundColor`
    pub selection_foreground: [f32; 3],
}

impl Default for ShaderUniforms {
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

/// 유니폼 블록 `Globals`의 std140 바이트 수
pub const UNIFORM_BLOCK_SIZE: usize = 4496;

/// 블록 필드의 바이트 자리(std140). prelude의 선언 순서와 같고, 시험이 naga가 계산한 값과 견준다
pub const FIELD_OFFSETS: &[(&str, usize)] = &[
    ("iResolution", 0),
    ("iTime", 12),
    ("iTimeDelta", 16),
    ("iFrameRate", 20),
    ("iFrame", 24),
    ("_bt_iChannelTime", 32),
    ("iChannelResolution", 96),
    ("iMouse", 160),
    ("iDate", 176),
    ("iSampleRate", 192),
    ("iCurrentCursor", 208),
    ("iPreviousCursor", 224),
    ("iCurrentCursorColor", 240),
    ("iPreviousCursorColor", 256),
    ("iCurrentCursorStyle", 272),
    ("iPreviousCursorStyle", 276),
    ("iCursorVisible", 280),
    ("iTimeCursorChange", 284),
    ("iTimeFocus", 288),
    ("iFocus", 292),
    ("iPalette", 304),
    ("iBackgroundColor", 4400),
    ("iForegroundColor", 4416),
    ("iCursorColor", 4432),
    ("iCursorText", 4448),
    ("iSelectionForegroundColor", 4464),
    ("iSelectionBackgroundColor", 4480),
];

fn offset_of(name: &str) -> usize {
    FIELD_OFFSETS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, o)| *o)
        .unwrap_or_else(|| unreachable!("모르는 필드 {name}"))
}

fn put(buf: &mut [u8], at: usize, values: &[f32]) {
    for (i, v) in values.iter().enumerate() {
        buf[at + i * 4..at + i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
}

fn put_i(buf: &mut [u8], at: usize, v: i32) {
    buf[at..at + 4].copy_from_slice(&v.to_le_bytes());
}

impl ShaderUniforms {
    /// 유니폼 블록 한 개의 std140 바이트. `resolution`은 `iResolution`(`iChannelResolution[0]`도 같은 값, 넷째는 0)
    pub fn to_std140(&self, resolution: [f32; 3]) -> Vec<u8> {
        let mut b = vec![0u8; UNIFORM_BLOCK_SIZE];
        put(&mut b, offset_of("iResolution"), &resolution);
        put(&mut b, offset_of("iTime"), &[self.time]);
        put(&mut b, offset_of("iTimeDelta"), &[self.time_delta]);
        put_i(&mut b, offset_of("iFrame"), self.frame);
        // iChannelResolution[0]만 쓴다(배열 원소 stride 16)
        put(&mut b, offset_of("iChannelResolution"), &resolution);
        put(&mut b, offset_of("iCurrentCursor"), &self.current_cursor);
        put(&mut b, offset_of("iPreviousCursor"), &self.previous_cursor);
        put(&mut b, offset_of("iCurrentCursorColor"), &self.current_cursor_color);
        put(&mut b, offset_of("iPreviousCursorColor"), &self.previous_cursor_color);
        put_i(&mut b, offset_of("iCurrentCursorStyle"), self.current_cursor_style);
        put_i(&mut b, offset_of("iPreviousCursorStyle"), self.previous_cursor_style);
        put_i(&mut b, offset_of("iCursorVisible"), self.cursor_visible);
        put(&mut b, offset_of("iTimeCursorChange"), &[self.time_cursor_change]);
        put(&mut b, offset_of("iTimeFocus"), &[self.time_focus]);
        put_i(&mut b, offset_of("iFocus"), self.focus);
        let pal = offset_of("iPalette");
        for (i, c) in self.palette.iter().enumerate() {
            put(&mut b, pal + i * 16, c);
        }
        put(&mut b, offset_of("iBackgroundColor"), &self.background);
        put(&mut b, offset_of("iForegroundColor"), &self.foreground);
        put(&mut b, offset_of("iCursorColor"), &self.cursor_color);
        put(&mut b, offset_of("iCursorText"), &self.cursor_text);
        put(
            &mut b,
            offset_of("iSelectionForegroundColor"),
            &self.selection_foreground,
        );
        put(
            &mut b,
            offset_of("iSelectionBackgroundColor"),
            &self.selection_background,
        );
        b
    }
}

/// 사용자 소스 앞에 붙이는 글(naga GLSL 읽기용으로 바꾼 Ghostty `shadertoy_prefix.glsl`).
/// 바인딩: 0 = 텍스처, 1 = `Globals`, 2 = 샘플러(모두 set 0).
///
/// Ghostty 원문과 다른 곳(이유는 ADR-040): `#version 450`(naga는 430을 받지 않는다), 블록 안의 `uniform` 한정자를 뺌,
/// 합친 `sampler2D iChannel0`를 `texture2D`+`sampler` 둘과 `#define iChannel0 sampler2D(...)`로, `gl_FragCoord` 다시 선언을 뺌
/// (내장 값을 그대로 쓴다), `set = 0`을 적음, `main`을 사용자 소스 뒤(`EPILOGUE`)로 옮김,
/// `float iChannelTime[4]`는 같은 바이트의 `vec4 _bt_iChannelTime[4]`와 0으로 채운 `const float iChannelTime[4]`로 나눔
/// (Metal 쪽 naga가 std140의 float 배열 stride 16을 지키지 않아 뒤의 필드가 모두 밀린다)
pub const PRELUDE: &str = "#version 450 core
layout(set = 0, binding = 1, std140) uniform Globals {
    vec3  iResolution;
    float iTime;
    float iTimeDelta;
    float iFrameRate;
    int   iFrame;
    vec4  _bt_iChannelTime[4];
    vec3  iChannelResolution[4];
    vec4  iMouse;
    vec4  iDate;
    float iSampleRate;
    vec4  iCurrentCursor;
    vec4  iPreviousCursor;
    vec4  iCurrentCursorColor;
    vec4  iPreviousCursorColor;
    int   iCurrentCursorStyle;
    int   iPreviousCursorStyle;
    int   iCursorVisible;
    float iTimeCursorChange;
    float iTimeFocus;
    int   iFocus;
    vec3  iPalette[256];
    vec3  iBackgroundColor;
    vec3  iForegroundColor;
    vec3  iCursorColor;
    vec3  iCursorText;
    vec3  iSelectionForegroundColor;
    vec3  iSelectionBackgroundColor;
};
const float iChannelTime[4] = float[4](0.0, 0.0, 0.0, 0.0);
#define CURSORSTYLE_BLOCK        0
#define CURSORSTYLE_BLOCK_HOLLOW 1
#define CURSORSTYLE_BAR          2
#define CURSORSTYLE_UNDERLINE    3
#define CURSORSTYLE_LOCK         4
layout(set = 0, binding = 0) uniform texture2D _bt_channel0_tex;
layout(set = 0, binding = 2) uniform sampler _bt_channel0_sampler;
#define iChannel0 sampler2D(_bt_channel0_tex, _bt_channel0_sampler)
layout(location = 0) out vec4 _fragColor;
#define texture2D texture
";

/// 사용자 소스 뒤에 붙이는 글. `main`을 사용자 소스 뒤에 둔다: naga는 정의 전에 부르는 함수(앞선 선언만 있는 `mainImage`)를
/// 검증에서 받지 않는다(함수는 부르는 쪽보다 앞에 있어야 한다)
pub const EPILOGUE: &str = "
void main() { mainImage(_fragColor, gl_FragCoord.xy); }
";

/// `PRELUDE`의 줄 수. 오류의 줄 번호에서 빼서 사용자 소스의 줄로 돌린다
pub fn prelude_lines() -> usize {
    PRELUDE.lines().count()
}

/// prelude와 사용자 소스를 이은 전체 글
pub fn full_source(user: &str) -> String {
    let mut s = String::with_capacity(PRELUDE.len() + user.len() + EPILOGUE.len() + 1);
    s.push_str(PRELUDE);
    s.push_str(user);
    s.push_str(EPILOGUE);
    s
}

/// prelude + 사용자 소스를 fragment로 읽고 검증한다. 오류 문구에는 `name`과 naga의 메시지(사용자 소스의 줄 번호)를 넣는다.
/// 잘못된 셰이더가 wgpu의 검증 패닉에 닿지 않게 장치에 넘기기 전에 여기서 막는다. 통과한 naga 모듈을 돌려준다
pub fn check(name: &str, user: &str) -> Result<wgpu::naga::Module, String> {
    let module = parse(&full_source(user)).map_err(|e| format!("{name}: {e}"))?;
    validate(&module).map_err(|e| format!("{name}: {e}"))?;
    Ok(module)
}

pub(crate) fn parse(source: &str) -> Result<wgpu::naga::Module, String> {
    use wgpu::naga::front::glsl;
    let options = glsl::Options::from(wgpu::naga::ShaderStage::Fragment);
    glsl::Frontend::default()
        .parse(&options, source)
        .map_err(|e| describe_parse_error(&e, source))
}

fn describe_parse_error(e: &wgpu::naga::front::glsl::ParseErrors, source: &str) -> String {
    let skip = prelude_lines();
    let mut out = String::new();
    for err in &e.errors {
        if !out.is_empty() {
            out.push_str("; ");
        }
        let _ = match err.meta.location(source) {
            loc if loc.line_number as usize > skip => {
                write!(out, "{}줄: {}", loc.line_number as usize - skip, err.kind)
            }
            // 줄 번호가 없거나 prelude 안이면 문구만
            _ => write!(out, "{}", err.kind),
        };
    }
    if out.is_empty() {
        out = "GLSL을 읽지 못했다".to_string();
    }
    out
}

pub(crate) fn validate(module: &wgpu::naga::Module) -> Result<(), String> {
    use wgpu::naga::valid::{Capabilities, ValidationFlags, Validator};
    Validator::new(ValidationFlags::all(), Capabilities::empty())
        .validate(module)
        .map(|_| ())
        .map_err(|e| format!("검증 오류: {}", e.as_inner()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::naga::{Handle, Module, TypeInner};

    const TRIVIAL: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) { fragColor = vec4(0.0); }";

    fn globals_struct(m: &Module) -> (Vec<(String, u32)>, u32) {
        let (_, var) = m
            .global_variables
            .iter()
            .find(|(_, v)| v.name.as_deref() == Some("Globals") || matches!(v.space, wgpu::naga::AddressSpace::Uniform))
            .expect("유니폼 블록");
        let ty: Handle<_> = var.ty;
        match &m.types[ty].inner {
            TypeInner::Struct { members, span } => (
                members
                    .iter()
                    .map(|mem| (mem.name.clone().unwrap_or_default(), mem.offset))
                    .collect(),
                *span,
            ),
            other => panic!("구조체가 아니다: {other:?}"),
        }
    }

    #[test]
    fn std140_offsets_match_what_naga_computes_for_the_prelude_block() {
        let module = parse(&full_source(TRIVIAL)).expect("prelude를 읽는다");
        let (members, span) = globals_struct(&module);
        let want: Vec<(String, u32)> = FIELD_OFFSETS.iter().map(|(n, o)| (n.to_string(), *o as u32)).collect();
        assert_eq!(members, want);
        // 블록 크기는 16의 배수로 올린다(std140)
        assert_eq!(span.next_multiple_of(16) as usize, UNIFORM_BLOCK_SIZE);
    }

    #[test]
    fn to_std140_puts_every_field_at_its_offset() {
        let mut u = ShaderUniforms {
            time: 1.5,
            time_delta: 2.5,
            frame: 7,
            current_cursor: [1.0, 2.0, 3.0, 4.0],
            previous_cursor: [5.0, 6.0, 7.0, 8.0],
            current_cursor_color: [0.1, 0.2, 0.3, 0.4],
            previous_cursor_color: [0.5, 0.6, 0.7, 0.8],
            current_cursor_style: 3,
            previous_cursor_style: 2,
            cursor_visible: 1,
            time_cursor_change: 9.0,
            time_focus: 10.0,
            focus: 1,
            background: [11.0, 12.0, 13.0],
            foreground: [14.0, 15.0, 16.0],
            cursor_color: [17.0, 18.0, 19.0],
            cursor_text: [20.0, 21.0, 22.0],
            selection_background: [23.0, 24.0, 25.0],
            selection_foreground: [26.0, 27.0, 28.0],
            ..ShaderUniforms::default()
        };
        u.palette[0] = [30.0, 31.0, 32.0];
        u.palette[255] = [33.0, 34.0, 35.0];
        let b = u.to_std140([640.0, 480.0, 1.0]);
        assert_eq!(b.len(), UNIFORM_BLOCK_SIZE);
        let f = |name: &str, i: usize| f32::from_le_bytes(b[offset_of(name) + i * 4..][..4].try_into().unwrap());
        let n = |name: &str| i32::from_le_bytes(b[offset_of(name)..][..4].try_into().unwrap());
        assert_eq!(
            [f("iResolution", 0), f("iResolution", 1), f("iResolution", 2)],
            [640.0, 480.0, 1.0]
        );
        assert_eq!((f("iTime", 0), f("iTimeDelta", 0), n("iFrame")), (1.5, 2.5, 7));
        assert_eq!(
            [
                f("iChannelResolution", 0),
                f("iChannelResolution", 1),
                f("iChannelResolution", 2)
            ],
            [640.0, 480.0, 1.0]
        );
        assert_eq!(
            f("iChannelResolution", 3),
            0.0,
            "iChannelResolution[0]의 뒤 자리와 [1]은 0"
        );
        assert_eq!(f("iCurrentCursor", 3), 4.0);
        assert_eq!(f("iPreviousCursor", 0), 5.0);
        assert_eq!(f("iCurrentCursorColor", 3), 0.4);
        assert_eq!(f("iPreviousCursorColor", 1), 0.6);
        assert_eq!(
            (n("iCurrentCursorStyle"), n("iPreviousCursorStyle"), n("iCursorVisible")),
            (3, 2, 1)
        );
        assert_eq!(
            (f("iTimeCursorChange", 0), f("iTimeFocus", 0), n("iFocus")),
            (9.0, 10.0, 1)
        );
        let pal = |i: usize, c: usize| {
            f32::from_le_bytes(b[offset_of("iPalette") + i * 16 + c * 4..][..4].try_into().unwrap())
        };
        assert_eq!(
            [pal(0, 0), pal(0, 2), pal(255, 0), pal(255, 2)],
            [30.0, 32.0, 33.0, 35.0]
        );
        assert_eq!(f("iBackgroundColor", 1), 12.0);
        assert_eq!(f("iForegroundColor", 2), 16.0);
        assert_eq!(f("iCursorColor", 0), 17.0);
        assert_eq!(f("iCursorText", 1), 21.0);
        // 선택 색은 prelude 순서: 앞이 foreground, 뒤가 background
        assert_eq!(f("iSelectionForegroundColor", 0), 26.0);
        assert_eq!(f("iSelectionBackgroundColor", 2), 25.0);
        // 쓰지 않는 자리는 0
        assert_eq!(f("iFrameRate", 0), 0.0);
        assert_eq!(f("iMouse", 0), 0.0);
    }

    #[test]
    fn ghostty_test_shaders_pass_naga_and_the_invalid_one_does_not() {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/shaders/");
        let read = |n: &str| std::fs::read_to_string(format!("{dir}{n}")).expect("시험 셰이더");
        assert!(check("crt", &read("test_shadertoy_crt.glsl")).is_ok());
        assert!(check("focus", &read("test_shadertoy_focus.glsl")).is_ok());
        let err = check("invalid", &read("test_shadertoy_invalid.glsl")).expect_err("오류");
        assert!(err.starts_with("invalid: ") && err.contains("mainImage"), "{err}");
    }
}
