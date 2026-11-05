//! 창 하나, 탭 여럿. winit 이벤트를 받아 PTY에 쓰고 화면을 그린다

use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use bt_core::Theme;
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

/// 다른 스레드(PTY 읽기)가 창 스레드를 깨우는 사건
#[derive(Debug, Clone)]
pub enum UserEvent {
    /// 어떤 탭의 화면이 바뀌었다(여러 번 와도 한 번 그린다)
    Redraw,
    Session(u64, SessionEvent),
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
        let pad = (self.config.padding as f64 * self.scale).round() as i32;
        let x = pad;
        let y = pad;
        let w = (size.width as i32 - x - pad).max(0) as u32;
        let h = (size.height as i32 - y - pad).max(0) as u32;
        (x, y, w, h)
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

    pub fn open_tab(&mut self, options: SpawnOptions) {
        let id = self.next_id;
        self.next_id += 1;
        let size = self.grid_size();
        let options = SpawnOptions {
            scrollback: self.config.scrollback,
            ..options
        };
        match Session::spawn(options, size, self.config.theme.clone(), self.sink(id)) {
            Ok(session) => {
                self.tabs.push(Tab {
                    id,
                    session,
                    title: String::new(),
                    exited: None,
                });
                self.active = self.tabs.len() - 1;
            }
            Err(e) => log::error!("셸을 띄우지 못했습니다: {e}"),
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
                let cwd = None;
                self.open_tab(SpawnOptions {
                    cwd,
                    ..Default::default()
                });
            }
            Shortcut::CloseTab => self.close_tab(self.active, event_loop),
            Shortcut::NextTab if !self.tabs.is_empty() => self.active = (self.active + 1) % self.tabs.len(),
            Shortcut::PrevTab if !self.tabs.is_empty() => {
                self.active = (self.active + self.tabs.len() - 1) % self.tabs.len()
            }
            Shortcut::GotoTab(i) if i < self.tabs.len() => self.active = i,
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
        if self.tabs.is_empty() {
            let first = self.first.take().unwrap_or_default();
            self.open_tab(first);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Redraw => self.request_redraw(),
            UserEvent::Session(id, e) => {
                let Some(idx) = self.tabs.iter().position(|t| t.id == id) else {
                    return;
                };
                match e {
                    SessionEvent::Title(t) => self.tabs[idx].title = t,
                    SessionEvent::Exited(code) => {
                        self.tabs[idx].exited = Some(code);
                        // 셸이 끝나면 탭을 닫는다(Ghostty 기본과 같다)
                        self.close_tab(idx, event_loop);
                        return;
                    }
                    SessionEvent::Bell | SessionEvent::Notification(_) | SessionEvent::Output => {}
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
        // 동기화 출력(DEC 2026)이 끝나지 않은 탭이 있으면 기한에 깨어 반영한다
        let deadline = self.tabs.iter().filter_map(|t| t.session.sync_deadline()).min();
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
