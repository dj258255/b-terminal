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

/// 바뀐 띠의 폭 `cols`가 칸 경계(128px)로 넓어진 최대 폭: 칸 경계에 걸치면 칸 하나가 더 든다(#393)
fn wide(cols: usize) -> usize {
    cols.next_multiple_of(128) + 128
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
            mark: None,
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
        selection_bg_cell: Default::default(),
        selection_fg_cell: Default::default(),
        preedit: None,
        min_contrast: 1.0,
        trail: None,
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
            pixel_shift: 0,
            font: 0,
            x: fx.l.a.0,
            y: fx.l.a.1,
            view: &va,
            clear: fx.sa.background,
        },
        PaneDraw {
            pixel_shift: 0,
            font: 0,
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
            pixel_shift: 0,
            font: 0,
            x: fx.l.a.0,
            y: fx.l.a.1,
            view: &va,
            clear: fx.sa.background,
        },
        PaneDraw {
            pixel_shift: 0,
            font: 0,
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
            pixel_shift: 0,
            font: 0,
            x: fx.l.a.0,
            y: fx.l.a.1,
            view: &va,
            clear: fx.sa.background,
        },
        PaneDraw {
            pixel_shift: 0,
            font: 0,
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
            pixel_shift: 0,
            font: 0,
            x: fx.l.a.0,
            y: fx.l.a.1,
            view: &va,
            clear: fx.sa.background,
        },
        PaneDraw {
            pixel_shift: 0,
            font: 0,
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

/// 진행 막대 같은 작은 덧그림: 칸 A 맨 위 몇 줄에 `width`픽셀 폭의 띠
fn bar_ops(c: &mut Canvas, l: &Layout, width: u32) {
    c.blend_rect(l.a.0, l.a.1, width, 3, Rgb::new(40, 200, 90), 200);
}

/// 올리기 뒤 층 텍스처에 남은 것이 CPU 프레임과 같아야 한다(부분 올리기가 옛 픽셀을 남기면 실패)
#[test]
fn partial_layer_upload_keeps_every_frame_equal_to_the_cpu_frame() {
    let Some(mut g) = gpu() else { return };
    let mut fx = fixture();
    let (va, vb) = (view(&fx.sa, &fx.ca, &fx.t), view(&fx.sb, &fx.cb, &fx.t));
    let panes = [
        PaneDraw {
            pixel_shift: 0,
            font: 0,
            x: fx.l.a.0,
            y: fx.l.a.1,
            view: &va,
            clear: fx.sa.background,
        },
        PaneDraw {
            pixel_shift: 0,
            font: 0,
            x: fx.l.b.0,
            y: fx.l.b.1,
            view: &vb,
            clear: fx.sb.background,
        },
    ];
    let size = (fx.l.w, fx.l.h);
    let full_bytes = 2 * (size.0 * size.1 * 4) as usize;
    let upper_with = |l: &Layout, width: u32| {
        let mut u = vec![0u32; (l.w * l.h) as usize];
        if width > 0 {
            bar_ops(&mut Canvas::new(&mut u, l.w, l.h), l, width);
        }
        u
    };
    let reference = |f: &mut Fonts, l: &Layout, bg: Rgb, width: u32| {
        let mut buf = cpu_reference(f, l, bg, [&va, &vb], false);
        if width > 0 {
            bar_ops(&mut Canvas::new(&mut buf, l.w, l.h), l, width);
        }
        buf
    };

    // 1. 덧그림 켬(첫 프레임이라 통째)
    let up1 = upper_with(&fx.l, 40);
    let got = g.compose(
        &mut fx.f,
        size,
        LayerUpdate {
            lower: Some(&fx.lower),
            upper: Some(&up1),
        },
        &panes,
    );
    assert_close("1. 덧그림 켬", &reference(&mut fx.f, &fx.l, fx.sa.background, 40), &got);
    // 아래층은 통째, 새 위층 텍스처는 0으로 시작하므로 0이 아닌 자리만(#387)
    let first = g.timings().upload_bytes;
    assert!(
        first >= full_bytes / 2 && first < full_bytes,
        "첫 프레임: 아래층 통째와 위층의 그린 자리: {first}"
    );

    // 2. 진행 막대 값만 바뀜: 아래층은 같고(같은 내용으로 다시 넘긴다) 위층은 띠 끝만 달라진다
    let up2 = upper_with(&fx.l, 90);
    let got = g.compose(
        &mut fx.f,
        size,
        LayerUpdate {
            lower: Some(&fx.lower),
            upper: Some(&up2),
        },
        &panes,
    );
    assert_close(
        "2. 막대 값 바뀜",
        &reference(&mut fx.f, &fx.l, fx.sa.background, 90),
        &got,
    );
    let bytes = g.timings().upload_bytes;
    assert!(
        bytes > 0 && bytes <= wide(90) * 3 * 4,
        "바뀐 띠만 올라야 한다: {bytes}바이트(통째 {full_bytes})"
    );

    // 3. 막대가 줄어듦: 줄어든 끝의 옛 픽셀이 남으면 안 된다
    let up3 = upper_with(&fx.l, 25);
    let got = g.compose(
        &mut fx.f,
        size,
        LayerUpdate {
            lower: Some(&fx.lower),
            upper: Some(&up3),
        },
        &panes,
    );
    assert_close(
        "3. 막대 줄어듦",
        &reference(&mut fx.f, &fx.l, fx.sa.background, 25),
        &got,
    );

    // 4. 덧그림 끔: 위층이 비었다. 그리지도 올리지도 않고, 옛 막대가 남지 않는다
    let up4 = upper_with(&fx.l, 0);
    let got = g.compose(
        &mut fx.f,
        size,
        LayerUpdate {
            lower: Some(&fx.lower),
            upper: Some(&up4),
        },
        &panes,
    );
    assert_close("4. 덧그림 끔", &reference(&mut fx.f, &fx.l, fx.sa.background, 0), &got);
    assert!(g.timings().upload_bytes <= wide(25) * 3 * 4);

    // 5. 빈 채로 한 번 더, 그리고 다시 켬(비었다가 켜지는 프레임)
    let got = g.compose(
        &mut fx.f,
        size,
        LayerUpdate {
            lower: Some(&fx.lower),
            upper: Some(&up4),
        },
        &panes,
    );
    assert_close("5. 빈 채로", &reference(&mut fx.f, &fx.l, fx.sa.background, 0), &got);
    assert_eq!(g.timings().upload_bytes, 0, "같은 층을 다시 넘기면 올릴 것이 없다");
    let got = g.compose(
        &mut fx.f,
        size,
        LayerUpdate {
            lower: Some(&fx.lower),
            upper: Some(&up1),
        },
        &panes,
    );
    assert_close("5. 다시 켬", &reference(&mut fx.f, &fx.l, fx.sa.background, 40), &got);

    // 6. 아래층 한 곳만 바뀜(탭 막대 글자 자리 같은 곳): 그 곳만 올라가고 다른 곳은 그대로
    let mut lower2 = fx.lower.clone();
    Canvas::new(&mut lower2, size.0, size.1).fill_rect(100, 5, 20, 8, Rgb::new(200, 50, 50));
    let got = g.compose(
        &mut fx.f,
        size,
        LayerUpdate {
            lower: Some(&lower2),
            upper: Some(&up1),
        },
        &panes,
    );
    let mut want = reference(&mut fx.f, &fx.l, fx.sa.background, 40);
    Canvas::new(&mut want, size.0, size.1).fill_rect(100, 5, 20, 8, Rgb::new(200, 50, 50));
    assert_close("6. 아래층 한 곳", &want, &got);
    assert!(g.timings().upload_bytes <= wide(20) * 8 * 4);
}

/// 앱이 층 입력이 같다고 보고 층을 넘기지 않은 프레임(ADR-038)과 미루거나 가려져 건너뛴 프레임 뒤에도,
/// 합친 결과가 같은 내용의 CPU 프레임과 같아야 한다. 층을 넘기지 않으면 견주지도 올리지도 않는다
#[test]
fn frames_without_a_layer_update_keep_the_last_layers_and_follow_later_changes() {
    let Some(mut g) = gpu() else { return };
    let mut fx = fixture();
    let (va, vb) = (view(&fx.sa, &fx.ca, &fx.t), view(&fx.sb, &fx.cb, &fx.t));
    let panes = [
        PaneDraw {
            pixel_shift: 0,
            font: 0,
            x: fx.l.a.0,
            y: fx.l.a.1,
            view: &va,
            clear: fx.sa.background,
        },
        PaneDraw {
            pixel_shift: 0,
            font: 0,
            x: fx.l.b.0,
            y: fx.l.b.1,
            view: &vb,
            clear: fx.sb.background,
        },
    ];
    let size = (fx.l.w, fx.l.h);
    let upper_with = |l: &Layout, width: u32| {
        let mut u = vec![0u32; (l.w * l.h) as usize];
        bar_ops(&mut Canvas::new(&mut u, l.w, l.h), l, width);
        u
    };
    let reference = |f: &mut Fonts, l: &Layout, bg: Rgb, width: u32| {
        let mut buf = cpu_reference(f, l, bg, [&va, &vb], false);
        bar_ops(&mut Canvas::new(&mut buf, l.w, l.h), l, width);
        buf
    };
    let none = || LayerUpdate {
        lower: None,
        upper: None,
    };

    // 1. 첫 프레임(통째)
    let up1 = upper_with(&fx.l, 40);
    let got = g.compose(
        &mut fx.f,
        size,
        LayerUpdate {
            lower: Some(&fx.lower),
            upper: Some(&up1),
        },
        &panes,
    );
    assert_close("1. 첫 프레임", &reference(&mut fx.f, &fx.l, fx.sa.background, 40), &got);

    // 2. 층 입력이 같음(진행 막대 값이 그대로): 층을 넘기지 않는다. 견주지도 올리지도 않고 결과는 같다
    for n in 0..3 {
        let got = g.compose(&mut fx.f, size, none(), &panes);
        assert_close(
            &format!("2. 층 없이 {n}번째"),
            &reference(&mut fx.f, &fx.l, fx.sa.background, 40),
            &got,
        );
        assert_eq!(g.timings().upload_bytes, 0);
        assert!(
            g.timings().upload_ms < 0.2,
            "층을 넘기지 않으면 견주기도 없다: {}ms",
            g.timings().upload_ms
        );
    }

    // 3. 덧그림이 바뀜(건너뛴 프레임 뒤): 새 층이 반영되고 옛 막대가 남지 않는다
    let up2 = upper_with(&fx.l, 15);
    let got = g.compose(
        &mut fx.f,
        size,
        LayerUpdate {
            lower: Some(&fx.lower),
            upper: Some(&up2),
        },
        &panes,
    );
    assert_close(
        "3. 막대 값 바뀜",
        &reference(&mut fx.f, &fx.l, fx.sa.background, 15),
        &got,
    );
    let bytes = g.timings().upload_bytes;
    assert!(bytes > 0 && bytes <= wide(40) * 3 * 4, "바뀐 띠만 올라야 한다: {bytes}");

    // 4. 미루거나 가려져 건너뛴 프레임 뒤: 앱이 층을 통째로 다시 넘겨도(같은 내용) 올릴 것이 없고 결과는 같다
    let got = g.compose(&mut fx.f, size, none(), &panes);
    assert_close("4. 건너뜀", &reference(&mut fx.f, &fx.l, fx.sa.background, 15), &got);
    let got = g.compose(
        &mut fx.f,
        size,
        LayerUpdate {
            lower: Some(&fx.lower),
            upper: Some(&up2),
        },
        &panes,
    );
    assert_close(
        "4. 통째로 다시",
        &reference(&mut fx.f, &fx.l, fx.sa.background, 15),
        &got,
    );
    assert_eq!(g.timings().upload_bytes, 0);
}

fn frame_panes<'a>(fx: &Fixture, va: &'a GridView<'a>, vb: &'a GridView<'a>) -> [PaneDraw<'a>; 2] {
    [
        PaneDraw {
            pixel_shift: 0,
            font: 0,
            x: fx.l.a.0,
            y: fx.l.a.1,
            view: va,
            clear: fx.sa.background,
        },
        PaneDraw {
            pixel_shift: 0,
            font: 0,
            x: fx.l.b.0,
            y: fx.l.b.1,
            view: vb,
            clear: fx.sb.background,
        },
    ]
}

