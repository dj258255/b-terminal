//! 접근성(VoiceOver) 트리(#560)
//!
//! 창 노드(Window) 아래 보이는 칸마다 터미널 노드(Terminal), 그 아래 보이는 줄마다 글 덩어리 노드(TextRun)를 둔다.
//! Ghostty macOS는 스크롤백까지 화면 전체 글을 내놓지만(`surfaceview.swift` 2347~2445줄) 여기는 **보이는 화면만** 내놓는다:
//! 스크롤백은 기본 10MB라 칸마다 500ms에 한 번 그만큼의 글을 만들어 접근성 트리로 넘기면 출력이 쏟아질 때 앱이 느려진다.
//!
//! 이 모듈의 함수들은 `Screen`과 값만 받는 순수 함수(트리 만들기, 늦추기 판단)이고, 창·어댑터를 쥐는 것은 `WindowA11y`뿐이다.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use accesskit::{
    Action, Node, NodeId, Rect, Role, TextDirection, TextPosition, TextSelection, TreeId, TreeInfo, TreeUpdate,
};
use bt_term::screen::{Cell, CellFlags, Screen};
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::Window;

use crate::app::UserEvent;

/// 칸 글을 다시 만드는 가장 짧은 간격(Ghostty의 화면 글 캐시 500ms, `surfaceview.swift` 245~293줄)
pub const TEXT_REFRESH: Duration = Duration::from_millis(500);
/// 선택이 바뀐 뒤 알리기까지 기다리는 시간(Ghostty의 `.debounce(for: .milliseconds(100))`, 같은 파일 300~323줄)
pub const SELECTION_DEBOUNCE: Duration = Duration::from_millis(100);

/// 이 어댑터가 일하는 플랫폼. Linux의 `accesskit_unix`(AT-SPI)는 의존성이 커서 껐으므로 그곳의 어댑터는 아무 일도 하지 않는다.
/// 일하지 않는 곳에서는 창을 보이지 않게 만들었다 보이게 하는 순서도 따르지 않는다
pub const SUPPORTED: bool = cfg!(any(target_os = "macos", target_os = "windows"));

/// 줄 번호에 쓰는 아래 20비트. 줄 노드 번호 = 칸 번호 << 20 | (줄 + 1), 칸 노드 번호 = 칸 번호 << 20
const ROW_BITS: u32 = 20;
/// 칸 하나가 내놓는 줄 수의 상한. 줄 번호가 모두 1인 값은 창 노드 번호와 겹칠 수 있어 하나를 비운다
const MAX_LINES: usize = (1 << ROW_BITS) - 2;
const PANE_MASK: u64 = (1 << (64 - ROW_BITS)) - 1;
/// 글 덩어리 하나의 글자 길이(바이트)는 u8이다
const MAX_CHAR_LEN: usize = u8::MAX as usize;

/// 창 노드
pub const ROOT: NodeId = NodeId(u64::MAX);

pub fn pane_node(pane: u64) -> NodeId {
    NodeId((pane & PANE_MASK) << ROW_BITS)
}

pub fn line_node(pane: u64, row: usize) -> NodeId {
    let row = (row.min(MAX_LINES - 1) as u64) + 1;
    NodeId(((pane & PANE_MASK) << ROW_BITS) | row)
}

/// 칸 노드나 줄 노드가 속한 칸. 창 노드는 없다
pub fn pane_of(node: NodeId) -> Option<u64> {
    (node != ROOT).then_some(node.0 >> ROW_BITS)
}

/// 한 줄의 글과 접근성이 요구하는 글자 정보
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Line {
    /// 끝 공백을 뗀 글. 넓은 글자의 뒤 칸(`SPACER`)은 들어 있지 않다
    pub text: String,
    /// 글자(칸 하나, 결합 문자가 많아 255바이트를 넘으면 쪼갠 조각)마다 UTF-8 바이트 수
    pub lens: Vec<u8>,
    /// 글자마다 (시작 열, 차지하는 칸 수). 쪼갠 뒷 조각은 칸 수 0
    pub cols: Vec<(u32, u32)>,
    /// 낱말이 시작하는 글자 번호(앞 글자가 공백이고 이 글자는 아닌 곳). u8이라 255번째 글자까지만 적는다
    pub words: Vec<u8>,
    /// 열마다 (그 칸의 첫 글자 번호, 끝 글자 번호 다음). 선택 범위를 글자 위치로 바꾸는 데 쓴다.
    /// 넓은 글자의 뒤 칸은 앞 칸과 같고, 끝 공백 칸은 줄 끝이다
    spans: Vec<(usize, usize)>,
}

/// 고른 범위: (줄, 글자 위치) 둘
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub anchor: (usize, usize),
    pub focus: (usize, usize),
}

/// 칸에 보이는 화면의 글
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PaneText {
    pub cols: usize,
    pub lines: Vec<Line>,
    pub selection: Option<Selection>,
}

/// 칸이 창 안에서 놓인 곳(물리 픽셀, 창 왼쪽 위가 원점)과 칸 크기
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geom {
    pub x: f64,
    pub y: f64,
    pub cell_w: f64,
    pub cell_h: f64,
}

/// 화면 글에서 보이지 않거나 위험한 글자는 공백으로: 가린 글자(SGR 8)는 읽어 주지 않고, 제어 문자는 줄 모양을 깨므로 쓰지 않는다
fn glyph(cell: &Cell) -> (char, Vec<char>) {
    if cell.flags.contains(CellFlags::HIDDEN) {
        return (' ', Vec::new());
    }
    let ch = cell.draw_ch();
    let ch = if ch.is_control() { ' ' } else { ch };
    let comb = cell
        .draw_combining()
        .iter()
        .copied()
        .filter(|c| !c.is_control())
        .collect();
    (ch, comb)
}

