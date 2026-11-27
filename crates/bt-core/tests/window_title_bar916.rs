//! 칸 제목 막대 설정(#916): 설정 파일 글만으로(공개 `Config`) 키를 읽고 `+show-config`에 다시 찍는다

use bt_core::config::Config;

const KEYS: &str = "window-title-bar = bottom\nwindow-title-bar-min-windows = 2\n\
    window-title-template = \"{title} {window.columns}x{window.lines}\"\n\
    active-window-title-template = \"{fmt.bold}{title}\"\n\
    window-title-bar-active-foreground = #010203\nwindow-title-bar-active-background = #040506\n\
    window-title-bar-inactive-foreground = #0a0b0c\nwindow-title-bar-inactive-background = none\n\
    window-title-bar-align = right\n";

#[test]
fn kitty_window_title_bar_keys_are_known_and_printed_back() {
    let c = Config::from_text(KEYS, true);
    assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    let shown = c.render();
    for line in [
        "window-title-bar = bottom",
        "window-title-bar-min-windows = 2",
        "window-title-template = \"{title} {window.columns}x{window.lines}\"",
        "active-window-title-template = \"{fmt.bold}{title}\"",
        "window-title-bar-active-foreground = #010203",
        "window-title-bar-inactive-background = none",
        "window-title-bar-align = right",
    ] {
        assert!(shown.lines().any(|l| l == line), "{line}\n{shown}");
    }
}

/// 설정이 없으면 막대는 보이지 않는다(`min-windows = 0`)
#[test]
fn without_settings_the_bar_is_off() {
    let shown = Config::from_text("", true).render();
    assert!(
        shown.lines().any(|l| l == "window-title-bar-min-windows = 0"),
        "{shown}"
    );
}
