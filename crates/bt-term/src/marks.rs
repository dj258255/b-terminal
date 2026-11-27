//! kitty 마커(#929, ADR-099): `toggle_marker`·`remove_marker`·`scroll_to_mark`.
//!
//! 원문은 kitty v0.51.1의 `docs/marks.rst`, `kitty/options/utils.py`(`parse_marker_spec` 477~500줄, `toggle_marker` 503~510줄,
//! `scroll_to_mark` 513~526줄), `kitty/marks.py`(`marker_from_multiple_regex` 29~44줄), `kitty/window.py`(`toggle_marker` 3052~3060줄,
//! `remove_marker` 3074~3078줄, `scroll_to_mark` 3080~3082줄), `kitty/screen.c`(`scroll_to_next_mark` 7256~7286줄), `kitty/line.c`(`line_has_mark`, `apply_marker`).
//!
//! - 종류는 `text`·`itext`·`regex`·`iregex`. `function`(파이썬 함수 마커)은 받지 않는다.
//! - 명세는 `<색 번호 1~3> <글> ...` 짝의 반복이고, 색 번호는 1~3으로 맞춘다. 짝이 여럿이면 한 정규식으로 묶어 같은 자리에서는 앞 짝이 이긴다.
//! - 마커는 칸(탭 안의 한 칸)마다 하나다. 같은 명세를 다시 걸면 끄고, 다른 명세는 바꾼다.
//! - 한 줄(화면의 줄 하나, 접힌 줄도 줄마다) 안에서만 맞춘다.
//! - 마크는 격자를 바꾸지 않는다. 스냅샷을 만든 뒤 칸 표시(`CellFlags::MARK1~3`)만 얹고, 그릴 때 색을 건다.
//!   보이는 줄(과 픽셀 스크롤로 걸친 줄)만 검사한다. `scroll_to_mark`는 스크롤백을 훑되 `SCROLL_SCAN_LINES`줄까지만 본다.
//!
//! 우리 정규식은 `regex` 크레이트 문법이다(kitty는 Python `re`). 역참조·앞뒤 보기는 없고, 맞추는 시간은 글 길이에 비례한다.

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::{Term, TermMode};
use regex::Regex;

use crate::screen::{Cell, CellFlags, Screen};

/// `scroll_to_mark`가 스크롤백에서 훑는 줄 수 상한(kitty는 상한이 없다)
pub const SCROLL_SCAN_LINES: usize = 50_000;

/// 마커 명세(종류는 정규식으로 푼 뒤의 모양). 정규식은 아직 컴파일하지 않았다
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkerSpec {
    ignore_case: bool,
    groups: Vec<(u8, String)>,
}

impl MarkerSpec {
    /// `toggle_marker`의 인자 `<종류> <색 번호> <글> [<색 번호> <글>...]`을 읽는다(kitty `toggle_marker`·`parse_marker_spec`).
    /// 종류와 나머지는 첫 공백에서 나누고, 나머지는 `shlex_split`으로 낱말로 나눈다
    pub fn parse(arg: &str) -> Result<Self, String> {
        let arg = arg.trim();
        let (kind, rest) = arg
            .split_once(char::is_whitespace)
            .ok_or_else(|| format!("{arg} 은(는) 마커 명세가 아닙니다(종류와 명세가 필요합니다)"))?;
        let parts = shlex_split(rest)?;
        let (ignore_case, escape) = match kind {
            "text" => (false, true),
            "itext" => (true, true),
            "regex" => (false, false),
            "iregex" => (true, false),
            "function" => return Err("함수 마커(function)는 지원하지 않습니다".into()),
            other => return Err(format!("모르는 마커 종류: {other}(text·itext·regex·iregex)")),
        };
        if parts.is_empty() || parts.len() % 2 != 0 {
            return Err(format!("마커 색 번호와 글·정규식이 짝으로 오지 않았습니다: {rest}"));
        }
        if parts.len() / 2 > bt_core::user_regex::MAX_PATTERNS {
            return Err(format!(
                "마커 짝은 {}개까지 받습니다",
                bt_core::user_regex::MAX_PATTERNS
            ));
        }
        let mut groups = Vec::new();
        for pair in parts.chunks(2) {
            let n: i64 = pair[0]
                .parse()
                .map_err(|_| format!("마커 색 번호가 정수가 아닙니다: {}", pair[0]))?;
            let color = n.clamp(1, 3) as u8;
            let text = if escape {
                regex::escape(&pair[1])
            } else {
                pair[1].clone()
            };
            groups.push((color, text));
        }
        Ok(MarkerSpec { ignore_case, groups })
    }

