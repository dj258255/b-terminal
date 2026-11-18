//! AppleScript 명령과 상태 스냅숏(#634, Ghostty `macos-applescript`).
//!
//! 앱 묶음의 Apple Event는 vendor winit(`platform::macos::scripting`)이 받아 이 모듈의 `on_script`로 넘긴다.
//! 창·탭·칸 목록은 여기서 만든 스냅숏을 vendor가 읽고, 바꾸는 명령(새 창·새 탭·포커스·글 넣기)은 btctl과 같은 길
//! (`handle_call`, `paste`의 붙여넣기 보호)로 처리한다. 순수 함수(스냅숏 만들기·인자 판단)는 아래 시험이 덮는다.
//!
//! ids: 칸은 `btctl list`의 번호 그대로, 탭은 그 탭의 첫 칸 번호(칸이 닫히거나 탭 첫 칸이 바뀌면 바뀐다),
//! 창은 창 번호(winit `WindowId`)다. 모두 10진수 글이다.

use std::sync::mpsc::Sender;

use winit::event_loop::ActiveEventLoop;
use winit::platform::macos::{
    ScriptCommand, ScriptReply, ScriptSnapshot, ScriptTab, ScriptTarget, ScriptTerminal, ScriptWindow,
};

use super::{App, UserEvent};

/// 한 번에 넣을 글의 상한(바이트). 제어 소켓 요청 한 줄의 상한(1MiB)과 같다
const MAX_TEXT: usize = 1 << 20;

/// 스냅숏을 만들 재료: 칸
pub struct PaneView {
    pub id: u64,
    pub title: String,
    pub cwd: Option<String>,
}

/// 스냅숏을 만들 재료: 탭(칸은 왼쪽·위부터). `title`은 덮어쓴 제목이나 초점 칸 제목
pub struct TabView {
    pub title: String,
    pub focus: u64,
    pub panes: Vec<PaneView>,
}

/// 스냅숏을 만들 재료: 창. `serial`은 창을 만든 차례, `active`는 보고 있는 탭의 자리
pub struct WinView {
    pub id: u64,
    pub serial: u64,
    pub active: usize,
    pub tabs: Vec<TabView>,
}

/// 창을 만든 차례로, 탭 줄 순서로, 탭 안에서는 왼쪽·위부터 스냅숏을 만든다. `front`는 마지막으로 보던 창
pub fn build_snapshot(windows: &[WinView], front: Option<u64>) -> ScriptSnapshot {
    let mut sorted: Vec<&WinView> = windows.iter().collect();
    sorted.sort_by_key(|w| w.serial);
    let windows: Vec<ScriptWindow> = sorted
        .into_iter()
        .map(|w| {
            let tabs: Vec<ScriptTab> = w
                .tabs
                .iter()
                .enumerate()
                .filter_map(|(i, t)| {
                    let first = t.panes.first()?;
                    Some(ScriptTab {
                        id: first.id.to_string(),
                        title: t.title.clone(),
                        selected: i == w.active,
                        focused_terminal: Some(t.focus.to_string()),
                        terminals: t
                            .panes
                            .iter()
                            .map(|p| ScriptTerminal {
                                id: p.id.to_string(),
                                title: p.title.clone(),
                                working_directory: p.cwd.clone(),
                            })
                            .collect(),
                    })
                })
                .collect();
            let selected = tabs.iter().find(|t| t.selected);
            ScriptWindow {
                id: w.id.to_string(),
                title: selected.map(|t| t.title.clone()).unwrap_or_default(),
                selected_tab: selected.map(|t| t.id.clone()),
                tabs,
            }
        })
        .collect();
    let front_window = front
        .map(|f| f.to_string())
        .filter(|f| windows.iter().any(|w| &w.id == f));
    ScriptSnapshot { windows, front_window }
}

/// 스크립트가 준 id(10진수 글)를 번호로. 그 밖의 글은 오류
pub fn parse_id(id: &str) -> Result<u64, String> {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("id가 숫자가 아닙니다: {id:?}"));
    }
    id.parse().map_err(|_| format!("id가 너무 큽니다: {id:?}"))
}

