//! 스크롤 막대 모양 키(kitty `scrollbar_*`, #917): 읽기·`+show-config`·경고, 보이는 때(`scrollbar`)

use bt_core::config::{Scrollbar, ScrollbarColor, ScrollbarStyle};
use bt_core::{Config, Rgb};

#[test]
fn unset_keys_keep_the_bar_b_terminal_always_drew() {
    // 설정이 없으면 모두 "정하지 않음"이다(kitty 기본값을 쓰지 않는다)
    assert_eq!(Config::default().scrollbar_style, ScrollbarStyle::default());
    let st = ScrollbarStyle::default();
    assert_eq!(
        (st.width, st.hover_width, st.radius, st.gap, st.min_handle_height),
        (None, None, None, None, None)
    );
    assert_eq!((st.handle_opacity, st.track_hover_opacity), (None, None));
    assert_eq!(st.track_opacity, 0.0);
    assert_eq!(
        (st.handle_color, st.track_color),
        (ScrollbarColor::Foreground, ScrollbarColor::Foreground)
    );
}

#[test]
fn keys_are_read_with_kitty_units_and_clamps() {
    let c = Config::from_text(
        "scrollbar-width = 1\nscrollbar-hover-width = 2.5\nscrollbar-radius = 0.4\nscrollbar-gap = 0.2\n\
         scrollbar-min-handle-height = 3\nscrollbar-handle-color = #ff0000\nscrollbar-handle-opacity = 0.8\n\
         scrollbar-track-color = selection-background\nscrollbar-track-opacity = 0.25\nscrollbar-track-hover-opacity = 7\n",
        true,
    );
    assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    let st = c.scrollbar_style;
    assert_eq!(st.width, Some(1.0));
    assert_eq!(st.hover_width, Some(2.5));
    assert_eq!(st.radius, Some(0.4));
    assert_eq!(st.gap, Some(0.2));
    assert_eq!(st.min_handle_height, Some(3.0));
    assert_eq!(st.handle_color, ScrollbarColor::Rgb(Rgb::new(255, 0, 0)));
    assert_eq!(st.handle_opacity, Some(0.8));
    assert_eq!(st.track_color, ScrollbarColor::SelectionBackground);
    assert_eq!(st.track_opacity, 0.25);
    assert_eq!(st.track_hover_opacity, Some(1.0), "투명도는 1을 넘지 않는다");
}

#[test]
fn bad_values_warn_and_keep_the_previous_value() {
    let c = Config::from_text(
        "scrollbar-width = 1\nscrollbar-width = -1\nscrollbar-radius = round\nscrollbar-handle-color = nope\n\
         scrollbar-track-opacity = x\nscrollbar-handle-opacity = nan\n",
        true,
    );
    assert_eq!(c.warnings.len(), 5, "{:?}", c.warnings);
    assert_eq!(c.scrollbar_style.width, Some(1.0));
    assert_eq!(c.scrollbar_style.radius, None);
    assert_eq!(c.scrollbar_style.handle_color, ScrollbarColor::Foreground);
    assert_eq!(c.scrollbar_style.handle_opacity, None);
}

#[test]
fn empty_or_default_goes_back_to_unset() {
    let c = Config::from_text(
        "scrollbar-width = 2\nscrollbar-width = default\nscrollbar-gap = 1\nscrollbar-gap =\n\
         scrollbar-handle-color = #00ff00\nscrollbar-handle-color = default\nscrollbar-track-opacity = 0.5\nscrollbar-track-opacity =\n",
        true,
    );
    assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    assert_eq!(c.scrollbar_style, ScrollbarStyle::default());
}

#[test]
fn show_config_prints_every_key_and_reads_back() {
    let shown = Config::default().render();
    for k in [
        "scrollbar-width = default",
        "scrollbar-hover-width = default",
        "scrollbar-radius = default",
        "scrollbar-gap = default",
        "scrollbar-min-handle-height = default",
        "scrollbar-handle-color = foreground",
        "scrollbar-handle-opacity = default",
        "scrollbar-track-color = foreground",
        "scrollbar-track-opacity = 0",
        "scrollbar-track-hover-opacity = default",
    ] {
        assert!(shown.contains(k), "{k}\n{shown}");
    }
    let set = Config::from_text(
        "scrollbar-width = 1\nscrollbar-handle-color = #ff0000\nscrollbar-track-opacity = 0.25\nscrollbar-track-color = selection-background\n",
        true,
    );
    let out = set.render();
    assert!(
        out.contains("scrollbar-width = 1\n") || out.contains("scrollbar-width = 1"),
        "{out}"
    );
    assert!(out.contains("scrollbar-handle-color = #ff0000"), "{out}");
    let back = Config::from_text(&out, true);
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    assert_eq!(back.scrollbar_style, set.scrollbar_style);
}

