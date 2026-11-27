//! 오른쪽 클릭 메뉴(#278, `right-click-action = context-menu`). 판단(무엇을 고를까, 어떤 항목을 보일까)을
//! 순수 함수로 빼서 창 없이 테스트한다. 메뉴를 띄우는 일은 app.rs와 macOS 전용 `popup`이 한다.
//! Ghostty: Surface.zig 4220~4246줄(오른쪽 누름의 고르기), SurfaceView_AppKit.swift 1580~1654줄(메뉴 항목)

use bt_term::links::LinkSpan;

/// 오른쪽 누름이 고르기를 어떻게 바꾸는가
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// 누른 자리가 이미 고른 영역 안이라 그대로 둔다
    Keep,
    /// 누른 자리의 링크를 고른다
    Link,
    /// 누른 자리의 단어를 고른다
    Word,
}

/// Ghostty 규칙: 이미 고른 영역이 누른 자리를 품으면 그대로, 아니면 링크가 있으면 링크, 없으면 단어
pub fn pick(selection_contains_click: bool, link_here: bool) -> Pick {
    if selection_contains_click {
        Pick::Keep
    } else if link_here {
        Pick::Link
    } else {
        Pick::Word
    }
}

/// 왼쪽 누름을 오른쪽 누름 + 메뉴로 볼까(macOS의 Ctrl+왼쪽 클릭, #725). Ghostty 1.4.0 `SurfaceView_AppKit.menu(for:)`처럼
/// 칸의 프로그램이 마우스 모드를 켜지 않았을 때만(`mouseCaptured`는 `terminal.flags.mouse_event != .none`).
/// Shift·읽기 전용·`mouse-reporting`은 보지 않는다. 모드가 켜졌으면 보통 왼쪽 클릭이라 보고하거나 고른다
pub fn ctrl_click_opens_menu(macos: bool, ctrl: bool, program_mouse_mode: bool) -> bool {
    macos && ctrl && !program_mouse_mode
}

/// 링크가 차지한 칸들의 처음 `(줄, 칸)`과 마지막 `(줄, 칸)`(마지막 칸 포함). 비었으면 없다
pub fn link_ends(spans: &[LinkSpan]) -> Option<((usize, usize), (usize, usize))> {
    let mut cells = spans.iter().filter(|s| s.end > s.start);
    let first = cells.next()?;
    let last = cells.next_back().unwrap_or(first);
    Some(((first.row, first.start), (last.row, last.end - 1)))
}

/// 메뉴 한 줄
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Item {
        id: &'static str,
        text: &'static str,
    },
    Check {
        id: &'static str,
        text: &'static str,
        checked: bool,
    },
    Separator,
}

/// 메뉴 항목 목록. 고른 글이 있을 때만 복사가 맨 위에 든다
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn entries(has_selection: bool, readonly: bool) -> Vec<Entry> {
    use Entry::{Check, Item, Separator};
    let mut v = Vec::new();
    if has_selection {
        v.push(Item {
            id: "copy",
            text: "복사",
        });
    }
    v.extend([
        Item {
            id: "paste",
            text: "붙여넣기",
        },
        Separator,
        Item {
            id: "split_right",
            text: "오른쪽으로 나누기",
        },
        Item {
            id: "split_left",
            text: "왼쪽으로 나누기",
        },
        Item {
            id: "split_down",
            text: "아래로 나누기",
        },
        Item {
            id: "split_up",
            text: "위로 나누기",
        },
        Separator,
        Item {
            id: "close_surface",
            text: "나눈 칸 닫기",
        },
        Separator,
        Item {
            id: "reset_terminal",
            text: "터미널 초기화",
        },
        Check {
            id: "toggle_readonly",
            text: "읽기 전용",
            checked: readonly,
        },
        Separator,
        Item {
            id: "tab_title",
            text: "탭 제목 바꾸기…",
        },
        Item {
            id: "surface_title",
            text: "터미널 제목 바꾸기…",
        },
    ]);
    v
}

/// 탭 오른쪽 단추 메뉴의 한 줄(#852)
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TabEntry {
    /// 탭 제목 바꾸기
    Rename,
    Separator,
    /// 색 한 가지. 지금 탭의 색이면 `selected`
    Color {
        color: crate::tabcolor::TabColor,
        selected: bool,
    },
}

/// 탭 메뉴 항목. Ghostty 1.4.0처럼 "탭 제목 바꾸기" 밑에 구분선과 색 팔레트가 온다
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn tab_entries(current: crate::tabcolor::TabColor) -> Vec<TabEntry> {
    let mut v = vec![TabEntry::Rename, TabEntry::Separator];
    v.extend(crate::tabcolor::TabColor::ALL.map(|color| TabEntry::Color {
        color,
        selected: color == current,
    }));
    v
}

