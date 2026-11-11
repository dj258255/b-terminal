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
            underline: {
                use bt_render::Underline;
                let f = c.flags;
                if f.contains(CellFlags::UNDERCURL) {
                    Underline::Curly
                } else if f.contains(CellFlags::DOUBLE_UNDERLINE) {
                    Underline::Double
                } else if f.contains(CellFlags::DOTTED_UNDERLINE) {
                    Underline::Dotted
                } else if f.contains(CellFlags::DASHED_UNDERLINE) {
                    Underline::Dashed
                } else if f.contains(CellFlags::UNDERLINE) {
                    Underline::Single
                } else {
                    Underline::None
                }
            },
            strike: c.flags.contains(CellFlags::STRIKE),
            wide: c.flags.contains(CellFlags::WIDE),
            spacer: c.flags.contains(CellFlags::SPACER),
            selected: c.flags.contains(CellFlags::SELECTED),
            emoji: c.combining.contains(&'\u{fe0f}'),
            cluster: None,
            cluster_cells: 0,
            overline: c.flags.contains(CellFlags::OVERLINE),
            underline_color: c.underline_color,
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
            opacity: 1.0,
        }),
        selection_bg: t.selection_background,
        selection_fg: t.selection_foreground,
        preedit: None,
        min_contrast: 1.0,
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
            opacity: 1.0,
        }),
        selection_bg: t.selection_background,
        selection_fg: t.selection_foreground,
        preedit: None,
        min_contrast: 1.0,
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

/// #180: 글자 묶음 폭(기본)에서 ❤️는 VT 엔진이 두 칸으로 정하고, 색 하트가 그 두 칸 안에 들어간다
#[test]
fn variation_selector_16_fills_two_cells_with_cluster_width() {
    let Some(mut f) = fonts(2.0) else { return };
    if !f.has_color_emoji() {
        eprintln!("건너뜀: 색 이모지 글꼴이 없다");
        return;
    }
    let fr = render(&mut f, "\u{2764}\u{fe0f} a".as_bytes(), 6, 1, false);
    let bg = Theme::default().background;
    assert!(ink(&fr, 0, 0, 2, bg) > 100, "하트가 두 칸에");
    assert_eq!(ink(&fr, 2, 0, 1, bg), 0, "하트 뒤 공백 칸은 비어 있다");
    assert!(ink(&fr, 3, 0, 1, bg) > 0, "a는 네 번째 칸");
}

