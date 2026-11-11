//! 칸마다 사용자 셰이더(#441): 칸마다 그 칸 영역을 칸 크기 텍스처로 떼어 셰이더를 돌리고 제자리에 그린다.
//! 창 없이 합친 프레임을 읽는다. GPU 어댑터가 없으면 알리고 끝낸다

use bt_core::Rgb;
use bt_gpu::{CustomShaderSource, GpuRenderer, LayerUpdate, PaneDraw, PixelRect, ShaderRegion, ShaderUniforms};
use bt_render::{DrawCell, FontConfig, Fonts, GridView, Underline};

const W: u32 = 300;
const H: u32 = 200;
const COLS: usize = 12;
const ROWS: usize = 2;
const BG: Rgb = Rgb { r: 30, g: 32, b: 40 };
const FG: Rgb = Rgb { r: 220, g: 220, b: 210 };

/// 칸 A: 탭 막대(위 30픽셀)와 칸 사이 틈(10픽셀)을 비운 두 칸. 크기가 서로 다르다
const A: PixelRect = PixelRect {
    x: 0,
    y: 30,
    w: 150,
    h: 170,
};
const B: PixelRect = PixelRect {
    x: 160,
    y: 30,
    w: 140,
    h: 120,
};

const IDENTITY: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    fragColor = texture(iChannel0, fragCoord / iResolution.xy);
}";
const INVERT: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec4 c = texture(iChannel0, fragCoord / iResolution.xy);
    fragColor = vec4(1.0 - c.rgb, 1.0);
}";

fn gpu() -> Option<GpuRenderer> {
    let g = GpuRenderer::new_headless();
    if g.is_none() {
        eprintln!("건너뜀: GPU 어댑터가 없다");
    }
    g
}

fn cell(ch: char) -> DrawCell {
    DrawCell {
        ch,
        fg: FG,
        bg: BG,
        bold: false,
        italic: false,
        underline: Underline::None,
        strike: false,
        wide: false,
        spacer: false,
        selected: false,
        emoji: false,
        cluster: None,
        cluster_cells: 0,
        overline: false,
        underline_color: None,
    }
}

/// 장면: 비대칭 그라데이션 아래층 위에 칸 A 안에 글자 격자 하나
struct Scene {
    fonts: Fonts,
    cells: Vec<DrawCell>,
    lower: Vec<u32>,
}

impl Scene {
    fn new() -> Self {
        let fonts = Fonts::new(&FontConfig {
            scale: 2.0,
            ..FontConfig::default()
        })
        .expect("넣어 둔 글꼴은 늘 있다");
        let mut cells = Vec::new();
        for line in ["Hello pane ga", "gjpqy 0123456"] {
            let mut row: Vec<DrawCell> = line.chars().map(cell).collect();
            row.resize(COLS, cell(' '));
            cells.extend(row);
        }
        let lower = (0..W * H)
            .map(|i| {
                let (x, y) = (i % W, i / W);
                let (r, g, b) = (x * 255 / W, y * 255 / H, 60 + (x + y) % 90);
                0xff00_0000 | r << 16 | g << 8 | b
            })
            .collect();
        Self { fonts, cells, lower }
    }

    fn compose(&mut self, g: &mut GpuRenderer, upper: Option<&[u32]>, top: Option<&[u32]>) -> Vec<u32> {
        let view = GridView {
            cols: COLS,
            rows: ROWS,
            cells: &self.cells,
            cursor: None,
            selection_bg: Rgb::new(60, 60, 120),
            selection_fg: None,
            preedit: None,
            min_contrast: 1.0,
        };
        let panes = [PaneDraw {
            font: 0,
            x: 6,
            y: 36,
            view: &view,
            clear: BG,
        }];
        g.compose_layers(
            &mut [&mut self.fonts],
            (W, H),
            LayerUpdate {
                lower: Some(&self.lower),
                upper,
            },
            top,
            &panes,
        )
    }
}

fn src(name: &str, glsl: &str) -> CustomShaderSource {
    CustomShaderSource {
        name: name.to_string(),
        glsl: glsl.to_string(),
    }
}

