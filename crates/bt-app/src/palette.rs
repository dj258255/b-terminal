//! 명령 팔레트(#77): 키 묶음과 같은 동작을 이름으로 찾아 부른다(Ghostty의 `toggle_command_palette`).
//! 동작 목록과 거르기는 창과 상관없는 순수 함수라 여기서 테스트한다

use crate::keymap::{Action, Keymap, Trigger, parse_action};
use bt_core::PaletteEntryConf;

/// 팔레트 항목 하나
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// 보이는 이름
    pub label: String,
    /// Ghostty 동작 이름(이것으로도 찾는다)
    pub name: String,
    /// 사용자 항목의 설명(`description:`). 이것으로도 찾는다. 기본 항목은 빈 글
    pub description: String,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Action(Action),
    /// 설정 파일 열기(메뉴의 "설정…")
    OpenSettings,
}

/// (이름, Ghostty 동작) 목록. 동작 글은 키 묶음과 같은 해석기로 푼다
const ENTRIES: &[(&str, &str)] = &[
    ("새 창", "new_window"),
    ("새 탭", "new_tab"),
    ("오른쪽으로 나누기", "new_split:right"),
    ("아래로 나누기", "new_split:down"),
    ("칸 닫기", "close_surface"),
    ("탭 닫기", "close_tab"),
    ("다음 칸", "goto_split:next"),
    ("앞 칸", "goto_split:previous"),
    ("칸 크게 보기", "toggle_split_zoom"),
    ("칸 크기 고르게", "equalize_splits"),
    ("다음 탭", "next_tab"),
    ("앞 탭", "previous_tab"),
    ("찾기", "start_search"),
    ("복사", "copy_to_clipboard"),
    ("붙여넣기", "paste_from_clipboard"),
    ("모두 고르기", "select_all"),
    ("화면 지우기", "clear_screen"),
    ("글자 크게", "increase_font_size:1"),
    ("글자 작게", "decrease_font_size:1"),
    ("글자 크기 처음대로", "reset_font_size"),
    ("앞 프롬프트로", "jump_to_prompt:-1"),
    ("뒤 프롬프트로", "jump_to_prompt:1"),
    ("스크롤 맨 위로", "scroll_to_top"),
    ("스크롤 맨 아래로", "scroll_to_bottom"),
    ("설정 다시 읽기", "reload_config"),
    ("퀵 터미널", "toggle_quick_terminal"),
    ("전체 화면", "toggle_fullscreen"),
    ("창 최대화", "toggle_maximize"),
    ("창 항상 위", "toggle_window_float_on_top"),
    ("제목 바꾸기", "prompt_surface_title"),
    ("탭 제목 바꾸기", "prompt_tab_title"),
    ("창 제목 바꾸기", "prompt_window_title"),
    ("제목 복사", "copy_title_to_clipboard"),
    ("스크롤백을 파일로 열기", "write_scrollback_file:open"),
    ("화면을 파일로 열기", "write_screen_file:open"),
    ("스크롤백 파일 경로 복사", "write_scrollback_file:copy"),
    ("b-terminal 종료", "quit"),
];

/// 팔레트의 모든 항목: 기본 항목(`keep_defaults`가 참일 때) 뒤에 사용자 항목(`command-palette-entry`, #150)
/// 동작을 풀지 못하는 사용자 항목은 뺀다(경고는 `user_warnings`)
pub fn entries(user: &[PaletteEntryConf], keep_defaults: bool) -> Vec<Entry> {
    let mut v: Vec<Entry> = Vec::new();
    if keep_defaults {
        v.extend(ENTRIES.iter().filter_map(|&(label, name)| {
            let action = parse_action(name).ok()??;
            Some(Entry {
                label: label.to_string(),
                name: name.to_string(),
                description: String::new(),
                command: Command::Action(action),
            })
        }));
        v.push(Entry {
            label: "설정 파일 열기".to_string(),
            name: "open_config".to_string(),
            description: String::new(),
            command: Command::OpenSettings,
        });
    }
    v.extend(user.iter().filter_map(|u| {
        let action = parse_action(&u.action).ok()??;
        Some(Entry {
            label: u.title.clone(),
            name: u.action.clone(),
            description: u.description.clone(),
            command: Command::Action(action),
        })
    }));
    v
}

/// 사용자 항목 중 동작을 풀지 못하는 것의 경고(설정 경고 꼴)
pub fn user_warnings(user: &[PaletteEntryConf]) -> Vec<String> {
    user.iter()
        .filter_map(|u| match parse_action(&u.action) {
            Ok(Some(_)) => None,
            Ok(None) => Some(format!(
                "command-palette-entry `{}`: unbind는 팔레트 항목의 동작이 될 수 없습니다",
                u.title
            )),
            Err(why) => Some(format!(
                "command-palette-entry `{}`: 동작을 풀지 못했습니다: {why}",
                u.title
            )),
        })
        .collect()
}

/// 찾는 글에 맞는 항목들의 자리(점수 높은 차례). 빈 글이면 모두.
/// 점수: 이름이 글로 시작 > 이름에 글이 들어 있음 > 동작 이름에 들어 있음 > 글자들이 차례로 들어 있음(띄어 있어도)
pub fn filter(entries: &[Entry], query: &str) -> Vec<usize> {
    let q: String = query.trim().to_lowercase();
    if q.is_empty() {
        return (0..entries.len()).collect();
    }
    let qn: String = q.split_whitespace().collect();
    let mut scored: Vec<(u32, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            let label = e.label.to_lowercase();
            let compact: String = label.split_whitespace().collect();
            let score = if label.starts_with(&q) || compact.starts_with(&qn) {
                4
            } else if label.contains(&q) || compact.contains(&qn) {
                3
            } else if e.name.contains(&q) || e.description.to_lowercase().contains(&q) {
                2
            } else if subsequence(&compact, &qn) {
                1
            } else {
                return None;
            };
            Some((score, i))
        })
        .collect();
    // 점수가 같으면 원래 차례
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, i)| i).collect()
}

