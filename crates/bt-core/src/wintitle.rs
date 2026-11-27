//! 칸마다 제목 막대(kitty `window_title_bar`·`window_title_bar_min_windows`·`window_title_template`·`active_window_title_template`·
//! `window_title_bar_active_foreground`·`_background`·`window_title_bar_inactive_foreground`·`_background`·`window_title_bar_align`, #916).
//! 키는 kitty와 같은 뜻이고 `-`로 잇는다. 틀 해석은 탭 제목 틀(`tabtitle`, ADR-098)을 그대로 쓴다: 이름 바꾸기와 꾸밈만 받고
//! 파이썬 표현식·`{custom}`은 받지 않으며, 칸 안 프로그램이 보낸 제목·작업 폴더는 값으로만 끼우고 다시 해석하지 않는다.
//!
//! 기본 `window-title-bar-min-windows = 0`은 막대를 보이지 않는다. 그러면 칸 배치와 픽셀은 이 기능이 없을 때와 같다

use crate::tabtitle;
use crate::theme::Rgb;

/// kitty `window_title_template`의 기본값
pub const DEFAULT_TEMPLATE: &str = "{fmt.fg.red}{bell_symbol}{activity_symbol}{fmt.fg.window}{progress_percent}{title}";

/// `window-title-bar`: 막대를 칸 글자 영역의 위나 아래에 둔다
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarEdge {
    #[default]
    Top,
    Bottom,
}

/// `window-title-bar-align`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BarAlign {
    Left,
    #[default]
    Center,
    Right,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowTitleConfig {
    pub edge: BarEdge,
    /// 막대를 보이는 최소 칸 수. 0이면 보이지 않고 1이면 늘 보인다
    pub min_windows: u32,
    pub template: String,
    /// `none`이면 `None`: 고른 칸도 `template`을 쓴다
    pub active_template: Option<String>,
    /// 적지 않으면(`none`) 탭 줄의 고른 탭·안 고른 탭 색을 따른다(kitty는 `active_tab_*`·`inactive_tab_*`)
    pub active_foreground: Option<Rgb>,
    pub active_background: Option<Rgb>,
    pub inactive_foreground: Option<Rgb>,
    pub inactive_background: Option<Rgb>,
    pub align: BarAlign,
}

impl Default for WindowTitleConfig {
    fn default() -> Self {
        Self {
            edge: BarEdge::Top,
            min_windows: 0,
            template: DEFAULT_TEMPLATE.to_string(),
            active_template: None,
            active_foreground: None,
            active_background: None,
            inactive_foreground: None,
            inactive_background: None,
            align: BarAlign::Center,
        }
    }
}

impl WindowTitleConfig {
    /// 키 하나를 적용한다. 이 모듈의 키가 아니면 `None`, 값이 틀리면 `Some(false)`(이전 값을 둔다), 받으면 `Some(true)`
    pub fn apply(&mut self, key: &str, v: &str) -> Option<bool> {
        let opt_color = |slot: &mut Option<Rgb>, v: &str| match crate::tabstyle::color(v) {
            Some(c) => {
                *slot = c;
                true
            }
            None => false,
        };
        let ok = match key {
            "window-title-bar" => match v.trim() {
                "top" => {
                    self.edge = BarEdge::Top;
                    true
                }
                "bottom" => {
                    self.edge = BarEdge::Bottom;
                    true
                }
                _ => false,
            },
            "window-title-bar-min-windows" => match v.trim().parse::<u32>() {
                Ok(n) => {
                    self.min_windows = n;
                    true
                }
                Err(_) => false,
            },
            "window-title-template" => {
                // 따옴표 없이 비운 값은 기본 틀로 되돌린다. `""`는 빈 틀(막대를 그리지 않음)이다
                let t = if v.trim().is_empty() {
                    DEFAULT_TEMPLATE
                } else {
                    tabtitle::unquote(v)
                };
                if tabtitle::window_template_ok(t) {
                    self.template = t.to_string();
                    true
                } else {
                    false
                }
            }
            "active-window-title-template" => {
                let t = tabtitle::unquote(v);
                if t == "none" || v.trim().is_empty() {
                    self.active_template = None;
                    true
                } else if tabtitle::window_template_ok(t) {
                    self.active_template = Some(t.to_string());
                    true
                } else {
                    false
                }
            }
            "window-title-bar-active-foreground" => opt_color(&mut self.active_foreground, v),
            "window-title-bar-active-background" => opt_color(&mut self.active_background, v),
            "window-title-bar-inactive-foreground" => opt_color(&mut self.inactive_foreground, v),
            "window-title-bar-inactive-background" => opt_color(&mut self.inactive_background, v),
            "window-title-bar-align" => match v.trim() {
                "left" => {
                    self.align = BarAlign::Left;
                    true
                }
                "center" => {
                    self.align = BarAlign::Center;
                    true
                }
                "right" => {
                    self.align = BarAlign::Right;
                    true
                }
                _ => false,
            },
            _ => return None,
        };
        Some(ok)
    }