fn set(g: &mut GpuRenderer, name: &str, glsl: &str) {
    let errors = g.set_custom_shaders(&[src(name, glsl)]);
    assert!(errors.is_empty(), "{errors:?}");
    let errors = g.wait_custom_shaders();
    assert!(errors.is_empty(), "{errors:?}");
    assert!(g.has_custom_shaders());
}

fn region(key: u64, rect: PixelRect, uniforms: ShaderUniforms) -> ShaderRegion {
    ShaderRegion {
        key,
        rect: Some(rect),
        uniforms,
    }
}

fn channels(p: u32) -> [i32; 3] {
    [(p >> 16 & 0xff) as i32, (p >> 8 & 0xff) as i32, (p & 0xff) as i32]
}

fn at(frame: &[u32], x: u32, y: u32) -> u32 {
    frame[(y * W + x) as usize]
}

fn inside(r: PixelRect, x: u32, y: u32) -> bool {
    x >= r.x as u32 && x < r.x as u32 + r.w && y >= r.y as u32 && y < r.y as u32 + r.h
}

fn near(got: [i32; 3], want: [i32; 3], what: &str) {
    for i in 0..3 {
        assert!((got[i] - want[i]).abs() <= 1, "{what}: 채널 {i}: {got:?} != {want:?}");
    }
}

#[test]
fn identity_per_pane_matches_the_plain_frame_exactly() {
    // 칸 영역을 떼어 제자리에 그리는 길이 한 픽셀도 옮기지 않는다: 비대칭 그림이라 어긋나면 바로 드러난다
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new();
    let want = s.compose(&mut g, None, None);
    set(&mut g, "identity", IDENTITY);
    g.set_shader_regions(&[
        region(1, A, ShaderUniforms::default()),
        region(2, B, ShaderUniforms::default()),
    ]);
    let got = s.compose(&mut g, None, None);
    assert!(want == got, "항등 셰이더를 칸마다 돌렸는데 픽셀이 다르다");
}

#[test]
fn a_shader_runs_only_inside_its_pane_rect() {
    // 칸 A만 뒤집는다: A 안은 뒤집히고, B와 탭 막대·칸 사이 틈은 셰이더 없는 화면 그대로다
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new();
    let want = s.compose(&mut g, None, None);
    set(&mut g, "invert", INVERT);
    g.set_shader_regions(&[region(1, A, ShaderUniforms::default())]);
    let got = s.compose(&mut g, None, None);
    let (mut flipped, mut kept) = (0, 0);
    for y in 0..H {
        for x in 0..W {
            let (w, p) = (channels(at(&want, x, y)), channels(at(&got, x, y)));
            if inside(A, x, y) {
                for c in 0..3 {
                    assert!(
                        (255 - w[c] - p[c]).abs() <= 1,
                        "칸 A 안 ({x},{y}) 채널 {c}: {w:?} {p:?}"
                    );
                }
                flipped += 1;
            } else {
                assert_eq!(at(&want, x, y), at(&got, x, y), "칸 밖 ({x},{y})가 바뀌었다");
                kept += 1;
            }
        }
    }
    assert_eq!(flipped, (A.w * A.h) as usize);
    assert!(kept > 5000, "칸 밖 픽셀 {kept}");
}

#[test]
fn resolution_is_each_panes_own_size() {
    // iResolution이 칸 크기라 칸마다 다른 값이 나온다(창 크기 300x200이 아니다)
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new();
    set(
        &mut g,
        "res",
        "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
            fragColor = vec4(iResolution.x / 255.0, iResolution.y / 255.0, 0.0, 1.0);
        }",
    );
    g.set_shader_regions(&[
        region(1, A, ShaderUniforms::default()),
        region(2, B, ShaderUniforms::default()),
    ]);
    let got = s.compose(&mut g, None, None);
    near(
        channels(at(&got, 75, 100)),
        [A.w as i32, A.h as i32, 0],
        "칸 A의 해상도",
    );
    near(
        channels(at(&got, 230, 90)),
        [B.w as i32, B.h as i32, 0],
        "칸 B의 해상도",
    );
}

