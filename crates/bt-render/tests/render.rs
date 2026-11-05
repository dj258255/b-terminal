//! 버퍼에 그려 픽셀로 확인한다. 시스템에 고정폭 글꼴이 없으면(일부 CI) 건너뛴다

use std::time::Instant;

use bt_core::{Rgb, Theme};
use bt_render::{Canvas, CursorStyle, DrawCell, DrawCursor, FontConfig, Fonts, GridView, draw_grid};
use bt_term::{CellFlags, Emulator, GridSize, Screen};

fn fonts(scale: f32) -> Option<Fonts> {
    match Fonts::new(&FontConfig {
        scale,
        ..FontConfig::default()
    }) {
        Ok(f) => Some(f),
        Err(e) => {
            eprintln!("건너뜀: {e}");
            None
        }
    }
}

fn cells(s: &Screen) -> Vec<DrawCell> {
    s.cells
        .iter()
        .map(|c| DrawCell {
            ch: c.ch,
            fg: c.fg,
            bg: c.bg,
            bold: c.flags.contains(CellFlags::BOLD),
            italic: c.flags.contains(CellFlags::ITALIC),
            underline: c.flags.contains(CellFlags::UNDERLINE),
            strike: c.flags.contains(CellFlags::STRIKE),
            wide: c.flags.contains(CellFlags::WIDE),
            spacer: c.flags.contains(CellFlags::SPACER),
            selected: c.flags.contains(CellFlags::SELECTED),
        })
        .collect()
}

struct Frame {
    buf: Vec<u32>,
    w: u32,
    h: u32,
    cw: u32,
    ch: u32,
}

fn render(fonts: &mut Fonts, bytes: &[u8], cols: u16, rows: u16, cursor: bool) -> Frame {
    let mut e = Emulator::new(GridSize::new(cols, rows), Theme::default());
    e.feed(bytes);
    let s = e.snapshot();
    let m = fonts.metrics();
    let (w, h) = (m.width * cols as u32, m.height * rows as u32);
    let mut buf = vec![0u32; (w * h) as usize];
    let dc = cells(&s);
    let t = Theme::default();
    let view = GridView {
        cols: s.cols,
        rows: s.rows,
        cells: &dc,
        cursor: cursor.then_some(DrawCursor {
            col: s.cursor.col,
            row: s.cursor.row,
            style: CursorStyle::Block,
            wide: s.cursor.wide,
            color: t.cursor,
            text_color: t.cursor_text,
        }),
        selection_bg: t.selection_background,
        selection_fg: t.selection_foreground,
        preedit: None,
    };
    let mut canvas = Canvas::new(&mut buf, w, h);
    canvas.clear(s.background);
    draw_grid(&mut canvas, fonts, 0, 0, &view);
    Frame {
        buf,
        w,
        h,
        cw: m.width,
        ch: m.height,
    }
}

/// 셀 (col,row) 안에서 바탕색 `bg`와 충분히 다른(잉크가 묻은) 픽셀 수. 1px 획은 안티에일리어싱으로 글자색에 딱 닿지 않는다
fn ink(f: &Frame, col: u32, row: u32, span: u32, bg: Rgb) -> usize {
    let mut n = 0;
    for y in row * f.ch..(row + 1) * f.ch {
        for x in col * f.cw..((col + span) * f.cw).min(f.w) {
            let p = Rgb::from_u32(f.buf[(y * f.w + x) as usize]);
            let d =
                (p.r as i32 - bg.r as i32).abs() + (p.g as i32 - bg.g as i32).abs() + (p.b as i32 - bg.b as i32).abs();
            if d > 120 {
                n += 1;
            }
        }
    }
    n
}

#[test]
fn ascii_glyph_paints_foreground_inside_its_cell() {
    let Some(mut f) = fonts(1.0) else { return };
    let fr = render(&mut f, b"A ", 4, 1, false);
    let bg = Theme::default().background;
    assert!(ink(&fr, 0, 0, 1, bg) > 3, "A 칸에 글자 픽셀이 없다");
    assert_eq!(ink(&fr, 1, 0, 1, bg), 0, "빈 칸에 글자가 번졌다");
}

#[test]
fn korean_uses_fallback_font_and_spans_two_cells() {
    let Some(mut f) = fonts(1.0) else { return };
    if f.glyph('한', bt_render::FontStyle::Regular).is_none() {
        eprintln!("건너뜀: 한글 글꼴이 없다");
        return;
    }
    let fr = render(&mut f, "한".as_bytes(), 4, 1, false);
    let bg = Theme::default().background;
    let left = ink(&fr, 0, 0, 1, bg);
    let right = ink(&fr, 1, 0, 1, bg);
    assert!(
        left > 3 && right > 3,
        "한글이 두 칸에 걸쳐 그려지지 않았다: {left} {right}"
    );
}

