//! GPU 렌더러와 CPU 렌더러(`bt_render::draw_grid`)의 픽셀 비교. GPU 어댑터가 없으면 알리고 끝낸다

use std::time::Instant;

use bt_core::{Rgb, Theme};
use bt_gpu::GpuRenderer;
use bt_render::{
    Canvas, CursorStyle, DrawCell, DrawCursor, FontConfig, Fonts, GridView, Underline, cluster, draw_grid,
};
use bt_term::{CellFlags, Emulator, GridSize, Screen};

fn fonts(scale: f32) -> Fonts {
    Fonts::new(&FontConfig {
        scale,
        ..FontConfig::default()
    })
    .expect("넣어 둔 글꼴은 늘 있다")
}

fn gpu(atlas: u32) -> Option<GpuRenderer> {
    match GpuRenderer::with_atlas_size(atlas) {
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
            underline: Underline::None,
            strike: false,
            wide: c.flags.contains(CellFlags::WIDE),
            spacer: c.flags.contains(CellFlags::SPACER),
            selected: false,
            emoji: c.combining.contains(&'\u{fe0f}'),
            cluster: None,
            cluster_cells: 0,
            overline: false,
            underline_color: None,
            hidden: false,
            faint: None,
        })
        .collect();
    cluster::mark(&mut cells, s.cols, |i| {
        let c = &s.cells[i];
        cluster::ClusterCell {
            ch: c.ch,
            combining: &c.combining,
            spacer: c.flags.contains(CellFlags::SPACER),
        }
    });
    cells
}

fn cursor_at(s: &Screen, t: &Theme, style: CursorStyle, opacity: f32) -> DrawCursor {
    DrawCursor {
        col: s.cursor.col,
        row: s.cursor.row,
        style,
        wide: s.cursor.wide,
        color: t.cursor,
        text_color: t.cursor_text,
        opacity,
    }
}

fn view<'a>(s: &Screen, dc: &'a [DrawCell], t: &Theme, cursor: Option<DrawCursor>) -> GridView<'a> {
    GridView {
        cols: s.cols,
        rows: s.rows,
        cells: dc,
        cursor,
        selection_bg: t.selection_background,
        selection_fg: t.selection_foreground,
        selection_bg_cell: Default::default(),
        selection_fg_cell: Default::default(),
        preedit: None,
        min_contrast: 1.0,
    }
}

fn screen(text: &str, cols: u16, rows: u16) -> (Screen, Theme) {
    let t = Theme::default();
    let mut e = Emulator::new(GridSize::new(cols, rows), t.clone());
    e.feed(text.as_bytes());
    (e.snapshot(), t)
}

fn cpu_frame(f: &mut Fonts, v: &GridView, clear: Rgb) -> Vec<u32> {
    let m = f.metrics();
    let (w, h) = (m.width * v.cols as u32, m.height * v.rows as u32);
    let mut buf = vec![0u32; (w * h) as usize];
    let mut c = Canvas::new(&mut buf, w, h);
    c.clear(clear);
    draw_grid(&mut c, f, 0, 0, v);
    buf
}

/// (다른 픽셀 수, 채널 하나의 최대 차이, 전체 픽셀 수)
fn diff(a: &[u32], b: &[u32]) -> (usize, i32, usize) {
    assert_eq!(a.len(), b.len(), "프레임 크기가 다르다");
    let (mut n, mut max) = (0, 0);
    for (x, y) in a.iter().zip(b) {
        if x != y {
            n += 1;
            for sh in [0, 8, 16] {
                let d = (((x >> sh) & 0xff) as i32 - ((y >> sh) & 0xff) as i32).abs();
                max = max.max(d);
            }
        }
    }
    (n, max, a.len())
}

