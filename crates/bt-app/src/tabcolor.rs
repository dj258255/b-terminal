//! 탭 색(#852). Ghostty 1.4.0 macOS `TerminalTabColor.swift`처럼 탭 오른쪽 단추 메뉴에서 열 가지(없음 포함)를 고르고,
//! 탭 줄에는 제목 오른쪽의 작은 점으로 보인다. 키 동작·설정 키는 Ghostty에도 없다(`src/input/Binding.zig`에 `tab_color`가 없음)

use bt_core::Rgb;

/// Ghostty `TerminalTabColor`의 순서 그대로(메뉴의 두 줄: 없음·파랑·보라·분홍·빨강 / 주황·노랑·초록·청록·그래파이트)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabColor {
    #[default]
    None,
    Blue,
    Purple,
    Pink,
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Graphite,
}

impl TabColor {
    pub const ALL: [TabColor; 10] = [
        TabColor::None,
        TabColor::Blue,
        TabColor::Purple,
        TabColor::Pink,
        TabColor::Red,
        TabColor::Orange,
        TabColor::Yellow,
        TabColor::Green,
        TabColor::Teal,
        TabColor::Graphite,
    ];

    /// 상태 파일에 적는 이름
    pub fn key(self) -> &'static str {
        match self {
            TabColor::None => "none",
            TabColor::Blue => "blue",
            TabColor::Purple => "purple",
            TabColor::Pink => "pink",
            TabColor::Red => "red",
            TabColor::Orange => "orange",
            TabColor::Yellow => "yellow",
            TabColor::Green => "green",
            TabColor::Teal => "teal",
            TabColor::Graphite => "graphite",
        }
    }

    /// 상태 파일에서 읽는다. 모르는 이름은 없다(파일을 믿지 않는다: 읽지 못해도 파일 전체를 버리지 않는다)
    pub fn from_key(key: &str) -> Option<TabColor> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }

    /// 상태 파일의 값 → 색. 없거나 모르는 이름이면 색 없음
    pub fn from_saved(key: Option<&str>) -> TabColor {
        key.and_then(Self::from_key).unwrap_or_default()
    }

    /// 메뉴에 보이는 이름
    pub fn label(self) -> &'static str {
        match self {
            TabColor::None => "없음",
            TabColor::Blue => "파랑",
            TabColor::Purple => "보라",
            TabColor::Pink => "분홍",
            TabColor::Red => "빨강",
            TabColor::Orange => "주황",
            TabColor::Yellow => "노랑",
            TabColor::Green => "초록",
            TabColor::Teal => "청록",
            TabColor::Graphite => "그래파이트",
        }
    }

    /// 메뉴 항목 id
    pub fn menu_id(self) -> String {
        format!("tab_color_{}", self.key())
    }

    pub fn from_menu_id(id: &str) -> Option<TabColor> {
        Self::from_key(id.strip_prefix("tab_color_")?)
    }

    /// 보이는 색. Ghostty는 AppKit 시스템 색(`systemBlue`…, 청록은 `systemMint`, 그래파이트는 `systemGray`)을 쓴다.
    /// 밝은 화면과 어두운 화면의 값이 다르므로 바탕이 어두운지로 고른다. 값은 Apple이 공개한 macOS 시스템 색(sRGB)이고 이 기계에서 잰 것이 아니다
    pub fn rgb(self, dark: bool) -> Option<Rgb> {
        let (l, d) = match self {
            TabColor::None => return None,
            TabColor::Blue => ((0, 122, 255), (10, 132, 255)),
            TabColor::Purple => ((175, 82, 222), (191, 90, 242)),
            TabColor::Pink => ((255, 45, 85), (255, 55, 95)),
            TabColor::Red => ((255, 59, 48), (255, 69, 58)),
            TabColor::Orange => ((255, 149, 0), (255, 159, 10)),
            TabColor::Yellow => ((255, 204, 0), (255, 214, 10)),
            TabColor::Green => ((52, 199, 89), (48, 209, 88)),
            TabColor::Teal => ((0, 199, 190), (99, 230, 226)),
            TabColor::Graphite => ((142, 142, 147), (152, 152, 157)),
        };
        let (r, g, b) = if dark { d } else { l };
        Some(Rgb { r, g, b })
    }
}

/// 바탕이 어두운가(상대 휘도 0.18 아래, 대략 중간 회색보다 어둡다)
pub fn is_dark(background: Rgb) -> bool {
    background.luminance() < 0.18
}

/// 메뉴 견본 그림의 한 변(픽셀). 메뉴에는 18pt로 걸려 레티나에서 또렷하다
pub const SWATCH_PX: u32 = 36;