/// 넣을 글의 크기를 읽기 전에 건다
pub fn check_text(text: &str) -> Result<(), String> {
    if text.len() > MAX_TEXT {
        return Err(format!("글이 너무 깁니다({} 바이트, 상한 {MAX_TEXT})", text.len()));
    }
    Ok(())
}

/// 명령을 받아들일지 정한다. `macos-applescript = false`면 모두 거절한다(vendor도 같은 값으로 막지만 설정은 앱이 쥔다)
pub fn admit(enabled: bool, cmd: ScriptCommand) -> Result<ScriptCommand, String> {
    if !enabled {
        return Err("AppleScript는 설정 macos-applescript = false로 꺼져 있습니다".into());
    }
    if let ScriptCommand::InputText { text, .. } = &cmd {
        check_text(text)?;
    }
    Ok(cmd)
}

impl App {
    /// vendor가 넘긴 명령 하나를 처리하고 답을 보낸다
    pub(super) fn on_script(&mut self, event_loop: &ActiveEventLoop, cmd: ScriptCommand, reply: Sender<ScriptReply>) {
        let result = admit(self.config.macos_applescript, cmd).and_then(|cmd| self.run_script(event_loop, cmd));
        // 읽기 쪽이 바뀐 상태를 보게 답보다 먼저 스냅숏을 넘긴다
        winit::platform::macos::set_scripting_snapshot(self.script_snapshot());
        let _ = reply.send(result);
    }

    /// 설정을 다시 읽은 뒤 vendor에 켜짐 여부를 알린다
    pub(super) fn sync_scripting_enabled(&self) {
        winit::platform::macos::set_scripting_enabled(self.config.macos_applescript);
    }

    fn script_snapshot(&self) -> ScriptSnapshot {
        let windows: Vec<WinView> = std::iter::once(&self.w)
            .chain(&self.others)
            .filter_map(|w| {
                let id = u64::from(w.id()?);
                let tabs = w
                    .tabs
                    .iter()
                    .map(|g| {
                        let panes: Vec<PaneView> = g
                            .layout
                            .leaves()
                            .into_iter()
                            .filter_map(|pid| self.pane_index(pid))
                            .map(|i| &self.panes[i])
                            .map(|p| PaneView {
                                id: p.id,
                                title: p.display_title(),
                                cwd: p.cwd.as_ref().map(|c| c.to_string_lossy().into_owned()),
                            })
                            .collect();
                        let title = g.title.clone().unwrap_or_else(|| {
                            panes
                                .iter()
                                .find(|p| p.id == g.focus)
                                .map(|p| p.title.clone())
                                .unwrap_or_default()
                        });
                        TabView {
                            title,
                            focus: g.focus,
                            panes,
                        }
                    })
                    .collect();
                Some(WinView {
                    id,
                    serial: w.serial,
                    active: w.active,
                    tabs,
                })
            })
            .collect();
        build_snapshot(&windows, self.last_focused.map(u64::from))
    }

    /// 창 번호로 창을 찾아 그 창의 아무 칸(없으면 오류)을 돌려준다
    fn script_window_pane(&self, window: &str) -> Result<u64, String> {
        let id = parse_id(window)?;
        std::iter::once(&self.w)
            .chain(&self.others)
            .find(|w| w.id().map(u64::from) == Some(id))
            .and_then(|w| w.tabs.get(w.active).or(w.tabs.first()).map(|g| g.focus))
            .ok_or_else(|| format!("창 {window}이 없습니다"))
    }

    /// 칸이 든 창의 번호
    fn script_window_of(&self, pane: u64) -> Option<u64> {
        std::iter::once(&self.w)
            .chain(&self.others)
            .find(|w| w.holds(pane))
            .and_then(|w| w.id())
            .map(u64::from)
    }

