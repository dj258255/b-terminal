//! 색 테마. VT 엔진의 색 이름(0~15번, 전경·배경·커서)을 실제 RGB로 바꿀 때 쓴다

use serde::{Deserialize, Serialize};

/// 24비트 색
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// softbuffer가 쓰는 0RGB 한 픽셀
    pub const fn to_u32(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }

    pub const fn from_u32(v: u32) -> Self {
        Self::new((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }

    /// `#rrggbb`·`rrggbb`를 읽는다. 모르는 꼴이면 없다
    pub fn parse_hex(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 || !s.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let v = u32::from_str_radix(s, 16).ok()?;
        Some(Self::from_u32(v))
    }

    /// 설정의 색 값을 Ghostty 1.4.0 `terminal/color.zig`의 `RGB.parse`대로 읽는다(#837).
    ///
    /// 앞뒤 공백·탭은 무시한다. 받는 꼴: `#rgb`·`#rrggbb`·`#rrrgggbbb`·`#rrrrggggbbbb`, X11 색 이름
    /// (대소문자 무시, 안쪽 빈칸은 표대로: `medium spring green`과 `MediumSpringGreen`은 되고 `medium springgreen`은 안 된다),
    /// `rgb`·`rrggbb`(`#` 없음), `rgb:r/g/b`(칸마다 1~4자리), `rgbi:r/g/b`(0.0~1.0).
    /// 칸의 비트 수는 `값*255/최댓값`(버림)으로 8비트가 되어 `#fff`는 `ffffff`, `#345`는 `334455`다.
    ///
    /// 터미널 색 OSC(`OSC 4`·`10`~`12`)는 이 함수가 아니라 VT 엔진의 xterm 규칙(낮은 자리를 0으로 채움)을 따른다(ADR-056·084).
    pub fn parse_color(s: &str) -> Option<Self> {
        let input = s.trim_matches([' ', '\t']).as_bytes();
        if input.is_empty() {
            return None;
        }
        if input[0] == b'#' {
            let n = match input.len() {
                4 => 1,
                7 => 2,
                10 => 3,
                13 => 4,
                _ => return None,
            };
            return Some(Self::new(
                hex_channel(&input[1..1 + n])?,
                hex_channel(&input[1 + n..1 + 2 * n])?,
                hex_channel(&input[1 + 2 * n..])?,
            ));
        }
        if let Some(c) = x11_color(input) {
            return Some(c);
        }
        if matches!(input.len(), 3 | 6) {
            let n = input.len() / 3;
            return Some(Self::new(
                hex_channel(&input[..n])?,
                hex_channel(&input[n..2 * n])?,
                hex_channel(&input[2 * n..])?,
            ));
        }
        if input.len() < "rgb:a/a/a".len() || &input[..3] != b"rgb" {
            return None;
        }
        let (intensity, rest) = match input[3] {
            b'i' => (true, &input[4..]),
            _ => (false, &input[3..]),
        };
        let rest = rest.strip_prefix(b":")?;
        let mut parts = rest.split(|&b| b == b'/');
        let (r, g, b) = (parts.next()?, parts.next()?, parts.next()?);
        if parts.next().is_some() {
            return None;
        }
        let ch = |p: &[u8]| {
            if intensity {
                intensity_channel(p)
            } else {
                hex_channel(p)
            }
        };
        Some(Self::new(ch(r)?, ch(g)?, ch(b)?))
    }

    /// WCAG 상대 휘도(0~1, #123)
    pub fn luminance(self) -> f32 {
        let lin = |c: u8| {
            let c = c as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.r) + 0.7152 * lin(self.g) + 0.0722 * lin(self.b)
    }

    /// WCAG 명암비(1~21, #123)
    pub fn contrast(self, other: Rgb) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// 바탕 `bg` 위에서 명암비가 `min`보다 낮으면 흰색과 검은색 중 바탕과 명암비가 큰 쪽으로 바꾼다
    /// (Ghostty `minimum-contrast`와 같은 규칙). `min`이 1 이하면 그대로
    pub fn contrasted(self, bg: Rgb, min: f32) -> Rgb {
        if min <= 1.0 || self.contrast(bg) >= min {
            return self;
        }
        let (white, black) = (Rgb::new(255, 255, 255), Rgb::new(0, 0, 0));
        if white.contrast(bg) > black.contrast(bg) {
            white
        } else {
            black
        }
    }

    /// 두 색을 섞는다. t=0이면 self, t=1이면 other
    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let f = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb::new(f(self.r, other.r), f(self.g, other.g), f(self.b, other.b))
    }
}