#[test]
fn frag_coord_and_channel0_are_pane_local() {
    // fragCoord는 칸 왼쪽 위가 (0, 0)이고 iChannel0은 그 칸의 그림이다: 칸 B의 왼쪽 위 모서리 픽셀이 칸 B의 첫 픽셀을 읽는다
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new();
    let want = s.compose(&mut g, None, None);
    set(
        &mut g,
        "local",
        "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
            // 칸 안 위치를 그대로 읽어 R에 x, G에 y를 쓴다(칸 크기로 나눠 0~1)
            vec4 c = texture(iChannel0, fragCoord / iResolution.xy);
            fragColor = vec4(fragCoord.x / iResolution.x, fragCoord.y / iResolution.y, c.b, 1.0);
        }",
    );
    g.set_shader_regions(&[region(2, B, ShaderUniforms::default())]);
    let got = s.compose(&mut g, None, None);
    let (px, py) = (B.x as u32 + 70, B.y as u32 + 60);
    let p = channels(at(&got, px, py));
    near(
        [p[0], p[1], 0],
        [
            (70.5 / B.w as f32 * 255.0) as i32,
            (60.5 / B.h as f32 * 255.0) as i32,
            0,
        ],
        "칸 안 위치",
    );
    // 파랑은 칸 B 자리의 원래 그림의 파랑(떼어 온 자리가 맞다)
    assert!(
        (p[2] - channels(at(&want, px, py))[2]).abs() <= 1,
        "iChannel0이 칸 B의 그림이 아니다"
    );
}

#[test]
fn uniforms_are_per_pane() {
    // 칸마다 다른 초점·시간·커서 값이 그 칸에만 닿는다
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new();
    set(
        &mut g,
        "uni",
        "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
            fragColor = vec4(float(iFocus), iTime / 10.0, iCurrentCursor.x / 255.0, 1.0);
        }",
    );
    let a = ShaderUniforms {
        focus: 1,
        time: 2.0,
        current_cursor: [10.0, 0.0, 0.0, 0.0],
        ..ShaderUniforms::default()
    };
    let b = ShaderUniforms {
        focus: 0,
        time: 7.0,
        current_cursor: [200.0, 0.0, 0.0, 0.0],
        ..ShaderUniforms::default()
    };
    g.set_shader_regions(&[region(1, A, a), region(2, B, b)]);
    let got = s.compose(&mut g, None, None);
    near(channels(at(&got, 75, 100)), [255, 51, 10], "칸 A의 초점·시간·커서");
    near(channels(at(&got, 230, 90)), [0, 178, 200], "칸 B의 초점·시간·커서");
}

#[test]
fn one_pane_covering_the_window_is_the_same_as_the_whole_window_path() {
    // 칸 하나가 창 전체이면 떼지 않고 지금 길 그대로다: 화면도 메모리도 같다
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new();
    let u = ShaderUniforms {
        focus: 1,
        time: 3.0,
        ..ShaderUniforms::default()
    };
    let probe = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
        vec4 c = texture(iChannel0, fragCoord / iResolution.xy);
        fragColor = vec4(1.0 - c.rgb, 1.0) * 0.5 + vec4(iTime / 20.0, float(iFocus) * 0.1, iResolution.x / 1000.0, 0.0);
    }";
    set(&mut g, "probe", probe);
    g.set_shader_uniforms(&u);
    let old = s.compose(&mut g, None, None);
    let old_mem = g.gpu_memory().custom_textures;
    g.set_shader_regions(&[ShaderRegion {
        key: 9,
        rect: Some(PixelRect { x: 0, y: 0, w: W, h: H }),
        uniforms: u,
    }]);
    let new = s.compose(&mut g, None, None);
    assert!(old == new, "칸 하나가 창 전체인데 화면이 지금과 다르다");
    assert_eq!(
        g.gpu_memory().custom_textures,
        old_mem,
        "칸 하나는 지금과 같은 메모리여야 한다"
    );
    // 셰이더가 하나면 창 크기 한 장(앞)뿐이다: 뒤 텍스처 없이 마지막 패스가 대상에 곧바로 그린다
    assert_eq!(old_mem, (W * H * 4) as usize);
    // rect가 없는 칸도 창 전체다
    g.set_shader_regions(&[ShaderRegion {
        key: 3,
        rect: None,
        uniforms: u,
    }]);
    assert!(old == s.compose(&mut g, None, None));
}

