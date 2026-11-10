//! 사용자 셰이더 엔진(ADR-040): 창 없이 합친 프레임 뒤에 셰이더 패스를 돌린 결과를 본다.
//! GPU 어댑터가 없으면 알리고 끝낸다

use bt_core::Rgb;
use bt_gpu::{CustomShaderSource, GpuRenderer, LayerUpdate, PaneDraw, ShaderUniforms};
use bt_render::{DrawCell, FontConfig, Fonts, GridView, Underline};

const COLS: usize = 20;
const ROWS: usize = 3;
const BG: Rgb = Rgb { r: 30, g: 32, b: 40 };
const FG: Rgb = Rgb { r: 220, g: 220, b: 210 };

const IDENTITY: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    fragColor = texture(iChannel0, fragCoord / iResolution.xy);
}";
const INVERT: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec4 c = texture(iChannel0, fragCoord / iResolution.xy);
    fragColor = vec4(1.0 - c.rgb, 1.0);
}";
/// 가로 왼쪽 반만 빨강으로 덮는다
const LEFT_RED: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec4 c = texture(iChannel0, fragCoord / iResolution.xy);
    fragColor = fragCoord.x < iResolution.x * 0.5 ? vec4(1.0, 0.0, 0.0, 1.0) : c;
}";
/// 빨강과 파랑을 맞바꾼다
const SWAP_RB: &str = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
    vec4 c = texture(iChannel0, fragCoord / iResolution.xy);
    fragColor = vec4(c.b, c.g, c.r, 1.0);
}";

fn gpu() -> Option<GpuRenderer> {
    let g = GpuRenderer::new_headless();
    if g.is_none() {
        eprintln!("건너뜀: GPU 어댑터가 없다");
    }
    g
}

fn src(name: &str, glsl: &str) -> CustomShaderSource {
    CustomShaderSource {
        name: name.to_string(),
        glsl: glsl.to_string(),
    }
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

/// 장면: 비대칭 그라데이션 아래층(위·아래, 왼쪽·오른쪽이 모두 다르다) 위에 글자 칸 하나
struct Scene {
    fonts: Fonts,
    w: u32,
    h: u32,
    cells: Vec<DrawCell>,
    lower: Vec<u32>,
}

impl Scene {
    fn new(w: u32, h: u32) -> Self {
        let fonts = Fonts::new(&FontConfig {
            scale: 2.0,
            ..FontConfig::default()
        })
        .expect("넣어 둔 글꼴은 늘 있다");
        let mut cells = Vec::new();
        for line in ["Hello shader 가나다", "gjpqy 0123456789 {}", "bottom line"] {
            let mut row: Vec<DrawCell> = line.chars().map(cell).collect();
            row.resize(COLS, cell(' '));
            cells.extend(row);
        }
        let lower = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                let (r, g, b) = (x * 255 / w.max(2), y * 255 / h.max(2), 60 + (x + y) % 90);
                0xff00_0000 | r << 16 | g << 8 | b
            })
            .collect();
        Self {
            fonts,
            w,
            h,
            cells,
            lower,
        }
    }

    fn compose(&mut self, g: &mut GpuRenderer) -> Vec<u32> {
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
            x: 12,
            y: 9,
            view: &view,
            clear: BG,
        }];
        g.compose(
            &mut self.fonts,
            (self.w, self.h),
            LayerUpdate {
                lower: Some(&self.lower),
                upper: None,
            },
            &panes,
        )
    }
}

fn channels(p: u32) -> [i32; 3] {
    [(p >> 16 & 0xff) as i32, (p >> 8 & 0xff) as i32, (p & 0xff) as i32]
}