    fn run_script(&mut self, event_loop: &ActiveEventLoop, cmd: ScriptCommand) -> ScriptReply {
        use bt_core::proto::{Call, Request};
        // btctl `new`와 같은 길: 만든 칸의 번호가 답에 들어 있다
        let new_pane = |app: &mut App, tab: Option<u64>, window: bool| -> Result<u64, String> {
            let call = Call::New {
                cwd: None,
                command: Vec::new(),
                split: None,
                tab,
                window,
            };
            let resp = app.handle_call(Request { id: 0, call }, event_loop);
            if !resp.ok {
                return Err(resp.error.unwrap_or_else(|| "열지 못했습니다".into()));
            }
            resp.result
                .get("tab")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| "열린 칸을 알 수 없습니다".to_string())
        };
        match cmd {
            ScriptCommand::Snapshot => Ok(None),
            ScriptCommand::NewWindow => {
                let pane = new_pane(self, None, true)?;
                let window = self.script_window_of(pane).ok_or("새 창을 찾지 못했습니다")?;
                Ok(Some(ScriptTarget::Window(window.to_string())))
            }
            ScriptCommand::NewTab { window } => {
                let at = match window {
                    Some(w) => Some(self.script_window_pane(&w)?),
                    None => None,
                };
                let pane = new_pane(self, at, false)?;
                // 새 탭의 첫 칸이 그 탭의 id다
                Ok(Some(ScriptTarget::Tab(pane.to_string())))
            }
            ScriptCommand::Focus { terminal } => {
                let pane = parse_id(&terminal)?;
                self.pane_index(pane)
                    .ok_or_else(|| format!("터미널 {terminal}이 없습니다"))?;
                self.on_note_clicked(event_loop, pane);
                Ok(None)
            }
            ScriptCommand::ActivateWindow { window } => {
                let pane = self.script_window_pane(&window)?;
                self.on_note_clicked(event_loop, pane);
                Ok(None)
            }
            ScriptCommand::SelectTab { tab } => {
                let first = parse_id(&tab)?;
                self.switch_to_pane(first);
                let focus = self
                    .w
                    .tabs
                    .iter()
                    .find(|g| g.layout.leaves().first() == Some(&first))
                    .map(|g| g.focus)
                    .ok_or_else(|| format!("탭 {tab}이 없습니다"))?;
                self.focus_pane(focus);
                Ok(None)
            }
            ScriptCommand::InputText { terminal, text } => {
                let pane = parse_id(&terminal)?;
                let i = self
                    .pane_index(pane)
                    .ok_or_else(|| format!("터미널 {terminal}이 없습니다"))?;
                // btctl send와 같은 거절: 읽기 전용 칸에는 넣지 않는다
                if self.panes[i].session.readonly() {
                    return Err(super::READONLY_REFUSAL.into());
                }
                if text.is_empty() {
                    return Ok(None);
                }
                self.switch_to_pane(pane);
                if self.w.confirm.is_some() {
                    return Err("그 창에 확인할 일이 있어 글을 넣지 않았습니다".into());
                }
                // 사람이 붙여넣을 때와 같은 길: 괄호 붙여넣기 틀, `ESC[201~` 지우기, 붙여넣기 보호
                if self.paste_to(pane, text) {
                    Ok(None)
                } else {
                    Err(
                        "붙여넣기 보호가 확인을 요청했습니다. 사용자가 허용하면 들어갑니다(여러 줄이거나 위험한 글)"
                            .into(),
                    )
                }
            }
        }
    }
}

