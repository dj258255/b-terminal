//! 버퍼에 그려 픽셀로 확인한다. 시스템에 고정폭 글꼴이 없으면(일부 CI) 건너뛴다

use std::time::Instant;

use bt_core::{Rgb, Theme};
use bt_render::{Canvas, CursorStyle, DrawCell, DrawCursor, FontConfig, Fonts, GridView, draw_grid, draw_grid_rows};
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
    let mut cells: Vec<DrawCell> = s
        .cells
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
            emoji: c.combining.contains(&'\u{fe0f}'),
            cluster: None,
            cluster_cells: 0,
        })
        .collect();
    bt_render::cluster::mark(&mut cells, s.cols, |i| {
        let c = &s.cells[i];
        bt_render::cluster::ClusterCell {
            ch: c.ch,
            combining: &c.combining,
            spacer: c.flags.contains(CellFlags::SPACER),
        }
    });
    cells
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

fn view_of<'a>(s: &Screen, dc: &'a [DrawCell], t: &Theme) -> GridView<'a> {
    GridView {
        cols: s.cols,
        rows: s.rows,
        cells: dc,
        cursor: Some(DrawCursor {
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
    }
}

/// #40: 바뀐 줄만 다시 그린 결과가 통째로 다시 그린 결과와 픽셀까지 같다.
/// 바뀐 줄의 위아래에 칸 밖으로 넘치는 한글·상자 선을 두고, 커서도 옮긴다
#[test]
fn partial_redraw_matches_full_redraw_pixel_for_pixel() {
    let Some(f) = fonts(2.0) else { return };
    check_partial_redraw(f);
    // 셀 높이를 줄여 글자가 위아래 줄로 넘치게 한 경우(adjust-cell-height = -30%)
    let squeezed = Fonts::new(&FontConfig {
        scale: 2.0,
        adjust_cell_height: Some(bt_core::config::Adjust::Percent(-30.0)),
        ..FontConfig::default()
    })
    .unwrap();
    check_partial_redraw(squeezed);
}

fn check_partial_redraw(mut f: Fonts) {
    let t = Theme::default();
    let (cols, rows) = (24u16, 7u16);
    let m = f.metrics();
    let (w, h) = (m.width * cols as u32, m.height * rows as u32);
    let mut e = Emulator::new(GridSize::new(cols, rows), t.clone());
    for i in 0..rows - 1 {
        e.feed(format!("\x1b[3{}m한글 줄 {i} │▀▄ gjpqy\x1b[0m\r\n", i % 8).as_bytes());
    }
    let before = e.snapshot();
    // 3번 줄을 바꾸고 커서가 그 줄로 간다
    e.feed("\x1b[4;3H바뀐 글 ÄÉ_".as_bytes());
    let after = e.snapshot();

    let full = |s: &Screen, f: &mut Fonts| {
        let dc = cells(s);
        let mut buf = vec![0u32; (w * h) as usize];
        let mut c = Canvas::new(&mut buf, w, h);
        c.clear(s.background);
        draw_grid(&mut c, f, 0, 0, &view_of(s, &dc, &t));
        buf
    };
    let want = full(&after, &mut f);
    let mut buf = full(&before, &mut f);
    // 바뀐 줄: 칸이 다르거나 커서가 있던·있는 줄
    let (a, b) = (cells(&before), cells(&after));
    let n = cols as usize;
    let dirty: Vec<bool> = (0..rows as usize)
        .map(|r| a[r * n..(r + 1) * n] != b[r * n..(r + 1) * n] || r == before.cursor.row || r == after.cursor.row)
        .collect();
    assert!(
        dirty.iter().filter(|d| **d).count() < rows as usize,
        "몇 줄만 바뀌어야 의미가 있다"
    );
    let dc = cells(&after);
    let mut c = Canvas::new(&mut buf, w, h);
    let spans = draw_grid_rows(&mut c, &mut f, 0, 0, &view_of(&after, &dc, &t), &dirty);
    assert!(!spans.is_empty());
    let diff = buf.iter().zip(&want).filter(|(x, y)| x != y).count();
    assert_eq!(diff, 0, "통째 그리기와 다른 픽셀 {diff}개");
}

/// 실험 E2(#40): E1과 같은 화면에서 한 줄만 바뀐 프레임을 바뀐 줄만 다시 그리는 시간과 통째로 다시 그리는 시간.
/// VT 해석은 빼고 그리기만 잰다. `cargo test -p bt-render --release --test render -- --ignored --nocapture e2`
#[test]
#[ignore]
fn e2_partial_vs_full_redraw_200x60_retina() {
    let Some(mut f) = fonts(2.0) else { return };
    let t = Theme::default();
    let (cols, rows) = (200u16, 60u16);
    let mut e = Emulator::new(GridSize::new(cols, rows), t.clone());
    for i in 0..rows - 1 {
        e.feed(
            format!(
                "\x1b[3{}m{i:03} 가나다라 the quick brown fox jumps over the lazy dog │ ▀▄ {}\x1b[0m\r\n",
                i % 8,
                "x".repeat(120)
            )
            .as_bytes(),
        );
    }
    let m = f.metrics();
    let (w, h) = (m.width * cols as u32, m.height * rows as u32);
    let mut buf = vec![0u32; (w * h) as usize];
    let mut out = vec![0u32; (w * h) as usize];
    let mut full_ms = Vec::new();
    let mut part_ms = Vec::new();
    let mut copy_ms = Vec::new();
    for n in 0..21 {
        // 마지막 줄(시계)만 바꾼다
        e.feed(format!("\r시계 {n}  ").as_bytes());
        let s = e.snapshot();
        let dc = cells(&s);
        let view = view_of(&s, &dc, &t);
        let t0 = Instant::now();
        {
            let mut c = Canvas::new(&mut buf, w, h);
            c.clear(s.background);
            draw_grid(&mut c, &mut f, 0, 0, &view);
        }
        let full = t0.elapsed().as_secs_f64() * 1000.0;
        let mut dirty = vec![false; rows as usize];
        dirty[s.cursor.row] = true;
        let t1 = Instant::now();
        {
            let mut c = Canvas::new(&mut buf, w, h);
            draw_grid_rows(&mut c, &mut f, 0, 0, &view, &dirty);
        }
        let part = t1.elapsed().as_secs_f64() * 1000.0;
        // 앱은 들고 있는 프레임을 창 버퍼로 통째로 옮긴다
        let t2 = Instant::now();
        out.copy_from_slice(&buf);
        let copy = t2.elapsed().as_secs_f64() * 1000.0;
        // 첫 회는 글리프 캐시를 채우므로 뺀다
        if n > 0 {
            full_ms.push(full);
            part_ms.push(part);
            copy_ms.push(copy);
        }
    }
    let stat = |v: &mut Vec<f64>| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        format!(
            "최소 {:.2}ms 중앙값 {:.2}ms 최대 {:.2}ms",
            v[0],
            v[v.len() / 2],
            v[v.len() - 1]
        )
    };
    println!("화면 {w}x{h}px, 20회");
    println!("통째: {}", stat(&mut full_ms));
    println!("바뀐 줄(1줄): {}", stat(&mut part_ms));
    println!("프레임 옮기기: {}", stat(&mut copy_ms));
    assert!(out[0] == buf[0]);
}