/// 채널 하나의 최대 차이(알파 제외)
fn max_diff(a: &[u32], b: &[u32]) -> i32 {
    assert_eq!(a.len(), b.len(), "프레임 크기가 다르다");
    a.iter()
        .zip(b)
        .map(|(x, y)| {
            let (x, y) = (channels(*x), channels(*y));
            (0..3).map(|i| (x[i] - y[i]).abs()).max().unwrap_or(0)
        })
        .max()
        .unwrap_or(0)
}

fn set(g: &mut GpuRenderer, shaders: &[(&str, &str)]) -> Vec<String> {
    let v: Vec<_> = shaders.iter().map(|(n, s)| src(n, s)).collect();
    g.set_custom_shaders(&v)
}

fn baseline(g: &mut GpuRenderer, s: &mut Scene) -> Vec<u32> {
    assert!(!g.has_custom_shaders());
    s.compose(g)
}

#[test]
fn identity_shader_matches_the_plain_frame_exactly() {
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new(300, 200);
    let want = baseline(&mut g, &mut s);
    let errors = set(&mut g, &[("identity", IDENTITY)]);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(g.has_custom_shaders());
    let got = s.compose(&mut g);
    assert_eq!(max_diff(&want, &got), 0);
    assert!(want == got, "항등 셰이더의 픽셀이 다르다");
}

#[test]
fn invert_shader_inverts_background_and_glyph_pixels() {
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new(300, 200);
    let want = baseline(&mut g, &mut s);
    assert!(set(&mut g, &[("invert", INVERT)]).is_empty());
    let got = s.compose(&mut g);
    for (i, (w, p)) in want.iter().zip(&got).enumerate() {
        let (w, p) = (channels(*w), channels(*p));
        for c in 0..3 {
            assert!(
                (255 - w[c] - p[c]).abs() <= 1,
                "픽셀 {i} 채널 {c}: 원래 {} 뒤집은 값 {}",
                w[c],
                p[c]
            );
        }
    }
    // 글자가 실제로 들어 있다: 바탕(BG)이 아닌 칸 픽셀이 있다
    let glyph_px = want
        .iter()
        .filter(|p| channels(**p) == [FG.r as i32, FG.g as i32, FG.b as i32])
        .count();
    assert!(glyph_px > 20, "글자 픽셀 {glyph_px}");
}

#[test]
fn two_inverts_make_the_original_and_order_matters() {
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new(300, 200);
    let want = baseline(&mut g, &mut s);
    assert!(set(&mut g, &[("a", INVERT), ("b", INVERT)]).is_empty());
    let got = s.compose(&mut g);
    assert!(max_diff(&want, &got) <= 1, "뒤집기 둘이 항등이 아니다");

    // 순서: [왼쪽 빨강, 빨강·파랑 맞바꾸기]이면 왼쪽은 파랑, [맞바꾸기, 왼쪽 빨강]이면 왼쪽이 빨강
    assert!(set(&mut g, &[("left", LEFT_RED), ("swap", SWAP_RB)]).is_empty());
    let a = s.compose(&mut g);
    assert!(set(&mut g, &[("swap", SWAP_RB), ("left", LEFT_RED)]).is_empty());
    let b = s.compose(&mut g);
    let w = s.w as usize;
    let (left, right) = (5 * w + 3, 5 * w + w - 4);
    assert_eq!(channels(a[left]), [0, 0, 255], "[왼쪽 빨강, 맞바꾸기]의 왼쪽");
    assert_eq!(channels(b[left]), [255, 0, 0], "[맞바꾸기, 왼쪽 빨강]의 왼쪽");
    let o = channels(want[right]);
    assert_eq!(channels(a[right]), [o[2], o[1], o[0]], "오른쪽은 맞바꾼 값");
    assert_eq!(channels(b[right]), [o[2], o[1], o[0]], "오른쪽은 맞바꾼 값");
}

