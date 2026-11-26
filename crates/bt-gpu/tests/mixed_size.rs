//! 칸마다 글자 크기가 달라도 한 프레임이 맞게 그려진다(#422). 장면 시험은 GPU 없이, 픽셀 시험은 어댑터가 없으면 알리고 끝낸다

use bt_core::{Rgb, Theme};
use bt_gpu::glyphs::GlyphCache;
use bt_gpu::scene::Scene;
use bt_gpu::{GpuRenderer, LayerUpdate, PaneDraw};
use bt_render::{Canvas, DrawCell, FontConfig, Fonts, GridView, Underline, draw_grid};

const BG: Rgb = Rgb { r: 30, g: 32, b: 40 };
const FG: Rgb = Rgb { r: 220, g: 220, b: 210 };

fn fonts_at(size: f32) -> Fonts {
    Fonts::new(&FontConfig {
        size,
        scale: 1.0,
        ..FontConfig::default()
    })
    .expect("넣어 둔 글꼴은 늘 있다")
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
    }
}

fn row(text: &str, cols: usize) -> Vec<DrawCell> {
    let mut v: Vec<DrawCell> = text.chars().map(cell).collect();
    v.resize(cols, cell(' '));
    v
}

fn view(cells: &[DrawCell], cols: usize) -> GridView<'_> {
    let t = Theme::default();
    GridView {
        cols,
        rows: cells.len() / cols,
        cells,
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

/// 크기가 다른 두 칸을 번갈아 만들어도 글리프 캐시가 서로의 글리프를 지우지 않는다.
/// 지우면 같은 프레임의 앞 칸 인스턴스가 가리키는 아틀라스 자리를 뒷 칸이 덮어쓴다
#[test]
fn glyph_cache_keeps_both_sizes_while_they_alternate() {
    let (mut small, mut big) = (fonts_at(13.0), fonts_at(26.0));
    assert_ne!(
        small.metrics().width,
        big.metrics().width,
        "시험 크기는 칸 크기가 달라야 한다"
    );
    let cells = row("Hello, 칸", 8);
    let v = view(&cells, 8);
    let mut cache = GlyphCache::new(1024, 4096);
    let mut scene = Scene::default();

    scene.build(&mut cache, &mut small, &v, BG);
    let small_first = scene.glyphs.clone();
    scene.build(&mut cache, &mut big, &v, BG);
    let big_first = scene.glyphs.clone();
    assert_ne!(small_first, big_first, "크기가 다르면 글리프 인스턴스도 달라야 한다");
    cache.uploads.clear();

    // 다시 작은 크기: 캐시에 그대로 있어 새로 그리지도 올리지도 않고, 같은 자리를 가리킨다
    scene.build(&mut cache, &mut small, &v, BG);
    assert!(
        cache.uploads.is_empty(),
        "작은 크기 글리프를 다시 그렸다: {}개",
        cache.uploads.len()
    );
    assert_eq!(scene.glyphs, small_first);
    scene.build(&mut cache, &mut big, &v, BG);
    assert!(
        cache.uploads.is_empty(),
        "큰 크기 글리프를 다시 그렸다: {}개",
        cache.uploads.len()
    );
    assert_eq!(scene.glyphs, big_first);
}

/// 같은 글자라도 크기마다 아틀라스 자리와 크기가 따로다(열쇠에 글자 크기가 든다)
#[test]
fn glyph_keys_differ_by_size() {
    let (mut small, mut big) = (fonts_at(13.0), fonts_at(26.0));
    let cells = row("A", 2);
    let v = view(&cells, 2);
    let mut cache = GlyphCache::new(1024, 4096);
    let mut scene = Scene::default();
    scene.build(&mut cache, &mut small, &v, BG);
    let a = scene.glyphs.clone();
    scene.build(&mut cache, &mut big, &v, BG);
    let b = scene.glyphs.clone();
    assert_eq!(cache.size_count(), 2);
    let (ga, gb) = (
        a.iter().find(|g| g.kind == 0).unwrap(),
        b.iter().find(|g| g.kind == 0).unwrap(),
    );
    assert_ne!(ga.atlas, gb.atlas, "두 크기의 `A`가 같은 아틀라스 자리를 쓴다");
    assert!(
        gb.size[0] > ga.size[0] && gb.size[1] > ga.size[1],
        "큰 글꼴의 글리프가 더 커야 한다"
    );
}

/// 프레임이 더는 쓰지 않는 크기가 있으면 캐시를 비운다(크기를 바꾼 뒤 옛 크기 글리프가 아틀라스를 채우고 있지 않게)
#[test]
fn begin_frame_forgets_sizes_no_longer_used() {
    use bt_gpu::glyphs::SizeSig;
    let (small, big) = (fonts_at(13.0), fonts_at(26.0));
    let (s, b) = (SizeSig::of(&small), SizeSig::of(&big));
    let mut cache = GlyphCache::new(512, 2048);
    cache.begin_frame(&[s, b]);
    assert_eq!(cache.size_count(), 2);
    cache.begin_frame(&[s, b]);
    assert_eq!(cache.size_count(), 2);
    // 큰 크기를 안 쓰는 프레임: 큰 크기를 잊는다
    cache.begin_frame(&[s]);
    assert_eq!(cache.size_count(), 1);
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

/// 크기가 다른 두 칸을 한 창에 나란히 놓고 칸마다 CPU `draw_grid`(그 칸의 크기)와 견준다: GPU 길도 칸마다 자기 크기로 그린다
#[test]
fn two_panes_of_different_sizes_match_cpu_per_pane() {
    let Some(mut g) = GpuRenderer::with_atlas_size(1024) else {
        eprintln!("건너뜀: GPU 어댑터가 없다");
        return;
    };
    let (mut small, mut big) = (fonts_at(13.0), fonts_at(26.0));
    let (ms, mb) = (small.metrics(), big.metrics());
    // 두 칸에 같은 글자가 있어야 크기가 열쇠에 없을 때 서로의 글리프를 잘못 쓰는 것이 드러난다
    let ca = row("abc 칸 Hello", 12);
    let cb = row("Hello 칸 abc", 12);
    let (va, vb) = (view(&ca, 12), view(&cb, 12));
    let (wa, ha) = (ms.width * 12, ms.height);
    let (wb, hb) = (mb.width * 12, mb.height);
    let gap = 4;
    let (w, h) = (wa + gap + wb, ha.max(hb));
    let panes = [
        PaneDraw {
            font: 0,
            x: 0,
            y: 0,
            view: &va,
            clear: BG,
        },
        PaneDraw {
            font: 1,
            x: (wa + gap) as i32,
            y: 0,
            view: &vb,
            clear: BG,
        },
    ];
    let bg = 0xff00_0000 | (BG.r as u32) << 16 | (BG.g as u32) << 8 | BG.b as u32;
    let lower = vec![bg; (w * h) as usize];
    let layers = LayerUpdate {
        lower: Some(&lower),
        upper: Some(&[]),
    };
    let got = g.compose_with(&mut [&mut small, &mut big], (w, h), layers, &panes);
    let region = |buf: &[u32], x0: u32, rw: u32, rh: u32| -> Vec<u32> {
        (0..rh)
            .flat_map(|y| (0..rw).map(move |x| buf[(y * w + x0 + x) as usize] & 0x00ff_ffff))
            .collect()
    };
    for (name, f, v, x0, rw, rh) in [
        ("작은 칸", &mut small, &va, 0, wa, ha),
        ("큰 칸", &mut big, &vb, wa + gap, wb, hb),
    ] {
        let want: Vec<u32> = cpu_frame(f, v).iter().map(|p| p & 0x00ff_ffff).collect();
        let have = region(&got, x0, rw, rh);
        let diff = want.iter().zip(&have).filter(|(a, b)| a != b).count();
        eprintln!("{name}: CPU와 다른 픽셀 {diff}/{}", want.len());
        assert!(diff * 1000 < want.len() * 5, "{name}: 다른 픽셀 {diff}/{}", want.len());
    }
}

/// 아틀라스에서 장면의 첫 글자(`kind == 0`) 그림을 잘라 온다
fn first_glyph_pixels(scene: &Scene, cache: &GlyphCache) -> Vec<u8> {
    let g = scene.glyphs.iter().find(|g| g.kind == 0).expect("글자 하나는 있다");
    let (x, y) = (g.atlas[0] as usize, g.atlas[1] as usize);
    let (w, h) = (g.size[0] as usize, g.size[1] as usize);
    let side = cache.gray.size() as usize;
    let data = cache.gray.data();
    (y..y + h)
        .flat_map(|r| data[r * side + x..r * side + x + w].iter().copied())
        .collect()
}

/// 설정을 다시 읽어 칸 치수는 그대로이고 그림만 바뀐 글꼴(`font-thicken`)로 바꾸면, 같은 캐시라도 새 글꼴의 그림을 쓴다
/// (#754, Ghostty #15280: 격자를 다시 만들면 셰이퍼 캐시를 주소가 아닌 번호로 가린다)
#[test]
fn new_fonts_with_same_metrics_do_not_reuse_old_glyphs() {
    use bt_gpu::glyphs::SizeSig;
    let plain = fonts_at(26.0);
    let thick = Fonts::new(&FontConfig {
        size: 26.0,
        scale: 1.0,
        thicken: true,
        thicken_strength: 255,
        ..FontConfig::default()
    })
    .expect("넣어 둔 글꼴은 늘 있다");
    assert_eq!(plain.metrics(), thick.metrics(), "시험 전제: 칸 치수가 같다");
    let (mut plain, mut thick) = (plain, thick);
    let cells = row("A", 1);
    let v = view(&cells, 1);

    let mut fresh = GlyphCache::new(512, 2048);
    let mut scene = Scene::default();
    fresh.begin_frame(&[SizeSig::of(&thick)]);
    scene.build(&mut fresh, &mut thick, &v, BG);
    let want = first_glyph_pixels(&scene, &fresh);

    let mut shared = GlyphCache::new(512, 2048);
    shared.begin_frame(&[SizeSig::of(&plain)]);
    scene.build(&mut shared, &mut plain, &v, BG);
    let old = first_glyph_pixels(&scene, &shared);
    assert_ne!(old, want, "시험 전제: 두껍게 한 그림이 다르다");
    shared.begin_frame(&[SizeSig::of(&thick)]);
    scene.build(&mut shared, &mut thick, &v, BG);
    assert_eq!(
        first_glyph_pixels(&scene, &shared),
        want,
        "옛 글꼴의 `A` 그림을 그대로 썼다"
    );
}
