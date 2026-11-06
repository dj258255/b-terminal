//! 시스템 글꼴을 찾아 글리프를 래스터하고 캐시한다(ADR-002)

use std::collections::HashMap;
use std::rc::Rc;

use fontdb::{Database, Family, Query, Style, Weight};

/// 넣어 둔 글꼴(Ghostty가 기본으로 넣어 두는 것과 같다. assets/fonts/README.md)
const EMBEDDED: &[&[u8]] = &[
    include_bytes!("../../../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../../../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
    include_bytes!("../../../assets/fonts/SymbolsNerdFontMono-Regular.ttf"),
];

/// 넣어 둔 기본 글꼴 이름(설정에 font-family가 없을 때. Ghostty 기본과 같다)
pub const DEFAULT_FAMILY: &str = "JetBrains Mono";
const NERD_SYMBOLS: &str = "Symbols Nerd Font Mono";

/// 셀 크기 보정
pub use bt_core::config::Adjust;

/// 글꼴 설정
#[derive(Debug, Clone, PartialEq)]
pub struct FontConfig {
    /// 기본 글꼴 이름. 없거나 못 찾으면 넣어 둔 JetBrains Mono, 그다음 고정폭 후보를 차례로 찾는다
    pub family: Option<String>,
    /// 굵게·기울임·굵은 기울임에 따로 쓸 글꼴(없으면 기본 글꼴의 그 꼴)
    pub family_bold: Option<String>,
    pub family_italic: Option<String>,
    pub family_bold_italic: Option<String>,
    pub adjust_cell_height: Option<Adjust>,
    pub adjust_cell_width: Option<Adjust>,
    /// 논리 크기(pt). 화면 배율을 곱해 픽셀로 쓴다
    pub size: f32,
    /// 화면 배율(Retina 2.0)
    pub scale: f32,
    /// 기본 글꼴에 없는 글자를 찾을 글꼴들. 앞에 있을수록 먼저
    pub fallbacks: Vec<String>,
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: None,
            family_bold: None,
            family_italic: None,
            family_bold_italic: None,
            adjust_cell_height: None,
            adjust_cell_width: None,
            size: 13.0,
            scale: 1.0,
            fallbacks: vec![
                // 프롬프트 아이콘(사설 영역)
                NERD_SYMBOLS.into(),
                // 한글
                "Apple SD Gothic Neo".into(),
                "Noto Sans CJK KR".into(),
                "Noto Sans KR".into(),
                "NanumGothicCoding".into(),
                // 기호·상자 그림
                "Menlo".into(),
                "Apple Symbols".into(),
                "DejaVu Sans Mono".into(),
                "DejaVu Sans".into(),
                // Claude Code의 ⏺(U+23FA) 같은 기호: 흑백 윤곽이 있는 글꼴(글자색을 따르게, 색 이모지보다 먼저)
                "STIX Two Math".into(),
                "Noto Sans Symbols 2".into(),
                "Noto Sans Math".into(),
            ],
        }
    }
}

/// 기본 글꼴을 못 찾았을 때 차례로 찾는 고정폭 글꼴
const MONO_CANDIDATES: &[&str] = &[
    DEFAULT_FAMILY,
    "SF Mono",
    "Menlo",
    "Monaco",
    "JetBrains Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Noto Sans Mono",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontStyle {
    Regular,
    Bold,
    Italic,
    BoldItalic,
}

impl FontStyle {
    pub fn from_flags(bold: bool, italic: bool) -> Self {
        match (bold, italic) {
            (false, false) => Self::Regular,
            (true, false) => Self::Bold,
            (false, true) => Self::Italic,
            (true, true) => Self::BoldItalic,
        }
    }
}

/// 래스터한 글리프. `bitmap`은 덮음 정도(0~255)
#[derive(Debug)]
pub struct Glyph {
    pub width: usize,
    pub height: usize,
    /// 셀 왼쪽에서 비트맵 왼쪽까지
    pub left: i32,
    /// 기준선에서 비트맵 위쪽까지(위가 양수)
    pub top: i32,
    pub bitmap: Vec<u8>,
    /// 색 글리프(이모지)면 RGBA(곱하지 않은 알파). 그때 `bitmap`은 비어 있다
    pub color: Option<Vec<u8>>,
}

/// 셀 크기(픽셀)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellMetrics {
    pub width: u32,
    pub height: u32,
    /// 셀 위에서 기준선까지
    pub baseline: u32,
    pub underline_offset: u32,
    pub stroke: u32,
}

