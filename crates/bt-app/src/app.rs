//! 창 하나, 탭 여럿. winit 이벤트를 받아 PTY에 쓰고 화면을 그린다

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bt_render::{Canvas, CursorStyle, DrawCell, DrawCursor, FontConfig, Fonts, GridView, draw_grid};
use bt_term::{CellFlags, CursorShape, GridSize, Screen, SelectKind, Session, SessionEvent, SpawnOptions};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoopProxy};
use winit::keyboard::ModifiersState;
use winit::window::{CursorIcon, Window, WindowId};

use crate::keymap::{Action, Keymap, SplitTarget};
use crate::keys;
use crate::mouse;
use crate::split::{Direction, Node, Rect};
use crate::tabbar;

fn truncate_chars(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    s.chars().take(n.saturating_sub(1)).chain(['…']).collect()
}

fn command_result(rec: &bt_term::CommandRecord) -> bt_core::proto::CommandResult {
    bt_core::proto::CommandResult {
        command: rec.command.clone(),
        output: rec.output.clone(),
        exit_code: rec.exit_code,
        duration_ms: rec.duration.as_millis() as u64,
    }
}

/// 알림 띠가 보이는 시간
const TOAST_FOR: Duration = Duration::from_millis(2500);

/// 다른 스레드(PTY 읽기)가 창 스레드를 깨우는 사건
#[derive(Debug, Clone)]
pub enum UserEvent {
    /// 어떤 탭의 화면이 바뀌었다(여러 번 와도 한 번 그린다)
    Redraw,
    Session(u64, SessionEvent),
    /// 메뉴 막대 항목을 눌렀다(항목 id). 메뉴는 macOS에만 있다
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Menu(String),
    /// 제어 소켓 요청. 답은 함께 온 통로로 보낸다
    Control(
        bt_core::proto::Request,
        std::sync::mpsc::SyncSender<bt_core::proto::Response>,
    ),
}

/// 앱 설정: 설정 파일(bt_core::Config, Ghostty 문법)을 그대로 쓴다(#13)
pub type AppConfig = bt_core::Config;

/// 설정의 글꼴 키를 렌더러 설정으로
fn font_config(c: &AppConfig, size: f32, scale: f32) -> FontConfig {
    let defaults = FontConfig::default();
    // font-family를 여러 번 쓰면 첫째가 기본 글꼴, 나머지는 기본 대체 글꼴보다 앞에 둔다(Ghostty와 같다)
    let mut fallbacks: Vec<String> = c.font_family.iter().skip(1).cloned().collect();
    fallbacks.extend(defaults.fallbacks);
    FontConfig {
        family: c.font_family.first().cloned(),
        family_bold: c.font_family_bold.clone(),
        family_italic: c.font_family_italic.clone(),
        family_bold_italic: c.font_family_bold_italic.clone(),
        adjust_cell_height: c.adjust_cell_height,
        adjust_cell_width: c.adjust_cell_width,
        size,
        scale,
        fallbacks,
    }
}

/// 칸 하나: 셸 하나와 그 상태. 제어 소켓의 "탭 id"(BT_TAB_ID)는 칸의 id다
pub struct Pane {
    pub id: u64,
    pub session: Session,
    pub title: String,
    pub exited: Option<Option<u32>>,
    /// 전경 프로세스 이름(1초마다 갱신). pid가 같으면 다시 묻지 않는다(node 스크립트는 ps를 부른다)
    pub fg_name: Option<String>,
    fg_pid: Option<i32>,
    /// 보지 않은 알림이 있다
    pub unread: bool,
    /// 셸이 OSC 7로 알린 작업 폴더
    pub cwd: Option<std::path::PathBuf>,
    /// 마지막으로 데스크톱 알림을 띄운 때(탭마다 묶는다)
    last_desktop: Option<Instant>,
    /// 셸 통합: 지금 도는 명령(글, 시작한 때)과 마지막으로 끝난 명령
    pub running: Option<(Option<String>, Instant)>,
    pub last_command: Option<bt_term::CommandRecord>,
    /// 진행 표시(OSC 9;4): (상태, 퍼센트, 받은 때). 15초 동안 새 소식이 없으면 지운다(Ghostty와 같다)
    pub progress: Option<(u8, u8, Instant)>,
}

impl Pane {
    /// 아직 살아 있는 진행 표시(상태, 퍼센트)
    fn live_progress(&self) -> Option<(u8, u8)> {
        self.progress
            .filter(|(_, _, at)| at.elapsed() < Duration::from_secs(15))
            .map(|(s, p, _)| (s, p))
    }

    /// 탭 줄·창 제목에 쓸 이름: 셸이 보낸 제목 → 전경 프로세스 이름
    fn display_title(&self) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        self.fg_name.clone().unwrap_or_else(|| "셸".into())
    }
}

/// 탭 줄의 탭 하나: 칸들을 나눈 트리와 고른 칸(#35)
struct TabGroup {
    layout: Node,
    focus: u64,
    /// 고른 칸만 탭 전체에 크게 보인다(toggle_split_zoom)
    zoomed: bool,
}

/// 그릴 칸 하나(다시 그릴 때 잠깐 쓴다)
struct PaneView {
    id: u64,
    outer: Rect,
    inner: Rect,
    screen: Screen,
    focused: bool,
    progress: Option<(u8, u8)>,
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
    /// 모든 탭의 칸들(탭 순서와 상관없다)
    panes: Vec<Pane>,
    tabs: Vec<TabGroup>,
    /// 보고 있는 탭(tabs의 자리)
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
    next_poll: Instant,
    /// 셸 통합 스크립트를 풀어 둔 폴더(없으면 셸 통합을 싣지 않는다)
    shell_integration_dir: Option<std::path::PathBuf>,
    /// btctl run이 기다리는 탭: 탭 id → (요청 id, 답할 통로, 기한)
    run_waiters: std::collections::HashMap<u64, (u64, std::sync::mpsc::SyncSender<bt_core::proto::Response>, Instant)>,
    /// 메뉴 막대(살려 두어야 메뉴가 남는다)
    #[cfg(target_os = "macos")]
    menu: Option<muda::Menu>,
    /// 창 안 알림 띠(글, 띄운 때). 2.5초 보인다
    toast: Option<(String, Instant)>,
    /// Cmd를 누른 채 마우스가 올라간 링크(칸 id, 링크)
    hover: Option<(u64, bt_term::links::LinkHit)>,
    /// Cmd-클릭으로 링크를 열었다: 뒤따르는 놓기를 프로그램에 보내지 않는다
    link_click: bool,
    control: Option<crate::control::ControlServer>,
    keymap: Keymap,
    /// --config-file(설정을 다시 읽을 때 같은 파일을 읽는다)
    config_extra: Option<std::path::PathBuf>,
}

impl App {
    pub fn new(
        config: AppConfig,
        config_extra: Option<std::path::PathBuf>,
        proxy: EventLoopProxy<UserEvent>,
        first: SpawnOptions,
    ) -> Self {
        let mut config = config;
        let font_size = config.font_size;
        let (keymap, kw) = Keymap::build(&config.keybinds);
        for w in &kw {
            log::warn!("설정: {w}");
        }
        config.warnings.extend(kw);
        Self {
            config,
            proxy,
            redraw_pending: Arc::new(AtomicBool::new(false)),
            gfx: None,
            fonts: None,
            font_size,
            scale: 1.0,
            panes: Vec::new(),
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
            next_poll: Instant::now(),
            toast: None,
            hover: None,
            link_click: false,
            #[cfg(target_os = "macos")]
            menu: None,
            shell_integration_dir: None,
            run_waiters: std::collections::HashMap::new(),
            control: None,
            keymap,
            config_extra,
        }
    }

