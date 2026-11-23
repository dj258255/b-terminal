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
    /// 이름 뒤에 흐리게 보이는 글(칸 이동 항목의 현재 폴더). 이것으로도 찾는다. 없으면 빈 글
    pub subtitle: String,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Action(Action),
    /// 설정 파일 열기(메뉴의 "설정…")
    OpenSettings,
    /// 그 칸(id)으로 간다(Ghostty `Focus: 제목` 항목, #690)
    Focus(u64),
}

/// 칸 이동 항목 수 상한. 칸이 수백 개여도 팔레트를 열고 글자를 칠 때마다 항목을 새로 만들고 거르므로 줄인다.
/// 100은 어림값이고 잰 값이 아니다(Ghostty는 상한이 없다). 넘으면 창·탭 차례로 앞의 칸만 남긴다
pub const MAX_FOCUS: usize = 100;

/// 칸 이동 항목을 만들 재료: 칸 하나의 제목들과 현재 폴더
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FocusPane {
    pub id: u64,
    /// 탭 제목 덮어쓰기(`set_tab_title`)나 창 제목 덮어쓰기. 없으면 None
    pub title_override: Option<String>,
    /// 칸의 제목(사람이 붙인 것 → 프로그램이 보낸 것). 비면 창 제목을 쓴다
    pub pane_title: String,
    pub window_title: String,
    /// 칸의 현재 폴더(OSC 7)
    pub cwd: Option<String>,
}

/// 칸 이동 항목의 제목(Ghostty `jumpOptions`): 덮어쓴 제목 → 칸 제목(비면 창 제목) → "Untitled". 앞에 "Focus: ".
/// 제목은 터미널 출력에서 온 글이라 `printable`을 지난다
pub fn focus_title(title_override: Option<&str>, pane_title: &str, window_title: &str) -> String {
    let clean = |t: &str| bt_core::proto::printable(t).trim().to_string();
    let over = title_override.map(clean).unwrap_or_default();
    let pane = clean(pane_title);
    let shown = if !over.is_empty() {
        over
    } else if !pane.is_empty() {
        pane
    } else {
        let win = clean(window_title);
        if win.is_empty() { "Untitled".to_string() } else { win }
    };
    format!("Focus: {shown}")
}

/// 부제: 줄인 현재 폴더(홈은 `~`). 제목에 이미 들어 있거나 폴더가 없으면 빈 글
pub fn focus_subtitle(title: &str, cwd: Option<&str>, home: Option<&str>) -> String {
    let Some(cwd) = cwd.map(bt_core::proto::printable).filter(|c| !c.trim().is_empty()) else {
        return String::new();
    };
    let short = match home.filter(|h| !h.is_empty() && *h != "/") {
        Some(h) if cwd == h => "~".to_string(),
        Some(h) => match cwd.strip_prefix(h) {
            Some(rest) if rest.starts_with('/') => format!("~{rest}"),
            _ => cwd,
        },
        None => cwd,
    };
    if title.contains(&short) { String::new() } else { short }
}

/// 칸마다 항목 하나(최대 `MAX_FOCUS`)
pub fn focus_entries(panes: &[FocusPane], home: Option<&str>) -> Vec<Entry> {
    panes
        .iter()
        .take(MAX_FOCUS)
        .map(|p| {
            let label = focus_title(p.title_override.as_deref(), &p.pane_title, &p.window_title);
            let subtitle = focus_subtitle(&label, p.cwd.as_deref(), home);
            Entry {
                label,
                name: "focus_pane".to_string(),
                description: String::new(),
                subtitle,
                command: Command::Focus(p.id),
            }
        })
        .collect()
}

