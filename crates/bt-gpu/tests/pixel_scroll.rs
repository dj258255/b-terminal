//! 픽셀 스크롤(kitty `pixel_scroll`, #902): 격자를 줄 높이보다 작은 픽셀만큼 위로 밀어 그릴 때 GPU 길이 CPU 길(`draw_grid_shifted`)과
//! 같은 픽셀을 내는지 견준다. 맨 위 줄은 위로 잘리고, 맨 아래에 걸친 줄은 보이는 만큼만 그려지며, 격자 사각형 밖(둘레)은 건드리지 않는다.
//! GPU 어댑터가 없으면 알리고 끝낸다

use bt_core::{Rgb, Theme};
use bt_gpu::{GpuRenderer, LayerUpdate, PaneDraw};
use bt_render::{
    Canvas, CursorStyle, DrawCell, DrawCursor, FontConfig, Fonts, GridView, Under, Underline, draw_grid_shifted,
    draw_grid_under,
};

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
/// 격자 둘레의 바탕: 격자 밖으로 번진 것이 있으면 이 색이 아닌 픽셀로 드러난다
const AROUND: u32 = 0xff30_5060;
const COLS: usize = 24;
/// 보이는 줄 수. 그리는 줄은 하나 더 많다(맨 아래에 걸친 줄)
const VISIBLE: usize = 5;
const PAD: i32 = 8;

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

