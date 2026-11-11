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
pub use bt_core::config::{Adjust, CodepointFont, FontStyleName, SyntheticStyle};

/// 셀 안 그림 자리·두께 보정(Ghostty `adjust-*`, #163). 값은 퍼센트 또는 픽셀
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MetricAdjust {
    pub baseline: Option<Adjust>,
    pub underline_position: Option<Adjust>,
    pub underline_thickness: Option<Adjust>,
    pub strikethrough_position: Option<Adjust>,
    pub strikethrough_thickness: Option<Adjust>,
    pub overline_position: Option<Adjust>,
    pub overline_thickness: Option<Adjust>,
    pub cursor_thickness: Option<Adjust>,
    pub cursor_height: Option<Adjust>,
    pub box_thickness: Option<Adjust>,
    /// Nerd Font 아이콘 높이(`adjust-icon-height`, #174)
    pub icon_height: Option<Adjust>,
}

/// 가변 글꼴 축 값(태그, 값)
pub type Variations = Vec<([u8; 4], f32)>;

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
    /// 밑줄·취소선·윗줄·커서·상자 선·기준선 보정(#163)
    pub adjust: MetricAdjust,
    /// 가변 글꼴 축(#163). `variation`은 모든 꼴에, 나머지는 그 꼴에만 더한다(같은 축이면 꼴별 값이 이긴다)
    pub variation: Variations,
    pub variation_bold: Variations,
    pub variation_italic: Variations,
    pub variation_bold_italic: Variations,
    /// 논리 크기(pt). 화면 배율을 곱해 픽셀로 쓴다
    pub size: f32,
    /// 화면 배율(Retina 2.0)
    pub scale: f32,
    /// 기본 글꼴에 없는 글자를 찾을 글꼴들. 앞에 있을수록 먼저
    pub fallbacks: Vec<String>,
    /// 다듬기에 줄 글꼴 기능(#96, `font-feature`). 없으면 글꼴의 기본(calt·liga 등이 켜짐)
    pub features: Vec<([u8; 4], u16)>,
    /// 꼴마다 고를 글꼴 스타일 이름(#174, `font-style*`). 차례는 보통·굵게·기울임·굵은 기울임(`FontStyle as usize`)
    pub styles: [FontStyleName; 4],
    /// 글자 범위마다 정한 글꼴(#174, `font-codepoint-map`). 뒤에 있는 것이 이긴다
    pub codepoint_map: Vec<CodepointFont>,
    /// 커서 칸에서 다듬기를 끊는다(#174, `font-shaping-break = cursor`). Ghostty 기본은 켬
    pub shaping_break_cursor: bool,
    /// 글자 두껍게(#193, `font-thicken`)와 강도(0~255, `font-thicken-strength`)
    pub thicken: bool,
    pub thicken_strength: u8,
    /// 없는 굵게·기울임 꼴을 합성할지(#193, `font-synthetic-style`)
    pub synthetic: SyntheticStyle,
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
            adjust: MetricAdjust::default(),
            variation: Vec::new(),
            variation_bold: Vec::new(),
            variation_italic: Vec::new(),
            variation_bold_italic: Vec::new(),
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
            features: Vec::new(),
            styles: Default::default(),
            codepoint_map: Vec::new(),
            shaping_break_cursor: true,
            thicken: false,
            thicken_strength: 255,
            synthetic: SyntheticStyle::default(),
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
    /// 밑줄 두께(`adjust-underline-thickness`, #163)
    pub underline_thickness: u32,
    /// 취소선 위 가장자리(셀 위에서)와 두께
    pub strike_offset: u32,
    pub strike_thickness: u32,
    /// 윗줄 위 가장자리(셀 위에서)와 두께. 윗줄(SGR 53)은 아직 그리지 않는다
    pub overline_offset: u32,
    pub overline_thickness: u32,
    /// 커서 높이(`adjust-cursor-height`). 셀 높이와 달라도 셀 가운데에 둔다
    pub cursor_height: u32,
    /// 막대 커서 두께와 속 빈 네모 커서 선 두께(`adjust-cursor-thickness`)
    pub cursor_beam: u32,
    pub cursor_hollow: u32,
    /// 상자 선 두께(`adjust-box-thickness`)
    pub box_thickness: u32,
    /// Nerd Font 아이콘이 넘지 않을 높이(`adjust-icon-height`, #174). 보정이 없으면 셀 높이
    pub icon_height: u32,
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
    /// 이모지 묶음 글리프(묶음 글)
    cluster_cache: HashMap<String, Option<Rc<Glyph>>>,
    pub family_name: String,
    /// 색 이모지 글꼴(#37). 없으면 이모지는 다른 대체 글꼴이나 빈 상자로
    emoji: Option<ColorFont>,
    /// 흑백 글리프를 푸는 swash 작업 공간
    context: std::cell::RefCell<swash::scale::ScaleContext>,
    /// 합자(#96): 글꼴 기능, 다듬기 작업 공간, 다듬은 묶음(글자마다 바뀐 글리프 번호, 0은 그대로), 글리프 번호로 푼 글리프
    features: Vec<([u8; 4], u16)>,
    shaper: std::cell::RefCell<swash::shape::ShapeContext>,
    shape_cache: HashMap<(Box<str>, FontStyle), Rc<[u16]>>,
    gid_cache: HashMap<(u16, FontStyle), Option<Rc<Glyph>>>,
    /// 꼴마다 적용할 가변 글꼴 축(#163). `font-variation`에 꼴별 값을 더한 것(기본 글꼴의 네 꼴에만 쓴다)
    variations: [Variations; 4],
    /// 글자 범위마다 정한 글꼴(#174). 뒤에 있는 것이 먼저 이긴다
    mapped: Vec<Mapped>,
    /// 커서 칸에서 다듬기를 끊는가(#174)
    shaping_break_cursor: bool,
    /// 꼴마다 합성할 것(#193, `font-synthetic-style`). 기본 글꼴의 네 꼴에만 쓴다
    synth: [Synth; 4],
    /// 모든 윤곽에 더하는 두껍게(px, #193 `font-thicken`). 끄면 0
    thicken: f32,
}

/// `font-codepoint-map` 한 칸: 범위, 네 꼴의 글꼴, 넓은 글자 배율
struct Mapped {
    start: u32,
    end: u32,
    fonts: [Rc<Font>; 4],
    wide_scale: f32,
}

/// 다듬은 묶음 캐시의 상한. 넘으면 비운다(화면에 보이는 글은 곧 다시 찬다)
const SHAPE_CACHE_MAX: usize = 8192;