#[test]
fn without_shaders_the_screen_does_not_depend_on_the_regions() {
    // 셰이더가 없으면 칸 목록을 줘도 화면은 같다
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new();
    let want = s.compose(&mut g, None, None);
    g.set_shader_regions(&[
        region(1, A, ShaderUniforms::default()),
        region(2, B, ShaderUniforms::default()),
    ]);
    assert!(!g.has_custom_shaders());
    assert!(want == s.compose(&mut g, None, None));
    // 셰이더를 걸었다 끄면 원래대로 돌아온다
    set(&mut g, "invert", INVERT);
    assert!(want != s.compose(&mut g, None, None));
    g.set_custom_shaders(&[]);
    assert!(want == s.compose(&mut g, None, None));
    assert_eq!(g.gpu_memory().custom_textures, 0);
}

fn overlay_rect(x0: u32, y0: u32, x1: u32, y1: u32, argb: u32) -> Vec<u32> {
    let mut v = vec![0u32; (W * H) as usize];
    for y in y0..y1 {
        for x in x0..x1 {
            v[(y * W + x) as usize] = argb;
        }
    }
    v
}

#[test]
fn top_layer_stays_outside_the_shader_and_upper_layer_is_inside() {
    // 위층(칸 그림에 속하는 kitty 그림 같은 것)은 셰이더가 덮고, 맨 위층(팔레트·알림 띠 같은 덧그림)은 셰이더 밖이다
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new();
    let upper = overlay_rect(20, 60, 60, 90, 0xff00_ff00); // 칸 A 안, 초록
    let top = overlay_rect(70, 60, 110, 90, 0xffff_0000); // 칸 A 안, 빨강
    set(&mut g, "invert", INVERT);
    g.set_shader_regions(&[region(1, A, ShaderUniforms::default())]);
    let got = s.compose(&mut g, Some(&upper), Some(&top));
    near(
        channels(at(&got, 40, 75)),
        [255, 0, 255],
        "위층의 초록은 셰이더가 뒤집는다",
    );
    near(
        channels(at(&got, 90, 75)),
        [255, 0, 0],
        "맨 위층의 빨강은 셰이더 밖이라 그대로다",
    );
}

#[test]
fn top_layer_without_shaders_lands_like_the_upper_layer() {
    // 셰이더가 없으면 맨 위층은 위층 바로 뒤에 같은 방식으로 얹힌다(층을 나눠도 화면이 같다)
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new();
    let px = overlay_rect(20, 60, 60, 90, 0x80_80_00_00);
    let as_upper = s.compose(&mut g, Some(&px), None);
    let mut g2 = GpuRenderer::new_headless().expect("앞에서 어댑터를 얻었다");
    let as_top = s.compose(&mut g2, None, Some(&px));
    assert!(as_upper == as_top);
    // 맨 위층을 빈 조각으로 주면 놓는다
    let cleared = s.compose(&mut g2, None, Some(&[]));
    let plain = s.compose(&mut g, Some(&[]), None);
    assert!(cleared == plain);
}

#[test]
fn pane_textures_follow_the_regions_and_are_released_when_a_pane_goes_away() {
    // 칸 크기 텍스처는 칸 크기가 바뀔 때만 다시 만들고, 칸이 사라지면 놓는다
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new();
    set(&mut g, "identity", IDENTITY);
    let u = ShaderUniforms::default();
    // 셰이더가 하나라 칸 크기 한 장씩(뒤 텍스처 없음)
    let px = |r: PixelRect| (r.w * r.h * 4) as usize;
    let scene = (W * H * 4) as usize;
    g.set_shader_regions(&[region(1, A, u), region(2, B, u)]);
    s.compose(&mut g, None, None);
    assert_eq!(g.custom_region_keys(), vec![1, 2]);
    assert_eq!(g.gpu_memory().custom_textures, px(A) + px(B) + scene);
    // 칸 B가 닫혔다
    g.set_shader_regions(&[region(1, A, u)]);
    s.compose(&mut g, None, None);
    assert_eq!(g.custom_region_keys(), vec![1]);
    assert_eq!(g.gpu_memory().custom_textures, px(A) + scene);
    // 칸 A의 크기가 바뀌었다: 그 크기로 다시 만든다
    let a2 = PixelRect { w: 100, ..A };
    g.set_shader_regions(&[region(1, a2, u)]);
    s.compose(&mut g, None, None);
    assert_eq!(g.gpu_memory().custom_textures, px(a2) + scene);
    // 칸이 창 전체가 되면 합친 화면 텍스처도 놓는다
    g.set_shader_regions(&[]);
    s.compose(&mut g, None, None);
    assert_eq!(g.custom_region_keys(), vec![0]);
    assert_eq!(g.gpu_memory().custom_textures, (W * H * 4) as usize);
    // 셰이더를 끄면 모두 놓는다
    g.set_custom_shaders(&[]);
    assert_eq!(g.gpu_memory().custom_textures, 0);
    assert_eq!(g.gpu_memory().uniform_buffers, 16);
    assert_eq!(g.gpu_memory().custom_texture_count, 0);
}

