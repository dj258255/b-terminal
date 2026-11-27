//! `text-composition-strategy`(#911): 글자 덮음 곡선. 창 없이 그려서 CPU 길과 GPU 길의 픽셀을 견준다.
//! GPU 어댑터가 없으면 알리고 끝낸다.

use bt_core::{AlphaBlending, Rgb, TextComposition, Theme};
use bt_gpu::GpuRenderer;
use bt_render::{Canvas, DrawCell, FontConfig, Fonts, GridView, Underline, draw_grid};

fn fonts() -> Fonts {
    Fonts::new(&FontConfig {
        scale: 2.0,
        ..FontConfig::default()
    })
    .expect("넣어 둔 글꼴은 늘 있다")
}

fn gpu() -> Option<GpuRenderer> {
    match GpuRenderer::new_headless() {
        Some(g) => {
            eprintln!("어댑터: {}", g.adapter_name());
            Some(g)
        }
        None => {
            eprintln!("건너뜀: GPU 어댑터가 없다");
            None
        }
    }
}

fn cell(ch: char, fg: Rgb, bg: Rgb) -> DrawCell {
    DrawCell {
        ch,
        fg,
        bg,
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
        hidden: false,
        faint: None,
    }
}

const COLS: usize = 16;
const TEXT: &str = "Hello, gypq|/\\OS~";

fn row(fg: Rgb, bg: Rgb) -> Vec<DrawCell> {
    let mut v: Vec<DrawCell> = TEXT.chars().take(COLS).map(|c| cell(c, fg, bg)).collect();
    v.resize(COLS, cell(' ', fg, bg));
    v
}

fn view(cells: &[DrawCell]) -> GridView<'_> {
    let t = Theme::default();
    GridView {
        cols: COLS,
        rows: 1,
        cells,
        cursor: None,
        selection_bg: t.selection_background,
        selection_fg: t.selection_foreground,
        selection_bg_cell: Default::default(),
        selection_fg_cell: Default::default(),
        preedit: None,
        min_contrast: 1.0,
        trail: None,
    }
}

fn cpu(f: &mut Fonts, cells: &[DrawCell], bg: Rgb, tc: Option<TextComposition>) -> Vec<u32> {
    let m = f.metrics();
    let (w, h) = (m.width * COLS as u32, m.height);
    let mut buf = vec![0u32; (w * h) as usize];
    let mut c = Canvas::new(&mut buf, w, h);
    if let Some(tc) = tc {
        c.set_text_composition(tc);
    }
    c.clear(bg);
    draw_grid(&mut c, f, 0, 0, &view(cells));
    buf
}

fn gpu_frame(g: &mut GpuRenderer, f: &mut Fonts, cells: &[DrawCell], bg: Rgb, tc: TextComposition) -> Vec<u32> {
    g.set_text_composition(tc);
    g.render(f, &view(cells), bg)
}

fn worst(a: &[u32], b: &[u32]) -> i32 {
    a.iter()
        .zip(b)
        .flat_map(|(x, y)| [0, 8, 16].map(|s| (((x >> s) & 0xff) as i32 - ((y >> s) & 0xff) as i32).abs()))
        .max()
        .unwrap_or(0)
}

fn differing(a: &[u32], b: &[u32]) -> usize {
    a.iter().zip(b).filter(|(x, y)| x != y).count()
}

const PAIRS: [(Rgb, Rgb); 3] = [
    (Rgb { r: 0, g: 0, b: 0 }, Rgb { r: 255, g: 255, b: 255 }),
    (Rgb { r: 255, g: 255, b: 255 }, Rgb { r: 0, g: 0, b: 0 }),
    (Rgb { r: 220, g: 220, b: 210 }, Rgb { r: 30, g: 32, b: 40 }),
];

/// 곡선을 정하지 않은 화면(기본)은 `none`과 같고, 곡선을 걸었다가 `none`으로 되돌려도 같다. CPU와도 같다
#[test]
fn default_pixels_are_unchanged_on_cpu_and_gpu() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts();
    for (fg, bg) in PAIRS {
        let cells = row(fg, bg);
        let plain = g.render(&mut f, &view(&cells), bg);
        let none = gpu_frame(&mut g, &mut f, &cells, bg, TextComposition::NONE);
        assert_eq!(plain, none, "기본은 곡선 없음");
        let _ = gpu_frame(&mut g, &mut f, &cells, bg, TextComposition::PLATFORM);
        assert_eq!(
            gpu_frame(&mut g, &mut f, &cells, bg, TextComposition::NONE),
            plain,
            "되돌리면 같다"
        );
        let cpu_plain = cpu(&mut f, &cells, bg, None);
        assert_eq!(cpu_plain, cpu(&mut f, &cells, bg, Some(TextComposition::NONE)));
        assert!(
            worst(&plain, &cpu_plain) <= 2,
            "기본 GPU와 CPU: {}",
            worst(&plain, &cpu_plain)
        );
    }
}