/// 찾기 강조에 쓰는 색 하나(Ghostty `search-*`의 값). 색이거나 칸의 글자색·바탕색을 따른다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchColor {
    Rgb(Rgb),
    /// `cell-foreground`: 칸의 글자색
    CellForeground,
    /// `cell-background`: 칸의 바탕색
    CellBackground,
}

impl SearchColor {
    /// 칸의 글자색·바탕색이 주어졌을 때 실제 색
    pub fn resolve(self, cell_fg: Rgb, cell_bg: Rgb) -> Rgb {
        match self {
            SearchColor::Rgb(c) => c,
            SearchColor::CellForeground => cell_fg,
            SearchColor::CellBackground => cell_bg,
        }
    }
}

/// 찾기에 맞은 칸의 색(Ghostty `search-foreground`·`search-background`·`search-selected-*`).
/// 기본은 Ghostty처럼 검정 글자에 노란 바탕(맞은 곳), 검정 글자에 살구색 바탕(지금 고른 곳)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchColors {
    pub foreground: SearchColor,
    pub background: SearchColor,
    pub selected_foreground: SearchColor,
    pub selected_background: SearchColor,
}

impl Default for SearchColors {
    fn default() -> Self {
        Self {
            foreground: SearchColor::Rgb(Rgb::new(0, 0, 0)),
            background: SearchColor::Rgb(Rgb::new(0xff, 0xe0, 0x82)),
            selected_foreground: SearchColor::Rgb(Rgb::new(0, 0, 0)),
            selected_background: SearchColor::Rgb(Rgb::new(0xf2, 0xa5, 0x7e)),
        }
    }
}

impl SearchColors {
    /// 찾기에 맞은 칸의 (글자색, 바탕색). `current`면 지금 고른 맞은 곳
    pub fn pick(&self, current: bool, cell_fg: Rgb, cell_bg: Rgb) -> (Rgb, Rgb) {
        let (fg, bg) = if current {
            (self.selected_foreground, self.selected_background)
        } else {
            (self.foreground, self.background)
        };
        (fg.resolve(cell_fg, cell_bg), bg.resolve(cell_fg, cell_bg))
    }
}

/// kitty 마커 색(`mark1_foreground`~`mark3_background`, #929). 마커 번호 1~3
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkColors {
    pub foreground: [Rgb; 3],
    pub background: [Rgb; 3],
}

impl Default for MarkColors {
    /// kitty `definition.py`의 기본: 검정 글자에 #98d3cb·#f2dcd3·#f274bc 바탕
    fn default() -> Self {
        Self {
            foreground: [Rgb::new(0, 0, 0); 3],
            background: [
                Rgb::new(0x98, 0xd3, 0xcb),
                Rgb::new(0xf2, 0xdc, 0xd3),
                Rgb::new(0xf2, 0x74, 0xbc),
            ],
        }
    }
}

impl MarkColors {
    /// 마커 번호(1~3, 그 밖은 가까운 쪽으로 맞춘다)의 (글자색, 바탕색)
    pub fn pick(&self, mark: u8) -> (Rgb, Rgb) {
        let i = usize::from(mark.clamp(1, 3)) - 1;
        (self.foreground[i], self.background[i])
    }
}

/// 설정이나 테마 파일이 직접 정한 색(#838). 정하지 않은 커서색·커서 글자색·고름 바탕·고름 글자는
/// 그릴 때 지금 글자색·바탕색(OSC 10·11 반영)에서 나온다(Ghostty `generic.zig`: `cursor-color`·`cursor-text`·`selection-*`의 기본은 정하지 않음)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplicitColors {
    pub cursor: bool,
    pub cursor_text: bool,
    pub selection_background: bool,
    pub selection_foreground: bool,
}

/// 터미널 색 테마
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
    /// 커서 칸 안의 글자색
    pub cursor_text: Rgb,
    pub selection_background: Rgb,
    /// 고른 칸의 글자색. 없으면 칸의 글자색 그대로
    pub selection_foreground: Option<Rgb>,
    /// 0~15번 색
    pub palette: [Rgb; 16],
    /// 굵은 글자의 0~7번 색을 밝은 8~15번으로 올린다(Ghostty 기본은 끔)
    pub bold_is_bright: bool,
    /// 굵은 기본 글자색(Ghostty `bold-color`의 색 값). 있으면 나머지 굵은 0~7번 색은 밝은 색으로 올린다
    pub bold_color: Option<Rgb>,
    /// 흐린(faint, SGR 2) 글자의 불투명도(0~1, Ghostty `faint-opacity`)
    pub faint_opacity: f32,
    /// 찾기에 맞은 칸의 색
    pub search: SearchColors,
    /// kitty 마커 색(#929)
    #[serde(default)]
    pub marks: MarkColors,
    /// 16~255번 색(240개). 비어 있으면 xterm 표를 그대로 쓴다. `palette-generate`로 만들었거나
    /// `palette = 100=#...`로 바꾼 값이 있을 때만 채운다
    #[serde(default)]
    pub extended: Vec<Rgb>,
    /// 위 색 가운데 설정이 직접 정한 것(#838)
    #[serde(default)]
    pub explicit: ExplicitColors,
}