/// 탭 메뉴에 붙는 탭 묶음 항목(#933)
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupEntry {
    Separator,
    /// 새 묶음으로 묶기
    New,
    /// 묶음에서 빼기
    Leave,
    /// 옆 탭의 묶음에 넣기
    Join,
    /// 묶음 이름 바꾸기
    Rename,
    /// 접기(`collapsed`가 거짓일 때) 또는 펴기
    Toggle {
        collapsed: bool,
    },
    /// 묶음 색 하위 메뉴. 지금 색이 고른 것이다
    Colors {
        current: crate::tabcolor::TabColor,
    },
}

/// 지금 탭의 메뉴에 붙는 묶음 항목. `in_bundle`은 탭이 묶음에 들었는가, `sole`은 그 묶음에 혼자인가,
/// `can_join`은 옆 탭의 묶음에 넣을 수 있는가. 묶음에 들지 않았으면 이름·접기·색은 없다
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn group_entries(
    in_bundle: bool,
    sole: bool,
    can_join: bool,
    collapsed: bool,
    color: crate::tabcolor::TabColor,
) -> Vec<GroupEntry> {
    let mut v = vec![GroupEntry::Separator];
    if !sole {
        v.push(GroupEntry::New);
    }
    if can_join {
        v.push(GroupEntry::Join);
    }
    if in_bundle {
        v.push(GroupEntry::Leave);
        v.push(GroupEntry::Rename);
        v.push(GroupEntry::Toggle { collapsed });
        v.push(GroupEntry::Colors { current: color });
    }
    v
}

/// 메뉴 항목 id(`menu::action_for`가 푼다)
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn group_item_id(e: &GroupEntry) -> Option<&'static str> {
    Some(match e {
        GroupEntry::New => "tab_group_new",
        GroupEntry::Leave => "tab_group_leave",
        GroupEntry::Join => "tab_group_join",
        GroupEntry::Rename => "tab_group_rename",
        GroupEntry::Toggle { .. } => "tab_group_toggle",
        GroupEntry::Separator | GroupEntry::Colors { .. } => return None,
    })
}

/// 묶음 머리표 메뉴의 항목 id: `chip_<묶음 번호>_<일>`(`menu::action_for`가 푼다)
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn chip_item_id(bundle: crate::tabbundle::Id, op: &str) -> String {
    format!("chip_{bundle}_{op}")
}

#[cfg(target_os = "macos")]
fn color_submenu(
    title: &str,
    current: crate::tabcolor::TabColor,
    dark: bool,
    id_of: &dyn Fn(crate::tabcolor::TabColor) -> String,
) -> muda::Submenu {
    use crate::tabcolor::{SWATCH_PX, TabColor, swatch_rgba};
    use muda::{Icon, IconMenuItem, Submenu};
    let sub = Submenu::new(title, true);
    for color in TabColor::ALL {
        let selected = color == current;
        let icon = Icon::from_rgba(swatch_rgba(color, selected, dark), SWATCH_PX, SWATCH_PX).ok();
        let text = if selected {
            format!("{} ✓", color.label())
        } else {
            color.label().to_string()
        };
        let _ = sub.append(&IconMenuItem::with_id(id_of(color), text, true, icon, None));
    }
    sub
}

/// 묶음 머리표의 오른쪽 단추 메뉴를 띄운다(macOS, 모달, #933): 접기·펴기, 이름 바꾸기, 색, 묶음 풀기
#[cfg(target_os = "macos")]
pub fn popup_bundle(bundle: crate::tabbundle::Id, collapsed: bool, color: crate::tabcolor::TabColor, dark: bool) {
    use muda::{ContextMenu, Menu, MenuItem, PredefinedMenuItem};
    let menu = Menu::new();
    let toggle = if collapsed { "묶음 펴기" } else { "묶음 접기" };
    let _ = menu.append(&MenuItem::with_id(chip_item_id(bundle, "toggle"), toggle, true, None));
    let _ = menu.append(&MenuItem::with_id(
        chip_item_id(bundle, "rename"),
        "묶음 이름 바꾸기…",
        true,
        None,
    ));
    let _ = menu.append(&color_submenu("묶음 색", color, dark, &|c| {
        chip_item_id(bundle, &format!("color_{}", c.key()))
    }));
    let _ = menu.append(&PredefinedMenuItem::separator());
    let _ = menu.append(&MenuItem::with_id(
        chip_item_id(bundle, "dissolve"),
        "묶음 풀기",
        true,
        None,
    ));
    winit::platform::macos::popup_context_menu(menu.ns_menu());
}