/// 색 아틀라스(RGBA 1024x1024 = 4MiB)는 색 글리프(이모지)를 처음 볼 때 잡는다: 글자만 있는 창은 쥐지 않는다(#400).
/// 처음 본 뒤의 화면은 CPU 기준과 같다
#[test]
fn color_atlas_is_taken_when_the_first_color_glyph_shows_up() {
    let Some(mut g) = gpu() else { return };
    let mut fx = fixture();
    let size = (fx.l.w, fx.l.h);
    let (va, vb) = (view(&fx.sa, &fx.ca, &fx.t), view(&fx.sb, &fx.cb, &fx.t));
    let none = |lower| LayerUpdate {
        lower,
        upper: Some(&[]),
    };
    let want = cpu_reference(&mut fx.f, &fx.l, fx.sa.background, [&va, &vb], false);
    let panes = frame_panes(&fx, &va, &vb);
    let got = g.compose(&mut fx.f, size, none(Some(&fx.lower)), &panes);
    assert_close("글자만 있는 프레임", &want, &got);
    let m = g.gpu_memory();
    eprintln!("글자만: {m:?}");
    assert!(m.color_atlas <= 4, "색 글리프가 없으면 색 아틀라스는 1x1이다: {m:?}");
    assert!(
        m.atlas_cpu < 2 * 1024 * 1024,
        "색 아틀라스 CPU 사본도 쥐지 않는다: {m:?}"
    );
    assert_eq!(m.gray_atlas, 1024 * 1024, "{m:?}");

    if !fx.f.has_color_emoji() {
        eprintln!("건너뜀: 색 이모지 글꼴이 없다");
        return;
    }
    let (se, _) = screen("a 😀 \u{2764}\u{fe0f} 🎉 한글", COLS, ROWS);
    let ce = cells(&se);
    let ve = view(&se, &ce, &fx.t);
    let want = cpu_reference(&mut fx.f, &fx.l, fx.sa.background, [&ve, &vb], false);
    let panes = frame_panes(&fx, &ve, &vb);
    let got = g.compose(&mut fx.f, size, none(None), &panes);
    assert_close("처음 본 이모지 프레임", &want, &got);
    let m = g.gpu_memory();
    eprintln!("이모지 뒤: {m:?}");
    assert!(
        m.color_atlas > 4 && m.color_atlas < 1024 * 1024 * 4,
        "처음 본 색 글리프에서 작게 잡는다: {m:?}"
    );
    // 한 번 잡은 뒤에는 같은 화면이 그대로다
    let again = g.compose(&mut fx.f, size, none(None), &panes);
    assert_close("다시 그린 이모지 프레임", &want, &again);
}

