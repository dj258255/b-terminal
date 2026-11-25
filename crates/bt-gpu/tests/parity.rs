//! GPU 렌더러가 채운 그리기(밑줄·상자 글자·선택·조합 중 글·최소 대비·빈 상자·투명 칸)를 CPU `draw_grid`와 픽셀로 견준다(#353).
//! GPU 어댑터가 없으면 알리고 끝낸다

use bt_core::{Rgb, Theme};
use bt_gpu::GpuRenderer;
use bt_render::{Canvas, CursorStyle, DrawCell, DrawCursor, FontConfig, Fonts, GridView, Underline, draw_grid};

fn fonts(scale: f32) -> Fonts {
    Fonts::new(&FontConfig {
        scale,
        ..FontConfig::default()
    })
    .expect("넣어 둔 글꼴은 늘 있다")
}

fn gpu() -> Option<GpuRenderer> {
    match GpuRenderer::with_atlas_size(1024) {
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

const BG: Rgb = Rgb { r: 30, g: 32, b: 40 };
const FG: Rgb = Rgb { r: 220, g: 220, b: 210 };

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

/// 글자 줄을 칸으로(넓은 글자는 뒤에 이어지는 칸을 둔다). `cols`칸으로 채우고 모자라면 공백
fn row_cells(text: &str, cols: usize) -> Vec<DrawCell> {
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
    v.resize(cols, cell(' '));
    v
}

fn grid(cells: &[DrawCell], cols: usize) -> GridView<'_> {
    let t = Theme::default();
    GridView {
        cols,
        rows: cells.len() / cols,
        cells,
        cursor: None,
        selection_bg: t.selection_background,
        selection_fg: t.selection_foreground,
        preedit: None,
        min_contrast: 1.0,
    }
}

#[derive(Clone, Copy, Default)]
struct Alpha {
    /// 투명 창의 알파
    window: Option<u8>,
    cells: Option<u8>,
}

fn cpu_frame(f: &mut Fonts, v: &GridView, a: Alpha) -> Vec<u32> {
    let m = f.metrics();
    let (w, h) = (m.width * v.cols as u32, m.height * v.rows as u32);
    let mut buf = vec![0u32; (w * h) as usize];
    let mut c = Canvas::new(&mut buf, w, h);
    if let Some(alpha) = a.window {
        c.set_see_through(BG, alpha);
    }
    c.set_cells_alpha(a.cells);
    c.clear(BG);
    draw_grid(&mut c, f, 0, 0, v);
    buf
}

fn gpu_frame(g: &mut GpuRenderer, f: &mut Fonts, v: &GridView, a: Alpha) -> Vec<u32> {
    match a.window {
        Some(alpha) => g.set_see_through(BG, alpha),
        None => g.clear_see_through(),
    }
    g.set_cells_alpha(a.cells);
    g.render(f, v, BG)
}

/// (다른 픽셀 수, 채널 하나의 최대 차이, 전체 픽셀 수). 알파 채널도 센다
fn diff(a: &[u32], b: &[u32]) -> (usize, i32, usize) {
    assert_eq!(a.len(), b.len(), "프레임 크기가 다르다");
    let (mut n, mut max) = (0, 0);
    for (x, y) in a.iter().zip(b) {
        if x != y {
            n += 1;
            for sh in [0, 8, 16, 24] {
                let d = (((x >> sh) & 0xff) as i32 - ((y >> sh) & 0xff) as i32).abs();
                max = max.max(d);
            }
        }
    }
    (n, max, a.len())
}

/// CPU와 GPU가 같은 화면을 그렸는지 견주고, `plain`(이 기능을 끈 화면)과는 다름을 확인한다(기능이 빠져도 통과하는 시험이 되지 않게).
/// 허용 오차는 기존 시험과 같다: 채널 최대 차이 2, 다른 픽셀 0.5% 미만
fn check(g: &mut GpuRenderer, f: &mut Fonts, name: &str, v: &GridView, plain: &GridView, a: Alpha) {
    let want = cpu_frame(f, v, a);
    let got = gpu_frame(g, f, v, a);
    let (n, max, total) = diff(&want, &got);
    eprintln!(
        "{name}: 다른 픽셀 {n}/{total} ({:.3}%), 최대 차이 {max}",
        n as f64 * 100.0 / total as f64
    );
    assert!(max <= 2, "{name}: 채널 최대 차이 {max}");
    assert!(n * 1000 < total * 5, "{name}: 다른 픽셀 {n}/{total}");
    let without = cpu_frame(f, plain, a);
    let (feature_px, _, _) = diff(&without, &want);
    assert!(feature_px > 0, "{name}: 시험 화면에 이 기능이 그린 것이 없다");
    // GPU도 그 기능을 그렸다: 기능 없는 CPU 화면과 다른 픽셀 수가 거의 같다
    let (gpu_px, _, _) = diff(&without, &got);
    assert!(
        gpu_px * 10 >= feature_px * 9,
        "{name}: GPU가 기능 픽셀을 덜 그렸다 {gpu_px}/{feature_px}"
    );
}

const SHAPES: [Underline; 5] = [
    Underline::Single,
    Underline::Double,
    Underline::Dotted,
    Underline::Dashed,
    Underline::Curly,
];

fn underlined(kind: Underline, color: Option<Rgb>) -> Vec<DrawCell> {
    let mut v = row_cells("gypq 가나 ab", 14);
    v.extend(row_cells("  _ x 한 ", 14));
    for c in &mut v {
        c.underline = kind;
        c.underline_color = color;
    }
    v
}

#[test]
fn underline_shapes_match_cpu() {
    let Some(mut g) = gpu() else { return };
    // 같은 렌더러로 배율을 바꿔 가며(칸 크기가 바뀌면 캐시를 비운다) 견준다
    for scale in [2.0, 1.0, 2.0] {
        let mut f = fonts(scale);
        for kind in SHAPES {
            for color in [None, Some(Rgb::new(255, 40, 60))] {
                let cells = underlined(kind, color);
                let plain = underlined(Underline::None, None);
                let name = format!("밑줄 {kind:?} 색 {} x{scale}", color.is_some());
                check(
                    &mut g,
                    &mut f,
                    &name,
                    &grid(&cells, 14),
                    &grid(&plain, 14),
                    Alpha::default(),
                );
            }
        }
    }
}

#[test]
fn box_and_block_chars_match_cpu() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts(2.0);
    // U+2500~259F 전부와 양쪽 경계 밖(U+24FF, U+25A0, U+25A1)
    let chars: Vec<char> = std::iter::once('\u{24ff}')
        .chain('\u{2500}'..='\u{259f}')
        .chain(['\u{25a0}', '\u{25a1}'])
        .collect();
    let cols = 16;
    let mut cells: Vec<DrawCell> = chars
        .iter()
        .map(|c| DrawCell {
            fg: Rgb::new(120, 200, 255),
            ..cell(*c)
        })
        .collect();
    cells.resize(cells.len().div_ceil(cols) * cols, cell(' '));
    let blank: Vec<DrawCell> = cells.iter().map(|_| cell(' ')).collect();
    check(
        &mut g,
        &mut f,
        "상자·블록 글자",
        &grid(&cells, cols),
        &grid(&blank, cols),
        Alpha::default(),
    );

    // 직접 그리는 글자만(글꼴 글리프가 끼지 않게): 가장자리 반올림도 없는 같은 픽셀
    let direct: Vec<char> = "─│━┃┌┐└┘├┤┬┴┼╭╮╰╯█▀▄▌▐▏▎▍▋▊▉▁▂▃▅▆▇░▒▓".chars().collect();
    let mut cells: Vec<DrawCell> = direct.iter().map(|c| cell(*c)).collect();
    cells.resize(cells.len().div_ceil(cols) * cols, cell(' '));
    let blank: Vec<DrawCell> = cells.iter().map(|_| cell(' ')).collect();
    let (v, p) = (grid(&cells, cols), grid(&blank, cols));
    let want = cpu_frame(&mut f, &v, Alpha::default());
    let got = gpu_frame(&mut g, &mut f, &v, Alpha::default());
    let (n, max, total) = diff(&want, &got);
    eprintln!("직접 그리는 상자 글자: 다른 픽셀 {n}/{total}, 최대 차이 {max}");
    assert!(max <= 1, "최대 차이 {max}");
    assert!(n * 10_000 < total, "다른 픽셀 {n}/{total}");
    assert!(diff(&cpu_frame(&mut f, &p, Alpha::default()), &got).0 > 1000);
}