    /// (색 번호 1~3, 정규식 글) 짝들. `text`·`itext`는 정규식 글자를 벗긴 꼴이다
    pub fn groups(&self) -> &[(u8, String)] {
        &self.groups
    }

    /// 정규식을 상한을 지켜 컴파일한다
    pub fn compile(&self) -> Result<Marker, String> {
        Marker::new(self)
    }
}

/// 컴파일한 마커
#[derive(Debug, Clone)]
pub struct Marker {
    re: Regex,
    /// 정규식의 캡처 번호 → 색. 묶음 `mcg{i}`의 자리
    group_slots: Vec<(usize, u8)>,
}

impl Marker {
    pub fn new(spec: &MarkerSpec) -> Result<Self, String> {
        let pats: Vec<&str> = spec.groups.iter().map(|(_, p)| p.as_str()).collect();
        let re = bt_core::user_regex::compile_alternation(&pats, spec.ignore_case)?;
        let mut group_slots = Vec::new();
        for (slot, name) in re.capture_names().enumerate() {
            if let Some(i) = name
                .and_then(|n| n.strip_prefix("mcg"))
                .and_then(|n| n.parse::<usize>().ok())
            {
                if let Some((color, _)) = spec.groups.get(i) {
                    group_slots.push((slot, *color));
                }
            }
        }
        Ok(Marker { re, group_slots })
    }

    /// 글에서 맞은 곳들: (시작 바이트, 끝 바이트(제외), 색 1~3). 빈 맞음은 칠하지 않으므로 뺀다
    pub fn spans(&self, text: &str) -> Vec<(usize, usize, u8)> {
        let mut out = Vec::new();
        for caps in self.re.captures_iter(text) {
            let Some(m) = caps.get(0) else { continue };
            if m.start() == m.end() {
                continue;
            }
            let color = self
                .group_slots
                .iter()
                .find(|(slot, _)| caps.get(*slot).is_some())
                .map_or(1, |(_, c)| *c);
            out.push((m.start(), m.end(), color));
        }
        out
    }

    /// 이 줄에 `mark`번(0이면 아무) 마크가 있는가(kitty `line_has_mark`)
    pub fn row_has_mark(&self, text: &str, mark: u8) -> bool {
        self.spans(text).iter().any(|(_, _, c)| mark == 0 || *c == mark)
    }
}

/// 칸의 마커 상태(명세와 컴파일한 것)
#[derive(Debug, Clone)]
pub struct MarkerState {
    spec: MarkerSpec,
    marker: Marker,
}

impl MarkerState {
    pub fn marker(&self) -> &Marker {
        &self.marker
    }
}

/// kitty `toggle_marker`: 같은 명세면 끄고(`Ok(false)`) 아니면 바꾸어 켠다(`Ok(true)`).
/// kitty는 대소문자 구분 여부를 같음 판단에 넣지 않는다(`text 1 a`와 `itext 1 a`를 같게 본다). 우리는 구분한다
pub fn toggle(slot: &mut Option<MarkerState>, spec: &MarkerSpec) -> Result<bool, String> {
    if slot.as_ref().is_some_and(|s| s.spec == *spec) {
        *slot = None;
        return Ok(false);
    }
    let marker = Marker::new(spec)?;
    *slot = Some(MarkerState {
        spec: spec.clone(),
        marker,
    });
    Ok(true)
}

/// `scroll_to_mark`의 인자 `[prev|previous|next] [색 번호]`를 (이전인가, 색 번호 0~3)으로 읽는다(kitty `scroll_to_mark`)
pub fn parse_scroll_args(rest: &str) -> Result<(bool, u8), String> {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    let num = |s: &str| {
        s.parse::<i64>()
            .map(|n| n.clamp(0, 3) as u8)
            .map_err(|_| format!("{rest} 은(는) scroll_to_mark 목적지가 아닙니다"))
    };
    match parts.as_slice() {
        [] => Ok((true, 0)),
        [one] => match one.to_lowercase().as_str() {
            "prev" | "previous" => Ok((true, 0)),
            "next" => Ok((false, 0)),
            q => Ok((true, num(q)?)),
        },
        [dir, n, ..] => Ok((!dir.eq_ignore_ascii_case("next"), num(n)?)),
    }
}