    fn load_fonts(&mut self) {
        let cfg = font_config(&self.config, self.font_size, self.scale as f32);
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

    /// 탭 줄 아래 칸들이 차지하는 자리. 여백은 칸마다 따로 뺀다(Ghostty처럼)
    fn content_rect(&self) -> Rect {
        let Some(g) = &self.gfx else { return Rect::default() };
        let size = g.window.inner_size();
        let top = self.tabbar_height();
        Rect::new(0, top as i32, size.width, size.height.saturating_sub(top))
    }

    /// 칸의 바깥 사각형에서 여백을 뺀 격자 자리
    fn inner_rect(&self, r: Rect) -> Rect {
        let px = |pt: f32| (pt as f64 * self.scale).round() as i32;
        let (left, right) = (px(self.config.padding_x.0), px(self.config.padding_x.1));
        let (top, bottom) = (px(self.config.padding_y.0), px(self.config.padding_y.1));
        Rect::new(
            r.x + left,
            r.y + top,
            (r.w as i32 - left - right).max(0) as u32,
            (r.h as i32 - top - bottom).max(0) as u32,
        )
    }

    /// 칸 사이 경계선 두께(픽셀)
    fn divider_px(&self) -> u32 {
        (self.scale.round() as u32).max(1)
    }

    /// 탭 하나의 칸 배치(바깥 사각형). 크게 보기면 고른 칸만 전체를 차지한다
    fn group_layout(&self, gi: usize) -> Vec<(u64, Rect)> {
        let Some(g) = self.tabs.get(gi) else { return Vec::new() };
        let area = self.content_rect();
        if g.zoomed {
            return vec![(g.focus, area)];
        }
        g.layout.layout(area, self.divider_px())
    }

    /// 칸의 바깥 사각형(보이지 않으면 없다)
    fn pane_rect(&self, id: u64) -> Option<Rect> {
        let gi = self.group_of(id)?;
        self.group_layout(gi)
            .into_iter()
            .find(|(i, _)| *i == id)
            .map(|(_, r)| r)
    }

    fn pane_index(&self, id: u64) -> Option<usize> {
        self.panes.iter().position(|p| p.id == id)
    }

    /// 칸이 든 탭
    fn group_of(&self, id: u64) -> Option<usize> {
        self.tabs.iter().position(|g| g.layout.contains(id))
    }

    /// 보고 있는 탭의 고른 칸 id
    fn focused_id(&self) -> Option<u64> {
        self.tabs.get(self.active).map(|g| g.focus)
    }

    /// 칸이 지금 창에 보이는가(보고 있는 탭에 있고 크게 보기에 가리지 않았다)
    fn is_visible(&self, idx: usize) -> bool {
        let Some(p) = self.panes.get(idx) else { return false };
        self.tabs.get(self.active).is_some_and(|g| {
            if g.zoomed {
                g.focus == p.id
            } else {
                g.layout.contains(p.id)
            }
        })
    }

    /// 키 입력을 받는 칸인가
    fn is_focused(&self, idx: usize) -> bool {
        self.panes.get(idx).is_some_and(|p| Some(p.id) == self.focused_id())
    }

    /// 탭 줄 높이. 탭이 하나면 없다(Ghostty처럼)
    fn tabbar_height(&self) -> u32 {
        match &self.fonts {
            Some(f) if self.tabs.len() > 1 => tabbar::height_px(f),
            _ => 0,
        }
    }

    fn activate(&mut self, index: usize) {
        if index < self.tabs.len() && index != self.active {
            let before = self.focused_id();
            self.active = index;
            self.focus_changed(before);
        }
        self.clear_visible_unread();
        self.request_redraw();
    }

    /// 칸 하나를 고른다(그 칸의 탭으로 옮겨 간다). 크게 보기 중에 다른 칸을 고르면 크게 보기를 푼다
    fn focus_pane(&mut self, id: u64) {
        let Some(gi) = self.group_of(id) else { return };
        let before = self.focused_id();
        self.active = gi;
        let g = &mut self.tabs[gi];
        let unzoom = g.zoomed && g.focus != id;
        g.focus = id;
        if unzoom {
            g.zoomed = false;
            self.resize_all();
        }
        self.focus_changed(before);
        self.clear_visible_unread();
        self.request_redraw();
    }

    /// 고른 칸이 바뀌었다: 포커스 보고(DEC 1004)를 켠 프로그램에 나가고 들어옴을 알린다
    fn focus_changed(&mut self, before: Option<u64>) {
        let after = self.focused_id();
        self.selecting = false;
        if before == after || !self.focused {
            return;
        }
        for (id, seq) in [(before, b"\x1b[O"), (after, b"\x1b[I")] {
            if let Some(p) = id.and_then(|id| self.pane_index(id)).map(|i| &self.panes[i]) {
                if p.session.modes().focus_events {
                    p.session.write(seq);
                }
            }
        }
    }

    /// 보이는 칸들의 안 읽음을 지운다
    fn clear_visible_unread(&mut self) {
        for i in 0..self.panes.len() {
            if self.is_visible(i) {
                self.panes[i].unread = false;
            }
        }
    }

    /// 전경 프로세스 이름을 본다(셸이 제목을 보내지 않을 때 탭 이름으로 쓴다). 바뀐 것이 있으면 참
    fn poll_foreground(&mut self) -> bool {
        let mut changed = false;
        for t in &mut self.panes {
            let Some(pid) = t.session.foreground_pid() else {
                continue;
            };
            if t.fg_pid == Some(pid) {
                continue;
            }
            let was_other = t.fg_pid.is_some();
            t.fg_pid = Some(pid);
            t.fg_name = bt_term::process::process_name(pid);
            changed = true;
            // 다른 프로그램이 끝나 셸로 돌아왔는데 마우스 모드가 켜져 있으면 끈다(#24).
            // 셸 자신(탭이 띄운 프로세스)이 전경일 때만: 셸이 일부러 켠 경우는 드물다
            let back_to_shell = t.session.pid().map(|p| p as i32) == Some(pid);
            if back_to_shell && was_other && self.config.mouse_reset_on_shell && t.session.reset_mouse_modes() {
                log::info!("탭 {}: 셸로 돌아왔는데 마우스 모드가 켜져 있어 껐습니다", t.id);
            }
        }
        changed
    }

    /// 탭이 알림을 보냈다. 보고 있는 탭이 아니면 안 읽음으로 두고, 창을 보고 있지 않으면 데스크톱 알림을 띄운다
    fn on_notification(&mut self, idx: usize, note: bt_term::Notification) {
        let watching = self.focused && self.is_visible(idx);
        let t = &mut self.panes[idx];
        if !watching {
            t.unread = true;
            // 같은 탭의 데스크톱 알림은 3초에 한 번(출력에 알림이 몰려도 화면을 덮지 않게)
            let now = Instant::now();
            if t.last_desktop.is_none_or(|at| now - at >= Duration::from_secs(3)) {
                t.last_desktop = Some(now);
                let title = if note.title.is_empty() {
                    t.display_title()
                } else {
                    note.title.clone()
                };
                crate::desktop::notify(&title, &note.body);
            }
        }
        self.request_redraw();
    }

    /// 셸 통합이 알린 명령의 끝: btctl run에 답하고, 오래 걸린 명령이면(창을 보지 않을 때) 알린다
    fn on_command_finished(&mut self, idx: usize, rec: bt_term::CommandRecord) {
        let tab_id = self.panes[idx].id;
        self.panes[idx].running = None;
        if let Some((req_id, reply, _)) = self.run_waiters.remove(&tab_id) {
            let _ = reply.send(bt_core::proto::Response::ok(
                req_id,
                serde_json::to_value(command_result(&rec)).unwrap_or_default(),
            ));
        }
        let limit = self.config.notify_command_seconds;
        let watching = self.focused && self.is_visible(idx);
        if limit > 0 && rec.duration.as_secs() >= limit && !watching {
            let cmd = rec.command.clone().unwrap_or_else(|| "명령".into());
            let code = rec
                .exit_code
                .map(|c| format!("종료 코드 {c}"))
                .unwrap_or_else(|| "끝남".into());
            let body = format!("{} — {code}, {}초", truncate_chars(&cmd, 60), rec.duration.as_secs());
            let title = if rec.exit_code.is_some_and(|c| c != 0) {
                "명령이 실패했습니다"
            } else {
                "명령이 끝났습니다"
            };
            self.on_notification(
                idx,
                bt_term::Notification {
                    title: title.into(),
                    body,
                },
            );
        }
        self.panes[idx].last_command = Some(rec);
    }

    /// 창 오른쪽 아래에 잠깐 알림 띠를 띄운다
    fn toast(&mut self, text: String) {
        self.toast = Some((text, Instant::now()));
        self.request_redraw();
    }

    /// 칸의 바깥 사각형에 들어가는 격자 크기
    fn grid_size_for(&self, outer: Rect) -> GridSize {
        let Some(f) = &self.fonts else {
            return GridSize::new(80, 24);
        };
        let m = f.metrics();
        let Rect { w, h, .. } = self.inner_rect(outer);
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

    /// 셸 하나를 띄워 칸을 만든다(아직 어느 탭에도 넣지 않는다)
    fn spawn_pane(&mut self, options: SpawnOptions, size: GridSize) -> Option<Pane> {
        let id = self.next_id;
        self.next_id += 1;
        let mut options = SpawnOptions {
            scrollback: self.config.scrollback_lines,
            cursor_shape: Some(match self.config.cursor_style {
                bt_core::config::CursorStyle::Block => CursorShape::Block,
                bt_core::config::CursorStyle::Bar => CursorShape::Beam,
                bt_core::config::CursorStyle::Underline => CursorShape::Underline,
                bt_core::config::CursorStyle::BlockHollow => CursorShape::HollowBlock,
            }),
            cursor_blink: self.config.cursor_blink.unwrap_or(false),
            clipboard_write: self.config.clipboard_write,
            clipboard_read: self.config.clipboard_read,
            ..options
        };
        // 칸 안의 btctl이 이 터미널과 이 칸을 찾게 한다
        if let Some(c) = &self.control {
            options
                .env
                .push((bt_core::proto::ENV_SOCKET.into(), c.path.to_string_lossy().into_owned()));
        }
        options.env.push((bt_core::proto::ENV_TAB.into(), id.to_string()));
        if let Some(dir) = &self.shell_integration_dir {
            crate::shellint::apply(&mut options, dir, &self.config.shell_integration);
        }
        match Session::spawn(options, size, self.config.theme.clone(), self.sink(id)) {
            Ok(session) => Some(Pane {
                id,
                session,
                title: String::new(),
                exited: None,
                fg_name: None,
                fg_pid: None,
                unread: false,
                cwd: None,
                last_desktop: None,
                progress: None,
                running: None,
                last_command: None,
            }),
            Err(e) => {
                log::error!("셸을 띄우지 못했습니다: {e}");
                None
            }
        }
    }

    /// 새 탭(칸 하나)을 연다. 돌려주는 값은 칸 id
    pub fn open_tab(&mut self, options: SpawnOptions) -> Option<u64> {
        // 탭이 하나에서 둘이 되면 탭 줄이 생긴다. 새 칸은 탭 줄을 뺀 크기로 띄운다
        let mut area = self.content_rect();
        if self.tabs.len() == 1 {
            let bar = self.fonts.as_ref().map(tabbar::height_px).unwrap_or(0);
            area.y += bar as i32;
            area.h = area.h.saturating_sub(bar);
        }
        let size = self.grid_size_for(area);
        let pane = self.spawn_pane(options, size)?;
        let id = pane.id;
        self.panes.push(pane);
        let before = self.focused_id();
        self.tabs.push(TabGroup {
            layout: Node::Leaf(id),
            focus: id,
            zoomed: false,
        });
        self.active = self.tabs.len() - 1;
        self.focus_changed(before);
        self.resize_all();
        Some(id)
    }

    /// 고른 칸을 나눠 새 칸을 연다. `dir`이 없으면 긴 쪽으로 나눈다
    fn open_split(&mut self, dir: Option<Direction>, options: SpawnOptions) -> Option<u64> {
        let gi = self.active;
        let target = self.focused_id()?;
        let dir = dir.unwrap_or_else(|| match self.pane_rect(target) {
            Some(r) if r.h > r.w => Direction::Down,
            _ => Direction::Right,
        });
        // 나눈 뒤의 배치로 새 칸 크기를 미리 잰다(셸이 처음부터 맞는 크기로 뜨게)
        let mut layout = self.tabs[gi].layout.clone();
        let new_id = self.next_id;
        layout.split(target, new_id, dir);
        let rect = layout
            .layout(self.content_rect(), self.divider_px())
            .into_iter()
            .find(|(id, _)| *id == new_id)
            .map(|(_, r)| r)?;
        let size = self.grid_size_for(rect);
        let pane = self.spawn_pane(options, size)?;
        debug_assert_eq!(pane.id, new_id);
        self.panes.push(pane);
        let before = self.focused_id();
        let g = &mut self.tabs[gi];
        g.layout = layout;
        g.focus = new_id;
        g.zoomed = false;
        self.focus_changed(before);
        self.resize_all();
        Some(new_id)
    }

    /// 새 칸을 띄울 값: 설정을 따르되, 고른 칸의 폴더를 이어받는다
    fn spawn_here(&self) -> SpawnOptions {
        // OSC 7로 알려진 폴더, 없으면 프로세스에게 묻는다
        let cwd = self
            .active_pane()
            .and_then(|t| t.cwd.clone().or_else(|| t.session.cwd()));
        let mut opts = self.default_spawn();
        if cwd.is_some() && self.config.working_directory == bt_core::config::WorkingDirectory::Inherit {
            opts.cwd = cwd;
        }
        opts
    }

    /// 설정의 command·working-directory로 새 탭 띄우기 값을 만든다
    fn default_spawn(&self) -> SpawnOptions {
        let mut o = SpawnOptions::default();
        if let Some(cmd) = &self.config.command {
            // Ghostty처럼 셸을 거쳐 띄운다
            o.program = Some("/bin/sh".into());
            o.args = vec!["-c".into(), format!("exec {cmd}")];
        }
        o.cwd = match &self.config.working_directory {
            bt_core::config::WorkingDirectory::Home => std::env::var_os("HOME").map(std::path::PathBuf::from),
            bt_core::config::WorkingDirectory::Path(p) => Some(p.clone()).filter(|p| p.is_dir()),
            // Finder에서 띄우면 지금 폴더가 /라 집 폴더로 둔다
            bt_core::config::WorkingDirectory::Inherit => {
                std::env::current_dir().ok().filter(|p| p != std::path::Path::new("/"))
            }
        };
        o
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
                    .panes
                    .iter()
                    .position(|x| x.id == t)
                    .ok_or_else(|| Response::err(id, format!("탭 {t}이 없습니다"))),
                None => app
                    .focused_id()
                    .and_then(|f| app.pane_index(f))
                    .ok_or_else(|| Response::err(id, "탭이 없습니다")),
            }
        };
        match req.call {
            Call::Ping => Response::ok(id, json!({ "app": "b-terminal", "version": env!("CARGO_PKG_VERSION") })),
            Call::List => {
                // 탭 줄 순서, 탭 안에서는 왼쪽·위부터
                let order: Vec<usize> = self
                    .tabs
                    .iter()
                    .flat_map(|g| g.layout.leaves())
                    .filter_map(|pid| self.pane_index(pid))
                    .collect();
                let tabs: Vec<TabInfo> = order
                    .into_iter()
                    .map(|i| (i, &self.panes[i]))
                    .map(|(i, t)| TabInfo {
                        id: t.id,
                        title: t.display_title(),
                        active: self.is_focused(i),
                        unread: t.unread,
                        cwd: t.cwd.clone(),
                        process: t.fg_name.clone(),
                    })
                    .collect();
                Response::ok(id, serde_json::to_value(tabs).unwrap_or_default())
            }
            Call::New {
                cwd,
                command,
                split,
                tab,
            } => {
                let mut opts = SpawnOptions {
                    cwd: cwd.filter(|c| c.is_dir()),
                    ..Default::default()
                };
                if let Some((prog, args)) = command.split_first() {
                    opts.program = Some(prog.clone());
                    opts.args = args.to_vec();
                }
                let Some(split) = split else {
                    return match self.open_tab(opts) {
                        Some(tab) => Response::ok(id, json!({ "tab": tab })),
                        None => Response::err(id, "탭을 열지 못했습니다"),
                    };
                };
                let dir = match split.as_str() {
                    "right" => Some(Direction::Right),
                    "down" => Some(Direction::Down),
                    "left" => Some(Direction::Left),
                    "up" => Some(Direction::Up),
                    "auto" => None,
                    other => return Response::err(id, format!("나눌 방향이 아닙니다: {other}")),
                };
                // 나눌 칸을 먼저 고른다(그 칸의 탭으로 옮겨 간다)
                match pick(self, tab) {
                    Ok(i) => {
                        let pid = self.panes[i].id;
                        self.focus_pane(pid);
                    }
                    Err(r) => return r,
                }
                match self.open_split(dir, opts) {
                    Some(tab) => Response::ok(id, json!({ "tab": tab })),
                    None => Response::err(id, "칸을 나누지 못했습니다"),
                }
            }
            Call::Focus { tab } => match pick(self, Some(tab)) {
                Ok(i) => {
                    let pid = self.panes[i].id;
                    self.focus_pane(pid);
                    Response::ok(id, json!({}))
                }
                Err(r) => r,
            },
            Call::Send { tab, text } => match pick(self, tab) {
                Ok(i) => {
                    self.panes[i].session.write(text.as_bytes());
                    Response::ok(id, json!({}))
                }
                Err(r) => r,
            },
            Call::Read { tab } => match pick(self, tab) {
                Ok(i) => Response::ok(id, json!({ "text": self.panes[i].session.snapshot().text() })),
                Err(r) => r,
            },
            Call::Run {
                tab,
                command,
                timeout_ms,
            } => match pick(self, tab) {
                Ok(i) => {
                    let t = &self.panes[i];
                    if !t.session.has_shell_integration() {
                        return Response::err(
                            id,
                            "이 탭은 셸 통합이 없습니다(zsh·bash 탭에서 쓰세요. shell-integration 설정을 보세요)",
                        );
                    }
                    if t.running.is_some() || self.run_waiters.contains_key(&t.id) {
                        return Response::err(id, "이 탭은 다른 명령을 돌리는 중입니다");
                    }
                    if command.contains('\n') || command.contains('\r') {
                        return Response::err(id, "명령은 한 줄이어야 합니다");
                    }
                    let tab_id = t.id;
                    t.session.write(format!("{command}\r").as_bytes());
                    // 답은 명령이 끝날 때(on_command_finished) 보낸다. 여기서는 빈 답을 돌려주지 않게 표시만 남긴다
                    Response {
                        id: u64::MAX,
                        ok: true,
                        result: json!({ "pending": tab_id, "deadline_ms": timeout_ms }),
                        error: None,
                    }
                }
                Err(r) => r,
            },
            Call::LastCommand { tab } => match pick(self, tab) {
                Ok(i) => match &self.panes[i].last_command {
                    Some(rec) => Response::ok(id, serde_json::to_value(command_result(rec)).unwrap_or_default()),
                    None => Response::err(id, "이 탭에서 셸 통합으로 끝난 명령이 아직 없습니다"),
                },
                Err(r) => r,
            },
            Call::ReloadConfig => {
                self.reload_config();
                Response::ok(
                    id,
                    json!({ "warnings": self.config.warnings, "sources": self.config.sources }),
                )
            }
            Call::Notify { tab, title, body } => match pick(self, tab) {
                Ok(i) => {
                    self.on_notification(i, bt_term::Notification { title, body });
                    Response::ok(id, json!({}))
                }
                Err(r) => r,
            },
        }
    }