    /// `+show-config`에 찍을 (키, 값)
    pub fn lines(&self) -> Vec<(&'static str, String)> {
        let hex =
            |c: Option<Rgb>| c.map_or_else(|| "none".to_string(), |c| format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b));
        vec![
            (
                "window-title-bar",
                match self.edge {
                    BarEdge::Top => "top",
                    BarEdge::Bottom => "bottom",
                }
                .into(),
            ),
            ("window-title-bar-min-windows", self.min_windows.to_string()),
            ("window-title-template", format!("\"{}\"", self.template)),
            (
                "active-window-title-template",
                self.active_template
                    .as_ref()
                    .map_or_else(|| "none".to_string(), |t| format!("\"{t}\"")),
            ),
            ("window-title-bar-active-foreground", hex(self.active_foreground)),
            ("window-title-bar-active-background", hex(self.active_background)),
            ("window-title-bar-inactive-foreground", hex(self.inactive_foreground)),
            ("window-title-bar-inactive-background", hex(self.inactive_background)),
            (
                "window-title-bar-align",
                match self.align {
                    BarAlign::Left => "left",
                    BarAlign::Center => "center",
                    BarAlign::Right => "right",
                }
                .into(),
            ),
        ]
    }

    /// 이 칸이 쓸 틀(kitty: 고른 칸이고 `active_window_title_template`이 있으면 그것)
    pub fn template_for(&self, active: bool) -> &str {
        match (&self.active_template, active) {
            (Some(t), true) => t,
            _ => &self.template,
        }
    }

    /// 보이는 칸이 `visible`개일 때 막대를 보이는가(kitty `layout/base.py`: `min_windows > 0 and visible >= min_windows`)
    pub fn shown(&self, visible: usize) -> bool {
        self.min_windows > 0 && visible >= self.min_windows as usize
    }
}