#[test]
fn box_drawing_lines_touch_cell_edges_so_rows_connect() {
    let Some(mut f) = fonts(1.0) else { return };
    let fr = render(&mut f, "│\r\n│".as_bytes(), 2, 2, false);
    let fg = Theme::default().foreground;
    // 첫 줄의 마지막 픽셀 줄과 둘째 줄의 첫 픽셀 줄 모두 선이 있어야 끊기지 않는다
    let row_has = |y: u32| (0..fr.cw).any(|x| Rgb::from_u32(fr.buf[(y * fr.w + x) as usize]) == fg);
    assert!(row_has(fr.ch - 1) && row_has(fr.ch), "세로선이 줄 사이에서 끊겼다");
}

#[test]
fn half_block_fills_exactly_top_half() {
    let Some(mut f) = fonts(1.0) else { return };
    let fr = render(&mut f, "\x1b[31;42m▀".as_bytes(), 1, 1, false);
    let t = Theme::default();
    let top = Rgb::from_u32(fr.buf[0]);
    let bottom = Rgb::from_u32(fr.buf[((fr.ch - 1) * fr.w) as usize]);
    assert_eq!(top, t.palette[1]);
    assert_eq!(bottom, t.palette[2]);
}

#[test]
fn block_cursor_inverts_its_cell() {
    let Some(mut f) = fonts(1.0) else { return };
    let fr = render(&mut f, b"", 3, 1, true);
    let t = Theme::default();
    assert_eq!(Rgb::from_u32(fr.buf[0]), t.cursor);
    assert_eq!(Rgb::from_u32(fr.buf[(fr.cw + 1) as usize]), t.background);
}

/// ADR-002의 실험: 200×60 화면을 Retina(2배)로 전체 다시 그리는 시간. `cargo test -p bt-render --release -- --ignored --nocapture`
#[test]
#[ignore]
fn frame_time_full_redraw_200x60_retina() {
    let Some(mut f) = fonts(2.0) else { return };
    let mut text = String::new();
    for i in 0..60 {
        text.push_str(&format!(
            "\x1b[3{}m{:03} 가나다라 the quick brown fox jumps over the lazy dog │ ▀▄ ",
            i % 8,
            i
        ));
        text.push_str(&"x".repeat(120));
        text.push_str("\r\n");
    }
    // 첫 그리기는 글리프 캐시를 채운다
    let _ = render(&mut f, text.as_bytes(), 200, 60, true);
    let mut times = Vec::new();
    for _ in 0..20 {
        let t0 = Instant::now();
        let fr = render(&mut f, text.as_bytes(), 200, 60, true);
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
        assert!(fr.w > 0 && fr.h > 0);
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let m = f.metrics();
    println!(
        "셀 {}x{}px, 화면 {}x{}px, 20회: 최소 {:.2}ms 중앙값 {:.2}ms 최대 {:.2}ms (해석 포함)",
        m.width,
        m.height,
        m.width * 200,
        m.height * 60,
        times[0],
        times[10],
        times[19]
    );
}

#[test]
fn embedded_jetbrains_mono_is_the_default_like_ghostty() {
    let f = Fonts::new(&FontConfig::default()).expect("넣어 둔 글꼴은 늘 있다");
    assert_eq!(f.family_name, "JetBrains Mono");
}

#[test]
fn nerd_font_icons_come_from_the_symbols_font() {
    let mut f = Fonts::new(&FontConfig::default()).unwrap();
    // U+E0A0: powerline 브랜치 아이콘, U+F07C: 폴더 열림
    for c in ['\u{e0a0}', '\u{f07c}'] {
        let g = f
            .glyph(c, bt_render::FontStyle::Regular)
            .unwrap_or_else(|| panic!("{:X} 아이콘이 없다", c as u32));
        assert!(g.width > 0 && g.height > 0);
    }
}

#[test]
fn adjust_cell_height_grows_the_cell() {
    let base = Fonts::new(&FontConfig::default()).unwrap().metrics();
    let taller = Fonts::new(&FontConfig {
        adjust_cell_height: Some(bt_render::font::Adjust::Percent(20.0)),
        ..FontConfig::default()
    })
    .unwrap()
    .metrics();
    assert!(taller.height > base.height, "{} > {}", taller.height, base.height);
    assert!(taller.baseline > base.baseline, "늘어난 만큼 글자를 가운데로 내린다");
    assert_eq!(taller.width, base.width);
}