fn is_blank(cell: &Cell) -> bool {
    let (ch, comb) = glyph(cell);
    ch == ' ' && comb.is_empty()
}

impl Line {
    fn entry(&mut self, len: usize, col: usize, width: u32) {
        self.lens.push(len as u8);
        self.cols.push((col as u32, width));
    }
}

/// 화면 한 줄을 글과 글자 정보로
fn line_of(screen: &Screen, row: usize) -> Line {
    let mut line = Line::default();
    let cols = screen.cols;
    let start = row.saturating_mul(cols);
    let Some(cells) = screen.cells.get(start..start.saturating_add(cols)) else {
        return line;
    };
    let end = cells.iter().rposition(|c| !is_blank(c)).map_or(0, |i| i + 1);
    let mut prev_space = false;
    for (col, cell) in cells.iter().enumerate() {
        if cell.flags.contains(CellFlags::SPACER) {
            let same = line.spans.last().copied().unwrap_or((0, 0));
            line.spans.push(same);
            continue;
        }
        if col >= end {
            let n = line.lens.len();
            line.spans.push((n, n));
            continue;
        }
        let first = line.lens.len();
        let (ch, comb) = glyph(cell);
        let width = if cell.flags.contains(CellFlags::WIDE) { 2 } else { 1 };
        let space = ch.is_whitespace() && comb.is_empty();
        if first > 0 && prev_space && !space && first <= MAX_CHAR_LEN {
            line.words.push(first as u8);
        }
        prev_space = space;
        // 한 칸의 글자(앞 글자 + 결합 문자)가 255바이트를 넘으면 글자 경계에서 쪼갠다
        let mut chunk = 0usize;
        let mut pieces = 0usize;
        for c in std::iter::once(ch).chain(comb) {
            let l = c.len_utf8();
            if chunk > 0 && chunk + l > MAX_CHAR_LEN {
                line.entry(chunk, col, if pieces == 0 { width } else { 0 });
                pieces += 1;
                chunk = 0;
            }
            chunk += l;
            line.text.push(c);
        }
        line.entry(chunk, col, if pieces == 0 { width } else { 0 });
        line.spans.push((first, line.lens.len()));
    }
    line
}

/// 고른 칸들의 첫 칸과 마지막 칸((줄, 열) 둘). 사각형 선택이어도 첫 칸부터 마지막 칸까지의 글로 근사한다
pub type Span = ((usize, usize), (usize, usize));

fn selection_span(screen: &Screen) -> Option<Span> {
    let cols = screen.cols;
    if cols == 0 || screen.cols.checked_mul(screen.rows) != Some(screen.cells.len()) {
        return None;
    }
    let mut first = None;
    let mut last = None;
    for (i, c) in screen.cells.iter().enumerate() {
        if c.flags.contains(CellFlags::SELECTED) {
            first.get_or_insert(i);
            last = Some(i);
        }
    }
    Some(((first? / cols, first? % cols), (last? / cols, last? % cols)))
}

/// 보이는 화면을 줄과 선택으로. 복사·`btctl read`가 쓰는 `Screen::row_text`와 같은 글을 내되(시험이 맞춘다),
/// 접근성에 필요한 글자별 길이·낱말 시작·선택 위치를 함께 센다
pub fn pane_text(screen: &Screen) -> PaneText {
    let rows = screen.rows.min(MAX_LINES);
    let lines: Vec<Line> = (0..rows).map(|r| line_of(screen, r)).collect();
    let selection = selection_span(screen).and_then(|((r0, c0), (r1, c1))| {
        let a = lines.get(r0)?.spans.get(c0)?.0;
        let f = lines.get(r1)?.spans.get(c1)?.1;
        Some(Selection {
            anchor: (r0, a),
            focus: (r1, f),
        })
    });
    PaneText {
        cols: screen.cols,
        lines,
        selection,
    }
}

/// 트리에 넣을 칸 하나
pub struct PaneIn<'a> {
    pub id: u64,
    pub name: &'a str,
    pub text: &'a PaneText,
    pub geom: Option<Geom>,
    /// 이 칸의 노드들을 이번 갱신에 싣는가(바뀌었거나 트리에 아직 없다)
    pub send: bool,
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect {
        x0: x,
        y0: y,
        x1: x + w,
        y1: y + h,
    }
}

fn run_node(line: &Line, last: bool, cols: usize, row: usize, geom: Option<Geom>) -> Node {
    let mut n = Node::new(Role::TextRun);
    let mut value = line.text.clone();
    let mut lens = line.lens.clone();
    // 줄 끝의 줄바꿈은 글에 넣고 한 글자로 센다(accesskit 글 덩어리 규칙)
    if !last {
        value.push('\n');
        lens.push(1);
    }
    n.set_value(value);
    n.set_character_lengths(lens);
    if !line.words.is_empty() {
        n.set_word_starts(line.words.clone());
    }
    if let Some(g) = geom {
        n.set_text_direction(TextDirection::LeftToRight);
        n.set_bounds(rect(g.x, g.y + row as f64 * g.cell_h, cols as f64 * g.cell_w, g.cell_h));
        let mut pos: Vec<f32> = line.cols.iter().map(|&(c, _)| (c as f64 * g.cell_w) as f32).collect();
        let mut wid: Vec<f32> = line.cols.iter().map(|&(_, w)| (w as f64 * g.cell_w) as f32).collect();
        if !last {
            let end = line.cols.last().map_or(0, |&(c, w)| c + w);
            pos.push((end as f64 * g.cell_w) as f32);
            wid.push(0.0);
        }
        n.set_character_positions(pos);
        n.set_character_widths(wid);
    }
    n
}

