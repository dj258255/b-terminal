//! AppleScript 명령과 상태 스냅숏(#634, Ghostty `macos-applescript`).
//!
//! 앱 묶음의 Apple Event는 vendor winit(`platform::macos::scripting`)이 받아 이 모듈의 `on_script`로 넘긴다.
//! 창·탭·칸 목록은 여기서 만든 스냅숏을 vendor가 읽고, 바꾸는 명령(새 창·새 탭·포커스·글 넣기)은 btctl과 같은 길
//! (`handle_call`, `paste`의 붙여넣기 보호)로 처리한다. 순수 함수(스냅숏 만들기·인자 판단)는 아래 시험이 덮는다.
//!
//! `send key`(#650)는 키 이름을 `script_key`가 사람이 누른 키와 같은 입력으로 만들고, 같은 인코더와 키 묶음 길(`feed_key`)로 보낸다.
//! `split`은 `btctl new --split`과 같은 길(`handle_call`)이고, `close`·`close tab`·`close window`는 확인 막대 없이 닫는다(Ghostty처럼).
//!
//! `perform action`(#658)은 동작 글을 키 묶음과 같은 해석기(`parse_script_action`)로 읽어 그 칸을 고른 칸으로 만든 뒤 `handle_action`으로 하고,
//! 지금 할 수 없는 동작(`can_perform`)은 거짓을 돌려준다. 거절은 `send key`와 같다.
//!
//! ids: 칸은 `btctl list`의 번호 그대로, 탭은 그 탭의 첫 칸 번호(칸이 닫히거나 탭 첫 칸이 바뀌면 바뀐다),
//! 창은 창 번호(winit `WindowId`)다. 모두 10진수 글이다.

use std::sync::mpsc::Sender;

use winit::event_loop::ActiveEventLoop;
use winit::platform::macos::{
    ScriptCommand, ScriptReply, ScriptSnapshot, ScriptSplit, ScriptTab, ScriptTarget, ScriptTerminal, ScriptTicket,
    ScriptWindow,
};

use super::{App, UserEvent};

/// 한 번에 넣을 글의 상한(바이트). 제어 소켓 요청 한 줄의 상한(1MiB)과 같다
const MAX_TEXT: usize = 1 << 20;

/// `send key`의 키 이름·수식 키 글의 상한(바이트). 가장 긴 키 이름은 20바이트다
const MAX_KEY: usize = 256;

/// `perform action`의 동작 글의 상한(바이트). 가장 긴 보통 동작(`text:`로 몇 줄)도 이보다 짧다. 더 긴 글은 `input text`로 넣는다
const MAX_ACTION: usize = 4096;

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

/// 스냅숏의 앞 창: 마지막으로 포커스를 받은 창이 아직 있으면 그 창, 없으면 지금 창(명령이 향하는 창)
///
/// 앱이 뒤에 있을 때 스크립트로 앞 창을 닫으면 남은 창이 포커스를 받지 못해 마지막 포커스가 닫힌 창을 가리킨다.
/// 그래도 창이 남아 있으면 Ghostty(`NSApp.orderedWindows`의 첫 창)처럼 `front window`가 있어야 한다
pub fn front_window_id(last_focused: Option<u64>, current: Option<u64>, ids: &[u64]) -> Option<u64> {
    let alive = |id: &u64| ids.contains(id);
    last_focused.filter(alive).or(current.filter(alive))
}

/// 앞 창을 먼저, 나머지 창은 만든 차례로, 탭 줄 순서로, 탭 안에서는 왼쪽·위부터 스냅숏을 만든다. `front`는 마지막으로 보던 창
pub fn build_snapshot(windows: &[WinView], front: Option<u64>) -> ScriptSnapshot {
    // Ghostty처럼 앞에서 뒤 차례: 앞 창이 먼저(`window 1`), 나머지는 만든 차례(#645). 앞뒤 전체 순서는 기록하지 않는다
    let mut sorted: Vec<&WinView> = windows.iter().collect();
    sorted.sort_by_key(|w| (Some(w.id) != front, w.serial));
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
    match &cmd {
        ScriptCommand::InputText { text, .. } => check_text(text)?,
        ScriptCommand::SendKey { key, modifiers, .. }
            if key.len() > MAX_KEY || modifiers.as_ref().is_some_and(|m| m.len() > MAX_KEY) =>
        {
            return Err(format!("키 이름이나 수식 키가 너무 깁니다(상한 {MAX_KEY} 바이트)"));
        }
        ScriptCommand::PerformAction { action, .. } if action.len() > MAX_ACTION => {
            return Err(format!(
                "동작 문자열이 너무 깁니다({} 바이트, 상한 {MAX_ACTION})",
                action.len()
            ));
        }
        _ => {}
    }
    Ok(cmd)
}

