//! `alpha-blending`(#405): 글자를 섞을 색 공간. 창 없이 그려서 가장자리 픽셀을 기대한 값과 견준다.
//! GPU 어댑터가 없으면 알리고 끝낸다.
//!
//! 기대값은 `native`로 그린 같은 화면에서 픽셀마다의 덮음 정도 a를 거꾸로 구해(a = (결과 - 바탕) / (글자 - 바탕)) 만든다.
//! 덮음 정도가 너무 작거나 크면 8비트 반올림이 선형 → sRGB 변환에서 크게 늘어나므로 가운데 구간(0.2~0.8)만 견준다

use bt_core::{AlphaBlending, Rgb, Theme};
use bt_gpu::{GpuRenderer, LayerUpdate, PaneDraw};
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
        preedit: None,
        min_contrast: 1.0,
    }
}

fn frame(g: &mut GpuRenderer, f: &mut Fonts, cells: &[DrawCell], bg: Rgb, mode: AlphaBlending) -> Vec<u32> {
    g.set_alpha_blending(mode);
    g.render(f, &view(cells), bg)
}

fn ch(p: u32, shift: u32) -> f64 {
    ((p >> shift) & 0xff) as f64
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

const SHIFTS: [u32; 3] = [16, 8, 0];

/// 가운데 구간의 가장자리 픽셀 수와 기대값에서 가장 많이 벗어난 채널 값. `expect(a)`는 덮음 정도 a에서의 기대 채널 값
fn worst_vs_expectation(
    native: &[u32],
    got: &[u32],
    fg: Rgb,
    bg: Rgb,
    expect: impl Fn(f64, usize) -> f64,
) -> (usize, f64) {
    let (f, b) = ([fg.r, fg.g, fg.b], [bg.r, bg.g, bg.b]);
    // 글자색과 바탕색이 가장 많이 다른 채널에서 a를 구한다
    let k = (0..3).max_by_key(|&i| (f[i] as i32 - b[i] as i32).abs()).unwrap();
    let (mut n, mut worst) = (0, 0f64);
    for (p, q) in native.iter().zip(got) {
        let a = (ch(*p, SHIFTS[k]) - b[k] as f64) / (f[k] as f64 - b[k] as f64);
        if !(0.2..=0.8).contains(&a) {
            continue;
        }
        n += 1;
        for (i, sh) in SHIFTS.iter().enumerate() {
            worst = worst.max((ch(*q, *sh) - expect(a, i)).abs());
        }
    }
    (n, worst)
}

const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
const GREEN: Rgb = Rgb { r: 0, g: 255, b: 0 };

#[test]
fn native_is_the_default_and_matches_the_cpu_canvas() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts();
    let (fg, bg) = (Rgb::new(220, 220, 210), Rgb::new(30, 32, 40));
    let cells = row(fg, bg);
    let default = g.render(&mut f, &view(&cells), bg);
    let native = frame(&mut g, &mut f, &cells, bg, AlphaBlending::Native);
    assert_eq!(default, native, "기본은 native");
    // CPU `draw_grid`와 견준다: 기존 parity 시험과 같은 허용(채널 최대 차이 2)
    let m = f.metrics();
    let (w, h) = (m.width * COLS as u32, m.height);
    let mut buf = vec![0u32; (w * h) as usize];
    let mut c = Canvas::new(&mut buf, w, h);
    c.clear(bg);
    draw_grid(&mut c, &mut f, 0, 0, &view(&cells));
    let worst = native
        .iter()
        .zip(&buf)
        .flat_map(|(a, b)| SHIFTS.iter().map(move |s| (ch(*a, *s) - ch(*b, *s)).abs()))
        .fold(0f64, f64::max);
    assert!(worst <= 2.0, "native가 CPU와 {worst}만큼 다르다");
    // 섞는 방식을 바꿨다가 native로 되돌려도 같은 화면이다
    let _ = frame(&mut g, &mut f, &cells, bg, AlphaBlending::Linear);
    assert_eq!(frame(&mut g, &mut f, &cells, bg, AlphaBlending::Native), native);
}

