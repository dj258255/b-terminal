//! 층 합치기(ADR-036): 창 없이 GPU로 합친 프레임 = CPU 캔버스에 같은 순서로 그린 프레임.
//! 아래층 → 칸마다 격자(자리가 다른 둘) → 위층(반투명 흐림 포함). GPU 어댑터가 없으면 알리고 끝낸다

use bt_core::{Rgb, Theme};
use bt_gpu::{GpuRenderer, LayerUpdate, PaneDraw};
use bt_render::{Canvas, DrawCell, FontConfig, Fonts, GridView, Underline, cluster, draw_grid};
use bt_term::{CellFlags, Emulator, GridSize, Screen};

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

fn screen(text: &str, cols: u16, rows: u16) -> (Screen, Theme) {
    let t = Theme::default();
    let mut e = Emulator::new(GridSize::new(cols, rows), t.clone());
    e.feed(text.as_bytes());
    (e.snapshot(), t)
}

fn view<'a>(s: &Screen, dc: &'a [DrawCell], t: &Theme) -> GridView<'a> {
    GridView {
        cols: s.cols,
        rows: s.rows,
        cells: dc,
        cursor: None,
        selection_bg: t.selection_background,
        selection_fg: t.selection_foreground,
        preedit: None,
        min_contrast: 1.0,
    }
}

const COLS: u16 = 24;
const ROWS: u16 = 5;

/// 창 크기와 두 칸의 자리(서로 x, y가 다르다)
struct Layout {
    w: u32,
    h: u32,
    a: (i32, i32),
    b: (i32, i32),
    grid: (u32, u32),
}

fn layout(f: &Fonts) -> Layout {
    let m = f.metrics();
    let grid = (COLS as u32 * m.width, ROWS as u32 * m.height);
    let a = (13, 41);
    let b = (a.0 + grid.0 as i32 + 29, a.1 + 2 * m.height as i32 + 3);
    Layout {
        w: b.0 as u32 + grid.0 + 17,
        h: b.1 as u32 + grid.1 + 23,
        a,
        b,
        grid,
    }
}

const WIN_BG: Rgb = Rgb {
    r: 0x20,
    g: 0x24,
    b: 0x2b,
};
const TAB_BG: Rgb = Rgb {
    r: 0x33,
    g: 0x38,
    b: 0x40,
};

/// 아래층: 창 바탕, 탭 막대, 칸마다 바깥 사각형(칸 바탕)
fn lower_ops(c: &mut Canvas, l: &Layout, pane_bg: Rgb) {
    c.clear(WIN_BG);
    c.fill_rect(0, 0, l.w, 30, TAB_BG);
    for (x, y) in [l.a, l.b] {
        c.fill_rect(x - 6, y - 6, l.grid.0 + 12, l.grid.1 + 12, pane_bg);
    }
}

/// 위층: 반투명 그림(kitty), 흐림(고르지 않은 칸), 칸 나눔 선, 스크롤바 같은 반투명 막대, 글 위 한 픽셀
fn upper_ops(c: &mut Canvas, l: &Layout) {
    let mut rgba = vec![0u8; 14 * 14 * 4];
    for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
        px.copy_from_slice(&[200, (i * 3 % 256) as u8, 40, (i * 5 % 256) as u8]);
    }
    c.draw_rgba(
        l.a.0 + 30,
        l.a.1 + 10,
        28,
        28,
        &rgba,
        14,
        14,
        (0, 0, l.w as i32, l.h as i32),
    );
    c.blend_rect(
        l.b.0 - 6,
        l.b.1 - 6,
        l.grid.0 + 12,
        l.grid.1 + 12,
        Rgb::new(0, 0, 0),
        90,
    );
    c.fill_rect(l.b.0 - 16, 30, 4, l.h - 30, Rgb::new(90, 90, 90));
    c.blend_rect(
        l.a.0 + l.grid.0 as i32 - 8,
        l.a.1,
        6,
        l.grid.1,
        Rgb::new(255, 255, 255),
        110,
    );
    c.blend_px(l.a.0 + 1, l.a.1 + 1, Rgb::new(255, 0, 0), 77);
}