/// 처음 보는 색 글리프(색 아틀라스를 잡는 프레임)의 비용(#400): 새 렌더러에 글자 프레임을 그린 뒤 이모지 프레임을 그리고,
/// 그 프레임의 벽시계 시간을 같은 이모지를 다시 그린 프레임과 견준다.
/// `cargo test -p bt-gpu --release --test compose -- --ignored --nocapture first_color_glyph_cost`
#[test]
#[ignore = "시간을 재는 시험(--ignored로 돌린다)"]
fn first_color_glyph_cost() {
    if gpu().is_none() {
        return;
    }
    let median = |mut v: Vec<f64>| {
        v.sort_by(|a, b| a.total_cmp(b));
        v[v.len() / 2]
    };
    let (mut first, mut second, mut text) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..15 {
        let Some(mut g) = gpu() else { return };
        let mut fx = fixture();
        if !fx.f.has_color_emoji() {
            eprintln!("건너뜀: 색 이모지 글꼴이 없다");
            return;
        }
        let size = (fx.l.w, fx.l.h);
        let (va, vb) = (view(&fx.sa, &fx.ca, &fx.t), view(&fx.sb, &fx.cb, &fx.t));
        let (se, _) = screen("a 😀 \u{2764}\u{fe0f} 🎉 한글", COLS, ROWS);
        let ce = cells(&se);
        let ve = view(&se, &ce, &fx.t);
        let (pa, pe) = (frame_panes(&fx, &va, &vb), frame_panes(&fx, &ve, &vb));
        let none = |lower| LayerUpdate {
            lower,
            upper: Some(&[]),
        };
        let run = |g: &mut GpuRenderer, f: &mut Fonts, lower, panes: &[PaneDraw]| {
            let t = std::time::Instant::now();
            g.compose(f, size, none(lower), panes);
            t.elapsed().as_secs_f64() * 1000.0
        };
        let lower = fx.lower.clone();
        let t0 = run(&mut g, &mut fx.f, Some(&lower), &pa);
        text.push(t0);
        first.push(run(&mut g, &mut fx.f, None, &pe));
        second.push(run(&mut g, &mut fx.f, None, &pe));
    }
    eprintln!(
        "글자 프레임(처음) {:.2}ms, 처음 본 이모지 프레임 {:.2}ms, 같은 이모지 다시 {:.2}ms (중앙값, 15회, 읽어 내기 포함)",
        median(text),
        median(first),
        median(second)
    );
}

