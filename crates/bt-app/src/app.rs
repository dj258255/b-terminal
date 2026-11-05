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
use winit::window::{Window, WindowId};

use crate::keys::{self, Shortcut};
use crate::mouse;
use crate::tabbar;

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

pub struct Tab {
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
}

impl Tab {
    /// 탭 줄·창 제목에 쓸 이름: 셸이 보낸 제목 → 전경 프로세스 이름
    fn display_title(&self) -> String {
        if !self.title.is_empty() {
            return self.title.clone();
        }
        self.fg_name.clone().unwrap_or_else(|| "셸".into())
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
    next_poll: Instant,
    control: Option<crate::control::ControlServer>,
}

impl App {
    pub fn new(config: AppConfig, proxy: EventLoopProxy<UserEvent>, first: SpawnOptions) -> Self {
        let font_size = config.font_size;
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
            next_poll: Instant::now(),
            control: None,
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

    /// 격자를 그릴 자리(픽셀): x, y, 폭, 높이
    fn grid_rect(&self) -> (i32, i32, u32, u32) {
        let Some(g) = &self.gfx else { return (0, 0, 0, 0) };
        let size = g.window.inner_size();
        let px = |pt: f32| (pt as f64 * self.scale).round() as i32;
        let (left, right) = (px(self.config.padding_x.0), px(self.config.padding_x.1));
        let (top, bottom) = (px(self.config.padding_y.0), px(self.config.padding_y.1));
        let x = left;
        let y = self.tabbar_height() as i32 + top;
        let w = (size.width as i32 - x - right).max(0) as u32;
        let h = (size.height as i32 - y - bottom).max(0) as u32;
        (x, y, w, h)
    }

    /// 탭 줄 높이. 탭이 하나면 없다(Ghostty처럼)
    fn tabbar_height(&self) -> u32 {
        match &self.fonts {
            Some(f) if self.tabs.len() > 1 => tabbar::height_px(f),
            _ => 0,
        }
    }

    fn activate(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
            self.tabs[index].unread = false;
            self.request_redraw();
        }
    }

    /// 전경 프로세스 이름을 본다(셸이 제목을 보내지 않을 때 탭 이름으로 쓴다). 바뀐 것이 있으면 참
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
            t.fg_name = bt_term::process::process_name(pid);
            changed = true;
        }
        changed
    }

    /// 탭이 알림을 보냈다. 보고 있는 탭이 아니면 안 읽음으로 두고, 창을 보고 있지 않으면 데스크톱 알림을 띄운다
    fn on_notification(&mut self, idx: usize, note: bt_term::Notification) {
        let watching = self.focused && idx == self.active;
        let t = &mut self.tabs[idx];
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
            scrollback: self.config.scrollback_lines,
            cursor_shape: Some(match self.config.cursor_style {
                bt_core::config::CursorStyle::Block => CursorShape::Block,
                bt_core::config::CursorStyle::Bar => CursorShape::Beam,
                bt_core::config::CursorStyle::Underline => CursorShape::Underline,
                bt_core::config::CursorStyle::BlockHollow => CursorShape::HollowBlock,
            }),
            cursor_blink: self.config.cursor_blink.unwrap_or(false),
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
                    unread: false,
                    cwd: None,
                    last_desktop: None,
                });
                self.active = self.tabs.len() - 1;
                // 탭이 하나에서 둘이 되면 탭 줄이 생겨 격자가 줄어든다
                self.resize_all();
                Some(id)
            }
            Err(e) => {
                log::error!("셸을 띄우지 못했습니다: {e}");
                None
            }
        }
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
                        title: t.display_title(),
                        active: i == self.active,
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
        }
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
        // 탭이 하나가 되면 탭 줄이 사라져 격자가 커진다
        self.resize_all();
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
            Shortcut::FontReset => self.set_font_size(self.config.font_size),
            Shortcut::NewTab => {
                // 새 탭은 지금 탭의 폴더에서 연다(OSC 7로 알려진 것, 없으면 프로세스에게 묻는다)
                let cwd = self
                    .active_tab()
                    .and_then(|t| t.cwd.clone().or_else(|| t.session.cwd()));
                let mut opts = self.default_spawn();
                if cwd.is_some() && self.config.working_directory == bt_core::config::WorkingDirectory::Inherit {
                    opts.cwd = cwd;
                }
                self.open_tab(opts);
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
        let labels: Vec<tabbar::TabLabel> = if self.tabs.len() > 1 {
            self.tabs
                .iter()
                .enumerate()
                .map(|(i, t)| tabbar::TabLabel {
                    title: t.display_title(),
                    active: i == self.active,
                    unread: t.unread,
                })
                .collect()
        } else {
            Vec::new()
        };
        let title = self
            .active_tab()
            .map(|t| t.display_title())
            .unwrap_or_else(|| "b-terminal".into());
        let title = self.config.title.clone().unwrap_or(title);
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
            if !labels.is_empty() {
                tabbar::draw(&mut canvas, fonts, &theme, &labels);
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
                    selection_fg: theme.selection_foreground,
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
                    SessionEvent::Exited(code) => {
                        self.tabs[idx].exited = Some(code);
                        // 셸이 끝나면 탭을 닫는다(Ghostty 기본과 같다)
                        self.close_tab(idx, event_loop);
                        return;
                    }
                    // 진행 표시(OSC 9;4)는 아직 그리지 않는다
                    SessionEvent::Bell | SessionEvent::Output | SessionEvent::Progress { .. } => {}
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
                let meta = self.config.option_as_alt != bt_core::config::OptionAsAlt::None;
                let bytes = keys::encode(&event, self.mods, app_cursor, meta);
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
                    ElementState::Released => {
                        self.selecting = false;
                        // copy-on-select(Ghostty 기본 켬): 고르기를 마치면 바로 클립보드로
                        if self.config.copy_on_select {
                            if let Some(text) = self.active_tab().and_then(|t| t.session.selection_text()) {
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
            if self.poll_foreground() {
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