/// 색 글꼴의 바이트와 모음 안의 자리. swash가 읽는다
struct ColorFont {
    /// 찾은 글꼴 이름(COLOR_EMOJI의 하나)
    name: &'static str,
    data: ColorData,
    index: usize,
    context: std::cell::RefCell<swash::scale::ScaleContext>,
    /// 이모지 묶음을 다듬는 swash 작업 공간(#87)
    shaper: std::cell::RefCell<swash::shape::ShapeContext>,
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

/// 골라 읽은 색 글꼴. 같은 파일·꼴·크기면 글꼴 묶음을 다시 만들어도, 창끼리도 다시 읽지 않는다.
/// 창마다 글자 크기가 달라도 번갈아 읽지 않게 몇 개(`SUBSET_CACHE_MAX`)까지 든다. 넘으면 가장 오래된 것부터 버린다
type SubsetKey = (std::path::PathBuf, usize, u16);
type SubsetEntry = (SubsetKey, Option<Rc<Vec<u8>>>);
const SUBSET_CACHE_MAX: usize = 4;
thread_local! {
    static COLOR_SUBSET: std::cell::RefCell<Vec<SubsetEntry>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn subset_cache_get(cache: &[SubsetEntry], key: &SubsetKey) -> Option<Option<Rc<Vec<u8>>>> {
    cache.iter().find(|(k, _)| k == key).map(|(_, d)| d.clone())
}

fn subset_cache_put(cache: &mut Vec<SubsetEntry>, key: SubsetKey, data: Option<Rc<Vec<u8>>>) {
    cache.retain(|(k, _)| *k != key);
    if cache.len() >= SUBSET_CACHE_MAX {
        cache.remove(0);
    }
    cache.push((key, data));
}

// 훑어 둔 시스템 글꼴 목록과 넣어 둔 글꼴. 창을 열 때마다 시스템 글꼴 폴더를 다시 훑고 넣어 둔 글꼴을 복사하지 않게
// 나눠 쓴다. 설정을 다시 읽을 때는 `forget_system_fonts`로 버려 새로 설치한 글꼴을 본다
thread_local! {
    static SYSTEM_DB: std::cell::RefCell<Option<Rc<Database>>> = const { std::cell::RefCell::new(None) };
}

fn system_db() -> Rc<Database> {
    if let Some(db) = SYSTEM_DB.with(|d| d.borrow().clone()) {
        return db;
    }
    let mut db = Database::new();
    db.load_system_fonts();
    for data in EMBEDDED {
        db.load_font_data(data.to_vec());
    }
    let db = Rc::new(db);
    SYSTEM_DB.with(|d| *d.borrow_mut() = Some(db.clone()));
    db
}

/// 훑어 둔 시스템 글꼴 목록을 버린다(다음에 글꼴을 만들 때 다시 훑는다). 설정을 다시 읽을 때 부른다
pub fn forget_system_fonts() {
    SYSTEM_DB.with(|d| *d.borrow_mut() = None);
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
                let hit = COLOR_SUBSET.with(|c| subset_cache_get(&c.borrow(), &key));
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
                        COLOR_SUBSET.with(|c| subset_cache_put(&mut c.borrow_mut(), key, d.clone()));
                        d
                    }
                };
                data.map(ColorBytes::Subset)
            }
        }
    }
}