#[test]
fn selected_cells_match_cpu() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts(2.0);
    for sel_fg in [None, Some(Rgb::new(10, 10, 10))] {
        let mut cells = row_cells("selected 가나다 text", 24);
        cells.extend(row_cells("second row ─│█", 24));
        for (i, c) in cells.iter_mut().enumerate() {
            c.selected = (3..15).contains(&i) || (30..40).contains(&i);
        }
        cells[5].underline = Underline::Curly;
        let plain: Vec<DrawCell> = cells
            .iter()
            .map(|c| DrawCell {
                selected: false,
                ..c.clone()
            })
            .collect();
        let mut v = grid(&cells, 24);
        v.selection_fg = sel_fg;
        let mut p = grid(&plain, 24);
        p.selection_fg = sel_fg;
        check(
            &mut g,
            &mut f,
            &format!("선택 칸 글자색 {}", sel_fg.is_some()),
            &v,
            &p,
            Alpha::default(),
        );
    }
}

/// #433: 사각 고르기가 덮는 칸(열 2~4, 줄 1~3의 사각)을 GPU도 CPU와 같게 그린다. 칸 왼쪽 위 픽셀이 사각 안에서만 고르기 바탕색이다
#[test]
fn block_selected_cells_form_a_rectangle_on_cpu_and_gpu() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts(2.0);
    let (cols, rows) = (8usize, 5usize);
    let mut cells: Vec<DrawCell> = (0..rows).flat_map(|_| row_cells("abcdefgh", cols)).collect();
    for (i, c) in cells.iter_mut().enumerate() {
        let (col, row) = (i % cols, i / cols);
        c.selected = (2..=4).contains(&col) && (1..=3).contains(&row);
    }
    let plain: Vec<DrawCell> = cells
        .iter()
        .map(|c| DrawCell {
            selected: false,
            ..c.clone()
        })
        .collect();
    let v = grid(&cells, cols);
    let p = grid(&plain, cols);
    check(&mut g, &mut f, "사각 고르기", &v, &p, Alpha::default());
    let m = f.metrics();
    let w = m.width as usize * cols;
    for (name, frame) in [
        ("CPU", cpu_frame(&mut f, &v, Alpha::default())),
        ("GPU", gpu_frame(&mut g, &mut f, &v, Alpha::default())),
    ] {
        for row in 0..rows {
            for col in 0..cols {
                let px = frame[row * m.height as usize * w + col * m.width as usize];
                let inside = (2..=4).contains(&col) && (1..=3).contains(&row);
                assert_eq!(
                    px & 0x00ff_ffff == BG.to_u32() & 0x00ff_ffff,
                    !inside,
                    "{name} 칸 ({col},{row}): 사각 안은 고르기 색, 밖은 화면 바탕"
                );
            }
        }
    }
}

