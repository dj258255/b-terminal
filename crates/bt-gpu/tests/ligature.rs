//! 합자(#359): GPU 렌더러가 CPU `draw_grid`와 같은 묶음 다듬기 결과(`bt_render::ligatures`)로 그린다.
//! 시험 글꼴은 넣어 둔 JetBrains Mono(calt 합자가 있다). 장면 시험은 GPU 없이, 픽셀 시험은 어댑터가 없으면 알리고 끝낸다

use bt_core::{Rgb, Theme};
use bt_gpu::GpuRenderer;
use bt_gpu::glyphs::GlyphCache;
use bt_gpu::scene::Scene;
use bt_render::{Canvas, CursorStyle, DrawCell, DrawCursor, FontConfig, Fonts, GridView, Underline, draw_grid};

const BG: Rgb = Rgb { r: 30, g: 32, b: 40 };
const FG: Rgb = Rgb { r: 220, g: 220, b: 210 };

fn fonts(features: Vec<([u8; 4], u16)>) -> Option<Fonts> {
    let f = Fonts::new(&FontConfig {
        scale: 2.0,
        features,
        ..FontConfig::default()
    })
    .expect("넣어 둔 글꼴은 늘 있다");
    if f.family_name != "JetBrains Mono" {
        eprintln!("건너뜀: 넣어 둔 합자 글꼴을 쓰지 못했다({})", f.family_name);
        return None;
    }
    Some(f)
}

fn gpu() -> Option<GpuRenderer> {
    let g = GpuRenderer::with_atlas_size(1024);
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
        faint: None,
        mark: None,
    }
}

fn row(text: &str, cols: usize) -> Vec<DrawCell> {
    let mut v: Vec<DrawCell> = text.chars().map(cell).collect();
    v.resize(cols, cell(' '));
    v
}

fn view(cells: &[DrawCell], cols: usize, cursor: Option<DrawCursor>) -> GridView<'_> {
    let t = Theme::default();
    GridView {
        cols,
        rows: cells.len() / cols,
        cells,
        cursor,
        selection_bg: t.selection_background,
        selection_fg: t.selection_foreground,
        selection_bg_cell: Default::default(),
        selection_fg_cell: Default::default(),
        preedit: None,
        min_contrast: 1.0,
        trail: None,
    }
}

fn block_cursor(col: usize, row: usize) -> DrawCursor {
    let t = Theme::default();
    DrawCursor {
        col,
        row,
        style: CursorStyle::Block,
        wide: false,
        color: t.cursor,
        text_color: t.cursor_text,
        opacity: 1.0,
    }
}

fn cpu_frame(f: &mut Fonts, v: &GridView) -> Vec<u32> {
    let m = f.metrics();
    let (w, h) = (m.width * v.cols as u32, m.height * v.rows as u32);
    let mut buf = vec![0u32; (w * h) as usize];
    let mut c = Canvas::new(&mut buf, w, h);
    c.clear(BG);
    draw_grid(&mut c, f, 0, 0, v);
    buf
}

fn diff(a: &[u32], b: &[u32]) -> (usize, i32) {
    assert_eq!(a.len(), b.len(), "프레임 크기가 다르다");
    let (mut n, mut max) = (0, 0);
    for (x, y) in a.iter().zip(b) {
        if x != y {
            n += 1;
            for sh in [0, 8, 16] {
                max = max.max((((x >> sh) & 0xff) as i32 - ((y >> sh) & 0xff) as i32).abs());
            }
        }
    }
    (n, max)
}

const LINES: [&str; 4] = [
    "a -> b != c === d",
    "x => y >= z <= w",
    "if a == b && c",
    "<!-- ok --> |> ::",
];

fn all_cells() -> Vec<DrawCell> {
    LINES.iter().flat_map(|l| row(l, 20)).collect()
}