/// 시험 셰이더 하나가 화면 전체를 한 색으로 칠한다: 첫 픽셀을 읽는다
fn probe(g: &mut GpuRenderer, s: &mut Scene, name: &str, body: &str) -> [i32; 3] {
    let glsl = format!("void mainImage(out vec4 fragColor, in vec2 fragCoord) {{ {body} }}");
    let errors = set(g, &[(name, &glsl)]);
    assert!(errors.is_empty(), "{errors:?}");
    let out = s.compose(g);
    assert!(out.iter().all(|p| *p == out[0]), "{name}: 한 색이 아니다");
    channels(out[0])
}

fn near(got: [i32; 3], want: [i32; 3], what: &str) {
    for i in 0..3 {
        assert!((got[i] - want[i]).abs() <= 1, "{what}: 채널 {i}: {got:?} != {want:?}");
    }
}

#[test]
fn uniform_values_reach_the_shader() {
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new(300, 200);
    let mut u = ShaderUniforms {
        time: 2.5,
        time_delta: 0.5,
        frame: 5,
        current_cursor: [30.0, 40.0, 50.0, 60.0],
        previous_cursor: [10.0, 20.0, 70.0, 80.0],
        current_cursor_color: [0.1, 0.2, 0.3, 1.0],
        previous_cursor_color: [0.5, 0.6, 0.7, 0.8],
        current_cursor_style: 3,
        previous_cursor_style: 2,
        cursor_visible: 1,
        time_cursor_change: 1.0,
        time_focus: 3.0,
        focus: 1,
        background: [0.1, 0.2, 0.3],
        foreground: [0.4, 0.5, 0.6],
        cursor_color: [0.7, 0.8, 0.9],
        cursor_text: [0.15, 0.25, 0.35],
        selection_background: [0.45, 0.55, 0.65],
        selection_foreground: [0.75, 0.85, 0.95],
        ..ShaderUniforms::default()
    };
    u.palette[0] = [0.0, 0.0, 0.2];
    u.palette[1] = [0.4, 0.0, 0.0];
    u.palette[255] = [0.0, 0.6, 0.0];
    g.set_shader_uniforms(&u);
    let px = |f: f32| (f * 255.0).round() as i32;

    near(
        probe(
            &mut g,
            &mut s,
            "time",
            "fragColor = vec4(iTime / 10.0, iTimeDelta / 10.0, float(iFrame) / 10.0, 1.0);",
        ),
        [px(0.25), px(0.05), px(0.5)],
        "시간·프레임",
    );
    near(
        probe(
            &mut g,
            &mut s,
            "cursor",
            "fragColor = vec4(iCurrentCursor.x / 100.0, iPreviousCursor.y / 100.0, iCurrentCursor.w / 100.0, 1.0);",
        ),
        [px(0.3), px(0.2), px(0.6)],
        "커서 사각형",
    );
    near(
        probe(
            &mut g,
            &mut s,
            "cursor-color",
            "fragColor = vec4(iCurrentCursorColor.y, iPreviousCursorColor.z, iPreviousCursorColor.a, 1.0);",
        ),
        [px(0.2), px(0.7), px(0.8)],
        "커서 색",
    );
    near(
        probe(
            &mut g,
            &mut s,
            "ints",
            "fragColor = vec4(float(iCurrentCursorStyle) / 4.0, float(iPreviousCursorStyle) / 4.0, float(iCursorVisible) * 0.5 + float(iFocus) * 0.25, 1.0);",
        ),
        [px(0.75), px(0.5), px(0.75)],
        "정수 유니폼",
    );
    near(
        probe(
            &mut g,
            &mut s,
            "change",
            "fragColor = vec4(iTimeCursorChange / 10.0, iTimeFocus / 10.0, 0.0, 1.0);",
        ),
        [px(0.1), px(0.3), 0],
        "커서·초점 시각",
    );
    near(
        probe(
            &mut g,
            &mut s,
            "palette",
            "fragColor = vec4(iPalette[255].y, iPalette[1].x, iPalette[0].z, 1.0);",
        ),
        [px(0.6), px(0.4), px(0.2)],
        "팔레트(배열 stride)",
    );
    near(
        probe(
            &mut g,
            &mut s,
            "colors-a",
            "fragColor = vec4(iBackgroundColor.y, iForegroundColor.z, iCursorColor.x, 1.0);",
        ),
        [px(0.2), px(0.6), px(0.7)],
        "vec3 색 앞쪽",
    );
    near(
        probe(
            &mut g,
            &mut s,
            "colors-b",
            "fragColor = vec4(iCursorText.y, iSelectionForegroundColor.x, iSelectionBackgroundColor.z, 1.0);",
        ),
        [px(0.25), px(0.75), px(0.65)],
        "vec3 색 뒤쪽(선택 앞·뒤 순서)",
    );
    near(
        probe(
            &mut g,
            &mut s,
            "resolution",
            "fragColor = vec4(iResolution.x / 512.0, iResolution.y / 512.0, iChannelResolution[0].y / 512.0 + (iResolution.z - 1.0), 1.0);",
        ),
        [px(300.0 / 512.0), px(200.0 / 512.0), px(200.0 / 512.0)],
        "해상도",
    );
    // 쓰지 않는 유니폼(Ghostty도 0으로 둔다)은 이름으로 읽혀야 하고 0이다
    near(
        probe(
            &mut g,
            &mut s,
            "unused",
            "fragColor = vec4(iChannelTime[2] + 0.25, iFrameRate + iSampleRate + iMouse.x + iDate.w + 0.5, iChannelResolution[1].x + 0.75, 1.0);",
        ),
        [px(0.25), px(0.5), px(0.75)],
        "쓰지 않는 유니폼",
    );
}