    /// 칸 하나를 닫는다. 탭의 마지막 칸이면 탭이, 마지막 탭이면 앱이 닫힌다
    fn close_pane(&mut self, index: usize, event_loop: &ActiveEventLoop) {
        if index >= self.panes.len() {
            return;
        }
        // Session을 떨어뜨리면 PTY가 닫히고 셸은 SIGHUP을 받는다
        let id = self.panes.remove(index).id;
        if let Some((req_id, reply, _)) = self.run_waiters.remove(&id) {
            let _ = reply.send(bt_core::proto::Response::err(
                req_id,
                "명령이 끝나기 전에 칸이 닫혔습니다",
            ));
        }
        let before = self.focused_id();
        if let Some(gi) = self.group_of(id) {
            let g = &mut self.tabs[gi];
            let layout = std::mem::replace(&mut g.layout, Node::Leaf(id));
            match layout.remove(id) {
                (Some(rest), next) => {
                    if g.focus == id {
                        g.focus = next.unwrap_or_else(|| rest.leaves()[0]);
                        g.zoomed = false;
                    }
                    g.layout = rest;
                }
                (None, _) => {
                    self.tabs.remove(gi);
                    if gi < self.active || self.active >= self.tabs.len() {
                        self.active = self.active.saturating_sub(1);
                    }
                }
            }
        }
        if self.tabs.is_empty() {
            event_loop.exit();
            return;
        }
        self.focus_changed(before);
        self.clear_visible_unread();
        // 남은 칸이 빈자리를 채운다. 탭이 하나가 되면 탭 줄도 사라진다
        self.resize_all();
    }