impl Default for Theme {
    fn default() -> Self {
        Self::ghostty_default()
    }
}

impl Theme {
    /// Ghostty 1.3의 기본 색(`ghostty +show-config --default`로 확인). 고른 칸은 Ghostty처럼 색을 뒤집는다
    pub fn ghostty_default() -> Self {
        let background = Rgb::new(0x28, 0x2c, 0x34);
        let foreground = Rgb::new(0xff, 0xff, 0xff);
        Self {
            foreground,
            background,
            cursor: foreground,
            cursor_text: background,
            selection_background: foreground,
            selection_foreground: Some(background),
            palette: [
                Rgb::new(0x1d, 0x1f, 0x21),
                Rgb::new(0xcc, 0x66, 0x66),
                Rgb::new(0xb5, 0xbd, 0x68),
                Rgb::new(0xf0, 0xc6, 0x74),
                Rgb::new(0x81, 0xa2, 0xbe),
                Rgb::new(0xb2, 0x94, 0xbb),
                Rgb::new(0x8a, 0xbe, 0xb7),
                Rgb::new(0xc5, 0xc8, 0xc6),
                Rgb::new(0x66, 0x66, 0x66),
                Rgb::new(0xd5, 0x4e, 0x53),
                Rgb::new(0xb9, 0xca, 0x4a),
                Rgb::new(0xe7, 0xc5, 0x47),
                Rgb::new(0x7a, 0xa6, 0xda),
                Rgb::new(0xc3, 0x97, 0xd8),
                Rgb::new(0x70, 0xc0, 0xb1),
                Rgb::new(0xea, 0xea, 0xea),
            ],
            bold_is_bright: false,
            bold_color: None,
            faint_opacity: 0.5,
            search: SearchColors::default(),
            marks: MarkColors::default(),
            extended: Vec::new(),
            explicit: ExplicitColors::default(),
        }
    }
}

/// 16~255번 색 240개를 만든다(Ghostty `palette-generate`, jake-stewart의 방법).
/// 바탕·글자색과 기본 8색(1~6번)을 CIELAB에서 3선형 보간해 6x6x6 정육면체를, 바탕에서 글자색까지의 보간으로 회색 단계 24개를 만든다.
/// 밝은 테마(글자색이 바탕보다 어두움)는 기본으로 바탕과 글자색을 맞바꿔 16번이 어둡고 231번이 밝은 옛 순서를 지킨다.
/// `harmonious`면 맞바꾸지 않고 테마의 뜻 그대로 둔다
pub fn generate_palette(base: &[Rgb; 16], bg: Rgb, fg: Rgb, harmonious: bool) -> Vec<Rgb> {
    let mut c8 = [
        Lab::from_rgb(bg),
        Lab::from_rgb(base[1]),
        Lab::from_rgb(base[2]),
        Lab::from_rgb(base[3]),
        Lab::from_rgb(base[4]),
        Lab::from_rgb(base[5]),
        Lab::from_rgb(base[6]),
        Lab::from_rgb(fg),
    ];
    if c8[7].l < c8[0].l && !harmonious {
        c8.swap(0, 7);
    }
    let mut out = Vec::with_capacity(240);
    for r in 0..6 {
        let tr = r as f64 / 5.0;
        let c0 = Lab::lerp(tr, c8[0], c8[1]);
        let c1 = Lab::lerp(tr, c8[2], c8[3]);
        let c2 = Lab::lerp(tr, c8[4], c8[5]);
        let c3 = Lab::lerp(tr, c8[6], c8[7]);
        for g in 0..6 {
            let tg = g as f64 / 5.0;
            let c4 = Lab::lerp(tg, c0, c1);
            let c5 = Lab::lerp(tg, c2, c3);
            for b in 0..6 {
                out.push(Lab::lerp(b as f64 / 5.0, c4, c5).to_rgb());
            }
        }
    }
    for i in 0..24 {
        out.push(Lab::lerp((i + 1) as f64 / 25.0, c8[0], c8[7]).to_rgb());
    }
    out
}