/// 창 하나의 GPU 자원 내역(#400, 창 없이, 2000x1360, 글자 화면): `cargo test -p bt-gpu --release --test compose -- --ignored --nocapture gpu_memory_report`
#[test]
#[ignore = "내역을 찍는다(--ignored로 돌린다)"]
fn gpu_memory_report_2000x1360() {
    let Some(mut g) = gpu() else { return };
    let mut fx = fixture();
    let (w, h) = (2000u32, 1360u32);
    let lower = vec![0xff20_242bu32; (w * h) as usize];
    let (va, vb) = (view(&fx.sa, &fx.ca, &fx.t), view(&fx.sb, &fx.cb, &fx.t));
    let panes = frame_panes(&fx, &va, &vb);
    g.compose(
        &mut fx.f,
        (w, h),
        LayerUpdate {
            lower: Some(&lower),
            upper: Some(&[]),
        },
        &panes,
    );
    let m = g.gpu_memory();
    let mb = |b: usize| b as f64 / 1048576.0;
    eprintln!("{m:?}");
    eprintln!(
        "GPU 합 {:.2}MiB (층 {:.2}, 회색조 아틀라스 {:.2}, 색 아틀라스 {:.2}, 인스턴스 {:.3}, 유니폼 {:.4}), 창 없는 길 전용(그림 텍스처 + 읽기 버퍼) {:.2}MiB",
        mb(m.gpu_total() - m.offscreen_target),
        mb(m.layer_textures),
        mb(m.gray_atlas),
        mb(m.color_atlas),
        mb(m.instance_buffers),
        mb(m.uniform_buffers),
        mb(m.offscreen_target),
    );
    eprintln!(
        "CPU 합 {:.2}MiB (아틀라스 사본 {:.2}, 층 칸 해시 {:.3}, 임시 버퍼 {:.2})",
        mb(m.cpu_total()),
        mb(m.atlas_cpu),
        mb(m.layer_hashes),
        mb(m.scratch)
    );
}

