//! 마우스 아래 한 칸의 링크를 찾을 때 화면 전체를 복사하지 않고 필요한 줄만 읽는다(#924).
//!
//! 옛 길은 `snapshot()`으로 화면 전체(칸마다 색·결합 문자 벡터까지)를 복사한 뒤 `links::link_at_with`로 한 칸만 찾았다.
//! 여기서는 엔진 격자를 직접 읽어
//! - 글에서 찾는 URL: 마우스 칸이 든 접힌 줄 묶음(논리 줄)의 줄만 작은 `Screen`으로 옮겨 **같은 `link_at_with`**에 넣는다.
//!   주소를 거르고 고르는 규칙은 `links.rs` 한 곳에 그대로 있고 여기서 다시 쓰지 않는다. 입력으로 받는 칸 범위만 줄어든다.
//! - OSC 8 링크: 마우스 칸의 링크 주소를 보고, 있으면 같은 주소의 다른 칸을 찾으러 격자의 링크 주소만 훑는다(칸을 복사하지 않는다).
//!   같은 주소는 한 링크(옛 길의 `link_ids`와 같다). 링크 수·주소 길이 상한(`MAX_LINKS`·`MAX_LINK_URI`)도 옛 길과 같게 센다.

use std::collections::HashSet;

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::cell::{Cell as VtCell, Flags};
use bt_core::Rgb;

use crate::links::{LinkHit, LinkOptions, LinkSpan, link_at_with};
use crate::screen::{Cell, CellFlags, Cursor, CursorShape, Modes, Screen};
use crate::session::{MAX_LINK_URI, MAX_LINKS};

/// `probe`의 결과와 읽은 양
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkProbe {
    pub hit: Option<LinkHit>,
    /// 글자와 폭 표시까지 복사해 읽은 칸 수
    pub cells_copied: usize,
    /// 복사하지 않고 표시(접힘·폭)나 링크 주소만 들여다본 칸 수
    pub cells_peeked: usize,
}

fn is_spacer(c: &VtCell) -> bool {
    c.flags
        .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
}

/// (col, row) 칸의 링크. `links::link_at_with(&snapshot, col, row, opts)`와 같은 결과를 돌려준다
pub(crate) fn probe<T: EventListener>(term: &Term<T>, col: usize, row: usize, opts: LinkOptions) -> LinkProbe {
    let cols = term.columns();
    let rows = term.screen_lines();
    let mut out = LinkProbe {
        hit: None,
        cells_copied: 0,
        cells_peeked: 0,
    };
    if col >= cols || row >= rows {
        return out;
    }
    let grid = term.grid();
    let offset = grid.display_offset() as i32;
    let line = |r: usize| Line(r as i32 - offset);
    let at = |c: usize, r: usize| &grid[line(r)][Column(c)];

    // 넓은 글자의 뒤 칸이면 앞 칸
    out.cells_peeked += 1;
    let hcol = if is_spacer(at(col, row)) && col > 0 {
        col - 1
    } else {
        col
    };

    // OSC 8 링크가 먼저
    if opts.osc8 {
        out.cells_peeked += 1;
        let uri = at(hcol, row)
            .hyperlink()
            .map(|h| h.uri().to_string())
            .filter(|u| u.len() <= MAX_LINK_URI);
        if let Some(uri) = uri {
            let (spans, scanned, interned) = osc8_spans(term, &uri);
            out.cells_peeked += scanned;
            if interned {
                out.hit = Some(LinkHit {
                    url: uri,
                    spans,
                    explicit: true,
                });
                return out;
            }
            // 링크 수 상한을 넘어 옛 길에서 링크가 아니게 된 칸: 글 URL로 넘어간다
        }
    }
    if !opts.url {
        return out;
    }

    // 접힌 줄 묶음의 처음과 끝
    let wraps = |r: usize| at(cols - 1, r).flags.contains(Flags::WRAPLINE);
    let mut top = row;
    while top > 0 && wraps(top - 1) {
        out.cells_peeked += 1;
        top -= 1;
    }
    let mut bottom = row;
    while bottom + 1 < rows && wraps(bottom) {
        out.cells_peeked += 1;
        bottom += 1;
    }

    // 그 줄들만 옮긴 작은 화면. 마우스 칸은 원래 칸(`col`) 그대로 준다: 뒤 칸 보정은 `link_at_with`가 한다
    let n = bottom - top + 1;
    let blank = Cell::blank(Rgb::new(0, 0, 0), Rgb::new(0, 0, 0));
    let mut cells = vec![blank; cols * n];
    for r in top..=bottom {
        for c in 0..cols {
            let src = at(c, r);
            let dst = &mut cells[(r - top) * cols + c];
            dst.ch = src.c;
            if is_spacer(src) {
                dst.flags.insert(CellFlags::SPACER);
            }
            if src.flags.contains(Flags::WIDE_CHAR) {
                dst.flags.insert(CellFlags::WIDE);
            }
            if src.flags.contains(Flags::WRAPLINE) {
                dst.flags.insert(CellFlags::WRAPLINE);
            }
        }
    }
    out.cells_copied = cols * n;
    let window = Screen {
        cols,
        rows: n,
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
        history: 0,
        pixel_shift: 0,
        edge: Vec::new(),
        background: Rgb::new(0, 0, 0),
        foreground: Rgb::new(0, 0, 0),
        cursor_color: None,
        chrome_background: Rgb::new(0, 0, 0),
        links: Vec::new(),
        images: Vec::new(),
    };
    // OSC 8는 위에서 끝냈다(창 화면에는 링크 번호가 없다)
    let url_only = LinkOptions { url: true, osc8: false };
    out.hit = link_at_with(&window, col, row - top, url_only).map(|mut h| {
        for s in &mut h.spans {
            s.row += top;
        }
        h
    });
    out
}