/// vendor에 명령 처리 함수를 등록한다. 함수는 주 스레드에서 불리므로 이벤트 루프에 일을 넘기기만 한다
pub fn install(proxy: winit::event_loop::EventLoopProxy<UserEvent>, enabled: bool) {
    winit::platform::macos::set_scripting_enabled(enabled);
    winit::platform::macos::set_scripting_handler(Box::new(move |cmd, reply| {
        if let Err(e) = proxy.send_event(UserEvent::Script(cmd, reply.clone())) {
            let _ = reply.send(Err(format!("터미널이 끝나는 중입니다: {e}")));
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane(id: u64, title: &str) -> PaneView {
        PaneView {
            id,
            title: title.into(),
            cwd: Some(format!("/work/{id}")),
        }
    }

    fn two_windows() -> Vec<WinView> {
        vec![
            // 나중에 만든 창이 앞에 와도(바꿔 끼우면 순서가 섞인다) 만든 차례로 나온다
            WinView {
                id: 22,
                serial: 2,
                active: 0,
                tabs: vec![TabView {
                    title: "b".into(),
                    focus: 5,
                    panes: vec![pane(5, "sh")],
                }],
            },
            WinView {
                id: 11,
                serial: 1,
                active: 1,
                tabs: vec![
                    TabView {
                        title: "a".into(),
                        focus: 1,
                        panes: vec![pane(1, "vim")],
                    },
                    TabView {
                        title: "split".into(),
                        focus: 4,
                        panes: vec![pane(3, "top"), pane(4, "zsh")],
                    },
                ],
            },
        ]
    }

    #[test]
    fn snapshot_follows_creation_order_and_names_tabs_by_first_pane() {
        let s = build_snapshot(&two_windows(), Some(22));
        let ids: Vec<&str> = s.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["11", "22"]);
        assert_eq!(s.front_window.as_deref(), Some("22"));
        let w = &s.windows[0];
        let tab_ids: Vec<&str> = w.tabs.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(tab_ids, ["1", "3"], "탭의 id는 첫 칸의 번호");
        assert_eq!(w.selected_tab.as_deref(), Some("3"));
        assert!(!w.tabs[0].selected && w.tabs[1].selected);
        assert_eq!(w.tabs[1].focused_terminal.as_deref(), Some("4"));
        assert_eq!(w.tabs[1].terminals.len(), 2);
        assert_eq!(w.tabs[1].terminals[0].working_directory.as_deref(), Some("/work/3"));
        assert_eq!(w.title, "split", "창 제목은 보고 있는 탭의 제목");
    }

    #[test]
    fn snapshot_without_front_window_has_none() {
        assert_eq!(build_snapshot(&two_windows(), None).front_window, None);
        assert_eq!(
            build_snapshot(&two_windows(), Some(99)).front_window,
            None,
            "모르는 창은 앞 창이 아니다"
        );
        assert!(build_snapshot(&[], None).windows.is_empty());
    }

    #[test]
    fn ids_must_be_plain_numbers() {
        assert_eq!(parse_id("42"), Ok(42));
        for bad in ["", "-1", "4 2", "0x2", "abc", "1;2", "99999999999999999999999"] {
            assert!(parse_id(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn input_text_is_capped_before_it_is_used() {
        assert!(check_text("ls -l\n").is_ok());
        assert!(check_text("").is_ok(), "빈 글은 아무 일도 하지 않을 뿐 오류가 아니다");
        assert!(check_text(&"x".repeat(MAX_TEXT)).is_ok());
        assert!(check_text(&"x".repeat(MAX_TEXT + 1)).is_err());
    }

    #[test]
    fn disabled_scripting_refuses_every_command() {
        use winit::platform::macos::ScriptCommand as C;
        let cmds = [
            C::Snapshot,
            C::NewWindow,
            C::NewTab { window: None },
            C::Focus { terminal: "1".into() },
            C::ActivateWindow { window: "1".into() },
            C::SelectTab { tab: "1".into() },
            C::InputText {
                terminal: "1".into(),
                text: "x".into(),
            },
        ];
        for c in &cmds {
            assert!(admit(false, c.clone()).is_err(), "{c:?}");
            assert_eq!(admit(true, c.clone()).as_ref(), Ok(c), "{c:?}");
        }
        let big = C::InputText {
            terminal: "1".into(),
            text: "x".repeat(MAX_TEXT + 1),
        };
        assert!(admit(true, big).is_err(), "켜져 있어도 너무 긴 글은 거절");
    }
}