/// 글꼴 묶음과 글리프 캐시
pub struct Fonts {
    /// 0: 기본 글꼴의 네 꼴(없는 꼴은 Regular로), 그 뒤: 대체 글꼴
    regular: [Rc<Font>; 4],
    fallbacks: Vec<Fallback>,
    px: f32,
    metrics: CellMetrics,
    cache: HashMap<(char, FontStyle), Option<Rc<Glyph>>>,
    /// 이모지 모양 글리프(글자, 칸 수)
    emoji_cache: HashMap<(char, u32), Option<Rc<Glyph>>>,
    pub family_name: String,
    /// 색 이모지 글꼴(#37). 없으면 이모지는 다른 대체 글꼴이나 빈 상자로
    emoji: Option<ColorFont>,
    /// 흑백 글리프를 푸는 swash 작업 공간
    context: std::cell::RefCell<swash::scale::ScaleContext>,
}

/// 색 글꼴의 바이트와 모음 안의 자리. swash가 읽는다
struct ColorFont {
    data: ColorData,
    index: usize,
    context: std::cell::RefCell<swash::scale::ScaleContext>,
}

/// 색 이모지 글꼴 이름(찾는 차례)
const COLOR_EMOJI: &[&str] = &[
    "Apple Color Emoji",
    "Noto Color Emoji",
    "Segoe UI Emoji",
    "Twemoji Mozilla",
];

/// 색 글꼴이 있는 곳. Apple Color Emoji는 180MiB라 글꼴을 읽을 때 바이트를 읽지 않는다
enum ColorData {
    /// 파일: 처음 이모지를 그릴 때 그 꼴과 필요한 크기의 비트맵 한 벌만 읽는다(#59)
    File(std::path::PathBuf),
    /// 이미 메모리에 있는 글꼴(fontdb가 들고 있는 것)
    Shared(std::sync::Arc<dyn AsRef<[u8]> + Send + Sync>),
}

/// 골라 읽은 색 글꼴. 같은 파일·꼴·크기면 글꼴 묶음을 다시 만들어도 다시 읽지 않는다. 크기가 바뀌면 바꿔 둔다(하나만 든다)
type SubsetKey = (std::path::PathBuf, usize, u16);
type SubsetEntry = (SubsetKey, Option<Rc<Vec<u8>>>);
thread_local! {
    static COLOR_SUBSET: std::cell::RefCell<Option<SubsetEntry>> = const { std::cell::RefCell::new(None) };
}

/// 색 글꼴 바이트(빌린 것 또는 골라 읽은 것)와 그 안의 꼴 번호
enum ColorBytes<'a> {
    Borrowed(&'a [u8], usize),
    Subset(Rc<Vec<u8>>),
}

impl ColorBytes<'_> {
    fn font(&self) -> Option<swash::FontRef<'_>> {
        match self {
            ColorBytes::Borrowed(b, i) => swash::FontRef::from_index(b, *i),
            ColorBytes::Subset(d) => swash::FontRef::from_index(d, 0),
        }
    }
}

