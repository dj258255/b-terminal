//! 화면 찾기(#39, #823, #824). Ghostty 1.4.0의 `search/`처럼 줄 글을 글자 그대로 찾는다.
//!
//! 찾는 글(`sliding_window.zig`):
//! - ASCII만 늘 대소문자를 무시하고(`std.ascii.findIgnoreCase`) 비ASCII는 가린다. 대문자를 쳐도 무시한다.
//! - 줄 글은 `PageFormatter`(plain, unwrap)처럼 만든다: 줄 끝 빈칸은 깎고, 하드 줄바꿈은 `\n`, 접힌 줄은 이어 붙이고,
//!   넓은 글자의 자리표시 칸은 건너뛰고, 칸에 덧붙은 코드포인트(결합 문자·VS16·ZWJ)도 글에 넣는다.
//! - 맞은 곳마다 시작 다음 바이트부터 다시 찾아 겹치는 맞은 곳도 센다.
//!
//! 고른 곳(`screen.zig`·`terminal.zig`):
//! - 바늘을 바꾸면 아무것도 고르지 않고 화면도 옮기지 않는다. 다음·이전을 처음 누르면 가장 아래·위 것을 고른다.
//! - 고른 곳은 화면 좌표가 아니라 줄 번호(`Grid::scrolled_total` + 줄)로 들고 있어 출력이 와서 화면이 밀려도
//!   같은 내용을 따라간다. 화면 안(active) 내용이 바뀌어 그 자리에 맞은 곳이 없으면 가장 아래 것으로 옮기고,
//!   스크롤백은 바뀌지 않는다고 본다. 크기가 바뀌면 고른 것을 푼다.
//! - 고를 때 맞은 곳이 보이는 화면과 겹치면 그대로 두고, 아니면 맞은 곳 시작이 맨 윗줄에 오게 옮긴다.
//!
//! 새로 고침(#833): 막대가 열린 동안 출력이 와 화면이 바뀌면(`refresh`, 지문이 다를 때만) 개수를 다시 센다.
//!
//! 성능 보호로 두는 상한은 Ghostty에 없다: 세는 상한 `MAX_COUNT`, 한 화면에서 칠하는 상한 `MAX_VISIBLE`.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::cell::{Cell, Flags};
use alacritty_terminal::term::{Term, TermMode};
use std::ops::RangeInclusive;

/// 맞은 곳: 시작과 끝 칸(끝 칸 포함)
pub type Match = RangeInclusive<Point>;

/// 한 화면에서 강조하는 맞은 곳의 상한
const MAX_VISIBLE: usize = 2000;
/// 개수를 세는 상한(넘으면 "1000+"로 보인다)
pub const MAX_COUNT: usize = 1000;

/// 찾기 결과 요약: 지금 맞은 곳이 몇 번째(위에서부터), 모두 몇 개
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchStatus {
    pub index: Option<usize>,
    pub total: usize,
    /// 개수가 상한에 닿아 더 셀 수 있다
    pub capped: bool,
}

/// 줄 번호로 든 칸. `id`는 `scrolled_total + Line`이라 줄이 스크롤백으로 밀려도 같다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Anchor {
    id: i64,
    col: usize,
}

/// 고른 맞은 곳(내용을 따라간다)
#[derive(Debug, Clone, PartialEq, Eq)]
struct Selected {
    start: Anchor,
    end: Anchor,
    columns: usize,
    lines: usize,
    alt: bool,
}

/// 칸 하나의 찾기 상태
pub(crate) struct Search {
    /// 찾는 글(ASCII 대문자를 소문자로 접은 UTF-8 바이트)
    needle: Vec<u8>,
    /// 찾는 글 안의 `\n` 수(앞뒤로 더 읽을 하드 줄 수)
    newlines: usize,
    selected: Option<Selected>,
    /// 마지막으로 센 때의 화면 지문. 같으면 다시 세지 않는다(#833)
    seen: Option<Fingerprint>,
    /// 스크롤백 전체를 센 횟수(시험용)
    recounts: usize,
}