/// 칸들의 화면 글·선택·포커스에서 접근성 트리 갱신을 만든다. 창 노드는 늘 싣고, 칸은 `send`인 것만 싣는다.
/// 초점은 `focus` 칸의 터미널 노드(그 칸이 없으면 창 노드)
pub fn build_update(title: &str, panes: &[PaneIn<'_>], focus: Option<u64>) -> TreeUpdate {
    let mut root = Node::new(Role::Window);
    root.set_label(title);
    root.set_children(panes.iter().map(|p| pane_node(p.id)).collect::<Vec<_>>());
    let mut nodes = vec![(ROOT, root)];
    for p in panes.iter().filter(|p| p.send) {
        let text = p.text;
        let mut term = Node::new(Role::Terminal);
        if !p.name.is_empty() {
            term.set_label(p.name);
        }
        term.set_description("터미널 화면");
        term.add_action(Action::Focus);
        term.set_children((0..text.lines.len()).map(|r| line_node(p.id, r)).collect::<Vec<_>>());
        if let Some(g) = p.geom {
            term.set_bounds(rect(
                g.x,
                g.y,
                text.cols as f64 * g.cell_w,
                text.lines.len() as f64 * g.cell_h,
            ));
        }
        if let Some(sel) = text.selection {
            let at = |(row, character_index): (usize, usize)| TextPosition {
                node: line_node(p.id, row),
                character_index,
            };
            term.set_text_selection(TextSelection {
                anchor: at(sel.anchor),
                focus: at(sel.focus),
            });
        }
        nodes.push((pane_node(p.id), term));
        for (row, line) in text.lines.iter().enumerate() {
            let last = row + 1 == text.lines.len();
            nodes.push((line_node(p.id, row), run_node(line, last, text.cols, row, p.geom)));
        }
    }
    let focus = focus
        .filter(|f| panes.iter().any(|p| p.id == *f))
        .map_or(ROOT, pane_node);
    TreeUpdate {
        nodes,
        tree: Some({
            let mut info = TreeInfo::new(ROOT);
            info.toolkit_name = Some("b-terminal".into());
            info
        }),
        tree_id: TreeId::ROOT,
        focus,
    }
}

/// 칸 하나의 글 갱신 늦추기: 글은 500ms에 한 번 이하, 선택이 바뀌면 100ms 뒤
#[derive(Debug, Default)]
pub struct Gate {
    last: Option<Instant>,
    sel: Option<Span>,
    /// 미룬 갱신을 해야 할 때
    wait: Option<Instant>,
}

impl Gate {
    /// 지금 글을 새로 만들어야 하면 참(그러면 지금을 마지막으로 적는다). 거짓이면 미룬 때를 적어 둔다
    pub fn check(&mut self, now: Instant, sel: Option<Span>) -> bool {
        let Some(last) = self.last else {
            return self.fire(now, sel);
        };
        let since = now.saturating_duration_since(last);
        let sel_changed = sel != self.sel;
        if since >= TEXT_REFRESH || (sel_changed && since >= SELECTION_DEBOUNCE) {
            return self.fire(now, sel);
        }
        let at = last + if sel_changed { SELECTION_DEBOUNCE } else { TEXT_REFRESH };
        self.wait = Some(self.wait.map_or(at, |w| w.min(at)));
        false
    }

    fn fire(&mut self, now: Instant, sel: Option<Span>) -> bool {
        self.last = Some(now);
        self.sel = sel;
        self.wait = None;
        true
    }

    /// 미룬 갱신이 있으면 그것을 할 때
    pub fn wake_at(&self) -> Option<Instant> {
        self.wait
    }

    /// 글을 잃어버렸으니 다음 확인에서 바로 만들게 한다
    fn forget(&mut self) {
        self.last = None;
    }
}

/// 어댑터로 이미 보낸 것
struct Sent {
    ids: Vec<u64>,
    focus: Option<u64>,
    title: String,
    meta: HashMap<u64, (String, Option<Geom>)>,
}

/// 칸이 이번에 보여 주는 것(보이는 칸마다 하나)
pub struct Frame<'a> {
    pub id: u64,
    pub name: String,
    pub screen: &'a Screen,
    pub geom: Option<Geom>,
}

/// 창 하나의 접근성 상태: 클라이언트(VoiceOver)가 붙었는지, 칸마다 만들어 둔 글, 이미 보낸 것.
/// 클라이언트가 붙기 전에는 화면 글을 만들지 않는다
#[derive(Default)]
pub struct TreeSync {
    active: bool,
    gates: HashMap<u64, Gate>,
    texts: HashMap<u64, PaneText>,
    sent: Option<Sent>,
}

impl TreeSync {
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// 클라이언트가 첫 트리를 청했다(이미 켜져 있어도 또 올 수 있다). 다음 갱신은 전체 트리다
    pub fn activate(&mut self) {
        self.active = true;
        self.gates.clear();
        self.texts.clear();
        self.sent = None;
    }

    pub fn deactivate(&mut self) {
        *self = Self::default();
    }

    #[cfg(test)]
    fn has_text(&self, pane: u64) -> bool {
        self.texts.contains_key(&pane)
    }

