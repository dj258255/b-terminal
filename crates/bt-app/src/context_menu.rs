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
}
