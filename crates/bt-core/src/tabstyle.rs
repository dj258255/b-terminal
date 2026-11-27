//! 탭 막대 모양과 색(kitty `tab_bar_style`·`tab_powerline_style`·`tab_separator`·`tab_fade`·`tab_bar_align`·
//! `active_tab_*`·`inactive_tab_*`·`tab_bar_background`). 키는 kitty와 같은 뜻이고 `-`로 잇는다.
//! 적지 않은 색은 `None`이라 지금처럼 테마에서 고른다. 기본 모양(`default`)은 설정을 바꾸지 않은 사람의 지금 모양이다

use crate::theme::Rgb;

/// `tab-bar-style`. `default`는 b-terminal의 지금 모양이고 나머지는 kitty의 이름이다(`custom`은 받지 않는다)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabBarStyle {
    #[default]
    Default,
    /// 탭 가장자리가 막대 바탕으로 번진다
    Fade,
    /// 파일철 탭처럼 양옆이 기울어진다
    Slant,
    /// 탭 사이에 구분 글자
    Separator,
    /// 파워라인 기호로 이어진 한 줄
    Powerline,
    /// 탭 막대를 그리지 않는다(`window-show-tab-bar = never`와 같은 효과)
    Hidden,
}

/// `tab-powerline-style`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabPowerlineStyle {
    #[default]
    Angled,
    Slanted,
    Round,
}

/// `tab-bar-align`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabBarAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// `active-tab-font-style`·`inactive-tab-font-style`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TabFontStyle {
    #[default]
    Normal,
    Bold,
    Italic,
    BoldItalic,
}

impl TabFontStyle {
    pub fn bold(self) -> bool {
        matches!(self, Self::Bold | Self::BoldItalic)
    }
    pub fn italic(self) -> bool {
        matches!(self, Self::Italic | Self::BoldItalic)
    }
}

/// kitty `tab_fade`의 기본값
pub const DEFAULT_FADE: [f32; 4] = [0.25, 0.5, 0.75, 1.0];
/// kitty `tab_separator`의 기본값(`default_tab_separator`)
pub const DEFAULT_SEPARATOR: &str = " ┇";

#[derive(Debug, Clone, PartialEq)]
pub struct TabBarLook {
    pub style: TabBarStyle,
    pub powerline: TabPowerlineStyle,
    pub separator: String,
    pub fade: Vec<f32>,
    pub align: TabBarAlign,
    pub active_foreground: Option<Rgb>,
    pub active_background: Option<Rgb>,
    pub inactive_foreground: Option<Rgb>,
    pub inactive_background: Option<Rgb>,
    pub bar_background: Option<Rgb>,
    pub active_font: TabFontStyle,
    pub inactive_font: TabFontStyle,
}

impl Default for TabBarLook {
    fn default() -> Self {
        Self {
            style: TabBarStyle::Default,
            powerline: TabPowerlineStyle::Angled,
            separator: DEFAULT_SEPARATOR.to_string(),
            fade: DEFAULT_FADE.to_vec(),
            align: TabBarAlign::Left,
            active_foreground: None,
            active_background: None,
            inactive_foreground: None,
            inactive_background: None,
            bar_background: None,
            active_font: TabFontStyle::Normal,
            inactive_font: TabFontStyle::Normal,
        }
    }
}

/// 색 값: 비었거나 `none`이면 `Some(None)`(정하지 않음), 읽으면 `Some(Some(색))`, 틀리면 `None`
fn color(v: &str) -> Option<Option<Rgb>> {
    let v = v.trim();
    if v.is_empty() || v.eq_ignore_ascii_case("none") {
        return Some(None);
    }
    Rgb::parse_color(v).map(Some)
}

fn font_style(v: &str) -> Option<TabFontStyle> {
    match v.trim().to_ascii_lowercase().replace('_', "-").as_str() {
        "normal" => Some(TabFontStyle::Normal),
        "bold" => Some(TabFontStyle::Bold),
        "italic" => Some(TabFontStyle::Italic),
        "bold-italic" => Some(TabFontStyle::BoldItalic),
        _ => None,
    }
}

/// kitty `tab_separator`: 따옴표로 감싸면 벗기고, 비면 구분 글자 없음, 공백뿐이면 그 폭만큼의 빈칸
fn separator(v: &str) -> String {
    let mut s = v;
    for q in ['\'', '"'] {
        if s.len() >= 2 && s.starts_with(q) && s.ends_with(q) {
            s = &s[1..s.len() - 1];
            break;
        }
    }
    s.to_string()
}

/// 구분 글자에서 앞뒤 공백을 뺀 것. 공백뿐이면 빈 글자(구분선 없이 그만큼 띄운다)
pub fn separator_glyph(sep: &str) -> &str {
    sep.trim_matches(' ')
}

