//! macOS 메뉴 막대(#34). 항목을 누르면 키 묶음과 같은 동작을 부른다.
//! 우리 항목에는 단축키를 걸지 않는다: 메뉴가 키를 먼저 가져가면 설정의 keybind로 바꿀 수 없다

use crate::keymap::{Action, SplitTarget};
use crate::split::Direction;

/// 메뉴 항목 id → 동작. "settings"는 설정 파일 열기
pub fn action_for(id: &str) -> Option<MenuCommand> {
    Some(match id {
        "settings" => MenuCommand::OpenSettings,
        "reload_config" => MenuCommand::Action(Action::ReloadConfig),
        "new_tab" => MenuCommand::Action(Action::NewTab),
        "new_window" => MenuCommand::Action(Action::NewWindow),
        "close_surface" => MenuCommand::Action(Action::CloseSurface),
        "close_tab" => MenuCommand::Action(Action::CloseTab),
        "split_right" => MenuCommand::Action(Action::NewSplit(Some(Direction::Right))),
        "split_down" => MenuCommand::Action(Action::NewSplit(Some(Direction::Down))),
        "split_prev" => MenuCommand::Action(Action::GotoSplit(SplitTarget::Previous)),
        "split_next" => MenuCommand::Action(Action::GotoSplit(SplitTarget::Next)),
        "split_zoom" => MenuCommand::Action(Action::ToggleSplitZoom),
        "split_equalize" => MenuCommand::Action(Action::EqualizeSplits),
        "quick_terminal" => MenuCommand::Action(Action::ToggleQuickTerminal),
        "surface_title" => MenuCommand::Action(Action::PromptSurfaceTitle),
        "tab_title" => MenuCommand::Action(Action::PromptTabTitle),
        "window_title" => MenuCommand::Action(Action::PromptWindowTitle),
        "copy" => MenuCommand::Action(Action::Copy(bt_term::CopyFormat::Mixed)),
        "paste" => MenuCommand::Action(Action::Paste),
        "paste_selection" => MenuCommand::Action(Action::PasteSelection),
        "select_all" => MenuCommand::Action(Action::SelectAll),
        "clear_screen" => MenuCommand::Action(Action::ClearScreen),
        "bigger" => MenuCommand::Action(Action::FontBigger(1.0)),
        "smaller" => MenuCommand::Action(Action::FontSmaller(1.0)),
        "reset_font" => MenuCommand::Action(Action::FontReset),
        "prompt_up" => MenuCommand::Action(Action::JumpToPrompt(-1)),
        "prompt_down" => MenuCommand::Action(Action::JumpToPrompt(1)),
        "next_tab" => MenuCommand::Action(Action::NextTab),
        "prev_tab" => MenuCommand::Action(Action::PrevTab),
        "quit" => MenuCommand::Action(Action::Quit),
        // 앱 전체의 보안 키보드 입력(#258). 키 묶음 `toggle_secure_input`(초점 칸 하나)과 다르다
        "secure_input" => MenuCommand::ToggleSecureKeyboardEntry,
        // 오른쪽 클릭 메뉴(#278)만 쓰는 항목
        "split_left" => MenuCommand::Action(Action::NewSplit(Some(Direction::Left))),
        "split_up" => MenuCommand::Action(Action::NewSplit(Some(Direction::Up))),
        "reset_terminal" => MenuCommand::Action(Action::Reset),
        "toggle_readonly" => MenuCommand::Action(Action::ToggleReadonly),
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuCommand {
    Action(Action),
    OpenSettings,
    /// 앱 메뉴의 보안 키보드 입력(앱 전체로 켜고 끈다)
    ToggleSecureKeyboardEntry,
}

#[cfg(target_os = "macos")]
/// 메뉴와, 체크 표시를 맞추려고 따로 들고 있을 보안 키보드 입력 항목
pub fn build(secure_on: bool) -> (muda::Menu, muda::CheckMenuItem) {
    use muda::{AboutMetadata, CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
    let secure = CheckMenuItem::with_id("secure_input", "보안 키보드 입력", true, secure_on, None);
    let item = |id: &str, text: &str| MenuItem::with_id(id, text, true, None);
    let menu = Menu::new();
    let about = AboutMetadata {
        name: Some("b-terminal".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        comments: Some("AI 에이전트 시대를 위한 나만의 Rust 터미널".into()),
        ..Default::default()
    };
    let app = Submenu::with_items(
        "b-terminal",
        true,
        &[
            &PredefinedMenuItem::about(Some("b-terminal에 관하여"), Some(about)),
            &PredefinedMenuItem::separator(),
            &item("settings", "설정…"),
            &item("reload_config", "설정 다시 읽기"),
            &secure,
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(Some("서비스")),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(Some("b-terminal 가리기")),
            &PredefinedMenuItem::hide_others(Some("기타 가리기")),
            &PredefinedMenuItem::show_all(Some("모두 보기")),
            &PredefinedMenuItem::separator(),
            // 미리 정한 종료 항목은 NSApp이 바로 끝내 닫기 확인을 건너뛴다. 우리 quit 동작으로(⌘Q는 키 묶음)
            &item("quit", "b-terminal 종료"),
        ],
    );
    let shell = Submenu::with_items(
        "셸",
        true,
        &[
            &item("new_window", "새 창"),
            &item("new_tab", "새 탭"),
            &item("split_right", "오른쪽으로 나누기"),
            &item("split_down", "아래로 나누기"),
            &PredefinedMenuItem::separator(),
            &item("close_surface", "닫기"),
            &item("close_tab", "탭 닫기"),
            &PredefinedMenuItem::separator(),
            &item("surface_title", "제목 바꾸기…"),
            &item("tab_title", "탭 제목 바꾸기…"),
            &item("window_title", "창 제목 바꾸기…"),
        ],
    );
    let edit = Submenu::with_items(
        "편집",
        true,
        &[
            &item("copy", "복사"),
            &item("paste", "붙여넣기"),
            &item("paste_selection", "선택 붙여넣기"),
            &item("select_all", "모두 고르기"),
            &PredefinedMenuItem::separator(),
            &item("clear_screen", "화면 지우기"),
        ],
    );
    let view = Submenu::with_items(
        "보기",
        true,
        &[
            &item("bigger", "글자 크게"),
            &item("smaller", "글자 작게"),
            &item("reset_font", "글자 크기 처음대로"),
            &PredefinedMenuItem::separator(),
            &item("prompt_up", "앞 프롬프트로"),
            &item("prompt_down", "뒤 프롬프트로"),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::fullscreen(Some("전체 화면")),
        ],
    );
    let window = Submenu::with_items(
        "윈도우",
        true,
        &[
            &PredefinedMenuItem::minimize(Some("최소화")),
            &PredefinedMenuItem::separator(),
            &item("next_tab", "다음 탭"),
            &item("prev_tab", "앞 탭"),
            &PredefinedMenuItem::separator(),
            &item("split_next", "다음 칸"),
            &item("split_prev", "앞 칸"),
            &item("split_zoom", "칸 크게 보기"),
            &item("split_equalize", "칸 크기 고르게"),
            &PredefinedMenuItem::separator(),
            &item("quick_terminal", "퀵 터미널"),
        ],
    );
    for sub in [&app, &shell, &edit, &view, &window] {
        let _ = sub.as_ref().map(|s| menu.append(s));
    }
    (menu, secure)
}

/// 설정 파일이 없으면 예시가 담긴 파일을 만들고 그 경로를 돌려준다
pub fn ensure_config_file() -> std::io::Result<std::path::PathBuf> {
    let path = bt_core::config::bterminal_config_path();
    if !path.exists() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&path, TEMPLATE)?;
    }
    Ok(path)
}

const TEMPLATE: &str = "# b-terminal 설정(Ghostty와 같은 문법). 고친 뒤 Cmd-Shift-, 로 다시 읽습니다.
# 지원하는 키: https://github.com/dj258255/b-terminal/blob/main/docs/configuration.md
# 테마 목록: b-terminal --list-themes

# theme = Catppuccin Mocha
# font-family = JetBrains Mono
# font-size = 13
# cursor-style = block
# window-padding-x = 4
# window-padding-y = 4
# macos-option-as-alt = true
# keybind = super+shift+n=new_tab
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_ids_map_to_actions() {
        assert_eq!(action_for("new_tab"), Some(MenuCommand::Action(Action::NewTab)));
        assert_eq!(action_for("settings"), Some(MenuCommand::OpenSettings));
        assert_eq!(
            action_for("split_down"),
            Some(MenuCommand::Action(Action::NewSplit(Some(Direction::Down))))
        );
        assert_eq!(action_for("quit"), Some(MenuCommand::Action(Action::Quit)));
        assert_eq!(action_for("secure_input"), Some(MenuCommand::ToggleSecureKeyboardEntry));
        assert_eq!(
            action_for("paste_selection"),
            Some(MenuCommand::Action(Action::PasteSelection))
        );
        assert_eq!(action_for("nope"), None);
    }
}