/// 탭 메뉴를 마우스 자리에 띄운다(`popup`처럼 모달). 색 항목은 동그란 견본 그림이 붙고, 고른 색은 고리와 뒤의 표시로 구별한다.
/// `groups`는 탭 묶음 항목(#933)이고 색 아래에 붙는다
#[cfg(target_os = "macos")]
pub fn popup_tab(entries: &[TabEntry], groups: &[GroupEntry], dark: bool) {
    use crate::tabcolor::{SWATCH_PX, swatch_rgba};
    use muda::{ContextMenu, Icon, IconMenuItem, Menu, MenuItem, PredefinedMenuItem};
    let menu = Menu::new();
    for e in entries {
        let _ = match e {
            TabEntry::Rename => menu.append(&MenuItem::with_id("tab_title", "탭 제목 바꾸기…", true, None)),
            TabEntry::Separator => menu.append(&PredefinedMenuItem::separator()),
            TabEntry::Color { color, selected } => {
                let icon = Icon::from_rgba(swatch_rgba(*color, *selected, dark), SWATCH_PX, SWATCH_PX).ok();
                let text = if *selected {
                    format!("{} ✓", color.label())
                } else {
                    color.label().to_string()
                };
                menu.append(&IconMenuItem::with_id(color.menu_id(), text, true, icon, None))
            }
        };
    }
    for g in groups {
        let _ = match g {
            GroupEntry::Separator => menu.append(&PredefinedMenuItem::separator()),
            GroupEntry::Colors { current } => menu.append(&color_submenu("묶음 색", *current, dark, &|c| {
                format!("tab_group_color_{}", c.key())
            })),
            GroupEntry::Toggle { collapsed } => menu.append(&MenuItem::with_id(
                "tab_group_toggle",
                if *collapsed { "묶음 펴기" } else { "묶음 접기" },
                true,
                None,
            )),
            other => {
                let text = match other {
                    GroupEntry::New => "새 묶음으로 묶기",
                    GroupEntry::Leave => "묶음에서 빼기",
                    GroupEntry::Join => "옆 탭의 묶음에 넣기",
                    _ => "묶음 이름 바꾸기…",
                };
                menu.append(&MenuItem::with_id(group_item_id(other).unwrap_or(""), text, true, None))
            }
        };
    }
    winit::platform::macos::popup_context_menu(menu.ns_menu());
}

