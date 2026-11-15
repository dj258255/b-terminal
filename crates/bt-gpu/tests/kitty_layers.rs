//! kitty 그림의 층(#547): 칸 바탕 아래(z < i32::MIN / 2) → 칸 바탕 → 글자 아래(z < 0) → 글자 → 글자 위. 잘라내기·오프셋·늘리기도 함께.
//! GPU 길(아래층·가운데 층·위층 + 칸 바탕·글자)로 합친 프레임이 CPU 길(`draw_grid_under` + 위 그림)과 같은 픽셀이어야 한다.
//! GPU 어댑터가 없으면 알리고 끝낸다

use bt_core::kitty_geom::{Geom, Layer};
use bt_core::{Rgb, Theme};
use bt_gpu::{GpuRenderer, LayerUpdate, PaneDraw};
use bt_render::kitty::{ImagePaint, has_layer, paint_layer};
use bt_render::{Canvas, DrawCell, FontConfig, Fonts, GridView, Under, Underline, cluster, draw_grid_under};
use bt_term::{CellFlags, Emulator, GridSize, Screen};

fn fonts() -> Fonts {
    Fonts::new(&FontConfig {
        scale: 2.0,
        ..FontConfig::default()
    })
    .expect("넣어 둔 글꼴은 늘 있다")
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
            emoji: false,
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

const COLS: u16 = 16;
const ROWS: u16 = 4;
const WIN_BG: Rgb = Rgb {
    r: 0x20,
    g: 0x24,
    b: 0x2b,
};
const GREEN: [u8; 4] = [0, 200, 0, 255];
const MAGENTA: [u8; 4] = [220, 0, 220, 255];

fn solid(rgba: [u8; 4], w: u32, h: u32) -> Vec<u8> {
    (0..w * h).flat_map(|_| rgba).collect()
}

/// 왼쪽 반은 빨강, 오른쪽 반은 파랑인 8×4 그림(잘라내기를 눈으로 가리려고)
fn halves() -> Vec<u8> {
    let mut v = Vec::new();
    for _ in 0..4 {
        for x in 0..8 {
            v.extend_from_slice(if x < 4 { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
        }
    }
    v
}

struct Scene {
    f: Fonts,
    screen: Screen,
    dc: Vec<DrawCell>,
    theme: Theme,
    origin: (i32, i32),
    win: (u32, u32),
}

fn scene() -> Scene {
    let f = fonts();
    let m = f.metrics();
    let theme = Theme::default();
    let mut e = Emulator::new(GridSize::new(COLS, ROWS), theme.clone());
    // 0줄: 기본 바탕 빈 줄. 1줄: 파란 바탕의 빈칸. 2줄: 파란 바탕 위의 글자. 3줄: 기본 바탕 위의 글자
    e.feed(b"\r\n\x1b[44m                \x1b[0m\r\n\x1b[44;97mWWWWWWWWWWWWWWWW\x1b[0m\r\ngjpqy Text");
    let screen = e.snapshot();
    let dc = cells(&screen);
    let origin = (13, 41);
    let win = (
        origin.0 as u32 + COLS as u32 * m.width + 17,
        origin.1 as u32 + ROWS as u32 * m.height + 23,
    );
    Scene {
        f,
        screen,
        dc,
        theme,
        origin,
        win,
    }
}

fn gv<'a>(s: &Scene, dc: &'a [DrawCell]) -> GridView<'a> {
    GridView {
        cols: s.screen.cols,
        rows: s.screen.rows,
        cells: dc,
        cursor: None,
        selection_bg: s.theme.selection_background,
        selection_fg: s.theme.selection_foreground,
        preedit: None,
        min_contrast: 1.0,
    }
}

fn geom(w: u32, h: u32) -> Geom {
    Geom {
        img_w: w,
        img_h: h,
        ..Geom::default()
    }
}

fn pixel(buf: &[u32], w: u32, x: i32, y: i32) -> Rgb {
    let p = buf[(y as u32 * w + x as u32) as usize];
    Rgb::new((p >> 16) as u8, (p >> 8) as u8, p as u8)
}

fn diff(a: &[u32], b: &[u32]) -> (usize, i32, usize) {
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
    (n, max, over)
}

#[test]
fn gpu_layers_match_the_cpu_path_and_stack_in_ghostty_order() {
    let Some(mut g) = GpuRenderer::new_headless() else {
        eprintln!("건너뜀: GPU 어댑터가 없다");
        return;
    };
    let mut sc = scene();
    let m = sc.f.metrics();
    let (cw, ch) = (m.width as i32, m.height as i32);
    let green = solid(GREEN, 4, 4);
    let magenta = solid(MAGENTA, 4, 4);
    let two = halves();
    let paints = [
        // 칸 바탕 아래: 칸 (2, 0)부터 6칸 x 2줄(0줄 기본 바탕, 1줄 파란 바탕)
        ImagePaint {
            rgba: &green,
            geom: Geom {
                want_cols: 6,
                want_rows: 2,
                ..geom(4, 4)
            },
            row: 0,
            col: 2,
            z: i32::MIN,
        },
        // 글자 아래: 칸 (4, 1)부터 4칸 x 2줄(파란 바탕 빈칸 줄과 글자 줄)
        ImagePaint {
            rgba: &magenta,
            geom: Geom {
                want_cols: 4,
                want_rows: 2,
                ..geom(4, 4)
            },
            row: 1,
            col: 4,
            z: -1,
        },
        // 글자 위: 오른쪽 반(파랑)만 잘라 칸 (10, 2)에서 오프셋 (3, 2)
        ImagePaint {
            rgba: &two,
            geom: Geom {
                src: [4, 0, 4, 4],
                off: [3, 2],
                ..geom(8, 4)
            },
            row: 2,
            col: 10,
            z: 5,
        },
    ];
    let clip = (
        sc.origin.0,
        sc.origin.1,
        sc.origin.0 + COLS as i32 * cw,
        sc.origin.1 + ROWS as i32 * ch,
    );
    let (w, h) = sc.win;
    let n = (w * h) as usize;
    let lower_base = |c: &mut Canvas| {
        c.clear(WIN_BG);
        c.fill_rect(
            sc.origin.0 - 6,
            sc.origin.1 - 6,
            COLS as u32 * m.width + 12,
            ROWS as u32 * m.height + 12,
            sc.screen.background,
        );
    };
    let paint = |c: &mut Canvas, layer| paint_layer(c, &paints, layer, sc.origin, (m.width, m.height), clip);

    // CPU 길: 칸 바탕 아래 → 칸 바탕 → 글자 아래 → 글자 → 글자 위
    let mut want = vec![0u32; n];
    {
        let mut c = Canvas::new(&mut want, w, h);
        lower_base(&mut c);
        let view = gv(&sc, &sc.dc);
        let below_bg = |c: &mut Canvas| paint(c, Layer::BelowBackground);
        let below_text = |c: &mut Canvas| paint(c, Layer::BelowText);
        let under = Under {
            below_bg: has_layer(&paints, Layer::BelowBackground).then_some(&below_bg as &dyn Fn(&mut Canvas)),
            below_text: has_layer(&paints, Layer::BelowText).then_some(&below_text as &dyn Fn(&mut Canvas)),
            default_bg: Some(sc.screen.background),
        };
        draw_grid_under(&mut c, &mut sc.f, sc.origin.0, sc.origin.1, &view, &under);
        paint(&mut c, Layer::AboveText);
    }

    // GPU 길: 아래층(+칸 바탕 아래 그림), 가운데 층(글자 아래 그림), 위층(글자 위 그림)
    let (mut lower, mut mid, mut upper) = (vec![0u32; n], vec![0u32; n], vec![0u32; n]);
    lower_base(&mut Canvas::new(&mut lower, w, h));
    paint(&mut Canvas::new(&mut lower, w, h), Layer::BelowBackground);
    paint(&mut Canvas::new(&mut mid, w, h), Layer::BelowText);
    paint(&mut Canvas::new(&mut upper, w, h), Layer::AboveText);
    let view = gv(&sc, &sc.dc);
    let panes = [PaneDraw {
        font: 0,
        x: sc.origin.0,
        y: sc.origin.1,
        view: &view,
        clear: sc.screen.background,
    }];
    let got = g.compose_layers(
        &mut [&mut sc.f],
        (w, h),
        LayerUpdate {
            lower: Some(&lower),
            upper: Some(&upper),
        },
        None,
        Some(&mid),
        &panes,
    );

    let (nd, max, over) = diff(&want, &got);
    eprintln!("kitty 층: 다른 픽셀 {nd}/{n}, 차이가 1을 넘는 픽셀 {over}, 최대 차이 {max}");
    assert!(max <= 3, "CPU 길과 GPU 길이 다르다: 최대 차이 {max}");
    assert!(over * 1000 < n * 5, "차이가 1을 넘는 픽셀 {over}/{n}");

    // 층 차례의 증거(두 길 모두):
    let at = |col: i32, row: i32, buf: &[u32]| pixel(buf, w, sc.origin.0 + col * cw + 1, sc.origin.1 + row * ch + 1);
    let (green, magenta) = (Rgb::new(0, 200, 0), Rgb::new(220, 0, 220));
    let blue_bg = sc.dc[(COLS as usize) + 3].bg; // 1줄의 파란 바탕 빈칸
    assert_ne!(blue_bg, sc.screen.background);
    for (name, buf) in [("CPU", &want), ("GPU", &got)] {
        // 칸 바탕 아래: 기본 바탕 칸(0줄)에는 그림이 비치고
        assert_eq!(
            at(3, 0, buf),
            green,
            "{name}: 기본 바탕 칸에 칸 바탕 아래 그림이 비친다"
        );
        // 파란 바탕 칸(1줄, 글자 아래 그림 밖)은 칸 바탕이 그림을 덮는다
        assert_eq!(
            at(3, 1, buf),
            blue_bg,
            "{name}: 색 바탕 칸은 칸 바탕 아래 그림을 덮는다"
        );
        // 글자 아래 그림은 칸 바탕 위에 있다
        assert_eq!(
            at(5, 1, buf),
            magenta,
            "{name}: 글자 아래 그림은 파란 칸 바탕 위에 있다"
        );
        // 잘라낸 파랑 반쪽이 칸 안 오프셋 (3, 2)에서 시작하고(그 앞은 칸 바탕), 빨강은 없다
        let (x0, y0) = (sc.origin.0 + 10 * cw + 3, sc.origin.1 + 2 * ch + 2);
        assert_eq!(
            pixel(buf, w, x0 + 1, y0 + 1),
            Rgb::new(0, 0, 255),
            "{name}: 잘라낸 오른쪽 반"
        );
        assert_ne!(
            pixel(buf, w, x0 - 1, y0 + 1),
            Rgb::new(0, 0, 255),
            "{name}: 오프셋 앞은 그림이 아니다"
        );
        assert_ne!(
            pixel(buf, w, x0 + 1, y0 - 1),
            Rgb::new(0, 0, 255),
            "{name}: 오프셋 위는 그림이 아니다"
        );
        assert_ne!(
            pixel(buf, w, x0 + 1, y0 + 1),
            Rgb::new(255, 0, 0),
            "{name}: 빨강은 잘라냈다"
        );
    }
    // 글자 아래 그림 위에 글자가 있다: 2줄은 파란 바탕에 흰 'W'. 그림이 글자를 덮었다면 글자 픽셀이 없다
    let row2_text = (0..cw).any(|dx| {
        (0..ch).any(|dy| {
            let p = pixel(&got, w, sc.origin.0 + 5 * cw + dx, sc.origin.1 + 2 * ch + dy);
            p.r > 200 && p.g > 200 && p.b > 200
        })
    });
    assert!(row2_text, "글자 아래 그림 위에 글자가 보여야 한다");
}