#[test]
fn frag_coord_origin_is_top_left_like_metal() {
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new(300, 200);
    assert!(
        set(
            &mut g,
            &[(
                "coord",
                "void mainImage(out vec4 fragColor, in vec2 fragCoord) { fragColor = vec4(fragCoord.x / iResolution.x, fragCoord.y / iResolution.y, 0.0, 1.0); }"
            )]
        )
        .is_empty()
    );
    assert!(g.has_custom_shaders());
    let out = s.compose(&mut g);
    let w = s.w as usize;
    near(channels(out[100 * w + 150]), [128, 128, 0], "가운데 픽셀");
    assert!(
        channels(out[0])[1] <= 2,
        "맨 위 줄의 y는 0 근처여야 한다: {:?}",
        channels(out[0])
    );
    assert!(channels(out[199 * w])[1] >= 253, "맨 아래 줄의 y는 1 근처여야 한다");
    assert!(channels(out[0])[0] <= 2 && channels(out[w - 1])[0] >= 253);
}

#[test]
fn ghostty_test_shaders_compile_and_render() {
    let Some(mut g) = gpu() else { return };
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/shaders/");
    let read = |n: &str| std::fs::read_to_string(format!("{dir}{n}")).expect("시험 셰이더 파일");
    let (crt, focus) = (read("test_shadertoy_crt.glsl"), read("test_shadertoy_focus.glsl"));
    let mut s = Scene::new(300, 200);
    let want = baseline(&mut g, &mut s);
    let errors = set(&mut g, &[("crt", &crt), ("focus", &focus)]);
    assert!(errors.is_empty(), "{errors:?}");
    g.set_shader_uniforms(&ShaderUniforms {
        time: 1.0,
        focus: 1,
        ..ShaderUniforms::default()
    });
    let got = s.compose(&mut g);
    assert!(max_diff(&want, &got) > 20, "셰이더가 화면을 바꾸지 않았다");
    // 초점이 없으면 초점 셰이더가 빨강을 섞는다(원본 바탕의 빨강이 늘어난다)
    assert!(set(&mut g, &[("focus", &focus)]).is_empty());
    g.set_shader_uniforms(&ShaderUniforms::default());
    let unfocused = s.compose(&mut g);
    let red = |v: &[u32]| v.iter().map(|p| channels(*p)[0] as i64).sum::<i64>();
    assert!(red(&unfocused) > red(&want), "초점 없음 분기가 빨강을 섞지 않았다");
}