impl TabBarLook {
    /// 탭이 `count`개일 때 탭 막대를 그리는가. `hidden`은 `window-show-tab-bar = never`와 같은 효과다
    /// (kitty의 `hidden`도 탭 막대만 없애고 탭은 그대로 있다)
    pub fn bar_visible(&self, show: crate::config::WindowShowTabBar, count: usize) -> bool {
        self.style != TabBarStyle::Hidden && show.visible(count)
    }

    /// 키 하나를 적용한다. 이 모듈의 키가 아니면 `None`, 값이 틀리면 `Some(false)`(이전 값을 둔다), 받으면 `Some(true)`
    pub fn apply(&mut self, key: &str, v: &str) -> Option<bool> {
        let ok = match key {
            "tab-bar-style" => match v.trim() {
                "default" => set(&mut self.style, TabBarStyle::Default),
                "fade" => set(&mut self.style, TabBarStyle::Fade),
                "slant" => set(&mut self.style, TabBarStyle::Slant),
                "separator" => set(&mut self.style, TabBarStyle::Separator),
                "powerline" => set(&mut self.style, TabBarStyle::Powerline),
                "hidden" => set(&mut self.style, TabBarStyle::Hidden),
                _ => false,
            },
            "tab-powerline-style" => match v.trim() {
                "angled" => set(&mut self.powerline, TabPowerlineStyle::Angled),
                "slanted" => set(&mut self.powerline, TabPowerlineStyle::Slanted),
                "round" => set(&mut self.powerline, TabPowerlineStyle::Round),
                _ => false,
            },
            "tab-bar-align" => match v.trim() {
                "left" | "start" => set(&mut self.align, TabBarAlign::Left),
                "center" => set(&mut self.align, TabBarAlign::Center),
                "right" | "end" => set(&mut self.align, TabBarAlign::Right),
                _ => false,
            },
            "tab-separator" => {
                self.separator = separator(v);
                true
            }
            // kitty `tab_fade`: 공백으로 가른 0~1 숫자. 비면 가장자리 번짐이 없다
            "tab-fade" => {
                let parsed: Option<Vec<f32>> = v
                    .split_whitespace()
                    .map(|t| t.parse::<f32>().ok().filter(|f| (0.0..=1.0).contains(f)))
                    .collect();
                match parsed {
                    Some(list) => {
                        self.fade = list;
                        true
                    }
                    None => false,
                }
            }
            "active-tab-foreground" => opt_color(&mut self.active_foreground, v),
            "active-tab-background" => opt_color(&mut self.active_background, v),
            "inactive-tab-foreground" => opt_color(&mut self.inactive_foreground, v),
            "inactive-tab-background" => opt_color(&mut self.inactive_background, v),
            "tab-bar-background" => opt_color(&mut self.bar_background, v),
            "active-tab-font-style" => font_style(v).is_some_and(|s| set(&mut self.active_font, s)),
            "inactive-tab-font-style" => font_style(v).is_some_and(|s| set(&mut self.inactive_font, s)),
            _ => return None,
        };
        Some(ok)
    }

    /// `+show-config`에 찍을 (키, 값)
    pub fn lines(&self) -> Vec<(&'static str, String)> {
        let hex =
            |c: Option<Rgb>| c.map_or_else(|| "none".to_string(), |c| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b));
        let font = |f: TabFontStyle| match f {
            TabFontStyle::Normal => "normal",
            TabFontStyle::Bold => "bold",
            TabFontStyle::Italic => "italic",
            TabFontStyle::BoldItalic => "bold-italic",
        };
        vec![
            (
                "tab-bar-style",
                match self.style {
                    TabBarStyle::Default => "default",
                    TabBarStyle::Fade => "fade",
                    TabBarStyle::Slant => "slant",
                    TabBarStyle::Separator => "separator",
                    TabBarStyle::Powerline => "powerline",
                    TabBarStyle::Hidden => "hidden",
                }
                .into(),
            ),
            (
                "tab-powerline-style",
                match self.powerline {
                    TabPowerlineStyle::Angled => "angled",
                    TabPowerlineStyle::Slanted => "slanted",
                    TabPowerlineStyle::Round => "round",
                }
                .into(),
            ),
            ("tab-separator", format!("\"{}\"", self.separator)),
            (
                "tab-fade",
                self.fade.iter().map(|f| f.to_string()).collect::<Vec<_>>().join(" "),
            ),
            (
                "tab-bar-align",
                match self.align {
                    TabBarAlign::Left => "left",
                    TabBarAlign::Center => "center",
                    TabBarAlign::Right => "right",
                }
                .into(),
            ),
            ("active-tab-foreground", hex(self.active_foreground)),
            ("active-tab-background", hex(self.active_background)),
            ("active-tab-font-style", font(self.active_font).into()),
            ("inactive-tab-foreground", hex(self.inactive_foreground)),
            ("inactive-tab-background", hex(self.inactive_background)),
            ("inactive-tab-font-style", font(self.inactive_font).into()),
            ("tab-bar-background", hex(self.bar_background)),
        ]
    }
}

fn set<T>(slot: &mut T, v: T) -> bool {
    *slot = v;
    true
}