/// 화면이 바뀌었는지 보는 지문(Ghostty는 화면의 바뀜 표시를 본다). 스크롤백은 쓰는 쪽으로만 바뀌므로
/// 밀려 올라간 줄 수와 스크롤백 크기, 활성 화면 칸의 해시로 갈음한다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    scrolled: u64,
    history: usize,
    columns: usize,
    lines: usize,
    alt: bool,
    active: u64,
}

impl Fingerprint {
    fn of<T>(term: &Term<T>) -> Self {
        // FNV-1a. 활성 화면(수천 칸)만 훑는다
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |v: u64| {
            h = (h ^ v).wrapping_mul(0x0000_0100_0000_01b3);
        };
        let grid = term.grid();
        for line in 0..term.screen_lines() as i32 {
            let row = &grid[Line(line)];
            for col in 0..term.columns() {
                let c = &row[Column(col)];
                eat(c.c as u64);
                if let Some(z) = c.zerowidth() {
                    for &x in z {
                        eat(x as u64 | 1 << 40);
                    }
                }
                if c.flags.intersects(Flags::WIDE_CHAR | Flags::WRAPLINE) {
                    eat(c.flags.bits() as u64 | 1 << 41);
                }
            }
        }
        Self {
            scrolled: grid.scrolled_total(),
            history: grid.history_size(),
            columns: term.columns(),
            lines: term.screen_lines(),
            alt: term.mode().contains(TermMode::ALT_SCREEN),
            active: h,
        }
    }
}

/// 줄 글(소문자로 접은 바이트)과 바이트마다 그 글자가 있는 칸(`첫 줄부터의 줄 * 열 수 + 열`)
struct Text {
    bytes: Vec<u8>,
    cells: Vec<u32>,
    first: i32,
    cols: usize,
}

fn is_blank(c: &Cell) -> bool {
    (c.c == ' ' || c.c == '\0') && c.zerowidth().is_none_or(<[char]>::is_empty)
}

fn wrapped<T>(term: &Term<T>, line: i32) -> bool {
    let last = term.last_column();
    term.grid()[Line(line)][last].flags.contains(Flags::WRAPLINE)
}

impl Text {
    /// `first..=last`줄의 글. Ghostty `PageFormatter`(plain, unwrap)와 같은 규칙으로 만든다
    fn build<T>(term: &Term<T>, first: i32, last: i32) -> Self {
        let cols = term.columns();
        let rows = (last - first + 1).max(0) as usize;
        let mut t = Text {
            bytes: Vec::with_capacity(rows * cols / 2),
            cells: Vec::with_capacity(rows * cols / 2),
            first,
            cols,
        };
        let grid = term.grid();
        // 줄 끝에서 바로 내놓지 않는 것: 지연된 줄바꿈 수와 아직 글자가 안 나온 빈칸(접힌 줄 너머로 이어진다)
        let mut pending_newlines = 0usize;
        let mut blanks: Vec<u32> = Vec::new();
        let mut last_cell = 0u32;
        let mut buf = [0u8; 4];
        for line in first..=last {
            let row = &grid[Line(line)];
            let base = ((line - first) as usize * cols) as u32;
            // 글자가 하나도 없는 줄은 줄바꿈 하나로 센다(접힌 줄이어도)
            if row[Column(0)..].iter().all(is_blank) {
                pending_newlines += 1;
                continue;
            }
            for _ in 0..pending_newlines {
                t.bytes.push(b'\n');
                t.cells.push(last_cell);
            }
            pending_newlines = 0;
            if !wrapped(term, line) {
                pending_newlines = 1;
            }
            let continues = line > first && wrapped(term, line - 1);
            if !continues {
                blanks.clear();
            }
            for col in 0..cols {
                let cell = &row[Column(col)];
                if cell
                    .flags
                    .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
                {
                    continue;
                }
                let idx = base + col as u32;
                if is_blank(cell) {
                    blanks.push(idx);
                    continue;
                }
                for b in blanks.drain(..) {
                    t.bytes.push(b' ');
                    t.cells.push(b);
                }
                let mut put = |c: char| {
                    for &b in c.encode_utf8(&mut buf).as_bytes() {
                        t.bytes.push(b.to_ascii_lowercase());
                        t.cells.push(idx);
                    }
                };
                put(cell.c);
                if let Some(extra) = cell.zerowidth() {
                    for &c in extra {
                        put(c);
                    }
                }
                last_cell = idx;
            }
        }
        // 마지막 줄이 접히지 않았으면 끝에 줄바꿈 하나
        if last >= first && !wrapped(term, last) {
            t.bytes.push(b'\n');
            t.cells.push(last_cell);
        }
        t
    }