/// 층 올리기 비용 재기(E8 준비, 창 없이): `cargo test -p bt-gpu --release --test compose -- --ignored --nocapture upload_cost`
#[test]
#[ignore = "시간을 재는 시험(--ignored로 돌린다)"]
fn upload_cost_2000x1360() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts();
    let (w, h) = (2000u32, 1360u32);
    let mut lower = vec![0u32; (w * h) as usize];
    for (i, p) in lower.iter_mut().enumerate() {
        *p = 0xff00_0000 | (i as u32).wrapping_mul(2654435761) >> 8;
    }
    let median = |mut v: Vec<f64>| {
        v.sort_by(|a, b| a.total_cmp(b));
        v[v.len() / 2]
    };
    let mut run = |name: &str, frames: &mut dyn FnMut(usize) -> Vec<u32>, lower: &[u32]| {
        let (mut ms, mut bytes) = (Vec::new(), Vec::new());
        for i in 0..40 {
            let up = frames(i);
            g.compose(
                &mut f,
                (w, h),
                LayerUpdate {
                    lower: Some(lower),
                    upper: Some(&up),
                },
                &[],
            );
            if i >= 5 {
                ms.push(g.timings().upload_ms);
                bytes.push(g.timings().upload_bytes as f64);
            }
        }
        eprintln!(
            "{name}: 층 올리기 중앙값 {:.3}ms, 올린 바이트 중앙값 {}",
            median(ms),
            median(bytes) as usize
        );
    };
    // 시나리오 B: 진행 막대(칸 맨 위 3줄)가 매 프레임 바뀐다
    run(
        "B 진행 막대",
        &mut |i| {
            let mut u = vec![0u32; (w * h) as usize];
            Canvas::new(&mut u, w, h).blend_rect(0, 0, 100 + (i as u32 * 37) % 1800, 6, Rgb::new(40, 200, 90), 200);
            u
        },
        &lower,
    );
    // 덧그림 없음, 층 그대로
    run("덧그림 없음 층 그대로", &mut |_| vec![0u32; (w * h) as usize], &lower);
    // 위층 전체가 매번 바뀜(흐림 두께가 바뀌는 최악)
    run(
        "위층 통째 바뀜",
        &mut |i| {
            let mut u = vec![0u32; (w * h) as usize];
            Canvas::new(&mut u, w, h).blend_rect(0, 0, w, h, Rgb::new(0, 0, 0), 20 + (i as u8 % 50));
            u
        },
        &lower,
    );
}