    /// 미룬 갱신을 해야 할 가장 이른 때
    pub fn wake_at(&self) -> Option<Instant> {
        self.gates.values().filter_map(Gate::wake_at).min()
    }

    /// 지금 보이는 칸들을 본다. 보낼 것이 있으면 트리 갱신을 돌려준다.
    /// 포커스·창 제목·칸 수·칸 모양은 바로, 글은 칸마다 `Gate`가 허락할 때만 다시 만든다
    pub fn observe(
        &mut self,
        now: Instant,
        title: &str,
        frames: &[Frame<'_>],
        focus: Option<u64>,
    ) -> Option<TreeUpdate> {
        if !self.active {
            return None;
        }
        let ids: Vec<u64> = frames.iter().map(|f| f.id).collect();
        self.gates.retain(|id, _| ids.contains(id));
        self.texts.retain(|id, _| ids.contains(id));
        let mut dirty: HashSet<u64> = HashSet::new();
        for f in frames {
            let gate = self.gates.entry(f.id).or_default();
            if !self.texts.contains_key(&f.id) {
                gate.forget();
            }
            if gate.check(now, selection_span(f.screen)) {
                let text = pane_text(f.screen);
                if self.texts.get(&f.id) != Some(&text) {
                    self.texts.insert(f.id, text);
                    dirty.insert(f.id);
                }
            }
        }
        let focus = focus.filter(|f| ids.contains(f));
        let resend = |f: &Frame<'_>| match &self.sent {
            None => true,
            Some(s) => {
                dirty.contains(&f.id)
                    || !s.ids.contains(&f.id)
                    || s.meta.get(&f.id).is_none_or(|(n, g)| *n != f.name || *g != f.geom)
            }
        };
        let send: Vec<bool> = frames.iter().map(resend).collect();
        let unchanged = self
            .sent
            .as_ref()
            .is_some_and(|s| s.ids == ids && s.focus == focus && s.title == title)
            && !send.contains(&true);
        if unchanged {
            return None;
        }
        let panes: Vec<PaneIn<'_>> = frames
            .iter()
            .zip(&send)
            .filter_map(|(f, &send)| {
                Some(PaneIn {
                    id: f.id,
                    name: &f.name,
                    text: self.texts.get(&f.id)?,
                    geom: f.geom,
                    send,
                })
            })
            .collect();
        let update = build_update(title, &panes, focus);
        self.sent = Some(Sent {
            ids,
            focus,
            title: title.to_string(),
            meta: frames.iter().map(|f| (f.id, (f.name.clone(), f.geom))).collect(),
        });
        Some(update)
    }
}

/// 접근성 클라이언트가 보낸 요청 중 앱이 하는 것
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    /// 화면 읽기 프로그램이 처음으로 트리를 청했다
    InitialTree,
    /// 그 칸으로 포커스(칸 노드나 그 줄 노드에 온 Focus)
    Focus(u64),
    /// 화면 읽기 프로그램이 떠났다
    Deactivated,
    /// 나머지 동작(클릭·스크롤·글 바꾸기 등)은 하지 않는다
    Ignored,
}

pub fn classify(event: &accesskit_winit::WindowEvent) -> Request {
    use accesskit_winit::WindowEvent as E;
    match event {
        E::InitialTreeRequested => Request::InitialTree,
        E::AccessibilityDeactivated => Request::Deactivated,
        E::ActionRequested(req) if req.action == Action::Focus && req.target_tree == TreeId::ROOT => {
            pane_of(req.target_node).map_or(Request::Ignored, Request::Focus)
        }
        E::ActionRequested(_) => Request::Ignored,
    }
}

impl From<accesskit_winit::Event> for UserEvent {
    fn from(e: accesskit_winit::Event) -> Self {
        UserEvent::Access(e.window_id, classify(&e.window_event))
    }
}

/// 창 하나에 붙는 접근성: 어댑터(macOS·Windows에서만 만든다)와 `TreeSync`
pub struct WindowA11y {
    adapter: Option<accesskit_winit::Adapter>,
    sync: TreeSync,
}

impl WindowA11y {
    /// 어댑터는 창이 처음 보이기 전에 붙여야 한다(accesskit_winit의 요구, 이미 보이면 panic).
    /// 부르는 쪽이 창을 `with_visible(false)`로 만들고, 이것을 만든 뒤 `set_visible(true)`로 보인다
    pub fn new(event_loop: &ActiveEventLoop, window: &Window, proxy: EventLoopProxy<UserEvent>) -> Self {
        let adapter = SUPPORTED.then(|| accesskit_winit::Adapter::with_event_loop_proxy(event_loop, window, proxy));
        Self {
            adapter,
            sync: TreeSync::default(),
        }
    }

    /// 창 사건은 앱이 처리하기 전에 어댑터에 먼저 알린다(accesskit_winit의 요구)
    pub fn process_event(&mut self, window: &Window, event: &winit::event::WindowEvent) {
        if let Some(a) = &mut self.adapter {
            a.process_event(window, event);
        }
    }

    pub fn is_active(&self) -> bool {
        self.sync.is_active()
    }

    pub fn activate(&mut self) {
        self.sync.activate();
    }

    pub fn deactivate(&mut self) {
        self.sync.deactivate();
    }

    pub fn wake_at(&self) -> Option<Instant> {
        self.sync.wake_at()
    }