/// 칸을 나누는 요청(`btctl new --split 방향 --tab 칸`과 같다)
pub fn split_call(pane: u64, direction: ScriptSplit) -> bt_core::proto::Call {
    bt_core::proto::Call::New {
        cwd: None,
        command: Vec::new(),
        split: Some(direction.as_str().into()),
        tab: Some(pane),
        window: false,
    }
}

/// 창에 막대(확인·검색·팔레트·제목)가 떠 있으면 키를 보내지 않는 까닭. 없으면 보낸다
pub fn overlay_refusal(confirm: bool, other_bar: bool) -> Option<String> {
    overlay_refusal_for("키를 보내지", confirm, other_bar)
}

/// `overlay_refusal`의 일반형. `what`은 하지 않은 일("키를 보내지", "동작을 실행하지")
pub fn overlay_refusal_for(what: &str, confirm: bool, other_bar: bool) -> Option<String> {
    if confirm {
        Some(format!("그 창에 확인할 일이 있어 {what} 않았습니다"))
    } else if other_bar {
        Some(format!("그 창에 검색·팔레트·제목 입력이 열려 있어 {what} 않았습니다"))
    } else {
        None
    }
}

/// `perform action`의 동작 글을 키 묶음과 같은 해석기(`keymap::parse_action`)로 읽는다. 모르는 동작과 인자 오류는 오류다.
/// `unbind`는 키를 푸는 글자라 칸에서 할 동작이 없고, `all:`·`performable:` 같은 접두사는 키 묶음 줄의 것이라 동작 이름이
/// 아니다(모르는 동작으로 오류). Ghostty는 이런 글에 오류 대신 거짓을 돌려준다(`ghostty_surface_binding_action`이 읽기 실패를 거짓으로 돌려준다)
pub fn parse_script_action(action: &str) -> Result<crate::keymap::Action, String> {
    match crate::keymap::parse_action(action)? {
        Some(a) => Ok(a),
        None => Err("unbind는 칸에서 할 동작이 아닙니다".into()),
    }
}

/// `perform action`이 실행할 동작. 읽지 못한 글(모르는 동작, 인자 오류, `unbind`, `all:` 같은 접두사)과 키 테이블·순서 동작은 `None`이고
/// 스크립트는 오류가 아니라 거짓을 받는다(Ghostty `ghostty_surface_binding_action`이 해석 실패에 `false`를 돌려주는 것과 같다)
pub fn resolve_script_action(action: &str) -> Option<crate::keymap::Action> {
    match parse_script_action(action) {
        // 키 테이블·키 순서 동작은 사람의 순서 상태(`Win::keyseq`)를 바꾸고 사람이 쌓은 앞 키를 대상 칸에 쏟아낸다.
        // 스크립트의 키는 사람이 켠 키 테이블을 쓰지 않으므로(`send key`) 스크립트에는 열지 않는다
        Ok(a) if crate::keymap::is_table_action(&a) => {
            log::debug!("AppleScript perform action: 키 테이블·순서 동작은 스크립트에 열지 않습니다");
            None
        }
        Ok(a) => Some(a),
        // 해석 오류 글에는 스크립트가 보낸 글이 들어 있어 로그에 남기지 않는다(길이만)
        Err(_) => {
            log::debug!("AppleScript perform action: 읽지 못한 동작({} 바이트)", action.len());
            None
        }
    }
}