#[test]
fn invalid_shaders_report_errors_without_panicking_and_are_skipped() {
    let Some(mut g) = gpu() else { return };
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/shaders/");
    let invalid = std::fs::read_to_string(format!("{dir}test_shadertoy_invalid.glsl")).expect("시험 셰이더 파일");
    let mut s = Scene::new(300, 200);
    let want = baseline(&mut g, &mut s);

    let errors = set(&mut g, &[("invalid.glsl", &invalid)]);
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(
        errors[0].contains("invalid.glsl"),
        "오류 문구에 이름이 있어야 한다: {}",
        errors[0]
    );
    assert!(
        errors[0].contains("mainImage"),
        "naga의 메시지가 있어야 한다: {}",
        errors[0]
    );
    assert!(!g.has_custom_shaders(), "성공한 것이 없으면 꺼져 있어야 한다");
    assert!(s.compose(&mut g) == want, "성공한 셰이더가 없으면 원래 길이어야 한다");

    // 다른 셰이더와 함께 주면 그것만 건너뛴다. 줄 번호는 사용자 소스의 줄이다
    let bad_syntax = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {\n    fragColor = vec4(1.0 +);\n}";
    let bad_type = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {\n    fragColor = vec3(1.0);\n}";
    let errors = set(
        &mut g,
        &[
            ("invalid.glsl", &invalid),
            ("invert", INVERT),
            ("syntax", bad_syntax),
            ("type", bad_type),
        ],
    );
    assert_eq!(errors.len(), 3, "{errors:?}");
    assert!(errors[1].starts_with("syntax"), "{}", errors[1]);
    assert!(errors[1].contains("2줄"), "사용자 소스의 줄 번호: {}", errors[1]);
    assert!(errors[2].starts_with("type"), "{}", errors[2]);
    assert!(g.has_custom_shaders());
    let got = s.compose(&mut g);
    for (w, p) in want.iter().zip(&got) {
        let (w, p) = (channels(*w), channels(*p));
        assert!(
            (0..3).all(|c| (255 - w[c] - p[c]).abs() <= 1),
            "남은 셰이더(뒤집기)만 돌아야 한다"
        );
    }
}

#[test]
fn turning_shaders_off_returns_to_the_plain_path_and_resize_keeps_working() {
    let Some(mut g) = gpu() else { return };
    let mut s = Scene::new(300, 200);
    let want = baseline(&mut g, &mut s);
    assert!(set(&mut g, &[("invert", INVERT)]).is_empty());
    assert!(g.has_custom_shaders());
    assert!(s.compose(&mut g) != want, "뒤집기가 화면을 바꿔야 한다");
    assert!(g.set_custom_shaders(&[]).is_empty());
    assert!(!g.has_custom_shaders());
    assert!(s.compose(&mut g) == want, "껐다 켜도(빈 목록) 원래 길과 같아야 한다");

    // 창 크기가 바뀌어도(중간 텍스처를 다시 만든다) 항등은 항등이다
    assert!(set(&mut g, &[("identity", IDENTITY)]).is_empty());
    let mut big = Scene::new(421, 263);
    let plain_big = {
        assert!(g.set_custom_shaders(&[]).is_empty());
        big.compose(&mut g)
    };
    assert!(set(&mut g, &[("identity", IDENTITY)]).is_empty());
    assert!(g.has_custom_shaders());
    assert!(big.compose(&mut g) == plain_big, "큰 크기의 항등");
    assert!(s.compose(&mut g) == want, "작은 크기로 돌아와도 같다");
}