#[test]
fn empty_upper_slice_releases_the_upper_layer_and_it_comes_back() {
    let Some(mut g) = gpu() else { return };
    let mut fx = fixture();
    let (va, vb) = (view(&fx.sa, &fx.ca, &fx.t), view(&fx.sb, &fx.cb, &fx.t));
    let panes = [
        PaneDraw {
            pixel_shift: 0,
            font: 0,
            x: fx.l.a.0,
            y: fx.l.a.1,
            view: &va,
            clear: fx.sa.background,
        },
        PaneDraw {
            pixel_shift: 0,
            font: 0,
            x: fx.l.b.0,
            y: fx.l.b.1,
            view: &vb,
            clear: fx.sb.background,
        },
    ];
    let size = (fx.l.w, fx.l.h);
    let layer = fx.l.w as usize * fx.l.h as usize * 4;
    let with_upper = cpu_reference(&mut fx.f, &fx.l, fx.sa.background, [&va, &vb], true);
    let without_upper = cpu_reference(&mut fx.f, &fx.l, fx.sa.background, [&va, &vb], false);

    let up = LayerUpdate {
        lower: Some(&fx.lower),
        upper: Some(&fx.upper),
    };
    let got = g.compose(&mut fx.f, size, up, &panes);
    assert_close("위층이 있는 프레임", &with_upper, &got);
    let m = g.layer_memory();
    assert_eq!(m.texture_bytes, 2 * layer, "두 층 텍스처: {m:?}");
    assert!(
        m.shadow_bytes > 0 && m.shadow_bytes * 10 < 2 * layer,
        "두 층 칸 해시는 두 층 텍스처 크기의 1/10보다 작다: {m:?}"
    );
    let both = m.shadow_bytes;

    // 위층이 비었다: 텍스처와 사본을 놓고, 화면은 위층 없는 기준과 같다
    let empty = LayerUpdate {
        lower: None,
        upper: Some(&[]),
    };
    let got = g.compose(&mut fx.f, size, empty, &panes);
    assert_close("위층을 놓은 프레임", &without_upper, &got);
    let m = g.layer_memory();
    assert_eq!(m.texture_bytes, layer, "아래층 텍스처만 남는다: {m:?}");
    assert!(
        m.shadow_bytes > 0 && m.shadow_bytes < both,
        "아래층 칸 해시만 남는다: {m:?}"
    );

    // 다시 위층을 주면 돌아온다
    let back = LayerUpdate {
        lower: None,
        upper: Some(&fx.upper),
    };
    let got = g.compose(&mut fx.f, size, back, &panes);
    assert_close("위층이 돌아온 프레임", &with_upper, &got);
    assert_eq!(g.layer_memory().texture_bytes, 2 * layer);
    // 새 텍스처는 0으로 시작하므로 0이 아닌 자리만 올린다(층을 통째로 올리지 않는다)
    let sent = g.timings().upload_bytes;
    assert!(
        sent > 0 && sent < layer,
        "돌아온 위층을 {sent}바이트 올렸다(층 {layer}바이트)"
    );

    // 다시 놓고, 10×10 조각만 그린 위층을 주면 그 조각만 올린다
    g.compose(&mut fx.f, size, empty, &panes);
    let mut small = vec![0u32; fx.l.w as usize * fx.l.h as usize];
    Canvas::new(&mut small, fx.l.w, fx.l.h).fill_rect(20, 30, 10, 10, Rgb::new(255, 0, 0));
    let small_up = LayerUpdate {
        lower: None,
        upper: Some(&small),
    };
    let got = g.compose(&mut fx.f, size, small_up, &panes);
    // 조각(열 20..30)은 칸 하나(0..128)에 든다: 폭이 칸 경계로 넓어진다(#393)
    assert_eq!(g.timings().upload_bytes, 128 * 10 * 4, "조각의 칸만 올린다");
    assert_eq!(got[35 * fx.l.w as usize + 25] & 0xff_ffff, 0xff_0000, "조각이 보인다");
    assert_close("조각 밖은 위층 없는 기준", &without_upper[..20], &got[..20]);
}