/// #37: 😀는 색 글꼴(Apple Color Emoji·Noto Color Emoji)에서 색 픽셀로 그린다. 색 이모지 글꼴이 없는 기기(CI)는 건너뛴다
#[test]
fn emoji_paints_color_pixels_across_two_cells() {
    let Some(mut f) = fonts(2.0) else { return };
    if !f.has_color_emoji() {
        eprintln!("건너뜀: 색 이모지 글꼴이 없다");
        return;
    }
    let fr = render(&mut f, "😀 a".as_bytes(), 6, 1, false);
    // 바탕과 멀고 채도가 높은(노랑 얼굴) 픽셀
    let mut colorful = 0;
    for y in 0..fr.ch {
        for x in 0..fr.cw * 2 {
            let p = Rgb::from_u32(fr.buf[(y * fr.w + x) as usize]);
            let (mx, mn) = (p.r.max(p.g).max(p.b) as i32, p.r.min(p.g).min(p.b) as i32);
            if mx - mn > 80 && mx > 150 {
                colorful += 1;
            }
        }
    }
    let cells = (fr.cw * fr.ch * 2) as usize;
    assert!(colorful > cells / 10, "색 픽셀 {colorful}개(두 칸 {cells}픽셀)");
    // 셋째 칸 너머(공백 뒤 a)는 이모지가 덮지 않는다
    assert_eq!(ink(&fr, 2, 0, 1, Theme::default().background), 0, "공백 칸은 비어 있다");
}

/// #37: Claude Code가 쓰는 ⏺(U+23FA)은 한 칸 글자다. 색 이모지가 아니라 글자색을 따르는 윤곽 글꼴에서 그린다
#[test]
fn record_symbol_used_by_claude_code_is_a_monochrome_glyph() {
    let Some(mut f) = fonts(2.0) else { return };
    let Some(g) = f.glyph('\u{23fa}', bt_render::FontStyle::Regular) else {
        // macOS에는 STIX Two Math가 늘 있다
        if cfg!(target_os = "macos") {
            panic!("macOS인데 ⏺를 그릴 글꼴을 찾지 못했다");
        }
        eprintln!("건너뜀: ⏺를 가진 기호 글꼴이 없다");
        return;
    };
    assert!(g.color.is_none(), "글자색을 따르는 흑백 글리프여야 한다");
    assert!(g.width > 0);
}

