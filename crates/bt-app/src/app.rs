//! 창 하나, 탭 여럿. winit 이벤트를 받아 PTY에 쓰고 화면을 그린다

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bt_core::{AgentKind, AgentState, Theme};
use bt_render::{Canvas, CursorStyle, DrawCell, DrawCursor, FontConfig, Fonts, GridView, draw_grid};
use bt_term::{CellFlags, CursorShape, GridSize, Screen, SelectKind, Session, SessionEvent, SpawnOptions};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::keys::{self, Shortcut};
use crate::mouse;
use crate::sidebar;

/// 다른 스레드(PTY 읽기)가 창 스레드를 깨우는 사건
#[derive(Debug, Clone)]
pub enum UserEvent {
    /// 어떤 탭의 화면이 바뀌었다(여러 번 와도 한 번 그린다)
    Redraw,
    Session(u64, SessionEvent),
    /// 제어 소켓 요청. 답은 함께 온 통로로 보낸다
    Control(
        bt_core::proto::Request,
        std::sync::mpsc::SyncSender<bt_core::proto::Response>,
    ),
}

/// 앱 설정. 설정 파일 읽기는 #13
#[derive(Debug, Clone)]
pub struct AppConfig {
    pub font: FontConfig,
    pub theme: Theme,
    /// macOS Option 키를 Alt(ESC 접두)로 쓴다
    pub option_as_alt: bool,
    pub padding: f32,
    pub scrollback: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            font: FontConfig::default(),
            theme: Theme::default(),
            option_as_alt: true,
            padding: 8.0,
            scrollback: 10_000,
        }
    }
}

pub struct Tab {
    pub id: u64,
    pub session: Session,
    pub title: String,
    pub exited: Option<Option<u32>>,
    /// 전경 프로세스 이름(1초마다 갱신). pid가 같으면 다시 묻지 않는다(node 스크립트는 ps를 부른다)
    pub fg_name: Option<String>,
    fg_pid: Option<i32>,
    /// 보이는 에이전트(프로세스 이름 또는 훅이 알려 준 것)
    pub agent: Option<AgentKind>,
    /// 전경 프로세스 이름으로 알아본 에이전트
    proc_agent: Option<AgentKind>,
    pub state: AgentState,
    /// 지금 하는 일 한 줄(훅이 알려 준 도구·대상)
    pub activity: Option<String>,
    /// 보지 않은 알림이 있다
    pub unread: bool,
    /// 마지막 알림 한 줄
    pub last_note: Option<String>,
    /// 셸이 OSC 7로 알린 작업 폴더
    pub cwd: Option<std::path::PathBuf>,
    /// 훅(btctl)이 상태를 알려 온 적이 있다. 있으면 OSC로 상태를 추측하지 않는다
    pub hooked: bool,
    /// 마지막으로 데스크톱 알림을 띄운 때(탭마다 묶는다)
    last_desktop: Option<Instant>,
}

impl Tab {
    fn sidebar_title(&self) -> String {
        if let Some(agent) = self.agent {
            return agent.name().to_string();
        }
        if !self.title.is_empty() {
            return self.title.clone();
        }
        self.fg_name.clone().unwrap_or_else(|| "셸".into())
    }

    fn sidebar_detail(&self) -> String {
        if self.unread {
            if let Some(n) = &self.last_note {
                return n.clone();
            }
        }
        if self.agent.is_some() || self.state != AgentState::Idle {
            return match &self.activity {
                Some(a) if self.state == AgentState::Working => a.clone(),
                _ => self.state.label().to_string(),
            };
        }
        if self.agent.is_none() && !self.title.is_empty() {
            return self.fg_name.clone().unwrap_or_default();
        }
        String::new()
    }
}

struct Gfx {
    window: Rc<Window>,
    surface: softbuffer::Surface<Rc<Window>, Rc<Window>>,
    _context: softbuffer::Context<Rc<Window>>,
}

pub struct App {
    config: AppConfig,
    proxy: EventLoopProxy<UserEvent>,
    redraw_pending: Arc<AtomicBool>,
    gfx: Option<Gfx>,
    fonts: Option<Fonts>,
    font_size: f32,
    scale: f64,
    tabs: Vec<Tab>,
    active: usize,
    next_id: u64,
    mods: ModifiersState,
    preedit: String,
    cursor_pos: PhysicalPosition<f64>,
    selecting: bool,
    last_click: Option<(Instant, usize, usize, u8)>,
    clipboard: Option<arboard::Clipboard>,
    scroll_accum: f64,
    first: Option<SpawnOptions>,
    focused: bool,
    sidebar_visible: bool,
    next_poll: Instant,
    control: Option<crate::control::ControlServer>,
    /// 마지막으로 받은 구독 한도 사용량(출처, 창들). #11에서 그린다
    pub usage: Option<(String, Vec<bt_core::proto::UsageWindow>)>,
}

