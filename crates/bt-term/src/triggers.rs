//! 트리거(iTerm2 Triggers의 안전한 부분집합, #928, ADR-102).
//!
//! 출력 줄이 사용자 정규식에 맞으면 **강조·알림·벨·탭 표시**만 한다. 글 보내기·명령 실행·프로필 바꾸기·파일 열기는 영구히 없다.
//! 그래서 이 모듈의 `Effect`에는 PTY에 쓸 바이트를 싣는 자리가 없다(알림 본문 글뿐이다). 화면에 찍힌 글(남의 서버, 에이전트 출력)이
//! 입력이나 명령이 되는 길을 만들지 않는다(AGENTS.md "OSC 문자열은 믿지 않습니다"와 같은 까닭).
//!
//! iTerm2 문서(https://iterm2.com/documentation-triggers.html, 믿지 않는 자료)와 맞춘 것과 다른 것:
//! - 한 번에 한 줄만 맞춘다(여러 줄 정규식 없음). iTerm2는 개행이나 커서를 옮기는 이스케이프에서 맞추고, `Instant` 트리거는
//!   개행 없이 맞는 즉시 한 번 낸다. 여기서는 읽기 한 번이 끝날 때마다 새 줄과 쓰는 중인 줄(커서 줄)을 본다. 그래서 `Password: `처럼
//!   개행이 없는 프롬프트도 따로 켜는 것 없이 한 번 맞는다.
//! - iTerm2 문서는 같은 줄에서 되풀이 맞음을 어떻게 다루는지 적지 않았다. 여기서는 (줄, 규칙) 하나당 한 번만 알린다.
//! - 알림·벨·탭 표시는 새로 찍힌 줄에서만 낸다. 강조는 지금 보이는 글에 매번 덧칠한다(상태를 쌓지 않는다).
//!
//! 읽기 스레드 비용 상한: 줄 하나는 `MAX_LINE_CHARS`글자까지, 읽기 한 번에 `MAX_SCAN_LINES`줄까지만 본다(그보다 많이 밀려 나간 줄은
//! 건너뛰고 `skipped_lines`에 센다). 정규식은 `regex` 크레이트(백트래킹·역참조 없음)라 시간이 글 길이에 비례한다.

use std::time::{Duration, Instant};

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::Line;
use alacritty_terminal::term::{Term, TermMode};
use bt_core::trigger::{Highlight, MAX_TRIGGERS, TriggerSpec};
use regex::Regex;

use crate::screen::{Cell, Screen};

/// 줄 하나에서 보는 글자 수 상한
pub const MAX_LINE_CHARS: usize = 4096;
/// 읽기 한 번에 검사하는 줄 수 상한
pub const MAX_SCAN_LINES: usize = 512;
/// 알림 본문의 글자 수 상한(맞은 글을 이만큼 자른다)
pub const MAX_BODY_CHARS: usize = 200;
/// 알림 속도 상한: 처음 이만큼은 곧바로 나가고(버킷), 그 뒤에는 `NOTIFY_REFILL`마다 하나씩 채운다
pub const NOTIFY_BURST: f64 = 3.0;
pub const NOTIFY_REFILL: Duration = Duration::from_secs(10);
/// 벨 사이 최소 간격
pub const BELL_GAP: Duration = Duration::from_secs(1);
/// 줄 하나, 규칙 하나에서 칠하는 맞음 수 상한
const MAX_HIGHLIGHT_MATCHES: usize = 64;

/// 트리거가 내는 일. 바이트를 싣는 값이 없다
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// 데스크톱 알림. 본문은 맞은 글을 자르고 제어 문자를 지운 것이다(제목은 앱이 칸 제목으로 채운다)
    Notify {
        body: String,
    },
    Bell,
    /// 탭 주의 표시(안 읽음 표시). 소리도 알림도 없다
    Attention,
}

struct Rule {
    re: Regex,
    spec: TriggerSpec,
}