/// 컴파일 시간과 셰이더 패스 한 번의 시간을 재서 찍는다(창 없이, 한 번 잰 값). `--ignored --nocapture`로 돌린다
#[test]
#[ignore = "시간을 재서 찍기만 한다"]
fn measure_compile_and_pass_cost() {
    let Some(mut g) = gpu() else { return };
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/shaders/");
    let crt = std::fs::read_to_string(format!("{dir}test_shadertoy_crt.glsl")).expect("시험 셰이더 파일");
    eprintln!("어댑터: {}", g.adapter_name());
    for (name, glsl) in [("identity", IDENTITY), ("crt", crt.as_str())] {
        let mut ms = Vec::new();
        for _ in 0..5 {
            let t = std::time::Instant::now();
            assert!(set(&mut g, &[(name, glsl)]).is_empty());
            ms.push(t.elapsed().as_secs_f64() * 1000.0);
        }
        eprintln!("컴파일(읽기+검증+파이프라인) {name}: {ms:.1?} ms");
    }
    let mut s = Scene::new(2000, 1360);
    let mut median = |g: &mut GpuRenderer, label: &str| {
        let mut v = Vec::new();
        for _ in 0..30 {
            s.compose(g);
            v.push(g.timings().gpu_ms);
        }
        v.sort_by(f64::total_cmp);
        eprintln!(
            "2000x1360 {label}: gpu_ms 중앙값 {:.2} (최소 {:.2}, 최대 {:.2})",
            v[15], v[0], v[29]
        );
        v[15]
    };
    assert!(g.set_custom_shaders(&[]).is_empty());
    let base = median(&mut g, "셰이더 없음");
    for (label, list) in [
        ("항등 1패스", vec![("identity", IDENTITY)]),
        ("crt 1패스", vec![("crt", crt.as_str())]),
        ("crt 3패스", vec![("crt", crt.as_str()); 3]),
    ] {
        assert!(set(&mut g, &list).is_empty());
        let m = median(&mut g, label);
        eprintln!("  없음과의 차이 {:.2} ms", m - base);
    }
}

#[test]
fn identity_in_a_see_through_window_keeps_the_alpha() {
    let Some(mut g) = gpu() else { return };
    g.set_see_through(BG, 160);
    let mut s = Scene::new(300, 200);
    // 아래층도 반투명(곱한 알파 0x90)이라 합친 프레임의 알파가 255가 아니다
    for p in &mut s.lower {
        let c = channels(*p).map(|v| (v * 0x90 / 255) as u32);
        *p = 0x90 << 24 | c[0] << 16 | c[1] << 8 | c[2];
    }
    let want = baseline(&mut g, &mut s);
    assert!(
        want.iter().any(|p| p >> 24 != 0xff),
        "알파가 255가 아닌 픽셀이 있어야 한다"
    );
    assert!(set(&mut g, &[("identity", IDENTITY)]).is_empty());
    let got = s.compose(&mut g);
    assert!(g.has_custom_shaders());
    assert!(want == got, "곱한 알파 픽셀(알파 포함)이 같아야 한다");

    // 셰이더가 정말 돌았는지: 색만 절반으로 하고 알파는 그대로 내면 알파는 같고 색은 절반이다
    let half = "void mainImage(out vec4 fragColor, in vec2 fragCoord) {
        vec4 c = texture(iChannel0, fragCoord / iResolution.xy);
        fragColor = vec4(c.rgb * 0.5, c.a);
    }";
    assert!(set(&mut g, &[("half", half)]).is_empty());
    let halved = s.compose(&mut g);
    for (w, p) in want.iter().zip(&halved) {
        assert_eq!(w >> 24, p >> 24, "알파는 셰이더 출력 그대로(여기서는 입력과 같다)");
        let (w, p) = (channels(*w), channels(*p));
        assert!(
            (0..3).all(|c| (w[c] / 2 - p[c]).abs() <= 1),
            "색이 절반이어야 한다: {w:?} {p:?}"
        );
    }
}
