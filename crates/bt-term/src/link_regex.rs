//! 사용자 정규식 링크(`link-url-regex`, kitty `detect_url_regex`, #929, ADR-101).
//!
//! 원문은 kitty v0.51.1의 `kitty/options/definition.py` 964~1000줄(설명)과 `kitty/screen.c` `detect_url_with_regex` 5190~5242줄·
//! `screen_detect_url` 5288줄:
//! - 보통 URL(`url_prefixes`)이 마우스 칸에 없을 때만 본다. OSC 8 링크가 먼저다.
//! - 식은 적은 순서대로 시도하고, 마우스 칸에 겹치는 첫 맞음이 링크다. 빈 맞음은 링크가 아니다.
//! - 접힌 줄은 한 줄로 이어 맞추되 마우스 줄의 위·아래 3줄까지만 본다(`DETECT_URL_REGEX_MAX_WRAPPED_LINES`).
//! - 맞은 글이 그대로 주소다.
//!
//! 이 모듈은 `links.rs`를 고치지 않는다. 찾은 주소는 보통 링크와 같은 `LinkHit`이고, 열기 전의 거르기(`classify`·`classify_text`)와
//! 확인 막대는 그대로 거친다. 그래서 정규식이 허용 스킴을 넓히지 못한다(`javascript:` 같은 주소는 맞아도 열리지 않는다).

use regex::Regex;

use crate::links::{LinkHit, LinkOptions, LinkSpan, link_at_with, wraps};
use crate::screen::{CellFlags, Screen};

/// 마우스 줄의 위·아래로 이어 보는 접힌 줄 수(kitty `DETECT_URL_REGEX_MAX_WRAPPED_LINES`)
pub const MAX_WRAPPED_ROWS: usize = 3;

/// 컴파일해 둔 사용자 링크 정규식들(설정 순서)
#[derive(Debug, Clone, Default)]
pub struct UserLinkRegexes {
    res: Vec<Regex>,
}

impl UserLinkRegexes {
    /// 설정의 정규식 글들을 상한을 지켜 컴파일한다. 못 읽는 것은 건너뛰고 이유를 돌려준다
    pub fn new(patterns: &[String]) -> (Self, Vec<String>) {
        let mut res = Vec::new();
        let mut warnings = Vec::new();
        for p in patterns.iter().take(bt_core::user_regex::MAX_PATTERNS) {
            match bt_core::user_regex::compile(p, false) {
                Ok(r) => res.push(r),
                Err(why) => warnings.push(format!("link-url-regex를 무시합니다({why}): {p}")),
            }
        }
        (UserLinkRegexes { res }, warnings)
    }

    pub fn is_empty(&self) -> bool {
        self.res.is_empty()
    }
}

/// `links::link_at_with`와 같고, 거기서 링크가 없고 `opts.url`이 켜져 있으면 사용자 정규식을 본다
pub fn link_at_with_user(
    screen: &Screen,
    col: usize,
    row: usize,
    opts: LinkOptions,
    user: Option<&UserLinkRegexes>,
) -> Option<LinkHit> {
    if let Some(h) = link_at_with(screen, col, row, opts) {
        return Some(h);
    }
    let user = user.filter(|u| !u.is_empty() && opts.url)?;
    if col >= screen.cols || row >= screen.rows {
        return None;
    }
    let col = if screen.cell(col, row).flags.contains(CellFlags::SPACER) && col > 0 {
        col - 1
    } else {
        col
    };
    // 접힌 줄 묶음 안에서 마우스 줄의 위·아래 `MAX_WRAPPED_ROWS`줄까지
    let mut top = row;
    while top > 0 && row - top < MAX_WRAPPED_ROWS && wraps(screen, top - 1) {
        top -= 1;
    }
    let mut bottom = row;
    while bottom + 1 < screen.rows && bottom - row < MAX_WRAPPED_ROWS && wraps(screen, bottom) {
        bottom += 1;
    }
    let mut text = String::new();
    // 글자마다 (시작 바이트, 줄, 칸, 폭)
    let mut pos: Vec<(usize, usize, usize, usize)> = Vec::new();
    for r in top..=bottom {
        // 마지막 줄 끝의 빈칸은 글이 아니다(kitty: 쓰지 않은 칸)
        let mut limit = screen.cols;
        if r == bottom {
            while limit > 0
                && screen.cell(limit - 1, r).ch == ' '
                && !screen.cell(limit - 1, r).flags.contains(CellFlags::SPACER)
            {
                limit -= 1;
            }
        }
        for c in 0..limit {
            let cell = screen.cell(c, r);
            if cell.flags.contains(CellFlags::SPACER) {
                continue;
            }
            pos.push((
                text.len(),
                r,
                c,
                if cell.flags.contains(CellFlags::WIDE) { 2 } else { 1 },
            ));
            text.push(if cell.ch == '\0' { ' ' } else { cell.ch });
            text.extend(cell.combining.iter());
        }
    }
    let here = pos.iter().position(|&(_, r, c, _)| r == row && c == col)?;
    let mouse_start = pos[here].0;
    let mouse_end = pos.get(here + 1).map_or(text.len(), |p| p.0);
    for re in &user.res {
        for m in re.find_iter(&text) {
            if m.start() >= mouse_end {
                break;
            }
            if m.end() > mouse_start && m.end() > m.start() {
                let first = pos.partition_point(|p| p.0 < m.start());
                let last = pos.partition_point(|p| p.0 < m.end());
                let mut spans: Vec<LinkSpan> = Vec::new();
                for &(_, r, c, w) in &pos[first..last] {
                    match spans.last_mut() {
                        Some(s) if s.row == r => s.end = c + w,
                        _ => spans.push(LinkSpan {
                            row: r,
                            start: c,
                            end: c + w,
                        }),
                    }
                }
                return Some(LinkHit {
                    url: m.as_str().to_string(),
                    spans,
                    explicit: false,
                });
            }
        }
    }
    None
}