fn load_color_font(db: &Database) -> Option<ColorFont> {
    COLOR_EMOJI.iter().copied().find_map(|name| {
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
            name,
            data,
            index: index as usize,
            context: std::cell::RefCell::new(swash::scale::ScaleContext::new()),
            shaper: std::cell::RefCell::new(swash::shape::ShapeContext::new()),
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

    /// 굵은 꼴인가: 글꼴이 알리는 굵기가 600 이상이거나, 굵기 축에 값을 줬다(#193)
    fn is_bold(&self, vars: &[([u8; 4], f32)]) -> bool {
        let f = self.font();
        f.attributes().weight().0 >= 600 || (has_axis(&f, b"wght") && vars.iter().any(|(t, _)| t == b"wght"))
    }

    /// 기울임 꼴인가: 글꼴이 기울임·비스듬함이라 알리거나, 기울임 축(ital·slnt)에 값을 줬다(#193)
    fn is_italic(&self, vars: &[([u8; 4], f32)]) -> bool {
        let f = self.font();
        f.attributes().style() != swash::Style::Normal
            || [b"ital", b"slnt"]
                .into_iter()
                .any(|a| has_axis(&f, a) && vars.iter().any(|(t, _)| t == a))
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

fn has_axis(f: &swash::FontRef<'_>, tag: &[u8; 4]) -> bool {
    f.variations().find_by_tag(swash::tag_from_bytes(tag)).is_some()
}

/// 글리프에 합성할 꼴(#193). 글꼴에 그 꼴이 없어서 윤곽을 부풀리거나(굵게) 기울인다(기울임)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Synth {
    bold: bool,
    italic: bool,
}

/// 합성 기울임의 각도(도). Ghostty의 값은 확인하지 못해 넣어 둔 JetBrains Mono 기울임(약 9도)과 비슷한 10도로 했다
const SYNTH_ITALIC_DEGREES: f32 = 10.0;

/// 합성 굵게로 윤곽을 부풀리는 양(px): 글자 크기의 1/90. swash의 `embolden` 강도가 글리프 먹을 늘리는 정도를 재어,
/// JetBrains Mono 'a'가 보통 꼴보다 넣어 둔 굵은 꼴만큼(약 27%) 먹이 늘게 맞춘 값이다(한 글꼴 한 글자로 잰 값)
fn synth_bold_px(px: f32) -> f32 {
    px / 90.0
}

/// `font-thicken-strength`(0~255)를 부풀리는 양(px)으로: 0이 가장 약해도 0.015px, 255는 0.065px에 화면 배율을 곱한 값
/// (배율 2에서 먹이 약 3%~12% 늘어난다, 한 글자로 잰 값). macOS의 두껍게 그리기(CoreText)와 같은 알고리즘이 아니라 비슷한 효과를 내는 근사다
fn thicken_px(strength: u8, scale: f32) -> f32 {
    (0.015 + 0.05 * strength as f32 / 255.0) * scale.max(1.0)
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

/// 글꼴 이름이 `family`이고 글꼴이 알리는 스타일 이름(name 표의 subfamily)이 `style_name`인 꼴(#174, `font-style*`).
/// 대소문자는 가리지 않는다. 없으면 없다
fn load_named(db: &Database, family: &str, style_name: &str) -> Option<Rc<Font>> {
    let key = (
        format!("{}\0{}", family.to_ascii_lowercase(), style_name.to_ascii_lowercase()),
        u16::MAX,
        0,
    );
    if let Some(hit) = PARSED.with(|p| p.borrow().get(&key).cloned()) {
        return hit;
    }
    let font = db
        .faces()
        .filter(|f| f.families.iter().any(|(n, _)| n.eq_ignore_ascii_case(family)))
        .find_map(|f| parse_face(db, f.id).filter(|font| has_style_name(font, style_name)))
        .map(Rc::new);
    PARSED.with(|p| p.borrow_mut().insert(key, font.clone()));
    font
}

/// 글꼴의 스타일 이름(name 표 2번과 17번) 가운데 `name`과 같은 것이 있는가
fn has_style_name(font: &Font, name: &str) -> bool {
    use swash::StringId;
    font.font()
        .localized_strings()
        .filter(|s| matches!(s.id(), StringId::SubFamily | StringId::TypographicSubFamily))
        .any(|s| s.chars().collect::<String>().trim().eq_ignore_ascii_case(name.trim()))
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
    parse_face(db, id)
}

/// 얼굴 하나를 읽는다
fn parse_face(db: &Database, id: fontdb::ID) -> Option<Font> {
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
    /// 시스템 글꼴을 읽어(이미 읽어 둔 것이 있으면 그것으로) 묶음을 만든다
    pub fn new(config: &FontConfig) -> Result<Self, FontError> {
        Self::with_database(&system_db(), config)
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
        // 보통 꼴에 스타일 이름을 정했으면(#174, `font-style`) 그 이름의 꼴. 못 찾으면 그대로
        let regular = match &config.styles[0] {
            FontStyleName::Named(name) => load_named(db, &family, name).unwrap_or(regular),
            _ => regular,
        };
        // 꼴마다 따로 정한 글꼴이 있으면 그것, 없으면 기본 글꼴의 그 꼴, 그것도 없으면 Regular
        let pick = |own: &Option<String>, w, s| {
            own.as_deref()
                .and_then(|name| load(db, name, w, s).or_else(|| load(db, name, Weight::NORMAL, Style::Normal)))
                .or_else(|| load(db, &family, w, s))
                .unwrap_or_else(|| regular.clone())
        };
        // 꼴마다 정한 스타일 이름(#174): 이름이면 그 이름의 꼴(그 꼴의 글꼴, 없으면 기본 글꼴에서), 못 찾으면 위에서 고른 꼴.
        // `false`면 그 꼴을 끄고 보통 꼴로 그린다(보통 꼴은 끌 수 없다)
        let by_style = |i: usize, own: &Option<String>, w, s| match &config.styles[i] {
            FontStyleName::Disabled => regular.clone(),
            FontStyleName::Named(name) => {
                load_named(db, own.as_deref().unwrap_or(&family), name).unwrap_or_else(|| pick(own, w, s))
            }
            FontStyleName::Default => pick(own, w, s),
        };
        let bold = by_style(1, &config.family_bold, Weight::BOLD, Style::Normal);
        let italic = by_style(2, &config.family_italic, Weight::NORMAL, Style::Italic);
        let bold_italic = by_style(3, &config.family_bold_italic, Weight::BOLD, Style::Italic);
        let vars = |i: usize, own: &Variations| {
            if config.styles[i] == FontStyleName::Disabled {
                config.variation.clone()
            } else {
                merge_variations(&config.variation, own)
            }
        };
        // 합성(#193): 글꼴 찾기가 이 꼴을 못 찾아 다른 꼴을 돌려줬을 때만. 꼴마다 글꼴·스타일 이름을 정했으면 그 꼴로 본다
        let faces = [&regular, &bold, &italic, &bold_italic];
        let flags = [
            false,
            config.synthetic.bold,
            config.synthetic.italic,
            config.synthetic.bold_italic,
        ];
        let own = [
            &None,
            &config.family_bold,
            &config.family_italic,
            &config.family_bold_italic,
        ];
        let synth: [Synth; 4] = std::array::from_fn(|i| {
            let explicit = own[i].is_some() || matches!(config.styles[i], FontStyleName::Named(_));
            if i == 0 || !flags[i] || explicit || config.styles[i] == FontStyleName::Disabled {
                return Synth::default();
            }
            let empty = Vec::new();
            let own_vars = [
                &empty,
                &config.variation_bold,
                &config.variation_italic,
                &config.variation_bold_italic,
            ];
            let v = vars(i, own_vars[i]);
            Synth {
                bold: matches!(i, 1 | 3) && !faces[i].is_bold(&v),
                italic: matches!(i, 2 | 3) && !faces[i].is_italic(&v),
            }
        });
        let px = (config.size * config.scale).max(4.0);
        let metrics = cell_metrics(
            &regular,
            px,
            config.adjust_cell_width,
            config.adjust_cell_height,
            &config.adjust,
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
        let mapped = config
            .codepoint_map
            .iter()
            .filter_map(|m| {
                let normal = load(db, &m.family, Weight::NORMAL, Style::Normal)?;
                let face = |w, s| load(db, &m.family, w, s).unwrap_or_else(|| normal.clone());
                Some(Mapped {
                    start: m.start,
                    end: m.end,
                    wide_scale: wide_scale(&normal, px, metrics),
                    fonts: [
                        normal.clone(),
                        face(Weight::BOLD, Style::Normal),
                        face(Weight::NORMAL, Style::Italic),
                        face(Weight::BOLD, Style::Italic),
                    ],
                })
            })
            .collect();
        Ok(Self {
            regular: [regular, bold, italic, bold_italic],
            fallbacks,
            px,
            metrics,
            cache: HashMap::new(),
            emoji_cache: HashMap::new(),
            cluster_cache: HashMap::new(),
            family_name: family,
            emoji: load_color_font(db),
            context: std::cell::RefCell::new(swash::scale::ScaleContext::new()),
            features: config.features.clone(),
            shaper: std::cell::RefCell::new(swash::shape::ShapeContext::new()),
            shape_cache: HashMap::new(),
            gid_cache: HashMap::new(),
            variations: [
                config.variation.clone(),
                vars(1, &config.variation_bold),
                vars(2, &config.variation_italic),
                vars(3, &config.variation_bold_italic),
            ],
            mapped,
            shaping_break_cursor: config.shaping_break_cursor,
            synth,
            thicken: if config.thicken {
                thicken_px(config.thicken_strength, config.scale)
            } else {
                0.0
            },
        })
    }

    pub fn metrics(&self) -> CellMetrics {
        self.metrics
    }

    /// 이 글꼴 묶음이 글리프를 그리는 픽셀 크기(글자 크기 × 화면 배율). 같은 글자도 크기마다 다른 그림이라,
    /// 글리프를 크기를 넘어 나눠 쓰는 캐시(GPU 아틀라스)가 열쇠에 넣는다(#422)
    pub fn px(&self) -> f32 {
        self.px
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
        let mapped = self.mapped_for(ch, style);
        // 두 칸 이모지는 색 글꼴이 먼저(기본 글꼴의 흑백 모양보다). 글자 범위로 글꼴을 정했으면 그것이 먼저
        if mapped.is_none() && is_emoji(ch) {
            let m = self.metrics;
            if let Some(g) = self.color_glyph(ch, m.width * 2, m.height) {
                return Some(g);
            }
        }
        let idx = style as usize;
        let primary = &self.regular[idx];
        let mut synth = Synth::default();
        let (font, px, vars) = if let Some(m) = mapped {
            let wide = crate::text::char_cells(ch) == 2;
            (
                m.fonts[idx].clone(),
                if wide { self.px * m.wide_scale } else { self.px },
                &[][..],
            )
        } else if primary.has(ch) {
            synth = self.synth[idx];
            (primary.clone(), self.px, &self.variations[idx][..])
        } else if self.regular[0].has(ch) {
            (self.regular[0].clone(), self.px, &self.variations[0][..])
        } else {
            // 사설 영역(Nerd Font 아이콘)은 기호 글꼴에서만 찾는다. 한글 글꼴이 같은 번호에 엉뚱한 글자를 둔 경우가 있다
            let pua = is_private_use(ch);
            let fb = self.fallbacks.iter().find(|f| (!pua || f.pua) && f.font.has(ch))?;
            let wide = crate::text::char_cells(ch) == 2;
            (
                fb.font.clone(),
                if wide { self.px * fb.wide_scale } else { self.px },
                &[][..],
            )
        };
        let mut ctx = self.context.borrow_mut();
        let gid = font.glyph_id(ch);
        let mut render_at = |px: f32| {
            let mut scaler = ctx.builder(font.font()).size(px).hint(false).variations(vars).build();
            self.render_outline(&mut scaler, gid, px, synth)
        };
        let img = render_at(px)?;
        let (mut left, mut top) = (img.placement.left, img.placement.top);
        let icon_k = self.icon_scale(ch);
        let img = if icon_k == 1.0 {
            img
        } else {
            // Nerd 아이콘 높이 보정(#174): 셀 높이 대신 보정한 높이에 맞춰 키우거나 줄이고, 원래 글리프의 가운데에 둔다.
            // 키울 때는 칸 폭을 넘지 않게 멈춘다(그래서 모든 아이콘이 같은 비율로 커지지는 않는다)
            let mut k = icon_k;
            let mut big = render_at(px * k)?;
            let slot = self.metrics.width * crate::text::char_cells(ch).max(1) as u32;
            if k > 1.0 && big.placement.width > slot {
                // 원래 크기가 이미 칸 폭을 채웠으면 더 키우지 않는다(원래보다 작아지지는 않는다)
                k = (k * slot as f32 / big.placement.width as f32).max(1.0);
                big = render_at(px * k)?;
            }
            let (w0, h0) = (img.placement.width as f32, img.placement.height as f32);
            let (w1, h1) = (big.placement.width as f32, big.placement.height as f32);
            left = (left as f32 + w0 / 2.0 - w1 / 2.0).round() as i32;
            top = (top as f32 - h0 / 2.0 + h1 / 2.0).round() as i32;
            big
        };
        Some(Glyph {
            width: img.placement.width as usize,
            height: img.placement.height as usize,
            left,
            top,
            bitmap: img.data,
            color: None,
        })
    }

    /// 윤곽 글리프 하나를 그린다. 두껍게(#193 `font-thicken`)와 합성 굵게는 윤곽을 부풀리고, 합성 기울임은 윤곽을 비튼다
    fn render_outline(
        &self,
        scaler: &mut swash::scale::Scaler<'_>,
        gid: u16,
        px: f32,
        synth: Synth,
    ) -> Option<swash::scale::image::Image> {
        let mut render = swash::scale::Render::new(&[swash::scale::Source::Outline]);
        let grow = self.thicken + if synth.bold { synth_bold_px(px) } else { 0.0 };
        if grow > 0.0 {
            render.embolden(grow);
        }
        if synth.italic {
            let skew = swash::zeno::Transform::skew(
                swash::zeno::Angle::from_degrees(SYNTH_ITALIC_DEGREES),
                swash::zeno::Angle::from_degrees(0.0),
            );
            render.transform(Some(skew));
        }
        render.render(scaler, gid)
    }

    /// 글자 범위로 정한 글꼴(`font-codepoint-map`, #174). 뒤에 쓴 것이 먼저 이기고, 그 글꼴에 글자가 없으면 건너뛴다
    fn mapped_for(&self, ch: char, style: FontStyle) -> Option<&Mapped> {
        let c = ch as u32;
        self.mapped
            .iter()
            .rev()
            .find(|m| (m.start..=m.end).contains(&c) && m.fonts[style as usize].has(ch))
    }

    /// Nerd Font 아이콘에 곱할 크기 배율(`adjust-icon-height`, #174). 아이콘이 아니거나 보정이 없으면 1
    fn icon_scale(&self, ch: char) -> f32 {
        let m = self.metrics;
        if !is_private_use(ch) || is_powerline(ch) || m.icon_height == m.height {
            return 1.0;
        }
        m.icon_height as f32 / m.height as f32
    }

    /// 커서 칸에서 다듬기를 끊는가(`font-shaping-break = cursor`, #174)
    pub fn breaks_shaping_at_cursor(&self) -> bool {
        self.shaping_break_cursor
    }

    /// 그 꼴의 기본 글꼴에 있는 글자인가(합자로 다듬을 수 있는 글자, #96)
    pub fn primary_has(&self, ch: char, style: FontStyle) -> bool {
        self.regular[style as usize].has(ch) && self.mapped_for(ch, style).is_none()
    }

    /// 한 꼴의 글자 묶음을 다듬어(#96) 글자마다 바뀐 글리프 번호를 돌려준다. 0이면 그 글자는 그대로 그린다.
    /// 터미널은 칸 폭이 정해져 있어, 글자 수와 글리프 수가 같은 다듬기(JetBrains Mono·Fira Code의 calt 합자)만 받는다
    pub fn shape_run(&mut self, text: &str, style: FontStyle) -> Rc<[u16]> {
        let key = (Box::<str>::from(text), style);
        if let Some(hit) = self.shape_cache.get(&key) {
            return hit.clone();
        }
        let font = self.regular[style as usize].clone();
        let chars: Vec<char> = text.chars().collect();
        let starts: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
        let mut out = vec![0u16; chars.len()];
        {
            let mut sctx = self.shaper.borrow_mut();
            let mut shaper = sctx
                .builder(font.font())
                .script(swash::text::Script::Latin)
                .features(self.features.iter())
                .variations(self.variations[style as usize].iter())
                .build();
            shaper.add_str(text);
            shaper.shape_with(|c| {
                let first = starts.partition_point(|&b| b < c.source.start as usize);
                let last = starts.partition_point(|&b| b < c.source.end as usize);
                if last - first != c.glyphs.len() {
                    return;
                }
                for (k, g) in c.glyphs.iter().enumerate() {
                    let i = first + k;
                    if g.id != 0 && g.id != font.glyph_id(chars[i]) {
                        out[i] = g.id;
                    }
                }
            });
        }
        let out: Rc<[u16]> = out.into();
        if self.shape_cache.len() >= SHAPE_CACHE_MAX {
            self.shape_cache.clear();
        }
        self.shape_cache.insert(key, out.clone());
        out
    }

    /// 그 꼴의 기본 글꼴에서 글리프 번호로 푼 글리프(합자, #96)
    pub fn glyph_by_id(&mut self, gid: u16, style: FontStyle) -> Option<Rc<Glyph>> {
        if let Some(g) = self.gid_cache.get(&(gid, style)) {
            return g.clone();
        }
        let font = self.regular[style as usize].clone();
        let g = {
            let mut ctx = self.context.borrow_mut();
            let mut scaler = ctx
                .builder(font.font())
                .size(self.px)
                .hint(false)
                .variations(&self.variations[style as usize])
                .build();
            self.render_outline(&mut scaler, gid, self.px, self.synth[style as usize])
                .map(|img| {
                    Rc::new(Glyph {
                        width: img.placement.width as usize,
                        height: img.placement.height as usize,
                        left: img.placement.left,
                        top: img.placement.top,
                        bitmap: img.data,
                        color: None,
                    })
                })
        };
        self.gid_cache.insert((gid, style), g.clone());
        g
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
        let cf = self.emoji.as_ref()?;
        let m = self.metrics;
        // 비트맵 벌은 늘 두 칸 크기로 고른다. 한 칸 이모지(#67)도 같은 벌을 줄여 쓴다(골라 읽은 글꼴을 번갈아 다시 읽지 않게)
        let strike_px = (m.width * 2).min(m.height) as u16;
        let bytes = cf.data.bytes(cf.index, strike_px)?;
        let font = bytes.font()?;
        let gid = font.charmap().map(ch);
        if gid == 0 {
            return None;
        }
        self.render_color_gid(font, gid, max_w, max_h)
    }

    /// 이모지 묶음(ZWJ·피부색·국기, #87)을 색 글꼴의 다듬기(morx·GSUB)로 한 글리프로 만들어 두 칸 크기로 그린다.
    /// 한 글리프로 다듬어지지 않으면 없다(그러면 글자마다 그린다)
    pub fn cluster_glyph(&mut self, text: &str) -> Option<Rc<Glyph>> {
        if let Some(g) = self.cluster_cache.get(text) {
            return g.clone();
        }
        let g = self.shape_color(text).map(Rc::new);
        if self.cluster_cache.len() < 4096 {
            self.cluster_cache.insert(text.to_string(), g.clone());
        }
        g
    }

    fn shape_color(&self, text: &str) -> Option<Glyph> {
        let cf = self.emoji.as_ref()?;
        let m = self.metrics;
        let (max_w, max_h) = (m.width * 2, m.height);
        let bytes = cf.data.bytes(cf.index, max_w.min(max_h) as u16)?;
        let font = bytes.font()?;
        let mut ids = Vec::new();
        {
            let mut sctx = cf.shaper.borrow_mut();
            let mut shaper = sctx.builder(font).size(max_w.min(max_h) as f32).build();
            shaper.add_str(text);
            shaper.shape_with(|c| ids.extend(c.glyphs.iter().map(|g| g.id)));
        }
        match ids.as_slice() {
            [gid] if *gid != 0 => self.render_color_gid(font, *gid, max_w, max_h),
            _ => None,
        }
    }

    /// 색 글꼴의 글리프 하나를 `max_w × max_h` 안에 래스터한다
    fn render_color_gid(&self, font: swash::FontRef<'_>, gid: u16, max_w: u32, max_h: u32) -> Option<Glyph> {
        use swash::scale::image::Content;
        use swash::scale::{Render, Source, StrikeWith};
        let cf = self.emoji.as_ref()?;
        let m = self.metrics;
        let target = max_w.min(max_h) as f32;
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

/// 파워라인 도형(U+E0B0~U+E0D7): 칸에 꽉 차게 그려야 해서 아이콘 높이 보정을 받지 않는다
fn is_powerline(c: char) -> bool {
    matches!(c as u32, 0xE0B0..=0xE0D7)
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

/// 꼴 하나의 가변 글꼴 축: 모든 꼴 공통 값 위에 꼴별 값을 얹는다(같은 축이면 꼴별 값이 이긴다)
fn merge_variations(all: &[([u8; 4], f32)], own: &[([u8; 4], f32)]) -> Variations {
    let mut out: Variations = all
        .iter()
        .filter(|(t, _)| !own.iter().any(|(o, _)| o == t))
        .copied()
        .collect();
    out.extend_from_slice(own);
    out
}

/// 보정 하나를 기준 값에 적용한다(Ghostty `Metrics.Modifier`와 같은 뜻). 퍼센트는 기준 값에 곱하고, 포인트는 화면 배율을 곱한 픽셀을 더한다.
/// 결과는 반올림한다. 아래 한계는 부른 쪽이 정한다
pub(crate) fn adjusted(base: f32, a: Option<Adjust>, scale: f32) -> i32 {
    let v = match a {
        None => base,
        Some(Adjust::Percent(p)) => base * (1.0 + p / 100.0),
        Some(Adjust::Points(p)) => base + p * scale,
    };
    v.round() as i32
}

fn cell_metrics(
    font: &Font,
    px: f32,
    adjust_w: Option<Adjust>,
    adjust_h: Option<Adjust>,
    adj: &MetricAdjust,
    scale: f32,
) -> CellMetrics {
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

    // 두께는 1픽셀 밑으로 내려가지 않는다(Ghostty도 `*-thickness`는 1px 아래로 가지 않는다)
    let thickness = |base: u32, a: Option<Adjust>| adjusted(base as f32, a, scale).max(1) as u32;
    let h = height as i32;
    // 위치(셀 위에서)는 줄이 셀 안에 들어가게 자른다
    let position = |base: u32, a: Option<Adjust>, thick: u32| {
        adjusted(base as f32, a, scale).clamp(0, (h - thick as i32).max(0)) as u32
    };

    let underline_thickness = thickness(stroke, adj.underline_thickness);
    let strike_thickness = thickness(stroke, adj.strikethrough_thickness);
    let overline_thickness = thickness(stroke, adj.overline_thickness);
    // 기준선 보정은 셀 아래에서 기준선까지의 거리에 한다: 늘리면 글자가 위로 올라간다
    let from_bottom = (height - baseline.min(height)) as f32;
    let baseline_adjusted = (h - adjusted(from_bottom, adj.baseline, scale)).clamp(1, h.max(1)) as u32;
    CellMetrics {
        width,
        height,
        baseline: baseline_adjusted,
        underline_offset: position(underline_offset, adj.underline_position, underline_thickness),
        stroke,
        underline_thickness,
        strike_offset: position(height / 2, adj.strikethrough_position, strike_thickness),
        strike_thickness,
        overline_offset: position(0, adj.overline_position, overline_thickness),
        overline_thickness,
        cursor_height: adjusted(height as f32, adj.cursor_height, scale).clamp(1, h.max(1)) as u32,
        cursor_beam: thickness((stroke * 2).max(2), adj.cursor_thickness),
        cursor_hollow: thickness(stroke, adj.cursor_thickness),
        box_thickness: thickness(stroke, adj.box_thickness),
        icon_height: adjusted(height as f32, adj.icon_height, scale).max(1) as u32,
    }
}

/// 시스템 글꼴 얼굴 하나(`+list-fonts`용 요약)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaceEntry {
    /// 글꼴 집안 이름
    pub family: String,
    /// 얼굴 이름(PostScript 이름, 없으면 집안 이름)
    pub name: String,
    pub monospaced: bool,
    pub bold: bool,
    pub italic: bool,
}

/// 시스템 글꼴과 넣어 둔 글꼴의 얼굴을 모두 모은다. 글꼴 파일을 읽지는 않고 목록만 본다
pub fn list_system_faces() -> Vec<FaceEntry> {
    let mut db = Database::new();
    db.load_system_fonts();
    for data in EMBEDDED {
        db.load_font_data(data.to_vec());
    }
    db.faces()
        .filter_map(|f| {
            let family = f.families.first()?.0.clone();
            Some(FaceEntry {
                name: if f.post_script_name.is_empty() {
                    family.clone()
                } else {
                    f.post_script_name.clone()
                },
                family,
                monospaced: f.monospaced,
                bold: f.weight.0 >= 600,
                italic: f.style != Style::Normal,
            })
        })
        .collect()
}

/// 글자 하나가 어디서 그려지는가(`+show-face`). `Fonts::rasterize`의 고르는 차례와 같다
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaceSource {
    /// 상자 선·블록 같은 직접 그리는 글자
    Sprite,
    /// 색 이모지 글꼴
    ColorEmoji(String),
    /// `font-codepoint-map`으로 정한 글꼴
    Mapped(String),
    /// 그 꼴의 기본 글꼴
    Primary(String),
    /// 그 꼴에 없어 보통 꼴의 기본 글꼴
    PrimaryRegular(String),
    /// 대체 글꼴
    Fallback(String),
    NotFound,
}

/// 얼굴 이름(name 표의 전체 이름, 없으면 집안 이름)
fn face_name(font: &Font) -> String {
    use swash::StringId;
    let f = font.font();
    let s = f.localized_strings();
    [StringId::Full, StringId::Family]
        .into_iter()
        .find_map(|id| {
            s.clone()
                .find_by_id(id, Some("en"))
                .or_else(|| s.clone().find_by_id(id, None))
        })
        .map(|n| n.to_string())
        .unwrap_or_default()
}

impl Fonts {
    /// 글자 `ch`를 어느 얼굴로 그리는지. 그리지는 않는다. `emoji_presentation`이 `Some(false)`면 색 이모지를 건너뛴다
    pub fn face_source(&self, ch: char, style: FontStyle, emoji_presentation: Option<bool>) -> FaceSource {
        let mut buf = vec![0u32; 32 * 64];
        let mut canvas = crate::Canvas::new(&mut buf, 32, 64);
        if crate::boxdraw::draw(&mut canvas, ch, 0, 0, 32, 64, 2, bt_core::Rgb::new(255, 255, 255)) {
            return FaceSource::Sprite;
        }
        let mapped = self.mapped_for(ch, style);
        let color_ok = emoji_presentation != Some(false);
        if mapped.is_none() && color_ok && (is_emoji(ch) || emoji_presentation == Some(true)) {
            if let Some(cf) = &self.emoji {
                let covered = cf
                    .data
                    .bytes(cf.index, 2)
                    .and_then(|b| b.font().map(|f| f.charmap().map(ch) != 0))
                    .unwrap_or(false);
                if covered {
                    return FaceSource::ColorEmoji(cf.name.to_string());
                }
            }
        }
        let idx = style as usize;
        if let Some(m) = mapped {
            return FaceSource::Mapped(face_name(&m.fonts[idx]));
        }
        if self.regular[idx].has(ch) {
            return FaceSource::Primary(face_name(&self.regular[idx]));
        }
        if self.regular[0].has(ch) {
            return FaceSource::PrimaryRegular(face_name(&self.regular[0]));
        }
        let pua = is_private_use(ch);
        match self.fallbacks.iter().find(|f| (!pua || f.pua) && f.font.has(ch)) {
            Some(fb) => FaceSource::Fallback(face_name(&fb.font)),
            None => FaceSource::NotFound,
        }
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

/// 터미널 글꼴과 탭 줄 제목 글꼴(#241 `window-title-font-family`). 키를 비우면 macOS에서는 시스템 UI 글꼴(#460)
pub struct FontSet {
    pub terminal: Fonts,
    /// 제목 글꼴. 키가 비었는데 시스템 UI 글꼴도 못 찾았거나 이름의 글꼴을 못 찾았으면 없다(그러면 제목도 터미널 글꼴로)
    pub title: Option<Fonts>,
    /// 제목 글꼴을 못 찾았을 때의 경고
    pub title_warning: Option<String>,
}

/// 시스템 UI 글꼴을 찾을 때 시도하는 이름과 순서(#460). macOS 26에서 `.SF NS`와 `System Font`는 둘 다 `SFNS.ttf`(가변 글꼴)의
/// 이름이고 fontdb가 그대로 본다. `.SF NS Text`는 macOS 10.11~10.15의 시스템 글꼴 이름이다(이 기기에서는 확인하지 못했다).
/// `unsafe` 없이 시스템이 알려 주는 UI 글꼴(CoreText)을 부를 수 없어 이름 목록으로 찾는다. 하나도 없으면 없다
const SYSTEM_UI_FAMILIES: &[&str] = &[".SF NS", "System Font", ".SF NS Text"];

/// 시스템 UI 글꼴의 패밀리 이름: `SYSTEM_UI_FAMILIES`를 차례로 `has`에 물어 처음 있는 것. 없으면 없다
fn system_ui_family(has: impl Fn(&str) -> bool) -> Option<&'static str> {
    SYSTEM_UI_FAMILIES.iter().copied().find(|n| has(n))
}

impl Fonts {
    /// 터미널 글꼴과 제목 글꼴을 한 번에 만든다(시스템 글꼴 읽기는 한 번).
    /// 제목 글꼴은 터미널 글꼴과 같은 크기이고, 그 글꼴에 없는 글자(한글·기호·이모지)는 터미널의 기본 글꼴 다음
    /// 터미널과 같은 대체 순서로 찾는다. 이름의 글꼴을 못 찾으면 제목 글꼴은 없고 경고가 있다
    pub fn new_with_title(config: &FontConfig, title_family: Option<&str>) -> Result<FontSet, FontError> {
        let db = system_db();
        let terminal = Self::with_database(&db, config)?;
        let (mut title, mut title_warning) = (None, None);
        // 이름을 정했으면 그 글꼴(없으면 경고). 비웠으면 macOS에서는 시스템 UI 글꼴(#460, 못 찾으면 경고 없이 터미널 글꼴)
        let name = match title_family {
            Some(name) => Some(name),
            None if cfg!(target_os = "macos") => {
                system_ui_family(|n| load(&db, n, Weight::NORMAL, Style::Normal).is_some())
            }
            None => None,
        };
        if let Some(name) = name {
            if load(&db, name, Weight::NORMAL, Style::Normal).is_none() {
                title_warning = Some(format!(
                    "window-title-font-family: 글꼴 \"{name}\"을 찾지 못해 탭 제목을 터미널 글꼴로 그립니다"
                ));
            } else {
                // 대체 글꼴: 터미널 기본 글꼴이 먼저, 그다음 터미널과 같은 순서
                let mut fallbacks = vec![terminal.family_name.clone()];
                fallbacks.extend(config.fallbacks.iter().cloned());
                let tc = FontConfig {
                    family: Some(name.to_string()),
                    fallbacks,
                    size: config.size,
                    scale: config.scale,
                    ..FontConfig::default()
                };
                title = Self::with_database(&db, &tc).ok();
            }
        }
        Ok(FontSet {
            terminal,
            title,
            title_warning,
        })
    }

    /// 글자 하나가 나아가는 폭(px). 제목처럼 셀 격자 밖에 그릴 때 쓴다. 그리는 글리프가 어느 글꼴에서 오는지를 따른다:
    /// 고정폭 제목에서 색 이모지와 대체 글꼴의 넓은 글자는 두 칸 폭. 비례폭 제목에서는 색 이모지는 그려지는 글리프 폭,
    /// 대체 글꼴의 글자는 그 글꼴의 advance. 그 밖에는 글꼴이 정한 advance
    pub fn advance(&mut self, ch: char) -> f32 {
        let cells = crate::text::char_cells(ch);
        if cells == 0 {
            return 0.0;
        }
        let slot = (self.metrics.width * cells as u32) as f32;
        let primary_adv = |s: &Self, c: char| s.regular[0].advance(c, s.px);
        // 고정폭 제목 글꼴이면 터미널처럼 두 칸, 비례폭이면 그려지는 글리프(대체 글꼴)의 폭을 따른다
        // (두 칸이면 글자 사이가 벌어진다)
        let monospace = (primary_adv(self, 'i') - primary_adv(self, 'M')).abs() < 0.5;
        if is_emoji(ch) {
            if let Some(g) = self.glyph(ch, FontStyle::Regular).filter(|g| g.color.is_some()) {
                // 색 글리프는 두 칸 × 줄 높이 안으로 줄어 그려진다. 비례폭이면 그 폭만큼만 나아가 앞뒤 여백이 넓지 않다
                return if monospace { slot } else { g.width as f32 };
            }
        }
        let primary = &self.regular[0];
        if primary.has(ch) {
            return primary.advance(ch, self.px);
        }
        let pua = is_private_use(ch);
        match self.fallbacks.iter().find(|f| (!pua || f.pua) && f.font.has(ch)) {
            Some(_) if cells == 2 && monospace => slot,
            Some(fb) => fb.font.advance(ch, self.px),
            None => slot,
        }
    }
}

#[cfg(test)]
mod title_font_tests {
    use super::*;

    #[test]
    fn no_title_family_has_no_warning() {
        let s = Fonts::new_with_title(&FontConfig::default(), None).unwrap();
        assert!(s.title_warning.is_none());
    }

    /// #460: 비우면 시스템 UI 글꼴. 시스템에 그 글꼴이 있을 때만(macOS) 확인하고, 없으면 건너뛴다
    #[test]
    fn empty_title_family_uses_system_ui_font_when_present() {
        let db = system_db();
        // 시험은 구현의 이름 목록이 아니라 macOS가 실제로 갖는 이름(.SF NS)으로 있는지 본다
        let present = |n: &str| load(&db, n, Weight::NORMAL, Style::Normal).is_some();
        let Some(name) = [".SF NS", "System Font"].into_iter().find(|n| present(n)) else {
            eprintln!("건너뜀: 이 시스템에는 시스템 UI 글꼴이 없다");
            let s = Fonts::new_with_title(&FontConfig::default(), None).unwrap();
            assert!(s.title.is_none(), "못 찾으면 터미널 글꼴(제목 글꼴 없음)");
            return;
        };
        let s = Fonts::new_with_title(&FontConfig::default(), None).unwrap();
        let title = s.title.expect("시스템 UI 글꼴이 제목 글꼴이어야 한다");
        assert!(
            title.family_name == name || present(&title.family_name),
            "제목 글꼴 {:?}",
            title.family_name
        );
        assert!(s.title_warning.is_none());
        assert!(title.metrics().height > 0);
    }

    #[test]
    fn system_ui_family_tries_names_in_order_and_none_when_absent() {
        assert_eq!(system_ui_family(|_| false), None);
        let all = SYSTEM_UI_FAMILIES;
        assert!(!all.is_empty());
        assert_eq!(system_ui_family(|_| true), Some(all[0]));
        let last = *all.last().unwrap();
        assert_eq!(system_ui_family(|n| n == last), Some(last));
        assert_eq!(system_ui_family(|n| n == "없는글꼴"), None);
    }

    /// #460: 가변 글꼴(SFNS)을 기본 인스턴스로 읽으면 Regular 굵기여야 한다
    #[test]
    fn system_ui_font_default_instance_is_regular() {
        let db = system_db();
        let Some(font) = [".SF NS", "System Font"]
            .iter()
            .find_map(|n| load(&db, n, Weight::NORMAL, Style::Normal))
        else {
            eprintln!("건너뜀: 이 시스템에는 시스템 UI 글꼴이 없다");
            return;
        };
        let f = font.font();
        assert_eq!(f.attributes().weight().0, 400);
        assert!(!font.is_bold(&[]));
        assert!(font.has('A') && font.has('0'));
    }

    #[test]
    fn missing_title_family_warns_and_falls_back() {
        let s = Fonts::new_with_title(&FontConfig::default(), Some("없는글꼴-XYZ-241")).unwrap();
        assert!(s.title.is_none(), "못 찾은 글꼴로 조용히 다른 글꼴을 쓰면 안 된다");
        let w = s.title_warning.expect("경고가 있어야 한다");
        assert!(
            w.contains("window-title-font-family") && w.contains("없는글꼴-XYZ-241"),
            "{w}"
        );
    }

    #[test]
    fn found_title_family_is_used_without_warning() {
        // 넣어 둔 JetBrains Mono는 어느 시스템에나 있다
        let s = Fonts::new_with_title(&FontConfig::default(), Some("JetBrains Mono")).unwrap();
        assert!(s.title.is_some());
        assert!(s.title_warning.is_none());
    }

    #[test]
    fn title_font_does_not_change_terminal_metrics() {
        let plain = Fonts::new(&FontConfig::default()).unwrap();
        let s = Fonts::new_with_title(&FontConfig::default(), Some("JetBrains Mono")).unwrap();
        assert_eq!(plain.metrics(), s.terminal.metrics());
    }

    #[test]
    fn monospace_advance_is_the_cell_width() {
        let mut f = Fonts::new(&FontConfig::default()).unwrap();
        let w = f.metrics().width as f32;
        assert!((f.advance('i') - w).abs() < 0.5, "{}", f.advance('i'));
        assert!((f.advance('W') - w).abs() < 0.5);
        // 두 칸 글자(한글)는 어느 글꼴에서 오든 두 칸을 나아가 겹치지 않는다
        if f.glyph('한', FontStyle::Regular).is_some() {
            assert!((f.advance('한') - 2.0 * w).abs() < 0.5, "{}", f.advance('한'));
        }
        assert_eq!(f.advance('\u{200d}'), 0.0);
    }

    #[test]
    fn proportional_title_font_has_different_advances() {
        // 비례폭 글꼴(macOS의 Helvetica)이 있는 시스템에서만
        let Ok(s) = Fonts::new_with_title(&FontConfig::default(), Some("Helvetica")) else {
            return;
        };
        let Some(mut t) = s.title else { return };
        let (i, w) = (t.advance('i'), t.advance('W'));
        assert!(i > 0.0 && i < w, "i={i} W={w}");
        // 글꼴에 없는 한글은 대체 글꼴에서 온다. 비례폭 제목이면 두 칸이 아니라 그 글꼴의 폭을 나아가
        // 글자 사이가 벌어지지 않고(실제 창에서 "안 녕"처럼 벌어졌다), 글리프보다 좁지는 않아 겹치지 않는다
        let cell = t.metrics().width as f32;
        if let Some(g) = t.glyph('한', FontStyle::Regular) {
            let a = t.advance('한');
            assert!(a < 2.0 * cell - 0.5, "두 칸({})보다 좁아야 한다: {a}", 2.0 * cell);
            assert!(a + 0.5 >= g.width as f32, "글리프 폭({})보다 넓어야 한다: {a}", g.width);
        }
    }

    #[test]
    fn proportional_title_color_emoji_advances_by_glyph_width() {
        // 비례폭 제목에서 색 이모지는 두 칸 폭이 아니라 그려지는 글리프 폭만큼 나아간다(#252). 고정폭이면 두 칸
        let Ok(s) = Fonts::new_with_title(&FontConfig::default(), Some("Helvetica")) else {
            return;
        };
        let Some(mut t) = s.title else { return };
        let Some(g) = t.glyph('😀', FontStyle::Regular) else {
            return;
        };
        if g.color.is_none() {
            return;
        }
        let a = t.advance('😀');
        let slot = 2.0 * t.metrics().width as f32;
        assert!(
            a + 0.5 >= g.width as f32,
            "글리프 폭({})보다 좁으면 겹친다: {a}",
            g.width
        );
        assert!(
            a <= g.width as f32 + 0.5,
            "글리프 폭({})보다 넓으면 여백이 생긴다: {a} (두 칸 {slot})",
            g.width
        );
        // 고정폭(터미널 글꼴) 경로는 그대로 두 칸
        let mut f = Fonts::new(&FontConfig::default()).unwrap();
        if f.glyph('😀', FontStyle::Regular).is_some_and(|g| g.color.is_some()) {
            assert_eq!(f.advance('😀'), 2.0 * f.metrics().width as f32);
        }
    }
}

#[cfg(test)]
mod shared_font_tests {
    use super::*;

    #[test]
    fn system_font_list_is_shared_until_forgotten() {
        let a = system_db();
        let b = system_db();
        assert!(Rc::ptr_eq(&a, &b), "창을 열 때마다 시스템 글꼴을 다시 훑으면 안 된다");
        forget_system_fonts();
        let c = system_db();
        assert!(!Rc::ptr_eq(&a, &c), "설정을 다시 읽으면 새로 훑는다");
    }

    #[test]
    fn windows_at_different_sizes_share_font_data() {
        let at = |size| FontConfig {
            size,
            ..FontConfig::default()
        };
        let a = Fonts::new(&at(13.0)).unwrap();
        let b = Fonts::new(&at(20.0)).unwrap();
        assert_ne!(a.metrics(), b.metrics());
        for i in 0..4 {
            assert!(
                Rc::ptr_eq(&a.regular[i], &b.regular[i]),
                "글꼴 파일 자료는 크기와 상관없이 하나"
            );
        }
        assert_eq!(a.fallbacks.len(), b.fallbacks.len());
        for (x, y) in a.fallbacks.iter().zip(&b.fallbacks) {
            assert!(Rc::ptr_eq(&x.font, &y.font));
        }
    }

    #[test]
    fn subset_cache_keeps_a_few_sizes() {
        let key = |px| (std::path::PathBuf::from("/x/emoji.ttc"), 0usize, px);
        let mut cache: Vec<SubsetEntry> = Vec::new();
        for px in [26u16, 40, 52] {
            subset_cache_put(&mut cache, key(px), Some(Rc::new(vec![px as u8])));
        }
        for px in [26u16, 40, 52] {
            assert!(subset_cache_get(&cache, &key(px)).is_some(), "{px}px");
        }
        for px in 100..110u16 {
            subset_cache_put(&mut cache, key(px), None);
        }
        assert!(cache.len() <= SUBSET_CACHE_MAX);
        assert!(
            subset_cache_get(&cache, &key(109)).is_some(),
            "가장 나중에 넣은 것은 남는다"
        );
    }
}