impl ColorData {
    /// `px` 크기로 그릴 바이트. 파일이면 sbix에서 `px` 이상인 가장 작은 한 벌만 읽는다
    fn bytes(&self, index: usize, px: u16) -> Option<ColorBytes<'_>> {
        match self {
            ColorData::Shared(a) => Some(ColorBytes::Borrowed((**a).as_ref(), index)),
            ColorData::File(path) => {
                let key = (path.clone(), index, px);
                let hit = COLOR_SUBSET.with(|c| c.borrow().as_ref().filter(|(k, _)| *k == key).map(|(_, d)| d.clone()));
                let data = match hit {
                    Some(d) => d,
                    None => {
                        let d = match crate::sfnt::read_face(path, index as u32, Some(px)) {
                            Ok(face) => {
                                log::info!(
                                    "색 이모지: {} {}px 한 벌, {}KiB",
                                    path.display(),
                                    face.sbix_ppem.unwrap_or(0),
                                    face.data.len() / 1024
                                );
                                Some(Rc::new(face.data))
                            }
                            Err(e) => {
                                log::warn!("색 이모지 글꼴을 읽지 못했습니다({}): {e}", path.display());
                                None
                            }
                        };
                        COLOR_SUBSET.with(|c| *c.borrow_mut() = Some((key, d.clone())));
                        d
                    }
                };
                data.map(ColorBytes::Subset)
            }
        }
    }
}

fn load_color_font(db: &Database) -> Option<ColorFont> {
    COLOR_EMOJI.iter().find_map(|name| {
        let families = [Family::Name(name)];
        let id = db.query(&Query {
            families: &families,
            ..Query::default()
        })?;
        let face = db.face(id)?;
        if !face.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(name)) {
            return None;
        }
        let (source, index) = db.face_source(id)?;
        let data = match source {
            fontdb::Source::File(path) => ColorData::File(path),
            fontdb::Source::Binary(data) | fontdb::Source::SharedFile(_, data) => ColorData::Shared(data),
        };
        Some(ColorFont {
            data,
            index: index as usize,
            context: std::cell::RefCell::new(swash::scale::ScaleContext::new()),
        })
    })
}

#[derive(Debug, thiserror::Error)]
pub enum FontError {
    #[error("고정폭 글꼴을 찾지 못했습니다(찾은 이름: {0})")]
    NotFound(String),
}

/// 글꼴 한 꼴(#65): 그 꼴만 담은 바이트와 swash의 캐시 열쇠. 글리프는 그릴 때 swash가 푼다(fontdue처럼 미리 풀지 않는다)
pub(crate) struct Font {
    data: Vec<u8>,
    offset: u32,
    key: swash::CacheKey,
}

impl Font {
    fn new(data: Vec<u8>, index: usize) -> Option<Self> {
        let f = swash::FontRef::from_index(&data, index)?;
        let (offset, key) = (f.offset, f.key);
        Some(Self { data, offset, key })
    }

    fn font(&self) -> swash::FontRef<'_> {
        swash::FontRef {
            data: &self.data,
            offset: self.offset,
            key: self.key,
        }
    }

    fn glyph_id(&self, c: char) -> u16 {
        self.font().charmap().map(c)
    }

    fn has(&self, c: char) -> bool {
        self.glyph_id(c) != 0
    }

    /// 글자의 나아가는 폭(px)
    fn advance(&self, c: char, px: f32) -> f32 {
        let f = self.font();
        f.glyph_metrics(&[]).scale(px).advance_width(f.charmap().map(c))
    }

    /// (위로 올라가는 높이, 아래로 내려가는 깊이(음수), 줄 사이) px
    fn line(&self, px: f32) -> (f32, f32, f32) {
        let m = self.font().metrics(&[]).scale(px);
        (m.ascent, -m.descent.abs(), m.leading)
    }
}

/// 읽어 둔 글꼴(#58). 글꼴은 크기와 상관없으므로, 설정을 다시 읽거나 글자 크기를 바꿀 때 같은 글꼴을 다시 읽지 않고 나눠 쓴다
type FontKey = (String, u16, u8);
thread_local! {
    static PARSED: std::cell::RefCell<HashMap<FontKey, Option<Rc<Font>>>> = std::cell::RefCell::new(HashMap::new());
}