    /// 지금 보이는 칸들을 본다. 보낼 것이 있으면 어댑터에 올린다
    pub fn observe(&mut self, now: Instant, title: &str, frames: &[Frame<'_>], focus: Option<u64>) {
        let Some(update) = self.sync.observe(now, title, frames, focus) else {
            return;
        };
        if let Some(a) = &mut self.adapter {
            a.update_if_active(|| update);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Action, ActionRequest, Role, TreeId};
    use bt_core::Rgb;
    use bt_term::screen::{Cell, CellFlags, Cursor, CursorShape, Modes, Screen};
    use std::time::{Duration, Instant};

    fn is_wide(ch: char) -> bool {
        matches!(ch as u32, 0x1100..=0x11FF | 0x2E80..=0xD7FF | 0xF900..=0xFAFF | 0x1F300..=0x1FAFF)
    }

    /// 글을 칸에 채운 화면: 넓은 글자는 앞 칸 WIDE + 뒤 칸 SPACER. `\u{301}`처럼 폭 0인 글자는 앞 칸의 결합 문자로 붙는다
    fn screen(rows: &[&str], cols: usize) -> Screen {
        let blank = Cell::blank(Rgb::new(255, 255, 255), Rgb::new(0, 0, 0));
        let mut cells = vec![blank; cols * rows.len()];
        for (r, line) in rows.iter().enumerate() {
            let mut c = 0;
            let mut last: Option<usize> = None;
            for ch in line.chars() {
                if ('\u{300}'..='\u{36f}').contains(&ch) {
                    cells[r * cols + last.unwrap()].combining.push(ch);
                    continue;
                }
                last = Some(c);
                cells[r * cols + c].ch = ch;
                if is_wide(ch) {
                    cells[r * cols + c].flags.insert(CellFlags::WIDE);
                    cells[r * cols + c + 1].flags.insert(CellFlags::SPACER);
                    c += 2;
                } else {
                    c += 1;
                }
            }
        }
        Screen {
            cols,
            rows: rows.len(),
            cells,
            cursor: Cursor {
                col: 0,
                row: 0,
                shape: CursorShape::Block,
                wide: false,
                visible: false,
                blinking: false,
            },
            modes: Modes::default(),
            display_offset: 0,
            pixel_shift: 0,
            edge: Vec::new(),
            history: 0,
            background: Rgb::new(0, 0, 0),
            foreground: Rgb::new(255, 255, 255),
            cursor_color: None,
            chrome_background: Rgb::new(0, 0, 0),
            links: Vec::new(),
            images: Vec::new(),
        }
    }

    fn select(s: &mut Screen, row: usize, cols: std::ops::RangeInclusive<usize>) {
        for c in cols {
            let i = row * s.cols + c;
            s.cells[i].flags.insert(CellFlags::SELECTED);
        }
    }

    fn node(u: &accesskit::TreeUpdate, id: accesskit::NodeId) -> &accesskit::Node {
        &u.nodes.iter().find(|(i, _)| *i == id).expect("노드가 있다").1
    }

    fn pane<'a>(id: u64, text: &'a PaneText, send: bool) -> PaneIn<'a> {
        PaneIn {
            id,
            name: "셸",
            text,
            geom: None,
            send,
        }
    }

    #[test]
    fn two_panes_make_two_terminal_nodes_and_focus_follows() {
        let a = pane_text(&screen(&["one", "two"], 8));
        let b = pane_text(&screen(&["셋", ""], 8));
        let u = build_update("제목", &[pane(1, &a, true), pane(2, &b, true)], Some(2));
        let root = node(&u, ROOT);
        assert_eq!(root.role(), Role::Window);
        assert_eq!(root.label(), Some("제목"));
        assert_eq!(root.children(), &[pane_node(1), pane_node(2)]);
        let terms: Vec<_> = u.nodes.iter().filter(|(_, n)| n.role() == Role::Terminal).collect();
        assert_eq!(terms.len(), 2);
        assert_eq!(u.focus, pane_node(2));
        assert_eq!(node(&u, pane_node(1)).children(), &[line_node(1, 0), line_node(1, 1)]);
        assert_eq!(
            node(&u, line_node(1, 1)).value(),
            Some("two"),
            "마지막 줄에는 줄바꿈이 없다"
        );
        assert_eq!(node(&u, line_node(1, 0)).value(), Some("one\n"));
        assert_eq!(u.tree.as_ref().map(|t| t.root), Some(ROOT));
        assert_eq!(u.tree_id, TreeId::ROOT);
    }

    #[test]
    fn focus_falls_back_to_the_window_when_the_pane_is_gone() {
        let a = pane_text(&screen(&["x"], 4));
        let u = build_update("t", &[pane(1, &a, true)], Some(9));
        assert_eq!(u.focus, ROOT);
        let u = build_update("t", &[pane(1, &a, true)], None);
        assert_eq!(u.focus, ROOT);
    }

    #[test]
    fn wide_chars_count_once_and_spacers_stay_out_of_the_text() {
        // 가(3바이트, 두 칸) a(1) 😀(4바이트, 두 칸) b(1)
        let s = screen(&["가a😀b"], 10);
        let t = pane_text(&s);
        let line = &t.lines[0];
        assert_eq!(line.text, "가a😀b");
        assert_eq!(line.text, s.row_text(0), "복사·읽기용 `row_text`와 같은 글이다");
        assert_eq!(line.lens, vec![3, 1, 4, 1]);
        assert_eq!(line.cols, vec![(0, 2), (2, 1), (3, 2), (5, 1)]);
    }

    #[test]
    fn combining_marks_stay_with_their_cell_and_lengths_add_up() {
        let s = screen(&["e\u{301}x"], 6);
        let line = &pane_text(&s).lines[0];
        assert_eq!(line.text, "e\u{301}x");
        assert_eq!(line.lens, vec![3, 1]);
    }