    fn point(&self, idx: u32) -> (i32, usize) {
        (self.first + (idx as usize / self.cols) as i32, idx as usize % self.cols)
    }

    /// 겹치는 것까지 모든 맞은 곳(위에서 아래로)
    fn find_all<T>(&self, term: &Term<T>, needle: &[u8], mut keep: impl FnMut(&Match) -> bool) -> Vec<Match> {
        let mut out = Vec::new();
        let n = needle.len();
        if n == 0 || self.bytes.len() < n {
            return out;
        }
        let first = needle[0];
        let last_start = self.bytes.len() - n;
        let mut i = 0;
        while i <= last_start {
            match self.bytes[i..=last_start].iter().position(|&b| b == first) {
                None => break,
                Some(off) => i += off,
            }
            if &self.bytes[i..i + n] == needle {
                let (sl, sc) = self.point(self.cells[i]);
                let (el, mut ec) = self.point(self.cells[i + n - 1]);
                // 넓은 글자가 끝이면 뒤 칸(자리표시)까지 칠한다
                if ec + 1 < self.cols && term.grid()[Line(el)][Column(ec)].flags.contains(Flags::WIDE_CHAR) {
                    ec += 1;
                }
                let m = Point::new(Line(sl), Column(sc))..=Point::new(Line(el), Column(ec));
                if keep(&m) {
                    out.push(m);
                }
            }
            // 시작 다음부터 다시 찾는다(겹침을 센다)
            i += 1;
        }
        out
    }
}

/// 이어진 줄의 첫 줄까지 위로 넓히고 `extra`개의 하드 줄을 더 넓힌다
fn widen_up<T>(term: &Term<T>, mut line: i32, extra: usize) -> i32 {
    let top = term.topmost_line().0;
    while line > top && wrapped(term, line - 1) {
        line -= 1;
    }
    for _ in 0..extra {
        if line <= top {
            break;
        }
        line -= 1;
        while line > top && wrapped(term, line - 1) {
            line -= 1;
        }
    }
    line
}

fn widen_down<T>(term: &Term<T>, mut line: i32, extra: usize) -> i32 {
    let bottom = term.bottommost_line().0;
    while line < bottom && wrapped(term, line) {
        line += 1;
    }
    for _ in 0..extra {
        if line >= bottom {
            break;
        }
        line += 1;
        while line < bottom && wrapped(term, line) {
            line += 1;
        }
    }
    line
}

impl Search {
    /// 빈 글이면 없다
    pub fn new(query: &str) -> Option<Self> {
        if query.is_empty() {
            return None;
        }
        let needle: Vec<u8> = query.bytes().map(|b| b.to_ascii_lowercase()).collect();
        let newlines = needle.iter().filter(|&&b| b == b'\n').count();
        Some(Self {
            needle,
            newlines,
            selected: None,
            seen: None,
            recounts: 0,
        })
    }

    /// `lo..=hi`줄과 겹치는 맞은 곳(위에서 아래로). 앞뒤로 이어진 줄과 바늘의 줄바꿈 수만큼 더 읽는다
    fn find_in<T>(&self, term: &Term<T>, lo: i32, hi: i32) -> Vec<Match> {
        let first = widen_up(term, lo.max(term.topmost_line().0), self.newlines);
        let last = widen_down(term, hi.min(term.bottommost_line().0), self.newlines);
        let text = Text::build(term, first, last);
        text.find_all(term, &self.needle, |m| m.start().line.0 <= hi && m.end().line.0 >= lo)
    }

