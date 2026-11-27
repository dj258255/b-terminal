//! 명령 표시(#927): 셸 통합(OSC 133)이 알린 명령마다 끝난 결과(종료 코드·걸린 시간)를 프롬프트 줄에 붙여 기억한다.
//! 격자 칸·커서·프롬프트 표시(`row_prompt`)는 바꾸지 않고, 줄의 안정된 번호(`scrolled_total + 줄`)로 곁에 둔다.
//!
//! 줄 번호는 스크롤백이 가득 차 오래된 줄이 밀려나도 바뀌지 않는다. 보이는 줄에 붙일 때는 그 줄이 지금도 프롬프트 첫 줄인지
//! 엔진 표시(`ROW_PROMPT`)로 다시 확인해, 줄이 지워졌거나 밀려 번호가 어긋난 기억은 그리지 않는다(틀린 종료 코드를 보이느니 비운다)

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::Line;
use alacritty_terminal::term::Term;
use alacritty_terminal::term::semantic::RowPrompt;

/// 기억하는 명령 수의 상한(오래된 것부터 버린다). 프롬프트 기억(`MAX_PROMPTS`)과 같다
pub const MAX_MARKS: usize = 2000;

/// 명령 하나의 진행
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkState {
    /// 프롬프트만 나왔다(아직 명령을 실행하지 않았다)
    Prompt,
    /// 실행 중(`133;C` 뒤, `133;D` 전)
    Running,
    /// 끝났다. 셸이 종료 코드를 알리지 않았으면 `exit_code`는 없다
    Done { exit_code: Option<i32>, duration: Duration },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Mark {
    id: i64,
    started: Option<Instant>,
    state: MarkState,
}

/// 보이는 줄에 붙은 명령 표시
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowMark {
    /// 화면 맨 위가 0인 줄
    pub row: usize,
    pub state: MarkState,
}

/// 한 칸(탭)의 명령 기억. 줄 번호 오름차순이다
#[derive(Debug, Default, Clone)]
pub struct CommandMarks {
    marks: VecDeque<Mark>,
}

impl CommandMarks {
    pub fn len(&self) -> usize {
        self.marks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.marks.is_empty()
    }

    /// 새 프롬프트(`133;A`)가 줄 `id`에 나왔다. 같은 줄에 다시 그린 프롬프트(redraw)는 새 명령이 아니다.
    /// 줄 번호가 뒤로 갔으면(화면을 지움) 그 줄 이후의 옛 기억은 버린다
    pub fn prompt(&mut self, id: i64) {
        while self.marks.back().is_some_and(|m| m.id > id) {
            self.marks.pop_back();
        }
        if self.marks.back().is_some_and(|m| m.id == id) {
            return;
        }
        self.marks.push_back(Mark {
            id,
            started: None,
            state: MarkState::Prompt,
        });
        if self.marks.len() > MAX_MARKS {
            self.marks.pop_front();
        }
    }

    /// 명령을 실행했다(`133;C`). 가장 새 프롬프트에만 붙는다
    pub fn executed(&mut self, now: Instant) {
        if let Some(m) = self.marks.back_mut().filter(|m| m.state == MarkState::Prompt) {
            m.state = MarkState::Running;
            m.started = Some(now);
        }
    }

    /// 명령이 끝났다(`133;D`). 실행 중인 가장 새 명령만 끝난다(`C` 없이 온 `D`는 빈 프롬프트라 무시한다)
    pub fn finished(&mut self, code: Option<i32>, now: Instant) {
        if let Some(m) = self.marks.back_mut().filter(|m| m.state == MarkState::Running) {
            let duration = m.started.map_or(Duration::ZERO, |t| now.saturating_duration_since(t));
            m.state = MarkState::Done {
                exit_code: code,
                duration,
            };
        }
    }

    /// `oldest`보다 앞 줄의 기억을 버린다(스크롤백에서 사라진 줄)
    pub fn prune(&mut self, oldest: i64) {
        while self.marks.front().is_some_and(|m| m.id < oldest) {
            self.marks.pop_front();
        }
    }

    /// `cut` 줄 이후(포함)의 기억을 버린다
    pub fn truncate_from(&mut self, cut: i64) {
        while self.marks.back().is_some_and(|m| m.id >= cut) {
            self.marks.pop_back();
        }
    }

    pub fn clear(&mut self) {
        self.marks.clear();
    }

    /// 줄 번호가 바뀐 뒤(크기 변경으로 줄이 다시 접힘) 프롬프트 줄들의 새 번호(`ids`, 오름차순)로 맞춘다.
    /// 아래(최근)부터 짝을 맞추고 남는 쪽은 버린다
    pub fn realign(&mut self, ids: &[i64]) {
        let keep = self.marks.len().min(ids.len());
        while self.marks.len() > keep {
            self.marks.pop_front();
        }
        for (m, id) in self.marks.iter_mut().zip(&ids[ids.len() - keep..]) {
            m.id = *id;
        }
    }

    pub fn get(&self, id: i64) -> Option<MarkState> {
        self.marks
            .binary_search_by_key(&id, |m| m.id)
            .ok()
            .map(|i| self.marks[i].state)
    }
}

/// 줄 `line`(스크롤백은 음수)의 안정된 번호. 스크롤백이 가득 차 줄이 밀려나도 같은 줄은 같은 번호다
pub fn stable_id<T>(term: &Term<T>, line: Line) -> i64 {
    term.grid().scrolled_total() as i64 + line.0 as i64
}

/// 지금 보이는 줄(`rows`줄)에 붙는 표시. 대체 화면이면 없다. 줄이 지금도 프롬프트 첫 줄인 기억만 붙인다
pub fn visible<T: EventListener>(term: &Term<T>, marks: &CommandMarks, rows: usize) -> Vec<RowMark> {
    if marks.is_empty() || term.mode().contains(alacritty_terminal::term::TermMode::ALT_SCREEN) {
        return Vec::new();
    }
    let offset = term.grid().display_offset() as i32;
    let bottom = term.bottommost_line();
    (0..rows)
        .filter_map(|row| {
            let line = Line(row as i32 - offset);
            if line > bottom || term.row_prompt(line) != RowPrompt::Prompt {
                return None;
            }
            marks.get(stable_id(term, line)).map(|state| RowMark { row, state })
        })
        .collect()
}

/// 격자의 모든 프롬프트 첫 줄의 안정된 번호(오름차순)
pub fn prompt_ids<T: EventListener>(term: &Term<T>) -> Vec<i64> {
    let (top, bottom) = (term.topmost_line(), term.bottommost_line());
    let mut line = top;
    let mut ids = Vec::new();
    while line <= bottom {
        if term.row_prompt(line) == RowPrompt::Prompt {
            ids.push(stable_id(term, line));
        }
        line += 1;
    }
    ids
}

/// 스크롤백에 남아 있는 가장 오래된 줄의 번호
pub fn oldest_id<T>(term: &Term<T>) -> i64 {
    stable_id(term, term.topmost_line())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(secs: u64) -> Instant {
        // 같은 기준에서 더한 시각이라 간격만 의미 있다
        static BASE: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
        *BASE + Duration::from_secs(secs)
    }

    #[test]
    fn a_command_goes_prompt_running_done_with_its_duration() {
        let mut m = CommandMarks::default();
        m.prompt(10);
        assert_eq!(m.get(10), Some(MarkState::Prompt));
        m.executed(at(1));
        assert_eq!(m.get(10), Some(MarkState::Running));
        m.finished(Some(3), at(4));
        assert_eq!(
            m.get(10),
            Some(MarkState::Done {
                exit_code: Some(3),
                duration: Duration::from_secs(3)
            })
        );
    }

    #[test]
    fn finish_without_execution_is_ignored() {
        let mut m = CommandMarks::default();
        m.prompt(1);
        m.finished(Some(1), at(2));
        assert_eq!(m.get(1), Some(MarkState::Prompt));
        // 끝난 명령에 다시 온 D도 바꾸지 않는다
        m.executed(at(1));
        m.finished(Some(0), at(2));
        m.finished(Some(9), at(3));
        assert!(matches!(m.get(1), Some(MarkState::Done { exit_code: Some(0), .. })));
    }

    #[test]
    fn a_prompt_redrawn_on_the_same_line_keeps_its_state() {
        let mut m = CommandMarks::default();
        m.prompt(5);
        m.executed(at(0));
        m.prompt(5);
        assert_eq!(m.len(), 1);
        assert_eq!(m.get(5), Some(MarkState::Running));
    }

    #[test]
    fn a_prompt_on_an_earlier_line_drops_the_later_stale_marks() {
        let mut m = CommandMarks::default();
        m.prompt(5);
        m.prompt(9);
        m.prompt(7);
        assert_eq!(
            (m.get(5).is_some(), m.get(9).is_some(), m.get(7).is_some()),
            (true, false, true)
        );
    }

    #[test]
    fn the_memory_is_capped_dropping_the_oldest() {
        let mut m = CommandMarks::default();
        for i in 0..(MAX_MARKS as i64 + 5) {
            m.prompt(i);
        }
        assert_eq!(m.len(), MAX_MARKS);
        assert!(m.get(0).is_none() && m.get(4).is_none() && m.get(5).is_some());
    }

    #[test]
    fn prune_truncate_and_clear() {
        let mut m = CommandMarks::default();
        for i in [2, 4, 6, 8] {
            m.prompt(i);
        }
        m.prune(4);
        assert!(m.get(2).is_none() && m.get(4).is_some());
        m.truncate_from(6);
        assert!(m.get(6).is_none() && m.get(8).is_none() && m.get(4).is_some());
        m.clear();
        assert!(m.is_empty());
    }

    #[test]
    fn realign_pairs_from_the_newest_and_drops_the_rest() {
        let mut m = CommandMarks::default();
        for i in [1, 2, 3] {
            m.prompt(i);
        }
        m.executed(at(0));
        m.finished(Some(7), at(1));
        // 줄이 둘뿐이면 가장 오래된 기억을 버린다
        m.realign(&[20, 30]);
        assert_eq!(m.len(), 2);
        assert!(matches!(m.get(30), Some(MarkState::Done { exit_code: Some(7), .. })));
        assert_eq!(m.get(20), Some(MarkState::Prompt));
        // 줄이 더 많으면 오래된 줄은 기억이 없는 채로 둔다
        m.realign(&[1, 2, 3, 4]);
        assert_eq!((m.get(1), m.get(2)), (None, None));
        assert_eq!(m.get(3), Some(MarkState::Prompt));
        assert!(matches!(m.get(4), Some(MarkState::Done { exit_code: Some(7), .. })));
    }
}