    #[test]
    fn a_cell_with_a_flood_of_combining_marks_is_split_under_255_bytes() {
        let mut s = screen(&["a"], 4);
        // 프로그램이 한 칸에 결합 문자를 수천 개 붙여도 길이가 u8을 넘지 않고, 합이 글 길이와 같다
        s.cells[0].combining = vec!['\u{301}'; 5000];
        let line = &pane_text(&s).lines[0];
        assert_eq!(line.lens.iter().map(|&l| l as usize).sum::<usize>(), line.text.len());
        assert!(line.lens.len() > 1);
        assert!(line.text.is_char_boundary(line.text.len()));
        assert_eq!(line.cols.len(), line.lens.len());
    }

    #[test]
    fn concealed_and_control_chars_do_not_leak_into_the_text() {
        let mut s = screen(&["ab"], 4);
        s.cells[0].flags.insert(CellFlags::HIDDEN);
        s.cells[1].ch = '\u{85}';
        let line = &pane_text(&s).lines[0];
        assert_eq!(line.text, "", "가린 글자와 제어 문자는 공백이 되고 줄 끝 공백은 떼인다");
        assert_eq!(line.lens, Vec::<u8>::new());
    }

    #[test]
    fn every_text_run_has_lengths_that_add_up_to_its_value() {
        let t = pane_text(&screen(&["가나 다", "e\u{301}", "", "😀 x"], 12));
        let u = build_update("t", &[pane(1, &t, true)], Some(1));
        for (_, n) in u.nodes.iter().filter(|(_, n)| n.role() == Role::TextRun) {
            let sum: usize = n.character_lengths().iter().map(|&l| l as usize).sum();
            assert_eq!(sum, n.value().unwrap().len());
        }
    }

    #[test]
    fn word_starts_follow_spaces() {
        let t = pane_text(&screen(&["  ab cd"], 12));
        let u = build_update("t", &[pane(1, &t, true)], Some(1));
        // 앞 공백은 한 낱말이고 ab(2)·cd(5)가 새 낱말
        assert_eq!(node(&u, line_node(1, 0)).word_starts(), &[2, 5]);
    }

    #[test]
    fn selection_becomes_a_range_over_line_nodes_and_char_indexes() {
        let mut s = screen(&["abcdef", "가나다라", "xyz"], 10);
        select(&mut s, 0, 3..=5);
        select(&mut s, 1, 0..=3);
        let t = pane_text(&s);
        let sel = t.selection.expect("고른 곳이 있다");
        assert_eq!(sel.anchor, (0, 3));
        // 가(0,1칸)·나(2,3칸)를 골랐으니 글자 2개 → 끝은 2
        assert_eq!(sel.focus, (1, 2));
        let u = build_update("t", &[pane(7, &t, true)], Some(7));
        let ts = node(&u, pane_node(7)).text_selection().expect("트리에 선택이 든다");
        assert_eq!(ts.anchor.node, line_node(7, 0));
        assert_eq!(ts.anchor.character_index, 3);
        assert_eq!(ts.focus.node, line_node(7, 1));
        assert_eq!(ts.focus.character_index, 2);
    }

    #[test]
    fn selecting_only_a_wide_lead_cell_still_covers_the_whole_char() {
        let mut s = screen(&["a가b"], 8);
        select(&mut s, 0, 1..=1);
        let sel = pane_text(&s).selection.unwrap();
        assert_eq!((sel.anchor, sel.focus), ((0, 1), (0, 2)));
        // 뒤 칸만 골라도 같은 글자다
        let mut s = screen(&["a가b"], 8);
        select(&mut s, 0, 2..=2);
        let sel = pane_text(&s).selection.unwrap();
        assert_eq!((sel.anchor, sel.focus), ((0, 1), (0, 2)));
    }

    #[test]
    fn selection_in_trailing_blanks_clamps_to_the_line_end() {
        let mut s = screen(&["ab", "cd"], 8);
        select(&mut s, 0, 1..=7);
        let sel = pane_text(&s).selection.unwrap();
        assert_eq!((sel.anchor, sel.focus), ((0, 1), (0, 2)));
    }

    #[test]
    fn no_selection_means_no_selection_property() {
        let t = pane_text(&screen(&["ab"], 4));
        assert!(t.selection.is_none());
        let u = build_update("t", &[pane(1, &t, true)], Some(1));
        assert!(node(&u, pane_node(1)).text_selection().is_none());
    }

    #[test]
    fn geometry_gives_bounds_and_character_positions() {
        let t = pane_text(&screen(&["a가"], 6));
        let g = Geom {
            x: 10.0,
            y: 20.0,
            cell_w: 8.0,
            cell_h: 16.0,
        };
        let p = PaneIn {
            id: 1,
            name: "셸",
            text: &t,
            geom: Some(g),
            send: true,
        };
        let u = build_update("t", &[p], Some(1));
        let run = node(&u, line_node(1, 0));
        let b = run.bounds().expect("줄 사각형");
        assert_eq!((b.x0, b.y0, b.x1, b.y1), (10.0, 20.0, 58.0, 36.0));
        assert_eq!(run.character_positions(), Some(&[0.0f32, 8.0][..]));
        assert_eq!(run.character_widths(), Some(&[8.0f32, 16.0][..]));
    }

    #[test]
    fn node_ids_round_trip() {
        assert_eq!(pane_of(pane_node(5)), Some(5));
        assert_eq!(pane_of(line_node(5, 300)), Some(5));
        assert_eq!(pane_of(ROOT), None);
        assert_ne!(pane_node(1), line_node(1, 0));
        assert_ne!(line_node(1, 0), line_node(2, 0));
    }