impl App {
    pub fn new(config: AppConfig, proxy: EventLoopProxy<UserEvent>, first: SpawnOptions) -> Self {
        let font_size = config.font.size;
        Self {
            config,
            proxy,
            redraw_pending: Arc::new(AtomicBool::new(false)),
            gfx: None,
            fonts: None,
            font_size,
            scale: 1.0,
            tabs: Vec::new(),
            active: 0,
            next_id: 1,
            mods: ModifiersState::empty(),
            preedit: String::new(),
            cursor_pos: PhysicalPosition::new(0.0, 0.0),
            selecting: false,
            last_click: None,
            clipboard: arboard::Clipboard::new().ok(),
            scroll_accum: 0.0,
            first: Some(first),
            focused: true,
            sidebar_visible: true,
            next_poll: Instant::now(),
            control: None,
            usage: None,
        }
    }

    fn load_fonts(&mut self) {
        let cfg = FontConfig {
            size: self.font_size,
            scale: self.scale as f32,
            ..self.config.font.clone()
        };
        match Fonts::new(&cfg) {
            Ok(f) => {
                log::info!("글꼴: {} {}pt × {}", f.family_name, self.font_size, self.scale);
                self.fonts = Some(f);
            }
            Err(e) => {
                log::error!("{e}");
                self.fonts = None;
            }
        }
    }

    /// 격자를 그릴 자리(픽셀): x, y, 폭, 높이
    fn grid_rect(&self) -> (i32, i32, u32, u32) {
        let Some(g) = &self.gfx else { return (0, 0, 0, 0) };
        let size = g.window.inner_size();
        let pad = self.pad();
        let x = self.sidebar_width() as i32 + pad;
        let y = pad;
        let w = (size.width as i32 - x - pad).max(0) as u32;
        let h = (size.height as i32 - y - pad).max(0) as u32;
        (x, y, w, h)
    }

    fn pad(&self) -> i32 {
        (self.config.padding as f64 * self.scale).round() as i32
    }

    fn sidebar_width(&self) -> u32 {
        match (&self.fonts, self.sidebar_visible) {
            (Some(f), true) => sidebar::width_px(f, self.pad()),
            _ => 0,
        }
    }

