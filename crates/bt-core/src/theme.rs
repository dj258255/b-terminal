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

    /// 두 색을 섞는다. t=0이면 self, t=1이면 other
    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let f = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgb::new(f(self.r, other.r), f(self.g, other.g), f(self.b, other.b))
    }
}

/// 터미널 색 테마
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
        }
    }
}

impl Theme {
    /// xterm 256색표의 한 칸. 0~15는 팔레트, 16~231은 6단계 정육면체, 232~255는 회색 단계
    pub fn indexed(&self, index: u8) -> Rgb {
        match index {
            0..=15 => self.palette[index as usize],
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn u32_round_trip() {
        let c = Rgb::new(1, 2, 3);
        assert_eq!(Rgb::from_u32(c.to_u32()), c);
        assert_eq!(c.to_u32(), 0x010203);
    }
}
