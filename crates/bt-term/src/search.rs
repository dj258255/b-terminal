//! 스크롤백 찾기(#39). VT 엔진의 정규식 검색을 쓰되, 사용자가 친 글은 글자 그대로 찾는다.
//!
//! 대소문자: 친 글에 대문자가 없으면 무시하고, 있으면 가린다(엔진의 smart case, Ghostty·vim과 같다)

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point, Side};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};

/// 한 화면에서 강조하는 맞은 곳의 상한
const MAX_VISIBLE: usize = 2000;
/// 개수를 세는 상한(넘으면 "1000+"로 보인다)
pub const MAX_COUNT: usize = 1000;

/// 정규식 특수 문자를 이스케이프해 글자 그대로 찾게 한다
pub fn escape(q: &str) -> String {
    let mut out = String::with_capacity(q.len() * 2);
    for c in q.chars() {
        if matches!(
            c,
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '|' | '[' | ']' | '{' | '}' | '^' | '$' | '#' | '&' | '-' | '~'
        ) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// 찾기 결과 요약: 지금 맞은 곳이 몇 번째(위에서부터), 모두 몇 개
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchStatus {
    pub index: Option<usize>,
    pub total: usize,
    /// 개수가 상한에 닿아 더 셀 수 있다
    pub capped: bool,
}

/// 칸 하나의 찾기 상태
pub(crate) struct Search {
    regex: RegexSearch,
    /// 지금 맞은 곳(엔진 좌표: 화면 맨 위가 0, 스크롤백은 음수)
    pub current: Option<Match>,
}

impl Search {
    /// 빈 글이거나 정규식을 만들 수 없으면 없다
    pub fn new(query: &str) -> Option<Self> {
        if query.is_empty() {
            return None;
        }
        RegexSearch::new(&escape(query))
            .ok()
            .map(|regex| Self { regex, current: None })
    }

    /// 다음 맞은 곳으로. `older`면 위(오래된 쪽)로, 아니면 아래로. 끝에 닿으면 반대 끝에서 다시 찾는다.
    /// 찾으면 그 자리가 보이게 화면을 옮긴다
    pub fn step<T: EventListener>(&mut self, term: &mut Term<T>, older: bool) -> bool {
        let found = if older {
            // 처음이면 화면 맨 아래 끝에서
            let origin = match &self.current {
                Some(m) => m.start().sub(term, Boundary::None, 1),
                None => Point::new(term.bottommost_line(), term.last_column()),
            };
            term.search_next(&mut self.regex, origin, Direction::Left, Side::Left, None)
        } else {
            let origin = match &self.current {
                Some(m) => m.end().add(term, Boundary::None, 1),
                None => Point::new(term.topmost_line(), Column(0)),
            };
            term.search_next(&mut self.regex, origin, Direction::Right, Side::Left, None)
        };
        if let Some(m) = &found {
            term.scroll_to_point(*m.start());
        }
        self.current = found;
        self.current.is_some()
    }

    /// 지금 보이는 줄들의 맞은 곳(위에서 아래로)
    pub fn visible<T>(&mut self, term: &Term<T>) -> Vec<Match> {
        let offset = term.grid().display_offset() as i32;
        let start = Point::new(Line(-offset), Column(0));
        let end = Point::new(Line(term.screen_lines() as i32 - 1 - offset), term.last_column());
        RegexIter::new(start, end, Direction::Right, term, &mut self.regex)
            .take(MAX_VISIBLE)
            .collect()
    }

    /// 스크롤백 전체에서 센다(상한까지)
    pub fn status<T>(&mut self, term: &Term<T>) -> SearchStatus {
        let start = Point::new(term.topmost_line(), Column(0));
        let end = Point::new(term.bottommost_line(), term.last_column());
        let mut total = 0;
        let mut index = None;
        for m in RegexIter::new(start, end, Direction::Right, term, &mut self.regex) {
            total += 1;
            if self.current.as_ref() == Some(&m) {
                index = Some(total);
            }
            if total >= MAX_COUNT {
                return SearchStatus {
                    index,
                    total,
                    capped: true,
                };
            }
        }
        SearchStatus {
            index,
            total,
            capped: false,
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
    use super::*;
    use crate::screen::CellFlags;
    use crate::session::{Emulator, GridSize};

    #[test]
    fn escape_makes_regex_chars_literal() {
        assert_eq!(escape("a.b*(c)"), "a\\.b\\*\\(c\\)");
        assert_eq!(escape("한글 [x]"), "한글 \\[x\\]");
    }

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
        assert_eq!(e.search("ERROR").unwrap().total, 0, "대문자가 있으면 대소문자를 가린다");
        let st = e.search("error").expect("찾음");
        assert_eq!((st.index, st.total), (Some(2), 2), "처음은 맨 아래에서 위로: 20번 줄");
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

    #[test]
    fn no_match_and_empty_query() {
        let mut e = Emulator::new(GridSize::new(20, 3), bt_core::Theme::ghostty_default());
        e.feed(b"hello");
        let st = e.search("nope").unwrap();
        assert_eq!((st.index, st.total), (None, 0));
        assert!(e.search("").is_none(), "빈 글은 찾기를 끈다");
    }
}