/// kitty의 낱말 나누기(`tools/utils/shlex/shlex.go` 83~165줄): 공백으로 나누고, `'`는 안을 그대로, `"` 안과 밖의 `\\`는 다음 글자를 그대로 낸다.
/// 닫히지 않은 따옴표와 끝의 `\\`는 오류
pub fn shlex_split(s: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut in_word = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\n' | '\r' | '\t' => {
                if in_word {
                    out.push(std::mem::take(&mut buf));
                    in_word = false;
                }
            }
            '\\' => {
                buf.push(chars.next().ok_or("끝에 백슬래시가 남았습니다")?);
                in_word = true;
            }
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => buf.push(ch),
                        None => return Err("닫히지 않은 작은따옴표".into()),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => buf.push(chars.next().ok_or("닫히지 않은 큰따옴표")?),
                        Some(ch) => buf.push(ch),
                        None => return Err("닫히지 않은 큰따옴표".into()),
                    }
                }
            }
            ch => {
                buf.push(ch);
                in_word = true;
            }
        }
    }
    if in_word {
        out.push(buf);
    }
    Ok(out)
}

/// 화면 한 줄의 글과 글자별 (시작 바이트, 시작 칸, 칸 수)
pub(crate) fn row_text(cells: &[Cell]) -> (String, Vec<(usize, usize, usize)>) {
    let mut text = String::new();
    let mut map = Vec::new();
    for (col, c) in cells.iter().enumerate() {
        if c.flags.contains(CellFlags::SPACER) {
            continue;
        }
        map.push((text.len(), col, if c.flags.contains(CellFlags::WIDE) { 2 } else { 1 }));
        text.push(if c.ch == '\0' { ' ' } else { c.ch });
        text.extend(c.combining.iter());
    }
    (text, map)
}

fn mark_flag(color: u8) -> CellFlags {
    match color {
        2 => CellFlags::MARK2,
        3 => CellFlags::MARK3,
        _ => CellFlags::MARK1,
    }
}

/// 한 줄(`cells`)에 마크를 얹는다
fn mark_row(marker: &Marker, cells: &mut [Cell]) {
    let (text, map) = row_text(cells);
    for (start, end, color) in marker.spans(&text) {
        let first = map.partition_point(|(b, _, _)| *b < start);
        let last = map.partition_point(|(b, _, _)| *b < end);
        let flag = mark_flag(color);
        for &(_, col, width) in &map[first..last] {
            for c in col..(col + width).min(cells.len()) {
                cells[c].flags.insert(flag);
            }
        }
    }
}

/// 보이는 줄(과 걸친 줄)에 마크를 얹는다. 검사한 줄 수를 돌려준다
pub fn apply(screen: &mut Screen, marker: &Marker) -> usize {
    let cols = screen.cols;
    if cols == 0 {
        return 0;
    }
    let mut checked = 0;
    for row in screen.cells.chunks_mut(cols) {
        mark_row(marker, row);
        checked += 1;
    }
    if screen.edge.len() == cols {
        mark_row(marker, &mut screen.edge);
        checked += 1;
    }
    checked
}

pub(crate) fn grid_row_text<T>(term: &Term<T>, line: Line) -> String {
    let row = &term.grid()[line];
    let mut text = String::new();
    for c in 0..term.columns() {
        let cell = &row[Column(c)];
        if cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }
        text.push(if cell.c == '\0' { ' ' } else { cell.c });
        if let Some(z) = cell.zerowidth() {
            text.extend(z.iter());
        }
    }
    text
}

/// kitty `scroll_to_next_mark`: 이전(`prev`)이면 보이는 맨 윗줄 위의 첫 마크 줄이 맨 윗줄에, 다음이면 보이는 맨 아랫줄 밑의 첫 마크 줄이
/// 맨 아랫줄에 오게 할 줄 수를 돌려준다(`Scroll::Delta` 값: 양수는 위로). 대체 화면이거나 `max_lines`줄 안에 없으면 없다
pub fn scroll_amount<T: EventListener>(
    term: &Term<T>,
    marker: &Marker,
    prev: bool,
    mark: u8,
    max_lines: usize,
) -> Option<i32> {
    if term.mode().contains(TermMode::ALT_SCREEN) {
        return None;
    }
    let offset = term.grid().display_offset();
    let history = term.grid().history_size();
    let screen = term.screen_lines() as i32;
    if prev {
        let n = history.saturating_sub(offset).min(max_lines);
        (0..n)
            .find(|&k| marker.row_has_mark(&grid_row_text(term, Line(-(offset as i32) - 1 - k as i32)), mark))
            .map(|k| k as i32 + 1)
    } else {
        let n = offset.min(max_lines);
        (0..n)
            .find(|&k| marker.row_has_mark(&grid_row_text(term, Line(screen - offset as i32 + k as i32)), mark))
            .map(|k| -(k as i32 + 1))
    }
}