fn load(db: &Database, family: &str, weight: Weight, style: Style) -> Option<Rc<Font>> {
    let key = (family.to_ascii_lowercase(), weight.0, style as u8);
    if let Some(hit) = PARSED.with(|p| p.borrow().get(&key).cloned()) {
        return hit;
    }
    let font = parse(db, family, weight, style).map(Rc::new);
    PARSED.with(|p| p.borrow_mut().insert(key, font.clone()));
    font
}

fn parse(db: &Database, family: &str, weight: Weight, style: Style) -> Option<Font> {
    let families = [Family::Name(family)];
    let id = db.query(&Query {
        families: &families,
        weight,
        style,
        ..Query::default()
    })?;
    // 이름이 다른 얼굴을 고르는 경우(대체 꼴)를 거른다
    let face = db.face(id)?;
    if !face.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(family)) {
        return None;
    }
    // 파일이면 그 꼴의 표만 읽는다(모음 파일 전체를 들지 않게, #65). 앱에 넣어 둔 글꼴은 이미 메모리에 있다
    let (source, index) = db.face_source(id)?;
    match source {
        fontdb::Source::File(path) => match crate::sfnt::read_face(&path, index, None) {
            Ok(face) => Font::new(face.data, 0),
            Err(e) => {
                log::warn!("글꼴을 읽지 못했습니다({}): {e}", path.display());
                None
            }
        },
        fontdb::Source::Binary(data) | fontdb::Source::SharedFile(_, data) => {
            Font::new((*data).as_ref().to_vec(), index as usize)
        }
    }
}

impl Fonts {
    /// 시스템 글꼴을 읽어 묶음을 만든다
    pub fn new(config: &FontConfig) -> Result<Self, FontError> {
        let mut db = Database::new();
        db.load_system_fonts();
        for data in EMBEDDED {
            db.load_font_data(data.to_vec());
        }
        Self::with_database(&db, config)
    }

    pub fn with_database(db: &Database, config: &FontConfig) -> Result<Self, FontError> {
        let mut names: Vec<&str> = Vec::new();
        if let Some(f) = &config.family {
            names.push(f);
        }
        names.extend_from_slice(MONO_CANDIDATES);
        let (family, regular) = names
            .iter()
            .find_map(|n| load(db, n, Weight::NORMAL, Style::Normal).map(|f| (n.to_string(), f)))
            .ok_or_else(|| FontError::NotFound(names.join(", ")))?;
        // 꼴마다 따로 정한 글꼴이 있으면 그것, 없으면 기본 글꼴의 그 꼴, 그것도 없으면 Regular
        let pick = |own: &Option<String>, w, s| {
            own.as_deref()
                .and_then(|name| load(db, name, w, s).or_else(|| load(db, name, Weight::NORMAL, Style::Normal)))
                .or_else(|| load(db, &family, w, s))
                .unwrap_or_else(|| regular.clone())
        };
        let bold = pick(&config.family_bold, Weight::BOLD, Style::Normal);
        let italic = pick(&config.family_italic, Weight::NORMAL, Style::Italic);
        let bold_italic = pick(&config.family_bold_italic, Weight::BOLD, Style::Italic);
        let px = (config.size * config.scale).max(4.0);
        let metrics = cell_metrics(
            &regular,
            px,
            config.adjust_cell_width,
            config.adjust_cell_height,
            config.scale,
        );
        let fallbacks = config
            .fallbacks
            .iter()
            .filter(|n| !n.eq_ignore_ascii_case(&family))
            .filter_map(|n| load(db, n, Weight::NORMAL, Style::Normal).map(|f| (n, f)))
            .map(|(n, f)| {
                let k = wide_scale(&f, px, metrics);
                let pua = is_symbol_font(n);
                Fallback {
                    font: f,
                    wide_scale: k,
                    pua,
                }
            })
            .collect();
        Ok(Self {
            regular: [regular, bold, italic, bold_italic],
            fallbacks,
            px,
            metrics,
            cache: HashMap::new(),
            emoji_cache: HashMap::new(),
            family_name: family,
            emoji: load_color_font(db),
            context: std::cell::RefCell::new(swash::scale::ScaleContext::new()),
        })
    }