#[test]
fn preedit_matches_cpu() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts(2.0);
    let t = Theme::default();
    let cells: Vec<DrawCell> = (0..3).flat_map(|_| row_cells("hello world", 16)).collect();
    let cursor = DrawCursor {
        col: 2,
        row: 1,
        style: CursorStyle::Block,
        wide: false,
        color: t.cursor,
        text_color: t.cursor_text,
        opacity: 1.0,
    };
    // 한글 조합 중 글과 영문, 직접 그리는 글자가 섞인다
    let mut v = grid(&cells, 16);
    v.cursor = Some(cursor);
    v.preedit = Some("한ㄱa─");
    let mut p = grid(&cells, 16);
    p.cursor = Some(cursor);
    check(&mut g, &mut f, "조합 중 글", &v, &p, Alpha::default());
}

#[test]
fn minimum_contrast_matches_cpu() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts(2.0);
    let mut cells = row_cells("low contrast text 가나", 24);
    cells.extend(row_cells("on a light cell ok", 24));
    for (i, c) in cells.iter_mut().enumerate() {
        // 바탕과 거의 같은 글자색, 둘째 줄은 밝은 바탕에 밝은 글자
        if i < 24 {
            c.fg = Rgb::new(40, 42, 50);
        } else {
            c.bg = Rgb::new(230, 230, 225);
            c.fg = Rgb::new(250, 250, 250);
        }
        c.selected = (6..12).contains(&i);
    }
    let mut v = grid(&cells, 24);
    v.min_contrast = 4.5;
    let p = grid(&cells, 24);
    check(&mut g, &mut f, "최소 대비", &v, &p, Alpha::default());
}

#[test]
fn missing_glyph_draws_an_empty_box() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts(2.0);
    // 어느 글꼴에도 없을 글자(사설 영역 끝). CPU가 빈 상자를 그리지 않으면 이 시험의 전제가 틀린 것이다
    let mut cells = row_cells("a\u{10fffd}b\u{e000}c", 8);
    cells.extend(row_cells("\u{10fffd} \u{10fffd}", 8));
    let blank: Vec<DrawCell> = cells.iter().map(|_| cell(' ')).collect();
    let v = grid(&cells, 8);
    check(&mut g, &mut f, "빈 상자", &v, &grid(&blank, 8), Alpha::default());
}