/// #67: U+FE0F가 붙은 ❤️는 그 칸(VT 엔진이 정한 한 칸) 안에 색으로 그린다. 옆 칸은 덮지 않는다
#[test]
fn variation_selector_16_draws_a_color_emoji_inside_its_cell() {
    let Some(mut f) = fonts(2.0) else { return };
    if !f.has_color_emoji() {
        eprintln!("건너뜀: 색 이모지 글꼴이 없다");
        return;
    }
    let fr = render(&mut f, "\u{2764}\u{fe0f} a".as_bytes(), 6, 1, false);
    let mut red = 0;
    for y in 0..fr.ch {
        for x in 0..fr.cw {
            let p = Rgb::from_u32(fr.buf[(y * fr.w + x) as usize]);
            if p.r > 180 && p.g < 90 && p.b < 110 {
                red += 1;
            }
        }
    }
    assert!(red > (fr.cw * fr.ch / 20) as usize, "빨간 하트 픽셀 {red}개");
    assert_eq!(
        ink(&fr, 1, 0, 1, Theme::default().background),
        0,
        "옆 칸(공백)은 비어 있다"
    );
}

/// #87: 피부색 👍🏽(VT 엔진에서 두 칸씩 네 칸)과 ZWJ 가족 👨‍👩‍👧(여섯 칸)을 한 글리프로, 묶음 칸 가운데에 그린다.
/// 글자마다 그리면 맨 앞·맨 뒤 칸에도 그림이 있다
#[test]
fn emoji_clusters_draw_as_one_glyph_centered() {
    let Some(mut f) = fonts(2.0) else { return };
    if !f.has_color_emoji() {
        eprintln!("건너뜀: 색 이모지 글꼴이 없다");
        return;
    }
    let bg = Theme::default().background;
    let fr = render(&mut f, "\u{1f44d}\u{1f3fd}".as_bytes(), 10, 1, false);
    assert_eq!(
        ink(&fr, 0, 0, 1, bg) + ink(&fr, 3, 0, 1, bg),
        0,
        "맨 앞·맨 뒤 칸은 비고"
    );
    assert!(ink(&fr, 1, 0, 2, bg) > 100, "가운데 두 칸에 한 글리프");
    let fr = render(
        &mut f,
        "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}".as_bytes(),
        10,
        1,
        false,
    );
    assert_eq!(
        ink(&fr, 0, 0, 1, bg) + ink(&fr, 5, 0, 1, bg),
        0,
        "가족 여섯 칸의 맨 앞·맨 뒤는 빔"
    );
    assert!(ink(&fr, 2, 0, 2, bg) > 100);
}

/// 셀 (col,row) 안에서 두 그림의 픽셀이 다른 수
fn cell_diff(a: &Frame, b: &Frame, col: u32, row: u32) -> usize {
    let mut n = 0;
    for y in row * a.ch..(row + 1) * a.ch {
        for x in col * a.cw..(col + 1) * a.cw {
            if a.buf[(y * a.w + x) as usize] != b.buf[(y * b.w + x) as usize] {
                n += 1;
            }
        }
    }
    n
}

/// "->"를 한 줄에 그린 것과, "-"만·">"만 따로 그린 칸이 얼마나 다른가(합자가 없으면 0)
fn ligature_diff(f: &mut Fonts, text: &str) -> usize {
    let both = render(f, text.as_bytes(), 6, 1, false);
    let mut chars = text.chars();
    let (a, b) = (chars.next().unwrap(), chars.next().unwrap());
    let left = render(f, format!("{a}").as_bytes(), 6, 1, false);
    let right = render(f, format!(" {b}").as_bytes(), 6, 1, false);
    cell_diff(&both, &left, 0, 0) + cell_diff(&both, &right, 1, 0)
}

/// #96: JetBrains Mono의 합자(calt). "->"는 글자마다 그린 것과 다른 글리프가 된다. 칸 폭은 그대로(글리프도 칸마다 하나)
#[test]
fn ligatures_replace_glyphs_like_ghostty() {
    let Some(mut f) = fonts(2.0) else { return };
    if f.family_name != "JetBrains Mono" {
        return;
    }
    assert!(ligature_diff(&mut f, "->") > 20, "-> 합자");
    assert!(ligature_diff(&mut f, "!=") > 20, "!= 합자");
    assert_eq!(ligature_diff(&mut f, "ab"), 0, "글자에는 합자가 없다");
}

/// #96: `font-feature = -calt`면 합자를 끈다
#[test]
fn ligatures_turn_off_with_minus_calt() {
    let Ok(mut f) = Fonts::new(&FontConfig {
        scale: 2.0,
        features: vec![(*b"calt", 0)],
        ..FontConfig::default()
    }) else {
        return;
    };
    if f.family_name != "JetBrains Mono" {
        return;
    }
    assert_eq!(ligature_diff(&mut f, "->"), 0);
    assert_eq!(ligature_diff(&mut f, "!="), 0);
}

/// #96: 커서 칸은 합자를 끊는다(블록 커서가 덮는 칸의 글자가 반쪽 합자로 보이지 않게)
#[test]
fn cursor_breaks_ligature() {
    let Some(mut f) = fonts(2.0) else { return };
    if f.family_name != "JetBrains Mono" {
        return;
    }
    // "->" 뒤로 커서를 왼쪽으로 한 칸 옮겨 ">" 위에 둔다
    let with_cursor = render(&mut f, b"->\x1b[D", 6, 1, true);
    let plain = render(&mut f, b"-", 6, 1, false);
    assert_eq!(cell_diff(&with_cursor, &plain, 0, 0), 0, "앞 칸은 합자가 아닌 '-'");
}
