//! 투명 창·배경 그림을 층 합치기로(#359): 창 없이 합친 프레임(곱한 알파 아래층 + 칸 알파 격자 + 위층) =
//! 같은 순서로 CPU 투명 캔버스에 그린 프레임(알파 채널 포함). GPU 어댑터가 없으면 알리고 끝낸다

use bt_core::{Rgb, Theme};
use bt_gpu::{GpuRenderer, LayerUpdate, PaneDraw};
use bt_render::{Canvas, CursorStyle, DrawCell, DrawCursor, FontConfig, Fonts, GridView, Underline, draw_grid};

const BG: Rgb = Rgb { r: 30, g: 32, b: 40 };
const TAB_BG: Rgb = Rgb {
    r: 0x33,
    g: 0x38,
    b: 0x40,
};
const FG: Rgb = Rgb { r: 220, g: 220, b: 210 };
const COLS: usize = 24;
const ROWS: usize = 4;

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
        hidden: false,
    }
}

fn row(text: &str) -> Vec<DrawCell> {
    let mut v = Vec::new();
    for c in text.chars() {
        let wide = bt_render::char_cells(c) == 2;
        v.push(DrawCell { wide, ..cell(c) });
        if wide {
            v.push(DrawCell {
                spacer: true,
                ..cell(' ')
            });
        }
    }
    v.resize(COLS, cell(' '));
    v
}

fn pane_cells() -> Vec<DrawCell> {
    let mut cells = row("transparent 가나다 gy ->");
    cells.extend(row("colored cells ─│█▒ !="));
    cells.extend(row("underline and sel"));
    cells.extend(row("plain last row"));
    for (i, c) in cells.iter_mut().enumerate() {
        if (28..36).contains(&i) {
            c.bg = Rgb::new(180, 60, 40);
        }
        if (48..72).contains(&i) {
            c.underline = Underline::Curly;
            c.selected = (52..58).contains(&i);
        }
    }
    cells
}

/// 창 크기와 칸의 자리
struct Layout {
    w: u32,
    h: u32,
    at: (i32, i32),
    grid: (u32, u32),
}

fn layout(f: &Fonts) -> Layout {
    let m = f.metrics();
    let grid = (COLS as u32 * m.width, ROWS as u32 * m.height);
    let at = (13, 41);
    Layout {
        w: at.0 as u32 + grid.0 + 17,
        h: at.1 as u32 + grid.1 + 23,
        at,
        grid,
    }
}

/// 배경 그림 층: 창 크기, 곱한 알파 `alpha`의 그라데이션(CPU 배경 그림 합성이 내는 꼴)
fn backdrop(l: &Layout, alpha: u32) -> Vec<u32> {
    let mul = |c: u32| (c * alpha + 127) / 255;
    (0..l.w * l.h)
        .map(|i| {
            let (x, y) = (i % l.w, i / l.w);
            let (r, g, b) = (x * 255 / l.w, y * 255 / l.h, 90 + (x + y) % 100);
            (alpha << 24) | (mul(r) << 16) | (mul(g) << 8) | mul(b)
        })
        .collect()
}

#[derive(Clone, Copy)]
struct Setup {
    /// 투명 창의 알파(`background-opacity`)
    window: Option<u8>,
    cells: Option<u8>,
    /// 배경 그림 층의 곱한 알파
    image: Option<u32>,
}

/// CPU 길의 아래층과 같은 순서로 아래층을 그린다: 바탕 지우기, 탭 막대, 칸 바깥 사각형
fn lower_ops(c: &mut Canvas, l: &Layout) {
    c.clear(BG);
    c.fill_rect(0, 0, l.w, 30, TAB_BG);
    c.fill_rect(l.at.0 - 6, l.at.1 - 6, l.grid.0 + 12, l.grid.1 + 12, BG);
}

/// 위층: 투명 바탕에 그린다(반투명 막대, 칸 위 한 픽셀)
fn upper_ops(c: &mut Canvas, l: &Layout) {
    c.blend_rect(
        l.at.0 + l.grid.0 as i32 - 8,
        l.at.1,
        6,
        l.grid.1,
        Rgb::new(255, 255, 255),
        110,
    );
    c.fill_rect(l.at.0, 32, 40, 4, Rgb::new(90, 90, 90));
    c.blend_px(l.at.0 + 1, l.at.1 + 1, Rgb::new(255, 0, 0), 77);
}

fn configure<'a>(c: &mut Canvas<'a>, s: Setup, layer: Option<&'a [u32]>) {
    if let (Some(_), Some(layer)) = (s.image, layer) {
        c.set_backdrop(BG, layer);
    }
    if let Some(a) = s.window {
        c.set_see_through(BG, a);
    }
    c.set_cells_alpha(s.cells);
}