    /// 탭을 칸째 닫는다
    fn close_tab(&mut self, gi: usize, event_loop: &ActiveEventLoop) {
        let Some(g) = self.tabs.get(gi) else { return };
        for id in g.layout.leaves() {
            if let Some(i) = self.pane_index(id) {
                self.close_pane(i, event_loop);
            }
        }
    }

    fn active_pane(&self) -> Option<&Pane> {
        self.focused_id()
            .and_then(|id| self.pane_index(id))
            .map(|i| &self.panes[i])
    }

    fn request_redraw(&self) {
        if let Some(g) = &self.gfx {
            g.window.request_redraw();
        }
    }

    fn write_active(&mut self, bytes: &[u8]) {
        if let Some(t) = self.active_pane() {
            t.session.select_clear();
            t.session.write(bytes);
        }
    }

    /// 모든 탭의 칸을 지금 배치에 맞춘다(창 크기·글꼴·분할이 바뀐 뒤)
    fn resize_all(&mut self) {
        for gi in 0..self.tabs.len() {
            for (id, rect) in self.group_layout(gi) {
                let size = self.grid_size_for(rect);
                if let Some(i) = self.pane_index(id) {
                    self.panes[i].session.resize(size);
                }
            }
        }
        self.request_redraw();
    }

    fn set_font_size(&mut self, size: f32) {
        self.font_size = size.clamp(6.0, 72.0);
        self.load_fonts();
        self.resize_all();
    }

    /// 창 좌표 아래의 칸(보고 있는 탭에서)
    fn pane_at(&self, pos: PhysicalPosition<f64>) -> Option<u64> {
        self.group_layout(self.active)
            .into_iter()
            .find(|(_, r)| r.contains(pos.x, pos.y))
            .map(|(id, _)| id)
    }