#[test]
fn linear_red_on_green_edges_match_linear_blending() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts();
    let cells = row(RED, GREEN);
    let native = frame(&mut g, &mut f, &cells, GREEN, AlphaBlending::Native);
    let linear = frame(&mut g, &mut f, &cells, GREEN, AlphaBlending::Linear);
    let (fl, bl) = ([lin(255.0), 0.0, 0.0], [0.0, lin(255.0), 0.0]);
    let (n, worst) = worst_vs_expectation(&native, &linear, RED, GREEN, |a, i| enc(a * fl[i] + (1.0 - a) * bl[i]));
    eprintln!("linear 빨강/초록: 견준 가장자리 픽셀 {n}, 기대값과 최대 차이 {worst:.2}");
    assert!(n >= 50, "시험 화면에 가운데 구간 가장자리가 너무 적다: {n}");
    assert!(worst <= 3.0, "기대한 선형 섞기와 {worst}만큼 다르다");
    // native와는 눈에 띄게 다르다(어두워지는 가장자리가 없어진다): a=0.5에서 빨강 채널 128 → 188
    let (n2, gap) = worst_vs_expectation(&native, &linear, RED, GREEN, |a, i| {
        enc(a * fl[i] + (1.0 - a) * bl[i]) - [a * 255.0, (1.0 - a) * 255.0, 0.0][i]
    });
    assert!(n2 > 0 && gap > 10.0, "linear가 native와 같은 화면이다(gap {gap})");
    // 덮지 않은 픽셀과 다 덮은 픽셀은 그대로다
    for (p, q) in native.iter().zip(&linear) {
        let (r, gr) = (ch(*p, 16), ch(*p, 8));
        if (r == 0.0 && gr == 255.0) || (r == 255.0 && gr == 0.0) {
            assert_eq!(p, q, "덮음 0과 1은 어느 공간에서도 같다");
        }
    }
}

/// 가운데 밝기의 글자·바탕(0과 255가 아닌 값): 바탕이 선형 값으로 풀려 섞이는지 가린다(0·255는 변환의 고정점이라 못 가린다)
#[test]
fn linear_midtone_gray_blends_in_linear_space_without_touching_the_background() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts();
    let (fg, bg) = (Rgb::new(230, 230, 230), Rgb::new(60, 60, 60));
    let cells = row(fg, bg);
    let native = frame(&mut g, &mut f, &cells, bg, AlphaBlending::Native);
    let linear = frame(&mut g, &mut f, &cells, bg, AlphaBlending::Linear);
    let (n, worst) = worst_vs_expectation(&native, &linear, fg, bg, |a, _| {
        enc(a * lin(230.0) + (1.0 - a) * lin(60.0))
    });
    eprintln!("linear 회색: 견준 가장자리 픽셀 {n}, 기대값과 최대 차이 {worst:.2}");
    assert!(n >= 50 && worst <= 3.0, "기대한 선형 섞기와 {worst}만큼 다르다(n={n})");
    // 글리프를 그리지 않은 곳(바탕)은 그대로다: 지우기가 sRGB 뷰에서 되어 바탕색이 밝아지는 일이 없다
    assert_eq!(native[0], linear[0]);
    assert_eq!(*native.last().unwrap(), *linear.last().unwrap());
}