fn subsequence(hay: &str, needle: &str) -> bool {
    let mut it = hay.chars();
    needle.chars().all(|c| it.any(|h| h == c))
}

/// 동작에 걸린 단축키를 보기 좋게(⌘⇧D). 여럿이면 짧은 것
pub fn shortcut(keymap: &Keymap, command: &Command) -> String {
    let Command::Action(action) = command else {
        return String::new();
    };
    keymap
        .triggers_for(action)
        .into_iter()
        .map(format_trigger)
        .min_by_key(|s| (s.chars().count(), s.clone()))
        .unwrap_or_default()
}

fn format_trigger(t: &Trigger) -> String {
    let mut s = String::new();
    if t.mods.ctrl {
        s.push('⌃');
    }
    if t.mods.alt {
        s.push('⌥');
    }
    if t.mods.shift {
        s.push('⇧');
    }
    if t.mods.sup {
        s.push('⌘');
    }
    let key = match t.key.as_str() {
        "arrow_up" => "↑".to_string(),
        "arrow_down" => "↓".to_string(),
        "arrow_left" => "←".to_string(),
        "arrow_right" => "→".to_string(),
        "enter" => "↩".to_string(),
        "equal" => "=".to_string(),
        "minus" => "-".to_string(),
        "comma" => ",".to_string(),
        "bracket_left" => "[".to_string(),
        "bracket_right" => "]".to_string(),
        "page_up" => "PgUp".to_string(),
        "page_down" => "PgDn".to_string(),
        k if k.starts_with("digit_") => k["digit_".len()..].to_string(),
        k => k.to_uppercase(),
    };
    s.push_str(&key);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_entry_parses_to_a_command() {
        assert_eq!(entries(&[], true).len(), ENTRIES.len() + 1, "동작 글이 모두 풀린다");
    }

    #[test]
    fn filter_ranks_prefix_then_substring_then_name_then_scattered() {
        let e = entries(&[], true);
        let labels = |q: &str| {
            filter(&e, q)
                .into_iter()
                .map(|i| e[i].label.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(labels("").len(), e.len());
        // "나누" → 오른쪽으로·아래로 나누기(들어 있음)
        assert_eq!(labels("나누"), vec!["오른쪽으로 나누기", "아래로 나누기"]);
        // "칸"으로 시작하는 것이 먼저, 들어 있는 것이 뒤
        let k = labels("칸");
        assert_eq!(&k[..3], &["칸 닫기", "칸 크게 보기", "칸 크기 고르게"]);
        assert!(k.contains(&"다음 칸"));
        // 동작 이름으로도 찾는다
        assert_eq!(labels("zoom"), vec!["칸 크게 보기"]);
        // 띄어 쓴 글자들도 차례로 맞으면
        assert_eq!(labels("글크"), vec!["글자 크게", "글자 크기 처음대로"]);
        assert!(labels("없는명령").is_empty());
    }

    #[test]
    fn shortcuts_come_from_the_keymap() {
        let (km, _) = Keymap::build(&["super+shift+x=toggle_split_zoom".into()]);
        let zoom = Command::Action(Action::ToggleSplitZoom);
        if cfg!(target_os = "macos") {
            assert_eq!(
                shortcut(
                    &km,
                    &Command::Action(Action::NewSplit(Some(crate::split::Direction::Right)))
                ),
                "⌘D"
            );
            // 기본(⇧⌘↩)과 사용자 것(⇧⌘X)이 같은 길이면 글자 차례로 하나를 늘 같게 고른다
            assert_eq!(shortcut(&km, &zoom), "⇧⌘X");
        }
        assert_eq!(shortcut(&km, &Command::OpenSettings), "");
    }

    fn conf(title: &str, action: &str, description: &str) -> PaletteEntryConf {
        PaletteEntryConf {
            title: title.into(),
            action: action.into(),
            description: description.into(),
        }
    }

    #[test]
    fn user_entries_follow_the_defaults() {
        let user = [conf("내 탭", "new_tab", "")];
        let e = entries(&user, true);
        assert_eq!(e.len(), ENTRIES.len() + 2);
        let last = e.last().unwrap();
        assert_eq!(last.label, "내 탭");
        assert_eq!(last.command, Command::Action(Action::NewTab));
    }

    #[test]
    fn cleared_defaults_leave_only_user_entries() {
        let user = [conf("내 탭", "new_tab", ""), conf("나가기", "quit", "")];
        let e = entries(&user, false);
        assert_eq!(
            e.iter().map(|x| x.label.as_str()).collect::<Vec<_>>(),
            vec!["내 탭", "나가기"]
        );
        assert!(entries(&[], false).is_empty());
    }

    #[test]
    fn unparsable_user_entries_warn_and_are_skipped() {
        let user = [
            conf("좋음", "new_tab", ""),
            conf("나쁨", "no_such_action", ""),
            conf("풀기", "unbind", ""),
        ];
        assert_eq!(entries(&user, false).len(), 1);
        let w = user_warnings(&user);
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(w[0].contains("나쁨"));
        assert!(w[1].contains("풀기"));
    }

    #[test]
    fn description_is_searchable() {
        let user = [conf("Reset Font Style", "reset_font_size", "Restore Defaults")];
        let e = entries(&user, false);
        assert_eq!(filter(&e, "defaults"), vec![0]);
        assert_eq!(filter(&e, "reset"), vec![0]);
        assert!(filter(&e, "nothing").is_empty());
    }
}