impl App {
    /// vendor가 넘긴 명령 하나를 처리하고 답을 보낸다
    pub(super) fn on_script(
        &mut self,
        event_loop: &ActiveEventLoop,
        cmd: ScriptCommand,
        ticket: ScriptTicket,
        reply: Sender<ScriptReply>,
    ) {
        // 스크립트가 시간 초과로 이미 포기한 요청은 늦게 와도 실행하지 않는다(글이 PTY에 들어가거나 창이 뜨면 안 된다)
        if !ticket.take() {
            log::debug!("AppleScript: 포기된 요청을 버렸습니다: {cmd:?}");
            return;
        }
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
        let ids: Vec<u64> = windows.iter().map(|w| w.id).collect();
        let front = front_window_id(self.last_focused.map(u64::from), self.w.id().map(u64::from), &ids);
        build_snapshot(&windows, front)
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

    /// `send key`: 해석한 키를 사람이 누른 키와 같은 인코더로 바이트로 만들어 그 칸에 쓴다. 붙여넣기 보호는 거치지 않는다.
    /// 키 묶음(`keybind`)과 `key-remap`을 사람이 누른 키와 같게 거친다(Ghostty `surface.sendKeyEvent`가 코어의 `keyCallback`을 거치는 것처럼)
    fn send_script_key(&mut self, event_loop: &ActiveEventLoop, pane: u64, prepared: crate::script_key::Prepared) {
        use crate::keys;
        let Some(input) = prepared.input else { return };
        let Some(i) = self.pane_index(pane) else { return };
        let modes = self.panes[i].session.modes();
        let sided = keys::sided_from_state(prepared.mods);
        let mods = keys::remapped_state(&self.config.key_remap, sided);
        let raw = keys::to_state(sided);
        let meta = keys::option_counts_as_alt(self.config.option_as_alt, sided);
        let bytes = keys::encode_input(&input, mods, raw, modes, meta);
        if !input.pressed {
            // 뗌: 사람이 뗄 때와 같다. 프로그램이 이벤트 종류 보고(kitty 2)를 켰고 이 키의 누름을 프로그램에 보냈을 때만 보낸다
            if self.w.kitty_down.remove(&input.physical) == Some(pane) && modes.kitty.report_events {
                if let Some(t) = self.pane_index(pane).map(|i| &self.panes[i]) {
                    t.session.write_input(&bytes, false);
                }
            }
            return;
        }
        let report_release = modes.kitty.report_events;
        // 키 묶음 판단도 `key-remap`을 거친 수식 키로 한다(`keys::trigger`가 `self.mods`로 하는 것과 같다)
        let trigger = keys::trigger_of(&input.without_modifiers, mods);
        self.feed_key(event_loop, trigger, bytes, input.physical, report_release, Some(pane));
    }

    fn run_script(&mut self, event_loop: &ActiveEventLoop, cmd: ScriptCommand) -> ScriptReply {
        use bt_core::proto::{Call, Request};
        // btctl `new`와 같은 길: 만든 칸의 번호가 답에 들어 있다
        let run_new = |app: &mut App, call: Call| -> Result<u64, String> {
            let resp = app.handle_call(Request { id: 0, call }, event_loop);
            if !resp.ok {
                return Err(resp.error.unwrap_or_else(|| "열지 못했습니다".into()));
            }
            resp.result
                .get("tab")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| "열린 칸을 알 수 없습니다".to_string())
        };
        let new_pane = |app: &mut App, tab: Option<u64>, window: bool| -> Result<u64, String> {
            let call = Call::New {
                cwd: None,
                command: Vec::new(),
                split: None,
                tab,
                window,
            };
            run_new(app, call)
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
            ScriptCommand::SendKey {
                terminal,
                key,
                release,
                modifiers,
            } => {
                let pane = parse_id(&terminal)?;
                let i = self
                    .pane_index(pane)
                    .ok_or_else(|| format!("터미널 {terminal}이 없습니다"))?;
                // `input text`와 같은 거절: 읽기 전용 칸에는 보내지 않는다
                if self.panes[i].session.readonly() {
                    return Err(super::READONLY_REFUSAL.into());
                }
                // 키 이름·수식 키가 틀렸으면 칸이나 창을 건드리기 전에 알린다
                let prepared = crate::script_key::prepare(&key, modifiers.as_deref(), release)?;
                self.switch_to_pane(pane);
                let other_bar = self.w.search.is_some() || self.w.palette.is_some() || self.w.title_prompt.is_some();
                if let Some(why) = overlay_refusal(self.w.confirm.is_some(), other_bar) {
                    return Err(why);
                }
                self.send_script_key(event_loop, pane, prepared);
                Ok(None)
            }
            // Ghostty `perform action`(`ghostty_surface_binding_action`): 키 묶음의 동작을 그 칸에서 한다. 했으면 참, 지금 할 수 없으면 거짓
            ScriptCommand::PerformAction { terminal, action } => {
                let pane = parse_id(&terminal)?;
                let i = self
                    .pane_index(pane)
                    .ok_or_else(|| format!("터미널 {terminal}이 없습니다"))?;
                // `send key`와 같은 거절: 읽기 전용 칸에는 키 묶음 동작도 하지 않는다(스크립트가 `toggle_readonly`로 읽기 전용을 풀 수 없게)
                if self.panes[i].session.readonly() {
                    return Err(super::READONLY_REFUSAL.into());
                }
                // 읽지 못한 동작은 칸이나 창을 건드리기 전에 거짓으로 답한다(Ghostty처럼 오류가 아니다)
                let Some(action) = resolve_script_action(&action) else {
                    return Ok(Some(ScriptTarget::Performed(false)));
                };
                self.switch_to_pane(pane);
                let other_bar = self.w.search.is_some() || self.w.palette.is_some() || self.w.title_prompt.is_some();
                if let Some(why) = overlay_refusal_for("동작을 실행하지", self.w.confirm.is_some(), other_bar) {
                    return Err(why);
                }
                // 동작은 고른 칸에 한다(`feed_key`가 스크립트 키의 동작을 하기 전에 하는 것과 같다). 지금 할 수 있는지는 그 칸 기준이다
                self.focus_pane(pane);
                if !self.can_perform(&action) {
                    return Ok(Some(ScriptTarget::Performed(false)));
                }
                self.handle_action(action, event_loop);
                Ok(Some(ScriptTarget::Performed(true)))
            }
            ScriptCommand::Split { terminal, direction } => {
                let pane = parse_id(&terminal)?;
                self.pane_index(pane)
                    .ok_or_else(|| format!("터미널 {terminal}이 없습니다"))?;
                let new = run_new(self, split_call(pane, direction))?;
                Ok(Some(ScriptTarget::Terminal(new.to_string())))
            }
            // 닫기는 Ghostty처럼 확인 막대 없이 한다(`closeSurface(withConfirmation: false)`). 되살리기 목록에도 넣지 않는다(`registerRedo: false`)
            ScriptCommand::Close { terminal } => {
                let pane = parse_id(&terminal)?;
                self.pane_index(pane)
                    .ok_or_else(|| format!("터미널 {terminal}이 없습니다"))?;
                self.switch_to_pane(pane);
                drop(self.redo.clear());
                if let Some(i) = self.pane_index(pane) {
                    self.close_pane(i, event_loop, false);
                }
                Ok(None)
            }
            ScriptCommand::CloseTab { tab } => {
                let first = parse_id(&tab)?;
                self.switch_to_pane(first);
                let leaves = self
                    .w
                    .tabs
                    .iter()
                    .map(|g| g.layout.leaves())
                    .find(|l| l.first() == Some(&first))
                    .ok_or_else(|| format!("탭 {tab}이 없습니다"))?;
                drop(self.redo.clear());
                // 칸을 하나씩 닫는다. 마지막 칸이 닫히면 탭이, 창의 마지막 탭이면 창이 닫힌다
                for id in leaves {
                    if let Some(i) = self.pane_index(id) {
                        self.close_pane(i, event_loop, false);
                    }
                }
                Ok(None)
            }
            ScriptCommand::CloseWindow { window } => {
                let pane = self.script_window_pane(&window)?;
                self.switch_to_pane(pane);
                drop(self.redo.clear());
                self.close_current_window(event_loop);
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
    winit::platform::macos::set_scripting_handler(Box::new(move |cmd, ticket, reply| {
        if let Err(e) = proxy.send_event(UserEvent::Script(cmd, ticket, reply.clone())) {
            let _ = reply.send(Err(format!("터미널이 끝나는 중입니다: {e}")));
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 명령을 로그에 찍어도 `input text`의 글은 나오지 않는다(암호나 비밀이 들어 있을 수 있다)
    #[test]
    fn command_debug_hides_input_text() {
        let cmd = ScriptCommand::InputText {
            terminal: "3".into(),
            text: "hunter2 비밀".into(),
        };
        let shown = format!("{cmd:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(!shown.contains("비밀"), "{shown}");
        assert!(shown.contains("InputText"), "{shown}");
    }

    /// `send key`의 키 이름과 수식 키도 로그에 나오지 않는다(눌러 쓴 암호일 수 있다)
    #[test]
    fn command_debug_hides_send_key() {
        let cmd = ScriptCommand::SendKey {
            terminal: "3".into(),
            key: "hunterkey".into(),
            release: false,
            modifiers: Some("secretmod".into()),
        };
        let shown = format!("{cmd:?}");
        assert!(!shown.contains("hunterkey") && !shown.contains("secretmod"), "{shown}");
        assert!(shown.contains("SendKey") && shown.contains("terminal"), "{shown}");
    }

    #[test]
    fn send_key_text_is_capped_before_it_is_used() {
        use winit::platform::macos::ScriptCommand as C;
        let key = |key: String, modifiers: Option<String>| C::SendKey {
            terminal: "1".into(),
            key,
            release: false,
            modifiers,
        };
        assert!(admit(true, key("enter".into(), Some("shift,control".into()))).is_ok());
        assert!(admit(true, key("x".repeat(MAX_KEY), None)).is_ok());
        assert!(admit(true, key("x".repeat(MAX_KEY + 1), None)).is_err());
        assert!(admit(true, key("a".into(), Some("x".repeat(MAX_KEY + 1)))).is_err());
    }

    /// 동작 글은 키 묶음과 같은 해석기(`keymap::parse_action`)로 읽는다. 모르는 동작·인자 오류·`unbind`·접두사는 오류다
    #[test]
    fn unreadable_actions_answer_false_like_ghostty() {
        for bad in [
            "no_such_action",
            "all:text:x",
            "unbind",
            "",
            "new_split:sideways",
            // 키 테이블·키 순서 동작은 사람의 순서 상태를 바꾸고 쌓인 앞 키를 쏟아내므로 스크립트에는 열지 않는다
            "end_key_sequence",
            "activate_key_table:vim",
            "activate_key_table_once:vim",
            "deactivate_key_table",
            "deactivate_all_key_tables",
        ] {
            assert_eq!(resolve_script_action(bad), None, "{bad}은 오류가 아니라 거짓");
        }
        assert_eq!(resolve_script_action("new_tab"), Some(crate::keymap::Action::NewTab));
        assert_eq!(
            resolve_script_action("text:a\\nb"),
            Some(crate::keymap::Action::Text("a\nb".into()))
        );
    }

    #[test]
    fn perform_action_is_read_by_the_keybind_parser() {
        for ok in [
            "new_split:right",
            "goto_tab:2",
            "toggle_fullscreen",
            "text:ls\\n",
            "csi:A",
            "esc:b",
            "close_surface",
            "copy_to_clipboard:plain",
            "scroll_page_lines:-3",
        ] {
            let got = parse_script_action(ok).unwrap_or_else(|e| panic!("{ok}: {e}"));
            let want = crate::keymap::parse_action(ok).unwrap().expect("unbind가 아니다");
            assert_eq!(got, want, "{ok}는 키 묶음과 같은 해석");
        }
        // text:는 키 묶음처럼 이스케이프를 푼다
        assert_eq!(
            parse_script_action("text:a\\nb").unwrap(),
            crate::keymap::Action::Text("a\nb".into())
        );
        for bad in [
            "",
            "   ",
            "no_such_action",
            "new_split:sideways",
            "goto_tab",
            "goto_tab:0",
            "cursor_key:x",
            // unbind는 키를 푸는 글자라 칸에서 할 동작이 아니다
            "unbind",
            // 접두사는 키 묶음 줄의 것이다. `all:`은 동작 글에 오지 않고 오면 모르는 동작이다(Ghostty도 동작 이름 `all`이 없어 거짓)
            "all:text:x",
            "performable:new_tab",
            "unconsumed:new_tab",
            "global:quit",
        ] {
            assert!(parse_script_action(bad).is_err(), "{bad:?}");
        }
    }

    /// 동작 글도 읽기 전에 크기를 건다
    #[test]
    fn perform_action_text_is_capped_before_it_is_used() {
        use winit::platform::macos::ScriptCommand as C;
        let cmd = |action: String| C::PerformAction {
            terminal: "1".into(),
            action,
        };
        assert!(admit(true, cmd("new_split:right".into())).is_ok());
        assert!(admit(true, cmd("x".repeat(MAX_ACTION))).is_ok());
        assert!(admit(true, cmd("x".repeat(MAX_ACTION + 1))).is_err());
        assert!(admit(false, cmd("new_tab".into())).is_err(), "꺼져 있으면 거절");
    }

    /// 로그에는 동작 글이 나오지 않는다(`text:`에 비밀이 들어 있을 수 있다)
    #[test]
    fn command_debug_hides_perform_action() {
        let cmd = ScriptCommand::PerformAction {
            terminal: "3".into(),
            action: "text:hunter2 비밀".into(),
        };
        let shown = format!("{cmd:?}");
        assert!(!shown.contains("hunter2") && !shown.contains("비밀"), "{shown}");
        assert!(shown.contains("PerformAction") && shown.contains("terminal"), "{shown}");
    }

    /// 막대가 떠 있는 창에는 동작도 실행하지 않는다. 까닭 글은 한 일에 맞다
    #[test]
    fn actions_are_refused_while_a_bar_is_open() {
        let why = overlay_refusal_for("동작을 실행하지", true, false).expect("확인 막대");
        assert!(why.contains("확인") && why.contains("동작을 실행하지"), "{why}");
        let why = overlay_refusal_for("동작을 실행하지", false, true).expect("다른 막대");
        assert!(why.contains("검색") && why.contains("동작을 실행하지"), "{why}");
        assert_eq!(overlay_refusal_for("동작을 실행하지", false, false), None);
        // 키 쪽 글은 그대로다
        assert!(overlay_refusal(true, false).is_some_and(|m| m.contains("키를 보내지")));
    }

    /// `perform action`의 이름·네 글자 코드는 Ghostty 사전과 같다: 명령 `GhstPfAc`, 직접 인자 text, `on` 인자 `GonT`(terminal), 결과 boolean
    #[test]
    fn perform_action_matches_the_ghostty_dictionary() {
        let start = SDEF
            .find("<command name=\"perform action\"")
            .expect("perform action 명령");
        let len = SDEF[start..].find("</command>").expect("명령 끝");
        let cmd = &SDEF[start..start + len];
        for needle in [
            "<command name=\"perform action\" code=\"GhstPfAc\"",
            "<direct-parameter type=\"text\"",
            "<parameter name=\"on\" code=\"GonT\" type=\"terminal\"",
            "<cocoa key=\"terminal\"/>",
            "<cocoa class=\"BTScriptPerformActionCommand\"/>",
            "<result type=\"boolean\"",
        ] {
            assert!(cmd.contains(needle), "sdef의 perform action에 없다: {needle}");
        }
    }

    /// `split`은 `btctl new --split 방향 --tab 칸`과 같은 요청이다
    #[test]
    fn split_is_the_same_call_as_btctl_new_split() {
        use bt_core::proto::Call;
        for (dir, word) in [
            (ScriptSplit::Right, "right"),
            (ScriptSplit::Left, "left"),
            (ScriptSplit::Down, "down"),
            (ScriptSplit::Up, "up"),
        ] {
            assert_eq!(
                split_call(7, dir),
                Call::New {
                    cwd: None,
                    command: Vec::new(),
                    split: Some(word.into()),
                    tab: Some(7),
                    window: false,
                },
                "{word}"
            );
        }
    }

    /// 확인 막대가 떠 있는 창에는 키를 보내지 않는다(`input text`와 같다). 검색·팔레트·제목 입력이 열린 창도 같다
    #[test]
    fn keys_are_refused_while_a_bar_is_open() {
        assert!(overlay_refusal(true, false).is_some_and(|m| m.contains("확인")));
        assert!(overlay_refusal(false, true).is_some());
        assert!(overlay_refusal(true, true).is_some());
        assert_eq!(overlay_refusal(false, false), None);
    }

    /// sdef의 enumeration 네 글자 코드는 Ghostty와 같고 vendor가 같은 코드를 읽는다
    #[test]
    fn enumeration_codes_match_ghostty_and_vendor() {
        let code = |s: &[u8; 4]| u32::from_be_bytes(*s);
        assert_eq!(ScriptSplit::from_code(code(b"GSrt")), Some(ScriptSplit::Right));
        assert_eq!(ScriptSplit::from_code(code(b"GSlf")), Some(ScriptSplit::Left));
        assert_eq!(ScriptSplit::from_code(code(b"GSdn")), Some(ScriptSplit::Down));
        assert_eq!(ScriptSplit::from_code(code(b"GSup")), Some(ScriptSplit::Up));
        assert_eq!(ScriptSplit::from_code(code(b"GIpr")), None);
        assert_eq!(ScriptSplit::from_code(0), None);
        assert!(winit::platform::macos::script_input_action_is_release(code(b"GIrl")));
        assert!(!winit::platform::macos::script_input_action_is_release(code(b"GIpr")));
        // 모르는 값은 누름이다(Ghostty `send key`의 default)
        assert!(!winit::platform::macos::script_input_action_is_release(0));
        // sdef에 같은 코드가 있다
        for needle in [
            "<enumeration name=\"split direction\" code=\"GSpD\"",
            "<enumerator name=\"right\" code=\"GSrt\"",
            "<enumerator name=\"left\" code=\"GSlf\"",
            "<enumerator name=\"down\" code=\"GSdn\"",
            "<enumerator name=\"up\" code=\"GSup\"",
            "<enumeration name=\"input action\" code=\"GIAc\"",
            "<enumerator name=\"press\" code=\"GIpr\"",
            "<enumerator name=\"release\" code=\"GIrl\"",
        ] {
            assert!(SDEF.contains(needle), "sdef에 없다: {needle}");
        }
    }

    /// 새 명령의 이름·네 글자 코드·인자는 Ghostty 사전과 같다(스크립트가 그대로 돌게)
    #[test]
    fn new_commands_match_the_ghostty_dictionary() {
        for needle in [
            "<command name=\"send key\" code=\"GhstSKey\"",
            "<cocoa class=\"BTScriptSendKeyCommand\"/>",
            "<parameter name=\"action\" code=\"GKeA\" type=\"input action\" optional=\"yes\"",
            "<parameter name=\"modifiers\" code=\"GKeM\" type=\"text\" optional=\"yes\"",
            "<parameter name=\"to\" code=\"GKeT\" type=\"terminal\"",
            "<command name=\"split\" code=\"GhstSplt\"",
            "<parameter name=\"direction\" code=\"GSpd\" type=\"split direction\"",
            "<command name=\"close\" code=\"GhstClos\"",
            "<command name=\"close tab\" code=\"GhstClTb\"",
            "<command name=\"close window\" code=\"GhstClWn\"",
        ] {
            assert!(SDEF.contains(needle), "sdef에 없다: {needle}");
        }
        // 클래스가 새 명령에 응답한다
        let class = |name: &str| -> &str {
            let start = SDEF.find(&format!("<class name=\"{name}\"")).expect(name);
            let len = SDEF[start..].find("</class>").expect(name);
            &SDEF[start..start + len]
        };
        for (cls, command, method) in [
            ("terminal", "split", "handleSplitCommand:"),
            ("terminal", "close", "handleCloseCommand:"),
            ("tab", "close tab", "handleCloseTabCommand:"),
            ("window", "close window", "handleCloseWindowCommand:"),
        ] {
            let c = class(cls);
            assert!(
                c.contains(&format!("<responds-to command=\"{command}\">"))
                    && c.contains(&format!("<cocoa method=\"{method}\"/>")),
                "{cls} 클래스가 {command}에 응답하지 않는다"
            );
        }
        // 뺀 것: split의 with configuration
        assert!(!SDEF.contains("name=\"with configuration\""));
    }

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

    /// Ghostty처럼 창은 앞에서 뒤 차례다: 앞 창이 `window 1`이고 나머지는 만든 차례(#645)
    #[test]
    fn front_window_comes_first_then_creation_order() {
        let ids = |front| -> Vec<String> {
            build_snapshot(&two_windows(), front)
                .windows
                .into_iter()
                .map(|w| w.id)
                .collect()
        };
        assert_eq!(ids(Some(22)), ["22", "11"]);
        assert_eq!(ids(Some(11)), ["11", "22"]);
        assert_eq!(ids(None), ["11", "22"]);
        assert_eq!(ids(Some(99)), ["11", "22"], "모르는 창은 앞 창이 아니다");
    }

    #[test]
    fn snapshot_names_tabs_by_first_pane() {
        let s = build_snapshot(&two_windows(), Some(11));
        let ids: Vec<&str> = s.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["11", "22"]);
        assert_eq!(s.front_window.as_deref(), Some("11"));
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

    const SDEF: &str = include_str!("../../../../assets/macos/b-terminal.sdef");

    /// sdef의 application 클래스 부분
    fn sdef_application_class() -> &'static str {
        let start = SDEF.find("<class name=\"application\"").expect("application 클래스");
        let len = SDEF[start..].find("</class>").expect("application 클래스 끝");
        &SDEF[start..start + len]
    }

    /// 로그아웃·종료 때 macOS가 보내는 `quit`(aevtquit)를 사전이 받는다. sdef가 있으면 Cocoa Scripting은 받는 클래스의
    /// `responds-to`가 있어야 메서드를 부른다(#644)
    #[test]
    fn application_class_responds_to_quit() {
        let app = sdef_application_class();
        assert!(
            app.contains("<responds-to command=\"quit\">")
                && app.contains("<cocoa method=\"handleQuitScriptCommand:\"/>"),
            "application 클래스에 quit 응답이 없다"
        );
    }

    /// vendor가 `NSApplication`에 다는 메서드는 AppKit에 이미 있는 이름과 겹치면 안 된다(겹치면 AppKit 것이 그대로 불린다,
    /// `frontWindow`가 그랬다). sdef application 클래스의 키·메서드는 vendor가 다는 것이거나 일부러 쓰는 AppKit 것이다(#645)
    #[cfg(target_os = "macos")]
    #[test]
    fn application_keys_do_not_collide_with_appkit() {
        assert_eq!(
            winit::platform::macos::scripting_app_key_conflicts(),
            Vec::<&str>::new()
        );
        // AppKit 것을 그대로 쓰는 키: 앞 앱인지(`isActive`), quit
        const APPKIT: [&str; 2] = ["isActive", "handleQuitScriptCommand:"];
        let installed = winit::platform::macos::scripting_app_keys();
        let app = sdef_application_class();
        for attr in ["cocoa key=\"", "cocoa method=\""] {
            for part in app.split(attr).skip(1) {
                let name = &part[..part.find('"').expect("따옴표")];
                assert!(
                    installed.contains(&name) || APPKIT.contains(&name),
                    "sdef의 {name}을 vendor가 달지 않는다"
                );
            }
        }
    }

    /// 창·탭·칸 클래스가 답하는 새 메서드 이름은 `NSObject`에 이미 있는 것과 겹치지 않는다(겹치면 모든 객체에서 덮어쓴다, #650).
    /// sdef의 창·탭·칸 클래스 메서드는 모두 vendor가 선언한 것이다
    #[cfg(target_os = "macos")]
    #[test]
    fn object_keys_do_not_collide_with_nsobject() {
        assert_eq!(
            winit::platform::macos::scripting_object_key_conflicts(),
            Vec::<&str>::new()
        );
        let declared = winit::platform::macos::scripting_object_keys();
        for part in SDEF.split("<cocoa method=\"").skip(1) {
            let name = &part[..part.find('"').expect("따옴표")];
            // application 클래스의 메서드(new window, new tab, quit)는 NSApplication에 단다
            if name.starts_with("handleNew") || name == "handleQuitScriptCommand:" {
                continue;
            }
            assert!(declared.contains(&name), "sdef의 {name}을 vendor가 선언하지 않는다");
        }
    }

    #[test]
    fn front_window_falls_back_to_the_current_window_when_the_focused_one_closed() {
        // 마지막 포커스 창(11)이 스크립트의 close window로 닫혔다. 남은 창 12가 지금 창이다
        assert_eq!(front_window_id(Some(11), Some(12), &[12, 13]), Some(12));
        assert_eq!(
            front_window_id(None, Some(13), &[12, 13]),
            Some(13),
            "포커스를 받은 적 없는 창만 있어도 앞 창이 있다"
        );
        assert_eq!(
            front_window_id(Some(12), Some(13), &[12, 13]),
            Some(12),
            "포커스 받은 창이 남아 있으면 그 창"
        );
        assert_eq!(front_window_id(Some(11), None, &[]), None, "창이 없으면 앞 창도 없다");
        assert_eq!(
            front_window_id(Some(11), Some(99), &[12]),
            None,
            "지금 창이 목록에 없으면 앞 창이 아니다"
        );
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
            C::SendKey {
                terminal: "1".into(),
                key: "enter".into(),
                release: false,
                modifiers: None,
            },
            C::PerformAction {
                terminal: "1".into(),
                action: "new_tab".into(),
            },
            C::Split {
                terminal: "1".into(),
                direction: ScriptSplit::Right,
            },
            C::Close { terminal: "1".into() },
            C::CloseTab { tab: "1".into() },
            C::CloseWindow { window: "1".into() },
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

    #[test]
    fn a_ticket_is_won_by_exactly_one_side() {
        use winit::platform::macos::ScriptTicket;
        // 앱이 먼저 가져가면 시간 초과 쪽의 취소는 실패한다(앱이 이미 실행 중이니 답을 더 기다린다)
        let t = ScriptTicket::new();
        assert!(t.clone().take());
        assert!(!t.cancel());
        assert!(!t.take(), "두 번 가져가지 못한다");
        // 취소가 먼저면 늦게 온 사건은 가져가지 못한다(버린다)
        let t = ScriptTicket::new();
        let seen_by_app = t.clone();
        assert!(t.cancel());
        assert!(!seen_by_app.take());
        assert!(!seen_by_app.take());
    }

    #[test]
    fn snapshot_refresh_backs_off_after_a_failure() {
        use std::time::{Duration, Instant};
        use winit::platform::macos::ScriptRefresh;
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let mut r = ScriptRefresh::new();
        assert!(r.due(t0), "처음에는 묻는다");
        r.record_ok(t0);
        assert!(!r.due(t0 + ms(50)), "0.1초 안은 그대로 쓴다");
        assert!(r.due(t0 + ms(200)));
        // 앱이 답하지 않았다: 한동안 다시 묻지 않는다(읽을 때마다 5초씩 기다리지 않게)
        r.record_fail(t0 + ms(300));
        assert!(!r.due(t0 + ms(300)));
        assert!(!r.due(t0 + ms(2200)));
        assert!(r.due(t0 + ms(2400)));
        r.record_ok(t0 + ms(2500));
        assert!(!r.due(t0 + ms(2550)), "성공하면 실패 기록은 지워진다");
        assert!(r.due(t0 + ms(2700)));
    }
}