/// #67: U+FE0F가 붙은 ❤️는 그 칸(글자별 폭에서는 VT 엔진이 정한 한 칸) 안에 색으로 그린다. 옆 칸은 덮지 않는다
#[test]
fn variation_selector_16_draws_a_color_emoji_inside_its_cell() {
    let Some(mut f) = fonts(2.0) else { return };
    if !f.has_color_emoji() {
        eprintln!("건너뜀: 색 이모지 글꼴이 없다");
        return;
    }
    let fr = render(&mut f, "\x1b[?2027l\u{2764}\u{fe0f} a".as_bytes(), 6, 1, false);
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

/// #87·#180: 피부색 👍🏽과 ZWJ 가족 👨‍👩‍👧을 한 글리프로, 묶음 칸 가운데에 그린다.
/// 글자 묶음 폭(mode 2027, 기본)에서는 엔진이 묶음을 두 칸에 담고, 끄면 글자마다 칸을 차지한다(네 칸·여섯 칸).
/// 글자마다 그리면 묶음 칸 밖에도 그림이 있다
#[test]
fn emoji_clusters_draw_as_one_glyph_centered() {
    let Some(mut f) = fonts(2.0) else { return };
    if !f.has_color_emoji() {
        eprintln!("건너뜀: 색 이모지 글꼴이 없다");
        return;
    }
    let bg = Theme::default().background;
    let tone = "\u{1f44d}\u{1f3fd}";
    let family = "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}";
    // 묶음 폭(기본): 둘 다 두 칸에 한 글리프, 그 뒤 칸은 빔
    for text in [tone, family] {
        let fr = render(&mut f, text.as_bytes(), 10, 1, false);
        assert!(ink(&fr, 0, 0, 2, bg) > 100, "두 칸에 한 글리프");
        assert_eq!(ink(&fr, 2, 0, 8, bg), 0, "묶음 뒤 칸은 빔");
    }
    // 글자별 폭(mode 2027 끔): 피부색 네 칸, 가족 여섯 칸의 가운데에
    let fr = render(&mut f, format!("\x1b[?2027l{tone}").as_bytes(), 10, 1, false);
    assert_eq!(
        ink(&fr, 0, 0, 1, bg) + ink(&fr, 3, 0, 1, bg),
        0,
        "맨 앞·맨 뒤 칸은 비고"
    );
    assert!(ink(&fr, 1, 0, 2, bg) > 100, "가운데 두 칸에 한 글리프");
    let fr = render(&mut f, format!("\x1b[?2027l{family}").as_bytes(), 10, 1, false);
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

/// #112: 탭 막대·찾기 막대의 글(`draw_text`)에 색 이모지가 있으면 죽지 않고 색으로 그린다.
/// 탭 제목은 프로그램이 OSC 2로 정하므로, 이것이 죽으면 출력 한 줄로 터미널이 꺼진다
#[test]
fn draw_text_paints_color_emoji_without_panicking() {
    let Some(mut f) = fonts(1.0) else { return };
    if !f.has_color_emoji() {
        eprintln!("건너뜀: 색 이모지 글꼴 없음");
        return;
    }
    let m = f.metrics();
    let (w, h) = (m.width * 8, m.height);
    let mut buf = vec![0u32; (w * h) as usize];
    let mut c = Canvas::new(&mut buf, w, h);
    c.clear(Rgb::new(0, 0, 0));
    let drawn = bt_render::draw_text(&mut c, &mut f, 0, 0, "😀🔔 a", Rgb::new(255, 255, 255), false);
    assert_eq!(drawn, (m.width * 6) as i32, "두 칸 이모지 둘과 공백·글자");
    let colored = (0..h)
        .flat_map(|y| (0..m.width * 4).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let p = c.pixel(x, y);
            p.r.max(p.g).max(p.b) - p.r.min(p.g).min(p.b) > 60
        })
        .count();
    assert!(colored > 20, "이모지 칸에 색 픽셀: {colored}");
}

/// #112: 흑백 그리기에 색 글리프(bitmap이 빔)가 잘못 들어와도 죽지 않는다
#[test]
fn draw_glyph_ignores_glyph_without_coverage_bitmap() {
    let mut buf = vec![0u32; 16 * 16];
    let mut c = Canvas::new(&mut buf, 16, 16);
    let g = bt_render::font::Glyph {
        width: 4,
        height: 4,
        left: 0,
        top: 4,
        bitmap: Vec::new(),
        color: Some(vec![255; 4 * 4 * 4]),
    };
    c.draw_glyph(2, 8, &g, Rgb::new(255, 255, 255));
    assert_eq!(c.pixel(3, 5), Rgb::new(0, 0, 0), "그리지 않는다");
}

/// #123: 어두운 바탕의 짙은 파랑 글자는 minimum-contrast가 3이면 흰색으로 그린다(Ghostty와 같은 규칙)
#[test]
fn minimum_contrast_turns_unreadable_text_white() {
    let Some(mut f) = fonts(1.0) else { return };
    let brightest = |f: &mut Fonts, min: f32| {
        let mut e = Emulator::new(GridSize::new(4, 1), Theme::default());
        e.feed(b"\x1b[38;2;0;0;128m\x1b[48;2;30;30;46mHH");
        let s = e.snapshot();
        let dc = cells(&s);
        let m = f.metrics();
        let (w, h) = (m.width * 4, m.height);
        let mut buf = vec![0u32; (w * h) as usize];
        let t = Theme::default();
        let view = GridView {
            cols: s.cols,
            rows: s.rows,
            cells: &dc,
            cursor: None,
            selection_bg: t.selection_background,
            selection_fg: t.selection_foreground,
            preedit: None,
            min_contrast: min,
        };
        let mut c = Canvas::new(&mut buf, w, h);
        draw_grid(&mut c, f, 0, 0, &view);
        (0..h)
            .flat_map(|y| (0..m.width * 2).map(move |x| (x, y)))
            .map(|(x, y)| {
                let p = c.pixel(x, y);
                p.r as u32 + p.g as u32 + p.b as u32
            })
            .max()
            .unwrap_or(0)
    };
    let (off, on) = (brightest(&mut f, 1.0), brightest(&mut f, 3.0));
    assert!(off <= 128 + 3 * 46, "끄면 짙은 파랑 그대로: {off}");
    assert!(on > 600, "켜면 흰 글자: {on}");
}

/// #135: cursor-opacity가 0.5면 블록 커서가 칸 바탕과 커서 색의 중간으로 칠해진다
#[test]
fn half_transparent_block_cursor_mixes_with_the_cell() {
    let Some(mut f) = fonts(1.0) else { return };
    let mut e = Emulator::new(GridSize::new(2, 1), Theme::default());
    e.feed(b"\x1b[48;2;0;0;0m ");
    let s = e.snapshot();
    let dc = cells(&s);
    let m = f.metrics();
    let (w, h) = (m.width * 2, m.height);
    let paint = |f: &mut Fonts, opacity: f32| {
        let mut buf = vec![0u32; (w * h) as usize];
        let t = Theme::default();
        let view = GridView {
            cols: s.cols,
            rows: s.rows,
            cells: &dc,
            cursor: Some(DrawCursor {
                col: 0,
                row: 0,
                style: CursorStyle::Block,
                wide: false,
                color: Rgb::new(200, 100, 0),
                text_color: t.cursor_text,
                opacity,
            }),
            selection_bg: t.selection_background,
            selection_fg: t.selection_foreground,
            preedit: None,
            min_contrast: 1.0,
        };
        let mut c = Canvas::new(&mut buf, w, h);
        draw_grid(&mut c, f, 0, 0, &view);
        c.pixel(1, 1)
    };
    assert_eq!(paint(&mut f, 1.0), Rgb::new(200, 100, 0));
    let half = paint(&mut f, 0.5);
    assert!(
        (half.r as i32 - 100).abs() <= 2 && (half.g as i32 - 50).abs() <= 2,
        "{half:?}"
    );
}

/// 주어진 테마로 바이트를 그린다. `edit`로 칸을 고친 뒤(찾기 강조 등) 그린다
fn paint_theme(f: &mut Fonts, theme: &Theme, bytes: &[u8], edit: impl FnOnce(&mut Vec<DrawCell>)) -> Frame {
    let mut e = Emulator::new(GridSize::new(4, 1), theme.clone());
    e.feed(bytes);
    let s = e.snapshot();
    let mut dc = cells(&s);
    edit(&mut dc);
    let m = f.metrics();
    let (w, h) = (m.width * 4, m.height);
    let mut buf = vec![0u32; (w * h) as usize];
    let view = GridView {
        cols: s.cols,
        rows: s.rows,
        cells: &dc,
        cursor: None,
        selection_bg: theme.selection_background,
        selection_fg: theme.selection_foreground,
        preedit: None,
        min_contrast: 1.0,
    };
    let mut c = Canvas::new(&mut buf, w, h);
    c.clear(s.background);
    draw_grid(&mut c, f, 0, 0, &view);
    Frame {
        buf,
        w,
        h,
        cw: m.width,
        ch: m.height,
    }
}

/// 첫 두 칸에서 가장 밝은 픽셀의 밝기 합
fn brightest(fr: &Frame) -> u32 {
    (0..fr.ch)
        .flat_map(|y| (0..fr.cw * 2).map(move |x| (x, y)))
        .map(|(x, y)| {
            let p = Rgb::from_u32(fr.buf[(y * fr.w + x) as usize]);
            p.r as u32 + p.g as u32 + p.b as u32
        })
        .max()
        .unwrap_or(0)
}

/// #162: faint-opacity가 클수록 흐린(SGR 2) 글자가 또렷하고, 0이면 바탕에 묻힌다
#[test]
fn faint_opacity_controls_how_visible_faint_text_is() {
    let Some(mut f) = fonts(1.0) else { return };
    let at = |f: &mut Fonts, o: f32| {
        let t = Theme {
            faint_opacity: o,
            ..Theme::default()
        };
        brightest(&paint_theme(f, &t, b"\x1b[2mHH", |_| {}))
    };
    let (full, half, none) = (at(&mut f, 1.0), at(&mut f, 0.5), at(&mut f, 0.0));
    let bg = Theme::default().background;
    assert!(full > 600, "불투명 1: 흰 글자 그대로 {full}");
    assert!(full > half && half > none, "{full} > {half} > {none}");
    assert!(
        none <= bg.r as u32 + bg.g as u32 + bg.b as u32 + 3,
        "0이면 바탕과 같아 보이지 않는다: {none}"
    );
}

/// #162: bold-color가 있으면 굵은 기본 글자는 그 색, 없으면 흰색 그대로
#[test]
fn bold_color_paints_bold_text_with_that_color() {
    let Some(mut f) = fonts(1.0) else { return };
    let orange = |fr: &Frame| {
        (0..fr.ch)
            .flat_map(|y| (0..fr.cw * 2).map(move |x| (x, y)))
            .any(|(x, y)| {
                let p = Rgb::from_u32(fr.buf[(y * fr.w + x) as usize]);
                p.r > 200 && p.g > 100 && p.g < 160 && p.b < 40
            })
    };
    let plain = paint_theme(&mut f, &Theme::default(), b"\x1b[1mHH", |_| {});
    assert!(!orange(&plain), "bold-color 없이는 흰 글자");
    let t = Theme {
        bold_color: Some(Rgb::new(0xff, 0x80, 0)),
        ..Theme::default()
    };
    let colored = paint_theme(&mut f, &t, b"\x1b[1mHH", |_| {});
    assert!(orange(&colored), "굵은 글자가 주황");
}

/// #162: 찾기에 맞은 칸의 바탕은 search-background / search-selected-background로 칠해진다
#[test]
fn search_match_background_is_painted_with_search_colors() {
    let Some(mut f) = fonts(1.0) else { return };
    let t = Theme::default();
    let fr = paint_theme(&mut f, &t, b"HHHH", |dc| {
        for (i, current) in [(0usize, false), (1, true)] {
            let (fg, bg) = t.search.pick(current, dc[i].fg, dc[i].bg);
            dc[i].fg = fg;
            dc[i].bg = bg;
        }
    });
    let corner = |col: u32| Rgb::from_u32(fr.buf[(col * fr.cw) as usize]);
    assert_eq!(corner(0), Rgb::new(0xff, 0xe0, 0x82), "맞은 곳");
    assert_eq!(corner(1), Rgb::new(0xf2, 0xa5, 0x7e), "지금 고른 곳");
    assert_eq!(corner(2), t.background, "맞지 않은 칸");
}

// ---- #163: adjust-underline-*/strikethrough-*/overline-*/cursor-*/font-baseline/box-thickness, font-variation ----

use bt_core::config::Adjust;
use bt_render::MetricAdjust;

fn metrics_of(adjust: MetricAdjust, scale: f32) -> bt_render::CellMetrics {
    Fonts::new(&FontConfig {
        scale,
        adjust,
        ..FontConfig::default()
    })
    .unwrap()
    .metrics()
}

#[test]
fn underline_position_and_thickness_adjust() {
    let base = metrics_of(MetricAdjust::default(), 2.0);
    let m = |a: MetricAdjust| metrics_of(a, 2.0);
    // 위치: 양수는 아래로, 음수는 위로(셀 위에서 잰다). 포인트는 화면 배율을 곱한다
    let up = m(MetricAdjust {
        underline_position: Some(Adjust::Points(-1.0)),
        ..Default::default()
    });
    assert_eq!(up.underline_offset, base.underline_offset - 2);
    // 셀 밖으로 나가지 않는다
    let far = m(MetricAdjust {
        underline_position: Some(Adjust::Points(100.0)),
        ..Default::default()
    });
    assert_eq!(far.underline_offset, far.height - far.underline_thickness);
    // 두께: 퍼센트·포인트, 1픽셀 밑으로는 내려가지 않는다
    let thick = m(MetricAdjust {
        underline_thickness: Some(Adjust::Percent(200.0)),
        ..Default::default()
    });
    assert_eq!(thick.underline_thickness, base.underline_thickness * 3);
    let plus = m(MetricAdjust {
        underline_thickness: Some(Adjust::Points(1.0)),
        ..Default::default()
    });
    assert_eq!(plus.underline_thickness, base.underline_thickness + 2);
    let none = m(MetricAdjust {
        underline_thickness: Some(Adjust::Points(-50.0)),
        ..Default::default()
    });
    assert_eq!(none.underline_thickness, 1);
    // 다른 값은 그대로
    assert_eq!(plus.strike_thickness, base.strike_thickness);
    assert_eq!(plus.stroke, base.stroke);
}

#[test]
fn strikethrough_and_overline_adjust() {
    let base = metrics_of(MetricAdjust::default(), 1.0);
    let m = metrics_of(
        MetricAdjust {
            strikethrough_position: Some(Adjust::Points(2.0)),
            strikethrough_thickness: Some(Adjust::Points(1.0)),
            overline_position: Some(Adjust::Points(3.0)),
            overline_thickness: Some(Adjust::Percent(200.0)),
            ..Default::default()
        },
        1.0,
    );
    assert_eq!(m.strike_offset, base.strike_offset + 2);
    assert_eq!(m.strike_thickness, base.strike_thickness + 1);
    assert_eq!(m.overline_offset, base.overline_offset + 3);
    assert_eq!(m.overline_thickness, base.overline_thickness * 3);
    assert_eq!(m.underline_offset, base.underline_offset);
}

#[test]
fn cursor_baseline_and_box_adjust() {
    let base = metrics_of(MetricAdjust::default(), 1.0);
    let m = metrics_of(
        MetricAdjust {
            cursor_height: Some(Adjust::Points(-4.0)),
            cursor_thickness: Some(Adjust::Points(2.0)),
            box_thickness: Some(Adjust::Points(1.0)),
            baseline: Some(Adjust::Points(2.0)),
            ..Default::default()
        },
        1.0,
    );
    assert_eq!(m.cursor_height, base.cursor_height - 4);
    assert_eq!(m.cursor_beam, base.cursor_beam + 2);
    assert_eq!(m.cursor_hollow, base.cursor_hollow + 2);
    assert_eq!(m.box_thickness, base.box_thickness + 1);
    // 기준선은 셀 아래에서 잰 거리: 늘리면 글자가 위로(셀 위에서 잰 값은 줄어든다)
    assert_eq!(m.baseline, base.baseline - 2);
    assert_eq!((m.width, m.height), (base.width, base.height));
    // 퍼센트 커서 높이는 셀 높이 기준
    let half = metrics_of(
        MetricAdjust {
            cursor_height: Some(Adjust::Percent(-50.0)),
            ..Default::default()
        },
        1.0,
    );
    assert_eq!(half.cursor_height, (base.height as f32 * 0.5).round() as u32);
}

fn blank(ch: char) -> DrawCell {
    DrawCell {
        ch,
        fg: Rgb::new(255, 255, 255),
        bg: Rgb::new(0, 0, 0),
        bold: false,
        italic: false,
        underline: bt_render::Underline::None,
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

/// 셀 한 줄을 그린다(바탕 검정, 글자 흰색). 픽셀과 크기를 돌려준다
fn draw_row(f: &mut Fonts, cells: &[DrawCell], cursor: Option<DrawCursor>) -> (Vec<u32>, u32, u32) {
    let m = f.metrics();
    let (w, h) = (m.width * cells.len() as u32, m.height);
    let mut buf = vec![0u32; (w * h) as usize];
    let t = Theme::default();
    let view = GridView {
        cols: cells.len(),
        rows: 1,
        cells,
        cursor,
        selection_bg: t.selection_background,
        selection_fg: t.selection_foreground,
        preedit: None,
        min_contrast: 1.0,
    };
    let mut c = Canvas::new(&mut buf, w, h);
    c.clear(Rgb::new(0, 0, 0));
    draw_grid(&mut c, f, 0, 0, &view);
    (buf, w, h)
}

/// 한 열에서 밝은(흰색에 가까운) 픽셀이 있는 줄들
fn lit_rows(buf: &[u32], w: u32, h: u32, x: u32) -> Vec<u32> {
    (0..h)
        .filter(|&y| Rgb::from_u32(buf[(y * w + x) as usize]).r > 200)
        .collect()
}

fn fonts_with(adjust: MetricAdjust) -> Fonts {
    Fonts::new(&FontConfig {
        adjust,
        ..FontConfig::default()
    })
    .unwrap()
}

#[test]
fn underline_is_drawn_at_adjusted_row_and_thickness() {
    let mut cell = blank(' ');
    cell.underline = bt_render::Underline::Single;
    let mut f = fonts_with(MetricAdjust::default());
    let m0 = f.metrics();
    let (b, w, h) = draw_row(&mut f, &[cell.clone()], None);
    let base = lit_rows(&b, w, h, 0);
    assert_eq!(base.first().copied(), Some(m0.underline_offset));
    assert_eq!(base.len() as u32, m0.underline_thickness);

    let mut f = fonts_with(MetricAdjust {
        underline_position: Some(Adjust::Points(-2.0)),
        underline_thickness: Some(Adjust::Points(2.0)),
        ..Default::default()
    });
    let (b, w, h) = draw_row(&mut f, &[cell], None);
    let rows = lit_rows(&b, w, h, 0);
    assert_eq!(rows.first().copied(), Some(m0.underline_offset - 2));
    assert_eq!(rows.len() as u32, m0.underline_thickness + 2);
}

#[test]
fn strikethrough_is_drawn_at_adjusted_row_and_thickness() {
    let mut cell = blank(' ');
    cell.strike = true;
    let mut f = fonts_with(MetricAdjust::default());
    let m0 = f.metrics();
    let (b, w, h) = draw_row(&mut f, &[cell.clone()], None);
    assert_eq!(lit_rows(&b, w, h, 0).first().copied(), Some(m0.strike_offset));

    let mut f = fonts_with(MetricAdjust {
        strikethrough_position: Some(Adjust::Points(-3.0)),
        strikethrough_thickness: Some(Adjust::Points(2.0)),
        ..Default::default()
    });
    let (b, w, h) = draw_row(&mut f, &[cell], None);
    let rows = lit_rows(&b, w, h, 0);
    assert_eq!(rows.first().copied(), Some(m0.strike_offset - 3));
    assert_eq!(rows.len() as u32, m0.strike_thickness + 2);
}

/// #277: 윗줄은 칸 맨 위(`overline_offset`)에 `overline_thickness` 두께로 글자색 줄을 긋는다
#[test]
fn overline_is_drawn_at_adjusted_row_and_thickness() {
    let mut cell = blank(' ');
    cell.overline = true;
    let plain = blank(' ');
    let mut f = fonts_with(MetricAdjust::default());
    let m0 = f.metrics();
    let (b, w, h) = draw_row(&mut f, &[cell.clone()], None);
    let base = lit_rows(&b, w, h, 0);
    assert_eq!(base.first().copied(), Some(m0.overline_offset));
    assert_eq!(base.len() as u32, m0.overline_thickness);
    // 켜지 않은 칸에는 줄이 없다
    let (b, w, h) = draw_row(&mut f, &[plain], None);
    assert!(lit_rows(&b, w, h, 0).is_empty());

    let mut f = fonts_with(MetricAdjust {
        overline_position: Some(Adjust::Points(2.0)),
        overline_thickness: Some(Adjust::Points(2.0)),
        ..Default::default()
    });
    let (b, w, h) = draw_row(&mut f, &[cell], None);
    let rows = lit_rows(&b, w, h, 0);
    assert_eq!(rows.first().copied(), Some(m0.overline_offset + 2));
    assert_eq!(rows.len() as u32, m0.overline_thickness + 2);
}

/// #277: 윗줄 색은 글자색이다(밑줄·가운데줄과 같다). 두 칸 글자는 두 칸 폭으로 긋는다
#[test]
fn overline_uses_the_cell_foreground_and_spans_wide_cells() {
    let red = Rgb::new(250, 10, 10);
    let mut cell = blank(' ');
    cell.overline = true;
    cell.fg = red;
    let mut f = fonts_with(MetricAdjust::default());
    let m = f.metrics();
    let (b, w, _h) = draw_row(&mut f, &[cell.clone()], None);
    assert_eq!(Rgb::from_u32(b[(m.overline_offset * w) as usize]), red);
    let mut wide = cell;
    wide.wide = true;
    let mut spacer = blank(' ');
    spacer.spacer = true;
    let (b, w, _h) = draw_row(&mut f, &[wide, spacer], None);
    assert_eq!(
        Rgb::from_u32(b[(m.overline_offset * w + 2 * m.width - 1) as usize]),
        red
    );
}

fn cursor_at(style: CursorStyle) -> DrawCursor {
    DrawCursor {
        col: 0,
        row: 0,
        style,
        wide: false,
        color: Rgb::new(255, 255, 255),
        text_color: Rgb::new(0, 0, 0),
        opacity: 1.0,
    }
}

#[test]
fn cursor_height_and_thickness_adjust() {
    let cells = [blank(' '), blank(' ')];
    let mut f = fonts_with(MetricAdjust::default());
    let m0 = f.metrics();
    let (b, w, h) = draw_row(&mut f, &cells, Some(cursor_at(CursorStyle::Beam)));
    assert_eq!(lit_rows(&b, w, h, 0).len() as u32, m0.height, "기본은 셀 높이");

    let mut f = fonts_with(MetricAdjust {
        cursor_height: Some(Adjust::Points(-4.0)),
        cursor_thickness: Some(Adjust::Points(2.0)),
        ..Default::default()
    });
    let (b, w, h) = draw_row(&mut f, &cells, Some(cursor_at(CursorStyle::Beam)));
    let rows = lit_rows(&b, w, h, 0);
    assert_eq!(rows.len() as u32, m0.height - 4);
    assert_eq!(rows.first().copied(), Some(2), "셀 가운데에 둔다");
    // 막대 두께: 가운데 줄에서 밝은 칸 수
    let mid = h / 2;
    let thick = (0..w)
        .filter(|&x| Rgb::from_u32(b[(mid * w + x) as usize]).r > 200)
        .count() as u32;
    assert_eq!(thick, m0.cursor_beam + 2);

    // 블록 커서도 높이를 따른다
    let (b, w, h) = draw_row(&mut f, &cells, Some(cursor_at(CursorStyle::Block)));
    assert_eq!(lit_rows(&b, w, h, 0).len() as u32, m0.height - 4);
}

#[test]
fn box_thickness_adjusts_line_drawing() {
    let cells = [blank('─')];
    let mut f = fonts_with(MetricAdjust::default());
    let (b, w, h) = draw_row(&mut f, &cells, None);
    let base = lit_rows(&b, w, h, 0).len();
    let mut f = fonts_with(MetricAdjust {
        box_thickness: Some(Adjust::Points(2.0)),
        ..Default::default()
    });
    let (b, w, h) = draw_row(&mut f, &cells, None);
    assert_eq!(lit_rows(&b, w, h, 0).len(), base + 2);
}

#[test]
fn font_baseline_moves_glyphs_up() {
    let top_of = |f: &mut Fonts| {
        let (b, w, h) = draw_row(f, &[blank('x')], None);
        (0..h)
            .find(|&y| (0..w).any(|x| Rgb::from_u32(b[(y * w + x) as usize]).r > 100))
            .unwrap()
    };
    let mut f = fonts_with(MetricAdjust::default());
    let base = top_of(&mut f);
    let mut f = fonts_with(MetricAdjust {
        baseline: Some(Adjust::Points(2.0)),
        ..Default::default()
    });
    assert_eq!(top_of(&mut f), base - 2);
}

/// 가변 글꼴(wght 축이 있는 것)을 찾는다. 없으면 건너뛴다
fn variable_font() -> Option<(fontdb::Database, String)> {
    for path in [
        "/System/Library/Fonts/SFNSMono.ttf",
        "/System/Library/Fonts/SFNS.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    ] {
        let Ok(data) = std::fs::read(path) else { continue };
        let Some(font) = swash::FontRef::from_index(&data, 0) else {
            continue;
        };
        if font.variations().find_by_tag(swash::tag_from_bytes(b"wght")).is_none() {
            continue;
        }
        let mut db = fontdb::Database::new();
        db.load_font_data(data);
        let name = db.faces().next()?.families.first()?.0.clone();
        return Some((db, name));
    }
    eprintln!("건너뜀: 가변 글꼴이 없다");
    None
}

fn ink_sum(g: &bt_render::font::Glyph) -> u32 {
    g.bitmap.iter().map(|&v| v as u32).sum()
}

#[test]
fn font_variation_changes_glyph_weight_per_style() {
    let Some((db, name)) = variable_font() else { return };
    let make = |variation: Vec<([u8; 4], f32)>, bold: Vec<([u8; 4], f32)>| {
        Fonts::with_database(
            &db,
            &FontConfig {
                family: Some(name.clone()),
                variation,
                variation_bold: bold,
                ..FontConfig::default()
            },
        )
        .unwrap()
    };
    let mut thin = make(vec![(*b"wght", 100.0)], vec![]);
    let mut heavy = make(vec![(*b"wght", 800.0)], vec![]);
    let t = ink_sum(&thin.glyph('l', bt_render::FontStyle::Regular).unwrap());
    let h = ink_sum(&heavy.glyph('l', bt_render::FontStyle::Regular).unwrap());
    assert!(h > t * 3 / 2, "wght=800({h})이 wght=100({t})보다 확실히 굵어야 한다");

    // 굵은 꼴에만 쓰는 값: 보통 꼴은 그대로, 굵은 꼴만 바뀐다
    let mut only_bold = make(vec![(*b"wght", 100.0)], vec![(*b"wght", 800.0)]);
    let r = ink_sum(&only_bold.glyph('l', bt_render::FontStyle::Regular).unwrap());
    let b = ink_sum(&only_bold.glyph('l', bt_render::FontStyle::Bold).unwrap());
    assert_eq!(r, t);
    assert_eq!(b, h);
}

// ---- #177: background-opacity-cells ----

/// 색 바탕 칸(0번)과 기본 바탕 칸(1번)을 투명 창에 그린 첫 칸·둘째 칸의 왼쪽 위 픽셀
fn translucent_corners(f: &mut Fonts, cells_on: bool) -> (u32, u32) {
    let t = Theme::default();
    let mut e = Emulator::new(GridSize::new(4, 1), t.clone());
    e.feed(b"  ");
    let s = e.snapshot();
    let mut dc = cells(&s);
    dc[0].bg = Rgb::new(200, 100, 50);
    let m = f.metrics();
    let (w, h) = (m.width * 4, m.height);
    let mut buf = vec![0u32; (w * h) as usize];
    let view = GridView {
        cols: s.cols,
        rows: s.rows,
        cells: &dc,
        cursor: None,
        selection_bg: t.selection_background,
        selection_fg: t.selection_foreground,
        preedit: None,
        min_contrast: 1.0,
    };
    let mut c = Canvas::new(&mut buf, w, h);
    c.set_see_through(s.background, 128);
    c.set_cells_alpha(bt_render::cells_alpha(0.5, cells_on));
    c.clear(s.background);
    draw_grid(&mut c, f, 0, 0, &view);
    (buf[0], buf[m.width as usize])
}

#[test]
fn background_opacity_cells_makes_colored_cells_translucent() {
    let Some(mut f) = fonts(1.0) else { return };
    let (colored_off, plain_off) = translucent_corners(&mut f, false);
    assert_eq!(colored_off >> 24, 0xff, "옵션이 없으면 색 바탕 칸은 불투명");
    assert_eq!(plain_off >> 24, 0x80, "기본 바탕 칸은 늘 반투명");
    let (colored_on, plain_on) = translucent_corners(&mut f, true);
    assert_eq!(colored_on, 0x8064_3219, "옵션을 켜면 색 바탕 칸도 알파 128, 곱한 색");
    assert_eq!(plain_on, plain_off, "기본 바탕 칸은 달라지지 않는다");
}

// ---- #174: font-style*, font-codepoint-map, font-shaping-break, adjust-icon-height ----

use bt_render::{CodepointFont, FontStyle, FontStyleName};

/// 넣어 둔 JetBrains Mono가 기본 글꼴인 묶음(시스템 글꼴에 기대지 않는다)
fn jb(config: FontConfig) -> Option<Fonts> {
    let f = Fonts::new(&config).ok()?;
    (f.family_name == "JetBrains Mono").then_some(f)
}

fn bitmap(f: &mut Fonts, c: char, s: FontStyle) -> Option<(usize, usize, Vec<u8>)> {
    f.glyph(c, s).map(|g| (g.width, g.height, g.bitmap.clone()))
}

/// `font-shaping-break = no-cursor`면 커서 칸에서도 합자를 끊지 않는다(기본 `cursor`는 끊는다)
#[test]
fn no_cursor_shaping_break_keeps_ligature_under_cursor() {
    let make = |brk| {
        jb(FontConfig {
            scale: 2.0,
            shaping_break_cursor: brk,
            ..FontConfig::default()
        })
    };
    let (Some(mut keep), Some(mut broken)) = (make(false), make(true)) else {
        return;
    };
    let plain = render(&mut broken, b"-", 6, 1, false);
    // "->" 뒤에서 커서를 왼쪽으로 한 칸: 커서는 '>' 위
    let broken_frame = render(&mut broken, b"->\x1b[D", 6, 1, true);
    assert_eq!(
        cell_diff(&broken_frame, &plain, 0, 0),
        0,
        "기본: 앞 칸은 합자가 아닌 '-'"
    );
    let keep_frame = render(&mut keep, b"->\x1b[D", 6, 1, true);
    assert!(
        cell_diff(&keep_frame, &plain, 0, 0) > 20,
        "no-cursor: 앞 칸은 합자의 앞쪽 반이다"
    );
}

/// `font-style-bold = false`는 굵은 꼴을 끄고 보통 꼴로 그린다
#[test]
fn disabled_font_style_draws_the_regular_face() {
    let mut normal = jb(FontConfig::default()).unwrap();
    let mut off = jb(FontConfig {
        styles: [
            FontStyleName::Default,
            FontStyleName::Disabled,
            FontStyleName::Disabled,
            FontStyleName::Default,
        ],
        ..FontConfig::default()
    })
    .unwrap();
    let regular = bitmap(&mut normal, 'a', FontStyle::Regular);
    assert_ne!(
        bitmap(&mut normal, 'a', FontStyle::Bold),
        regular,
        "기본: 굵은 꼴은 다르다"
    );
    assert_eq!(bitmap(&mut off, 'a', FontStyle::Bold), regular);
    assert_eq!(bitmap(&mut off, 'a', FontStyle::Italic), regular);
    // 끄지 않은 꼴은 그대로
    assert_eq!(
        bitmap(&mut off, 'a', FontStyle::BoldItalic),
        bitmap(&mut normal, 'a', FontStyle::BoldItalic)
    );
}

/// `font-style-bold = Italic`은 글꼴이 알리는 스타일 이름이 Italic인 꼴을 굵은 꼴 자리에 쓴다.
/// 이름을 못 찾으면 알아서 고르던 꼴 그대로
#[test]
fn named_font_style_picks_the_face_by_its_style_name() {
    let mut normal = jb(FontConfig::default()).unwrap();
    let named = |name: &str| {
        jb(FontConfig {
            styles: [
                FontStyleName::Default,
                FontStyleName::Named(name.into()),
                FontStyleName::Default,
                FontStyleName::Default,
            ],
            ..FontConfig::default()
        })
        .unwrap()
    };
    let italic = bitmap(&mut normal, 'a', FontStyle::Italic);
    let bold = bitmap(&mut normal, 'a', FontStyle::Bold);
    assert_ne!(italic, bold);
    assert_eq!(bitmap(&mut named("Italic"), 'a', FontStyle::Bold), italic);
    assert_eq!(
        bitmap(&mut named("italic"), 'a', FontStyle::Bold),
        italic,
        "대소문자는 가리지 않는다"
    );
    assert_eq!(bitmap(&mut named("No Such Style"), 'a', FontStyle::Bold), bold);
}

fn jb_with_map(codepoint_map: Vec<CodepointFont>) -> Fonts {
    jb(FontConfig {
        codepoint_map,
        ..FontConfig::default()
    })
    .unwrap()
}

/// `font-codepoint-map`: 그 범위 글자는 정한 글꼴에서 찾고, 그 글꼴에 없으면 보통 차례로 찾는다
#[test]
fn codepoint_map_forces_a_font_for_a_range() {
    let map = |family: &str| {
        vec![CodepointFont {
            start: 'A' as u32,
            end: 'Z' as u32,
            family: family.into(),
        }]
    };
    let mut reference = jb(FontConfig::default()).unwrap();
    let want = bitmap(&mut reference, 'A', FontStyle::Regular).expect("JetBrains Mono에 A가 있다");
    // 기본 글꼴을 Nerd 기호 글꼴로 바꾸면 'A'는 다른 대체 글꼴(또는 없음)에서 나온다
    let symbols = |codepoint_map| {
        Fonts::new(&FontConfig {
            family: Some("Symbols Nerd Font Mono".into()),
            codepoint_map,
            ..FontConfig::default()
        })
        .unwrap()
    };
    assert_ne!(
        bitmap(&mut symbols(vec![]), 'A', FontStyle::Regular),
        Some(want.clone())
    );
    let mut mapped = symbols(map("JetBrains Mono"));
    assert_eq!(
        bitmap(&mut mapped, 'A', FontStyle::Regular),
        Some(want.clone()),
        "범위 안: 정한 글꼴"
    );
    assert!(
        !mapped.primary_has('A', FontStyle::Regular),
        "정한 글꼴로 그리는 글자는 합자로 묶지 않는다"
    );
    // 범위 밖은 그대로
    assert_eq!(
        bitmap(&mut mapped, 'a', FontStyle::Regular),
        bitmap(&mut symbols(vec![]), 'a', FontStyle::Regular)
    );
    // 정한 글꼴에 글자가 없거나 글꼴이 없으면 보통 차례로
    let plain = bitmap(&mut reference, 'A', FontStyle::Regular);
    assert_eq!(
        bitmap(&mut jb_with_map(map("Symbols Nerd Font Mono")), 'A', FontStyle::Regular),
        plain
    );
    assert_eq!(
        bitmap(&mut jb_with_map(map("No Such Font Anywhere")), 'A', FontStyle::Regular),
        plain
    );
    // 뒤에 쓴 것이 이긴다
    let mut later = map("Symbols Nerd Font Mono");
    later.extend(map("JetBrains Mono"));
    assert_eq!(bitmap(&mut symbols(later), 'A', FontStyle::Regular), Some(want));
}

/// `adjust-icon-height`: 없으면 셀 높이, 있으면 Nerd 아이콘의 높이를 그만큼으로. 파워라인 도형은 건드리지 않는다
#[test]
fn icon_height_scales_nerd_icons_but_not_powerline() {
    let make = |icon_height| {
        Fonts::new(&FontConfig {
            scale: 2.0,
            adjust: MetricAdjust {
                icon_height,
                ..MetricAdjust::default()
            },
            ..FontConfig::default()
        })
        .unwrap()
    };
    let mut base = make(None);
    let cell_h = base.metrics().height;
    assert_eq!(base.metrics().icon_height, cell_h);
    let mut zero = make(Some(Adjust::Percent(0.0)));
    let mut small = make(Some(Adjust::Percent(-40.0)));
    let mut big = make(Some(Adjust::Percent(50.0)));
    assert_eq!(small.metrics().icon_height, (cell_h as f32 * 0.6).round() as u32);
    let folder = '\u{f07c}';
    let b = base.glyph(folder, FontStyle::Regular).unwrap();
    let z = zero.glyph(folder, FontStyle::Regular).unwrap();
    assert_eq!(
        (z.width, z.height, &z.bitmap),
        (b.width, b.height, &b.bitmap),
        "0%는 보정 없음과 같다"
    );
    let s = small.glyph(folder, FontStyle::Regular).unwrap();
    assert!(s.height < b.height * 3 / 4, "-40%: {} -> {}", b.height, s.height);
    let g = big.glyph(folder, FontStyle::Regular).unwrap();
    assert!(g.height >= b.height, "+50%: {} -> {}", b.height, g.height);
    // 키울 때 칸 폭에서 멈춘다. 원래 이미 칸보다 넓은 아이콘은 더 넓어지지 않는다
    assert!(
        g.width as u32 <= big.metrics().width.max(b.width as u32),
        "칸 폭을 넘지 않는다: {} > {}",
        g.width,
        big.metrics().width.max(b.width as u32)
    );
    // 파워라인(U+E0B0) 도형은 그대로
    let p = base.glyph('\u{e0b0}', FontStyle::Regular).unwrap();
    let ps = small.glyph('\u{e0b0}', FontStyle::Regular).unwrap();
    assert_eq!((ps.width, ps.height, &ps.bitmap), (p.width, p.height, &p.bitmap));
}

// ---- #193: font-thicken, font-synthetic-style ----

use bt_render::SyntheticStyle;

/// 보통 꼴만 있는 JetBrains Mono(굵은·기울임 꼴이 없는 글꼴을 흉내 낸다). 시스템 글꼴을 읽지 않는다
fn regular_only(config: FontConfig) -> Fonts {
    let mut db = fontdb::Database::new();
    db.load_font_data(include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf").to_vec());
    let f = Fonts::with_database(&db, &config).unwrap();
    assert_eq!(f.family_name, "JetBrains Mono");
    f
}

fn glyph_ink(f: &mut Fonts, c: char, s: FontStyle) -> u32 {
    f.glyph(c, s).unwrap().bitmap.iter().map(|&b| b as u32).sum()
}

/// 글리프 윗반과 아랫반의 무게중심 가로 위치 차(기울어졌으면 커진다)
fn lean(f: &mut Fonts, c: char, s: FontStyle) -> f32 {
    let g = f.glyph(c, s).unwrap();
    let centroid = |rows: std::ops::Range<usize>| {
        let (mut sum, mut w) = (0f32, 0f32);
        for y in rows {
            for x in 0..g.width {
                let v = g.bitmap[y * g.width + x] as f32;
                sum += v * x as f32;
                w += v;
            }
        }
        sum / w
    };
    centroid(0..g.height / 2) - centroid(g.height / 2..g.height)
}

fn no_synth() -> SyntheticStyle {
    SyntheticStyle {
        bold: false,
        italic: false,
        bold_italic: false,
    }
}

/// 굵은 꼴이 없는 글꼴은 기본으로 굵게를 합성한다(윤곽을 부풀림). `no-bold`면 보통 꼴 그대로
#[test]
fn missing_bold_face_is_synthesized_by_default() {
    let cfg = |synthetic| FontConfig {
        scale: 2.0,
        synthetic,
        ..FontConfig::default()
    };
    let mut on = regular_only(cfg(SyntheticStyle::default()));
    let regular = glyph_ink(&mut on, 'a', FontStyle::Regular);
    let bold = glyph_ink(&mut on, 'a', FontStyle::Bold);
    assert!(
        bold * 100 < regular * 150,
        "너무 굵지 않다(넣어 둔 굵은 꼴은 보통의 1.27배): {bold} / {regular}"
    );
    assert!(
        bold * 100 > regular * 115,
        "합성 굵게는 보통 꼴보다 먹이 15% 이상 많다: {bold} / {regular}"
    );
    let mut off = regular_only(cfg(SyntheticStyle {
        bold: false,
        ..SyntheticStyle::default()
    }));
    assert_eq!(
        glyph_ink(&mut off, 'a', FontStyle::Bold),
        regular,
        "no-bold: 보통 꼴 그대로"
    );
    assert!(
        glyph_ink(&mut off, 'a', FontStyle::BoldItalic) > regular,
        "bold-italic은 따로 켜져 있다"
    );
}

/// 기울임 꼴이 없으면 글자를 기울인다. 끄면 보통 꼴
#[test]
fn missing_italic_face_is_slanted_by_default() {
    let cfg = |synthetic| FontConfig {
        scale: 2.0,
        synthetic,
        ..FontConfig::default()
    };
    let mut on = regular_only(cfg(SyntheticStyle::default()));
    let upright = lean(&mut on, 'H', FontStyle::Regular);
    let slanted = lean(&mut on, 'H', FontStyle::Italic);
    assert!(
        slanted > upright + 1.0,
        "윗반이 오른쪽으로 쏠린다: {slanted} / {upright}"
    );
    assert!(lean(&mut on, 'H', FontStyle::BoldItalic) > upright + 1.0);
    let mut off = regular_only(cfg(SyntheticStyle {
        italic: false,
        ..SyntheticStyle::default()
    }));
    assert_eq!(lean(&mut off, 'H', FontStyle::Italic), upright);
}

/// `font-synthetic-style = false`면 아무것도 합성하지 않고 모두 보통 꼴
#[test]
fn synthetic_style_false_keeps_regular_for_every_style() {
    let mut f = regular_only(FontConfig {
        scale: 2.0,
        synthetic: no_synth(),
        ..FontConfig::default()
    });
    let regular = bitmap(&mut f, 'a', FontStyle::Regular);
    for s in [FontStyle::Bold, FontStyle::Italic, FontStyle::BoldItalic] {
        assert_eq!(bitmap(&mut f, 'a', s), regular, "{s:?}");
    }
}

/// 글꼴에 그 꼴이 있으면 합성하지 않는다(설정과 상관없이 같은 글리프)
#[test]
fn native_faces_are_never_synthesized() {
    let make = |synthetic| {
        jb(FontConfig {
            synthetic,
            ..FontConfig::default()
        })
    };
    let (Some(mut on), Some(mut off)) = (make(SyntheticStyle::default()), make(no_synth())) else {
        return;
    };
    for s in [FontStyle::Bold, FontStyle::Italic, FontStyle::BoldItalic] {
        assert_eq!(bitmap(&mut on, 'a', s), bitmap(&mut off, 'a', s), "{s:?}");
    }
}

/// 합자로 다듬은 글리프(글리프 번호로 푼 것)도 같은 합성을 받는다
#[test]
fn synthetic_bold_applies_to_ligature_glyphs() {
    let mut f = regular_only(FontConfig {
        scale: 2.0,
        ..FontConfig::default()
    });
    let ids = f.shape_run("->", FontStyle::Bold);
    let mut sum = |style| -> u32 {
        ids.iter()
            .filter(|&&g| g != 0)
            .map(|&g| {
                f.glyph_by_id(g, style)
                    .map_or(0, |g| g.bitmap.iter().map(|&b| b as u32).sum::<u32>())
            })
            .sum()
    };
    let (bold, reg) = (sum(FontStyle::Bold), sum(FontStyle::Regular));
    assert!(reg > 0, "-> 합자 글리프가 있다");
    assert!(bold > reg, "{bold} / {reg}");
}

/// `font-thicken`은 기본 끔. 켜면 먹이 늘고, 강도가 클수록 더 늘어난다(0도 켠 것이라 끈 것보다는 두껍다)
#[test]
fn thicken_adds_ink_scaled_by_strength() {
    let make = |thicken, strength| {
        jb(FontConfig {
            scale: 2.0,
            thicken,
            thicken_strength: strength,
            ..FontConfig::default()
        })
    };
    let (Some(mut off), Some(mut light), Some(mut strong)) = (make(false, 255), make(true, 0), make(true, 255)) else {
        return;
    };
    let (a, b, c) = (
        glyph_ink(&mut off, 'a', FontStyle::Regular),
        glyph_ink(&mut light, 'a', FontStyle::Regular),
        glyph_ink(&mut strong, 'a', FontStyle::Regular),
    );
    assert!(a < b && b < c, "끔 {a} < 가장 약함 {b} < 가장 강함 {c}");
}

// ---- 밑줄 모양과 밑줄 색(#276) ----

fn ul_cell(kind: bt_render::Underline, color: Option<Rgb>) -> DrawCell {
    let mut c = blank(' ');
    c.underline = kind;
    c.underline_color = color;
    c
}

/// 밑줄 칸 하나를 그린 픽셀. 글자는 없다(공백)
fn ul_frame(kind: bt_render::Underline, color: Option<Rgb>) -> Option<(Vec<u32>, u32, u32, bt_render::CellMetrics)> {
    let mut f = fonts(1.0)?;
    let m = f.metrics();
    let (b, w, h) = draw_row(&mut f, &[ul_cell(kind, color)], None);
    Some((b, w, h, m))
}

fn lit(buf: &[u32], w: u32, x: u32, y: u32) -> bool {
    buf[(y * w + x) as usize] & 0x00ff_ffff != 0
}

fn ink_rows(buf: &[u32], w: u32, h: u32) -> Vec<u32> {
    (0..h).filter(|&y| (0..w).any(|x| lit(buf, w, x, y))).collect()
}

fn ink_cols_in_row(buf: &[u32], w: u32, y: u32) -> Vec<u32> {
    (0..w).filter(|&x| lit(buf, w, x, y)).collect()
}

#[test]
fn single_underline_is_one_solid_band() {
    use bt_render::Underline;
    let Some((b, w, h, m)) = ul_frame(Underline::Single, None) else {
        return;
    };
    let rows = ink_rows(&b, w, h);
    let top = m.underline_offset.min(h - m.underline_thickness);
    assert_eq!(rows, (top..top + m.underline_thickness).collect::<Vec<_>>());
    assert_eq!(ink_cols_in_row(&b, w, top).len() as u32, w, "칸 너비 전체");
}

#[test]
fn double_underline_is_two_bands_with_one_thickness_gap() {
    use bt_render::Underline;
    let Some((b, w, h, m)) = ul_frame(Underline::Double, None) else {
        return;
    };
    let t = m.underline_thickness;
    let top = m.underline_offset.min(h - 2 * t);
    let want: Vec<u32> = (top - t..top).chain(top + t..top + 2 * t).collect();
    assert_eq!(ink_rows(&b, w, h), want, "두 줄, 사이는 두께 하나");
    assert_eq!(ink_cols_in_row(&b, w, top - t).len() as u32, w);
    assert!(ink_cols_in_row(&b, w, top).is_empty(), "두 줄 사이는 비어 있다");
}

#[test]
fn dashed_underline_follows_special_zig_pattern() {
    use bt_render::Underline;
    let Some((b, w, h, m)) = ul_frame(Underline::Dashed, None) else {
        return;
    };
    let top = m.underline_offset.min(h - m.underline_thickness);
    let dash = w / 3 + 1;
    let want: Vec<u32> = (0..w).filter(|x| (x / dash) % 2 == 0).collect();
    assert_eq!(ink_cols_in_row(&b, w, top), want, "{dash}칸 켜고 {dash}칸 끄기");
    assert!(!want.is_empty() && (want.len() as u32) < w);
    assert_eq!(ink_rows(&b, w, h).len() as u32, m.underline_thickness);
}

#[test]
fn dotted_underline_has_gaps_and_stays_in_the_cell() {
    use bt_render::Underline;
    let Some((b, w, h, m)) = ul_frame(Underline::Dotted, None) else {
        return;
    };
    let rows = ink_rows(&b, w, h);
    assert!(!rows.is_empty(), "점이 있다");
    // 점 줄기의 가운데는 밑줄 윗변에서 두께의 절반 아래이고 반지름은 두께 × √½
    let t = m.underline_thickness;
    assert!(rows[0] + 1 >= m.underline_offset.saturating_sub(t));
    assert!(*rows.last().unwrap() < h);
    let mid = rows[rows.len() / 2];
    let cols = ink_cols_in_row(&b, w, mid);
    assert!(
        !cols.is_empty() && (cols.len() as u32) < w,
        "점 사이가 비어 있다: {cols:?}"
    );
    let groups = cols.windows(2).filter(|p| p[1] != p[0] + 1).count() + 1;
    assert!(groups >= 2, "점이 둘 이상: {cols:?}");
}

#[test]
fn curly_underline_rises_in_the_middle_and_dips_at_the_edges() {
    use bt_render::Underline;
    let Some((b, w, h, m)) = ul_frame(Underline::Curly, None) else {
        return;
    };
    let rows = ink_rows(&b, w, h);
    let t = m.underline_thickness;
    // 한 주기의 높낮이: 진폭은 칸 너비/π. 두께 줄 하나보다 분명히 높다
    assert!(rows.len() as u32 > t + 1, "물결의 높낮이: {rows:?}");
    let top_of = |x: u32| (0..h).find(|&y| lit(&b, w, x, y));
    let (edge, center) = (top_of(0).unwrap(), top_of(w / 2).unwrap());
    assert!(center < edge, "가운데({center})가 가장자리({edge})보다 위");
    assert!(*rows.last().unwrap() < h);
    // 부드러운 가장자리: 흰색도 검정도 아닌 중간 밝기 픽셀이 있다
    let mid = b.iter().any(|p| {
        let r = Rgb::from_u32(*p).r;
        r > 20 && r < 235
    });
    assert!(mid, "안티에일리어싱 픽셀");
}

#[test]
fn underline_color_overrides_the_text_color() {
    use bt_render::Underline;
    let red = Rgb::new(255, 0, 0);
    for kind in [Underline::Single, Underline::Double, Underline::Dashed] {
        let Some((b, w, h, _)) = ul_frame(kind, Some(red)) else {
            return;
        };
        let rows = ink_rows(&b, w, h);
        let y = rows[0];
        let x = ink_cols_in_row(&b, w, y)[0];
        assert_eq!(Rgb::from_u32(b[(y * w + x) as usize]), red, "{kind:?}");
        // 색이 없으면 글자색(흰색)
        let Some((b, w, _, _)) = ul_frame(kind, None) else {
            return;
        };
        assert_eq!(
            Rgb::from_u32(b[(y * w + x) as usize]),
            Rgb::new(255, 255, 255),
            "{kind:?}"
        );
    }
    for kind in [Underline::Curly, Underline::Dotted] {
        let Some((b, _, _, _)) = ul_frame(kind, Some(red)) else {
            return;
        };
        assert!(
            b.iter().all(|p| {
                let c = Rgb::from_u32(*p);
                c.g == 0 && c.b == 0
            }),
            "{kind:?}: 빨강과 검정의 섞음만 있어야 한다"
        );
        assert!(b.iter().any(|p| Rgb::from_u32(*p).r > 200), "{kind:?}");
    }
}

#[test]
fn underline_is_drawn_under_the_glyph() {
    use bt_render::Underline;
    // 가득 찬 블록(█)은 칸 전체를 글자색으로 칠한다. 밑줄이 글자 밑이면 밑줄 줄도 글자색이다
    let mut c = blank('\u{2588}');
    c.underline = Underline::Single;
    c.underline_color = Some(Rgb::new(255, 0, 0));
    let Some(mut f) = fonts(1.0) else {
        return;
    };
    let m = f.metrics();
    let (b, w, _) = draw_row(&mut f, &[c], None);
    let y = m.underline_offset;
    assert_eq!(Rgb::from_u32(b[(y * w) as usize]), Rgb::new(255, 255, 255));
}

#[test]
fn adjust_underline_thickness_applies_to_every_shape() {
    use bt_render::Underline;
    let adj = MetricAdjust {
        underline_thickness: Some(Adjust::Points(2.0)),
        ..Default::default()
    };
    for kind in [
        Underline::Single,
        Underline::Double,
        Underline::Dashed,
        Underline::Dotted,
        Underline::Curly,
    ] {
        let mut thin = fonts_with(MetricAdjust::default());
        let mut thick = fonts_with(adj);
        let c = [ul_cell(kind, None)];
        let (a, _, _) = draw_row(&mut thin, &c, None);
        let (b, _, _) = draw_row(&mut thick, &c, None);
        let ink = |buf: &[u32]| buf.iter().map(|p| Rgb::from_u32(*p).r as u64).sum::<u64>();
        assert!(
            ink(&b) > ink(&a),
            "{kind:?}: 두께를 키우면 잉크가 늘어난다 ({} > {})",
            ink(&b),
            ink(&a)
        );
    }
}

#[test]
fn sgr_4_variants_and_58_reach_the_screen_cells() {
    let mut e = Emulator::new(GridSize::new(10, 1), Theme::default());
    // 4:2 이중(빨강 RGB), 4:3 물결(256색 2번), 4:4 점, 4:5 파선, 4 곧은 줄(색 없음), 59로 색 되돌리기
    e.feed(b"\x1b[4:2;58:2::255:0:0ma\x1b[4:3;58;5;2mb\x1b[4:4mc\x1b[4:5;59md\x1b[4;24me");
    let s = e.snapshot();
    let c = |i: usize| (s.cells[i].flags, s.cells[i].underline_color);
    assert!(c(0).0.contains(CellFlags::DOUBLE_UNDERLINE));
    assert_eq!(c(0).1, Some(Rgb::new(255, 0, 0)));
    assert!(c(1).0.contains(CellFlags::UNDERCURL));
    assert_eq!(c(1).1, Some(Theme::default().indexed(2)), "256색 번호는 지금 팔레트");
    assert!(c(2).0.contains(CellFlags::DOTTED_UNDERLINE));
    assert!(c(3).0.contains(CellFlags::DASHED_UNDERLINE));
    assert_eq!(c(3).1, None, "59는 밑줄 색을 되돌린다");
    assert!(!c(4).0.contains(CellFlags::UNDERLINE), "24는 밑줄을 끈다");
}

#[test]
fn underline_color_index_follows_osc4_palette_and_ignores_inverse() {
    let mut e = Emulator::new(GridSize::new(4, 1), Theme::default());
    e.feed(b"\x1b]4;2;rgb:11/22/33\x07\x1b[4;58;5;2;7ma\x1b[0m");
    let s = e.snapshot();
    assert_eq!(
        s.cells[0].underline_color,
        Some(Rgb::new(0x11, 0x22, 0x33)),
        "OSC 4로 바꾼 색"
    );
    // 반전은 글자·바탕을 맞바꾸지만 밑줄 색은 그대로(Ghostty `underlineColor`는 반전을 보지 않는다)
    assert_eq!(s.cells[0].fg, Theme::default().background);
}

#[test]
fn faint_mixes_underline_color_toward_the_background() {
    let mut e = Emulator::new(GridSize::new(4, 1), Theme::default());
    e.feed(b"\x1b[4;58:2::255:0:0;2ma\x1b[0m");
    let t = Theme::default();
    let s = e.snapshot();
    assert_eq!(
        s.cells[0].underline_color,
        Some(Rgb::new(255, 0, 0).mix(t.background, 1.0 - t.faint_opacity))
    );
}

#[test]
fn undercurl_from_the_vt_draws_a_wave_in_its_color() {
    let Some(mut f) = fonts(1.0) else {
        return;
    };
    let fr = render(&mut f, b"\x1b[4:3;58:2::255:0:0m  ", 2, 1, false);
    let reddish = fr.buf.iter().filter(|p| {
        let c = Rgb::from_u32(**p);
        c.r > c.g + 60
    });
    assert!(reddish.count() > 0, "빨간 물결 픽셀");
    assert!(
        fr.buf.iter().all(|p| Rgb::from_u32(*p).g < 200),
        "글자색(밝은 쪽)으로 그어진 픽셀은 없다"
    );
}

/// 칸마다 글자 크기가 다르다(#422): 크기가 다른 두 칸을 한 캔버스에 나란히 그리면 칸마다 그 크기로 혼자 그린 화면과 같다.
/// 한글(두 칸 글자)도 칸마다 그 칸의 칸 크기를 따른다
#[test]
fn two_panes_of_different_font_sizes_draw_like_their_own_frames() {
    let at = |size: f32| {
        Fonts::new(&FontConfig {
            size,
            scale: 1.0,
            ..FontConfig::default()
        })
    };
    let (Ok(mut small), Ok(mut big)) = (at(13.0), at(26.0)) else {
        eprintln!("건너뜀: 글꼴을 읽지 못했다");
        return;
    };
    let text = "ab 한글 cd\r\nAB 12 ef".as_bytes();
    let (cols, rows) = (10, 2);
    let (alone_small, alone_big) = (
        render(&mut small, text, cols, rows, false),
        render(&mut big, text, cols, rows, false),
    );
    assert!(alone_big.cw > alone_small.cw && alone_big.ch > alone_small.ch);

    // 한 창에 나란히: 작은 칸은 (0,0), 큰 칸은 그 오른쪽
    let gap = 3;
    let (w, h) = (alone_small.w + gap + alone_big.w, alone_small.h.max(alone_big.h));
    let mut buf = vec![0u32; (w * h) as usize];
    let mut canvas = Canvas::new(&mut buf, w, h);
    canvas.clear(Theme::default().background);
    for (fonts, x) in [(&mut small, 0), (&mut big, (alone_small.w + gap) as i32)] {
        let mut e = Emulator::new(GridSize::new(cols, rows), Theme::default());
        e.feed(text);
        let s = e.snapshot();
        let dc = cells(&s);
        let t = Theme::default();
        let view = GridView {
            cols: s.cols,
            rows: s.rows,
            cells: &dc,
            cursor: None,
            selection_bg: t.selection_background,
            selection_fg: t.selection_foreground,
            preedit: None,
            min_contrast: 1.0,
        };
        draw_grid(&mut canvas, fonts, x, 0, &view);
    }
    let crop = |x0: u32, f: &Frame| -> Vec<u32> {
        (0..f.h)
            .flat_map(|y| (0..f.w).map(move |x| (y * w + x0 + x) as usize))
            .map(|i| buf[i])
            .collect()
    };
    assert_eq!(
        crop(0, &alone_small),
        alone_small.buf,
        "작은 칸이 혼자 그린 화면과 다르다"
    );
    assert_eq!(
        crop(alone_small.w + gap, &alone_big),
        alone_big.buf,
        "큰 칸이 혼자 그린 화면과 다르다"
    );
}