    /// 창 좌표 → 칸 idx의 격자 칸. 칸 밖이면 가장자리 칸으로 붙인다(고르며 끌다 칸을 벗어나도)
    fn cell_in(&self, idx: usize, pos: PhysicalPosition<f64>) -> Option<(usize, usize)> {
        let m = self.fonts.as_ref()?.metrics();
        let pane = self.panes.get(idx)?;
        let r = self.inner_rect(self.pane_rect(pane.id)?);
        let size = pane.session.size();
        let col = ((pos.x - r.x as f64) / m.width as f64).floor().max(0.0) as usize;
        let row = ((pos.y - r.y as f64) / m.height as f64).floor().max(0.0) as usize;
        Some((
            col.min((size.cols as usize).saturating_sub(1)),
            row.min((size.rows as usize).saturating_sub(1)),
        ))
    }

    /// 링크를 짚는 수식 키: macOS는 Cmd, 그 밖에는 Ctrl(Ghostty와 같다)
    fn link_mod(&self) -> bool {
        if cfg!(target_os = "macos") {
            self.mods.super_key()
        } else {
            self.mods.control_key()
        }
    }

    /// 창 좌표 아래의 링크(칸의 격자 안에서만)
    fn link_under(&self, pos: PhysicalPosition<f64>) -> Option<(u64, bt_term::links::LinkHit)> {
        let id = self.pane_at(pos)?;
        let i = self.pane_index(id)?;
        if !self.inner_rect(self.pane_rect(id)?).contains(pos.x, pos.y) {
            return None;
        }
        let (c, r) = self.cell_in(i, pos)?;
        bt_term::links::link_at(&self.panes[i].session.snapshot(), c, r).map(|h| (id, h))
    }

    /// Cmd를 누른 동안 마우스 아래 링크를 다시 본다. 바뀌면 밑줄과 마우스 모양을 바꾼다
    fn update_hover(&mut self) {
        let next = if self.link_mod() {
            self.link_under(self.cursor_pos)
        } else {
            None
        };
        if next == self.hover {
            return;
        }
        if let Some(g) = &self.gfx {
            g.window.set_cursor(if next.is_some() {
                CursorIcon::Pointer
            } else {
                CursorIcon::Default
            });
        }
        self.hover = next;
        self.request_redraw();
    }

    /// 링크를 연다: 열어도 되는 것(http·https, 이 컴퓨터의 파일)만
    fn open_link(&mut self, url: &str) {
        match bt_term::links::classify(url, &is_local_host) {
            Some(target) => crate::desktop::open_link(&target),
            None => {
                let shown: String = bt_core::proto::printable(url).chars().take(60).collect();
                self.toast(format!("열 수 없는 링크입니다: {shown}"));
            }
        }
    }

    /// 고른 칸의 자리
    fn active_index(&self) -> Option<usize> {
        self.focused_id().and_then(|id| self.pane_index(id))
    }

    fn handle_action(&mut self, action: Action, event_loop: &ActiveEventLoop) {
        match action {
            Action::Copy => {
                if let Some(text) = self.active_pane().and_then(|t| t.session.selection_text()) {
                    if let Some(cb) = self.clipboard.as_mut() {
                        let _ = cb.set_text(text);
                    }
                }
            }
            Action::Paste => {
                let text = self.clipboard.as_mut().and_then(|cb| cb.get_text().ok());
                if let (Some(text), Some(t)) = (text, self.active_pane()) {
                    let bytes = bt_term::encode_paste(&text, t.session.modes().bracketed_paste);
                    self.write_active(&bytes);
                }
            }
            Action::FontBigger(n) => self.set_font_size(self.font_size + n),
            Action::FontSmaller(n) => self.set_font_size(self.font_size - n),
            Action::FontReset => self.set_font_size(self.config.font_size),
            // 새 탭·새 칸은 고른 칸의 폴더에서 연다
            Action::NewTab => {
                self.open_tab(self.spawn_here());
            }
            Action::NewSplit(dir) => {
                self.open_split(dir, self.spawn_here());
            }
            Action::CloseSurface => {
                if let Some(i) = self.active_index() {
                    self.close_pane(i, event_loop);
                }
            }
            Action::CloseTab => self.close_tab(self.active, event_loop),
            Action::GotoSplit(target) => {
                let area = self.content_rect();
                let divider = self.divider_px();
                let next = self.tabs.get(self.active).and_then(|g| match target {
                    SplitTarget::Previous => g.layout.cycle(g.focus, -1),
                    SplitTarget::Next => g.layout.cycle(g.focus, 1),
                    SplitTarget::Toward(d) => g.layout.neighbor(area, divider, g.focus, d),
                });
                if let Some(id) = next {
                    self.focus_pane(id);
                }
            }
            Action::ResizeSplit(dir, points) => {
                let area = self.content_rect();
                let divider = self.divider_px();
                let px = (points as f64 * self.scale).round() as u32;
                if let Some(g) = self.tabs.get_mut(self.active) {
                    let focus = g.focus;
                    // 크게 보는 중에는 경계가 보이지 않으니 밀지 않는다
                    if !g.zoomed && g.layout.resize(area, divider, focus, dir, px) {
                        self.resize_all();
                    }
                }
            }
            Action::EqualizeSplits => {
                if let Some(g) = self.tabs.get_mut(self.active) {
                    g.layout.equalize();
                    self.resize_all();
                }
            }
            Action::ToggleSplitZoom => {
                // 칸이 하나면 크게 볼 것이 없다
                if let Some(g) = self.tabs.get_mut(self.active).filter(|g| g.layout.leaves().len() > 1) {
                    g.zoomed = !g.zoomed;
                    self.resize_all();
                }
            }
            Action::NextTab if !self.tabs.is_empty() => self.activate((self.active + 1) % self.tabs.len()),
            Action::PrevTab if !self.tabs.is_empty() => {
                self.activate((self.active + self.tabs.len() - 1) % self.tabs.len())
            }
            Action::LastTab if !self.tabs.is_empty() => self.activate(self.tabs.len() - 1),
            Action::GotoTab(n) if n <= self.tabs.len() => self.activate(n - 1),
            // Ghostty의 clear_screen: 화면과 스크롤백을 지운다. 셸에 Ctrl-L을 보내 프롬프트를 다시 그리게 한다
            Action::ClearScreen => self.write_active(b"\x0c"),
            Action::ScrollPageUp | Action::ScrollPageDown => {
                if let Some(t) = self.active_pane() {
                    let rows = t.session.size().rows as i32;
                    t.session.scroll(if action == Action::ScrollPageUp {
                        rows - 1
                    } else {
                        -(rows - 1)
                    });
                }
            }
            Action::ScrollTop => {
                if let Some(t) = self.active_pane() {
                    t.session.scroll(i32::MAX / 2);
                }
            }
            Action::ScrollBottom => {
                if let Some(t) = self.active_pane() {
                    t.session.scroll_to_bottom();
                }
            }
            Action::SelectAll => {
                if let Some(t) = self.active_pane() {
                    let size = t.session.size();
                    t.session.select_start(0, 0, SelectKind::Line);
                    t.session.select_update(size.cols as usize - 1, size.rows as usize - 1);
                }
            }
            Action::ReloadConfig => self.reload_config(),
            Action::JumpToPrompt(dir) => {
                if let Some(t) = self.active_pane() {
                    if !t.session.jump_to_prompt(dir) && !t.session.has_shell_integration() {
                        self.toast("셸 통합이 없는 탭이라 프롬프트를 모릅니다".into());
                    }
                }
            }
            Action::Text(text) => {
                if let Some(t) = self.active_pane() {
                    t.session.write(text.as_bytes());
                }
            }
            _ => {}
        }
        self.request_redraw();
    }