/// 막대가 `columns`칸이고 글이 `text_cells`칸일 때 글 앞의 빈 칸 수(kitty `WindowTitleBarScreen.render`:
/// 가운데면 `(칸 - 글) // 2`, 오른쪽이면 `칸 - 글`, 왼쪽이면 0. 글이 넘치면 0)
pub fn align_pad(align: BarAlign, columns: usize, text_cells: usize) -> usize {
    let room = columns.saturating_sub(text_cells);
    match align {
        BarAlign::Left => 0,
        BarAlign::Center => room / 2,
        BarAlign::Right => room,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_hide_the_bar_and_match_kitty() {
        let c = WindowTitleConfig::default();
        assert!(!c.shown(1) && !c.shown(2) && !c.shown(99));
        assert_eq!(c.edge, BarEdge::Top);
        assert_eq!(c.align, BarAlign::Center);
        assert_eq!(c.template_for(true), DEFAULT_TEMPLATE);
        assert!(tabtitle::window_template_ok(DEFAULT_TEMPLATE));
    }

    #[test]
    fn min_windows_zero_never_one_always_n_from_n() {
        let mut c = WindowTitleConfig::default();
        assert_eq!(c.apply("window-title-bar-min-windows", "1"), Some(true));
        assert!(c.shown(1) && c.shown(3));
        c.apply("window-title-bar-min-windows", "2");
        assert!(!c.shown(1) && c.shown(2) && c.shown(3));
        c.apply("window-title-bar-min-windows", "0");
        assert!(!c.shown(5));
    }

    #[test]
    fn align_follows_kitty_padding_rule() {
        assert_eq!(align_pad(BarAlign::Left, 20, 6), 0);
        assert_eq!(align_pad(BarAlign::Center, 20, 6), 7);
        assert_eq!(align_pad(BarAlign::Center, 21, 6), 7);
        assert_eq!(align_pad(BarAlign::Right, 20, 6), 14);
        // 글이 넘치면 빈 칸이 없다
        assert_eq!(align_pad(BarAlign::Center, 4, 10), 0);
        assert_eq!(align_pad(BarAlign::Right, 4, 10), 0);
    }

    #[test]
    fn apply_takes_each_key_and_rejects_bad_values_keeping_the_old_one() {
        let mut c = WindowTitleConfig::default();
        assert_eq!(c.apply("window-title-bar", "bottom"), Some(true));
        assert_eq!(c.edge, BarEdge::Bottom);
        assert_eq!(c.apply("window-title-bar", "left"), Some(false));
        assert_eq!(c.edge, BarEdge::Bottom);
        assert_eq!(c.apply("window-title-bar-min-windows", "-1"), Some(false));
        assert_eq!(c.apply("window-title-bar-min-windows", "x"), Some(false));
        assert_eq!(c.apply("window-title-bar-align", "right"), Some(true));
        assert_eq!(c.align, BarAlign::Right);
        assert_eq!(c.apply("window-title-bar-align", "middle"), Some(false));
        assert_eq!(c.apply("window-title-bar-active-foreground", "#010203"), Some(true));
        assert_eq!(c.active_foreground, Some(Rgb::new(1, 2, 3)));
        assert_eq!(c.apply("window-title-bar-active-foreground", "none"), Some(true));
        assert_eq!(c.active_foreground, None);
        assert_eq!(
            c.apply("window-title-bar-inactive-background", "nosuchcolor"),
            Some(false)
        );
        // 표현식·모르는 이름·탭 틀의 이름은 받지 않는다
        assert_eq!(c.apply("window-title-template", "{title.upper()}"), Some(false));
        assert_eq!(c.apply("window-title-template", "{index}"), Some(false));
        assert_eq!(c.apply("window-title-template", "{custom}"), Some(true));
        assert_eq!(c.apply("window-title-template", "\"{window.id}: {title}\""), Some(true));
        assert_eq!(c.template, "{window.id}: {title}");
        assert_eq!(
            c.apply("active-window-title-template", "\"{fmt.bold}{title}\""),
            Some(true)
        );
        assert_eq!(c.template_for(true), "{fmt.bold}{title}");
        assert_eq!(c.template_for(false), "{window.id}: {title}");
        assert_eq!(c.apply("active-window-title-template", "none"), Some(true));
        assert_eq!(c.active_template, None);
        assert_eq!(c.apply("tab-title-template", "x"), None);
    }

    /// 설정 파일에서: 키를 알아보고 경고가 없고, `+show-config`에 다시 찍히고, 틀린 값은 경고 하나에 기본값이 남는다
    #[test]
    fn config_reads_the_keys_and_prints_them_back() {
        use crate::config::Config;
        let text = "window-title-bar = bottom\nwindow-title-bar-min-windows = 2\n\
                    window-title-template = \"{title} {window.columns}x{window.lines}\"\n\
                    active-window-title-template = \"{fmt.bold}{title}\"\n\
                    window-title-bar-active-foreground = #010203\nwindow-title-bar-active-background = #040506\n\
                    window-title-bar-inactive-foreground = red\nwindow-title-bar-inactive-background = none\n\
                    window-title-bar-align = left\n";
        let c = Config::from_text(text, true);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        let w = &c.window_title;
        assert_eq!((w.edge, w.min_windows, w.align), (BarEdge::Bottom, 2, BarAlign::Left));
        assert_eq!(w.inactive_foreground, Some(Rgb::new(255, 0, 0)));
        assert_eq!(w.inactive_background, None);
        let again = Config::from_text(&c.render(), true);
        assert!(again.warnings.is_empty(), "{:?}", again.warnings);
        assert_eq!(again.window_title, c.window_title);
        for line in [
            "window-title-bar = middle",
            "window-title-bar-min-windows = many",
            "window-title-template = \"{no_such_name}\"",
            "window-title-bar-align = justify",
            "window-title-bar-active-background = nosuchcolor",
        ] {
            let c = Config::from_text(line, true);
            assert_eq!(c.warnings.len(), 1, "{line}: {:?}", c.warnings);
            assert_eq!(c.window_title, WindowTitleConfig::default(), "{line}");
        }
    }
}