/// 같은 곡선을 CPU와 GPU가 같은 픽셀로 그린다(채널 최대 차이 2, 기존 parity 시험과 같은 허용)
#[test]
fn curve_matches_between_cpu_and_gpu() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts();
    let curves = [
        TextComposition::PLATFORM,
        TextComposition {
            gamma: 2.2,
            contrast: 0.0,
        },
        TextComposition {
            gamma: 0.5,
            contrast: 80.0,
        },
    ];
    for tc in curves {
        for (fg, bg) in PAIRS {
            let cells = row(fg, bg);
            let want = cpu(&mut f, &cells, bg, Some(tc));
            let got = gpu_frame(&mut g, &mut f, &cells, bg, tc);
            let w = worst(&want, &got);
            let plain = cpu(&mut f, &cells, bg, None);
            let changed = differing(&plain, &want);
            eprintln!("{tc:?} {fg:?}/{bg:?}: CPU/GPU 최대 차이 {w}, 곡선으로 바뀐 픽셀 {changed}");
            assert!(w <= 2, "{tc:?} {fg:?}/{bg:?}: CPU와 GPU가 {w}만큼 다르다");
            // 감마만 있고 흰 글자 검은 바탕이면 가중이 0이라 바뀌지 않는 것이 kitty 식이다("밝은 글자는 거의 안 바뀐다")
            if tc.contrast == 0.0 && fg == Rgb::new(255, 255, 255) {
                assert_eq!(changed, 0, "가중 0인데 바뀌었다");
            } else {
                assert!(
                    changed > 50,
                    "곡선이 화면을 바꾸지 않았다(시험이 아무것도 못 가린다): {changed}"
                );
            }
        }
    }
}

/// 어두운 글자 흰 바탕은 굵어지고(덮은 픽셀 합이 늘어남) 밝은 글자 어두운 바탕은 명암만 곱해진다
#[test]
fn platform_makes_dark_on_light_thicker_than_light_on_dark() {
    let mut f = fonts();
    let mut ink = |fg: Rgb, bg: Rgb, tc: Option<TextComposition>| -> f64 {
        let cells = row(fg, bg);
        let px = cpu(&mut f, &cells, bg, tc);
        // 글자색과 바탕이 가장 다른 채널의 (바탕에서 멀어진 양)을 더한다
        px.iter()
            .map(|p| ((p >> 16) & 0xff) as f64 - bg.r as f64)
            .map(f64::abs)
            .sum()
    };
    let (black, white) = (Rgb::new(0, 0, 0), Rgb::new(255, 255, 255));
    let dark_gain = ink(black, white, Some(TextComposition::PLATFORM)) / ink(black, white, None);
    let light_gain = ink(white, black, Some(TextComposition::PLATFORM)) / ink(white, black, None);
    eprintln!("먹물 늘어난 배율: 어두운 글자 {dark_gain:.3}, 밝은 글자 {light_gain:.3}");
    assert!(dark_gain > light_gain + 0.05, "{dark_gain} vs {light_gain}");
    assert!(light_gain > 1.0, "명암 30%는 밝은 글자도 조금 굵게 한다");
}

fn lin(v: f64) -> f64 {
    let c = v / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn enc(v: f64) -> f64 {
    let c = if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    c.clamp(0.0, 1.0) * 255.0
}

/// `alpha-blending = linear`와 함께 쓰면 곡선을 건 덮음으로 선형 섞기를 한다(곡선이 먼저, 섞기가 나중)
#[test]
fn curve_is_applied_before_linear_blending() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts();
    let (fg, bg) = (Rgb::new(230, 230, 230), Rgb::new(60, 60, 60));
    let cells = row(fg, bg);
    g.set_alpha_blending(AlphaBlending::Native);
    let native = gpu_frame(&mut g, &mut f, &cells, bg, TextComposition::NONE);
    g.set_alpha_blending(AlphaBlending::Linear);
    let tc = TextComposition {
        gamma: 2.0,
        contrast: 20.0,
    };
    let curved = gpu_frame(&mut g, &mut f, &cells, bg, tc);
    let uncurved = gpu_frame(&mut g, &mut f, &cells, bg, TextComposition::NONE);
    assert!(
        differing(&curved, &uncurved) > 50,
        "곡선이 선형 섞기 화면을 바꾸지 않았다"
    );
    let (mut n, mut worst_gap) = (0, 0f64);
    for (p, q) in native.iter().zip(&curved) {
        let a = (((p >> 8) & 0xff) as f64 - 60.0) / (230.0 - 60.0);
        if !(0.2..=0.7).contains(&a) {
            continue;
        }
        let a2 = tc.coverage(a as f32, [230.0 / 255.0; 3], [60.0 / 255.0; 3]) as f64;
        let want = enc(a2 * lin(230.0) + (1.0 - a2) * lin(60.0));
        n += 1;
        worst_gap = worst_gap.max((((q >> 8) & 0xff) as f64 - want).abs());
    }
    eprintln!("곡선+linear: 견준 가장자리 {n}, 기대값과 최대 차이 {worst_gap:.2}");
    assert!(n >= 50, "견줄 가장자리가 너무 적다: {n}");
    assert!(worst_gap <= 4.0, "곡선 뒤 선형 섞기와 {worst_gap}만큼 다르다");
}