/// `uri`와 같은 주소를 가진 칸의 줄별 구간, 훑은 칸 수, 옛 길에서 링크로 쳤을 주소인지.
/// 옛 길은 화면을 위에서 아래·왼쪽에서 오른쪽으로 훑으며 처음 보는 주소에 번호를 주고 `MAX_LINKS`개가 차면 새 주소를 링크로 치지 않는다.
/// 한 주소의 번호는 처음 본 때 정해지므로 처음 보기 전까지 본 서로 다른 주소 수로 같은 판정을 한다
fn osc8_spans<T: EventListener>(term: &Term<T>, uri: &str) -> (Vec<LinkSpan>, usize, bool) {
    let cols = term.columns();
    let rows = term.screen_lines();
    let grid = term.grid();
    let offset = grid.display_offset() as i32;
    let mut seen: HashSet<String> = HashSet::new();
    let mut interned: Option<bool> = None;
    let mut spans = Vec::new();
    let mut scanned = 0;
    for r in 0..rows {
        let vt_row = &grid[Line(r as i32 - offset)];
        let mut run: Option<usize> = None;
        for c in 0..cols {
            scanned += 1;
            let mut same = false;
            if let Some(h) = vt_row[Column(c)].hyperlink() {
                let u = h.uri();
                if u.len() <= MAX_LINK_URI {
                    if u == uri {
                        same = true;
                        if interned.is_none() {
                            interned = Some(seen.len() < MAX_LINKS);
                        }
                    } else if interned.is_none() && seen.len() < MAX_LINKS {
                        seen.insert(u.to_string());
                    }
                }
            }
            if interned == Some(false) {
                same = false;
            }
            match (same, run) {
                (true, None) => run = Some(c),
                (false, Some(start)) => {
                    spans.push(LinkSpan { row: r, start, end: c });
                    run = None;
                }
                _ => {}
            }
        }
        if let Some(start) = run {
            spans.push(LinkSpan {
                row: r,
                start,
                end: cols,
            });
        }
    }
    (spans, scanned, interned == Some(true))
}