    /// 설정을 다시 읽어 글꼴·색·키 묶음·여백을 바꾼다. 이미 열린 탭에도 새 색을 쓴다
    fn reload_config(&mut self) {
        let next = AppConfig::load(self.config_extra.as_deref(), crate::system_is_dark());
        for w in &next.warnings {
            log::warn!("설정: {w}");
        }
        let mut next = next;
        let (keymap, kw) = Keymap::build(&next.keybinds);
        for w in &kw {
            log::warn!("설정: {w}");
        }
        // 키 묶음 경고도 설정 경고로 함께 보인다(btctl reload-config)
        next.warnings.extend(kw);
        self.keymap = keymap;
        self.font_size = next.font_size;
        for t in &self.panes {
            t.session.set_theme(next.theme.clone());
        }
        #[cfg(target_os = "macos")]
        if let Some(g) = &self.gfx {
            use winit::platform::macos::{OptionAsAlt, WindowExtMacOS};
            g.window.set_option_as_alt(match next.option_as_alt {
                bt_core::config::OptionAsAlt::Both => OptionAsAlt::Both,
                bt_core::config::OptionAsAlt::Left => OptionAsAlt::OnlyLeft,
                bt_core::config::OptionAsAlt::Right => OptionAsAlt::OnlyRight,
                bt_core::config::OptionAsAlt::None => OptionAsAlt::None,
            });
        }
        self.config = next;
        self.load_fonts();
        self.resize_all();
        log::info!("설정을 다시 읽었습니다");
        let warn = self.config.warnings.len();
        self.toast(if warn == 0 {
            "설정을 다시 읽었습니다".into()
        } else {
            format!("설정을 다시 읽었습니다(경고 {warn}개)")
        });
    }

    /// 칸 idx의 프로그램이 마우스 보고를 켰으면 그 모드
    fn mouse_reporting(&self, idx: usize) -> Option<bt_term::Modes> {
        let m = self.panes.get(idx)?.session.modes();
        // Shift를 누르면 앱의 마우스 보고를 건너뛰고 고르기를 한다(xterm 관례)
        (m.mouse_report || m.mouse_drag || m.mouse_motion)
            .then_some(m)
            .filter(|_| !self.mods.shift_key())
    }