    fn all<T>(&self, term: &Term<T>) -> Vec<Match> {
        self.find_in(term, term.topmost_line().0, term.bottommost_line().0)
    }

    fn anchor<T>(term: &Term<T>, p: Point) -> Anchor {
        Anchor {
            id: term.grid().scrolled_total() as i64 + p.line.0 as i64,
            col: p.column.0,
        }
    }

    fn to_match<T>(term: &Term<T>, s: &Selected) -> Match {
        let total = term.grid().scrolled_total() as i64;
        Point::new(Line((s.start.id - total) as i32), Column(s.start.col))
            ..=Point::new(Line((s.end.id - total) as i32), Column(s.end.col))
    }

    fn pick<T>(&mut self, term: &Term<T>, m: &Match) {
        self.selected = Some(Selected {
            start: Self::anchor(term, *m.start()),
            end: Self::anchor(term, *m.end()),
            columns: term.columns(),
            lines: term.screen_lines(),
            alt: term.mode().contains(TermMode::ALT_SCREEN),
        });
    }

    /// 출력으로 화면이 바뀐 것을 고른 곳에 맞춘다(Ghostty `reloadActive`)
    fn reconcile<T>(&mut self, term: &Term<T>) {
        let Some(sel) = self.selected.clone() else { return };
        if sel.columns != term.columns()
            || sel.lines != term.screen_lines()
            || sel.alt != term.mode().contains(TermMode::ALT_SCREEN)
        {
            self.selected = None;
            return;
        }
        let total = term.grid().scrolled_total() as i64;
        let (sl, el) = (sel.start.id - total, sel.end.id - total);
        if sl < term.topmost_line().0 as i64 || el > term.bottommost_line().0 as i64 {
            // 스크롤백에서 밀려났다: 가장 오래된 맞은 곳으로
            self.selected = None;
            if let Some(m) = self.all(term).first() {
                self.pick(term, m);
            }
            return;
        }
        // 스크롤백은 바뀌지 않는다. 화면 안에 걸친 곳만 아직 그 글인지 본다
        if el >= 0 {
            let m = Self::to_match(term, &sel);
            let (lo, hi) = (m.start().line.0, m.end().line.0);
            if !self.find_in(term, lo, hi).contains(&m) {
                // 화면 안 내용이 바뀌어 사라졌다: 가장 아래 맞은 곳으로
                self.selected = None;
                if let Some(m) = self.all(term).last() {
                    self.pick(term, m);
                }
            }
        }
    }

    /// 지금 고른 맞은 곳(출력에 맞춘 뒤)
    pub fn current<T>(&mut self, term: &Term<T>) -> Option<Match> {
        self.reconcile(term);
        self.selected.as_ref().map(|s| Self::to_match(term, s))
    }

    /// 다음 맞은 곳으로. `older`면 위(오래된 쪽)로, 아니면 아래로. 고른 것이 없으면 위로는 가장 아래 것, 아래로는 가장 위 것.
    /// 끝에 닿으면 반대 끝으로 돈다. 고른 맞은 곳이 보이는 화면과 겹치지 않으면 맞은 곳 시작이 맨 윗줄에 오게 옮긴다
    pub fn step<T: EventListener>(&mut self, term: &mut Term<T>, older: bool) -> bool {
        let cur = self.current(term);
        let all = self.all(term);
        if all.is_empty() {
            return false;
        }
        let pos = cur.and_then(|c| all.iter().position(|m| *m == c));
        let len = all.len();
        let next = match (pos, older) {
            (None, true) => len - 1,
            (None, false) => 0,
            (Some(p), true) => p.checked_sub(1).unwrap_or(len - 1),
            (Some(p), false) => (p + 1) % len,
        };
        let m = all[next].clone();
        self.pick(term, &m);
        // 보이는 화면과 겹치면 그대로, 아니면 시작이 맨 윗줄에
        let offset = term.grid().display_offset() as i32;
        let (top, bottom) = (-offset, term.screen_lines() as i32 - 1 - offset);
        if m.end().line.0 < top || m.start().line.0 > bottom {
            let want = (-m.start().line.0).max(0);
            term.scroll_display(Scroll::Delta(want - offset));
        }
        true
    }

