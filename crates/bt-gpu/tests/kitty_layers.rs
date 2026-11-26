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
        selection_bg_cell: Default::default(),
        selection_fg_cell: Default::default(),
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

// ---- 유니코드 자리표시(#553) ----

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for ch in data.chunks(3) {
        let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            s.push(if i <= ch.len() {
                T[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    s
}

/// 화면 값에서 렌더러 셀로. `raw`면 자리표시 글자와 결합 문자를 그대로(그리지 않으면 안 되는 쪽의 대조), 아니면 앱(`to_draw_cells`)처럼 빈칸으로
fn placeholder_cells(s: &Screen, raw: bool) -> Vec<DrawCell> {
    let ch = |c: &bt_term::Cell| if raw { c.ch } else { c.draw_ch() };
    let mut cells: Vec<DrawCell> = s
        .cells
        .iter()
        .map(|c| DrawCell {
            ch: ch(c),
            fg: c.fg,
            bg: c.bg,
            bold: false,
            italic: false,
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
            hidden: false,
            faint: None,
        })
        .collect();
    cluster::mark(&mut cells, s.cols, |i| {
        let c = &s.cells[i];
        let comb: &[char] = if raw { &c.combining } else { c.draw_combining() };
        cluster::ClusterCell {
            ch: ch(c),
            combining: comb,
            spacer: c.flags.contains(CellFlags::SPACER),
        }
    });
    cells
}

#[test]
fn unicode_placeholders_draw_the_image_on_both_paths_and_never_the_glyph() {
    let Some(mut g) = GpuRenderer::new_headless() else {
        eprintln!("건너뜀: GPU 어댑터가 없다");
        return;
    };
    let mut f = fonts();
    let m = f.metrics();
    let (cw, ch) = (m.width as i32, m.height as i32);
    let theme = Theme::default();
    let mut e = Emulator::new(
        GridSize {
            cols: COLS,
            rows: ROWS,
            cell_width: m.width as u16,
            cell_height: m.height as u16,
        },
        theme.clone(),
    );
    // 왼쪽 반 빨강, 오른쪽 반 파랑인 8x4 RGBA 그림을 4칸 x 2줄 가상 배치로: 줄마다 자리표시 4칸(둘째 줄은 줄·칸 결합 문자)
    let img = halves();
    e.feed(format!("\x1b_Ga=t,f=32,s=8,v=4,i=1,q=2;{}\x1b\\", b64(&img)).as_bytes());
    e.feed(b"\x1b_Ga=p,i=1,U=1,q=2,c=4,r=2\x1b\\");
    let dia = ['\u{0305}', '\u{030D}', '\u{030E}', '\u{0310}'];
    let mut text = String::from("\x1b[38;5;1m");
    for r in 0..2 {
        text += &format!("\x1b[{};3H", r + 2);
        for c in 0..4 {
            text += &format!("\u{10EEEE}{}{}", dia[r], dia[c]);
        }
    }
    e.feed(text.as_bytes());
    let screen = e.snapshot();
    assert_eq!(screen.images.len(), 2, "줄마다 한 조각");
    for im in &screen.images {
        eprintln!(
            "조각 row={} col={} cols={} geom={:?} resolve={:?}",
            im.row,
            im.col,
            im.cols,
            im.geom,
            im.geom.resolve(m.width, m.height)
        );
    }
    assert!(screen.cell(2, 1).flags.contains(CellFlags::PLACEHOLDER));
    let paints: Vec<ImagePaint> = screen
        .images
        .iter()
        .map(|im| ImagePaint {
            rgba: &im.rgba,
            geom: im.geom,
            row: im.row,
            col: im.col,
            z: im.z,
        })
        .collect();

    let origin = (13, 41);
    let win = (
        origin.0 as u32 + COLS as u32 * m.width + 17,
        origin.1 as u32 + ROWS as u32 * m.height + 23,
    );
    let (w, h) = win;
    let n = (w * h) as usize;
    let clip = (
        origin.0,
        origin.1,
        origin.0 + COLS as i32 * cw,
        origin.1 + ROWS as i32 * ch,
    );
    let paint = |c: &mut Canvas, layer| paint_layer(c, &paints, layer, origin, (m.width, m.height), clip);
    let lower_base = |c: &mut Canvas| {
        c.clear(WIN_BG);
        c.fill_rect(
            origin.0 - 6,
            origin.1 - 6,
            COLS as u32 * m.width + 12,
            ROWS as u32 * m.height + 12,
            screen.background,
        );
    };

    // CPU 길(앱 `to_draw_cells`와 같은 빈칸)
    let dc = placeholder_cells(&screen, false);
    fn gv<'a>(screen: &Screen, theme: &Theme, dc: &'a [DrawCell]) -> GridView<'a> {
        GridView {
            cols: screen.cols,
            rows: screen.rows,
            cells: dc,
            cursor: None,
            selection_bg: theme.selection_background,
            selection_fg: theme.selection_foreground,
            selection_bg_cell: Default::default(),
            selection_fg_cell: Default::default(),
            preedit: None,
            min_contrast: 1.0,
        }
    }
    let render_cpu = |f: &mut Fonts, dc: &[DrawCell], with_images: bool| -> Vec<u32> {
        let mut buf = vec![0u32; n];
        let mut c = Canvas::new(&mut buf, w, h);
        lower_base(&mut c);
        let view = gv(&screen, &theme, dc);
        let below_text = |c: &mut Canvas| paint(c, Layer::BelowText);
        let under = Under {
            below_bg: None,
            below_text: (with_images && has_layer(&paints, Layer::BelowText))
                .then_some(&below_text as &dyn Fn(&mut Canvas)),
            default_bg: Some(screen.background),
        };
        draw_grid_under(&mut c, f, origin.0, origin.1, &view, &under);
        buf
    };
    let want = render_cpu(&mut f, &dc, true);

    // GPU 길: 글자 아래 층(가운데)에 조각들
    let (mut lower, mut mid, upper) = (vec![0u32; n], vec![0u32; n], vec![0u32; n]);
    lower_base(&mut Canvas::new(&mut lower, w, h));
    paint(&mut Canvas::new(&mut mid, w, h), Layer::BelowText);
    let view = gv(&screen, &theme, &dc);
    let panes = [PaneDraw {
        font: 0,
        x: origin.0,
        y: origin.1,
        view: &view,
        clear: screen.background,
    }];
    let got = g.compose_layers(
        &mut [&mut f],
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
    eprintln!("자리표시: 다른 픽셀 {nd}/{n}, 차이가 1을 넘는 픽셀 {over}, 최대 차이 {max}");
    assert!(max <= 3, "CPU 길과 GPU 길이 다르다: 최대 차이 {max}");
    assert!(over * 1000 < n * 5, "차이가 1을 넘는 픽셀 {over}/{n}");

    // 그림이 그 칸들에 그려졌다: 8x4 그림(2:1)을 4x2칸 격자에 맞추면 왼쪽 반은 빨강 오른쪽 반은 파랑
    let red = Rgb::new(255, 0, 0);
    let blue = Rgb::new(0, 0, 255);
    // 그림은 격자 가운데에 비율을 지켜 앉는다: 위 줄 조각은 칸 아래쪽 16px, 아래 줄 조각은 칸 위쪽 16px에 있다
    let at = |col: i32, dy: i32, row: i32, buf: &[u32]| {
        pixel(buf, w, origin.0 + col * cw + cw / 2, origin.1 + row * ch + dy)
    };
    for (name, buf) in [("CPU", &want), ("GPU", &got)] {
        let top: Vec<Rgb> = (2..6).map(|c| at(c, ch - 4, 1, buf)).collect();
        let bottom: Vec<Rgb> = (2..6).map(|c| at(c, 4, 2, buf)).collect();
        assert_eq!(top, [red, red, blue, blue], "{name}: 위 줄 조각");
        assert_eq!(bottom, [red, red, blue, blue], "{name}: 아래 줄 조각");
        // 그림 밖(맞추고 남은 위아래 여백)은 칸 바탕이다
        assert_eq!(at(3, 2, 1, buf), screen.background, "{name}: 위 줄 조각 위 여백");
    }

    // 자리표시 칸 영역에서 그림·바탕이 아닌 픽셀(= 글자 모양)이 없다. 같은 장면을 글자 그대로 그리면 있다(대조: 이 검사가 글자를 알아본다)
    let bg = screen.background;
    let odd = |buf: &[u32]| -> usize {
        let mut count = 0;
        for row in 1..3 {
            for dx in 0..4 * cw {
                for dy in 0..ch {
                    let p = pixel(buf, w, origin.0 + 2 * cw + dx, origin.1 + row * ch + dy);
                    // 그림 가장자리의 보간으로 섞인 색은 빨강·파랑·바탕 사이의 색이다: 초록 성분이 바탕과 다르게 튀면 글자다
                    if p != red && p != blue && p != bg && p.g > bg.g.max(8) + 24 {
                        count += 1;
                    }
                }
            }
        }
        count
    };
    assert_eq!(odd(&want), 0, "CPU: 자리표시 글자를 그리면 안 된다");
    assert_eq!(odd(&got), 0, "GPU: 자리표시 글자를 그리면 안 된다");
    let raw_dc = placeholder_cells(&screen, true);
    let raw_cpu = render_cpu(&mut f, &raw_dc, true);
    eprintln!("글자 그대로 그린 대조: 글자 픽셀 {}", odd(&raw_cpu));
    assert_ne!(
        raw_cpu, want,
        "대조: 글자를 그대로 그리면 프레임이 달라야 한다(빈 네모가 찍힌다)"
    );
}

// ---- 스크롤 영역에서 잘린 그림(#800) ----

/// 영역 스크롤에 위쪽이 잘린 그림(`geom.src`가 아래 반쪽)을 CPU 길과 GPU 길이 같은 픽셀로 그린다. 잘린 부분의 빨강은 없고 남은 파랑만 있다
#[test]
fn scroll_clipped_placement_draws_the_same_on_both_paths() {
    let Some(mut g) = GpuRenderer::new_headless() else {
        eprintln!("건너뜀: GPU 어댑터가 없다");
        return;
    };
    let mut f = fonts();
    let m = f.metrics();
    let (cw, ch) = (m.width as i32, m.height as i32);
    let theme = Theme::default();
    let mut e = Emulator::new(
        GridSize {
            cols: COLS,
            rows: ROWS,
            cell_width: m.width as u16,
            cell_height: m.height as u16,
        },
        theme.clone(),
    );
    // 위 반은 빨강, 아래 반은 파랑인 8x8 그림을 2칸 x 2줄로 줄 2에 놓는다. 영역은 1~3줄이고 맨 아래 줄에서 IND를 두 번 보낸다:
    // 한 번은 한 줄 위로 올라가고, 두 번째는 위 여백을 넘어 위 한 줄(빨강 반쪽)이 잘린다
    let mut img = Vec::new();
    for y in 0..8 {
        for _ in 0..8 {
            img.extend_from_slice(if y < 4 { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
        }
    }
    e.feed(b"\x1b[3;4H");
    e.feed(format!("\x1b_Ga=T,f=32,s=8,v=8,i=1,c=2,r=2,C=1,q=2;{}\x1b\\", b64(&img)).as_bytes());
    e.feed(b"\x1b[2;4r\x1b[4;1H\x1bD\x1bD");
    let screen = e.snapshot();
    assert_eq!(screen.images.len(), 1);
    let im = &screen.images[0];
    assert_eq!(
        (im.row, im.col, im.rows, im.geom.src),
        (1, 3, 1, [0, 4, 8, 4]),
        "위 한 줄이 잘렸다"
    );
    let paints = [ImagePaint {
        rgba: &im.rgba,
        geom: im.geom,
        row: im.row,
        col: im.col,
        z: im.z,
    }];

    let origin = (13, 41);
    let (w, h) = (
        origin.0 as u32 + COLS as u32 * m.width + 17,
        origin.1 as u32 + ROWS as u32 * m.height + 23,
    );
    let n = (w * h) as usize;
    let clip = (
        origin.0,
        origin.1,
        origin.0 + COLS as i32 * cw,
        origin.1 + ROWS as i32 * ch,
    );
    let paint = |c: &mut Canvas, layer| paint_layer(c, &paints, layer, origin, (m.width, m.height), clip);
    let lower_base = |c: &mut Canvas| {
        c.clear(WIN_BG);
        c.fill_rect(
            origin.0 - 6,
            origin.1 - 6,
            COLS as u32 * m.width + 12,
            ROWS as u32 * m.height + 12,
            screen.background,
        );
    };
    let dc = cells(&screen);
    let view = GridView {
        cols: screen.cols,
        rows: screen.rows,
        cells: &dc,
        cursor: None,
        selection_bg: theme.selection_background,
        selection_fg: theme.selection_foreground,
        selection_bg_cell: Default::default(),
        selection_fg_cell: Default::default(),
        preedit: None,
        min_contrast: 1.0,
    };
    // CPU 길
    let mut want = vec![0u32; n];
    {
        let mut c = Canvas::new(&mut want, w, h);
        lower_base(&mut c);
        let under = Under {
            below_bg: None,
            below_text: None,
            default_bg: Some(screen.background),
        };
        draw_grid_under(&mut c, &mut f, origin.0, origin.1, &view, &under);
        paint(&mut c, Layer::AboveText);
    }
    // GPU 길: 그림은 위층
    let (mut lower, mid, mut upper) = (vec![0u32; n], vec![0u32; n], vec![0u32; n]);
    lower_base(&mut Canvas::new(&mut lower, w, h));
    paint(&mut Canvas::new(&mut upper, w, h), Layer::AboveText);
    let panes = [PaneDraw {
        font: 0,
        x: origin.0,
        y: origin.1,
        view: &view,
        clear: screen.background,
    }];
    let got = g.compose_layers(
        &mut [&mut f],
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
    eprintln!("잘린 그림: 다른 픽셀 {nd}/{n}, 차이가 1을 넘는 픽셀 {over}, 최대 차이 {max}");
    assert!(max <= 3, "CPU 길과 GPU 길이 다르다: 최대 차이 {max}");
    assert!(over * 1000 < n * 5, "차이가 1을 넘는 픽셀 {over}/{n}");
    // 잘린 그림은 줄 1의 두 칸(3·4열)에 남은 파랑 반쪽만 늘려 그린다. 빨강은 없다
    for (name, buf) in [("CPU", &want), ("GPU", &got)] {
        let p = pixel(buf, w, origin.0 + 3 * cw + cw / 2, origin.1 + ch + ch / 2);
        assert_eq!(p, Rgb::new(0, 0, 255), "{name}: 잘리고 남은 파랑");
        let any_red =
            (0..2 * cw).any(|dx| (0..ch).any(|dy| pixel(buf, w, origin.0 + 3 * cw + dx, origin.1 + ch + dy).r > 200));
        assert!(!any_red, "{name}: 잘린 빨강 반쪽이 보이면 안 된다");
    }
}
