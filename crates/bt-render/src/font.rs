//! 시스템 글꼴을 찾아 글리프를 래스터하고 캐시한다(ADR-002)

use std::collections::HashMap;
use std::rc::Rc;

use fontdb::{Database, Family, Query, Style, Weight};
use fontdue::{Font, FontSettings};

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
        let regular = Rc::new(regular);
        // 꼴마다 따로 정한 글꼴이 있으면 그것, 없으면 기본 글꼴의 그 꼴, 그것도 없으면 Regular
        let pick = |own: &Option<String>, w, s| {
            own.as_deref()
                .and_then(|name| load(db, name, w, s).or_else(|| load(db, name, Weight::NORMAL, Style::Normal)))
                .or_else(|| load(db, &family, w, s))
                .map(Rc::new)
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
                    font: Rc::new(f),
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
            // 사설 영역(Nerd Font 아이콘)은 기호 글꼴에서만 찾는다. 한글 글꼴이 같은 번호에 엉뚱한 글자를 둔 경우가 있다
            let pua = is_private_use(ch);
            let fb = self
                .fallbacks
                .iter()
                .find(|f| (!pua || f.pua) && f.font.lookup_glyph_index(ch) != 0)?;
            let wide = crate::text::char_cells(ch) == 2;
            (fb.font.clone(), if wide { self.px * fb.wide_scale } else { self.px })
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
    if font.lookup_glyph_index('가') == 0 {
        return 1.0;
    }
    let adv = font.metrics('가', px).advance_width;
    if adv <= 0.0 {
        return 1.0;
    }
    let by_width = (m.width as f32 * 2.0 * 0.92) / adv;
    let by_height = font
        .horizontal_line_metrics(px)
        .map(|l| (l.ascent - l.descent).max(1.0))
        .map(|h| m.height as f32 / h)
        .unwrap_or(by_width);
    by_width.min(by_height).clamp(1.0, 1.35)
}

fn cell_metrics(font: &Font, px: f32, adjust_w: Option<Adjust>, adjust_h: Option<Adjust>, scale: f32) -> CellMetrics {
    let advance = font.metrics('M', px).advance_width;
    let (ascent, descent, gap) = font
        .horizontal_line_metrics(px)
        .map(|l| (l.ascent, l.descent, l.line_gap))
        .unwrap_or((px * 0.8, -px * 0.2, 0.0));
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