    /// 복사 모드의 `/ ? n N`(#932): `from` 칸을 기준으로 앞(`forward`)이나 뒤의 가장 가까운 맞은 곳을 골라 돌려준다.
    /// 기준 칸에서 시작하는 맞은 곳은 건너뛴다. 끝에 닿으면 반대 끝으로 돈다. 화면은 옮기지 않는다(복사 모드가 커서를 따라 옮긴다)
    pub fn nearest<T>(&mut self, term: &Term<T>, from: Point, forward: bool) -> Option<Match> {
        let all = self.all(term);
        let m = if forward {
            all.iter().find(|m| *m.start() > from).or_else(|| all.first())
        } else {
            all.iter().rev().find(|m| *m.start() < from).or_else(|| all.last())
        }?
        .clone();
        self.pick(term, &m);
        Some(m)
    }

    /// 지금 보이는 줄들의 맞은 곳(위에서 아래로)
    /// `edge`이면 픽셀 스크롤(#902)로 화면 맨 아래에 걸쳐 보이는 다음 줄까지 찾는다
    pub fn visible_with_edge<T>(&self, term: &Term<T>, edge: bool) -> Vec<Match> {
        let offset = term.grid().display_offset() as i32;
        let last = term.screen_lines() as i32 - 1 - offset + i32::from(edge && offset > 0);
        let mut v = self.find_in(term, -offset, last);
        v.truncate(MAX_VISIBLE);
        v
    }

    /// 화면이 지난번 센 뒤로 바뀌었으면 다시 세어 돌려준다. 안 바뀌었으면 찾지 않고 없다(Ghostty `reloadActive`).
    /// 고른 곳은 `status`가 출력에 맞춘다
    pub fn refresh<T>(&mut self, term: &Term<T>) -> Option<SearchStatus> {
        if self.seen == Some(Fingerprint::of(term)) {
            return None;
        }
        Some(self.status(term))
    }

    /// 스크롤백 전체를 센 횟수(시험용)
    pub fn recounts(&self) -> usize {
        self.recounts
    }

    /// 스크롤백 전체에서 센다(상한까지)
    pub fn status<T>(&mut self, term: &Term<T>) -> SearchStatus {
        self.seen = Some(Fingerprint::of(term));
        self.recounts += 1;
        let cur = self.current(term);
        let all = self.all(term);
        let capped = all.len() >= MAX_COUNT;
        let index = cur
            .and_then(|c| all.iter().position(|m| *m == c))
            .map(|p| p + 1)
            .filter(|&i| i <= MAX_COUNT);
        SearchStatus {
            index,
            total: all.len().min(MAX_COUNT),
            capped,
        }
    }
}

/// 위에서 아래로 놓인 맞은 곳들에서 칸이 든 것을 찾는다. 칸도 위에서 아래로 묻는다고 보고 `cursor`를 앞으로만 옮긴다
pub(crate) fn hit(matches: &[Match], cursor: &mut usize, point: Point) -> bool {
    while *cursor < matches.len() && *matches[*cursor].end() < point {
        *cursor += 1;
    }
    *cursor < matches.len() && *matches[*cursor].start() <= point
}

#[cfg(test)]
mod tests {
    use crate::screen::CellFlags;
    use crate::session::{Emulator, GridSize};

