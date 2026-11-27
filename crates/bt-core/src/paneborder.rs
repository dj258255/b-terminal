//! 칸 테두리 설정(kitty `window_border_width`·`draw_minimal_borders`·`window_border_radius`·`draw_window_borders_for_single_window`·
//! `active_border_color`·`inactive_border_color`·`bell_border_color`). 키 이름은 하이픈이다(`window-border-width`).
//!
//! 설정이 하나도 없으면 지금 칸 경계선(`split-divider-color`) 그대로 그린다(#915). 그래서 두께와 색은 `Option`이고,
//! kitty의 기본값(0.5pt, 초록 `#00ff00`, 회색 `#cccccc`, 주황 `#ff5a00`)은 여기서 쓰지 않는다

use crate::theme::Rgb;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Len {
    pub value: f32,
    pub px: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveColor {
    #[default]
    Unset,
    Off,
    Color(Rgb),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PaneBorderConfig {
    pub width: Option<Len>,
    pub minimal: bool,
    pub radius: Len,
    pub single: bool,
    pub active: ActiveColor,
    pub inactive: Option<Rgb>,
    pub bell: Option<Rgb>,
}

impl Len {
    /// kitty `window_border_width`: `px`·`pt`, 단위가 없으면 pt, 음수는 0
    pub fn parse(s: &str) -> Option<Len> {
        let s = s.trim();
        let (num, px) = match s.strip_suffix("px") {
            Some(n) => (n, true),
            None => (s.strip_suffix("pt").unwrap_or(s), false),
        };
        let value: f32 = num.trim().parse().ok()?;
        value.is_finite().then_some(Len {
            value: value.max(0.0),
            px,
        })
    }

    /// `--show-config`에 적는 글(다시 읽으면 같은 값)
    pub fn render(&self) -> String {
        format!("{}{}", self.value, if self.px { "px" } else { "pt" })
    }

    /// 화면 픽셀. pt는 `scale`을 곱해 반올림한다. `at_least_one`이면 0보다 큰 pt값은 한 픽셀을 남긴다
    /// (kitty `Window.effective_border`: `max(1 if val > 0 else 0, pt_to_px(val))`). px는 반올림만 한다
    pub fn to_px(&self, scale: f64, at_least_one: bool) -> u32 {
        if self.px {
            return self.value.round() as u32;
        }
        let v = (self.value as f64 * scale).round() as u32;
        if at_least_one && self.value > 0.0 { v.max(1) } else { v }
    }
}

impl Default for PaneBorderConfig {
    fn default() -> Self {
        Self {
            width: None,
            minimal: true,
            radius: Len { value: 0.0, px: false },
            single: false,
            active: ActiveColor::Unset,
            inactive: None,
            bell: None,
        }
    }
}

fn hex(c: Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

impl PaneBorderConfig {
    /// 키 하나를 적용한다. 이 모듈의 키가 아니면 `None`, 값이 틀리면 `Some(false)`(이전 값을 둔다), 받으면 `Some(true)`.
    /// 값이 비면 정하지 않은 상태(두께·색) 또는 기본값으로 되돌린다
    pub fn apply(&mut self, key: &str, v: &str) -> Option<bool> {
        let v = v.trim();
        let ok = match key {
            "window-border-width" => {
                if v.is_empty() {
                    self.width = None;
                    true
                } else {
                    Len::parse(v).map(|l| self.width = Some(l)).is_some()
                }
            }
            "window-border-radius" => {
                if v.is_empty() {
                    self.radius = Self::default().radius;
                    true
                } else {
                    Len::parse(v).map(|l| self.radius = l).is_some()
                }
            }
            "draw-minimal-borders" => bool_of(v, Self::default().minimal).map(|b| self.minimal = b).is_some(),
            "draw-window-borders-for-single-window" => {
                bool_of(v, Self::default().single).map(|b| self.single = b).is_some()
            }
            "active-border-color" => {
                if v.is_empty() {
                    self.active = ActiveColor::Unset;
                    true
                } else if v == "none" {
                    self.active = ActiveColor::Off;
                    true
                } else {
                    Rgb::parse_color(v)
                        .map(|c| self.active = ActiveColor::Color(c))
                        .is_some()
                }
            }
            "inactive-border-color" => color_or_unset(v, &mut self.inactive),
            "bell-border-color" => color_or_unset(v, &mut self.bell),
            _ => return None,
        };
        Some(ok)
    }

    /// `--show-config`에 적을 줄. 정하지 않은 두께·색은 적지 않는다(적으면 kitty 기본값을 정한 것처럼 보인다)
    pub fn lines(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        if let Some(w) = &self.width {
            out.push(("window-border-width", w.render()));
        }
        out.push(("draw-minimal-borders", self.minimal.to_string()));
        out.push(("window-border-radius", self.radius.render()));
        out.push(("draw-window-borders-for-single-window", self.single.to_string()));
        match self.active {
            ActiveColor::Unset => {}
            ActiveColor::Off => out.push(("active-border-color", "none".into())),
            ActiveColor::Color(c) => out.push(("active-border-color", hex(c))),
        }
        if let Some(c) = self.inactive {
            out.push(("inactive-border-color", hex(c)));
        }
        if let Some(c) = self.bell {
            out.push(("bell-border-color", hex(c)));
        }
        out
    }

    /// 아무 키도 정하지 않았다: 칸 경계선을 지금처럼 그린다
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// `true`·`false`(kitty `yes`·`no`도). 비면 `default`
fn bool_of(v: &str, default: bool) -> Option<bool> {
    match v {
        "" => Some(default),
        "true" | "yes" | "y" | "on" => Some(true),
        "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

fn color_or_unset(v: &str, slot: &mut Option<Rgb>) -> bool {
    if v.is_empty() {
        *slot = None;
        return true;
    }
    Rgb::parse_color(v).map(|c| *slot = Some(c)).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(r: u8, g: u8, b: u8) -> Rgb {
        Rgb::new(r, g, b)
    }

    #[test]
    fn nothing_set_is_default() {
        let d = PaneBorderConfig::default();
        assert!(d.is_default());
        assert_eq!(d.width, None);
        assert!(d.minimal);
        assert!(!d.single);
        assert_eq!(d.active, ActiveColor::Unset);
        assert_eq!((d.inactive, d.bell), (None, None));
    }

    #[test]
    fn lengths_follow_kitty_units() {
        assert_eq!(Len::parse("0.5pt"), Some(Len { value: 0.5, px: false }));
        assert_eq!(Len::parse("2px"), Some(Len { value: 2.0, px: true }));
        // 단위가 없으면 pt
        assert_eq!(Len::parse("3"), Some(Len { value: 3.0, px: false }));
        // 음수는 0으로(kitty `max(0, val)`)
        assert_eq!(Len::parse("-1px"), Some(Len { value: 0.0, px: true }));
        assert_eq!(Len::parse("wide"), None);
        assert_eq!(Len::parse("1em"), None);
        assert_eq!(Len::parse("nanpx"), None);
    }

    #[test]
    fn lengths_become_pixels_like_kitty_effective_border() {
        let pt = |v: f32| Len { value: v, px: false };
        let px = |v: f32| Len { value: v, px: true };
        // pt는 화면 배율로 곱해 반올림하고, 0보다 크면 한 픽셀은 남긴다(kitty `max(1 if val > 0 else 0, pt_to_px)`)
        assert_eq!(pt(0.5).to_px(2.0, true), 1);
        assert_eq!(pt(0.1).to_px(1.0, true), 1);
        assert_eq!(pt(0.0).to_px(2.0, true), 0);
        assert_eq!(pt(2.0).to_px(2.0, true), 4);
        // px는 배율과 상관없이 반올림만
        assert_eq!(px(2.4).to_px(2.0, true), 2);
        assert_eq!(px(0.4).to_px(2.0, true), 0);
        // 둥근 모서리 반지름은 한 픽셀 남기기를 하지 않는다
        assert_eq!(pt(0.1).to_px(1.0, false), 0);
        assert_eq!(pt(3.0).to_px(2.0, false), 6);
    }

    #[test]
    fn keys_are_read_and_bad_values_keep_the_old_one() {
        let mut b = PaneBorderConfig::default();
        assert_eq!(b.apply("window-border-width", "2px"), Some(true));
        assert_eq!(b.width, Some(Len { value: 2.0, px: true }));
        assert_eq!(b.apply("window-border-width", "thick"), Some(false));
        assert_eq!(b.width, Some(Len { value: 2.0, px: true }));
        assert_eq!(b.apply("window-border-width", ""), Some(true));
        assert_eq!(b.width, None);
        assert_eq!(b.apply("draw-minimal-borders", "false"), Some(true));
        assert!(!b.minimal);
        assert_eq!(b.apply("draw-minimal-borders", "maybe"), Some(false));
        assert_eq!(b.apply("window-border-radius", "6px"), Some(true));
        assert_eq!(b.radius, Len { value: 6.0, px: true });
        assert_eq!(b.apply("draw-window-borders-for-single-window", "true"), Some(true));
        assert!(b.single);
        assert_eq!(b.apply("active-border-color", "#00ff00"), Some(true));
        assert_eq!(b.active, ActiveColor::Color(c(0, 255, 0)));
        assert_eq!(b.apply("active-border-color", "none"), Some(true));
        assert_eq!(b.active, ActiveColor::Off);
        assert_eq!(b.apply("inactive-border-color", "red"), Some(true));
        assert_eq!(b.inactive, Some(c(255, 0, 0)));
        assert_eq!(b.apply("bell-border-color", "nope"), Some(false));
        assert_eq!(b.bell, None);
        assert_eq!(b.apply("bell-border-color", "#ff5a00"), Some(true));
        assert_eq!(b.bell, Some(c(255, 90, 0)));
        // 이 모듈의 키가 아니면 None
        assert_eq!(b.apply("split-divider-color", "red"), None);
        // inactive·bell에는 none이 없다
        assert_eq!(b.apply("inactive-border-color", "none"), Some(false));
    }

    #[test]
    fn show_config_lines_read_back_to_the_same_value() {
        let mut b = PaneBorderConfig::default();
        for (k, v) in [
            ("window-border-width", "1.5pt"),
            ("draw-minimal-borders", "false"),
            ("window-border-radius", "6px"),
            ("draw-window-borders-for-single-window", "true"),
            ("active-border-color", "#00ff00"),
            ("inactive-border-color", "#cccccc"),
            ("bell-border-color", "#ff5a00"),
        ] {
            assert_eq!(b.apply(k, v), Some(true), "{k}");
        }
        let mut again = PaneBorderConfig::default();
        for (k, v) in b.lines() {
            assert_eq!(again.apply(k, &v), Some(true), "{k} = {v}");
        }
        assert_eq!(again, b);
        // 아무것도 안 정했으면 정하지 않은 값(두께·색)은 적지 않는다
        let keys: Vec<_> = PaneBorderConfig::default().lines().into_iter().map(|l| l.0).collect();
        assert!(!keys.contains(&"window-border-width"));
        assert!(!keys.contains(&"active-border-color"));
        assert!(keys.contains(&"draw-minimal-borders"));
    }
}