    pub fn metrics(&self) -> CellMetrics {
        self.metrics
    }

    /// 색 이모지 글꼴을 찾았는가
    pub fn has_color_emoji(&self) -> bool {
        self.emoji.is_some()
    }

    /// 글리프를 꺼낸다. 어느 글꼴에도 없으면 없다
    pub fn glyph(&mut self, ch: char, style: FontStyle) -> Option<Rc<Glyph>> {
        if let Some(g) = self.cache.get(&(ch, style)) {
            return g.clone();
        }
        let g = self.rasterize(ch, style).map(Rc::new);
        self.cache.insert((ch, style), g.clone());
        g
    }

    fn rasterize(&self, ch: char, style: FontStyle) -> Option<Glyph> {
        // 두 칸 이모지는 색 글꼴이 먼저(기본 글꼴의 흑백 모양보다)
        if is_emoji(ch) {
            let m = self.metrics;
            if let Some(g) = self.color_glyph(ch, m.width * 2, m.height) {
                return Some(g);
            }
        }
        let idx = style as usize;
        let primary = &self.regular[idx];
        let (font, px) = if primary.has(ch) {
            (primary.clone(), self.px)
        } else if self.regular[0].has(ch) {
            (self.regular[0].clone(), self.px)
        } else {
            // 사설 영역(Nerd Font 아이콘)은 기호 글꼴에서만 찾는다. 한글 글꼴이 같은 번호에 엉뚱한 글자를 둔 경우가 있다
            let pua = is_private_use(ch);
            let fb = self.fallbacks.iter().find(|f| (!pua || f.pua) && f.font.has(ch))?;
            let wide = crate::text::char_cells(ch) == 2;
            (fb.font.clone(), if wide { self.px * fb.wide_scale } else { self.px })
        };
        let mut ctx = self.context.borrow_mut();
        let mut scaler = ctx.builder(font.font()).size(px).hint(false).build();
        let gid = font.glyph_id(ch);
        let img = swash::scale::Render::new(&[swash::scale::Source::Outline]).render(&mut scaler, gid)?;
        Some(Glyph {
            width: img.placement.width as usize,
            height: img.placement.height as usize,
            left: img.placement.left,
            top: img.placement.top,
            bitmap: img.data,
            color: None,
        })
    }

    /// U+FE0F가 붙어 이모지 모양으로 그릴 글자(#67). `cells`칸 안에 맞춘 색 글리프. 색 글꼴에 없으면 없다
    pub fn emoji_glyph(&mut self, ch: char, cells: u32) -> Option<Rc<Glyph>> {
        if let Some(g) = self.emoji_cache.get(&(ch, cells)) {
            return g.clone();
        }
        let m = self.metrics;
        let g = self.color_glyph(ch, m.width * cells.max(1), m.height).map(Rc::new);
        self.emoji_cache.insert((ch, cells), g.clone());
        g
    }