/// CIELAB(D65). 팔레트를 만들 때만 쓴다
#[derive(Clone, Copy)]
struct Lab {
    l: f64,
    a: f64,
    b: f64,
}

impl Lab {
    const WHITE: (f64, f64, f64) = (0.95047, 1.0, 1.08883);

    fn from_rgb(c: Rgb) -> Self {
        let lin = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let (r, g, b) = (lin(c.r), lin(c.g), lin(c.b));
        let x = 0.4124564 * r + 0.3575761 * g + 0.1804375 * b;
        let y = 0.2126729 * r + 0.7151522 * g + 0.0721750 * b;
        let z = 0.0193339 * r + 0.1191920 * g + 0.9503041 * b;
        let f = |t: f64| {
            if t > 216.0 / 24389.0 {
                t.cbrt()
            } else {
                (24389.0 / 27.0 * t + 16.0) / 116.0
            }
        };
        let (fx, fy, fz) = (f(x / Self::WHITE.0), f(y / Self::WHITE.1), f(z / Self::WHITE.2));
        Self {
            l: 116.0 * fy - 16.0,
            a: 500.0 * (fx - fy),
            b: 200.0 * (fy - fz),
        }
    }

    fn to_rgb(self) -> Rgb {
        let fy = (self.l + 16.0) / 116.0;
        let (fx, fz) = (fy + self.a / 500.0, fy - self.b / 200.0);
        let inv = |t: f64| {
            if t.powi(3) > 216.0 / 24389.0 {
                t.powi(3)
            } else {
                (116.0 * t - 16.0) / (24389.0 / 27.0)
            }
        };
        let (x, y, z) = (
            inv(fx) * Self::WHITE.0,
            inv(fy) * Self::WHITE.1,
            inv(fz) * Self::WHITE.2,
        );
        let r = 3.2404542 * x - 1.5371385 * y - 0.4985314 * z;
        let g = -0.9692660 * x + 1.8760108 * y + 0.0415560 * z;
        let b = 0.0556434 * x - 0.2040259 * y + 1.0572252 * z;
        let gamma = |v: f64| {
            let v = v.clamp(0.0, 1.0);
            let v = if v <= 0.0031308 {
                12.92 * v
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            };
            (v * 255.0).round() as u8
        };
        Rgb::new(gamma(r), gamma(g), gamma(b))
    }

    fn lerp(t: f64, a: Lab, b: Lab) -> Lab {
        Lab {
            l: a.l + t * (b.l - a.l),
            a: a.a + t * (b.a - a.a),
            b: a.b + t * (b.b - a.b),
        }
    }
}

impl Theme {
    /// 16~255번 색을 정한다: `generate`면 기본 16색에서 만들고, 아니면 xterm 표에서 시작한다.
    /// `set`에 있는 번호(사용자가 `palette`로 정한 값)는 어느 쪽이든 덮어쓰지 않고 그 값을 쓴다.
    /// 만들 것도 바꿀 것도 없으면 비워 둔다
    /// 정하지 않은 커서색·커서 글자색·고름 바탕·고름 글자를 지금 글자색·바탕색에서 채운다(#838).
    /// 설정을 다 읽은 뒤에 불러서 `foreground`·`background`가 어느 순서로 와도 같은 결과다
    pub fn settle_unset_colors(&mut self) {
        if !self.explicit.cursor {
            self.cursor = self.foreground;
        }
        if !self.explicit.cursor_text {
            self.cursor_text = self.background;
        }
        if !self.explicit.selection_background {
            self.selection_background = self.foreground;
        }
        if !self.explicit.selection_foreground {
            self.selection_foreground = Some(self.background);
        }
    }

    /// 커서 색: 정했으면 그 색, 아니면 `foreground`(지금 글자색, OSC 10이 바꿨을 수 있다)
    pub fn cursor_or(&self, foreground: Rgb) -> Rgb {
        if self.explicit.cursor { self.cursor } else { foreground }
    }

    pub fn build_extended(&mut self, generate: bool, harmonious: bool, set: &[(u8, Rgb)]) {
        if !generate && set.is_empty() {
            self.extended.clear();
            return;
        }
        let mut table = if generate {
            generate_palette(&self.palette, self.background, self.foreground, harmonious)
        } else {
            (16..=255u8).map(Self::xterm).collect()
        };
        for &(i, c) in set {
            if i >= 16 {
                table[i as usize - 16] = c;
            }
        }
        self.extended = table;
    }
}