    /// 화면을 넘겨 스크롤백에 들어간 줄도 찾고, 위·아래로 돌며, 그 자리로 화면을 옮긴다
    #[test]
    fn finds_lines_in_scrollback_and_cycles() {
        let mut e = Emulator::new(GridSize::new(30, 5), bt_core::Theme::ghostty_default());
        let mut text = String::new();
        for i in 0..40 {
            if i == 3 || i == 20 {
                text.push_str(&format!("line {i} error: 실패\r\n"));
            } else {
                text.push_str(&format!("line {i} ok\r\n"));
            }
        }
        e.feed(text.as_bytes());
        assert_eq!(
            e.search("ERROR").unwrap().total,
            2,
            "ASCII는 대문자를 쳐도 무시한다(Ghostty)"
        );
        let st = e.search("error").expect("찾음");
        assert_eq!((st.index, st.total), (None, 2), "바늘을 바꾸면 고르지 않는다");
        assert_eq!(
            e.search_step(true).unwrap().index,
            Some(2),
            "처음 다음은 맨 아래에서: 20번 줄"
        );
        let s = e.snapshot();
        assert!(
            s.text().contains("line 20 error"),
            "그 줄이 보이게 옮긴다:\n{}",
            s.text()
        );
        // 위로 한 번 더 → 3번 줄, 또 한 번 → 돌아서 20번 줄
        assert_eq!(e.search_step(true).unwrap().index, Some(1));
        assert!(e.snapshot().text().contains("line 3 error"));
        assert_eq!(e.search_step(true).unwrap().index, Some(2));
        // 아래로 → 돌아서 3번 줄
        assert_eq!(e.search_step(false).unwrap().index, Some(1));
    }

    #[test]
    fn snapshot_marks_matches_and_the_current_one() {
        let mut e = Emulator::new(GridSize::new(20, 3), bt_core::Theme::ghostty_default());
        e.feed("ab 한글 ab\r\nzz ab".as_bytes());
        e.search("ab").unwrap();
        e.search_step(true).unwrap();
        let s = e.snapshot();
        let flags = |c: usize, r: usize| s.cell(c, r).flags;
        // 맨 아래 것(1줄 3칸)이 지금 것
        assert!(flags(3, 1).contains(CellFlags::MATCH_CURRENT));
        assert!(flags(4, 1).contains(CellFlags::MATCH_CURRENT));
        assert!(flags(0, 0).contains(CellFlags::MATCH) && !flags(0, 0).contains(CellFlags::MATCH_CURRENT));
        // "한글"(두 칸 글자) 뒤의 ab: 3+4=7번 칸부터
        assert!(flags(8, 0).contains(CellFlags::MATCH));
        assert!(!flags(2, 0).contains(CellFlags::MATCH), "공백");
        e.search_clear();
        assert!(
            !e.snapshot().cell(0, 0).flags.contains(CellFlags::MATCH),
            "찾기를 끝내면 지운다"
        );
    }

    /// 보안 검토: 기억해 둔 맞은 곳이 스크롤백 지우기·창 줄이기 뒤에 격자 밖을 가리켜도 패닉하지 않는다
    #[test]
    fn stale_match_after_clear_or_resize_does_not_panic() {
        let mut e = Emulator::new(GridSize::new(40, 5), bt_core::Theme::ghostty_default());
        let mut text = String::new();
        for i in 0..50 {
            text.push_str(&format!("row {i:02} {} target x\r\n", ".".repeat(24)));
        }
        e.feed(text.as_bytes());
        e.search("target").unwrap();
        for _ in 0..31 {
            e.search_step(true);
        }
        // 맞은 곳은 31번 칸쯤이다. 스크롤백과 화면을 지우고 창을 10칸으로 줄인다
        e.feed(b"\x1b[2J\x1b[3J\x1b[H");
        e.resize(GridSize::new(10, 3));
        e.feed(b"target");
        let st = e.search_step(true).unwrap();
        assert_eq!(st.total, 1);
        let st = e.search_step(false).unwrap();
        assert_eq!(st.index, Some(1));
    }

    #[test]
    fn no_match_and_empty_query() {
        let mut e = Emulator::new(GridSize::new(20, 3), bt_core::Theme::ghostty_default());
        e.feed(b"hello");
        let st = e.search("nope").unwrap();
        assert_eq!((st.index, st.total), (None, 0));
        assert!(e.search("").is_none(), "빈 글은 찾기를 끈다");
    }
}