    /// 색 글꼴에서 이모지를 `max_w × max_h` 안에 래스터한다(sbix·CBDT 비트맵, COLR 윤곽)
    fn color_glyph(&self, ch: char, max_w: u32, max_h: u32) -> Option<Glyph> {
        use swash::scale::image::Content;
        use swash::scale::{Render, Source, StrikeWith};
        let cf = self.emoji.as_ref()?;
        let m = self.metrics;
        // 폭과 높이 중 작은 쪽에 맞춘다
        let target = max_w.min(max_h) as f32;
        // 비트맵 벌은 늘 두 칸 크기로 고른다. 한 칸 이모지(#67)도 같은 벌을 줄여 쓴다(골라 읽은 글꼴을 번갈아 다시 읽지 않게)
        let strike_px = (m.width * 2).min(m.height) as u16;
        let bytes = cf.data.bytes(cf.index, strike_px)?;
        let font = bytes.font()?;
        let gid = font.charmap().map(ch);
        if gid == 0 {
            return None;
        }
        let mut ctx = cf.context.borrow_mut();
        let mut scaler = ctx.builder(font).size(target).build();
        let img = Render::new(&[Source::ColorBitmap(StrikeWith::BestFit), Source::ColorOutline(0)])
            .render(&mut scaler, gid)?;
        if img.content != Content::Color || img.placement.width == 0 || img.placement.height == 0 {
            return None;
        }
        let (w, h, rgba) = fit_rgba(
            &img.data,
            img.placement.width as usize,
            img.placement.height as usize,
            max_w as usize,
            max_h as usize,
        );
        Some(Glyph {
            width: w,
            height: h,
            left: 0,
            // 셀 높이 가운데에 둔다
            top: m.baseline as i32 - (m.height as i32 - h as i32) / 2,
            bitmap: Vec::new(),
            color: Some(rgba),
        })
    }
}

/// 두 칸에 그리는 이모지인가(유니코드 폭이 2이고 이모지 영역)
fn is_emoji(c: char) -> bool {
    let u = c as u32;
    let range = matches!(u, 0x1F000..=0x1FAFF | 0x2300..=0x23FF | 0x2600..=0x27BF | 0x2B00..=0x2BFF);
    range && crate::text::char_cells(c) == 2
}

/// RGBA 그림이 `max_w × max_h`보다 크면 비율을 지켜 줄인다(칸마다 평균)
fn fit_rgba(data: &[u8], w: usize, h: usize, max_w: usize, max_h: usize) -> (usize, usize, Vec<u8>) {
    if w <= max_w && h <= max_h {
        return (w, h, data.to_vec());
    }
    let k = (max_w as f32 / w as f32).min(max_h as f32 / h as f32);
    let (nw, nh) = (((w as f32 * k) as usize).max(1), ((h as f32 * k) as usize).max(1));
    let mut out = vec![0u8; nw * nh * 4];
    for y in 0..nh {
        let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1));
        for x in 0..nw {
            let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1));
            // 알파를 곱해 더한 뒤 나눈다(투명한 칸의 색이 섞여 테두리가 어두워지지 않게)
            let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for sy in y0..y1.min(h) {
                for sx in x0..x1.min(w) {
                    let i = (sy * w + sx) * 4;
                    let pa = data[i + 3] as u32;
                    r += data[i] as u32 * pa;
                    g += data[i + 1] as u32 * pa;
                    b += data[i + 2] as u32 * pa;
                    a += pa;
                    n += 1;
                }
            }
            let o = (y * nw + x) * 4;
            if let (Some(r), Some(g), Some(b)) = (r.checked_div(a), g.checked_div(a), b.checked_div(a)) {
                out[o] = r as u8;
                out[o + 1] = g as u8;
                out[o + 2] = b as u8;
                out[o + 3] = (a / n.max(1)) as u8;
            }
        }
    }
    (nw, nh, out)
}

struct Fallback {
    font: Rc<Font>,
    /// 넓은 글자를 그릴 때의 크기 배율
    wide_scale: f32,
    /// 사설 영역 글자를 찾아도 되는 기호 글꼴
    pua: bool,
}

fn is_symbol_font(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("nerd") || n.contains("symbols") || n.contains("powerline")
}

/// 유니코드 사설 영역(Nerd Font·Powerline 아이콘이 쓰는 자리)
pub fn is_private_use(c: char) -> bool {
    matches!(c as u32, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD)
}