    /// 다음으로 볼 탭: 지금 탭이 아닌 것 중 가장 급한 것(같으면 오래 기다린 순서를 모르니 앞 탭)
    fn next_attention(&self) -> Option<usize> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(i, t)| *i != self.active && (t.unread || t.state.urgency() >= 3))
            .max_by_key(|(i, t)| (t.state.urgency(), t.unread, std::cmp::Reverse(*i)))
            .map(|(i, _)| i)
    }

    fn activate(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
            let t = &mut self.tabs[index];
            t.unread = false;
            // 끝남은 본 순간 쉼으로 돌린다. 기다림은 사람이 답할 때까지 둔다
            if t.state == AgentState::Done {
                t.state = AgentState::Idle;
            }
            self.request_redraw();
        }
    }

    /// 전경 프로세스로 에이전트를 알아본다(훅이 없어도 이름만은 보이게). 바뀐 것이 있으면 참
    fn poll_foreground(&mut self) -> bool {
        let mut changed = false;
        for t in &mut self.tabs {
            let Some(pid) = t.session.foreground_pid() else {
                continue;
            };
            if t.fg_pid == Some(pid) {
                continue;
            }
            t.fg_pid = Some(pid);
            let name = bt_term::process::process_name(pid);
            let proc_agent = name.as_deref().and_then(AgentKind::from_process);
            if proc_agent.is_none() && t.proc_agent.is_some() {
                // 프로세스로 알아본 에이전트가 끝나 셸로 돌아왔다. 훅이 알려 준 상태도 함께 끝낸다
                t.state = AgentState::Idle;
                t.activity = None;
                t.agent = None;
                t.hooked = false;
            }
            if proc_agent.is_some() {
                t.agent = proc_agent;
            }
            // 훅이 알려 준 에이전트(전경 이름이 node·셸 스크립트라 못 알아본 경우)는 지우지 않는다
            t.proc_agent = proc_agent;
            t.fg_name = name;
            changed = true;
        }
        changed
    }

    /// 탭이 알림을 보냈다. 보고 있는 탭이 아니면 안 읽음으로 두고, 창을 보고 있지 않으면 데스크톱 알림을 띄운다
    fn on_notification(&mut self, idx: usize, note: bt_term::Notification) {
        let watching = self.focused && idx == self.active;
        let line = match (note.title.is_empty(), note.body.is_empty()) {
            (false, false) => format!("{}: {}", note.title, note.body),
            (true, _) => note.body.clone(),
            (false, true) => note.title.clone(),
        };
        let t = &mut self.tabs[idx];
        t.last_note = Some(line);
        // 훅이 없는 에이전트 탭의 알림은 "사람을 기다린다"는 뜻으로 본다(Claude Code·Codex가 끝나거나 물을 때 보낸다)
        if !t.hooked && t.agent.is_some() {
            t.state = AgentState::Waiting(bt_core::WaitReason::Input);
        }
        if !watching {
            t.unread = true;
            // 같은 탭의 데스크톱 알림은 3초에 한 번(출력에 알림이 몰려도 화면을 덮지 않게)
            let now = Instant::now();
            if t.last_desktop.is_none_or(|at| now - at >= Duration::from_secs(3)) {
                t.last_desktop = Some(now);
                let title = if note.title.is_empty() {
                    t.sidebar_title()
                } else {
                    note.title.clone()
                };
                crate::desktop::notify(&title, &note.body);
            }
        }
        self.request_redraw();
    }

    fn grid_size(&self) -> GridSize {
        let Some(f) = &self.fonts else {
            return GridSize::new(80, 24);
        };
        let m = f.metrics();
        let (_, _, w, h) = self.grid_rect();
        GridSize {
            cols: (w / m.width).max(2) as u16,
            rows: (h / m.height).max(1) as u16,
            cell_width: m.width as u16,
            cell_height: m.height as u16,
        }
    }

    fn sink(&self, id: u64) -> bt_term::EventSink {
        let proxy = self.proxy.clone();
        let pending = self.redraw_pending.clone();
        Arc::new(move |e: SessionEvent| match e {
            SessionEvent::Output => {
                // 출력이 몰려도 깨우기는 한 번만 보낸다
                if !pending.swap(true, Ordering::AcqRel) {
                    let _ = proxy.send_event(UserEvent::Redraw);
                }
            }
            other => {
                let _ = proxy.send_event(UserEvent::Session(id, other));
            }
        })
    }

    pub fn open_tab(&mut self, options: SpawnOptions) -> Option<u64> {
        let id = self.next_id;
        self.next_id += 1;
        let size = self.grid_size();
        let mut options = SpawnOptions {
            scrollback: self.config.scrollback,
            ..options
        };
        // 탭 안의 btctl·훅이 이 터미널과 이 탭을 찾게 한다
        if let Some(c) = &self.control {
            options
                .env
                .push((bt_core::proto::ENV_SOCKET.into(), c.path.to_string_lossy().into_owned()));
        }
        options.env.push((bt_core::proto::ENV_TAB.into(), id.to_string()));
        match Session::spawn(options, size, self.config.theme.clone(), self.sink(id)) {
            Ok(session) => {
                self.tabs.push(Tab {
                    id,
                    session,
                    title: String::new(),
                    exited: None,
                    fg_name: None,
                    fg_pid: None,
                    agent: None,
                    proc_agent: None,
                    state: AgentState::Idle,
                    activity: None,
                    unread: false,
                    last_note: None,
                    cwd: None,
                    hooked: false,
                    last_desktop: None,
                });
                self.active = self.tabs.len() - 1;
                self.request_redraw();
                Some(id)
            }
            Err(e) => {
                log::error!("셸을 띄우지 못했습니다: {e}");
                None
            }
        }
    }

    /// 제어 소켓 요청 하나를 처리한다
    fn handle_call(&mut self, req: bt_core::proto::Request) -> bt_core::proto::Response {
        use bt_core::proto::{Call, Response, TabInfo};
        use serde_json::json;
        let id = req.id;
        // tab을 주지 않으면 지금 보는 탭
        let pick = |app: &App, tab: Option<u64>| -> Result<usize, Response> {
            match tab {
                Some(t) => app
                    .tabs
                    .iter()
                    .position(|x| x.id == t)
                    .ok_or_else(|| Response::err(id, format!("탭 {t}이 없습니다"))),
                None if !app.tabs.is_empty() => Ok(app.active),
                None => Err(Response::err(id, "탭이 없습니다")),
            }
        };
        match req.call {
            Call::Ping => Response::ok(id, json!({ "app": "b-terminal", "version": env!("CARGO_PKG_VERSION") })),
            Call::List => {
                let tabs: Vec<TabInfo> = self
                    .tabs
                    .iter()
                    .enumerate()
                    .map(|(i, t)| TabInfo {
                        id: t.id,
                        title: t.sidebar_title(),
                        active: i == self.active,
                        state: t.state,
                        agent: t.agent.map(|a| a.name().to_string()),
                        activity: if t.state == AgentState::Working {
                            t.activity.clone()
                        } else {
                            t.last_note.clone()
                        },
                        unread: t.unread,
                        cwd: t.cwd.clone(),
                        process: t.fg_name.clone(),
                    })
                    .collect();
                Response::ok(id, serde_json::to_value(tabs).unwrap_or_default())
            }
            Call::New { cwd, command } => {
                let mut opts = SpawnOptions {
                    cwd: cwd.filter(|c| c.is_dir()),
                    ..Default::default()
                };
                if let Some((prog, args)) = command.split_first() {
                    opts.program = Some(prog.clone());
                    opts.args = args.to_vec();
                }
                match self.open_tab(opts) {
                    Some(tab) => Response::ok(id, json!({ "tab": tab })),
                    None => Response::err(id, "탭을 열지 못했습니다"),
                }
            }
            Call::Focus { tab } => match pick(self, Some(tab)) {
                Ok(i) => {
                    self.activate(i);
                    Response::ok(id, json!({}))
                }
                Err(r) => r,
            },
            Call::Send { tab, text } => match pick(self, tab) {
                Ok(i) => {
                    self.tabs[i].session.write(text.as_bytes());
                    Response::ok(id, json!({}))
                }
                Err(r) => r,
            },
            Call::Read { tab } => match pick(self, tab) {
                Ok(i) => Response::ok(id, json!({ "text": self.tabs[i].session.snapshot().text() })),
                Err(r) => r,
            },
            Call::Notify { tab, title, body } => match pick(self, tab) {
                Ok(i) => {
                    self.on_notification(i, bt_term::Notification { title, body });
                    Response::ok(id, json!({}))
                }
                Err(r) => r,
            },
            Call::Status {
                tab,
                state,
                activity,
                agent,
            } => match pick(self, tab) {
                Ok(i) => {
                    self.set_status(i, state, activity, agent);
                    Response::ok(id, json!({}))
                }
                Err(r) => r,
            },
            Call::Usage { source, windows } => {
                self.usage = Some((source, windows));
                self.request_redraw();
                Response::ok(id, json!({}))
            }
        }
    }

    /// 사이드바 아래에 그릴 사용량. 리셋 시각이 지난 창은 버린다(Claude Code도 그 창을 지운다)
    fn usage_rows(&self) -> Vec<sidebar::UsageRow> {
        let Some((_, windows)) = &self.usage else {
            return Vec::new();
        };
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        windows
            .iter()
            .filter(|w| w.resets_at.is_none_or(|r| r > now))
            .map(|w| sidebar::UsageRow {
                label: match w.kind.as_str() {
                    "five_hour" => "5시간".into(),
                    "seven_day" => "주간".into(),
                    "spend_limit" => "지출".into(),
                    other => other.into(),
                },
                percent: w.used_percent,
                resets_in: w.resets_at.map(|r| sidebar::short_duration(r - now)),
            })
            .collect()
    }

    /// 훅이 알린 상태. 보고 있지 않은 탭이 기다림·끝남·오류가 되면 안 읽음으로 둔다
    fn set_status(&mut self, idx: usize, state: AgentState, activity: Option<String>, agent: Option<String>) {
        let watching = self.focused && idx == self.active;
        let t = &mut self.tabs[idx];
        t.hooked = true;
        if let Some(kind) = agent.as_deref().and_then(AgentKind::from_process) {
            t.agent = Some(kind);
        }
        let attention = state.urgency() >= 3 && t.state != state;
        t.state = state;
        let clean = activity.map(|a| bt_core::proto::printable(&a).chars().take(200).collect::<String>());
        if state == AgentState::Working {
            t.activity = clean;
        } else {
            // 작업이 끝났으면 지난 도구 줄은 지우고, 기다리는 까닭(권한을 묻는 명령)이나 마지막 답을 알림 줄로 둔다
            t.activity = None;
            if clean.is_some() {
                t.last_note = clean;
            }
        }
        if attention && !watching {
            t.unread = true;
        }
        self.request_redraw();
    }

    fn close_tab(&mut self, index: usize, event_loop: &ActiveEventLoop) {
        if index >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(index);
        // Session을 떨어뜨리면 PTY가 닫히고 셸은 SIGHUP을 받는다
        drop(tab);
        if self.tabs.is_empty() {
            event_loop.exit();
            return;
        }
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len() - 1;
        }
        self.request_redraw();
    }

    fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    fn request_redraw(&self) {
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }

    fn write_active(&mut self, bytes: &[u8]) {
        if let Some(t) = self.active_tab() {
            t.session.select_clear();
            t.session.write(bytes);
        }
    }

    fn resize_all(&mut self) {
        let size = self.grid_size();
        for t in &self.tabs {
            t.session.resize(size);
        }
        self.request_redraw();
    }

    fn set_font_size(&mut self, size: f32) {
        self.font_size = size.clamp(6.0, 72.0);
        self.load_fonts();
        self.resize_all();
    }

    /// 창 좌표 → 격자 칸
    fn cell_at(&self, pos: PhysicalPosition<f64>) -> Option<(usize, usize)> {
        let f = self.fonts.as_ref()?;
        let m = f.metrics();
        let (x, y, _, _) = self.grid_rect();
        let size = self.grid_size();
        let col = ((pos.x - x as f64) / m.width as f64).floor().max(0.0) as usize;
        let row = ((pos.y - y as f64) / m.height as f64).floor().max(0.0) as usize;
        Some((col.min(size.cols as usize - 1), row.min(size.rows as usize - 1)))
    }

    fn handle_shortcut(&mut self, s: Shortcut, event_loop: &ActiveEventLoop) {
        match s {
            Shortcut::Copy => {
                if let Some(text) = self.active_tab().and_then(|t| t.session.selection_text()) {
                    if let Some(cb) = self.clipboard.as_mut() {
                        let _ = cb.set_text(text);
                    }
                }
            }
            Shortcut::Paste => {
                let text = self.clipboard.as_mut().and_then(|cb| cb.get_text().ok());
                if let (Some(text), Some(t)) = (text, self.active_tab()) {
                    let bytes = bt_term::encode_paste(&text, t.session.modes().bracketed_paste);
                    self.write_active(&bytes);
                }
            }
            Shortcut::FontBigger => self.set_font_size(self.font_size + 1.0),
            Shortcut::FontSmaller => self.set_font_size(self.font_size - 1.0),
            Shortcut::FontReset => self.set_font_size(self.config.font.size),
            Shortcut::NewTab => {
                // 새 탭은 지금 탭의 폴더에서 연다(OSC 7로 알려진 것, 없으면 프로세스에게 묻는다)
                let cwd = self
                    .active_tab()
                    .and_then(|t| t.cwd.clone().or_else(|| t.session.cwd()));
                self.open_tab(SpawnOptions {
                    cwd,
                    ..Default::default()
                });
            }
            Shortcut::CloseTab => self.close_tab(self.active, event_loop),
            Shortcut::NextTab if !self.tabs.is_empty() => self.activate((self.active + 1) % self.tabs.len()),
            Shortcut::PrevTab if !self.tabs.is_empty() => {
                self.activate((self.active + self.tabs.len() - 1) % self.tabs.len())
            }
            Shortcut::GotoTab(i) if i < self.tabs.len() => self.activate(i),
            Shortcut::ClearScrollback => self.write_active(b"\x0c"),
            Shortcut::ScrollPageUp | Shortcut::ScrollPageDown => {
                let rows = self.grid_size().rows as i32;
                if let Some(t) = self.active_tab() {
                    t.session.scroll(if s == Shortcut::ScrollPageUp {
                        rows - 1
                    } else {
                        -(rows - 1)
                    });
                }
            }
            Shortcut::ScrollTop => {
                if let Some(t) = self.active_tab() {
                    t.session.scroll(i32::MAX / 2);
                }
            }
            Shortcut::ScrollBottom => {
                if let Some(t) = self.active_tab() {
                    t.session.scroll_to_bottom();
                }
            }
            Shortcut::ToggleSidebar => {
                self.sidebar_visible = !self.sidebar_visible;
                self.resize_all();
            }
            Shortcut::NextAttention => {
                if let Some(i) = self.next_attention() {
                    self.activate(i);
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    fn mouse_reporting(&self) -> Option<bt_term::Modes> {
        let m = self.active_tab()?.session.modes();
        // Shift를 누르면 앱의 마우스 보고를 건너뛰고 고르기를 한다(xterm 관례)
        (m.mouse_report || m.mouse_drag || m.mouse_motion)
            .then_some(m)
            .filter(|_| !self.mods.shift_key())
    }

    fn send_mouse(&mut self, button: mouse::Button, action: mouse::Action) {
        let Some(m) = self.mouse_reporting() else { return };
        let Some((col, row)) = self.cell_at(self.cursor_pos) else {
            return;
        };
        let bytes = mouse::encode(
            button,
            action,
            col,
            row,
            self.mods.shift_key(),
            self.mods.alt_key(),
            self.mods.control_key(),
            m.sgr_mouse,
        );
        if let Some(t) = self.active_tab() {
            t.session.write(&bytes);
        }
    }

    fn redraw(&mut self) {
        self.redraw_pending.store(false, Ordering::Release);
        let theme = self.config.theme.clone();
        let (gx, gy, _, _) = self.grid_rect();
        let screen = self.active_tab().map(|t| t.session.snapshot());
        let pad = self.pad();
        let usage_rows = self.usage_rows();
        let sidebar_items: Option<Vec<sidebar::Item>> = self.sidebar_visible.then(|| {
            self.tabs
                .iter()
                .enumerate()
                .map(|(i, t)| sidebar::Item {
                    title: t.sidebar_title(),
                    detail: t.sidebar_detail(),
                    state: t.state,
                    unread: t.unread,
                    active: i == self.active,
                })
                .collect()
        });
        let title = self
            .active_tab()
            .map(|t| t.title.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "b-terminal".into());
        let Some(gfx) = self.gfx.as_mut() else { return };
        let Some(fonts) = self.fonts.as_mut() else { return };
        let size = gfx.window.inner_size();
        let (Some(w), Some(h)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else {
            return;
        };
        if gfx.surface.resize(w, h).is_err() {
            return;
        }
        let Ok(mut buffer) = gfx.surface.buffer_mut() else {
            return;
        };
        {
            let mut canvas = Canvas::new(&mut buffer, size.width, size.height);
            let bg = screen.as_ref().map(|s| s.background).unwrap_or(theme.background);
            canvas.clear(bg);
            if let Some(items) = &sidebar_items {
                sidebar::draw(&mut canvas, fonts, &theme, items, pad, &usage_rows);
            }
            if let Some(screen) = &screen {
                let cells = to_draw_cells(screen);
                let cursor = screen.cursor.visible.then_some(DrawCursor {
                    col: screen.cursor.col,
                    row: screen.cursor.row,
                    style: match screen.cursor.shape {
                        CursorShape::Block => CursorStyle::Block,
                        CursorShape::Beam => CursorStyle::Beam,
                        CursorShape::Underline => CursorStyle::Underline,
                        CursorShape::HollowBlock => CursorStyle::Hollow,
                    },
                    wide: screen.cursor.wide,
                    color: theme.cursor,
                    text_color: theme.cursor_text,
                });
                let view = GridView {
                    cols: screen.cols,
                    rows: screen.rows,
                    cells: &cells,
                    cursor,
                    selection_bg: theme.selection_background,
                    preedit: (!self.preedit.is_empty()).then_some(self.preedit.as_str()),
                };
                draw_grid(&mut canvas, fonts, gx, gy, &view);
                // IME 후보 창을 커서 옆에 띄운다
                let m = fonts.metrics();
                let px = gx + (screen.cursor.col as u32 * m.width) as i32;
                let py = gy + (screen.cursor.row as u32 * m.height) as i32;
                gfx.window
                    .set_ime_cursor_area(PhysicalPosition::new(px, py), PhysicalSize::new(m.width, m.height));
            }
        }
        let _ = buffer.present();
        gfx.window.set_title(&title);
    }
}

/// OSC 7 폴더를 새 탭의 시작 폴더로 써도 되는가: 호스트가 이 컴퓨터(빈 값·localhost·호스트 이름)이고 실제 폴더일 때만.
/// ssh 안의 셸이 보낸 원격 경로나 터미널에 찍힌 글이 고른 경로로 새 셸을 열지 않는다
pub fn osc7_is_local(host: &str, path: &std::path::Path) -> bool {
    let local = gethostname::gethostname().to_string_lossy().to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    let short = local.split('.').next().unwrap_or(&local).to_string();
    let host_ok =
        host.is_empty() || host == "localhost" || host == local || host.split('.').next() == Some(short.as_str());
    host_ok && path.is_absolute() && path.is_dir()
}

pub fn to_draw_cells(s: &Screen) -> Vec<DrawCell> {
    s.cells
        .iter()
        .map(|c| DrawCell {
            ch: c.ch,
            fg: c.fg,
            bg: c.bg,
            bold: c.flags.contains(CellFlags::BOLD),
            italic: c.flags.contains(CellFlags::ITALIC),
            underline: c.flags.contains(CellFlags::UNDERLINE) || c.flags.contains(CellFlags::UNDERCURL),
            strike: c.flags.contains(CellFlags::STRIKE),
            wide: c.flags.contains(CellFlags::WIDE),
            spacer: c.flags.contains(CellFlags::SPACER),
            selected: c.flags.contains(CellFlags::SELECTED),
        })
        .collect()
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("b-terminal")
            .with_inner_size(LogicalSize::new(1000.0, 680.0))
            .with_min_inner_size(LogicalSize::new(320.0, 200.0));
        #[cfg(target_os = "macos")]
        let attrs = {
            use winit::platform::macos::{OptionAsAlt, WindowAttributesExtMacOS};
            attrs.with_option_as_alt(if self.config.option_as_alt {
                OptionAsAlt::Both
            } else {
                OptionAsAlt::None
            })
        };
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Rc::new(w),
            Err(e) => {
                log::error!("창을 만들지 못했습니다: {e}");
                event_loop.exit();
                return;
            }
        };
        window.set_ime_allowed(true);
        self.scale = window.scale_factor();
        let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
        let surface = softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface");
        self.gfx = Some(Gfx {
            window,
            surface,
            _context: context,
        });
        self.load_fonts();
        if self.fonts.is_none() {
            event_loop.exit();
            return;
        }
        if self.control.is_none() {
            match crate::control::start(self.proxy.clone()) {
                Ok(c) => {
                    log::info!("제어 소켓: {}", c.path.display());
                    self.control = Some(c);
                }
                Err(e) => log::warn!("제어 소켓을 열지 못했습니다(btctl·훅이 동작하지 않습니다): {e}"),
            }
        }
        if self.tabs.is_empty() {
            let first = self.first.take().unwrap_or_default();
            self.open_tab(first);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Redraw => self.request_redraw(),
            UserEvent::Control(req, reply) => {
                let resp = self.handle_call(req);
                let _ = reply.send(resp);
            }
            UserEvent::Session(id, e) => {
                let Some(idx) = self.tabs.iter().position(|t| t.id == id) else {
                    return;
                };
                match e {
                    SessionEvent::Title(t) => {
                        self.tabs[idx].title = bt_core::proto::printable(&t).chars().take(200).collect()
                    }
                    SessionEvent::Cwd { host, path } => {
                        if osc7_is_local(&host, &path) {
                            self.tabs[idx].cwd = Some(path);
                        }
                    }
                    SessionEvent::Notification(note) => self.on_notification(idx, note),
                    SessionEvent::Progress { state, .. } => {
                        // 진행 표시는 훅이 없는 에이전트 탭에서만 작업 중의 근거로 쓴다
                        let t = &mut self.tabs[idx];
                        if !t.hooked && t.agent.is_some() && matches!(state, 1 | 3) {
                            t.state = AgentState::Working;
                        }
                    }
                    SessionEvent::Exited(code) => {
                        self.tabs[idx].exited = Some(code);
                        // 셸이 끝나면 탭을 닫는다(Ghostty 기본과 같다)
                        self.close_tab(idx, event_loop);
                        return;
                    }
                    SessionEvent::Bell | SessionEvent::Output => {}
                }
                self.request_redraw();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(_) => self.resize_all(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                // 창을 만든 직후에도 같은 배율로 한 번 오므로 바뀌었을 때만 글꼴을 다시 읽는다
                if (scale_factor - self.scale).abs() < f64::EPSILON {
                    return;
                }
                self.scale = scale_factor;
                self.load_fonts();
                self.resize_all();
            }
            WindowEvent::RedrawRequested => self.redraw(),
            WindowEvent::ModifiersChanged(m) => self.mods = m.state(),
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                if focused {
                    // 창으로 돌아오면 보고 있던 탭의 안 읽음을 지운다
                    if let Some(t) = self.tabs.get_mut(self.active) {
                        t.unread = false;
                    }
                    self.request_redraw();
                }
                if let Some(t) = self.active_tab() {
                    if t.session.modes().focus_events {
                        t.session.write(if focused { b"\x1b[I" } else { b"\x1b[O" });
                    }
                }
            }
            WindowEvent::Ime(ime) => match ime {
                Ime::Preedit(text, _) => {
                    self.preedit = text;
                    self.request_redraw();
                }
                Ime::Commit(text) => {
                    self.preedit.clear();
                    self.write_active(text.as_bytes());
                }
                Ime::Enabled | Ime::Disabled => {
                    self.preedit.clear();
                    self.request_redraw();
                }
            },
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state != ElementState::Pressed {
                    return;
                }
                if let Some(s) = keys::shortcut(&event, self.mods) {
                    self.handle_shortcut(s, event_loop);
                    return;
                }
                let app_cursor = self.active_tab().map(|t| t.session.modes().app_cursor).unwrap_or(false);
                let bytes = keys::encode(&event, self.mods, app_cursor, self.config.option_as_alt);
                if !bytes.is_empty() {
                    self.write_active(&bytes);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = position;
                if self.selecting {
                    if let (Some((c, r)), Some(t)) = (self.cell_at(position), self.active_tab()) {
                        t.session.select_update(c, r);
                        self.request_redraw();
                    }
                } else if let Some(m) = self.mouse_reporting() {
                    if m.mouse_motion {
                        self.send_mouse(mouse::Button::None, mouse::Action::Motion);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // 사이드바를 누르면 그 탭으로
                if (self.cursor_pos.x as u32) < self.sidebar_width() {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        let hit = self
                            .fonts
                            .as_ref()
                            .and_then(|f| sidebar::hit(f, self.pad(), self.cursor_pos.y, self.tabs.len()));
                        if let Some(i) = hit {
                            self.activate(i);
                        }
                    }
                    return;
                }
                let mb = match button {
                    MouseButton::Left => mouse::Button::Left,
                    MouseButton::Middle => mouse::Button::Middle,
                    MouseButton::Right => mouse::Button::Right,
                    _ => return,
                };
                if self.mouse_reporting().is_some() {
                    let action = if state == ElementState::Pressed {
                        mouse::Action::Press
                    } else {
                        mouse::Action::Release
                    };
                    self.send_mouse(mb, action);
                    return;
                }
                if button != MouseButton::Left {
                    return;
                }
                match state {
                    ElementState::Pressed => {
                        let Some((c, r)) = self.cell_at(self.cursor_pos) else {
                            return;
                        };
                        let now = Instant::now();
                        let clicks = match self.last_click {
                            Some((t, lc, lr, n)) if now - t < Duration::from_millis(400) && lc == c && lr == r => {
                                n % 3 + 1
                            }
                            _ => 1,
                        };
                        self.last_click = Some((now, c, r, clicks));
                        let kind = match clicks {
                            2 => SelectKind::Word,
                            3 => SelectKind::Line,
                            _ => SelectKind::Simple,
                        };
                        if let Some(t) = self.active_tab() {
                            if clicks == 1 {
                                t.session.select_clear();
                            }
                            t.session.select_start(c, r, kind);
                            if clicks > 1 {
                                t.session.select_update(c, r);
                            }
                        }
                        self.selecting = true;
                        self.request_redraw();
                    }
                    ElementState::Released => self.selecting = false,
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64 * 3.0,
                    MouseScrollDelta::PixelDelta(p) => {
                        let ch = self.fonts.as_ref().map(|f| f.metrics().height as f64).unwrap_or(16.0);
                        p.y / ch
                    }
                };
                self.scroll_accum += lines;
                let whole = self.scroll_accum.trunc() as i32;
                if whole == 0 {
                    return;
                }
                self.scroll_accum -= whole as f64;
                let Some(modes) = self.active_tab().map(|t| t.session.modes()) else {
                    return;
                };
                if self.mouse_reporting().is_some() {
                    let b = if whole > 0 {
                        mouse::Button::WheelUp
                    } else {
                        mouse::Button::WheelDown
                    };
                    for _ in 0..whole.unsigned_abs().min(10) {
                        self.send_mouse(b, mouse::Action::Press);
                    }
                } else if modes.alt_screen && modes.alternate_scroll {
                    // 대체 화면(less·vim)에서는 휠을 위·아래 화살표로 보낸다
                    let key = if whole > 0 {
                        bt_term::NamedKey::Up
                    } else {
                        bt_term::NamedKey::Down
                    };
                    let one = bt_term::encode_named(key, bt_term::Mods::default(), modes.app_cursor);
                    let bytes = one.repeat(whole.unsigned_abs().min(10) as usize);
                    if let Some(t) = self.active_tab() {
                        t.session.write(&bytes);
                    }
                } else if let Some(t) = self.active_tab() {
                    t.session.scroll(whole);
                    self.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // 1초마다 전경 프로세스를 본다(사이드바 이름, 에이전트 알아보기)
        let now = Instant::now();
        if now >= self.next_poll {
            if self.poll_foreground() && self.sidebar_visible {
                self.request_redraw();
            }
            self.next_poll = now + Duration::from_secs(1);
        }
        // 동기화 출력(DEC 2026)이 끝나지 않은 탭이 있으면 기한에 깨어 반영한다
        let deadline = self
            .tabs
            .iter()
            .filter_map(|t| t.session.sync_deadline())
            .chain([self.next_poll])
            .min();
        match deadline {
            Some(d) if d <= Instant::now() => {
                for t in &self.tabs {
                    if t.session.flush_expired_sync() {
                        self.request_redraw();
                    }
                }
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            Some(d) => event_loop.set_control_flow(ControlFlow::WaitUntil(d)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
    }
}