impl Theme {
    /// xterm 256색표의 한 칸. 0~15는 팔레트, 16~231은 6단계 정육면체, 232~255는 회색 단계
    pub fn indexed(&self, index: u8) -> Rgb {
        match index {
            0..=15 => self.palette[index as usize],
            _ if self.extended.len() == 240 => self.extended[index as usize - 16],
            _ => Self::xterm(index),
        }
    }

    /// xterm 표의 16~255번 색
    fn xterm(index: u8) -> Rgb {
        match index {
            0..=15 => Rgb::new(0, 0, 0),
            16..=231 => {
                let i = index - 16;
                let step = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                Rgb::new(step(i / 36), step((i / 6) % 6), step(i % 6))
            }
            _ => {
                let v = 8 + (index - 232) * 10;
                Rgb::new(v, v, v)
            }
        }
    }
}

/// X11 색 이름 표(Ghostty `src/terminal/res/rgb.txt` 782줄, `vendor/vte`와 같은 파일). 줄 모양은 `RRR GGG BBB<탭>이름`
const X11_RGB: &str = include_str!("../../../vendor/vte/src/x11_rgb.txt");

/// X11 색 이름 표를 파일 차례대로: `(이름, 색)`. `+list-colors`가 쓴다
pub fn x11_colors() -> impl Iterator<Item = (&'static str, Rgb)> {
    X11_RGB.lines().filter_map(|line| {
        let line = line.trim_end_matches('\r');
        let name = line.get(12..)?.trim_matches([' ', '\t']);
        let ch = |r: std::ops::Range<usize>| line.get(r)?.trim().parse::<u8>().ok();
        Some((name, Rgb::new(ch(0..3)?, ch(4..7)?, ch(8..11)?)))
    })
}

/// 이름(대소문자 무시, 안쪽 빈칸은 그대로)으로 X11 표에서 찾는다
fn x11_color(name: &[u8]) -> Option<Rgb> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    static MAP: OnceLock<HashMap<String, Rgb>> = OnceLock::new();
    let map = MAP.get_or_init(|| {
        let mut m = HashMap::new();
        for (n, c) in x11_colors() {
            m.entry(n.to_ascii_lowercase()).or_insert(c);
        }
        m
    });
    map.get(std::str::from_utf8(name).ok()?.to_ascii_lowercase().as_str())
        .copied()
}

/// 16진수 1~4자리(숫자만)를 `값*255/최댓값`(버림)으로 8비트로 만든다(Ghostty `RGB.fromHex`). `+`·`-`·`_`는 받지 않는다
fn hex_channel(d: &[u8]) -> Option<u8> {
    if d.is_empty() || d.len() > 4 || !d.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let v = u32::from_str_radix(std::str::from_utf8(d).ok()?, 16).ok()?;
    let max = (1u32 << (4 * d.len())) - 1;
    Some((v * 255 / max) as u8)
}

/// `rgbi:`의 한 칸: 0.0~1.0 십진 분수(부호 하나 가능, 숫자와 소수점 하나, 숫자 한 자리 이상, 지수 없음)를 버림으로 8비트로(Ghostty `fromIntensity`)
fn intensity_channel(p: &[u8]) -> Option<u8> {
    let body = match p.first() {
        Some(b'+' | b'-') => &p[1..],
        _ => p,
    };
    let digits = body.iter().filter(|b| b.is_ascii_digit()).count();
    let dots = body.iter().filter(|&&b| b == b'.').count();
    if digits == 0 || dots > 1 || digits + dots != body.len() {
        return None;
    }
    let f: f64 = std::str::from_utf8(p).ok()?.parse().ok()?;
    (0.0..=1.0).contains(&f).then_some((f * 255.0) as u8)
}

/// `palette = N=색`의 값을 Ghostty `parsePaletteEntry`대로 읽는다(#837). 번호는 10진수나 `0x`·`0o`·`0b` 접두 수(0~255,
/// 앞뒤 공백·탭 무시), 색은 [`Rgb::parse_color`]. 번호가 0~255를 넘으면 `Err(true)`, 모양이 틀리면 `Err(false)`
pub fn parse_palette_entry(v: &str) -> Result<(u8, Rgb), bool> {
    let (n, c) = v.split_once('=').ok_or(false)?;
    let idx = parse_int_base0(n.trim_matches([' ', '\t']))?;
    let color = Rgb::parse_color(c).ok_or(false)?;
    Ok((idx, color))
}