#[test]
fn color_resolves_against_the_current_theme() {
    let (fg, sel) = (Rgb::new(1, 2, 3), Rgb::new(4, 5, 6));
    assert_eq!(ScrollbarColor::Foreground.resolve(fg, sel), fg);
    assert_eq!(ScrollbarColor::SelectionBackground.resolve(fg, sel), sel);
    assert_eq!(
        ScrollbarColor::Rgb(Rgb::new(9, 9, 9)).resolve(fg, sel),
        Rgb::new(9, 9, 9)
    );
}

#[test]
fn scrollbar_key_takes_the_kitty_values_too() {
    for (v, want) in [
        ("system", Scrollbar::System),
        ("never", Scrollbar::Never),
        ("always", Scrollbar::Always),
        ("scrolled", Scrollbar::Scrolled),
        ("hovered", Scrollbar::Hovered),
        ("scrolled-and-hovered", Scrollbar::ScrolledAndHovered),
        ("scrolled-or-hovered", Scrollbar::ScrolledOrHovered),
    ] {
        let c = Config::from_text(&format!("scrollbar = {v}\n"), true);
        assert_eq!(c.scrollbar, want, "{v}");
        assert!(
            c.render().contains(&format!("scrollbar = {v}\n")) || c.render().contains(&format!("scrollbar = {v}")),
            "{v}"
        );
    }
    assert_eq!(
        Config::from_text("scrollbar = sometimes\n", true).scrollbar,
        Scrollbar::System
    );
}

#[test]
fn kitty_scrollbar_values_decide_when_it_shows() {
    // (history, scrolling, hovering, system_always, scrolled_back)
    let s = |m: Scrollbar, h, sc, ho, al, sb| m.shows(h, sc, ho, al, sb);
    // 스크롤백이 없으면 어느 값이든 없다
    for m in [
        Scrollbar::Always,
        Scrollbar::Scrolled,
        Scrollbar::Hovered,
        Scrollbar::ScrolledOrHovered,
    ] {
        assert!(!s(m, 0, true, true, true, true), "{m:?}");
    }
    assert!(s(Scrollbar::Always, 5, false, false, false, false));
    assert!(
        !s(Scrollbar::Scrolled, 5, true, true, true, false),
        "위로 올라가지 않았으면 없다(방금 스크롤했어도)"
    );
    assert!(s(Scrollbar::Scrolled, 5, false, false, false, true));
    assert!(s(Scrollbar::Hovered, 5, false, true, false, false));
    assert!(!s(Scrollbar::Hovered, 5, true, false, true, true));
    assert!(!s(Scrollbar::ScrolledAndHovered, 5, false, true, false, false));
    assert!(!s(Scrollbar::ScrolledAndHovered, 5, false, false, false, true));
    assert!(s(Scrollbar::ScrolledAndHovered, 5, false, true, false, true));
    assert!(s(Scrollbar::ScrolledOrHovered, 5, false, true, false, false));
    assert!(s(Scrollbar::ScrolledOrHovered, 5, false, false, false, true));
    assert!(!s(Scrollbar::ScrolledOrHovered, 5, true, false, true, false));
    // 시스템 방식은 그대로: 방금 스크롤했거나 마우스가 있거나 시스템 설정이 항상
    assert!(s(Scrollbar::System, 5, true, false, false, false));
    assert!(s(Scrollbar::System, 5, false, false, true, false));
    assert!(
        !s(Scrollbar::System, 5, false, false, false, true),
        "시스템 방식은 올라가 있는 것만으로 보이지 않는다"
    );
    assert!(!s(Scrollbar::Never, 5, true, true, true, true));
}