#[test]
fn background_only_cells_match_cpu_exactly() {
    let Some(mut g) = gpu(1024) else { return };
    let mut f = fonts(1.0);
    // 칸마다 다른 바탕색, 글자는 공백
    let (cols, rows) = (12usize, 5usize);
    let dc: Vec<DrawCell> = (0..cols * rows)
        .map(|i| DrawCell {
            ch: ' ',
            fg: Rgb::new(255, 255, 255),
            bg: Rgb::new((i * 20) as u8, (255 - i * 4) as u8, (i * 7 % 256) as u8),
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
        })
        .collect();
    let t = Theme::default();
    let v = GridView {
        cols,
        rows,
        cells: &dc,
        cursor: None,
        selection_bg: t.selection_background,
        selection_fg: t.selection_foreground,
        selection_bg_cell: Default::default(),
        selection_fg_cell: Default::default(),
        preedit: None,
        min_contrast: 1.0,
    };
    let clear = Rgb::new(1, 2, 3);
    let want = cpu_frame(&mut f, &v, clear);
    let got = g.render(&mut f, &v, clear);
    let (n, max, total) = diff(&want, &got);
    eprintln!("바탕만: 다른 픽셀 {n}/{total}, 최대 차이 {max}");
    assert_eq!((n, max), (0, 0), "바탕만 있는 칸은 같아야 한다");
}

#[test]
fn text_matches_cpu_within_tolerance() {
    let Some(mut g) = gpu(1024) else { return };
    let mut f = fonts(2.0);
    let (s, t) = screen(
        "\x1b[31mThe quick brown fox\x1b[0m jumps over 12345\r\n\x1b[1;32mbold\x1b[0m \x1b[3;34mitalic\x1b[0m \x1b[43;30m on yellow \x1b[0m\r\n한글 가나다라 \x1b[36m넓은 글자\x1b[0m mixed gjpqy {}[]",
        40,
        5,
    );
    let dc = cells(&s);
    let v = view(&s, &dc, &t, None);
    let want = cpu_frame(&mut f, &v, s.background);
    let got = g.render(&mut f, &v, s.background);
    let (n, max, total) = diff(&want, &got);
    eprintln!(
        "글자: 다른 픽셀 {n}/{total} ({:.3}%), 최대 차이 {max}",
        n as f64 * 100.0 / total as f64
    );
    // 글리프 가장자리의 반올림 차이만 허용한다
    assert!(max <= 2, "채널 최대 차이 {max}");
    assert!(n * 1000 < total * 5, "다른 픽셀 {n}/{total}");
    // 글자가 실제로 그려졌다(바탕만 있는 프레임이 아니다)
    let bg = 0xff00_0000 | s.background.to_u32();
    assert!(got.iter().filter(|p| **p != bg).count() > 1000, "글자 픽셀이 너무 적다");
}

#[test]
fn cursor_shapes_match_cpu() {
    let Some(mut g) = gpu(1024) else { return };
    let mut f = fonts(2.0);
    for (style, opacity) in [
        (CursorStyle::Block, 1.0),
        (CursorStyle::Block, 0.5),
        (CursorStyle::Beam, 1.0),
        (CursorStyle::Underline, 1.0),
        (CursorStyle::Hollow, 1.0),
        (CursorStyle::Lock, 1.0),
    ] {
        // 커서 칸에 글자가 있도록 한 칸 되돌린다
        let (s, t) = screen("hello wide\x1b[D", 16, 3);
        let dc = cells(&s);
        let v = view(&s, &dc, &t, Some(cursor_at(&s, &t, style, opacity)));
        let want = cpu_frame(&mut f, &v, s.background);
        let got = g.render(&mut f, &v, s.background);
        let (n, max, total) = diff(&want, &got);
        eprintln!("커서 {style:?} 불투명도 {opacity}: 다른 픽셀 {n}/{total}, 최대 차이 {max}");
        assert!(max <= 2, "{style:?}: 채널 최대 차이 {max}");
        assert!(n * 1000 < total * 5, "{style:?}: 다른 픽셀 {n}/{total}");
        // 커서가 실제로 그려졌다: 커서 없는 프레임과 다르다
        let none = view(&s, &dc, &t, None);
        let plain = g.render(&mut f, &none, s.background);
        assert!(plain != got, "{style:?}: 커서가 그려지지 않았다");
    }
}

