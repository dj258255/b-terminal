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
//! `send mouse button`·`send mouse position`·`send mouse scroll`(#662)은 사람의 마우스와 같은 처리(`app/mouse_core.rs`)를 지나되, 상태는 칸마다 둔
//! 스크립트 마우스 상태(`script_mouse::ScriptMouse`)로 따로 쓴다. 사람의 포인터·누름 횟수·고르는 중·휠 누적과 사람의 수식 키는 건드리지 않는다.
//! 창 장식(제목 막대·탭 줄·손잡이·막대 띠)과 초점 누름 삼키기는 지나지 않고, 클립보드를 읽는 동작(붙여넣기)과 모달 메뉴는 하지 않는다.
//!
//! `new surface configuration`과 `new window`·`new tab`·`split`의 `with configuration`(#666)은 레코드를 vendor가 읽어(크기 상한을 복사 전에) 넘기면
//! `script_config`가 검사하고, 만들 칸 하나에만 덮어쓴다(`run_new`가 `App::spawn_overrides`에 넣고 호출이 끝나면 비운다). 레코드 오류는 창이나 칸을
//! 만들기 전에 낸다. 설정 파일과 `btctl` 소켓 규약(`Call::New`)은 건드리지 않는다.
//!
//! ids: 칸은 `btctl list`의 번호 그대로, 탭은 그 탭의 첫 칸 번호(칸이 닫히거나 탭 첫 칸이 바뀌면 바뀐다),
//! 창은 창 번호(winit `WindowId`)다. 모두 10진수 글이다.

use std::sync::mpsc::Sender;

use winit::event_loop::ActiveEventLoop;
use winit::platform::macos::{
    ScriptCommand, ScriptConfig, ScriptReply, ScriptSnapshot, ScriptSplit, ScriptTab, ScriptTarget, ScriptTerminal,
    ScriptTicket, ScriptWindow,
};

use crate::script_config::{self, RawConfig, SpawnOverrides, SurfaceConfig};

use super::{App, UserEvent, clean_user_title};

/// 한 번에 넣을 글의 상한(바이트). 제어 소켓 요청 한 줄의 상한(1MiB)과 같다
const MAX_TEXT: usize = 1 << 20;

/// `send key`의 키 이름·수식 키 글의 상한(바이트). 가장 긴 키 이름은 20바이트다
const MAX_KEY: usize = 256;

/// `name override`로 쓰는 제목 글의 상한(바이트). 제목은 `clean_user_title`이 200자로 자르므로(최대 800바이트) 넉넉하다.
/// vendor가 복사 전에 같은 값으로 먼저 본다(시험이 두 값이 같음을 지킨다)
const MAX_TITLE: usize = 4096;

/// `perform action`의 동작 글의 상한(바이트). 가장 긴 보통 동작(`text:`로 몇 줄)도 이보다 짧다. 더 긴 글은 `input text`로 넣는다
const MAX_ACTION: usize = 4096;

/// 스냅숏을 만들 재료: 칸
pub struct PaneView {
    pub id: u64,
    pub title: String,
    /// 사람이나 스크립트가 붙인 제목(`Pane::user_title`). 없으면 빈 글
    pub title_override: String,
    /// 전경 프로세스 그룹의 pid. 모르면 0
    pub pid: i32,
    /// PTY 장치 경로. 모르면 빈 글
    pub tty: String,
    pub cwd: Option<String>,
}

