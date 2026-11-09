//! 명령 팔레트(#77): 키 묶음과 같은 동작을 이름으로 찾아 부른다(Ghostty의 `toggle_command_palette`).
//! 동작 목록과 거르기는 창과 상관없는 순수 함수라 여기서 테스트한다

use crate::keymap::{Action, Keymap, Trigger, parse_action};

/// 팔레트 항목 하나
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// 보이는 이름
    pub label: &'static str,
    /// Ghostty 동작 이름(이것으로도 찾는다)
    pub name: &'static str,
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
    ("b-terminal 종료", "quit"),
];

/// 팔레트의 모든 항목(이름 차례)
pub fn entries() -> Vec<Entry> {
    let mut v: Vec<Entry> = ENTRIES
        .iter()
        .filter_map(|&(label, name)| {
            let action = parse_action(name).ok()??;
            Some(Entry {
                label,
                name,
                command: Command::Action(action),
            })
        })
        .collect();
    v.push(Entry {
        label: "설정 파일 열기",
        name: "open_config",
        command: Command::OpenSettings,
    });
    v
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
            } else if e.name.contains(&q) {
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
        assert_eq!(entries().len(), ENTRIES.len() + 1, "동작 글이 모두 풀린다");
    }

    #[test]
    fn filter_ranks_prefix_then_substring_then_name_then_scattered() {
        let e = entries();
        let labels = |q: &str| filter(&e, q).into_iter().map(|i| e[i].label).collect::<Vec<_>>();
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
}