#[test]
fn big_uploads_go_in_row_chunks_and_small_uploads_reuse_the_scratch() {
    let Some(mut g) = gpu() else { return };
    let mut f = fonts();
    let (w, h) = (800u32, 600u32);
    let n = (w * h) as usize;
    // 줄마다 색이 달라, 줄 묶음이 제자리에 올라갔는지 보인다
    let mut lower: Vec<u32> = (0..n)
        .map(|i| 0xff00_0000 | ((i / w as usize) as u32 % 256) << 16 | 0x2c34)
        .collect();
    let mut draw = |g: &mut GpuRenderer, lower: &[u32]| {
        let layers = LayerUpdate {
            lower: Some(lower),
            upper: None,
        };
        g.compose(&mut f, (w, h), layers, &[])
    };
    // 첫 통째 올리기(1.92MB): 줄 묶음으로 나눠 올려 임시 버퍼가 상한(1MiB)을 넘지 않는다(#388)
    let got = draw(&mut g, &lower);
    assert_eq!(g.timings().upload_bytes, n * 4);
    let want: Vec<u32> = lower.iter().map(|p| p & 0xff_ffff).collect();
    assert_close("나눠 올린 아래층", &want, &got);
    let s = g.layer_memory().scratch_bytes;
    assert!(s <= 1 << 20, "통째 올리기 뒤 임시 버퍼가 {s}바이트 남았다");

    // 작은 올리기 둘: 임시 버퍼를 쥐고, 두 번째는 다시 잡지 않는다
    lower[10 * w as usize + 10] = 0xffff_0000;
    let got = draw(&mut g, &lower);
    assert_eq!(got[10 * w as usize + 10] & 0xff_ffff, 0xff_0000, "작은 올리기가 보인다");
    let s1 = g.layer_memory().scratch_bytes;
    assert!(s1 > 0 && s1 <= 1 << 20, "작은 올리기 뒤 임시 버퍼 {s1}바이트");
    lower[20 * w as usize + 20] = 0xff00_ff00;
    draw(&mut g, &lower);
    assert_eq!(
        g.layer_memory().scratch_bytes,
        s1,
        "같은 크기의 작은 올리기는 그대로 쓴다"
    );
}