fn opt_color(slot: &mut Option<Rgb>, v: &str) -> bool {
    match color(v) {
        Some(c) => {
            *slot = c;
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_the_current_look() {
        let l = TabBarLook::default();
        assert_eq!(l.style, TabBarStyle::Default);
        assert!(l.active_background.is_none() && l.bar_background.is_none());
        assert_eq!(l.fade, DEFAULT_FADE.to_vec());
        assert_eq!(l.separator, " ┇");
    }

    #[test]
    fn hidden_style_hides_the_bar_like_never() {
        use crate::config::WindowShowTabBar::*;
        let mut l = TabBarLook::default();
        assert!(l.bar_visible(Auto, 2) && !l.bar_visible(Auto, 1) && l.bar_visible(Always, 1));
        l.apply("tab-bar-style", "hidden");
        for show in [Auto, Always, Never] {
            assert!(!l.bar_visible(show, 5), "{show:?}");
        }
    }

    #[test]
    fn keys_parse_like_kitty_and_bad_values_keep_the_old() {
        let mut l = TabBarLook::default();
        assert_eq!(l.apply("tab-bar-style", "powerline"), Some(true));
        assert_eq!(l.apply("tab-powerline-style", "round"), Some(true));
        assert_eq!(l.apply("tab-bar-align", "end"), Some(true));
        assert_eq!(
            (l.style, l.powerline, l.align),
            (TabBarStyle::Powerline, TabPowerlineStyle::Round, TabBarAlign::Right)
        );
        assert_eq!(l.apply("tab-bar-style", "custom"), Some(false));
        assert_eq!(l.style, TabBarStyle::Powerline, "kitty의 custom은 받지 않는다");
        assert_eq!(l.apply("tab-fade", "0 0.5 1"), Some(true));
        assert_eq!(l.fade, vec![0.0, 0.5, 1.0]);
        assert_eq!(l.apply("tab-fade", "0.5 2"), Some(false));
        assert_eq!(l.fade, vec![0.0, 0.5, 1.0]);
        assert_eq!(l.apply("tab-separator", "\" | \""), Some(true));
        assert_eq!(l.separator, " | ");
        assert_eq!(l.apply("active-tab-background", "#102030"), Some(true));
        assert_eq!(l.active_background, Some(Rgb::new(0x10, 0x20, 0x30)));
        assert_eq!(l.apply("active-tab-background", "none"), Some(true));
        assert_eq!(l.active_background, None);
        assert_eq!(l.apply("inactive-tab-foreground", "nosuchcolor"), Some(false));
        assert_eq!(l.apply("active-tab-font-style", "bold_italic"), Some(true));
        assert!(l.active_font.bold() && l.active_font.italic());
        assert_eq!(l.apply("inactive-tab-font-style", "heavy"), Some(false));
        assert_eq!(l.apply("something-else", "x"), None);
    }

    /// 설정 파일에서: 키를 알아보고(경고 없음), 틀린 값은 경고하고 이전 값을 두며, `+show-config`에 다시 찍힌다
    #[test]
    fn config_reads_the_keys_warns_on_bad_values_and_prints_them_back() {
        use crate::config::Config;
        let text = "tab-bar-style = powerline\ntab-powerline-style = round\ntab-bar-align = center\n\
                    tab-separator = \" | \"\ntab-fade = 0.5 1\nactive-tab-foreground = #010203\n\
                    active-tab-background = #040506\nactive-tab-font-style = bold-italic\n\
                    inactive-tab-foreground = red\ninactive-tab-background = #070809\n\
                    inactive-tab-font-style = italic\ntab-bar-background = #0a0b0c\n";
        let c = Config::from_text(text, true);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        let l = &c.tab_bar_look;
        assert_eq!(
            (l.style, l.powerline, l.align),
            (TabBarStyle::Powerline, TabPowerlineStyle::Round, TabBarAlign::Center)
        );
        assert_eq!(l.separator, " | ");
        assert_eq!(l.fade, vec![0.5, 1.0]);
        assert_eq!(l.bar_background, Some(Rgb::new(10, 11, 12)));
        assert_eq!(l.inactive_foreground, Some(Rgb::new(255, 0, 0)));
        // 찍은 것을 다시 읽으면 같다
        let again = Config::from_text(&c.render(), true);
        assert!(again.warnings.is_empty(), "{:?}", again.warnings);
        assert_eq!(again.tab_bar_look, c.tab_bar_look);
        // 틀린 값은 키마다 경고 하나이고 기본값이 남는다
        for line in [
            "tab-bar-style = custom",
            "tab-powerline-style = square",
            "tab-bar-align = middle",
            "tab-fade = 2",
            "active-tab-foreground = nosuchcolor",
            "inactive-tab-font-style = heavy",
        ] {
            let c = Config::from_text(&format!("{line}\n"), true);
            assert_eq!(c.warnings.len(), 1, "{line}: {:?}", c.warnings);
            assert_eq!(c.tab_bar_look, TabBarLook::default(), "{line}");
        }
    }
}