#[test]
fn transparent_window_matches_cpu() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts(2.0);
    let t = Theme::default();
    let mut cells = row_cells("transparent 가나다 gy", 24);
    cells.extend(row_cells("colored cells ─│█▒", 24));
    cells.extend(row_cells("underline and sel", 24));
    for (i, c) in cells.iter_mut().enumerate() {
        if (28..36).contains(&i) {
            c.bg = Rgb::new(180, 60, 40);
        }
        if i >= 48 {
            c.underline = Underline::Curly;
            c.selected = (52..58).contains(&i);
        }
    }
    let cursor = DrawCursor {
        col: 1,
        row: 0,
        style: CursorStyle::Block,
        wide: false,
        color: t.cursor,
        text_color: t.cursor_text,
        opacity: 0.6,
    };
    let mut v = grid(&cells, 24);
    v.cursor = Some(cursor);
    let m = f.metrics();
    let width = (m.width * 24) as usize;
    for (name, a) in [
        (
            "투명 창 알파 128",
            Alpha {
                window: Some(128),
                cells: None,
            },
        ),
        (
            "투명 창 + 색 칸 알파 128",
            Alpha {
                window: Some(128),
                cells: Some(128),
            },
        ),
        (
            "투명 창 알파 40 + 색 칸 알파 200",
            Alpha {
                window: Some(40),
                cells: Some(200),
            },
        ),
    ] {
        let want = cpu_frame(&mut f, &v, a);
        let got = gpu_frame(&mut g, &mut f, &v, a);
        let (n, max, total) = diff(&want, &got);
        eprintln!(
            "{name}: 다른 픽셀 {n}/{total} ({:.3}%), 최대 차이 {max}",
            n as f64 * 100.0 / total as f64
        );
        assert!(max <= 2, "{name}: 채널 최대 차이 {max}");
        assert!(n * 1000 < total * 5, "{name}: 다른 픽셀 {n}/{total}");
        // 알파가 실제로 나갔다: 기본 바탕 칸의 알파는 설정값, 색 칸은 cells 알파(없으면 불투명)
        let alpha_at = |x: usize, y: usize| got[y * width + x] >> 24;
        assert_eq!(
            alpha_at((m.width * 23) as usize, 1),
            a.window.unwrap() as u32,
            "{name}: 기본 바탕 알파"
        );
        assert_eq!(
            alpha_at((m.width * 5) as usize, (m.height + 1) as usize),
            a.cells.unwrap_or(255) as u32,
            "{name}: 색 바탕 칸 알파"
        );
    }
    // 끄면 다시 불투명 그림으로 돌아온다
    g.clear_see_through();
    g.set_cells_alpha(None);
    let opaque = g.render(&mut f, &v, BG);
    assert!(opaque.iter().all(|p| p >> 24 == 0xff));
}

/// #834: 숨김 칸은 고르기·최소 명암비·꾸밈이 있어도 글자를 그리지 않는다(GPU도 CPU와 같다)
#[test]
fn hidden_cells_match_cpu_and_draw_no_text() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts(2.0);
    let mut cells = row_cells("secret value ab", 16);
    cells.extend(row_cells("hidden 가나 ──", 16));
    for (i, c) in cells.iter_mut().enumerate() {
        c.selected = (2..9).contains(&(i % 16));
        if (4..11).contains(&(i % 16)) {
            c.hidden = true;
            c.underline = Underline::Single;
            c.strike = true;
            c.overline = true;
        }
    }
    let plain: Vec<DrawCell> = cells
        .iter()
        .map(|c| DrawCell {
            hidden: false,
            ..c.clone()
        })
        .collect();
    let mut v = grid(&cells, 16);
    v.min_contrast = 4.5;
    let mut p = grid(&plain, 16);
    p.min_contrast = 4.5;
    check(&mut g, &mut f, "숨김", &v, &p, Alpha::default());
    // 숨긴 칸만 있는 줄은 고른 바탕 색 한 가지뿐이다
    let only: Vec<DrawCell> = cells
        .iter()
        .map(|c| DrawCell {
            hidden: true,
            selected: true,
            ..c.clone()
        })
        .collect();
    let mut o = grid(&only, 16);
    o.min_contrast = 4.5;
    let frame = gpu_frame(&mut g, &mut f, &o, Alpha::default());
    let first = frame[0];
    assert!(frame.iter().all(|&px| px == first), "숨긴 칸에서 글자·꾸밈이 그려졌다");
}