/// 시각 벨(#893): 칸 위 번쩍임(색 × 세기 × 감쇠)을 위층에 얹은 GPU 프레임 = CPU 프레임. 세기가 바뀔 때마다,
/// 그리고 번쩍임이 끝나 위층이 비었을 때 옛 번쩍임이 남지 않아야 한다
#[test]
fn visual_bell_flash_in_upper_layer_equals_the_cpu_frame() {
    use bt_core::visual_bell::{VisualBell, flash_alpha};
    let Some(mut g) = gpu() else { return };
    let mut fx = fixture();
    let (va, vb) = (view(&fx.sa, &fx.ca, &fx.t), view(&fx.sb, &fx.cb, &fx.t));
    let panes = [
        PaneDraw {
            pixel_shift: 0,
            font: 0,
            x: fx.l.a.0,
            y: fx.l.a.1,
            view: &va,
            clear: fx.sa.background,
        },
        PaneDraw {
            pixel_shift: 0,
            font: 0,
            x: fx.l.b.0,
            y: fx.l.b.1,
            view: &vb,
            clear: fx.sb.background,
        },
    ];
    let size = (fx.l.w, fx.l.h);
    // 0.8초를 적으면 실제 번쩍임은 0.4초(kitty 코드처럼 절반)
    let bell = VisualBell::parse("0.8 linear").unwrap();
    // 번쩍임이 칸 A 전체를 덮는다. 색은 선택 바탕(밝은 쪽)과 어두운 쪽 둘
    let flash = |c: &mut Canvas, l: &Layout, t: f64, color: Rgb| {
        let a = flash_alpha(bell.intensity(t), color);
        if a > 0 {
            c.blend_rect(l.a.0, l.a.1, l.grid.0, l.grid.1, color, a);
        }
    };
    for color in [Rgb::new(235, 235, 235), Rgb::new(30, 60, 160)] {
        // 시작(0) → 올라가는 중 → 꼭대기 → 내려가는 중 → 끝(위층 빔)
        for (i, t) in [0.0, 0.05, 0.1, 0.2, 0.3, 0.39, 0.4].into_iter().enumerate() {
            let mut upper = vec![0u32; (fx.l.w * fx.l.h) as usize];
            flash(&mut Canvas::new(&mut upper, fx.l.w, fx.l.h), &fx.l, t, color);
            let got = g.compose(
                &mut fx.f,
                size,
                LayerUpdate {
                    lower: Some(&fx.lower),
                    upper: Some(&upper),
                },
                &panes,
            );
            let mut want = cpu_reference(&mut fx.f, &fx.l, fx.sa.background, [&va, &vb], false);
            flash(&mut Canvas::new(&mut want, fx.l.w, fx.l.h), &fx.l, t, color);
            assert_close(&format!("번쩍임 {color:?} {i}번째(t={t})"), &want, &got);
        }
    }
}