// ---- #445: 칸마다 셰이더의 GPU 텍스처 줄이기 ----
// 아래 시험은 두 가지를 고정한다. (1) 칸 하나(창 전체·top inset)·둘·셋과 셰이더 하나·둘의 화면이 텍스처 줄이기 전과 한 픽셀도 다르지
// 않다(줄이기 전 코드에서 낸 해시). (2) 그 장면들이 잡는 텍스처의 장 수와 바이트

/// 칸 안 위치·초점·칸 크기·iChannel0을 비스듬히 읽는 셰이더: 칸 원점이 어긋나거나 y가 뒤집히면 해시가 바뀐다
const PROBE: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec2 uv = fragCoord / iResolution.xy;
    vec4 c = texture(iChannel0, uv + vec2(0.03, -0.02) * sin(uv.y * 20.0));
    fragColor = vec4(c.rgb * 0.7 + 0.3 * vec3(uv, 0.5 * float(iFocus)), 1.0);
}";

/// `gl_FragCoord`를 직접 쓰는 셰이더(`mainImage`의 `fragCoord`가 아니라)
const DIRECT_FRAG_COORD: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec4 c = texture(iChannel0, gl_FragCoord.xy / iResolution.xy);
    fragColor = vec4(gl_FragCoord.x / iResolution.x, gl_FragCoord.y / iResolution.y, c.b, 1.0);
}";

fn fnv(frame: &[u32]) -> u64 {
    frame.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, p| {
        p.to_le_bytes()
            .iter()
            .fold(h, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3))
    })
}

fn set_chain(g: &mut GpuRenderer, shaders: &[(&str, &str)]) {
    let list: Vec<_> = shaders.iter().map(|(n, s)| src(n, s)).collect();
    let errors = g.set_custom_shaders(&list);
    assert!(errors.is_empty(), "{errors:?}");
    let errors = g.wait_custom_shaders();
    assert!(errors.is_empty(), "{errors:?}");
}

const INSET: PixelRect = PixelRect {
    x: 0,
    y: 30,
    w: W,
    h: H - 30,
};
const WHOLE: PixelRect = PixelRect { x: 0, y: 0, w: W, h: H };
/// 같은 크기 둘과 크기가 다른 하나
const EQ1: PixelRect = PixelRect {
    x: 0,
    y: 30,
    w: 140,
    h: 80,
};
const EQ2: PixelRect = PixelRect {
    x: 150,
    y: 30,
    w: 140,
    h: 80,
};
const WIDE: PixelRect = PixelRect {
    x: 5,
    y: 120,
    w: 290,
    h: 75,
};

fn uni(i: u32) -> ShaderUniforms {
    ShaderUniforms {
        focus: (i % 2) as i32,
        time: i as f32,
        ..ShaderUniforms::default()
    }
}

struct Case {
    name: &'static str,
    shaders: Vec<(&'static str, &'static str)>,
    panes: Vec<PixelRect>,
    /// 줄이기 전 코드의 화면 해시
    hash: u64,
    /// 줄인 뒤 텍스처 장 수와 바이트
    textures: usize,
    bytes: usize,
}

fn px(r: PixelRect) -> usize {
    (r.w * r.h * 4) as usize
}