#[test]
fn block_cursor_over_a_wide_char_covers_two_cells_with_cursor_text_color() {
    let Some(mut g) = gpu(1024) else { return };
    let mut f = fonts(2.0);
    let (s, t) = screen("가나다\x1b[3D", 8, 2);
    assert!(s.cursor.wide, "커서가 넓은 글자 위에 있어야 한다");
    let dc = cells(&s);
    let v = view(&s, &dc, &t, Some(cursor_at(&s, &t, CursorStyle::Block, 1.0)));
    let got = g.render(&mut f, &v, s.background);
    let want = cpu_frame(&mut f, &v, s.background);
    let (n, max, total) = diff(&want, &got);
    eprintln!("넓은 글자 위 상자 커서: 다른 픽셀 {n}/{total}, 최대 차이 {max}");
    assert!(max <= 2 && n * 1000 < total * 5);
    // 커서 칸 두 개의 왼쪽 위 모서리가 커서색이다
    let m = f.metrics();
    let w = (m.width * 8) as usize;
    let cur = 0xff00_0000 | t.cursor.to_u32();
    let x0 = s.cursor.col * m.width as usize;
    assert_eq!(got[x0 + 1], cur);
    assert_eq!(got[x0 + 2 * m.width as usize - 2 + w], cur, "둘째 칸 끝도 커서색");
}

#[test]
fn color_emoji_matches_cpu() {
    let Some(mut g) = gpu(1024) else { return };
    let mut f = fonts(2.0);
    if !f.has_color_emoji() {
        eprintln!("건너뜀: 색 이모지 글꼴이 없다");
        return;
    }
    let (s, t) = screen("a\u{2764}\u{fe0f}b 😀 c", 16, 2);
    let dc = cells(&s);
    let v = view(&s, &dc, &t, None);
    let want = cpu_frame(&mut f, &v, s.background);
    let got = g.render(&mut f, &v, s.background);
    let (n, max, total) = diff(&want, &got);
    eprintln!("이모지: 다른 픽셀 {n}/{total}, 최대 차이 {max}");
    assert!(max <= 2 && n * 1000 < total * 5);
    let bg = 0xff00_0000 | s.background.to_u32();
    let colored = got
        .iter()
        .filter(|p| {
            let (r, gg, b) = ((**p >> 16 & 0xff) as i32, (**p >> 8 & 0xff) as i32, (**p & 0xff) as i32);
            **p != bg && (r - gg).abs() + (gg - b).abs() > 60
        })
        .count();
    assert!(colored > 100, "색 있는 픽셀 {colored}");
}

/// 색 아틀라스는 처음 쓸 때 잡고(#400) 모자라면 커진다. 작은 아틀라스에서 이모지가 많아도 큰 것과 같은 그림이다
#[test]
fn lazy_color_atlas_grows_with_many_emoji_and_renders_the_same() {
    let (Some(mut small), Some(mut big)) = (gpu(16), gpu(2048)) else {
        return;
    };
    let mut f = fonts(2.0);
    if !f.has_color_emoji() {
        eprintln!("건너뜀: 색 이모지 글꼴이 없다");
        return;
    }
    let (s, t) = screen(
        "a 😀 😁 😂 🎉 🚀 🌈 🍎 🐱 🐶 ❤\u{fe0f} b\r\n🍕 🍔 🎸 🏀 ⚽ 🌙 ⭐ 🔥 💧 🌸 c",
        40,
        2,
    );
    let dc = cells(&s);
    let v = view(&s, &dc, &t, None);
    assert_eq!(small.gpu_memory().color_atlas, 4, "그리기 전에는 1x1");
    let a = small.render(&mut f, &v, s.background);
    let b = big.render(&mut f, &v, s.background);
    let m = small.gpu_memory();
    eprintln!("작은 것 {m:?}, 키운 횟수 {}", small.atlas_grow_count());
    assert!(m.color_atlas > 4, "{m:?}");
    assert!(small.atlas_grow_count() >= 1, "16px 색 아틀라스는 커져야 한다");
    assert_eq!(a, b, "아틀라스 크기가 그림을 바꾸면 안 된다");
    assert_eq!(small.render(&mut f, &v, s.background), b);
}