pub struct Triggers {
    rules: Vec<Rule>,
    fires: bool,
    /// 아직 끝까지 검사하지 않은 첫 줄(절대 번호). 없으면 다음 검사가 커서 줄부터 시작한다
    done: Option<i64>,
    /// 쓰는 중인 줄에서 이미 낸 규칙들: (절대 줄, 규칙 비트)
    memo: (i64, u16),
    dims: (usize, usize),
    notify_tokens: f64,
    notify_at: Option<Instant>,
    bell_at: Option<Instant>,
    /// 줄 글을 담는 버퍼(읽기마다 새로 할당하지 않는다)
    scratch: String,
    /// 지금까지 검사한 줄 수, 상한 때문에 건너뛴 줄 수, 알림·벨·표시 규칙에 맞은 줄 수(시험·진단용)
    pub scanned_lines: u64,
    pub skipped_lines: u64,
    pub matched_lines: u64,
}

/// 격자 한 줄을 `buf`에 글로 옮긴다(끝의 빈 칸은 뺀다, `MAX_LINE_CHARS`글자까지). 빠른 출력 때 읽기 스레드가 줄마다 부르므로
/// 줄 끝의 빈 칸을 먼저 건너뛰고 글자만 옮긴다
fn line_text_into<T>(term: &Term<T>, line: Line, buf: &mut String) {
    use alacritty_terminal::index::Column;
    use alacritty_terminal::term::cell::Flags;
    buf.clear();
    let row = &term.grid()[line];
    let mut end = term.columns();
    while end > 0 {
        let c = &row[Column(end - 1)];
        if (c.c == ' ' || c.c == '\0') && c.zerowidth().is_none() {
            end -= 1;
        } else {
            break;
        }
    }
    let mut chars = 0;
    for col in 0..end {
        let cell = &row[Column(col)];
        if cell
            .flags
            .intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER)
        {
            continue;
        }
        buf.push(if cell.c == '\0' { ' ' } else { cell.c });
        chars += 1;
        if let Some(z) = cell.zerowidth() {
            buf.extend(z.iter());
            chars += z.len();
        }
        if chars >= MAX_LINE_CHARS {
            break;
        }
    }
    let n = buf.trim_end().len();
    buf.truncate(n);
}

fn line_text<T>(term: &Term<T>, line: Line) -> String {
    let mut s = String::new();
    line_text_into(term, line, &mut s);
    s
}

/// 알림 본문: 제어 문자·글 순서를 바꾸는 서식 문자를 지우고 자른다
pub fn sanitize_body(s: &str) -> String {
    let clean = bt_core::proto::printable(s);
    let clean = clean.trim();
    clean
        .chars()
        .take(MAX_BODY_CHARS)
        .collect::<String>()
        .trim_end()
        .to_string()
}

impl Triggers {
    /// 규칙을 컴파일한다. 못 읽는 규칙은 빼고 이유를 돌려준다(`MAX_TRIGGERS`개까지)
    pub fn new(specs: &[TriggerSpec]) -> (Self, Vec<String>) {
        let mut rules = Vec::new();
        let mut errors = Vec::new();
        for spec in specs {
            if rules.len() >= MAX_TRIGGERS {
                errors.push(format!("트리거는 {MAX_TRIGGERS}개까지 받습니다"));
                break;
            }
            match bt_core::user_regex::compile(&spec.pattern, false) {
                Ok(re) => rules.push(Rule { re, spec: spec.clone() }),
                Err(why) => errors.push(format!("{}: {why}", spec.pattern)),
            }
        }
        let fires = rules.iter().any(|r| r.spec.fires());
        (
            Self {
                rules,
                fires,
                done: None,
                memo: (i64::MIN, 0),
                dims: (0, 0),
                notify_tokens: NOTIFY_BURST,
                notify_at: None,
                bell_at: None,
                scratch: String::new(),
                scanned_lines: 0,
                skipped_lines: 0,
                matched_lines: 0,
            },
            errors,
        )
    }