/// Zig `std.fmt.parseInt(u8, s, 0)`: 부호 하나, `0x`·`0o`·`0b` 접두(소문자), 숫자 사이의 `_`. 넘치면 `Err(true)`
fn parse_int_base0(s: &str) -> Result<u8, bool> {
    let (neg, body) = match s.as_bytes().first() {
        Some(b'+') => (false, &s[1..]),
        Some(b'-') => (true, &s[1..]),
        _ => (false, s),
    };
    let (radix, digits) = match body.get(..2) {
        Some("0x") if body.len() > 2 => (16, &body[2..]),
        Some("0o") if body.len() > 2 => (8, &body[2..]),
        Some("0b") if body.len() > 2 => (2, &body[2..]),
        _ => (10, body),
    };
    if digits.is_empty() || digits.starts_with('_') || digits.ends_with('_') || digits.contains("__") {
        return Err(false);
    }
    let mut acc: u32 = 0;
    for c in digits.chars().filter(|&c| c != '_') {
        let d = c.to_digit(radix).ok_or(false)?;
        acc = (acc * radix + d).min(1000);
    }
    // 부호 없는 수에 `-`가 붙으면 0만 받는다(Zig는 넘침으로 본다)
    if acc > 255 || (neg && acc != 0) {
        return Err(true);
    }
    Ok(acc as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_matches_wcag_and_fixes_unreadable_colors_like_ghostty() {
        let (white, black) = (Rgb::new(255, 255, 255), Rgb::new(0, 0, 0));
        assert!((white.contrast(black) - 21.0).abs() < 0.01);
        assert!((black.contrast(white) - 21.0).abs() < 0.01, "순서와 상관없다");
        assert!((white.contrast(white) - 1.0).abs() < 1e-6);
        // WCAG 예시: #777777은 흰 바탕에서 약 4.48
        assert!((Rgb::new(0x77, 0x77, 0x77).contrast(white) - 4.48).abs() < 0.01);
        assert!((Rgb::new(255, 0, 0).luminance() - 0.2126).abs() < 1e-4);
        // 어두운 바탕에 짙은 파랑: 3을 원하면 흰색으로
        let (navy, bg) = (Rgb::new(0, 0, 0x80), Rgb::new(0x1e, 0x1e, 0x2e));
        assert!(navy.contrast(bg) < 3.0);
        assert_eq!(navy.contrasted(bg, 3.0), white);
        // 밝은 바탕에 연노랑: 검은색으로
        let (pale, light) = (Rgb::new(0xff, 0xff, 0xcc), Rgb::new(0xf0, 0xf0, 0xf0));
        assert_eq!(pale.contrasted(light, 3.0), black);
        // 이미 충분하거나 min이 1이면 그대로
        assert_eq!(navy.contrasted(white, 3.0), navy);
        assert_eq!(navy.contrasted(bg, 1.0), navy);
    }

    #[test]
    fn search_colors_default_and_cell_colors_like_ghostty() {
        let (cfg, cbg) = (Rgb::new(10, 20, 30), Rgb::new(200, 100, 50));
        let d = SearchColors::default();
        assert_eq!(d.pick(false, cfg, cbg), (Rgb::new(0, 0, 0), Rgb::new(0xff, 0xe0, 0x82)));
        assert_eq!(d.pick(true, cfg, cbg), (Rgb::new(0, 0, 0), Rgb::new(0xf2, 0xa5, 0x7e)));
        let s = SearchColors {
            foreground: SearchColor::CellBackground,
            background: SearchColor::CellForeground,
            selected_foreground: SearchColor::CellForeground,
            selected_background: SearchColor::Rgb(Rgb::new(1, 2, 3)),
        };
        assert_eq!(s.pick(false, cfg, cbg), (cbg, cfg));
        assert_eq!(s.pick(true, cfg, cbg), (cfg, Rgb::new(1, 2, 3)));
    }

    fn near(a: Rgb, b: Rgb) -> bool {
        let d = |x: u8, y: u8| (x as i32 - y as i32).abs();
        d(a.r, b.r) <= 1 && d(a.g, b.g) <= 1 && d(a.b, b.b) <= 1
    }

    fn dark_theme() -> Theme {
        Theme::default()
    }

    /// #177: 정육면체의 여덟 모서리는 바탕·기본 6색·글자색이고, 회색 단계는 바탕에서 글자색으로 오른다
    #[test]
    fn generate_palette_corners_and_gray_ramp() {
        let t = dark_theme();
        let p = generate_palette(&t.palette, t.background, t.foreground, false);
        assert_eq!(p.len(), 240);
        let at = |i: usize| p[i - 16];
        assert!(near(at(16), t.background), "16번(0,0,0)은 바탕: {:?}", at(16));
        assert!(near(at(231), t.foreground), "231번(5,5,5)은 글자색: {:?}", at(231));
        assert!(near(at(16 + 36 * 5), t.palette[1]), "(5,0,0)은 빨강");
        assert!(near(at(16 + 6 * 5), t.palette[2]), "(0,5,0)은 초록");
        assert!(near(at(16 + 36 * 5 + 6 * 5), t.palette[3]), "(5,5,0)은 노랑");
        assert!(near(at(16 + 5), t.palette[4]), "(0,0,5)는 파랑");
        assert!(near(at(16 + 36 * 5 + 5), t.palette[5]), "(5,0,5)는 자홍");
        assert!(near(at(16 + 6 * 5 + 5), t.palette[6]), "(0,5,5)는 청록");
        // 회색 단계: 232번은 바탕과 글자색 사이 1/25, 255번은 24/25. 밝기가 계속 오른다
        let lum: Vec<f32> = (232..=255).map(|i| at(i).luminance()).collect();
        assert!(lum.windows(2).all(|w| w[0] < w[1]), "회색은 어두운 쪽에서 밝은 쪽으로");
        assert!(at(232).luminance() > t.background.luminance());
        assert!(at(255).luminance() < t.foreground.luminance());
    }

    /// #177: 밝은 테마는 기본으로 바탕과 글자색을 맞바꿔 16번이 어둡고, harmonious면 테마 그대로
    #[test]
    fn generate_palette_light_theme_swaps_unless_harmonious() {
        let t = dark_theme();
        let (light_bg, dark_fg) = (Rgb::new(0xfd, 0xf6, 0xe3), Rgb::new(0x65, 0x7b, 0x83));
        let legacy = generate_palette(&t.palette, light_bg, dark_fg, false);
        assert!(near(legacy[0], dark_fg), "16번은 어두운 글자색");
        assert!(near(legacy[215], light_bg), "231번은 밝은 바탕");
        let harmonious = generate_palette(&t.palette, light_bg, dark_fg, true);
        assert!(near(harmonious[0], light_bg), "harmonious는 16번이 바탕");
        assert!(near(harmonious[215], dark_fg));
        // 어두운 테마는 harmonious여도 같다
        assert_eq!(
            generate_palette(&t.palette, t.background, t.foreground, true),
            generate_palette(&t.palette, t.background, t.foreground, false)
        );
    }

    /// #177: CIELAB 보간이라 가운데 칸이 RGB 평균과 다르다(검정-흰색 사이 회색의 L이 직선)
    #[test]
    fn generate_palette_interpolates_in_lab() {
        let base = Theme::default().palette;
        let p = generate_palette(&base, Rgb::new(0, 0, 0), Rgb::new(255, 255, 255), false);
        // 232번 = L 4% 지점(1/25). RGB 선형 보간이면 10, Lab이면 약 9~10이 아니라 L*에 선형인 값
        // L*=4 -> Y=4/903.3 -> sRGB 감마 적용으로 약 0x0e
        assert_eq!(p[232 - 16], Rgb::new(0x0e, 0x0e, 0x0e));
        // 128 근처: 12/25 지점이 아니라 정확히 한가운데는 없고, 243번(12/25)은 L=48 -> 약 0x73
        let mid = p[243 - 16];
        assert!(
            (mid.r as i32 - 0x74).abs() <= 2 && mid.r == mid.g && mid.g == mid.b,
            "{mid:?}"
        );
    }

    /// #177: 만들지 않고 바꾼 것도 없으면 xterm 표 그대로, 있으면 바꾼 번호만 그 값
    #[test]
    fn build_extended_keeps_explicit_entries_and_xterm_by_default() {
        let mut t = dark_theme();
        t.build_extended(false, false, &[]);
        assert!(t.extended.is_empty(), "할 일이 없으면 비워 둔다");
        assert_eq!(t.indexed(196), Rgb::new(255, 0, 0));

        let mine = Rgb::new(1, 2, 3);
        t.build_extended(false, false, &[(100, mine)]);
        assert_eq!(t.indexed(100), mine);
        assert_eq!(t.indexed(196), Rgb::new(255, 0, 0), "나머지는 xterm");
        assert_eq!(t.indexed(255), Rgb::new(238, 238, 238));

        let mut g = dark_theme();
        g.build_extended(true, false, &[(100, mine)]);
        assert_eq!(g.indexed(100), mine, "정한 값은 만든 색이 덮지 않는다");
        assert_ne!(g.indexed(196), Rgb::new(255, 0, 0), "나머지는 만든 색");
        assert!(near(g.indexed(16), g.background));
        assert_eq!(g.indexed(5), g.palette[5], "0~15는 그대로");
    }

    #[test]
    fn xterm_256_cube_and_gray() {
        let t = Theme::default();
        assert_eq!(t.indexed(16), Rgb::new(0, 0, 0));
        assert_eq!(t.indexed(196), Rgb::new(255, 0, 0));
        assert_eq!(t.indexed(231), Rgb::new(255, 255, 255));
        assert_eq!(t.indexed(232), Rgb::new(8, 8, 8));
        assert_eq!(t.indexed(255), Rgb::new(238, 238, 238));
        assert_eq!(t.indexed(1), t.palette[1]);
    }

    #[test]
    fn parse_hex_accepts_hash_and_rejects_garbage() {
        assert_eq!(Rgb::parse_hex("#ff8000"), Some(Rgb::new(255, 128, 0)));
        assert_eq!(Rgb::parse_hex("ff8000"), Some(Rgb::new(255, 128, 0)));
        assert_eq!(Rgb::parse_hex("#ff80"), None);
        assert_eq!(Rgb::parse_hex("zzzzzz"), None);
    }

    /// #837: Ghostty `color.zig` `RGB.parse` 시험의 값
    #[test]
    fn parse_color_matches_ghostty_rgb_parse() {
        let c = |s| Rgb::parse_color(s);
        assert_eq!(c("rgbi:1.0/0/0"), Some(Rgb::new(255, 0, 0)));
        assert_eq!(c("rgb:7f/a0a0/0"), Some(Rgb::new(127, 160, 0)));
        assert_eq!(c("rgb:f/ff/fff"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(c("#ff0010"), Some(Rgb::new(255, 0, 16)));
        assert_eq!(c("0A0B0C"), Some(Rgb::new(10, 11, 12)));
        assert_eq!(c("FFF"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(c("#345"), Some(Rgb::new(51, 68, 85)));
        assert_eq!(c("#fffffffff"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(c("#ffffffffffff"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(c(" #AABBCC   "), Some(Rgb::new(170, 187, 204)));
        assert_eq!(c("white"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(c("LawnGreen"), Some(Rgb::new(124, 252, 0)));
        assert_eq!(c("medium spring green"), Some(Rgb::new(0, 250, 154)));
        assert_eq!(c(" Forest Green "), Some(Rgb::new(34, 139, 34)));
        assert_eq!(c("\tForestGreen\t"), Some(Rgb::new(34, 139, 34)));
        for bad in [
            "",
            "  ",
            "rgb;",
            "rgb:",
            ":a/a/a",
            "a/a/a",
            "rgb:a/a/a/",
            "rgb:00000///",
            "rgb:000/",
            "rgbi:a/a/a",
            "rgb:0.5/0.0/1.0",
            "rgb:not/hex/zz",
            "#",
            "#ff",
            "#ffff",
            "#fffff",
            "#gggggg",
            "#12345",
            "12345",
            "nosuchcolor",
            "rgb:f_f/0/0",
            "#f_f000000",
            "medium springgreen",
        ] {
            assert_eq!(c(bad), None, "{bad:?}");
        }
        assert_eq!(x11_colors().count(), 782);
    }

    /// #837: Ghostty `color.zig` `parsePaletteEntry` 시험의 값
    #[test]
    fn parse_palette_entry_matches_ghostty() {
        let p = parse_palette_entry;
        assert_eq!(p("0=#AABBCC"), Ok((0, Rgb::new(170, 187, 204))));
        assert_eq!(p("0b1=#014589"), Ok((1, Rgb::new(1, 69, 137))));
        assert_eq!(p("0o7=#234567"), Ok((7, Rgb::new(35, 69, 103))));
        assert_eq!(p("0xF=#ABCDEF"), Ok((15, Rgb::new(171, 205, 239))));
        assert_eq!(p("0 =  #AABBCC"), Ok((0, Rgb::new(170, 187, 204))));
        assert_eq!(p(" 1= #DDEEFF    "), Ok((1, Rgb::new(221, 238, 255))));
        assert_eq!(p("  2  =  #123456 "), Ok((2, Rgb::new(18, 52, 86))));
        assert_eq!(p("1=black"), Ok((1, Rgb::new(0, 0, 0))));
        assert_eq!(p("256=#fff"), Err(true));
        assert_eq!(p("0x100=#fff"), Err(true));
        assert_eq!(p("x=#fff"), Err(false));
        assert_eq!(p("#fff"), Err(false));
        assert_eq!(p("1=nosuch"), Err(false));
        assert_eq!(p("=#fff"), Err(false));
    }

    #[test]
    fn u32_round_trip() {
        let c = Rgb::new(1, 2, 3);
        assert_eq!(Rgb::from_u32(c.to_u32()), c);
        assert_eq!(c.to_u32(), 0x010203);
    }
}
