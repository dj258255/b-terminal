//! 글자를 바탕에 섞는 덮음 곡선(kitty `text_composition_strategy`, #911). CPU 길(`bt-render`)과 GPU 셰이더가 같은 식을 쓴다.
//!
//! kitty v0.51.1 `kitty/shaders/cell.slang`의 `foreground_contrast_new`(231~241줄)를 그대로 옮긴 것이다:
//! `a' = clamp(lerp(a, pow(a, 1/감마), (1 - 글자밝기 + 바탕밝기) * 0.5) * (1 + 명암/100), 0, 1)`.
//! 밝기는 sRGB 값(0~1)에 `(0.2126, 0.7152, 0.0722)`를 곱해 더한 값이다(선형으로 바꾸지 않는다)

use crate::Rgb;

/// 밝기 차에 따른 감마 가중의 배율(kitty `cell.slang` 222줄 `text_gamma_scaling`)
pub const TEXT_GAMMA_SCALING: f32 = 0.5;

/// 덮음 곡선의 두 수. 기본(`NONE`)은 곡선을 걸지 않아 글자 픽셀이 지금과 같다
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextComposition {
    /// 감마 조정(0.01 이상). 클수록 어두운 글자가 굵어진다. 1이면 조정 없음
    pub gamma: f32,
    /// 추가 곱셈 명암 퍼센트(0~100). 0이면 조정 없음
    pub contrast: f32,
}

impl Default for TextComposition {
    fn default() -> Self {
        Self::NONE
    }
}

impl TextComposition {
    /// 곡선을 걸지 않는다(기본)
    pub const NONE: Self = Self {
        gamma: 1.0,
        contrast: 0.0,
    };
    /// kitty `platform`의 macOS 값(`1.7 30`). kitty는 Linux에서는 `1.0 0`이지만 여기서는 어느 OS에서나 이 값이다
    pub const PLATFORM: Self = Self {
        gamma: 1.7,
        contrast: 30.0,
    };

    /// 설정 값을 읽는다: `none`·`legacy`(곡선 없음), `platform`, `감마`, `감마 명암`. 잘못되면 `None`.
    /// 감마는 0.01 아래면 0.01로, 명암은 0~100으로 자른다(kitty `to-c.h` 641~676줄과 같다)
    pub fn parse(v: &str) -> Option<Self> {
        match v.trim() {
            "none" | "legacy" => return Some(Self::NONE),
            "platform" => return Some(Self::PLATFORM),
            _ => {}
        }
        let mut parts = v.split_whitespace();
        let num = |s: &str| s.parse::<f32>().ok().filter(|n| n.is_finite());
        let gamma = num(parts.next()?)?.max(0.01);
        let contrast = match parts.next() {
            Some(s) => num(s)?.clamp(0.0, 100.0),
            None => 0.0,
        };
        parts.next().is_none().then_some(Self { gamma, contrast })
    }

    /// 곡선이 아무것도 바꾸지 않는가(그러면 덮음 값을 건드리지 않는다)
    pub fn is_identity(self) -> bool {
        self.gamma == 1.0 && self.contrast == 0.0
    }

    /// 덮음 `a`(0~1)를 글자색 `fg`와 바탕색 `bg`(0~1 sRGB)에 맞게 바꾼다
    pub fn coverage(self, a: f32, fg: [f32; 3], bg: [f32; 3]) -> f32 {
        if self.is_identity() {
            return a;
        }
        // kitty `shaders.c` 1494~1495줄: 셰이더에는 1/감마와 1 + 명암/100을 넘긴다
        let (inv_gamma, contrast) = (1.0 / self.gamma, 1.0 + self.contrast * 0.01);
        let lum = |c: [f32; 3]| c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722;
        let weight = (1.0 - lum(fg) + lum(bg)) * TEXT_GAMMA_SCALING;
        let mixed = a + (a.powf(inv_gamma) - a) * weight;
        (mixed * contrast).clamp(0.0, 1.0)
    }

