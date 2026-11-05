//! 시스템 글꼴을 찾아 글리프를 래스터하고 캐시한다(ADR-002)

use std::collections::HashMap;
use std::rc::Rc;

use fontdb::{Database, Family, Query, Style, Weight};
use fontdue::{Font, FontSettings};

/// 글꼴 설정
#[derive(Debug, Clone, PartialEq)]
pub struct FontConfig {
    /// 기본 글꼴 이름. 없거나 못 찾으면 고정폭 후보를 차례로 찾는다
    pub family: Option<String>,
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
            size: 14.0,
            scale: 1.0,
            fallbacks: vec![
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
            ],
        }
    }
}

/// 기본 글꼴을 못 찾았을 때 차례로 찾는 고정폭 글꼴
const MONO_CANDIDATES: &[&str] = &[
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
    /// 대체 글꼴과 넓은 글자를 그릴 때의 크기 배율
    fallbacks: Vec<(Rc<Font>, f32)>,
    px: f32,
    metrics: CellMetrics,
    cache: HashMap<(char, FontStyle), Option<Rc<Glyph>>>,
    pub family_name: String,
}

#[derive(Debug, thiserror::Error)]
pub enum FontError {
    #[error("고정폭 글꼴을 찾지 못했습니다(찾은 이름: {0})")]
    NotFound(String),
}

fn load(db: &Database, family: &str, weight: Weight, style: Style) -> Option<Font> {
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
    db.with_face_data(id, |data, index| {
        Font::from_bytes(
            data,
            FontSettings {
                collection_index: index,
                scale: 40.0,
                ..FontSettings::default()
            },
        )
        .ok()
    })
    .flatten()
}

impl Fonts {
    /// 시스템 글꼴을 읽어 묶음을 만든다
    pub fn new(config: &FontConfig) -> Result<Self, FontError> {
        let mut db = Database::new();
        db.load_system_fonts();
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
        let regular = Rc::new(regular);
        let pick = |w, s| load(db, &family, w, s).map(Rc::new).unwrap_or_else(|| regular.clone());
        let bold = pick(Weight::BOLD, Style::Normal);
        let italic = pick(Weight::NORMAL, Style::Italic);
        let bold_italic = pick(Weight::BOLD, Style::Italic);
        let px = (config.size * config.scale).max(4.0);
        let metrics = cell_metrics(&regular, px);
        let fallbacks = config
            .fallbacks
            .iter()
            .filter(|n| !n.eq_ignore_ascii_case(&family))
            .filter_map(|n| load(db, n, Weight::NORMAL, Style::Normal))
            .map(|f| {
                let k = wide_scale(&f, px, metrics);
                (Rc::new(f), k)
            })
            .collect();
        Ok(Self {
            regular: [regular, bold, italic, bold_italic],
            fallbacks,
            px,
            metrics,
            cache: HashMap::new(),
            family_name: family,
        })
    }

    pub fn metrics(&self) -> CellMetrics {
        self.metrics
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
        let idx = style as usize;
        let primary = &self.regular[idx];
        let (font, px) = if primary.lookup_glyph_index(ch) != 0 {
            (primary.clone(), self.px)
        } else if self.regular[0].lookup_glyph_index(ch) != 0 {
            (self.regular[0].clone(), self.px)
        } else {
            let (f, k) = self.fallbacks.iter().find(|(f, _)| f.lookup_glyph_index(ch) != 0)?;
            let wide = crate::text::char_cells(ch) == 2;
            (f.clone(), if wide { self.px * k } else { self.px })
        };
        let (m, bitmap) = font.rasterize(ch, px);
        Some(Glyph {
            width: m.width,
            height: m.height,
            left: m.xmin,
            top: m.ymin + m.height as i32,
            bitmap,
        })
    }
}

/// 대체 글꼴의 넓은 글자(한글)를 두 칸에 맞추는 배율. 기본 글꼴의 두 칸보다 좁으면 벌어져 보인다.
/// 셀 높이를 넘지 않게 1.0~1.12로 묶는다(Ghostty의 대체 글꼴 크기 맞춤과 같은 생각)
fn wide_scale(font: &Font, px: f32, m: CellMetrics) -> f32 {
    if font.lookup_glyph_index('가') == 0 {
        return 1.0;
    }
    let adv = font.metrics('가', px).advance_width;
    if adv <= 0.0 {
        return 1.0;
    }
    ((m.width as f32 * 2.0 * 0.92) / adv).clamp(1.0, 1.12)
}

fn cell_metrics(font: &Font, px: f32) -> CellMetrics {
    let advance = font.metrics('M', px).advance_width;
    let (ascent, descent, gap) = font
        .horizontal_line_metrics(px)
        .map(|l| (l.ascent, l.descent, l.line_gap))
        .unwrap_or((px * 0.8, -px * 0.2, 0.0));
    let width = advance.round().max(1.0) as u32;
    let height = (ascent - descent + gap).ceil().max(1.0) as u32;
    let baseline = (ascent + gap / 2.0).round() as u32;
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