#[test]
fn small_atlas_grows_and_renders_the_same() {
    let (Some(mut small), Some(mut big)) = (gpu(16), gpu(2048)) else {
        return;
    };
    let mut f = fonts(2.0);
    let mut text = String::new();
    for r in 0..6 {
        text.push_str(&format!(
            "\x1b[3{}m{r} ABCDEFGHIJKLMNOPQRSTUVWXYZ abcdefghijklmnopqrstuvwxyz 가나다라마바사아자차\r\n",
            r % 7 + 1
        ));
    }
    let (s, t) = screen(&text, 60, 6);
    let dc = cells(&s);
    let v = view(&s, &dc, &t, None);
    let a = small.render(&mut f, &v, s.background);
    let b = big.render(&mut f, &v, s.background);
    eprintln!(
        "아틀라스 키운 횟수: 작은 것 {}, 큰 것 {}",
        small.atlas_grow_count(),
        big.atlas_grow_count()
    );
    assert!(small.atlas_grow_count() >= 2, "16px 아틀라스는 여러 번 커져야 한다");
    assert_eq!(big.atlas_grow_count(), 0);
    assert_eq!(a, b, "아틀라스 크기가 그림을 바꾸면 안 된다");
    // 키운 뒤에도 같은 화면을 다시 그리면 같다(캐시 위치가 유효하다)
    assert_eq!(small.render(&mut f, &v, s.background), b);
}

/// 실험 E6: E1과 같은 화면(200x60, 2배)을 CPU `draw_grid`와 GPU로 통째 그리는 시간.
/// `cargo test -p bt-gpu --release --test gpu -- --ignored --nocapture e6`
#[test]
#[ignore]
fn e6_cpu_vs_gpu_full_redraw_200x60_retina() {
    let Some(mut g) = gpu(1024) else { return };
    let mut f = fonts(2.0);
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
    let (s, t) = screen(&text, 200, 60);
    let dc = cells(&s);
    let v = view(&s, &dc, &t, Some(cursor_at(&s, &t, CursorStyle::Block, 1.0)));
    let m = f.metrics();
    let (w, h) = (m.width * 200, m.height * 60);
    let mut buf = vec![0u32; (w * h) as usize];
    // 첫 회는 글리프 캐시와 아틀라스를 채운다
    let _ = g.render(&mut f, &v, s.background);
    let mut cpu = Vec::new();
    let (mut prep, mut gpu_ms, mut read, mut total) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for _ in 0..20 {
        let t0 = Instant::now();
        {
            let mut c = Canvas::new(&mut buf, w, h);
            c.clear(s.background);
            draw_grid(&mut c, &mut f, 0, 0, &v);
        }
        cpu.push(t0.elapsed().as_secs_f64() * 1000.0);
        let t1 = Instant::now();
        let px = g.render(&mut f, &v, s.background);
        total.push(t1.elapsed().as_secs_f64() * 1000.0);
        let tm = g.timings();
        prep.push(tm.prepare_ms);
        gpu_ms.push(tm.gpu_ms);
        read.push(tm.readback_ms);
        assert_eq!(px.len(), buf.len());
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
    println!(
        "어댑터 {}, 셀 {}x{}px, 화면 {w}x{h}px, 20회",
        g.adapter_name(),
        m.width,
        m.height
    );
    println!("CPU draw_grid(지우기 포함): {}", stat(&mut cpu));
    println!("GPU 준비(인스턴스 만들기): {}", stat(&mut prep));
    println!("GPU 제출~끝: {}", stat(&mut gpu_ms));
    println!("GPU 읽어 내기: {}", stat(&mut read));
    println!("GPU 합계(렌더+읽기): {}", stat(&mut total));
}