/// 스냅숏을 만들 재료: 탭(칸은 왼쪽·위부터). `title`은 덮어쓴 제목이나 초점 칸 제목
pub struct TabView {
    pub title: String,
    /// 탭에 덮어쓴 제목(`Action::SetTabTitle`과 같은 상태). 없으면 빈 글
    pub title_override: String,
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
                        title_override: t.title_override.clone(),
                        selected: i == w.active,
                        focused_terminal: Some(t.focus.to_string()),
                        terminals: t
                            .panes
                            .iter()
                            .map(|p| ScriptTerminal {
                                id: p.id.to_string(),
                                title: p.title.clone(),
                                title_override: p.title_override.clone(),
                                working_directory: p.cwd.clone(),
                                pid: p.pid,
                                tty: p.tty.clone(),
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
    // `surface configuration` 레코드의 크기 상한(vendor가 복사 전에 먼저 본다. 여기서 한 번 더)
    if let Some(c) = config_of(&cmd) {
        script_config::check_raw_sizes(&raw_config(c))?;
    }
    match &cmd {
        ScriptCommand::InputText { text, .. } => check_text(text)?,
        ScriptCommand::SendKey { key, modifiers, .. }
            if key.len() > MAX_KEY || modifiers.as_ref().is_some_and(|m| m.len() > MAX_KEY) =>
        {
            return Err(format!("키 이름이나 수식 키가 너무 깁니다(상한 {MAX_KEY} 바이트)"));
        }
        // 마우스 명령: 수식 키 글은 `send key`와 같은 상한으로, 좌표와 휠 값은 유한해야 한다
        ScriptCommand::SendMouseButton { modifiers, .. } | ScriptCommand::SendMousePosition { modifiers, .. }
            if modifiers.as_ref().is_some_and(|m| m.len() > MAX_KEY) =>
        {
            return Err(format!("수식 키가 너무 깁니다(상한 {MAX_KEY} 바이트)"));
        }
        ScriptCommand::SendMousePosition { x, y, .. } | ScriptCommand::SendMouseScroll { x, y, .. }
            if !x.is_finite() || !y.is_finite() =>
        {
            return Err("좌표와 스크롤 값은 유한한 수여야 합니다".into());
        }
        // 스크립트가 쓰는 제목도 믿지 않는 글이다: 크기는 읽기 전에 건다(내용은 `clean_user_title`이 거른다)
        ScriptCommand::SetTabTitleOverride { title, .. } | ScriptCommand::SetTerminalTitleOverride { title, .. }
            if title.len() > MAX_TITLE =>
        {
            return Err(format!("제목이 너무 깁니다({} 바이트, 상한 {MAX_TITLE})", title.len()));
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

/// 명령이 들고 온 `surface configuration` 레코드
fn config_of(cmd: &ScriptCommand) -> Option<&ScriptConfig> {
    match cmd {
        ScriptCommand::NewWindow { config }
        | ScriptCommand::NewTab { config, .. }
        | ScriptCommand::Split { config, .. } => config.as_ref(),
        ScriptCommand::NewSurfaceConfiguration { from } => from.as_ref(),
        _ => None,
    }
}

/// vendor의 레코드를 `script_config`의 것으로(같은 여섯 칸이다. vendor는 앱 크레이트를 모른다)
fn raw_config(c: &ScriptConfig) -> RawConfig {
    RawConfig {
        font_size: c.font_size,
        working_directory: c.working_directory.clone(),
        command: c.command.clone(),
        initial_input: c.initial_input.clone(),
        wait_after_command: c.wait_after_command,
        environment: c.environment.clone(),
    }
}

/// `script_config`의 레코드를 vendor의 것으로
fn script_config_of(r: RawConfig) -> ScriptConfig {
    ScriptConfig {
        font_size: r.font_size,
        working_directory: r.working_directory,
        command: r.command,
        initial_input: r.initial_input,
        wait_after_command: r.wait_after_command,
        environment: r.environment,
    }
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
                                title_override: p.user_title.clone().unwrap_or_default(),
                                pid: p.session.foreground_pid().unwrap_or(0),
                                tty: p
                                    .session
                                    .tty_name()
                                    .map(|t| t.to_string_lossy().into_owned())
                                    .unwrap_or_default(),
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
                            title_override: g.title.clone().unwrap_or_default(),
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

    /// 레코드(`with configuration`)를 검사해 새 칸 하나용 덮어쓰기로 만든다. 레코드가 없으면 `None`.
    /// 오류는 창이나 칸을 만들기 전에 알리려고 호출하는 쪽이 맨 앞에서 부른다
    fn script_overrides(config: Option<ScriptConfig>) -> Result<Option<SpawnOverrides>, String> {
        let Some(c) = config else { return Ok(None) };
        let cfg = script_config::validate(raw_config(&c))?;
        Ok(Some(script_config::overrides(&cfg, |p| p.is_dir())))
    }

    /// 만든 칸에 칸 단위 값을 마저 적용한다: 끝나도 칸을 남기기(`wait after command`)와 이 칸만의 글자 크기.
    /// 글자 크기는 창의 기준 크기와 다른 칸을 바꾸지 않고(`window-inherit-font-size`보다 이긴다), 격자는 이 칸의 크기로 다시 맞춘다
    fn finish_script_pane(&mut self, pane: u64, ov: &SpawnOverrides) {
        let Some(i) = self.pane_index(pane) else { return };
        if ov.wait_after {
            self.panes[i].wait_after_exit = true;
        }
        if let Some(size) = ov.font_size {
            self.panes[i].font_size = size;
            // resize_all이 글꼴 묶음을 맞추고 칸마다 격자를 다시 잰다
            self.resize_all();
        }
    }

    fn run_script(&mut self, event_loop: &ActiveEventLoop, cmd: ScriptCommand) -> ScriptReply {
        use bt_core::proto::{Call, Request};
        // btctl `new`와 같은 길: 만든 칸의 번호가 답에 들어 있다. `ov`는 이 칸 하나에만 덮어쓸 값이다.
        // 호출이 끝나면 쓰였든 아니든 비운다(다음 칸에 남지 않게)
        let run_new = |app: &mut App, call: Call, ov: Option<SpawnOverrides>| -> Result<u64, String> {
            app.spawn_overrides = ov.clone();
            let resp = app.handle_call(Request { id: 0, call }, event_loop);
            app.spawn_overrides = None;
            if !resp.ok {
                return Err(resp.error.unwrap_or_else(|| "열지 못했습니다".into()));
            }
            let pane = resp
                .result
                .get("tab")
                .and_then(|v| v.as_u64())
                .ok_or_else(|| "열린 칸을 알 수 없습니다".to_string())?;
            if let Some(ov) = &ov {
                app.finish_script_pane(pane, ov);
            }
            Ok(pane)
        };
        let new_pane =
            |app: &mut App, tab: Option<u64>, window: bool, ov: Option<SpawnOverrides>| -> Result<u64, String> {
                let call = Call::New {
                    cwd: None,
                    command: Vec::new(),
                    split: None,
                    tab,
                    window,
                };
                run_new(app, call, ov)
            };
        match cmd {
            ScriptCommand::Snapshot => Ok(None),
            ScriptCommand::NewSurfaceConfiguration { from } => {
                // 검사한 레코드를 모든 키가 있는 레코드로 돌려준다(없으면 기본값 레코드)
                let cfg = match from {
                    Some(c) => script_config::validate(raw_config(&c))?,
                    None => SurfaceConfig::default(),
                };
                Ok(Some(ScriptTarget::Configuration(script_config_of(
                    script_config::to_record(&cfg),
                ))))
            }
            ScriptCommand::NewWindow { config } => {
                let ov = Self::script_overrides(config)?;
                let pane = new_pane(self, None, true, ov)?;
                let window = self.script_window_of(pane).ok_or("새 창을 찾지 못했습니다")?;
                Ok(Some(ScriptTarget::Window(window.to_string())))
            }
            ScriptCommand::NewTab { window, config } => {
                // 레코드가 틀렸으면 창을 찾기 전에, 아무것도 만들기 전에 알린다
                let ov = Self::script_overrides(config)?;
                let at = match window {
                    Some(w) => Some(self.script_window_pane(&w)?),
                    None => None,
                };
                let pane = new_pane(self, at, false, ov)?;
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
            ScriptCommand::SendMouseButton {
                terminal,
                button,
                release,
                modifiers,
            } => {
                let (idx, mods) = self.script_mouse_target(&terminal, modifiers.as_deref())?;
                self.script_mouse_button(idx, mods, button, !release);
                Ok(None)
            }
            ScriptCommand::SendMousePosition {
                terminal,
                x,
                y,
                modifiers,
            } => {
                let (idx, mods) = self.script_mouse_target(&terminal, modifiers.as_deref())?;
                self.script_mouse_position(idx, mods, (x, y));
                Ok(None)
            }
            // `momentum`은 Ghostty 코어가 읽지 않아(Surface.zig `scrollCallback`) 읽고 버린다
            ScriptCommand::SendMouseScroll {
                terminal,
                x,
                y,
                precision,
                momentum: _,
            } => {
                let (idx, mods) = self.script_mouse_target(&terminal, None)?;
                self.script_mouse_scroll(idx, mods, (x, y), precision);
                Ok(None)
            }
            ScriptCommand::Split {
                terminal,
                direction,
                config,
            } => {
                let ov = Self::script_overrides(config)?;
                let pane = parse_id(&terminal)?;
                self.pane_index(pane)
                    .ok_or_else(|| format!("터미널 {terminal}이 없습니다"))?;
                let new = run_new(self, split_call(pane, direction), ov)?;
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
            // 제목 덮어쓰기는 PTY에 아무것도 쓰지 않으므로 읽기 전용 칸도 막지 않는다. 사람이 `set_tab_title`·`set_surface_title`로 하는 것과 같은
            // 상태(탭의 `title`, 칸의 `user_title`)를 같은 거르기(`clean_user_title`)로 쓴다. 빈 글은 지운다. 글은 로그에 남기지 않는다
            ScriptCommand::SetTabTitleOverride { tab, title } => {
                let first = parse_id(&tab)?;
                let group = std::iter::once(&mut self.w)
                    .chain(self.others.iter_mut())
                    .flat_map(|w| w.tabs.iter_mut())
                    .find(|g| g.layout.leaves().first() == Some(&first))
                    .ok_or_else(|| format!("탭 {tab}이 없습니다"))?;
                group.title = clean_user_title(&title);
                self.request_redraw_all();
                Ok(None)
            }
            ScriptCommand::SetTerminalTitleOverride { terminal, title } => {
                let pane = parse_id(&terminal)?;
                self.pane_index(pane)
                    .ok_or_else(|| format!("터미널 {terminal}이 없습니다"))?;
                self.set_user_title(pane, &title);
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

// 마우스 명령의 시작
// 이 구간은 사람의 마우스 상태(포인터·누름 횟수·고르는 중·휠 누적·링크 누름·칸의 눌린 단추)와 사람의 수식 키를 읽거나 쓰지 않는다.
// 스크립트 마우스는 칸마다 둔 `ScriptMouse`(위치·누름 기록·눌린 단추·고르는 중·마지막 칸·휠 누적·링크 누름)와 명령이 준 수식 키로만 일한다.
// 시험(`script_mouse::tests::shared_mouse_code_never_touches_the_human_state`)이 지킨다.
impl App {
    /// 마우스 명령의 공통 앞부분. 칸이 있는지, 읽기 전용이 아닌지, 수식 키 글이 맞는지를 칸이나 창을 건드리기 전에 보고, 칸이 든 창을
    /// 지금 창으로 만든 뒤 막대(확인·검색·팔레트·제목)가 열려 있으면 거절하고, 칸을 고른 칸으로 만든다(`perform action`과 같은 순서).
    /// 이미 고른 칸이면 `focus_pane`을 부르지 않는다(고르는 중이던 사람의 고르기를 끊지 않게). 돌려주는 값은 칸의 자리와 수식 키
    fn script_mouse_target(
        &mut self,
        terminal: &str,
        modifiers: Option<&str>,
    ) -> Result<(usize, crate::script_mouse::PtrMods), String> {
        let pane = parse_id(terminal)?;
        let i = self
            .pane_index(pane)
            .ok_or_else(|| format!("터미널 {terminal}이 없습니다"))?;
        // `send key`와 같은 거절: 읽기 전용 칸에는 마우스도 보내지 않는다(고르기·붙여넣기 길도 막는다)
        if self.panes[i].session.readonly() {
            return Err(super::READONLY_REFUSAL.into());
        }
        // 수식 키 이름이 틀렸으면 칸이나 창을 건드리기 전에 알린다
        let remap = self.config.key_remap.clone();
        let mods = crate::script_mouse::parse_mods(modifiers, |m| {
            crate::keys::remapped_state(&remap, crate::keys::sided_from_state(m))
        })?;
        self.switch_to_pane(pane);
        let other_bar = self.w.search.is_some() || self.w.palette.is_some() || self.w.title_prompt.is_some();
        if let Some(why) = overlay_refusal_for("마우스를 보내지", self.w.confirm.is_some(), other_bar) {
            return Err(why);
        }
        if self.focused_id() != Some(pane) {
            self.focus_pane(pane);
        }
        let idx = self
            .pane_index(pane)
            .ok_or_else(|| format!("터미널 {terminal}이 없습니다"))?;
        Ok((idx, mods))
    }

    /// 칸 `idx`에서 스크립트 마우스 상태를 꺼내 `run`에 넘기고 돌려놓는다. `run`은 위치(창 실제 픽셀)를 담은 입력과 상태를 받는다.
    /// 칸이 보이지 않으면 아무것도 하지 않는다(고른 칸이라 보통은 보인다)
    pub(super) fn with_script_mouse(
        &mut self,
        idx: usize,
        mods: crate::script_mouse::PtrMods,
        pos: Option<(f64, f64)>,
        run: impl FnOnce(&mut Self, &super::mouse_core::PtrIn, &mut crate::script_mouse::ScriptMouse),
    ) {
        let Some(rect) = self.pane_rect(self.panes[idx].id) else {
            return;
        };
        let mut sm = std::mem::take(&mut self.panes[idx].script_mouse);
        sm.mods = mods;
        if let Some(p) = pos {
            sm.pos_pt = p;
        }
        let rel = crate::script_mouse::rel_px((rect.w as f64, rect.h as f64), sm.pos_pt.0, sm.pos_pt.1, self.scale);
        let input = super::mouse_core::PtrIn {
            pos: winit::dpi::PhysicalPosition::new(rect.x as f64 + rel.0, rect.y as f64 + rel.1),
            mods,
            script: true,
        };
        run(self, &input, &mut sm);
        self.panes[idx].script_mouse = sm;
    }

    /// `send mouse position`: 위치를 옮긴다. 앞 위치에서 1픽셀 미만이면 움직임을 처리하지 않는다(Ghostty처럼)
    fn script_mouse_position(&mut self, idx: usize, mods: crate::script_mouse::PtrMods, pos: (f64, f64)) {
        self.with_script_mouse(idx, mods, Some(pos), |app, input, sm| {
            let Some(rect) = app.pane_rect(app.panes[idx].id) else {
                return;
            };
            let rel = (input.pos.x - rect.x as f64, input.pos.y - rect.y as f64);
            if crate::script_mouse::moved_enough(sm.sent_rel, rel) {
                sm.sent_rel = Some(rel);
                app.pointer_moved(idx, input, &mut sm.state, true);
            }
        });
    }

    /// `send mouse button`: 스크립트가 마지막으로 보낸 위치에서 단추를 누르거나 뗀다
    fn script_mouse_button(
        &mut self,
        idx: usize,
        mods: crate::script_mouse::PtrMods,
        button: winit::platform::macos::ScriptMouseButton,
        pressed: bool,
    ) {
        use winit::platform::macos::ScriptMouseButton as B;
        let mb = match button {
            B::Left => crate::mouse::Button::Left,
            B::Right => crate::mouse::Button::Right,
            B::Middle => crate::mouse::Button::Middle,
        };
        self.with_script_mouse(idx, mods, None, |app, input, sm| {
            // 링크 수식 키 + 왼쪽 단추는 링크를 연다(#705, 사람과 같은 처리)
            let clicks = sm.state.last_click.map_or(0, |l| l.count);
            if !app.link_click_step(mb, pressed, input, &mut sm.link_click, clicks) {
                app.pointer_button(idx, mb, pressed, input, &mut sm.state);
            }
        });
    }

    /// `send mouse scroll`: 값을 줄 수로 바꿔(`script_mouse::scroll_lines`, Ghostty `scrollCallback`) 사람의 휠과 같은 처리로 보낸다
    fn script_mouse_scroll(
        &mut self,
        idx: usize,
        mods: crate::script_mouse::PtrMods,
        delta: (f64, f64),
        precision: bool,
    ) {
        let (discrete, precise) = self.config.scroll_multiplier;
        // 그 칸의 칸 크기(#422). 재지 못하면 사람의 휠과 같은 기본값을 쓴다
        let cell = self
            .metrics_at(self.panes[idx].font_size)
            .map_or((8.0, 16.0), |m| (m.width as f64, m.height as f64));
        let (xlines, ylines) = crate::script_mouse::scroll_lines(precision, delta.0, delta.1, cell, discrete, precise);
        self.with_script_mouse(idx, mods, None, |app, input, sm| {
            app.pointer_wheel(Some(idx), ylines, xlines, input, &mut sm.state);
        });
    }
}
// 마우스 명령의 끝

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
    }

    /// `name override`·`pid`·`tty`는 Ghostty 1.4.0의 사전과 같은 이름·코드·형·접근(#672)
    #[test]
    fn name_override_pid_tty_match_the_ghostty_dictionary() {
        let class = |name: &str| -> &str {
            let start = SDEF.find(&format!("<class name=\"{name}\"")).expect(name);
            let len = SDEF[start..].find("</class>").expect(name);
            &SDEF[start..start + len]
        };
        for cls in ["tab", "terminal"] {
            let c = class(cls);
            let at = c
                .find("<property name=\"name override\" code=\"GTno\" type=\"text\" access=\"rw\"")
                .unwrap_or_else(|| panic!("{cls}의 name override"));
            assert!(c[at..].contains("<cocoa key=\"titleOverride\"/>"), "{cls}");
        }
        let t = class("terminal");
        assert!(t.contains("<property name=\"pid\" code=\"Gpid\" type=\"integer\" access=\"r\""));
        assert!(t.contains("<property name=\"tty\" code=\"Gtty\" type=\"text\" access=\"r\""));
        // 창에는 없다
        assert!(!class("window").contains("name override"));
    }

    /// 스크립트가 쓰는 제목은 크기를 읽기 전에 건다. vendor와 앱의 상한이 같고, 상한 안의 글은 지나간다(#672)
    #[test]
    fn title_override_is_capped_before_it_is_used() {
        use winit::platform::macos::ScriptCommand as C;
        assert_eq!(winit::platform::macos::SCRIPT_TITLE_MAX_BYTES, MAX_TITLE);
        let tab = |t: String| C::SetTabTitleOverride {
            tab: "1".into(),
            title: t,
        };
        let term = |t: String| C::SetTerminalTitleOverride {
            terminal: "1".into(),
            title: t,
        };
        for make in [tab, term] {
            assert!(
                admit(true, make("가".repeat(MAX_TITLE / 3 + 1))).is_err(),
                "바이트로 센다"
            );
            assert!(admit(true, make("x".repeat(MAX_TITLE))).is_ok());
            assert!(admit(false, make(String::new())).is_err(), "꺼져 있으면 빈 글도 거절");
        }
    }

    /// 제목을 로그에 찍어도 글은 나오지 않는다(길이만)
    #[test]
    fn command_debug_hides_title_override() {
        use winit::platform::macos::ScriptCommand as C;
        let secret = "hunter2-비밀제목";
        for cmd in [
            C::SetTabTitleOverride {
                tab: "3".into(),
                title: secret.into(),
            },
            C::SetTerminalTitleOverride {
                terminal: "3".into(),
                title: secret.into(),
            },
        ] {
            let shown = format!("{cmd:?}");
            assert!(!shown.contains("hunter2") && !shown.contains("비밀제목"), "{shown}");
            assert!(shown.contains("title_bytes"), "{shown}");
        }
    }

    /// 스냅숏이 탭·칸의 덮어쓴 제목과 칸의 pid·tty를 그대로 싣는다(#672)
    #[test]
    fn snapshot_carries_overrides_pid_and_tty() {
        let mut wins = two_windows();
        wins[1].tabs[1].title_override = "빌드 서버".into();
        wins[1].tabs[1].panes[1].title_override = "로그".into();
        wins[1].tabs[1].panes[1].pid = 4242;
        wins[1].tabs[1].panes[1].tty = "/dev/ttys016".into();
        let s = build_snapshot(&wins, Some(11));
        let tab = &s.windows[0].tabs[1];
        assert_eq!(tab.title_override, "빌드 서버");
        let t = &tab.terminals[1];
        assert_eq!(
            (t.title_override.as_str(), t.pid, t.tty.as_str()),
            ("로그", 4242, "/dev/ttys016")
        );
        // 덮어쓰지 않은 것은 빈 글, 모르는 pid는 0
        assert_eq!(s.windows[0].tabs[0].title_override, "");
        assert_eq!(
            (
                s.windows[0].tabs[0].terminals[0].pid,
                s.windows[0].tabs[0].terminals[0].tty.as_str()
            ),
            (0, "")
        );
    }

    /// 쓴 제목은 사람이 정하는 것과 같은 거르기를 지난다: 제어 문자를 지우고 200자로 자르며 빈 글은 지운다
    #[test]
    fn script_titles_are_cleaned_like_human_titles() {
        assert_eq!(clean_user_title("빌드\x1b[31m 서버\n"), Some("빌드 [31m 서버".into()));
        assert_eq!(
            clean_user_title(&"가".repeat(300)).map(|t| t.chars().count()),
            Some(200)
        );
        assert_eq!(clean_user_title(""), None, "빈 글은 덮어쓴 제목을 지운다");
    }

    /// `surface configuration` 레코드와 명령 인자의 이름·네 글자 코드·cocoa key는 Ghostty 사전과 같다(#666, 스크립트가 그대로 돌게)
    #[test]
    fn surface_configuration_matches_the_ghostty_dictionary() {
        for needle in [
            "<record-type name=\"surface configuration\" code=\"GScf\"",
            "<property name=\"font size\" code=\"GScF\" type=\"real\"",
            "<cocoa key=\"fontSize\"/>",
            "<property name=\"initial working directory\" code=\"GScD\" type=\"text\"",
            "<cocoa key=\"workingDirectory\"/>",
            "<property name=\"command\" code=\"GScC\" type=\"text\"",
            "<cocoa key=\"command\"/>",
            "<property name=\"initial input\" code=\"GScI\" type=\"text\"",
            "<cocoa key=\"initialInput\"/>",
            "<property name=\"wait after command\" code=\"GScW\" type=\"boolean\"",
            "<cocoa key=\"waitAfterCommand\"/>",
            "<property name=\"environment variables\" code=\"GScE\"",
            "<type type=\"text\" list=\"yes\"/>",
            "<cocoa key=\"environmentVariables\"/>",
            "<command name=\"new surface configuration\" code=\"GhstNSCf\"",
            "<cocoa class=\"BTScriptNewSurfaceConfigurationCommand\"/>",
            "<parameter name=\"from\" code=\"GScS\" type=\"surface configuration\" optional=\"yes\"",
            "<result type=\"surface configuration\"",
            "<parameter name=\"with configuration\" code=\"GNwS\" type=\"surface configuration\" optional=\"yes\"",
            "<parameter name=\"with configuration\" code=\"GNtS\" type=\"surface configuration\" optional=\"yes\"",
            "<parameter name=\"with configuration\" code=\"GSpS\" type=\"surface configuration\" optional=\"yes\"",
        ] {
            assert!(SDEF.contains(needle), "sdef에 없다: {needle}");
        }
        // 세 명령의 인자는 모두 cocoa key `configuration`이다(vendor가 한 키로 읽는다)
        let command = |name: &str| -> &str {
            let start = SDEF.find(&format!("<command name=\"{name}\"")).expect(name);
            let len = SDEF[start..].find("</command>").expect(name);
            &SDEF[start..start + len]
        };
        for (name, code) in [("new window", "GNwS"), ("new tab", "GNtS"), ("split", "GSpS")] {
            let c = command(name);
            let at = c.find(&format!("code=\"{code}\"")).expect(name);
            assert!(c[at..].contains("<cocoa key=\"configuration\"/>"), "{name}");
        }
        assert!(command("new surface configuration").contains("<cocoa key=\"configuration\"/>"));
    }

    /// 레코드의 크기 상한은 vendor(복사 전)와 앱이 같은 값이다(#666)
    #[test]
    fn config_limits_are_the_same_in_vendor_and_app() {
        assert_eq!(
            winit::platform::macos::SCRIPT_CONFIG_LIMITS,
            (
                script_config::MAX_TEXT_BYTES,
                script_config::MAX_INPUT_BYTES,
                script_config::MAX_ENV_COUNT
            )
        );
        // 처음 입력의 상한은 `input text`와 같다
        assert_eq!(script_config::MAX_INPUT_BYTES, MAX_TEXT);
    }

    /// 명령을 로그에 찍어도 레코드의 값(명령·폴더·처음 입력·환경 변수)은 나오지 않는다. 길이와 개수만 나온다
    #[test]
    fn command_debug_hides_configuration_values() {
        use winit::platform::macos::ScriptCommand as C;
        let secret = "hunter2-토큰";
        let config = || {
            Some(ScriptConfig {
                font_size: Some(12.0),
                working_directory: Some(format!("/{secret}")),
                command: Some(format!("echo {secret}")),
                initial_input: Some(secret.into()),
                wait_after_command: Some(true),
                environment: Some(vec![format!("API_TOKEN={secret}")]),
            })
        };
        let cmds = [
            C::NewWindow { config: config() },
            C::NewTab {
                window: Some("1".into()),
                config: config(),
            },
            C::Split {
                terminal: "3".into(),
                direction: ScriptSplit::Down,
                config: config(),
            },
            C::NewSurfaceConfiguration { from: config() },
        ];
        for cmd in cmds {
            let shown = format!("{cmd:?}");
            for hidden in ["hunter2", "토큰", "API_TOKEN", "echo"] {
                assert!(!shown.contains(hidden), "{shown}");
            }
            assert!(
                shown.contains("command_bytes") && shown.contains("environment_count"),
                "{shown}"
            );
        }
    }

    /// 레코드의 크기 상한은 `admit`이 한 번 더 본다. 세 명령과 `new surface configuration` 모두
    #[test]
    fn configuration_sizes_are_capped_before_anything_is_created() {
        use winit::platform::macos::ScriptCommand as C;
        let config = |command: usize, input: usize, env: usize| {
            Some(ScriptConfig {
                command: Some("x".repeat(command)),
                initial_input: Some("y".repeat(input)),
                environment: Some((0..env).map(|i| format!("K{i}=v")).collect()),
                ..Default::default()
            })
        };
        let all = |c: Option<ScriptConfig>| {
            [
                C::NewWindow { config: c.clone() },
                C::NewTab {
                    window: None,
                    config: c.clone(),
                },
                C::Split {
                    terminal: "1".into(),
                    direction: ScriptSplit::Right,
                    config: c.clone(),
                },
                C::NewSurfaceConfiguration { from: c },
            ]
        };
        let (t, i, e) = (
            script_config::MAX_TEXT_BYTES,
            script_config::MAX_INPUT_BYTES,
            script_config::MAX_ENV_COUNT,
        );
        for cmd in all(config(t, i, e)) {
            assert!(admit(true, cmd).is_ok());
        }
        for over in [config(t + 1, 0, 0), config(0, i + 1, 0), config(0, 0, e + 1)] {
            for cmd in all(over) {
                assert!(admit(true, cmd).is_err());
            }
        }
        // 꺼져 있으면 레코드가 맞아도 거절한다
        for cmd in all(config(1, 1, 1)) {
            assert!(admit(false, cmd).is_err());
        }
    }

    /// vendor의 레코드와 앱의 레코드는 값이 그대로 오간다
    #[test]
    fn config_records_convert_both_ways() {
        let c = ScriptConfig {
            font_size: Some(14.0),
            working_directory: Some("/tmp".into()),
            command: Some("ls".into()),
            initial_input: Some("x".into()),
            wait_after_command: Some(true),
            environment: Some(vec!["A=b".into()]),
        };
        assert_eq!(script_config_of(raw_config(&c)), c);
        assert_eq!(
            config_of(&ScriptCommand::NewSurfaceConfiguration { from: Some(c.clone()) }),
            Some(&c)
        );
        assert_eq!(config_of(&ScriptCommand::NewWindow { config: None }), None);
        assert_eq!(config_of(&ScriptCommand::Snapshot), None);
    }

    /// 레코드가 틀리면 창이나 칸을 만들기 전에 오류다(`script_overrides`는 `run_script`의 맨 앞에서 불린다)
    #[test]
    fn a_bad_record_is_an_error_before_anything_is_created() {
        let bad = |c: ScriptConfig| App::script_overrides(Some(c));
        assert!(
            bad(ScriptConfig {
                environment: Some(vec!["NOEQUALS".into()]),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            bad(ScriptConfig {
                font_size: Some(f64::NAN),
                ..Default::default()
            })
            .is_err()
        );
        assert!(
            bad(ScriptConfig {
                font_size: Some(-1.0),
                ..Default::default()
            })
            .is_err()
        );
        // 레코드가 없으면 덮어쓰기도 없다(사람의 새 칸·btctl new와 같다)
        assert_eq!(App::script_overrides(None), Ok(None));
        // 맞는 레코드는 덮어쓰기가 된다
        let ok = App::script_overrides(Some(ScriptConfig {
            command: Some("echo hi".into()),
            ..Default::default()
        }))
        .unwrap()
        .unwrap();
        assert!(ok.wait_after && ok.program.as_deref() == Some("/bin/sh"));
    }

    /// 레코드는 `btctl` 소켓 규약을 넓히지 않는다: 세 명령의 `Call::New`는 그대로이고 덮어쓰기는 앱 안에서만 쓴다
    #[test]
    fn configuration_does_not_widen_the_btctl_call() {
        use bt_core::proto::Call;
        // 분할은 레코드가 있어도 같은 호출이다(`split_is_the_same_call_as_btctl_new_split`)
        assert_eq!(
            split_call(7, ScriptSplit::Right),
            Call::New {
                cwd: None,
                command: Vec::new(),
                split: Some("right".into()),
                tab: Some(7),
                window: false,
            }
        );
    }

    fn pane(id: u64, title: &str) -> PaneView {
        PaneView {
            id,
            title: title.into(),
            title_override: String::new(),
            pid: 0,
            tty: String::new(),
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
                    title_override: String::new(),
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
                        title_override: String::new(),
                        focus: 1,
                        panes: vec![pane(1, "vim")],
                    },
                    TabView {
                        title: "split".into(),
                        title_override: String::new(),
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
            C::NewWindow { config: None },
            C::NewTab {
                window: None,
                config: None,
            },
            C::NewSurfaceConfiguration { from: None },
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
                config: None,
            },
            C::Close { terminal: "1".into() },
            C::CloseTab { tab: "1".into() },
            C::CloseWindow { window: "1".into() },
            C::SetTabTitleOverride {
                tab: "1".into(),
                title: "빌드".into(),
            },
            C::SetTerminalTitleOverride {
                terminal: "1".into(),
                title: String::new(),
            },
            C::SendMouseButton {
                terminal: "1".into(),
                button: winit::platform::macos::ScriptMouseButton::Left,
                release: false,
                modifiers: None,
            },
            C::SendMousePosition {
                terminal: "1".into(),
                x: 1.0,
                y: 2.0,
                modifiers: None,
            },
            C::SendMouseScroll {
                terminal: "1".into(),
                x: 0.0,
                y: 1.0,
                precision: false,
                momentum: winit::platform::macos::ScriptMomentum::None,
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

    /// 마우스 명령 셋의 Debug에는 수식 키 글이 나오지 않는다(길이만)
    #[test]
    fn command_debug_hides_mouse_modifiers() {
        use winit::platform::macos::{ScriptCommand as C, ScriptMouseButton};
        let cmds = [
            C::SendMouseButton {
                terminal: "3".into(),
                button: ScriptMouseButton::Left,
                release: false,
                modifiers: Some("secretmod".into()),
            },
            C::SendMousePosition {
                terminal: "3".into(),
                x: 1.0,
                y: 2.0,
                modifiers: Some("secretmod".into()),
            },
        ];
        for cmd in &cmds {
            let shown = format!("{cmd:?}");
            assert!(!shown.contains("secretmod"), "{shown}");
            assert!(
                shown.contains("modifier_bytes") && shown.contains("terminal"),
                "{shown}"
            );
        }
    }

    /// 수식 키 글은 `send key`와 같은 상한으로 복사하기 전에 거른다. 좌표와 휠 값은 유한해야 한다
    #[test]
    fn mouse_commands_are_checked_before_they_are_used() {
        use winit::platform::macos::{ScriptCommand as C, ScriptMomentum, ScriptMouseButton};
        let button = |m: Option<String>| C::SendMouseButton {
            terminal: "1".into(),
            button: ScriptMouseButton::Right,
            release: true,
            modifiers: m,
        };
        assert!(admit(true, button(Some("shift,control".into()))).is_ok());
        assert!(admit(true, button(Some("x".repeat(MAX_KEY)))).is_ok());
        assert!(admit(true, button(Some("x".repeat(MAX_KEY + 1)))).is_err());
        let pos = |x: f64, y: f64, m: Option<String>| C::SendMousePosition {
            terminal: "1".into(),
            x,
            y,
            modifiers: m,
        };
        assert!(admit(true, pos(10.0, 20.5, None)).is_ok());
        assert!(
            admit(true, pos(-3.0, 1e300, None)).is_ok(),
            "칸 밖 값은 칸 안으로 자른다"
        );
        assert!(admit(true, pos(0.0, 0.0, Some("y".repeat(MAX_KEY + 1)))).is_err());
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(admit(true, pos(bad, 0.0, None)).is_err(), "x {bad}");
            assert!(admit(true, pos(0.0, bad, None)).is_err(), "y {bad}");
        }
        let scroll = |x: f64, y: f64| C::SendMouseScroll {
            terminal: "1".into(),
            x,
            y,
            precision: true,
            momentum: ScriptMomentum::Changed,
        };
        assert!(admit(true, scroll(0.0, -4.5)).is_ok());
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(admit(true, scroll(bad, 0.0)).is_err(), "x {bad}");
            assert!(admit(true, scroll(0.0, bad)).is_err(), "y {bad}");
        }
    }

    /// 새 열거형의 네 글자 코드는 Ghostty와 같고 vendor가 같은 코드를 읽는다
    #[test]
    fn mouse_enumerations_match_ghostty() {
        use winit::platform::macos::{ScriptMomentum as M, ScriptMouseButton as B};
        let code = |s: &[u8; 4]| u32::from_be_bytes(*s);
        assert_eq!(B::from_code(code(b"GMlf")), Some(B::Left));
        assert_eq!(B::from_code(code(b"GMrt")), Some(B::Right));
        assert_eq!(B::from_code(code(b"GMmd")), Some(B::Middle));
        assert_eq!(B::from_code(code(b"GIpr")), None);
        assert_eq!(B::from_code(0), None);
        for (word, want) in [
            (b"SMno", M::None),
            (b"SMbg", M::Began),
            (b"SMch", M::Changed),
            (b"SMen", M::Ended),
            (b"SMcn", M::Cancelled),
            (b"SMmb", M::MayBegin),
            (b"SMst", M::Stationary),
        ] {
            assert_eq!(M::from_code(code(word)), want);
        }
        // 모르는 값은 none이다(Ghostty `ScriptMouseScrollCommand`의 default)
        assert_eq!(M::from_code(0), M::None);
        for needle in [
            "<enumeration name=\"mouse button\" code=\"GMBt\"",
            "<enumerator name=\"left button\" code=\"GMlf\"",
            "<enumerator name=\"right button\" code=\"GMrt\"",
            "<enumerator name=\"middle button\" code=\"GMmd\"",
            "<enumeration name=\"scroll momentum\" code=\"GSMo\"",
            "<enumerator name=\"none\" code=\"SMno\"",
            "<enumerator name=\"began\" code=\"SMbg\"",
            "<enumerator name=\"changed\" code=\"SMch\"",
            "<enumerator name=\"ended\" code=\"SMen\"",
            "<enumerator name=\"cancelled\" code=\"SMcn\"",
            "<enumerator name=\"may begin\" code=\"SMmb\"",
            "<enumerator name=\"stationary\" code=\"SMst\"",
        ] {
            assert!(SDEF.contains(needle), "sdef에 없다: {needle}");
        }
    }

    /// 마우스 명령의 이름·네 글자 코드·인자 코드는 Ghostty 사전과 같다. 결과는 없다. 클래스는 `BTScript…Command`다
    #[test]
    fn mouse_commands_match_the_ghostty_dictionary() {
        let command = |name: &str| -> &str {
            let start = SDEF.find(&format!("<command name=\"{name}\"")).expect(name);
            let len = SDEF[start..].find("</command>").expect(name);
            &SDEF[start..start + len]
        };
        let button = command("send mouse button");
        for needle in [
            "<command name=\"send mouse button\" code=\"GhstSMBt\"",
            "<cocoa class=\"BTScriptMouseButtonCommand\"/>",
            "<direct-parameter type=\"mouse button\"",
            "<parameter name=\"action\" code=\"GMbA\" type=\"input action\" optional=\"yes\"",
            "<parameter name=\"modifiers\" code=\"GMbM\" type=\"text\" optional=\"yes\"",
            "<parameter name=\"to\" code=\"GMbT\" type=\"terminal\"",
        ] {
            assert!(button.contains(needle), "send mouse button에 없다: {needle}");
        }
        let position = command("send mouse position");
        for needle in [
            "<command name=\"send mouse position\" code=\"GhstSMPs\"",
            "<cocoa class=\"BTScriptMousePosCommand\"/>",
            "<parameter name=\"x\" code=\"GMpX\" type=\"real\"",
            "<parameter name=\"y\" code=\"GMpY\" type=\"real\"",
            "<parameter name=\"modifiers\" code=\"GMpM\" type=\"text\" optional=\"yes\"",
            "<parameter name=\"to\" code=\"GMpT\" type=\"terminal\"",
        ] {
            assert!(position.contains(needle), "send mouse position에 없다: {needle}");
        }
        let scroll = command("send mouse scroll");
        for needle in [
            "<command name=\"send mouse scroll\" code=\"GhstSMSc\"",
            "<cocoa class=\"BTScriptMouseScrollCommand\"/>",
            "<parameter name=\"x\" code=\"GMsX\" type=\"real\"",
            "<parameter name=\"y\" code=\"GMsY\" type=\"real\"",
            "<parameter name=\"precision\" code=\"GMsP\" type=\"boolean\" optional=\"yes\"",
            "<parameter name=\"momentum\" code=\"GMsM\" type=\"scroll momentum\" optional=\"yes\"",
            "<parameter name=\"to\" code=\"GMsT\" type=\"terminal\"",
        ] {
            assert!(scroll.contains(needle), "send mouse scroll에 없다: {needle}");
        }
        // Ghostty는 이 명령들이 결과를 돌려주지 않는다
        for c in [button, position, scroll] {
            assert!(!c.contains("<result"), "마우스 명령에는 결과가 없다");
        }
    }

    /// 마우스 명령의 거절 까닭 글은 한 일에 맞다
    #[test]
    fn mouse_is_refused_while_a_bar_is_open() {
        let why = overlay_refusal_for("마우스를 보내지", true, false).expect("확인 막대");
        assert!(why.contains("확인") && why.contains("마우스를 보내지"), "{why}");
        assert!(overlay_refusal_for("마우스를 보내지", false, true).is_some_and(|m| m.contains("검색")));
        assert_eq!(overlay_refusal_for("마우스를 보내지", false, false), None);
    }
}