    fn send_mouse(&mut self, idx: usize, button: mouse::Button, action: mouse::Action) {
        let Some(m) = self.mouse_reporting(idx) else { return };
        let Some((col, row)) = self.cell_in(idx, self.cursor_pos) else {
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
        self.panes[idx].session.write(&bytes);
    }

    fn redraw(&mut self) {
        self.redraw_pending.store(false, Ordering::Release);
        let theme = self.config.theme.clone();
        let toast = self.toast.clone().filter(|(_, at)| at.elapsed() < TOAST_FOR);
        let focus = self.focused_id();
        // 보이는 칸마다: 바깥 사각형, 격자 자리, 화면, 고른 칸인가, 진행 표시
        let layout = self.group_layout(self.active);
        let views: Vec<PaneView> = layout
            .iter()
            .filter_map(|(id, outer)| {
                let p = &self.panes[self.pane_index(*id)?];
                Some(PaneView {
                    id: *id,
                    outer: *outer,
                    inner: self.inner_rect(*outer),
                    screen: p.session.snapshot(),
                    focused: Some(*id) == focus,
                    progress: p.live_progress(),
                })
            })
            .collect();
        let split = views.len() > 1;
        let dividers = match self.tabs.get(self.active) {
            Some(g) if split => g.layout.dividers(self.content_rect(), self.divider_px()),
            _ => Vec::new(),
        };
        let divider_color = self
            .config
            .split_divider_color
            .unwrap_or_else(|| theme.background.mix(theme.foreground, 0.2));
        // 고르지 않은 칸은 바탕색으로 살짝 덮는다(Ghostty의 unfocused-split-opacity)
        let dim_alpha = ((1.0 - self.config.unfocused_split_opacity) * 255.0).round() as u8;
        let dim_fill = self.config.unfocused_split_fill.unwrap_or(theme.background);
        let single_tab = self.tabs.len() == 1;
        let labels: Vec<tabbar::TabLabel> = if single_tab {
            Vec::new()
        } else {
            self.tabs
                .iter()
                .enumerate()
                .map(|(gi, g)| {
                    let panes: Vec<&Pane> = g
                        .layout
                        .leaves()
                        .into_iter()
                        .filter_map(|id| self.pane_index(id).map(|i| &self.panes[i]))
                        .collect();
                    let focused = panes.iter().find(|p| p.id == g.focus);
                    tabbar::TabLabel {
                        title: focused.map(|p| p.display_title()).unwrap_or_default(),
                        active: gi == self.active,
                        unread: panes.iter().any(|p| p.unread),
                        // 고른 칸의 진행 표시, 없으면 다른 칸의 것
                        progress: focused
                            .and_then(|p| p.live_progress())
                            .or_else(|| panes.iter().find_map(|p| p.live_progress())),
                    }
                })
                .collect()
        };
        let title = self
            .active_pane()
            .map(|t| t.display_title())
            .unwrap_or_else(|| "b-terminal".into());
        let title = self.config.title.clone().unwrap_or(title);
        let preedit = self.preedit.clone();
        let hover = self.hover.clone();
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
            let bg = views
                .iter()
                .find(|v| v.focused)
                .map(|v| v.screen.background)
                .unwrap_or(theme.background);
            canvas.clear(bg);
            if !labels.is_empty() {
                tabbar::draw(&mut canvas, fonts, &theme, &labels);
            }
            let m = fonts.metrics();
            for PaneView {
                id,
                outer,
                inner,
                screen,
                focused,
                progress,
            } in &views
            {
                canvas.fill_rect(outer.x, outer.y, outer.w, outer.h, screen.background);
                let mut cells = to_draw_cells(screen);
                // Cmd를 누른 채 올라간 링크에 밑줄
                if let Some((_, hit)) = hover.as_ref().filter(|(h, _)| h == id) {
                    for sp in &hit.spans {
                        for c in sp.start..sp.end.min(screen.cols) {
                            if let Some(cell) = cells.get_mut(sp.row * screen.cols + c) {
                                cell.underline = true;
                            }
                        }
                    }
                }
                let cursor = screen.cursor.visible.then_some(DrawCursor {
                    col: screen.cursor.col,
                    row: screen.cursor.row,
                    // 고르지 않은 칸의 커서는 속이 빈 네모(Ghostty와 같다)
                    style: match (focused, screen.cursor.shape) {
                        (false, _) => CursorStyle::Hollow,
                        (true, CursorShape::Block) => CursorStyle::Block,
                        (true, CursorShape::Beam) => CursorStyle::Beam,
                        (true, CursorShape::Underline) => CursorStyle::Underline,
                        (true, CursorShape::HollowBlock) => CursorStyle::Hollow,
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
                    selection_fg: theme.selection_foreground,
                    // IME 조합 글은 고른 칸에만 그린다
                    preedit: (*focused && !preedit.is_empty()).then_some(preedit.as_str()),
                };
                draw_grid(&mut canvas, fonts, inner.x, inner.y, &view);
                if *focused {
                    // IME 후보 창을 커서 옆에 띄운다
                    let px = inner.x + (screen.cursor.col as u32 * m.width) as i32;
                    let py = inner.y + (screen.cursor.row as u32 * m.height) as i32;
                    gfx.window
                        .set_ime_cursor_area(PhysicalPosition::new(px, py), PhysicalSize::new(m.width, m.height));
                } else if split && dim_alpha > 0 {
                    canvas.blend_rect(outer.x, outer.y, outer.w, outer.h, dim_fill, dim_alpha);
                }
                // 탭이 하나면 진행 막대를 칸 맨 위에 얇게(Ghostty와 같은 자리). 탭이 여럿이면 탭 줄에 보인다
                if let (true, Some(p)) = (single_tab, progress) {
                    tabbar::draw_progress(&mut canvas, &theme, outer.x, outer.y, outer.w, *p, m.stroke * 3);
                }
            }
            for d in &dividers {
                canvas.fill_rect(d.x, d.y, d.w, d.h, divider_color);
            }
            // 실제 주소를 창 왼쪽 아래에 보인다(OSC 8 링크는 보이는 글과 주소가 다를 수 있다)
            if let Some((_, hit)) = &hover {
                tabbar::draw_link_hint(&mut canvas, fonts, &theme, &bt_core::proto::printable(&hit.url));
            }
            if let Some((text, _)) = &toast {
                crate::tabbar::draw_toast(&mut canvas, fonts, &theme, text);
            }
        }
        let _ = buffer.present();
        gfx.window.set_title(&title);
    }
}

/// OSC 7 폴더를 새 탭의 시작 폴더로 써도 되는가: 호스트가 이 컴퓨터(빈 값·localhost·호스트 이름)이고 실제 폴더일 때만.
/// ssh 안의 셸이 보낸 원격 경로나 터미널에 찍힌 글이 고른 경로로 새 셸을 열지 않는다
pub fn osc7_is_local(host: &str, path: &std::path::Path) -> bool {
    is_local_host(host) && path.is_absolute() && path.is_dir()
}

/// 호스트 이름이 이 컴퓨터인가(빈 값·localhost·호스트 이름·그 짧은 이름). OSC 7 폴더와 file 링크가 쓴다
pub fn is_local_host(host: &str) -> bool {
    let local = gethostname::gethostname().to_string_lossy().to_ascii_lowercase();
    let host = host.to_ascii_lowercase();
    let short = local.split('.').next().unwrap_or(&local).to_string();
    host.is_empty() || host == "localhost" || host == local || host.split('.').next() == Some(short.as_str())
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
            attrs.with_option_as_alt(match self.config.option_as_alt {
                bt_core::config::OptionAsAlt::Both => OptionAsAlt::Both,
                bt_core::config::OptionAsAlt::Left => OptionAsAlt::OnlyLeft,
                bt_core::config::OptionAsAlt::Right => OptionAsAlt::OnlyRight,
                bt_core::config::OptionAsAlt::None => OptionAsAlt::None,
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
        #[cfg(target_os = "macos")]
        if self.menu.is_none() {
            let menu = crate::menu::build();
            menu.init_for_nsapp();
            let proxy = self.proxy.clone();
            muda::MenuEvent::set_event_handler(Some(move |e: muda::MenuEvent| {
                let _ = proxy.send_event(UserEvent::Menu(e.id.0.clone()));
            }));
            self.menu = Some(menu);
        }
        self.scale = window.scale_factor();
        let context = softbuffer::Context::new(window.clone()).expect("softbuffer context");
        let surface = softbuffer::Surface::new(&context, window.clone()).expect("softbuffer surface");
        self.gfx = Some(Gfx {
            window,
            surface,
            _context: context,
        });
        self.load_fonts();
        if let (Some(f), Some(g)) = (&self.fonts, &self.gfx) {
            if let (Some(cols), Some(rows)) = (self.config.window_width, self.config.window_height) {
                let m = f.metrics();
                let px = |pt: f32| (pt as f64 * self.scale).round() as u32;
                let w = m.width * cols as u32 + px(self.config.padding_x.0) + px(self.config.padding_x.1);
                let h = m.height * rows as u32 + px(self.config.padding_y.0) + px(self.config.padding_y.1);
                let _ = g.window.request_inner_size(PhysicalSize::new(w, h));
            }
        }
        if self.fonts.is_none() {
            event_loop.exit();
            return;
        }
        if self.shell_integration_dir.is_none() && self.config.shell_integration != "none" {
            match crate::shellint::install() {
                Ok(dir) => self.shell_integration_dir = Some(dir),
                Err(e) => log::warn!("셸 통합 스크립트를 풀지 못했습니다(셸 통합 없이 띄웁니다): {e}"),
            }
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
        if self.panes.is_empty() {
            let mut first = self.first.take().unwrap_or_default();
            if first.program.is_none() {
                let d = self.default_spawn();
                first.program = d.program;
                first.args = d.args;
            }
            if first.cwd.is_none() {
                first.cwd = self.default_spawn().cwd;
            }
            self.open_tab(first);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Redraw => {
                // 화면이 바뀌면 짚고 있던 링크도 바뀔 수 있다
                if self.hover.is_some() {
                    self.update_hover();
                }
                self.request_redraw()
            }
            UserEvent::Menu(id) => match crate::menu::action_for(&id) {
                Some(crate::menu::MenuCommand::Action(a)) => self.handle_action(a, event_loop),
                Some(crate::menu::MenuCommand::OpenSettings) => match crate::menu::ensure_config_file() {
                    // 기본 글 편집기로 연다
                    Ok(path) => {
                        let _ = std::process::Command::new("open").arg("-t").arg(&path).spawn();
                        self.toast("설정을 고친 뒤 Cmd-Shift-, 로 다시 읽습니다".into());
                    }
                    Err(e) => log::warn!("설정 파일을 만들지 못했습니다: {e}"),
                },
                None => {}
            },
            UserEvent::Control(req, reply) => {
                let req_id = req.id;
                let timeout = match &req.call {
                    bt_core::proto::Call::Run { timeout_ms, .. } => Some(*timeout_ms),
                    _ => None,
                };
                let resp = self.handle_call(req);
                // run: 명령을 넣었으면 끝날 때 답한다(기한이 지나면 about_to_wait가 오류로 답한다)
                if resp.id == u64::MAX {
                    if let Some(tab_id) = resp.result.get("pending").and_then(|v| v.as_u64()) {
                        let deadline = Instant::now() + Duration::from_millis(timeout.unwrap_or(60_000));
                        self.run_waiters.insert(tab_id, (req_id, reply, deadline));
                    }
                } else {
                    let _ = reply.send(resp);
                }
            }
            UserEvent::Session(id, e) => {
                let Some(idx) = self.panes.iter().position(|t| t.id == id) else {
                    return;
                };
                match e {
                    SessionEvent::Title(t) => {
                        self.panes[idx].title = bt_core::proto::printable(&t).chars().take(200).collect()
                    }
                    SessionEvent::Cwd { host, path } => {
                        if osc7_is_local(&host, &path) {
                            self.panes[idx].cwd = Some(path);
                        }
                    }
                    SessionEvent::Notification(note) => self.on_notification(idx, note),
                    SessionEvent::Exited(code) => {
                        self.panes[idx].exited = Some(code);
                        // 셸이 끝나면 그 칸을 닫는다(Ghostty 기본과 같다)
                        self.close_pane(idx, event_loop);
                        return;
                    }
                    SessionEvent::Progress { state, percent } => {
                        self.panes[idx].progress = (state != 0).then(|| (state, percent, Instant::now()));
                    }
                    SessionEvent::ClipboardStore(text) => {
                        // 프로그램이 OSC 52로 보낸 글(Claude Code의 /copy 등).
                        // 보고 있는 창의 고른 칸에서 온 것만 받는다: 뒤쪽 탭이나 옆 칸에 흘러간 출력이 몰래 클립보드를 바꿔
                        // 나중의 붙여넣기에 명령을 끼워 넣지 못하게(붙여넣기 하이재킹). 받으면 알림으로 보인다
                        let watching = self.focused && self.is_focused(idx);
                        if watching && text.len() <= 1024 * 1024 {
                            if let Some(cb) = self.clipboard.as_mut() {
                                let _ = cb.set_text(text.clone());
                            }
                            let shown: String = bt_core::proto::printable(&text).chars().take(40).collect();
                            log::info!(
                                "탭 {}: 프로그램이 클립보드에 {}글자를 썼습니다",
                                self.panes[idx].id,
                                text.chars().count()
                            );
                            // 보고 있는 탭이라 안 읽음·데스크톱 알림으로는 보이지 않는다. 창 안 알림 띠로 늘 보인다
                            self.toast(format!("클립보드에 복사됨: {shown}"));
                        } else {
                            log::warn!(
                                "탭 {}: 보고 있지 않은 탭의 클립보드 쓰기(OSC 52)를 버렸습니다",
                                self.panes[idx].id
                            );
                        }
                    }
                    SessionEvent::ClipboardLoad(reply) => {
                        // clipboard-read = allow일 때만 엔진이 이 사건을 낸다
                        let text = self
                            .clipboard
                            .as_mut()
                            .and_then(|cb| cb.get_text().ok())
                            .unwrap_or_default();
                        self.panes[idx].session.write(reply.format(&text).as_bytes());
                    }
                    SessionEvent::CommandStarted(cmd) => self.panes[idx].running = Some((cmd, Instant::now())),
                    SessionEvent::CommandFinished(rec) => self.on_command_finished(idx, rec),
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
            WindowEvent::ModifiersChanged(m) => {
                self.mods = m.state();
                self.update_hover();
            }
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                if focused {
                    // 창으로 돌아오면 보고 있던 탭의 안 읽음을 지운다
                    self.clear_visible_unread();
                    self.request_redraw();
                }
                if let Some(t) = self.active_pane() {
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
                let action = keys::trigger(&event, self.mods).and_then(|t| self.keymap.lookup(&t).cloned());
                if let Some(action) = action {
                    self.handle_action(action, event_loop);
                    return;
                }
                let modes = self.active_pane().map(|t| t.session.modes()).unwrap_or_default();
                let meta = self.config.option_as_alt != bt_core::config::OptionAsAlt::None;
                let bytes = keys::encode(&event, self.mods, modes, meta);
                if self.config.mouse_hide_while_typing {
                    if let Some(g) = &self.gfx {
                        g.window.set_cursor_visible(false);
                    }
                }
                if !bytes.is_empty() {
                    self.write_active(&bytes);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_pos = position;
                if self.config.mouse_hide_while_typing {
                    if let Some(g) = &self.gfx {
                        g.window.set_cursor_visible(true);
                    }
                }
                self.update_hover();
                let Some(idx) = self.active_index() else { return };
                if self.selecting {
                    if let Some((c, r)) = self.cell_in(idx, position) {
                        self.panes[idx].session.select_update(c, r);
                        self.request_redraw();
                    }
                } else if let Some(m) = self.mouse_reporting(idx) {
                    // 움직임 보고는 고른 칸 위에서만
                    if m.mouse_motion && self.pane_at(position) == Some(self.panes[idx].id) {
                        self.send_mouse(idx, mouse::Button::None, mouse::Action::Motion);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // 탭 줄을 누르면 그 탭으로
                if (self.cursor_pos.y as u32) < self.tabbar_height() {
                    if state == ElementState::Pressed && button == MouseButton::Left {
                        let width = self.gfx.as_ref().map(|g| g.window.inner_size().width).unwrap_or(0);
                        let hit = self
                            .fonts
                            .as_ref()
                            .and_then(|f| tabbar::hit(f, width, self.cursor_pos.x, self.tabs.len()));
                        if let Some(i) = hit {
                            self.activate(i);
                        }
                    }
                    return;
                }
                // Cmd-클릭: 링크를 연다. 누름과 놓기 모두 프로그램(마우스 보고)·고르기에 보내지 않는다
                if button == MouseButton::Left {
                    if state == ElementState::Released && self.link_click {
                        self.link_click = false;
                        return;
                    }
                    if state == ElementState::Pressed && self.link_mod() {
                        if let Some((_, hit)) = self.link_under(self.cursor_pos) {
                            self.link_click = true;
                            self.open_link(&hit.url);
                            return;
                        }
                    }
                }
                // 다른 칸을 누르면 그 칸을 고른다. 누름은 고른 칸이 그대로 받는다
                if state == ElementState::Pressed {
                    if let Some(id) = self
                        .pane_at(self.cursor_pos)
                        .filter(|id| Some(*id) != self.focused_id())
                    {
                        self.focus_pane(id);
                    }
                }
                let Some(idx) = self.active_index() else { return };
                let mb = match button {
                    MouseButton::Left => mouse::Button::Left,
                    MouseButton::Middle => mouse::Button::Middle,
                    MouseButton::Right => mouse::Button::Right,
                    _ => return,
                };
                if self.mouse_reporting(idx).is_some() {
                    let action = if state == ElementState::Pressed {
                        mouse::Action::Press
                    } else {
                        mouse::Action::Release
                    };
                    self.send_mouse(idx, mb, action);
                    return;
                }
                if button != MouseButton::Left {
                    return;
                }
                match state {
                    ElementState::Pressed => {
                        let Some((c, r)) = self.cell_in(idx, self.cursor_pos) else {
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
                        if let Some(t) = self.active_pane() {
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
                    ElementState::Released => {
                        self.selecting = false;
                        // copy-on-select(Ghostty 기본 켬): 고르기를 마치면 바로 클립보드로
                        if self.config.copy_on_select {
                            if let Some(text) = self.active_pane().and_then(|t| t.session.selection_text()) {
                                if let Some(cb) = self.clipboard.as_mut() {
                                    let _ = cb.set_text(text);
                                }
                            }
                        }
                    }
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
                // 휠은 마우스 아래 칸이 받는다(고르지 않은 칸도 굴려 볼 수 있게)
                let Some(idx) = self
                    .pane_at(self.cursor_pos)
                    .and_then(|id| self.pane_index(id))
                    .or_else(|| self.active_index())
                else {
                    return;
                };
                let modes = self.panes[idx].session.modes();
                if self.mouse_reporting(idx).is_some() {
                    let b = if whole > 0 {
                        mouse::Button::WheelUp
                    } else {
                        mouse::Button::WheelDown
                    };
                    for _ in 0..whole.unsigned_abs().min(10) {
                        self.send_mouse(idx, b, mouse::Action::Press);
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
                    self.panes[idx].session.write(&bytes);
                } else {
                    self.panes[idx].session.scroll(whole);
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
            // 15초 넘게 소식이 없는 진행 표시는 지운다
            let mut expired = false;
            for t in &mut self.panes {
                if t.progress
                    .is_some_and(|(_, _, at)| at.elapsed() >= Duration::from_secs(15))
                {
                    t.progress = None;
                    expired = true;
                }
            }
            if self.toast.as_ref().is_some_and(|(_, at)| at.elapsed() >= TOAST_FOR) {
                self.toast = None;
                expired = true;
            }
            let late: Vec<u64> = self
                .run_waiters
                .iter()
                .filter(|(_, (_, _, d))| *d <= now)
                .map(|(k, _)| *k)
                .collect();
            for tab_id in late {
                if let Some((req_id, reply, _)) = self.run_waiters.remove(&tab_id) {
                    let _ = reply.send(bt_core::proto::Response::err(
                        req_id,
                        "기한 안에 명령이 끝나지 않았습니다(명령은 탭에서 계속 돕니다)",
                    ));
                }
            }
            if self.poll_foreground() || expired {
                self.request_redraw();
            }
            self.next_poll = now + Duration::from_secs(1);
        }
        // 동기화 출력(DEC 2026)이 끝나지 않은 탭이 있으면 기한에 깨어 반영한다
        let deadline = self
            .panes
            .iter()
            .filter_map(|t| t.session.sync_deadline())
            .chain([self.next_poll])
            .min();
        match deadline {
            Some(d) if d <= Instant::now() => {
                for t in &self.panes {
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