/// 메뉴를 마우스 자리에 띄운다. 항목을 고르면 앱 메뉴와 같은 길(`muda::MenuEvent` → `UserEvent::Menu`)로 동작이 온다.
/// 메뉴를 닫을 때까지 돌아오지 않는다(AppKit 메뉴 추적). vendor winit의 안전한 함수로 띄운다(unsafe는 vendor 안에만 있다)
#[cfg(target_os = "macos")]
pub fn popup(entries: &[Entry]) {
    use muda::{CheckMenuItem, ContextMenu, Menu, MenuItem, PredefinedMenuItem};
    let menu = Menu::new();
    for e in entries {
        let _ = match e {
            Entry::Item { id, text } => menu.append(&MenuItem::with_id(*id, *text, true, None)),
            Entry::Check { id, text, checked } => {
                menu.append(&CheckMenuItem::with_id(*id, *text, true, *checked, None))
            }
            Entry::Separator => menu.append(&PredefinedMenuItem::separator()),
        };
    }
    // `menu`가 이 호출 동안 NSMenu를 쥐고 있다
    winit::platform::macos::popup_context_menu(menu.ns_menu());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::Action;
    use crate::menu::{MenuCommand, action_for};
    use crate::split::Direction;

    #[test]
    fn pick_keeps_selection_under_click() {
        assert_eq!(pick(true, false), Pick::Keep);
        assert_eq!(pick(true, true), Pick::Keep);
    }

    /// Ghostty 1.4.0 `SurfaceView_AppKit.menu(for:)`(#725): Ctrl+왼쪽 누름은 칸의 프로그램이 마우스 모드를 켰으면
    /// (`mouseCaptured`, Shift·읽기 전용·`mouse-reporting`과 상관없이) 메뉴 없이 보통 왼쪽 클릭이다. macOS만
    #[test]
    fn ctrl_click_menu_only_when_the_program_has_no_mouse_mode() {
        assert!(ctrl_click_opens_menu(true, true, false), "셸");
        assert!(
            !ctrl_click_opens_menu(true, true, true),
            "vim set mouse=a: Ctrl+Shift·읽기 전용이어도"
        );
        assert!(!ctrl_click_opens_menu(true, false, false), "Ctrl 없음");
        assert!(!ctrl_click_opens_menu(false, true, false), "macOS 밖");
    }

    #[test]
    fn pick_prefers_link_over_word() {
        assert_eq!(pick(false, true), Pick::Link);
        assert_eq!(pick(false, false), Pick::Word);
    }

    #[test]
    fn link_ends_spans_first_to_last_cell() {
        let spans = [
            LinkSpan {
                row: 3,
                start: 10,
                end: 80,
            },
            LinkSpan {
                row: 4,
                start: 0,
                end: 7,
            },
        ];
        assert_eq!(link_ends(&spans), Some(((3, 10), (4, 6))));
        assert_eq!(
            link_ends(&[LinkSpan {
                row: 1,
                start: 2,
                end: 3
            }]),
            Some(((1, 2), (1, 2)))
        );
        assert_eq!(link_ends(&[]), None);
        // 빈 구간만 있으면 고를 칸이 없다
        assert_eq!(
            link_ends(&[LinkSpan {
                row: 1,
                start: 2,
                end: 2
            }]),
            None
        );
    }

    fn ids(es: &[Entry]) -> Vec<&'static str> {
        es.iter()
            .map(|e| match e {
                Entry::Item { id, .. } | Entry::Check { id, .. } => *id,
                Entry::Separator => "-",
            })
            .collect()
    }

    #[test]
    fn entries_without_selection_have_no_copy() {
        assert_eq!(
            ids(&entries(false, false)),
            [
                "paste",
                "-",
                "split_right",
                "split_left",
                "split_down",
                "split_up",
                "-",
                "close_surface",
                "-",
                "reset_terminal",
                "toggle_readonly",
                "-",
                "tab_title",
                "surface_title"
            ]
        );
    }

    #[test]
    fn entries_with_selection_start_with_copy() {
        let e = entries(true, false);
        assert_eq!(ids(&e)[..2], ["copy", "paste"]);
        assert_eq!(e.len(), entries(false, false).len() + 1);
    }

    #[test]
    fn readonly_item_shows_check_state() {
        for on in [true, false] {
            let e = entries(false, on);
            let check = e.iter().find_map(|e| match e {
                Entry::Check { id, checked, .. } => Some((*id, *checked)),
                _ => None,
            });
            assert_eq!(check, Some(("toggle_readonly", on)));
        }
    }

    #[test]
    fn every_item_maps_to_an_existing_action() {
        let want = |id: &str| match id {
            "copy" => Action::Copy(bt_term::CopyFormat::Mixed),
            "paste" => Action::Paste,
            "split_right" => Action::NewSplit(Some(Direction::Right)),
            "split_left" => Action::NewSplit(Some(Direction::Left)),
            "split_down" => Action::NewSplit(Some(Direction::Down)),
            "split_up" => Action::NewSplit(Some(Direction::Up)),
            "close_surface" => Action::CloseSurface,
            "reset_terminal" => Action::Reset,
            "toggle_readonly" => Action::ToggleReadonly,
            "tab_title" => Action::PromptTabTitle,
            "surface_title" => Action::PromptSurfaceTitle,
            other => panic!("모르는 항목 {other}"),
        };
        let all = entries(true, false);
        assert!(!all.is_empty());
        for e in all {
            if let Entry::Item { id, .. } | Entry::Check { id, .. } = e {
                assert_eq!(action_for(id), Some(MenuCommand::Action(want(id))), "{id}");
            }
        }
    }

    /// 탭 메뉴(#852): 제목 바꾸기, 구분선, 색 열 가지(Ghostty 순서). 지금 색만 고른 표시이고 색이 없으면 "없음"이 고른 것이다
    #[test]
    fn tab_menu_has_rename_then_the_ten_colors_with_one_selected() {
        use crate::tabcolor::TabColor;
        for current in TabColor::ALL {
            let e = tab_entries(current);
            assert_eq!(e.len(), 2 + 10, "{current:?}");
            assert_eq!(e[0], TabEntry::Rename);
            assert_eq!(e[1], TabEntry::Separator);
            let colors: Vec<_> = e[2..]
                .iter()
                .map(|x| match x {
                    TabEntry::Color { color, selected } => (*color, *selected),
                    other => panic!("{other:?}"),
                })
                .collect();
            let order: Vec<_> = colors.iter().map(|c| c.0).collect();
            assert_eq!(order, TabColor::ALL);
            let selected: Vec<_> = colors.iter().filter(|c| c.1).map(|c| c.0).collect();
            assert_eq!(selected, [current]);
        }
    }

    /// 탭 메뉴의 색 항목 id가 모두 `SetTabColor`로 가고, 모르는 id는 계속 없다. 탭 제목 항목은 지금 탭의 제목 막대를 연다
    #[test]
    fn tab_menu_items_map_to_actions() {
        use crate::tabcolor::TabColor;
        for color in TabColor::ALL {
            assert_eq!(
                action_for(&color.menu_id()),
                Some(MenuCommand::Action(Action::SetTabColor(color)))
            );
        }
        assert_eq!(action_for("tab_color_magenta"), None);
        assert_eq!(
            action_for("tab_title"),
            Some(MenuCommand::Action(Action::PromptTabTitle))
        );
    }

    #[test]
    fn item_texts_are_korean_and_not_empty() {
        let all = entries(true, false);
        assert!(!all.is_empty());
        for e in all {
            if let Entry::Item { text, .. } | Entry::Check { text, .. } = e {
                assert!(!text.is_empty());
                assert!(text.chars().any(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c)), "{text}");
            }
        }
    }

    /// 탭 묶음 항목(#933): 묶음에 들지 않은 탭은 묶기·넣기만, 든 탭은 빼기·이름·접기·색도 보인다. 혼자뿐인 묶음은 "새 묶음" 없음
    #[test]
    fn group_menu_shows_only_what_applies() {
        use crate::tabcolor::TabColor;
        let none = group_entries(false, false, false, false, TabColor::None);
        assert_eq!(none, [GroupEntry::Separator, GroupEntry::New]);
        let join = group_entries(false, false, true, false, TabColor::None);
        assert_eq!(join, [GroupEntry::Separator, GroupEntry::New, GroupEntry::Join]);
        let inside = group_entries(true, false, false, true, TabColor::Teal);
        assert_eq!(
            inside,
            [
                GroupEntry::Separator,
                GroupEntry::New,
                GroupEntry::Leave,
                GroupEntry::Rename,
                GroupEntry::Toggle { collapsed: true },
                GroupEntry::Colors {
                    current: TabColor::Teal
                },
            ]
        );
        let sole = group_entries(true, true, false, false, TabColor::None);
        assert!(!sole.contains(&GroupEntry::New));
    }

    /// 탭 메뉴·머리표 메뉴의 항목 id가 모두 묶음 동작으로 가고, 모르는 id·깨진 번호는 없다
    #[test]
    fn group_menu_ids_map_to_actions() {
        use crate::tabbundle::BundleOp;
        use crate::tabcolor::TabColor;
        let act = |id: &str| match action_for(id) {
            Some(MenuCommand::Action(a)) => Some(a),
            other => {
                assert!(other.is_none(), "{id}: {other:?}");
                None
            }
        };
        assert_eq!(act("tab_group_new"), Some(Action::NewTabGroup));
        assert_eq!(act("tab_group_leave"), Some(Action::LeaveTabGroup));
        assert_eq!(act("tab_group_join"), Some(Action::JoinTabGroup));
        assert_eq!(act("tab_group_rename"), Some(Action::TabGroup(None, BundleOp::Rename)));
        assert_eq!(act("tab_group_toggle"), Some(Action::TabGroup(None, BundleOp::Toggle)));
        for c in TabColor::ALL {
            assert_eq!(
                act(&format!("tab_group_color_{}", c.key())),
                Some(Action::TabGroup(None, BundleOp::Color(c)))
            );
            assert_eq!(
                act(&chip_item_id(7, &format!("color_{}", c.key()))),
                Some(Action::TabGroup(Some(7), BundleOp::Color(c)))
            );
        }
        assert_eq!(
            act(&chip_item_id(7, "toggle")),
            Some(Action::TabGroup(Some(7), BundleOp::Toggle))
        );
        assert_eq!(
            act(&chip_item_id(12, "rename")),
            Some(Action::TabGroup(Some(12), BundleOp::Rename))
        );
        assert_eq!(
            act(&chip_item_id(3, "dissolve")),
            Some(Action::TabGroup(Some(3), BundleOp::Dissolve))
        );
        for bad in [
            "chip_x_toggle",
            "chip_7_nope",
            "chip_7",
            "chip__toggle",
            "chip_7_color_magenta",
            "tab_group_color_magenta",
            "tab_group_nope",
        ] {
            assert_eq!(act(bad), None, "{bad}");
        }
        // 기존 탭 색 항목은 그대로 간다
        assert_eq!(act("tab_color_blue"), Some(Action::SetTabColor(TabColor::Blue)));
    }
}