/// 대체 글꼴의 넓은 글자(한글)를 두 칸에 맞추는 배율. 기본 글꼴의 두 칸보다 좁으면 벌어져 보인다.
/// 폭으로는 두 칸의 92%까지, 높이로는 셀 높이까지 키운다(Ghostty의 대체 글꼴 크기 맞춤과 같은 생각)
fn wide_scale(font: &Font, px: f32, m: CellMetrics) -> f32 {
    if !font.has('가') {
        return 1.0;
    }
    let adv = font.advance('가', px);
    if adv <= 0.0 {
        return 1.0;
    }
    let by_width = (m.width as f32 * 2.0 * 0.92) / adv;
    let (ascent, descent, _) = font.line(px);
    let by_height = m.height as f32 / (ascent - descent).max(1.0);
    by_width.min(by_height).clamp(1.0, 1.35)
}

fn cell_metrics(font: &Font, px: f32, adjust_w: Option<Adjust>, adjust_h: Option<Adjust>, scale: f32) -> CellMetrics {
    let advance = font.advance('M', px);
    let (ascent, descent, gap) = font.line(px);
    // 보정 값의 포인트는 화면 배율을 곱해 픽셀로
    let scaled = |a: Adjust| match a {
        Adjust::Points(p) => Adjust::Points(p * scale),
        other => other,
    };
    let natural_h = ascent - descent + gap;
    let width = adjust_w
        .map(|a| scaled(a).apply(advance))
        .unwrap_or(advance)
        .round()
        .max(1.0) as u32;
    let height_f = adjust_h.map(|a| scaled(a).apply(natural_h)).unwrap_or(natural_h);
    let height = height_f.ceil().max(1.0) as u32;
    // 높이를 바꾸면 늘어난(줄어든) 만큼의 절반을 위에 둬 글자를 가운데에 둔다(Ghostty와 같다)
    let baseline = (ascent + gap / 2.0 + (height_f - natural_h) / 2.0).round().max(1.0) as u32;
    let stroke = (px / 14.0).round().max(1.0) as u32;
    let underline_offset = (baseline + stroke + 1).min(height.saturating_sub(stroke));
    CellMetrics {
        width,
        height,
        baseline,
        underline_offset,
        stroke,
    }
}

#[cfg(test)]
mod parsed_cache_tests {
    use super::*;

    /// #58: 글꼴 묶음을 다시 만들어도(설정 다시 읽기, 글자 크기) 풀어 둔 글꼴을 다시 풀지 않는다
    #[test]
    fn rebuilding_fonts_reuses_parsed_faces() {
        let a = Fonts::new(&FontConfig::default()).expect("넣어 둔 글꼴은 늘 있다");
        let b = Fonts::new(&FontConfig {
            size: 20.0,
            ..FontConfig::default()
        })
        .unwrap();
        assert!(Rc::ptr_eq(&a.regular[0], &b.regular[0]), "같은 글꼴이면 같은 객체");
        assert_ne!(a.metrics(), b.metrics(), "크기는 따로");
    }
}

#[cfg(test)]
mod face_size_tests {
    use super::*;

    /// #65: 한글 대체 글꼴(Apple SD Gothic Neo, 53MiB 모음)에서 한 꼴만 들고 있다(macOS에서만)
    #[test]
    fn korean_fallback_keeps_only_one_face() {
        let f = Fonts::new(&FontConfig::default()).unwrap();
        let Some(fb) = f.fallbacks.iter().find(|fb| fb.font.has('가')) else {
            return;
        };
        let len = fb.font.data.len();
        let whole = std::fs::metadata("/System/Library/Fonts/AppleSDGothicNeo.ttc")
            .map(|m| m.len() as usize)
            .unwrap_or(usize::MAX);
        assert!(len < whole / 2, "한 꼴 {len}바이트, 모음 전체 {whole}바이트");
    }
}