    #[test]
    fn only_the_focus_action_on_a_pane_is_taken() {
        let req = |action, node| ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: node,
            data: None,
        };
        use accesskit_winit::WindowEvent as E;
        assert_eq!(
            classify(&E::ActionRequested(req(Action::Focus, pane_node(3)))),
            Request::Focus(3)
        );
        assert_eq!(
            classify(&E::ActionRequested(req(Action::Focus, line_node(3, 2)))),
            Request::Focus(3)
        );
        assert_eq!(
            classify(&E::ActionRequested(req(Action::Click, pane_node(3)))),
            Request::Ignored
        );
        assert_eq!(
            classify(&E::ActionRequested(req(Action::Focus, ROOT))),
            Request::Ignored
        );
        assert_eq!(classify(&E::InitialTreeRequested), Request::InitialTree);
        assert_eq!(classify(&E::AccessibilityDeactivated), Request::Deactivated);
    }

    #[test]
    fn gate_delays_the_second_text_refresh_inside_500ms() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let mut g = Gate::default();
        assert!(g.check(t0, None), "처음에는 바로 만든다");
        assert!(g.wake_at().is_none());
        assert!(!g.check(t0 + ms(100), None), "500ms 안의 두 번째는 미룬다");
        assert!(!g.check(t0 + ms(499), None));
        assert_eq!(g.wake_at(), Some(t0 + ms(500)), "미룬 것은 500ms 때에 깨운다");
        assert!(g.check(t0 + ms(500), None));
        assert!(g.wake_at().is_none());
    }

    #[test]
    fn gate_takes_a_selection_change_after_100ms() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let mut g = Gate::default();
        assert!(g.check(t0, None));
        let sel = Some(((0, 1), (0, 3)));
        assert!(!g.check(t0 + ms(50), sel), "선택은 100ms 디바운스");
        assert_eq!(g.wake_at(), Some(t0 + ms(100)));
        assert!(g.check(t0 + ms(100), sel));
        assert!(!g.check(t0 + ms(150), sel), "같은 선택이면 500ms를 기다린다");
    }

    fn sync_frame<'a>(id: u64, s: &'a Screen) -> Frame<'a> {
        Frame {
            id,
            name: "셸".into(),
            screen: s,
            geom: None,
        }
    }

    #[test]
    fn nothing_is_built_before_a_client_asks() {
        let s = screen(&["x"], 4);
        let mut sy = TreeSync::default();
        assert!(sy.observe(Instant::now(), "t", &[sync_frame(1, &s)], Some(1)).is_none());
        assert!(!sy.has_text(1), "클라이언트가 없으면 화면 글을 만들지 않는다");
    }

    #[test]
    fn sync_sends_focus_at_once_but_holds_back_output_within_500ms() {
        let ms = Duration::from_millis;
        let t0 = Instant::now();
        let s1 = screen(&["old"], 8);
        let s2 = screen(&["new"], 8);
        let mut sy = TreeSync::default();
        sy.activate();
        let first = sy
            .observe(t0, "t", &[sync_frame(1, &s1), sync_frame(2, &s1)], Some(1))
            .expect("처음은 전체");
        assert_eq!(
            first.nodes.iter().filter(|(_, n)| n.role() == Role::Terminal).count(),
            2
        );
        // 100ms 뒤 출력이 바뀌었지만 미룬다
        assert!(
            sy.observe(t0 + ms(100), "t", &[sync_frame(1, &s2), sync_frame(2, &s1)], Some(1))
                .is_none()
        );
        // 포커스가 바뀌면 바로 보내고, 글은 아직 옛것이다(다시 만들지 않는다)
        let f = sy
            .observe(t0 + ms(200), "t", &[sync_frame(1, &s2), sync_frame(2, &s1)], Some(2))
            .expect("포커스는 바로");
        assert_eq!(f.focus, pane_node(2));
        assert_eq!(f.nodes.len(), 1, "바뀐 것이 포커스뿐이면 창 노드만 보낸다");
        // 창 제목도 바로
        let t = sy
            .observe(
                t0 + ms(250),
                "새 제목",
                &[sync_frame(1, &s2), sync_frame(2, &s1)],
                Some(2),
            )
            .expect("제목은 바로");
        assert_eq!(node(&t, ROOT).label(), Some("새 제목"));
        // 500ms가 지나면 바뀐 칸 하나만 보낸다
        let late = sy
            .observe(
                t0 + ms(500),
                "새 제목",
                &[sync_frame(1, &s2), sync_frame(2, &s1)],
                Some(2),
            )
            .expect("500ms 뒤에는 글을 갱신");
        assert_eq!(node(&late, line_node(1, 0)).value(), Some("new"));
        assert!(
            late.nodes.iter().all(|(id, _)| pane_of(*id) != Some(2)),
            "바뀌지 않은 칸은 보내지 않는다"
        );
    }

    #[test]
    fn selection_change_goes_out_after_the_debounce_not_500ms() {
        let ms = Duration::from_millis;
        let t0 = Instant::now();
        let s1 = screen(&["abcdef"], 8);
        let mut s2 = screen(&["abcdef"], 8);
        select(&mut s2, 0, 1..=2);
        let mut sy = TreeSync::default();
        sy.activate();
        sy.observe(t0, "t", &[sync_frame(1, &s1)], Some(1)).unwrap();
        assert!(sy.observe(t0 + ms(40), "t", &[sync_frame(1, &s2)], Some(1)).is_none());
        let u = sy
            .observe(t0 + ms(110), "t", &[sync_frame(1, &s2)], Some(1))
            .expect("100ms 뒤 바로");
        assert!(node(&u, pane_node(1)).text_selection().is_some());
    }

    #[test]
    fn a_new_pane_is_sent_at_once_and_a_closed_pane_is_forgotten() {
        let ms = Duration::from_millis;
        let t0 = Instant::now();
        let s = screen(&["x"], 4);
        let mut sy = TreeSync::default();
        sy.activate();
        sy.observe(t0, "t", &[sync_frame(1, &s)], Some(1)).unwrap();
        let u = sy
            .observe(t0 + ms(10), "t", &[sync_frame(1, &s), sync_frame(2, &s)], Some(2))
            .expect("새 칸은 바로");
        assert!(u.nodes.iter().any(|(id, _)| *id == pane_node(2)));
        let u = sy
            .observe(t0 + ms(20), "t", &[sync_frame(2, &s)], Some(2))
            .expect("칸이 줄었다");
        assert_eq!(node(&u, ROOT).children(), &[pane_node(2)]);
        assert!(!sy.has_text(1));
    }

    #[test]
    fn activating_again_sends_a_full_tree() {
        let t0 = Instant::now();
        let s = screen(&["x"], 4);
        let mut sy = TreeSync::default();
        sy.activate();
        sy.observe(t0, "t", &[sync_frame(1, &s)], Some(1)).unwrap();
        sy.activate();
        let u = sy
            .observe(t0 + Duration::from_millis(1), "t", &[sync_frame(1, &s)], Some(1))
            .unwrap();
        assert!(
            u.nodes.iter().any(|(id, _)| *id == pane_node(1)),
            "다시 켜지면 칸도 다시 보낸다"
        );
        sy.deactivate();
        assert!(
            sy.observe(t0 + Duration::from_secs(9), "t", &[sync_frame(1, &s)], Some(1))
                .is_none()
        );
    }

    struct NoChanges;
    impl accesskit_consumer::TreeChangeHandler for NoChanges {
        fn node_added(&mut self, _: &accesskit_consumer::NodeRef) {}
        fn node_updated(&mut self, _: &accesskit_consumer::NodeRef, _: &accesskit_consumer::NodeRef) {}
        fn focus_moved(&mut self, _: Option<&accesskit_consumer::NodeRef>, _: Option<&accesskit_consumer::NodeRef>) {}
        fn node_removed(&mut self, _: &accesskit_consumer::NodeRef) {}
    }

    /// 어댑터가 쓰는 소비자(accesskit_consumer)가 우리 트리를 그대로 받아 터미널 글·선택·낱말을 읽는다
    #[test]
    fn the_consumer_reads_the_terminal_text_and_selection() {
        let mut s = screen(&["가a 😀b", "", "e\u{301}x  y"], 12);
        select(&mut s, 0, 2..=2);
        select(&mut s, 2, 0..=0);
        let t = pane_text(&s);
        let g = Geom {
            x: 0.0,
            y: 0.0,
            cell_w: 8.0,
            cell_h: 16.0,
        };
        let p = PaneIn {
            id: 4,
            name: "셸",
            text: &t,
            geom: Some(g),
            send: true,
        };
        let u = build_update("창", &[p], Some(4));
        let tree = accesskit_consumer::Tree::new(u, true);
        let st = tree.state();
        assert_eq!(st.focus().map(|n| n.locate().0), Some(pane_node(4)));
        let term = st
            .node_by_tree_local_id(pane_node(4), TreeId::ROOT)
            .expect("터미널 노드");
        assert!(term.supports_text_ranges());
        assert_eq!(term.document_range().text(), "가a 😀b\n\ne\u{301}x  y");
        // 둘째 줄 첫 글자(가)에서 시작해 셋째 줄 첫 글자(e + 결합 문자)까지가 고른 범위다
        let sel = term.text_selection().expect("선택");
        assert_eq!(sel.text(), "a 😀b\n\ne\u{301}");
    }

    #[test]
    fn the_consumer_accepts_partial_updates_that_add_and_drop_panes() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let s1 = screen(&["one", "two"], 8);
        let mut s2 = screen(&["one", "TWO"], 8);
        s2.cells[0].ch = 'O';
        let mut sy = TreeSync::default();
        sy.activate();
        let first = sy.observe(t0, "t", &[sync_frame(1, &s1)], Some(1)).unwrap();
        let mut tree = accesskit_consumer::Tree::new(first, true);
        // 칸 하나 늘고 글이 바뀐 갱신, 칸 하나가 사라진 갱신, 포커스만 바뀐 갱신을 차례로 받는다
        let steps = [
            (t0 + ms(600), vec![sync_frame(1, &s2), sync_frame(2, &s1)], 2),
            (t0 + ms(700), vec![sync_frame(2, &s1)], 2),
            (t0 + ms(800), vec![sync_frame(2, &s1), sync_frame(3, &s1)], 3),
        ];
        for (at, frames, focus) in steps {
            let u = sy.observe(at, "t", &frames, Some(focus)).expect("갱신이 있다");
            tree.update_and_process_changes(u, &mut NoChanges);
            let st = tree.state();
            let ids: Vec<u64> = frames.iter().map(|f| f.id).collect();
            for id in &ids {
                assert!(st.node_by_tree_local_id(pane_node(*id), TreeId::ROOT).is_some());
            }
            assert!(st.node_by_tree_local_id(pane_node(1), TreeId::ROOT).is_some() == ids.contains(&1));
        }
    }
}