/// 같은 순서로 CPU 캔버스 하나에 그린 기준 프레임
fn cpu_reference(f: &mut Fonts, l: &Layout, bg: Rgb, views: [&GridView; 2], with_upper: bool) -> Vec<u32> {
    let mut buf = vec![0u32; (l.w * l.h) as usize];
    let mut c = Canvas::new(&mut buf, l.w, l.h);
    lower_ops(&mut c, l, bg);
    draw_grid(&mut c, f, l.a.0, l.a.1, views[0]);
    draw_grid(&mut c, f, l.b.0, l.b.1, views[1]);
    if with_upper {
        upper_ops(&mut c, l);
    }
    buf
}

/// (다른 픽셀 수, 채널 하나의 최대 차이, 차이가 1을 넘는 픽셀 수, 전체)
fn diff(a: &[u32], b: &[u32]) -> (usize, i32, usize, usize) {
    assert_eq!(a.len(), b.len(), "프레임 크기가 다르다");
    let (mut n, mut max, mut over) = (0, 0, 0);
    for (x, y) in a.iter().zip(b) {
        if x != y {
            n += 1;
            let d = [0, 8, 16]
                .iter()
                .map(|sh| (((x >> sh) & 0xff) as i32 - ((y >> sh) & 0xff) as i32).abs())
                .max()
                .unwrap_or(0);
            max = max.max(d);
            over += usize::from(d > 1);
        }
    }
    (n, max, over, a.len())
}

fn assert_close(what: &str, want: &[u32], got: &[u32]) {
    let (n, max, over, total) = diff(want, got);
    eprintln!("{what}: 다른 픽셀 {n}/{total}, 차이가 1을 넘는 픽셀 {over}, 최대 차이 {max}");
    assert!(max <= 3, "{what}: 최대 차이 {max}");
    assert!(over * 1000 < total * 5, "{what}: 차이가 1을 넘는 픽셀 {over}/{total}");
}

struct Fixture {
    f: Fonts,
    l: Layout,
    sa: Screen,
    sb: Screen,
    ca: Vec<DrawCell>,
    cb: Vec<DrawCell>,
    t: Theme,
    lower: Vec<u32>,
    upper: Vec<u32>,
}

fn fixture() -> Fixture {
    let f = fonts();
    let l = layout(&f);
    let (sa, t) = screen(
        "\x1b[31mThe quick brown fox\x1b[0m\r\n\x1b[1;32mbold\x1b[0m \x1b[43;30m on yellow \x1b[0m\r\n한글 가나다라 gjpqy",
        COLS,
        ROWS,
    );
    let (sb, _) = screen(
        "\x1b[36mpane B\x1b[0m 12345\r\n\x1b[44mblue bg\x1b[0m {}[]\r\n넓은 글자",
        COLS,
        ROWS,
    );
    let (ca, cb) = (cells(&sa), cells(&sb));
    let mut lower = vec![0u32; (l.w * l.h) as usize];
    lower_ops(&mut Canvas::new(&mut lower, l.w, l.h), &l, sa.background);
    let mut upper = vec![0u32; (l.w * l.h) as usize];
    upper_ops(&mut Canvas::new(&mut upper, l.w, l.h), &l);
    Fixture {
        f,
        l,
        sa,
        sb,
        ca,
        cb,
        t,
        lower,
        upper,
    }
}

#[test]
fn composed_frame_matches_cpu_canvas_drawn_in_the_same_order() {
    let Some(mut g) = gpu() else { return };
    let mut fx = fixture();
    let (va, vb) = (view(&fx.sa, &fx.ca, &fx.t), view(&fx.sb, &fx.cb, &fx.t));
    let want = cpu_reference(&mut fx.f, &fx.l, fx.sa.background, [&va, &vb], true);
    let panes = [
        PaneDraw {
            x: fx.l.a.0,
            y: fx.l.a.1,
            view: &va,
            clear: fx.sa.background,
        },
        PaneDraw {
            x: fx.l.b.0,
            y: fx.l.b.1,
            view: &vb,
            clear: fx.sb.background,
        },
    ];
    let layers = LayerUpdate {
        lower: Some(&fx.lower),
        upper: Some(&fx.upper),
    };
    let got = g.compose(&mut fx.f, (fx.l.w, fx.l.h), layers, &panes);
    assert_close("합친 프레임", &want, &got);

    // 합쳤다는 증거: 두 칸이 모두 자리에 있고(글자 픽셀이 바탕과 다르다), 위층이 얹혔다
    let without_upper = cpu_reference(&mut fx.f, &fx.l, fx.sa.background, [&va, &vb], false);
    let (n, max, _, _) = diff(&without_upper, &got);
    assert!(
        n > 500 && max > 20,
        "위층이 보이지 않는다: 다른 픽셀 {n}, 최대 차이 {max}"
    );
}