    /// 지금 화면에서부터 본다(규칙을 걸 때 부른다). 이미 있던 줄은 다시 알리지 않는다: 커서 줄에 이미 맞는 규칙은 낸 것으로 친다
    pub fn arm<T: EventListener>(&mut self, term: &Term<T>) {
        self.dims = (term.screen_lines(), term.columns());
        self.memo = (i64::MIN, 0);
        if term.mode().contains(TermMode::ALT_SCREEN) {
            self.done = None;
            return;
        }
        let cur = term.grid().scrolled_total() as i64 + i64::from(term.grid().cursor.point.line.0);
        let text = line_text(term, term.grid().cursor.point.line);
        let mut mask = 0u16;
        for (i, rule) in self.rules.iter().enumerate() {
            if rule.spec.fires() && rule.re.find_iter(&text).any(|m| !m.is_empty()) {
                mask |= 1 << i;
            }
        }
        self.done = Some(cur);
        self.memo = (cur, mask);
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// 칠하기 색표(규칙 순서, 번호는 1부터). 칠하지 않는 규칙은 빈 색이다
    pub fn colors(&self) -> Vec<Highlight> {
        self.rules
            .iter()
            .map(|r| r.spec.highlight.unwrap_or_default())
            .collect()
    }

    fn has_highlight(&self) -> bool {
        self.rules.iter().any(|r| r.spec.highlight.is_some())
    }

    /// 읽기 한 번이 끝난 뒤 불러 새로 찍힌 줄(과 쓰는 중인 줄)을 검사한다. 대체 화면은 보지 않는다
    pub fn scan<T: EventListener>(&mut self, term: &Term<T>, now: Instant) -> Vec<Effect> {
        if !self.fires {
            return Vec::new();
        }
        if term.mode().contains(TermMode::ALT_SCREEN) {
            // 전체 화면 프로그램은 같은 글을 계속 다시 그린다. 돌아오면 그때의 커서 줄부터 본다
            self.done = None;
            return Vec::new();
        }
        let grid = term.grid();
        let scrolled = term.grid().scrolled_total() as i64;
        let cur = scrolled + i64::from(grid.cursor.point.line.0);
        let lowest = scrolled - grid.history_size() as i64;
        let dims = (term.screen_lines(), term.columns());
        if dims != self.dims {
            // 크기가 바뀌면 줄이 다시 접혀 번호가 맞지 않는다. 옛 줄을 되풀이해 알리지 않게 여기서부터 본다
            self.dims = dims;
            self.done = None;
        }
        let mut start = match self.done {
            Some(d) if d <= cur => d,
            _ => {
                self.memo = (i64::MIN, 0);
                cur
            }
        };
        start = start.max(lowest);
        if cur - start + 1 > MAX_SCAN_LINES as i64 {
            let keep = cur - (MAX_SCAN_LINES as i64 - 1);
            self.skipped_lines += (keep - start) as u64;
            start = keep;
        }
        let mut notify: Option<String> = None;
        let mut bell = false;
        let mut attention = false;
        let mut memo_mask = if self.memo.0 == cur { self.memo.1 } else { 0 };
        let mut text = std::mem::take(&mut self.scratch);
        for abs in start..=cur {
            let line = Line((abs - scrolled) as i32);
            line_text_into(term, line, &mut text);
            self.scanned_lines += 1;
            if text.is_empty() {
                continue;
            }
            let mut seen: u16 = if abs == self.memo.0 { self.memo.1 } else { 0 };
            let mut hit = false;
            for (i, rule) in self.rules.iter().enumerate() {
                let bit = 1u16 << i;
                if !rule.spec.fires() || seen & bit != 0 {
                    continue;
                }
                let Some(m) = rule.re.find_iter(&text).find(|m| !m.is_empty()) else {
                    continue;
                };
                hit = true;
                seen |= bit;
                if rule.spec.notify && notify.is_none() {
                    let body = sanitize_body(m.as_str());
                    if !body.is_empty() {
                        notify = Some(body);
                    }
                }
                bell |= rule.spec.bell;
                attention |= rule.spec.attention;
            }
            if hit {
                self.matched_lines += 1;
            }
            if abs == cur {
                memo_mask = seen;
            }
        }
        self.scratch = text;
        self.done = Some(cur);
        self.memo = (cur, memo_mask);

        let mut out = Vec::new();
        if let Some(body) = notify
            && self.take_notify_token(now)
        {
            out.push(Effect::Notify { body });
        }
        if bell
            && self
                .bell_at
                .is_none_or(|t| now.saturating_duration_since(t) >= BELL_GAP)
        {
            self.bell_at = Some(now);
            out.push(Effect::Bell);
        }
        if attention {
            out.push(Effect::Attention);
        }
        out
    }

    fn take_notify_token(&mut self, now: Instant) -> bool {
        if let Some(at) = self.notify_at {
            let gained = now.saturating_duration_since(at).as_secs_f64() / NOTIFY_REFILL.as_secs_f64();
            self.notify_tokens = (self.notify_tokens + gained).min(NOTIFY_BURST);
        }
        self.notify_at = Some(now);
        if self.notify_tokens >= 1.0 {
            self.notify_tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// 보이는 줄(과 픽셀 스크롤로 걸친 줄)에서 맞는 글을 칠한다. 격자는 바꾸지 않고 칸 표시와 색표만 얹는다.
    /// 같은 자리에서는 앞 규칙이 이긴다
    pub fn highlight(&self, screen: &mut Screen) {
        if !self.has_highlight() || screen.cols == 0 {
            return;
        }
        screen.trigger_colors = self.colors();
        let cols = screen.cols;
        for row in screen.cells.chunks_mut(cols) {
            self.highlight_row(row);
        }
        if screen.edge.len() == cols {
            self.highlight_row(&mut screen.edge);
        }
    }

    fn highlight_row(&self, cells: &mut [Cell]) {
        let (text, map) = crate::marks::row_text(cells);
        // 줄 끝의 빈 칸에는 맞추지 않는다(`.*`가 빈 칸까지 칠하지 않게)
        let text = &text[..text.trim_end().len()];
        if text.is_empty() {
            return;
        }
        for (i, rule) in self.rules.iter().enumerate() {
            if rule.spec.highlight.is_none() {
                continue;
            }
            let n = (i + 1) as u8;
            for m in rule
                .re
                .find_iter(text)
                .filter(|m| !m.is_empty())
                .take(MAX_HIGHLIGHT_MATCHES)
            {
                let first = map.partition_point(|(b, _, _)| *b < m.start());
                let last = map.partition_point(|(b, _, _)| *b < m.end());
                for &(_, col, width) in &map[first..last] {
                    for c in col..(col + width).min(cells.len()) {
                        if cells[c].flags.trigger().is_none() {
                            cells[c].flags.set_trigger(n);
                        }
                    }
                }
            }
        }
    }
}

/// 칸에 칠한 색: 정하지 않은 쪽은 칸의 원래 색
pub fn pick(colors: &[Highlight], n: u8, fg: bt_core::Rgb, bg: bt_core::Rgb) -> (bt_core::Rgb, bt_core::Rgb) {
    match colors.get(usize::from(n).wrapping_sub(1)) {
        Some(h) => (h.fg.unwrap_or(fg), h.bg.unwrap_or(bg)),
        None => (fg, bg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notify_bucket_refills_slowly() {
        let (mut t, _) = Triggers::new(&[]);
        let t0 = Instant::now();
        assert!(t.take_notify_token(t0));
        assert!(t.take_notify_token(t0));
        assert!(t.take_notify_token(t0));
        assert!(!t.take_notify_token(t0), "burst를 다 썼다");
        assert!(!t.take_notify_token(t0 + Duration::from_secs(5)));
        assert!(t.take_notify_token(t0 + Duration::from_secs(10)));
        assert!(!t.take_notify_token(t0 + Duration::from_secs(10)));
        // 오래 쉬어도 burst를 넘겨 쌓이지 않는다
        let later = t0 + Duration::from_secs(3600);
        let n = (0..10).filter(|_| t.take_notify_token(later)).count();
        assert_eq!(n, 3);
    }

    #[test]
    fn sanitize_cuts_controls_and_length() {
        assert_eq!(sanitize_body("a\u{7}b\u{1b}c\u{202e}d"), "a b cd");
        assert_eq!(sanitize_body(&"가".repeat(500)).chars().count(), MAX_BODY_CHARS);
        assert_eq!(sanitize_body("  \t "), "");
    }

    #[test]
    fn pick_keeps_unset_sides() {
        use bt_core::Rgb;
        let c = [Highlight {
            fg: None,
            bg: Some(Rgb::new(1, 2, 3)),
        }];
        assert_eq!(
            pick(&c, 1, Rgb::new(9, 9, 9), Rgb::new(8, 8, 8)),
            (Rgb::new(9, 9, 9), Rgb::new(1, 2, 3))
        );
        assert_eq!(
            pick(&c, 2, Rgb::new(9, 9, 9), Rgb::new(8, 8, 8)),
            (Rgb::new(9, 9, 9), Rgb::new(8, 8, 8))
        );
        assert_eq!(
            pick(&c, 0, Rgb::new(9, 9, 9), Rgb::new(8, 8, 8)),
            (Rgb::new(9, 9, 9), Rgb::new(8, 8, 8))
        );
    }
}