fn cases() -> Vec<Case> {
    let win = (W * H * 4) as usize;
    vec![
        Case {
            name: "창 전체 칸 하나, 셰이더 하나",
            shaders: vec![("probe", PROBE)],
            panes: vec![WHOLE],
            hash: 0xcc5f693615dbe452,
            textures: 1,
            bytes: win,
        },
        Case {
            name: "top inset 칸 하나, 셰이더 하나",
            shaders: vec![("probe", PROBE)],
            panes: vec![INSET],
            hash: 0x7d6c2f6f10c91c3b,
            textures: 2,
            bytes: win + px(INSET),
        },
        Case {
            name: "top inset 칸 하나, 셰이더 둘",
            shaders: vec![("invert", INVERT), ("probe", PROBE)],
            panes: vec![INSET],
            hash: 0x7059ecf8dcf850ce,
            textures: 3,
            bytes: win + 2 * px(INSET),
        },
        Case {
            name: "칸 셋(둘은 같은 크기), 셰이더 하나",
            shaders: vec![("probe", PROBE)],
            panes: vec![EQ1, EQ2, WIDE],
            hash: 0x3cfc65b7c2374628,
            textures: 3,
            bytes: win + px(EQ1) + px(WIDE),
        },
        Case {
            name: "칸 셋(둘은 같은 크기), 셰이더 둘",
            shaders: vec![("invert", INVERT), ("probe", PROBE)],
            panes: vec![EQ1, EQ2, WIDE],
            hash: 0x9d2323812c3c47b3,
            textures: 5,
            bytes: win + 2 * px(EQ1) + 2 * px(WIDE),
        },
        Case {
            name: "칸 둘(크기가 다름), 셰이더 하나, gl_FragCoord 직접",
            shaders: vec![("direct", DIRECT_FRAG_COORD)],
            panes: vec![A, B],
            hash: 0xb0bdcd62f5e8ac9c,
            textures: 3,
            bytes: win + px(A) + px(B),
        },
    ]
}

#[test]
fn frames_stay_the_same_and_textures_shrink() {
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new();
    let mut report = String::new();
    let mut bad = Vec::new();
    for c in cases() {
        set_chain(&mut g, &c.shaders);
        let regions: Vec<_> = c
            .panes
            .iter()
            .enumerate()
            .map(|(i, r)| region(i as u64 + 1, *r, uni(i as u32)))
            .collect();
        g.set_shader_regions(&regions);
        let h = fnv(&s.compose(&mut g, None, None));
        let m = g.gpu_memory();
        report += &format!(
            "{}: 해시 {h:#x} 텍스처 {}장 {}바이트\n",
            c.name, m.custom_texture_count, m.custom_textures
        );
        if h != c.hash {
            bad.push(format!("{}: 화면 해시 {h:#x} != {:#x}", c.name, c.hash));
        }
        if (m.custom_texture_count, m.custom_textures) != (c.textures, c.bytes) {
            bad.push(format!(
                "{}: 텍스처 {}장 {}바이트 != {}장 {}바이트",
                c.name, m.custom_texture_count, m.custom_textures, c.textures, c.bytes
            ));
        }
    }
    eprintln!("{report}");
    assert!(bad.is_empty(), "{}\n{report}", bad.join("\n"));
}

#[test]
fn direct_gl_frag_coord_is_pane_local_in_every_pass() {
    // 사용자 소스가 `gl_FragCoord`를 직접 써도 `fragCoord`와 같은 칸 안 좌표를 받는다. 마지막 패스가 대상에 뷰포트로 곧바로 그려서
    // 내장 값은 창 좌표가 되지만 prelude의 매크로가 칸 왼쪽 위를 뺀다. 셰이더가 둘이면 앞 패스(칸 크기 텍스처)도 같다
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new();
    let direct = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
        vec2 d = gl_FragCoord.xy - fragCoord;
        fragColor = vec4(gl_FragCoord.x / iResolution.x, gl_FragCoord.y / iResolution.y, abs(d.x) + abs(d.y), 1.0);
    }";
    for chain in [vec![("direct", direct)], vec![("invert", INVERT), ("direct", direct)]] {
        set_chain(&mut g, &chain);
        g.set_shader_regions(&[region(2, B, ShaderUniforms::default())]);
        let got = s.compose(&mut g, None, None);
        let (px, py) = (B.x as u32 + 70, B.y as u32 + 60);
        let p = channels(at(&got, px, py));
        near(
            p,
            [
                (70.5 / B.w as f32 * 255.0) as i32,
                (60.5 / B.h as f32 * 255.0) as i32,
                0,
            ],
            "칸 안 위치(gl_FragCoord 직접)",
        );
    }
}