    /// 8비트 덮음용. 곡선이 없으면 `a` 그대로다
    pub fn coverage_u8(self, a: u8, fg: Rgb, bg: Rgb) -> u8 {
        if self.is_identity() {
            return a;
        }
        let f = |c: Rgb| [c.r as f32 / 255.0, c.g as f32 / 255.0, c.b as f32 / 255.0];
        (self.coverage(a as f32 / 255.0, f(fg), f(bg)) * 255.0).round() as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHITE: [f32; 3] = [1.0; 3];
    const BLACK: [f32; 3] = [0.0; 3];

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    /// kitty 식을 파이썬으로 따로 계산한 값(`1.7 30`): 흰 바탕의 검은 글자는 크게 올라가고 어두운 바탕의 흰 글자는 명암만 곱해진다
    #[test]
    fn platform_curve_matches_kitty_formula_at_points() {
        let c = TextComposition::PLATFORM;
        // (덮음, 흰 바탕 검은 글자, 검은 바탕 흰 글자)
        for (a, dark_on_light, light_on_dark) in [
            (0.1, 0.335_512, 0.13),
            (0.25, 0.575_162_3, 0.325),
            (0.5, 0.864_702_8, 0.65),
            (0.75, 1.0, 0.975),
            (0.9, 1.0, 1.0),
        ] {
            let got = c.coverage(a, BLACK, WHITE);
            assert!(close(got, dark_on_light), "검정/흰 a={a}: {got} != {dark_on_light}");
            let got = c.coverage(a, WHITE, BLACK);
            assert!(close(got, light_on_dark), "흰/검정 a={a}: {got} != {light_on_dark}");
        }
        // 밝은 글자 어두운 바탕(색 있는 짝): 220/220/210 on 30/32/40
        let fg = [220.0 / 255.0, 220.0 / 255.0, 210.0 / 255.0];
        let bg = [30.0 / 255.0, 32.0 / 255.0, 40.0 / 255.0];
        assert!(close(c.coverage(0.5, fg, bg), 0.678_574_2));
    }

    #[test]
    fn gamma_only_and_endpoints() {
        let c = TextComposition {
            gamma: 2.2,
            contrast: 0.0,
        };
        assert!(close(c.coverage(0.5, BLACK, WHITE), 0.729_740));
        assert!(close(c.coverage(0.1, BLACK, WHITE), 0.351_119));
        // 0과 1은 어느 곡선에서도 제자리다(명암 0일 때)
        assert_eq!(c.coverage(0.0, BLACK, WHITE), 0.0);
        assert!(close(c.coverage(1.0, BLACK, WHITE), 1.0));
        // 글자와 바탕 밝기가 같으면(차 0) 가중이 0.5라 감마의 절반만 걸린다
        let mid = [0.5; 3];
        let want = 0.5 + (0.5f32.powf(1.0 / 2.2) - 0.5) * 0.5;
        assert!(close(c.coverage(0.5, mid, mid), want));
    }

    #[test]
    fn identity_leaves_every_byte_alone() {
        let n = TextComposition::NONE;
        assert!(n.is_identity() && TextComposition::default().is_identity());
        assert!(!TextComposition::PLATFORM.is_identity());
        let (fg, bg) = (Rgb::new(10, 200, 30), Rgb::new(250, 240, 5));
        for a in 0..=255u8 {
            assert_eq!(n.coverage_u8(a, fg, bg), a);
        }
        // 1.0 0은 식으로도 제자리(kitty가 셰이더를 건너뛰는 값)
        assert!(close(
            TextComposition {
                gamma: 1.0,
                contrast: 0.0
            }
            .coverage(0.37, BLACK, WHITE),
            0.37
        ));
    }

    #[test]
    fn eight_bit_matches_float_and_stays_in_range() {
        let c = TextComposition::PLATFORM;
        let (fg, bg) = (Rgb::new(0, 0, 0), Rgb::new(255, 255, 255));
        let mut prev = 0;
        for a in 0..=255u8 {
            let got = c.coverage_u8(a, fg, bg);
            assert!(got >= prev, "덮음이 커지는데 결과가 줄었다: a={a}");
            prev = got;
        }
        assert_eq!(c.coverage_u8(0, fg, bg), 0);
        assert_eq!(c.coverage_u8(255, fg, bg), 255);
        // a=128 → 0.50196... 의 곡선 값을 반올림한 것
        let want = (c.coverage(128.0 / 255.0, BLACK, WHITE) * 255.0).round() as u8;
        assert_eq!(c.coverage_u8(128, fg, bg), want);
        assert!(want > 128 + 80);
    }

    #[test]
    fn parse_accepts_kitty_forms() {
        let p = TextComposition::parse;
        assert_eq!(p("none"), Some(TextComposition::NONE));
        assert_eq!(p("legacy"), Some(TextComposition::NONE));
        assert_eq!(p("platform"), Some(TextComposition::PLATFORM));
        assert_eq!(p("1.7 30"), Some(TextComposition::PLATFORM));
        assert_eq!(
            p("2"),
            Some(TextComposition {
                gamma: 2.0,
                contrast: 0.0
            })
        );
        assert_eq!(
            p("  1.2   50  "),
            Some(TextComposition {
                gamma: 1.2,
                contrast: 50.0
            })
        );
        // 범위 밖은 자른다(kitty와 같다): 감마 0.01 아래 → 0.01, 명암 0~100
        assert_eq!(
            p("0 500"),
            Some(TextComposition {
                gamma: 0.01,
                contrast: 100.0
            })
        );
        assert_eq!(p("1.5 -4").map(|c| c.contrast), Some(0.0));
        for bad in ["", "abc", "1 2 3", "1 x", "nan", "inf 3", "1 nan"] {
            assert_eq!(p(bad), None, "{bad:?}");
        }
    }
}