fn row_cells(text: &str) -> Vec<DrawCell> {
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

/// 보이는 줄 + 걸친 줄. 고른 칸·밑줄·취소선·윗줄·한글·색 바탕을 섞는다
fn screen() -> Vec<DrawCell> {
    let mut cells = Vec::new();
    for t in [
        "top line gy 가나다 Hello",
        "second row ~~~~ 한글 text",
        "third WWW iii underline ok",
        "fourth row mixed 水 漢字 x",
        "fifth row sits at the foot",
        "hanging line below the end",
    ] {
        cells.extend(row_cells(t));
    }
    // 고른 칸, 밑줄, 취소선, 색 바탕
    for c in 2..8 {
        cells[COLS + c].selected = true;
    }
    for c in 0..8 {
        cells[2 * COLS + c].underline = Underline::Single;
        cells[3 * COLS + c].strike = true;
        cells[4 * COLS + c].overline = true;
    }
    for c in 10..16 {
        cells[3 * COLS + c].bg = Rgb::new(120, 40, 40);
        cells[5 * COLS + c].bg = Rgb::new(40, 100, 60);
    }
    cells
}

fn view(cells: &[DrawCell], cursor: Option<DrawCursor>) -> GridView<'_> {
    let t = Theme::default();
    GridView {
        cols: COLS,
        rows: cells.len() / COLS,
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

fn cursor(row: usize, style: CursorStyle) -> DrawCursor {
    let t = Theme::default();
    DrawCursor {
        col: 3,
        row,
        style,
        wide: false,
        color: t.cursor,
        text_color: t.cursor_text,
        opacity: 1.0,
    }
}

/// 창 크기와 격자 왼쪽 위
fn geometry(f: &Fonts) -> ((u32, u32), (i32, i32)) {
    let m = f.metrics();
    (
        (
            m.width * COLS as u32 + 2 * PAD as u32,
            m.height * VISIBLE as u32 + 2 * PAD as u32,
        ),
        (PAD, PAD),
    )
}

fn cpu(f: &mut Fonts, v: &GridView, shift: u32) -> Vec<u32> {
    let ((w, h), (ox, oy)) = geometry(f);
    let mut buf = vec![AROUND; (w * h) as usize];
    let mut c = Canvas::new(&mut buf, w, h);
    let m = f.metrics();
    c.fill_rect(ox, oy, m.width * COLS as u32, m.height * VISIBLE as u32, BG);
    if shift == 0 {
        // 줄에 맞춘 화면: 걸친 줄은 없다
        draw_grid_under(&mut c, f, ox, oy, v, &Under::default());
    } else {
        draw_grid_shifted(&mut c, f, ox, oy, v, &Under::default(), shift);
    }
    buf
}

fn gpu_frame(g: &mut GpuRenderer, f: &mut Fonts, v: &GridView, shift: u32) -> Vec<u32> {
    let ((w, h), (ox, oy)) = geometry(f);
    let m = f.metrics();
    let mut lower = vec![AROUND; (w * h) as usize];
    Canvas::new(&mut lower, w, h).fill_rect(ox, oy, m.width * COLS as u32, m.height * VISIBLE as u32, BG);
    let panes = [PaneDraw {
        pixel_shift: shift,
        font: 0,
        x: ox,
        y: oy,
        view: v,
        clear: BG,
    }];
    g.compose(
        f,
        (w, h),
        LayerUpdate {
            lower: Some(&lower),
            upper: None,
        },
        &panes,
    )
}

/// (다른 픽셀 수, 채널 하나의 최대 차이, 전체 픽셀 수)
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

/// 격자 사각형 밖 픽셀이 `AROUND` 그대로인가
fn outside_untouched(f: &Fonts, frame: &[u32]) -> bool {
    let ((w, _), (ox, oy)) = geometry(f);
    let m = f.metrics();
    let (gw, gh) = (m.width as i32 * COLS as i32, m.height as i32 * VISIBLE as i32);
    frame.iter().enumerate().all(|(i, &p)| {
        let (x, y) = ((i as u32 % w) as i32, (i as u32 / w) as i32);
        let inside = x >= ox && x < ox + gw && y >= oy && y < oy + gh;
        inside || p == AROUND
    })
}

#[test]
fn gpu_matches_cpu_when_shifted() {
    let Some(mut g) = gpu() else { return };
    for scale in [1.0f32, 2.0] {
        let mut f = fonts(scale);
        let ch = f.metrics().height;
        let cells = screen();
        let plain = vec![cell(' '); cells.len()];
        for shift in [1, 3, ch / 2, ch - 1] {
            // 커서가 없는 화면, 보이는 줄의 커서, 걸친 줄의 커서(모양마다)
            let cursors = [
                None,
                Some(cursor(1, CursorStyle::Block)),
                Some(cursor(2, CursorStyle::Beam)),
                Some(cursor(5, CursorStyle::Block)),
                Some(cursor(5, CursorStyle::Underline)),
                Some(cursor(0, CursorStyle::Hollow)),
            ];
            for (i, c) in cursors.into_iter().enumerate() {
                let v = view(&cells, c);
                let want = cpu(&mut f, &v, shift);
                let got = gpu_frame(&mut g, &mut f, &v, shift);
                let (n, max, total) = diff(&want, &got);
                let name = format!("배율 {scale} 밀기 {shift}px 커서 {i}");
                assert!(
                    max <= 2 && n * 200 < total,
                    "{name}: 다른 픽셀 {n}/{total}, 최대 차이 {max}"
                );
                assert!(outside_untouched(&f, &want), "{name}: CPU가 격자 밖을 칠했다");
                assert!(outside_untouched(&f, &got), "{name}: GPU가 격자 밖을 칠했다");
                // 밀지 않은 화면과 달라야 한다(밀기가 빠져도 통과하는 시험이 되지 않게)
                let unshifted = cpu(&mut f, &view(&cells, c), 0);
                assert_ne!(want, unshifted, "{name}: 밀었는데 줄에 맞춘 화면과 같다");
                // 글자가 없는 화면과도 달라야 한다(글자가 그려졌다)
                let blank = cpu(&mut f, &view(&plain, None), shift);
                assert_ne!(want, blank, "{name}: 글자가 그려지지 않았다");
            }
        }
    }
}

/// 걸친 줄만 한 줄 뺀 줄에 맞춘 화면(`shift = 0`, 보이는 줄만)은 CPU·GPU가 같다(기본값이 지금과 같은 픽셀임을 지킨다)
#[test]
fn without_a_shift_the_view_is_the_plain_view() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts(1.0);
    let cells = screen();
    let visible = &cells[..COLS * VISIBLE];
    let v = view(visible, Some(cursor(1, CursorStyle::Block)));
    let want = cpu(&mut f, &v, 0);
    let got = gpu_frame(&mut g, &mut f, &v, 0);
    let (n, max, total) = diff(&want, &got);
    assert!(max <= 2 && n * 200 < total, "다른 픽셀 {n}/{total}, 최대 차이 {max}");
}

/// 걸친 줄이 맨 위 줄 위로 올려 그려진 만큼은 보이는 줄의 글자와 겹치지 않는다: `shift`만큼 올린 화면의 위쪽 `ch - shift`는 `shift = 0`이 아닌
/// 두 번째 줄의 글자 아랫부분이다(CPU 기준으로 맨 윗줄의 아랫부분이 격자 맨 위에 온다)
#[test]
fn the_top_row_is_cut_by_the_shift() {
    let mut f = fonts(1.0);
    let ch = f.metrics().height;
    let cells = screen();
    let shift = ch / 2;
    let v = view(&cells, None);
    let shifted = cpu(&mut f, &v, shift);
    // 둘째 줄을 맨 윗줄로 올려 둔 화면(걸친 줄 없이)에서 같은 모양이 `ch - shift` 아래에 있다: 밀린 화면의 y 줄은 밀지 않은 화면의 y + shift 줄이다
    let unshifted = cpu(&mut f, &view(&cells, None), 0);
    let ((w, _), (ox, oy)) = geometry(&f);
    let m = f.metrics();
    for y in 0..(m.height as i32 * VISIBLE as i32 - shift as i32) {
        for x in 0..(m.width as i32 * COLS as i32) {
            let a = shifted[((oy + y) as u32 * w + (ox + x) as u32) as usize];
            let b = unshifted[((oy + y + shift as i32) as u32 * w + (ox + x) as u32) as usize];
            assert_eq!(a, b, "({x}, {y})");
        }
    }
}