/// 메뉴 견본(Ghostty `swatchImage`): 색 동그라미. 없음은 속이 빈 회색 고리에 빗금이고, 고른 것은 바깥에 강조 고리가 붙는다.
/// RGBA 8비트(곱하지 않은 알파)를 `SWATCH_PX`의 제곱으로 돌려준다
pub fn swatch_rgba(color: TabColor, selected: bool, dark: bool) -> Vec<u8> {
    let n = SWATCH_PX as usize;
    let mut out = vec![0u8; n * n * 4];
    let c = n as f32 / 2.0;
    // 동그라미는 한 변에서 2px(18pt 기준 1pt)씩 들여 그리고, 강조 고리는 그 바깥 가장자리에 4px 굵기로 그린다
    let r = c - 2.0;
    let accent = (0.0, 122.0, 255.0);
    let gray = if dark {
        (160.0, 160.0, 165.0)
    } else {
        (110.0, 110.0, 115.0)
    };
    for y in 0..n {
        for x in 0..n {
            let (px, py) = (x as f32 + 0.5 - c, y as f32 + 0.5 - c);
            let d = (px * px + py * py).sqrt();
            // 가장자리 안티앨리어싱: 거리 d에서 반지름 radius 안쪽이면 1
            let inside = |radius: f32| (radius + 0.5 - d).clamp(0.0, 1.0);
            let mut px_rgba = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            let mut over = |rgb: (f32, f32, f32), a: f32| {
                let a = a.clamp(0.0, 1.0);
                if a <= 0.0 {
                    return;
                }
                let oa = a + px_rgba.3 * (1.0 - a);
                let mix = |s: f32, d: f32| (s * a + d * px_rgba.3 * (1.0 - a)) / oa;
                px_rgba = (mix(rgb.0, px_rgba.0), mix(rgb.1, px_rgba.1), mix(rgb.2, px_rgba.2), oa);
            };
            match color.rgb(dark) {
                Some(rgb) => over((rgb.r as f32, rgb.g as f32, rgb.b as f32), inside(r)),
                None => {
                    // 속이 빈 고리(굵기 2px)
                    over(gray, (inside(r) - inside(r - 2.0)).max(0.0));
                    // 왼쪽 아래에서 오른쪽 위로 가는 빗금(굵기 3px)
                    let line = (px + py).abs() / std::f32::consts::SQRT_2;
                    if d < r - 3.0 {
                        over(gray, (1.5 + 0.5 - line).clamp(0.0, 1.0));
                    }
                }
            }
            if selected {
                over(accent, (inside(c - 0.5) - inside(c - 4.5)).max(0.0));
            }
            let i = (y * n + x) * 4;
            out[i] = px_rgba.0.round() as u8;
            out[i + 1] = px_rgba.1.round() as u8;
            out[i + 2] = px_rgba.2.round() as u8;
            out[i + 3] = (px_rgba.3 * 255.0).round() as u8;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_colors_in_ghostty_order_with_unique_keys() {
        let keys: Vec<_> = TabColor::ALL.iter().map(|c| c.key()).collect();
        assert_eq!(
            keys,
            [
                "none", "blue", "purple", "pink", "red", "orange", "yellow", "green", "teal", "graphite"
            ]
        );
        for c in TabColor::ALL {
            assert_eq!(TabColor::from_key(c.key()), Some(c));
            assert_eq!(TabColor::from_menu_id(&c.menu_id()), Some(c));
            assert_eq!(c.rgb(false).is_none(), c == TabColor::None);
        }
        assert_eq!(TabColor::from_key("magenta"), None);
        assert_eq!(TabColor::from_menu_id("tab_title"), None);
        assert_eq!(TabColor::from_menu_id("tab_color_"), None);
    }

    #[test]
    fn colors_are_distinct_and_follow_the_appearance() {
        for dark in [false, true] {
            let all: Vec<_> = TabColor::ALL.iter().filter_map(|c| c.rgb(dark)).collect();
            assert_eq!(all.len(), 9);
            for (i, a) in all.iter().enumerate() {
                assert!(all[i + 1..].iter().all(|b| a != b), "{a:?}");
            }
        }
        assert_eq!(TabColor::Blue.rgb(false), Some(Rgb { r: 0, g: 122, b: 255 }));
        assert_eq!(TabColor::Blue.rgb(true), Some(Rgb { r: 10, g: 132, b: 255 }));
    }

    #[test]
    fn swatch_is_a_filled_circle_with_a_ring_when_selected() {
        let n = SWATCH_PX as usize;
        let at = |img: &[u8], x: usize, y: usize| {
            let i = (y * n + x) * 4;
            (img[i], img[i + 1], img[i + 2], img[i + 3])
        };
        let blue = swatch_rgba(TabColor::Blue, false, false);
        assert_eq!(blue.len(), n * n * 4);
        assert_eq!(at(&blue, n / 2, n / 2), (0, 122, 255, 255), "가운데는 색");
        assert_eq!(at(&blue, 0, 0).3, 0, "모서리는 투명");
        assert_eq!(at(&blue, n / 2, 0).3, 0, "고르지 않으면 바깥 고리가 없다");
        let sel = swatch_rgba(TabColor::Red, true, false);
        assert_eq!(at(&sel, n / 2, n / 2), (255, 59, 48, 255));
        assert!(at(&sel, n / 2, 1).3 > 200, "고르면 맨 바깥에 고리가 있다");
        // 없음: 가운데는 빗금(잉크), 빗금에서 먼 안쪽은 비어 있다
        let none = swatch_rgba(TabColor::None, false, false);
        assert!(at(&none, n / 2, n / 2).3 > 200);
        assert!(
            at(&none, n / 2 + 8, n / 2 - 8).3 > 200,
            "빗금은 왼쪽 아래에서 오른쪽 위로"
        );
        assert_eq!(at(&none, n / 2 + 8, n / 2 + 8).3, 0);
    }

    #[test]
    fn labels_are_korean() {
        for c in TabColor::ALL {
            assert!(
                c.label().chars().all(|ch| ('\u{AC00}'..='\u{D7A3}').contains(&ch)),
                "{}",
                c.label()
            );
        }
    }
}