/// 이름순으로 고른다(Ghostty `sortedTerminalPaletteOptions`): 대소문자를 가리지 않고 ":"를 탭 문자로 바꿔 견주어
/// "Foo:"가 "Foo Bar"보다 앞에 오게 한다. 같으면 들어온 차례를 지킨다
pub fn sort_entries(entries: &mut [Entry]) {
    entries.sort_by_cached_key(|e| e.label.to_lowercase().replace(':', "\t"));
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
    ("선택 붙여넣기", "paste_from_selection"),
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
    ("창 닫기", "close_window"),
    ("다음 창", "goto_window:next"),
    ("앞 창", "goto_window:previous"),
    ("탭을 새 창으로", "move_tab_to_new_window"),
    ("탭을 오른쪽으로 옮기기", "move_tab:1"),
    ("탭을 왼쪽으로 옮기기", "move_tab:-1"),
    ("창 크기 처음대로", "reset_window_size"),
    ("창 꾸밈 켜고 끄기", "toggle_window_decorations"),
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
                subtitle: String::new(),
                command: Command::Action(action),
            })
        }));
        v.push(Entry {
            label: "설정 파일 열기".to_string(),
            name: "open_config".to_string(),
            description: String::new(),
            subtitle: String::new(),
            command: Command::OpenSettings,
        });
    }
    v.extend(user.iter().filter_map(|u| {
        let action = parse_action(&u.action).ok()??;
        Some(Entry {
            label: u.title.clone(),
            name: u.action.clone(),
            description: u.description.clone(),
            subtitle: String::new(),
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
            } else if e.name.contains(&q)
                || e.description.to_lowercase().contains(&q)
                || e.subtitle.to_lowercase().contains(&q)
            {
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

/// 동작에 걸린 단축키를 보기 좋게(⌘⇧D). 여럿이면 가장 나중에 정한 것(Ghostty `getTrigger`), performable은 뺀다
pub fn shortcut(keymap: &Keymap, command: &Command) -> String {
    let Command::Action(action) = command else {
        return String::new();
    };
    keymap
        .triggers_for(action)
        .into_iter()
        .next()
        .map(format_trigger)
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

/// 친 글이 바뀐 뒤 고른 줄(Ghostty `CommandPalette.onChange(of: query)`): 글이 있으면 첫 줄, 다 지우면 고르지 않음
pub fn selection_after_edit(query: &str, _sel: Option<usize>) -> Option<usize> {
    (!query.is_empty()).then_some(0)
}

/// 위·아래 화살표(Ghostty `.move`): 고르지 않았으면 아래는 첫 줄, 위는 끝 줄. 끝에서 넘으면 반대쪽 끝으로 돈다
pub fn move_selection(sel: Option<usize>, len: usize, down: bool) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(match (sel, down) {
        (None, true) => 0,
        (None, false) => len - 1,
        (Some(i), true) if i + 1 >= len => 0,
        (Some(i), true) => i + 1,
        (Some(0), false) => len - 1,
        (Some(i), false) => (i - 1).min(len - 1),
    })
}

/// Enter로 실행할 줄. 고르지 않았으면 없다. 고른 줄이 목록보다 뒤면 끝 줄(Ghostty `selectedOption`)
pub fn picked(sel: Option<usize>, len: usize) -> Option<usize> {
    sel.filter(|_| len > 0).map(|i| i.min(len - 1))
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
    fn focus_subtitle_is_searchable() {
        let e = focus_entries(&[fp(1, None, "vim", "", Some("/work/app"))], None);
        assert_eq!(filter(&e, "work"), vec![0]);
        assert!(filter(&e, "nothing").is_empty());
    }

    #[test]
    fn description_is_searchable() {
        let user = [conf("Reset Font Style", "reset_font_size", "Restore Defaults")];
        let e = entries(&user, false);
        assert_eq!(filter(&e, "defaults"), vec![0]);
        assert_eq!(filter(&e, "reset"), vec![0]);
        assert!(filter(&e, "nothing").is_empty());
    }

    fn fp(id: u64, over: Option<&str>, pane: &str, win: &str, cwd: Option<&str>) -> FocusPane {
        FocusPane {
            id,
            title_override: over.map(str::to_string),
            pane_title: pane.into(),
            window_title: win.into(),
            cwd: cwd.map(str::to_string),
        }
    }

    #[test]
    fn focus_title_override_wins() {
        assert_eq!(focus_title(Some("내 탭"), "vim", "창"), "Focus: 내 탭");
        // 덮어쓴 제목이 비면 없는 것과 같다
        assert_eq!(focus_title(Some(""), "vim", "창"), "Focus: vim");
    }

    #[test]
    fn focus_title_falls_back_to_window_then_untitled() {
        assert_eq!(focus_title(None, "", "창 제목"), "Focus: 창 제목");
        assert_eq!(focus_title(None, "   ", "창 제목"), "Focus: 창 제목");
        assert_eq!(focus_title(None, "", ""), "Focus: Untitled");
    }

    #[test]
    fn focus_title_strips_untrusted_text() {
        // U+202E는 글 순서를 뒤집으니 지우고 제어 문자는 공백이 된다
        assert_eq!(focus_title(None, "ok\u{202e}gnp\x1b[31m", ""), "Focus: okgnp [31m");
        // 지우고 나서 비면 비어 있는 것으로 본다
        assert_eq!(focus_title(None, "\u{202e}", ""), "Focus: Untitled");
    }

    #[test]
    fn focus_subtitle_hidden_when_title_has_path() {
        let home = Some("/Users/me");
        assert_eq!(focus_subtitle("Focus: x", Some("/Users/me/src/app"), home), "~/src/app");
        assert_eq!(
            focus_subtitle("Focus: ~/src/app 안", Some("/Users/me/src/app"), home),
            ""
        );
        assert_eq!(focus_subtitle("Focus: x", None, home), "");
        assert_eq!(focus_subtitle("Focus: x", Some("/Users/me"), home), "~");
        // 홈 밖은 그대로, 홈과 앞부분만 같은 다른 폴더는 줄이지 않는다
        assert_eq!(focus_subtitle("Focus: x", Some("/Users/meow/a"), home), "/Users/meow/a");
        assert_eq!(focus_subtitle("Focus: x", Some("/etc"), home), "/etc");
    }

    #[test]
    fn focus_entries_one_per_pane_with_command() {
        let panes = [
            fp(7, None, "vim", "", Some("/Users/me/a")),
            fp(9, Some("빌드"), "zsh", "", None),
        ];
        let e = focus_entries(&panes, Some("/Users/me"));
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].label, "Focus: vim");
        assert_eq!(e[0].subtitle, "~/a");
        assert_eq!(e[0].command, Command::Focus(7));
        assert_eq!(e[1].label, "Focus: 빌드");
        assert_eq!(e[1].command, Command::Focus(9));
    }

    #[test]
    fn focus_entries_are_capped() {
        let panes: Vec<FocusPane> = (0..MAX_FOCUS as u64 + 50).map(|i| fp(i, None, "t", "", None)).collect();
        let e = focus_entries(&panes, None);
        assert_eq!(e.len(), MAX_FOCUS);
        assert_eq!(e[MAX_FOCUS - 1].command, Command::Focus(MAX_FOCUS as u64 - 1));
    }

    #[test]
    fn sort_puts_colon_before_space_and_mixes_commands() {
        let mk = |label: &str| Entry {
            label: label.into(),
            name: String::new(),
            description: String::new(),
            subtitle: String::new(),
            command: Command::OpenSettings,
        };
        let mut v = vec![mk("Foo Bar"), mk("foo:"), mk("Focus: b"), mk("새 탭"), mk("Focus: A")];
        sort_entries(&mut v);
        let l: Vec<&str> = v.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(l, vec!["Focus: A", "Focus: b", "foo:", "Foo Bar", "새 탭"]);
    }

    #[test]
    fn sort_is_stable_for_equal_titles() {
        let mk = |id: u64| Entry {
            label: "Focus: x".into(),
            name: String::new(),
            description: String::new(),
            subtitle: String::new(),
            command: Command::Focus(id),
        };
        let mut v = vec![mk(3), mk(1), mk(2)];
        sort_entries(&mut v);
        let ids: Vec<Command> = v.into_iter().map(|e| e.command).collect();
        assert_eq!(ids, vec![Command::Focus(3), Command::Focus(1), Command::Focus(2)]);
    }

    /// Ghostty처럼 처음에는 고르지 않고(Enter가 아무 일도 하지 않음), 글을 치면 첫 줄, 다 지우면 다시 고르지 않는다
    #[test]
    fn selection_follows_ghostty_palette() {
        assert_eq!(selection_after_edit("f", None), Some(0));
        assert_eq!(selection_after_edit("fo", Some(3)), Some(0));
        assert_eq!(selection_after_edit("", Some(0)), None);
        assert_eq!(selection_after_edit("", Some(2)), None);
        assert_eq!(move_selection(None, 5, true), Some(0));
        assert_eq!(move_selection(None, 5, false), Some(4));
        assert_eq!(move_selection(Some(4), 5, true), Some(0));
        assert_eq!(move_selection(Some(0), 5, false), Some(4));
        assert_eq!(move_selection(Some(1), 5, true), Some(2));
        assert_eq!(move_selection(Some(1), 0, true), None);
        assert_eq!(picked(None, 5), None);
        assert_eq!(picked(Some(2), 5), Some(2));
        assert_eq!(picked(Some(9), 5), Some(4));
        assert_eq!(picked(Some(0), 0), None);
    }
}