fn diff(a: &[u32], b: &[u32]) -> (usize, i32, usize) {
    assert_eq!(a.len(), b.len(), "프레임 크기가 다르다");
    let (mut n, mut max, mut over) = (0, 0, 0);
    for (x, y) in a.iter().zip(b) {
        if x != y {
            n += 1;
            let d = [0, 8, 16, 24]
                .iter()
                .map(|sh| (((x >> sh) & 0xff) as i32 - ((y >> sh) & 0xff) as i32).abs())
                .max()
                .unwrap_or(0);
            max = max.max(d);
            over += usize::from(d > 1);
        }
    }
    (n, max, over)
}

fn run(name: &str, s: Setup, with_upper: bool) {
    let Some(mut g) = gpu() else { return };
    let mut f = Fonts::new(&FontConfig {
        scale: 2.0,
        ..FontConfig::default()
    })
    .expect("넣어 둔 글꼴은 늘 있다");
    let l = layout(&f);
    let t = Theme::default();
    let cells = pane_cells();
    let cursor = DrawCursor {
        col: 1,
        row: 0,
        style: CursorStyle::Block,
        wide: false,
        color: t.cursor,
        text_color: t.cursor_text,
        opacity: 0.6,
    };
    let view = GridView {
        cols: COLS,
        rows: ROWS,
        cells: &cells,
        cursor: Some(cursor),
        selection_bg: t.selection_background,
        selection_fg: t.selection_foreground,
        preedit: None,
        min_contrast: 1.0,
    };
    let layer = s.image.map(|a| backdrop(&l, a));
    let n = (l.w * l.h) as usize;

    // 기준: CPU 캔버스 하나에 아래층 → 격자 → 위층 순서로
    let mut want = vec![0u32; n];
    {
        let mut c = Canvas::new(&mut want, l.w, l.h);
        configure(&mut c, s, layer.as_deref());
        lower_ops(&mut c, &l);
        draw_grid(&mut c, &mut f, l.at.0, l.at.1, &view);
        if with_upper {
            upper_ops(&mut c, &l);
        }
    }
    // GPU 길: CPU가 아래층(같은 설정)과 위층(투명 바탕)을 그리고 GPU가 격자를 합친다
    let mut lower = vec![0u32; n];
    {
        let mut c = Canvas::new(&mut lower, l.w, l.h);
        configure(&mut c, s, layer.as_deref());
        lower_ops(&mut c, &l);
    }
    let mut upper = vec![0u32; n];
    upper_ops(&mut Canvas::new(&mut upper, l.w, l.h), &l);
    match s.window {
        Some(a) => g.set_see_through(BG, a),
        None => g.clear_see_through(),
    }
    g.set_cells_alpha(s.cells);
    g.set_backdrop(s.image.map(|_| BG));
    let panes = [PaneDraw {
        font: 0,
        x: l.at.0,
        y: l.at.1,
        view: &view,
        clear: BG,
    }];
    let got = g.compose(
        &mut f,
        (l.w, l.h),
        LayerUpdate {
            lower: Some(&lower),
            upper: with_upper.then_some(&upper[..]),
        },
        &panes,
    );
    let (px, max, over) = diff(&want, &got);
    eprintln!("{name}: 다른 픽셀 {px}/{n}, 차이가 1을 넘는 픽셀 {over}, 최대 차이 {max}(알파 채널 포함)");
    assert!(max <= 3, "{name}: 최대 차이 {max}");
    assert!(over * 1000 < n * 5, "{name}: 차이가 1을 넘는 픽셀 {over}/{n}");

    // 알파가 실제로 나갔다: 기본 바탕 칸(맨 오른쪽 위)의 알파
    let m = f.metrics();
    let (x, y) = (l.at.0 as u32 + m.width * (COLS as u32 - 1), l.at.1 as u32 + 1);
    let alpha = got[(y * l.w + x) as usize] >> 24;
    let expect = match (s.image, s.window) {
        (Some(a), _) => a,
        (None, Some(a)) => a as u32,
        (None, None) => 255,
    };
    assert_eq!(alpha, expect, "{name}: 기본 바탕 칸의 알파");
}

#[test]
fn see_through_window_composes_like_cpu() {
    run(
        "투명 창 128",
        Setup {
            window: Some(128),
            cells: None,
            image: None,
        },
        false,
    );
}

#[test]
fn see_through_window_with_cells_alpha_and_upper_layer() {
    run(
        "투명 창 40 + 색 칸 200 + 위층",
        Setup {
            window: Some(40),
            cells: Some(200),
            image: None,
        },
        true,
    );
}

#[test]
fn backdrop_image_composes_like_cpu() {
    run(
        "배경 그림(불투명 창)",
        Setup {
            window: None,
            cells: None,
            image: Some(255),
        },
        false,
    );
}

#[test]
fn translucent_backdrop_image_with_cells_alpha_composes_like_cpu() {
    run(
        "배경 그림 + 투명 창 + 색 칸 알파 + 위층",
        Setup {
            window: Some(160),
            cells: Some(160),
            image: Some(160),
        },
        true,
    );
}