#[test]
fn linear_corrected_follows_the_correction_formula() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts();
    let (fg, bg) = (Rgb::new(255, 255, 255), Rgb::new(40, 40, 40));
    let cells = row(fg, bg);
    let native = frame(&mut g, &mut f, &cells, bg, AlphaBlending::Native);
    let linear = frame(&mut g, &mut f, &cells, bg, AlphaBlending::Linear);
    let corrected = frame(&mut g, &mut f, &cells, bg, AlphaBlending::LinearCorrected);
    let (fl, bl) = (lin(255.0), lin(40.0));
    // Ghostty와 같은 보정: 선형 글자·바탕의 밝기에서 sRGB로 섞은 밝기를 구하고 그 밝기에 맞는 a를 다시 구한다
    let (n, worst) = worst_vs_expectation(&native, &corrected, fg, bg, |a, _| {
        let blend_l = lin(a * enc(fl) + (1.0 - a) * enc(bl));
        let a2 = ((blend_l - bl) / (fl - bl)).clamp(0.0, 1.0);
        enc(a2 * fl + (1.0 - a2) * bl)
    });
    eprintln!("linear-corrected 흰/회색: 견준 픽셀 {n}, 기대값과 최대 차이 {worst:.2}");
    assert!(n >= 50 && worst <= 3.0, "보정 식과 {worst}만큼 다르다(n={n})");
    // 회색조에서는 native와 거의 같은 굵기다(보정 없는 linear는 크게 다르다)
    let (_, near) = worst_vs_expectation(&native, &corrected, fg, bg, |a, _| a * 255.0 + (1.0 - a) * 40.0);
    let (_, far) = worst_vs_expectation(&native, &linear, fg, bg, |a, _| a * 255.0 + (1.0 - a) * 40.0);
    eprintln!("native와의 최대 차이: corrected {near:.1}, linear {far:.1}");
    assert!(near <= 4.0, "corrected가 native와 {near}만큼 다르다");
    assert!(
        far >= 20.0,
        "linear가 native와 거의 같다({far}): 시험이 아무것도 가리지 못한다"
    );
}

#[test]
fn color_glyph_and_alpha_edge_cases_do_not_break() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts();
    // 바탕과 같은 글자색(보정이 0으로 나누지 않는다), 흰 바탕의 검은 글자
    for (fg, bg) in [
        (Rgb::new(90, 90, 90), Rgb::new(90, 90, 90)),
        (Rgb::new(0, 0, 0), Rgb::new(255, 255, 255)),
    ] {
        let cells = row(fg, bg);
        for mode in [AlphaBlending::Linear, AlphaBlending::LinearCorrected] {
            let px = frame(&mut g, &mut f, &cells, bg, mode);
            assert!(!px.is_empty());
        }
    }
    // 색 글리프(이모지)는 자기 색을 선형으로 바꿔 섞는다: 가장자리가 반투명이라 native와 다른 픽셀이 생긴다
    let mut emoji = row(Rgb::new(200, 200, 200), Rgb::new(30, 30, 80));
    emoji[2] = DrawCell {
        wide: true,
        emoji: true,
        ..cell('\u{1f600}', Rgb::new(200, 200, 200), Rgb::new(30, 30, 80))
    };
    emoji[3] = DrawCell {
        spacer: true,
        ..cell(' ', Rgb::new(200, 200, 200), Rgb::new(30, 30, 80))
    };
    let bg = Rgb::new(30, 30, 80);
    let n = frame(&mut g, &mut f, &emoji, bg, AlphaBlending::Native);
    let l = frame(&mut g, &mut f, &emoji, bg, AlphaBlending::Linear);
    assert_eq!(n.len(), l.len());
    assert!(
        n.iter().zip(&l).any(|(a, b)| a != b),
        "linear가 색 글리프를 건드리지 않았다"
    );
    // 같은 색이면 어느 공간이든 화면이 같다
    let same = row(Rgb::new(90, 90, 90), Rgb::new(90, 90, 90));
    let a = frame(&mut g, &mut f, &same, Rgb::new(90, 90, 90), AlphaBlending::Native);
    let b = frame(&mut g, &mut f, &same, Rgb::new(90, 90, 90), AlphaBlending::Linear);
    let worst = a
        .iter()
        .zip(&b)
        .flat_map(|(p, q)| SHIFTS.iter().map(move |s| (ch(*p, *s) - ch(*q, *s)).abs()))
        .fold(0f64, f64::max);
    assert!(worst <= 1.0, "글자색과 바탕이 같은데 {worst}만큼 다르다");
}