/// 합자가 든 줄을 CPU와 GPU로 그려 견준다. 합자를 끈 글꼴로 그린 것과는 많이 달라야 시험이 뜻이 있다
#[test]
fn ligature_lines_match_cpu() {
    let Some(mut g) = gpu() else { return };
    let (Some(mut f), Some(mut plain_font)) = (fonts(vec![]), fonts(vec![(*b"calt", 0), (*b"liga", 0)])) else {
        return;
    };
    let cells = all_cells();
    let v = view(&cells, 20, None);
    let want = cpu_frame(&mut f, &v);
    let got = g.render(&mut f, &v, BG);
    let (n, max) = diff(&want, &got);
    let total = want.len();
    eprintln!("합자 줄: CPU와 다른 픽셀 {n}/{total}, 최대 차이 {max}");
    assert!(max <= 2, "채널 최대 차이 {max}");
    assert!(n * 1000 < total * 5, "다른 픽셀 {n}/{total}");

    // 합자가 있다는 증거: 합자를 끈 CPU 화면과 많이 다르고, GPU도 그만큼 다르다
    let plain = cpu_frame(&mut plain_font, &v);
    let (lig_px, _) = diff(&plain, &want);
    let (gpu_px, _) = diff(&plain, &got);
    eprintln!("합자를 끈 CPU 화면과 다른 픽셀: CPU {lig_px}, GPU {gpu_px}");
    assert!(lig_px > 200, "시험 화면에 합자가 없다: {lig_px}");
    assert!(gpu_px * 10 >= lig_px * 9, "GPU가 합자를 덜 그렸다 {gpu_px}/{lig_px}");
}

/// 커서가 합자 위에 있으면 그 칸에서 묶음이 끊겨 풀어 보인다(CPU와 같게)
#[test]
fn cursor_on_a_ligature_matches_cpu() {
    let Some(mut g) = gpu() else { return };
    let Some(mut f) = fonts(vec![]) else { return };
    let cells = all_cells();
    // 첫 줄의 `->`(칸 2·3), `===`(칸 10~12)의 가운데
    for (name, col) in [("->의 뒤 칸", 3usize), ("===의 가운데", 11)] {
        let v = view(&cells, 20, Some(block_cursor(col, 0)));
        let want = cpu_frame(&mut f, &v);
        let got = g.render(&mut f, &v, BG);
        let (n, max) = diff(&want, &got);
        eprintln!("커서 {name}: 다른 픽셀 {n}/{}, 최대 차이 {max}", want.len());
        assert!(max <= 2, "{name}: 채널 최대 차이 {max}");
        assert!(n * 1000 < want.len() * 5, "{name}: 다른 픽셀 {n}");
        // 커서가 없는 화면과 달라야 한다(풀림이 반영됐다)
        let free = g.render(&mut f, &view(&cells, 20, None), BG);
        assert!(diff(&free, &got).0 > 50, "{name}: 커서가 합자를 풀지 못했다");
    }
}

/// GPU 없이: 합자 줄과 커서가 합자를 푼 줄은 만들어진 글리프 인스턴스가 다르다(글리프 번호가 아틀라스 열쇠에 든다)
#[test]
fn scene_uses_glyph_ids_for_ligatures_and_breaks_at_the_cursor() {
    let Some(mut f) = fonts(vec![]) else { return };
    let cells = row("a->b", 8);
    let mut cache = GlyphCache::new(512, 2048);
    let mut scene = Scene::default();
    scene.build(&mut cache, &mut f, &view(&cells, 8, None), BG);
    let with = scene.glyphs.clone();
    let entries_with = cache.len();
    scene.build(&mut cache, &mut f, &view(&cells, 8, Some(block_cursor(2, 0))), BG);
    let broken = scene.glyphs.clone();
    assert_ne!(with, broken, "커서 칸에서 합자가 풀리지 않았다");
    assert!(cache.len() > entries_with, "낱글자 글리프가 새 열쇠로 들어간다");
}