#[test]
fn translucent_upper_pixels_blend_over_the_grid() {
    let Some(mut g) = gpu() else { return };
    let mut fx = fixture();
    let (va, vb) = (view(&fx.sa, &fx.ca, &fx.t), view(&fx.sb, &fx.cb, &fx.t));
    let panes = [
        PaneDraw {
            x: fx.l.a.0,
            y: fx.l.a.1,
            view: &va,
            clear: fx.sa.background,
        },
        PaneDraw {
            x: fx.l.b.0,
            y: fx.l.b.1,
            view: &vb,
            clear: fx.sb.background,
        },
    ];
    let got = g.compose(
        &mut fx.f,
        (fx.l.w, fx.l.h),
        LayerUpdate {
            lower: Some(&fx.lower),
            upper: Some(&fx.upper),
        },
        &panes,
    );
    // 흐린 칸(B)의 맨 위 왼쪽 안쪽 바탕: 칸 바탕색을 알파 90만큼 검게
    let m = fx.f.metrics();
    let (px, py) = (fx.l.b.0 as usize + 1, fx.l.b.1 as usize + 4 * m.height as usize + 2);
    let bg = fx.sa.background;
    let mul = |c: u8| ((c as u32 * (255 - 90) + 127) / 255) as i32;
    let p = got[py * fx.l.w as usize + px];
    let ch = |sh: u32| ((p >> sh) & 0xff) as i32;
    assert!(
        (ch(16) - mul(bg.r)).abs() <= 1 && (ch(8) - mul(bg.g)).abs() <= 1 && (ch(0) - mul(bg.b)).abs() <= 1,
        "흐림이 섞이지 않았다: {p:08x}, 바탕 {bg:?}"
    );
    assert_ne!(p & 0xff_ffff, bg.to_u32(), "흐림이 바탕을 바꾸지 못했다");
}

#[test]
fn layers_stay_when_only_the_grid_changes() {
    let Some(mut g) = gpu() else { return };
    let mut fx = fixture();
    let (va, vb) = (view(&fx.sa, &fx.ca, &fx.t), view(&fx.sb, &fx.cb, &fx.t));
    let panes = [
        PaneDraw {
            x: fx.l.a.0,
            y: fx.l.a.1,
            view: &va,
            clear: fx.sa.background,
        },
        PaneDraw {
            x: fx.l.b.0,
            y: fx.l.b.1,
            view: &vb,
            clear: fx.sb.background,
        },
    ];
    let size = (fx.l.w, fx.l.h);
    let first = g.compose(
        &mut fx.f,
        size,
        LayerUpdate {
            lower: Some(&fx.lower),
            upper: Some(&fx.upper),
        },
        &panes,
    );
    // 층을 올리지 않고 같은 격자를 다시 합치면 같은 프레임이다(층 텍스처가 남아 있다)
    let again = g.compose(&mut fx.f, size, LayerUpdate::default(), &panes);
    assert_eq!(first, again, "올리지 않아도 층이 남아 있어야 한다");

    // 격자만 바뀐 프레임: 층은 그대로, B칸의 글자만 새것
    let (sb2, _) = screen("\x1b[35mchanged\x1b[0m only the grid", COLS, ROWS);
    let cb2 = cells(&sb2);
    let vb2 = view(&sb2, &cb2, &fx.t);
    let panes2 = [
        PaneDraw {
            x: fx.l.a.0,
            y: fx.l.a.1,
            view: &va,
            clear: fx.sa.background,
        },
        PaneDraw {
            x: fx.l.b.0,
            y: fx.l.b.1,
            view: &vb2,
            clear: sb2.background,
        },
    ];
    let got = g.compose(&mut fx.f, size, LayerUpdate::default(), &panes2);
    let want = cpu_reference(&mut fx.f, &fx.l, fx.sa.background, [&va, &vb2], true);
    assert_close("격자만 바뀐 프레임", &want, &got);
    assert_ne!(first, got);
}