/// 창에 올리는 길(`present`가 쓰는 한 패스 합치기)도 같은 섞기를 낸다
#[test]
fn composed_path_blends_like_the_direct_path() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts();
    let cells = row(RED, GREEN);
    let m = f.metrics();
    let (w, h) = (m.width * COLS as u32, m.height);
    let lower = vec![0xff00_ff00u32; (w * h) as usize];
    let v = view(&cells);
    let native = frame(&mut g, &mut f, &cells, GREEN, AlphaBlending::Native);
    for mode in [
        AlphaBlending::Native,
        AlphaBlending::Linear,
        AlphaBlending::LinearCorrected,
    ] {
        let direct = frame(&mut g, &mut f, &cells, GREEN, mode);
        g.set_alpha_blending(mode);
        let panes = [PaneDraw {
            font: 0,
            x: 0,
            y: 0,
            view: &v,
            clear: GREEN,
        }];
        let layers = LayerUpdate {
            lower: Some(&lower),
            upper: None,
        };
        let composed = g.compose(&mut f, (w, h), layers, &panes);
        assert_eq!(composed, direct, "{mode:?}: 합친 길과 바로 그린 길이 다르다");
        if mode != AlphaBlending::Native {
            assert_ne!(composed, native, "{mode:?}: 합친 길이 선형으로 섞지 않았다");
        }
    }
}

/// 글자 프레임 시간: 글자가 가득한 200x60 화면(2배)을 `native`·`linear`·`linear-corrected`로 그리는 시간(창 없이, 한 번 잰 값).
/// 이슈 기준은 native 대비 +0.3ms 이하다. `cargo test -p bt-gpu --release --test alpha_blending -- --ignored --nocapture frame_time`
#[test]
#[ignore = "시간을 재서 찍기만 한다(--ignored로 돌린다)"]
fn frame_time_by_alpha_blending() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts();
    let (fg, bg) = (Rgb::new(220, 220, 210), Rgb::new(30, 32, 40));
    let cols = 200;
    let rows = 60;
    let mut cells = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let ch = char::from(b'!' + ((r * 7 + c) % 90) as u8);
            cells.push(cell(ch, fg, bg));
        }
    }
    let t = Theme::default();
    let v = GridView {
        cols,
        rows,
        cells: &cells,
        cursor: None,
        selection_bg: t.selection_background,
        selection_fg: t.selection_foreground,
        preedit: None,
        min_contrast: 1.0,
    };
    let m = f.metrics();
    let (w, h) = (m.width * cols as u32, m.height * rows as u32);
    let lower = vec![0xff00_0000u32 | (bg.r as u32) << 16 | (bg.g as u32) << 8 | bg.b as u32; (w * h) as usize];
    let pane = [PaneDraw {
        font: 0,
        x: 0,
        y: 0,
        view: &v,
        clear: bg,
    }];
    let stat = |mut v: Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        (v[0], v[v.len() / 2])
    };
    // 모드를 번갈아 여러 번 돌려 기기 상태(발열)가 한쪽에 몰리지 않게 한다
    let modes = [
        AlphaBlending::Native,
        AlphaBlending::Linear,
        AlphaBlending::LinearCorrected,
    ];
    let mut direct: Vec<Vec<f64>> = vec![Vec::new(); 3];
    let mut composed: Vec<Vec<f64>> = vec![Vec::new(); 3];
    for round in 0..25 {
        for (i, mode) in modes.iter().enumerate() {
            g.set_alpha_blending(*mode);
            let _ = g.render(&mut f, &v, bg);
            let first = LayerUpdate {
                lower: Some(&lower[..]),
                upper: None,
            };
            let _ = g.compose(&mut f, (w, h), first, &pane);
            // 앞 5회는 캐시·파이프라인을 데운다
            if round >= 5 {
                let _ = g.render(&mut f, &v, bg);
                direct[i].push(g.timings().gpu_ms);
                let _ = g.compose(&mut f, (w, h), LayerUpdate::default(), &pane);
                composed[i].push(g.timings().gpu_ms);
            }
        }
    }
    for (i, mode) in modes.iter().enumerate() {
        let (d0, d1) = stat(direct[i].clone());
        let (c0, c1) = stat(composed[i].clone());
        eprintln!("{mode:?}: draw GPU 최소 {d0:.3}ms 중앙값 {d1:.3}ms | compose 최소 {c0:.3}ms 중앙값 {c1:.3}ms");
    }
}
