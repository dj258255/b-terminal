//! 키보드로 화면의 글을 고르는 힌트(kitty `hints` kitten의 터미널 안 몫, #865).
//!
//! 화면에 보이는 칸에서 종류(`HintKind`)별로 맞은 곳(`HintMark`)을 찾고, 맞은 곳마다 라벨을 붙인다. 라벨을 치면 그 글을 열거나·복사하거나·붙여넣는다
//! (동작은 앱이 한다). 여기는 창 없이 시험할 수 있는 찾기와 라벨 차례만 둔다. 찾기는 링크 스캐너(`links::find_urls`)를 다시 쓴다.
//!
//! kitty 0.51.1 `kittens/hints/`(marks.go, main.go, hints_generator.go)와 맞춘 것: 라벨 알파벳·차례·`offset 1`·아래가 1번,
//! 종류별 정규식(`path`·`line`·`hash`·`word`·`linenum`)과 앞뒤 괄호·따옴표 벗기기, 최소 3글자. 다른 것은 docs/decisions.md ADR-094
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::links::{LinkSpan, find_urls, starts_with_known_scheme, wraps};
use crate::screen::{CellFlags, Screen};

/// 라벨 글자(kitty `DEFAULT_HINT_ALPHABET`)
pub const DEFAULT_ALPHABET: &str = "0123456789abcdefghijklmnopqrstuvwxyz";
/// 번호 시작(kitty `--hints-offset` 기본 1): 한 글자 "0"을 건너뛰어 두 글자 라벨이 "0"으로 시작하게 한다
const HINT_OFFSET: usize = 1;
/// 맞은 곳의 최소 글자 수(kitty `--minimum-match-length` 기본 3)
const MIN_CHARS: usize = 3;
/// 한 번에 붙일 라벨 수의 상한: 라벨이 두 글자를 넘지 않는 수(36 + 36 * 36 - 번호 시작 1 - 1). 넘으면 맨 아래 쪽만 남긴다
pub const MAX_MARKS: usize = 1332;
/// 해시: 16진 글자 7~128자(kitty `[0-9a-f][0-9a-f\r]{6,127}`)
const HASH_MIN: usize = 7;
const HASH_MAX: usize = 128;
/// 낱말에 들어가는 글자 말고도 낱말로 치는 글자(kitty `select_by_word_characters` 기본)
const WORD_CHARS: &str = "@-./_~?&=%+#";

/// 무엇을 고를까(kitty `--type`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintKind {
    /// 스킴 주소
    Url,
    /// 경로·파일 이름
    Path,
    /// `경로:줄`(오류 메시지 꼴)
    Linenum,
    /// 줄 전체
    Line,
    /// 16진 해시(git 커밋 등)
    Hash,
    /// 낱말
    Word,
    /// 프로그램이 OSC 8로 단 링크
    Hyperlink,
}

impl HintKind {
    pub const ALL: [HintKind; 7] = [
        HintKind::Url,
        HintKind::Path,
        HintKind::Linenum,
        HintKind::Line,
        HintKind::Hash,
        HintKind::Word,
        HintKind::Hyperlink,
    ];

    pub fn name(self) -> &'static str {
        match self {
            HintKind::Url => "url",
            HintKind::Path => "path",
            HintKind::Linenum => "linenum",
            HintKind::Line => "line",
            HintKind::Hash => "hash",
            HintKind::Word => "word",
            HintKind::Hyperlink => "hyperlink",
        }
    }

    pub fn parse(s: &str) -> Option<HintKind> {
        HintKind::ALL.into_iter().find(|k| k.name() == s)
    }

    /// 이 종류에 이 동작을 할 수 있는가. 열기는 열 수 있는 것(주소·경로·`경로:줄`·OSC 8)만, 복사·붙여넣기는 모두
    pub fn allows(self, a: HintAction) -> bool {
        a != HintAction::Open
            || matches!(
                self,
                HintKind::Url | HintKind::Path | HintKind::Linenum | HintKind::Hyperlink
            )
    }

    /// 동작을 적지 않았을 때: 열 수 있는 것은 열기, 나머지는 붙여넣기(kitty 기본 묶음의 `--program -`와 같다)
    pub fn default_action(self) -> HintAction {
        if self.allows(HintAction::Open) {
            HintAction::Open
        } else {
            HintAction::Paste
        }
    }
}

/// 고른 글로 무엇을 할까(kitty `--program` 기본·`@`·`-`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintAction {
    Open,
    Copy,
    Paste,
}

impl HintAction {
    pub fn name(self) -> &'static str {
        match self {
            HintAction::Open => "open",
            HintAction::Copy => "copy",
            HintAction::Paste => "paste",
        }
    }

    pub fn parse(s: &str) -> Option<HintAction> {
        [HintAction::Open, HintAction::Copy, HintAction::Paste]
            .into_iter()
            .find(|a| a.name() == s)
    }
}

/// 화면에서 찾은 맞은 곳 하나
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HintMark {
    /// 고른 글(OSC 8이면 주소). 칸의 글자에 붙은 결합 문자까지 이은 것이고 아직 믿지 않는다
    pub text: String,
    /// 차지한 칸 구간(접힌 줄이면 줄마다)
    pub spans: Vec<LinkSpan>,
    /// `Linenum`의 경로와 줄 번호
    pub path: Option<String>,
    pub line: Option<u32>,
    /// 구간 칸의 글자를 묶은 값. 화면이 밀리거나 바뀌었는지 볼 때 쓴다
    pub sig: u64,
}

/// 접힌 줄을 이은 한 논리 줄: 칸마다(뒤 칸 제외) 글자, 자리(줄·칸·폭), 결합 문자를 이은 글
struct Line {
    chars: Vec<char>,
    pos: Vec<(usize, usize, usize)>,
    text: Vec<String>,
}

fn logical_lines(screen: &Screen) -> Vec<Line> {
    let mut out = Vec::new();
    let mut row = 0;
    while row < screen.rows {
        let top = row;
        let mut bottom = row;
        while bottom + 1 < screen.rows && wraps(screen, bottom) {
            bottom += 1;
        }
        let mut line = Line {
            chars: Vec::new(),
            pos: Vec::new(),
            text: Vec::new(),
        };
        for r in top..=bottom {
            for c in 0..screen.cols {
                let cell = screen.cell(c, r);
                if cell.flags.contains(CellFlags::SPACER) {
                    continue;
                }
                let ch = if cell.ch == '\0' { ' ' } else { cell.ch };
                line.chars.push(ch);
                line.pos
                    .push((r, c, if cell.flags.contains(CellFlags::WIDE) { 2 } else { 1 }));
                let mut t = String::from(ch);
                t.extend(cell.combining.iter());
                line.text.push(t);
            }
        }
        out.push(line);
        row = bottom + 1;
    }
    out
}

/// 구간의 칸 묶음이 바뀌었는지 보는 값(줄·칸·글자)
pub fn spans_signature(screen: &Screen, spans: &[LinkSpan]) -> u64 {
    let mut h = DefaultHasher::new();
    for sp in spans {
        sp.row.hash(&mut h);
        sp.start.hash(&mut h);
        if sp.row >= screen.rows {
            u32::MAX.hash(&mut h);
            continue;
        }
        for c in sp.start..sp.end.min(screen.cols) {
            screen.cell(c, sp.row).ch.hash(&mut h);
        }
    }
    h.finish()
}

fn mark_of(screen: &Screen, line: &Line, s: usize, e: usize) -> HintMark {
    let mut spans: Vec<LinkSpan> = Vec::new();
    for &(r, c, w) in &line.pos[s..e] {
        match spans.last_mut() {
            Some(sp) if sp.row == r => sp.end = c + w,
            _ => spans.push(LinkSpan {
                row: r,
                start: c,
                end: c + w,
            }),
        }
    }
    let sig = spans_signature(screen, &spans);
    HintMark {
        text: line.text[s..e].concat(),
        spans,
        path: None,
        line: None,
        sig,
    }
}

fn closing(c: char) -> Option<char> {
    Some(match c {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '<' => '>',
        '"' => '"',
        '\'' => '\'',
        '“' => '”',
        '‘' => '’',
        _ => return None,
    })
}

/// kitty `matching_remover`: 구간이 여는 글자로 시작하면, 닫는 글자로 끝날 때 둘 다 벗기고, 닫는 글자가 구간 바로 뒤에 있으면 앞만 벗긴다
fn strip_pair(chars: &[char], s: usize, e: usize, openers: &[char]) -> (usize, usize) {
    if s < e && e <= chars.len() && openers.contains(&chars[s]) {
        if let Some(q) = closing(chars[s]) {
            if s < e - 1 && chars[e - 1] == q {
                return (s + 1, e - 1);
            }
            if chars.get(e) == Some(&q) {
                return (s + 1, e);
            }
        }
    }
    (s, e)
}

const BRACKETS: [char; 4] = ['(', '{', '[', '<'];
const QUOTES: [char; 4] = ['\'', '"', '“', '‘'];

fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '\0'
}

/// 파일 이름처럼 보이는 확장자(kitty `FILE_EXTENSION`): 영숫자 2~7자, 또는 한 글자 `a h c m o`
fn has_extension(s: &[char]) -> bool {
    let Some(dot) = s.iter().rposition(|&c| c == '.') else {
        return false;
    };
    if dot == 0 {
        return false;
    }
    let ext = &s[dot + 1..];
    let alnum = ext.iter().all(char::is_ascii_alphanumeric);
    alnum && ((2..=7).contains(&ext.len()) || (ext.len() == 1 && "ahcmo".contains(ext[0])))
}

/// 경로로 볼 만한가(kitty `path_regex`): 슬래시 뒤에 글자가 있거나 파일 확장자가 있고, 글자·숫자가 하나는 있다
fn looks_like_path(s: &[char]) -> bool {
    s.len() >= MIN_CHARS
        && s.iter().any(|c| c.is_alphanumeric())
        && (s.windows(2).any(|w| w[0] == '/') || has_extension(s))
}

/// 낱말 끝에 붙은 문장부호와 짝 없는 닫는 글자를 벗긴다
fn trim_token_end(chars: &[char], s: usize, mut e: usize) -> usize {
    while e > s {
        let c = chars[e - 1];
        let unmatched_closer = |open: char| {
            chars[s..e].iter().filter(|&&x| x == c).count() > chars[s..e].iter().filter(|&&x| x == open).count()
        };
        let strip = match c {
            '.' | ',' | ';' | ':' | '!' | '?' | '"' | '\'' | '”' | '’' => true,
            ')' => unmatched_closer('('),
            ']' => unmatched_closer('['),
            '}' => unmatched_closer('{'),
            '>' => unmatched_closer('<'),
            _ => false,
        };
        if !strip {
            break;
        }
        e -= 1;
    }
    e
}

/// 경로 후보 구간: 링크 스캐너가 찾은 비스킴 구간과, 공백으로 나눈 낱말 중 경로로 볼 만한 것(스캐너가 놓친 `Cargo.toml` 같은 것)
fn path_candidates(chars: &[char]) -> Vec<(usize, usize)> {
    let mut found: Vec<(usize, usize)> = find_urls(chars)
        .into_iter()
        .filter(|&(s, _)| !starts_with_known_scheme(chars, s))
        .collect();
    let mut i = 0;
    while i < chars.len() {
        if is_space(chars[i]) {
            i += 1;
            continue;
        }
        let a = i;
        while i < chars.len() && !is_space(chars[i]) {
            i += 1;
        }
        let tok = &chars[a..i];
        if tok.windows(3).any(|w| w == [':', '/', '/']) {
            continue;
        }
        let (s, e) = strip_pair(chars, a, i, &BRACKETS);
        let (s, e) = strip_pair(chars, s, e, &QUOTES);
        let e = trim_token_end(chars, s, e);
        // 줄 번호가 붙은 낱말(`a.rs:12:5`)은 확장자 검사에서 `:줄`을 뺀 것으로 본다
        let core_end = line_suffix(chars, s, e).map_or(e, |(end, _)| end);
        if looks_like_path(&chars[s..core_end]) && !found.iter().any(|&(fs, fe)| s < fe && fs < e) {
            found.push((s, e));
        }
    }
    found.sort_unstable();
    found
}

/// 끝의 `:줄` 또는 `:줄:열…`을 뗀다: (경로 끝, 줄 번호). 줄은 경로에 가장 가까운 숫자(kitty `linenum_group_processor`)
fn line_suffix(chars: &[char], s: usize, e: usize) -> Option<(usize, u32)> {
    let mut j = e;
    let mut line = None;
    loop {
        let mut k = j;
        while k > s && chars[k - 1].is_ascii_digit() {
            k -= 1;
        }
        if k < j && k > s && chars[k - 1] == ':' && j - k <= 9 {
            line = Some(chars[k..j].iter().collect::<String>().parse::<u32>().ok()?);
            j = k - 1;
        } else {
            break;
        }
    }
    line.filter(|_| j > s).map(|l| (j, l))
}

fn hex(c: char) -> bool {
    matches!(c, '0'..='9' | 'a'..='f')
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || WORD_CHARS.contains(c)
}

/// 맞은 곳을 위에서 아래, 왼쪽에서 오른쪽 차례로 찾는다. `MAX_MARKS`를 넘으면 맨 아래 쪽만 남긴다
pub fn find_marks(screen: &Screen, kind: HintKind) -> Vec<HintMark> {
    let mut marks = if kind == HintKind::Hyperlink {
        hyperlink_marks(screen)
    } else {
        let mut v = Vec::new();
        for line in logical_lines(screen) {
            line_marks(screen, &line, kind, &mut v);
        }
        v
    };
    if marks.len() > MAX_MARKS {
        marks.drain(..marks.len() - MAX_MARKS);
    }
    marks
}

fn line_marks(screen: &Screen, line: &Line, kind: HintKind, out: &mut Vec<HintMark>) {
    let chars = &line.chars;
    let mut push = |s: usize, e: usize, path: Option<String>, ln: Option<u32>| {
        if e > s && e - s >= MIN_CHARS {
            let mut m = mark_of(screen, line, s, e);
            m.path = path;
            m.line = ln;
            out.push(m);
        }
    };
    match kind {
        HintKind::Url => {
            for (s, mut e) in find_urls(chars) {
                if starts_with_known_scheme(chars, s) {
                    // kitty `url` 후처리: 끝의 `, . ? !`와 reStructuredText의 `` `_ ``를 뺀다
                    while e > s + 1 && matches!(chars[e - 1], ',' | '.' | '?' | '!') {
                        e -= 1;
                    }
                    if e > s + 3 && chars[e - 2] == '`' && chars[e - 1] == '_' {
                        e -= 2;
                    }
                    push(s, e, None, None);
                }
            }
        }
        HintKind::Path => {
            for (s, e) in path_candidates(chars) {
                let end = line_suffix(chars, s, e).map_or(e, |(end, _)| end);
                if looks_like_path(&chars[s..end]) {
                    push(s, end, None, None);
                }
            }
        }
        HintKind::Linenum => {
            for (s, e) in path_candidates(chars) {
                if let Some((end, ln)) = line_suffix(chars, s, e) {
                    if looks_like_path(&chars[s..end]) {
                        let path: String = line.text[s..end].concat();
                        push(s, e, Some(path), Some(ln));
                    }
                }
            }
        }
        HintKind::Line => {
            let s = chars.iter().position(|&c| !is_space(c));
            let e = chars.iter().rposition(|&c| !is_space(c));
            if let (Some(s), Some(e)) = (s, e) {
                push(s, e + 1, None, None);
            }
        }
        HintKind::Hash => {
            let mut i = 0;
            while i < chars.len() {
                if hex(chars[i]) {
                    let mut j = i;
                    while j < chars.len() && j - i < HASH_MAX && hex(chars[j]) {
                        j += 1;
                    }
                    if j - i >= HASH_MIN {
                        push(i, j, None, None);
                    }
                    i = j;
                } else {
                    i += 1;
                }
            }
        }
        HintKind::Word => {
            let mut i = 0;
            while i < chars.len() {
                if word_char(chars[i]) {
                    let a = i;
                    while i < chars.len() && word_char(chars[i]) {
                        i += 1;
                    }
                    let (s, e) = strip_pair(chars, a, i, &BRACKETS);
                    let (s, e) = strip_pair(chars, s, e, &QUOTES);
                    // 글자·숫자 없이 `---`·`...`만 있는 덩어리는 낱말이 아니다(kitty는 마크로 만든다)
                    if chars[s..e].iter().any(|c| c.is_alphanumeric()) {
                        push(s, e, None, None);
                    }
                } else {
                    i += 1;
                }
            }
        }
        HintKind::Hyperlink => {}
    }
}

/// OSC 8 링크: 같은 링크가 이어진 칸 묶음 하나가 마크 하나. 줄 끝에서 접혀 다음 줄 처음으로 이어지면 같은 마크
fn hyperlink_marks(screen: &Screen) -> Vec<HintMark> {
    let mut runs: Vec<(u32, Vec<LinkSpan>)> = Vec::new();
    for r in 0..screen.rows {
        let mut c = 0;
        while c < screen.cols {
            let Some(id) = screen.cell(c, r).link else {
                c += 1;
                continue;
            };
            let start = c;
            while c < screen.cols && screen.cell(c, r).link == Some(id) {
                c += 1;
            }
            let span = LinkSpan { row: r, start, end: c };
            let joins = start == 0
                && r > 0
                && runs.last().is_some_and(|(pid, sp)| {
                    *pid == id
                        && sp.last().is_some_and(|l| l.row == r - 1 && l.end == screen.cols)
                        && wraps(screen, r - 1)
                });
            match runs.last_mut() {
                Some((_, sp)) if joins => sp.push(span),
                _ => runs.push((id, vec![span])),
            }
        }
    }
    runs.into_iter()
        .filter_map(|(id, spans)| {
            let url = screen.links.get(id as usize).filter(|u| !u.is_empty())?.clone();
            let sig = spans_signature(screen, &spans);
            Some(HintMark {
                text: url,
                spans,
                path: None,
                line: None,
                sig,
            })
        })
        .collect()
}

/// kitty `encode_hint`: 알파벳으로 너비 우선 접두 트리를 편 `num`번째 라벨(0번이 첫 글자 한 글자)
pub fn encode_hint(num: usize, alphabet: &str) -> String {
    let a: Vec<char> = alphabet.chars().collect();
    if a.is_empty() {
        return String::new();
    }
    let mut out = Vec::new();
    let mut n = num as i64;
    while n >= 0 {
        out.push(a[(n % a.len() as i64) as usize]);
        n = n / a.len() as i64 - 1;
    }
    out.iter().rev().collect()
}

/// kitty `decode_hint`: 라벨의 번호. 알파벳 밖 글자가 있거나 비었으면 없다
pub fn decode_hint(hint: &str, alphabet: &str) -> Option<usize> {
    let a: Vec<char> = alphabet.chars().collect();
    if hint.is_empty() || a.is_empty() {
        return None;
    }
    let mut res = 0usize;
    for ch in hint.chars() {
        let i = a.iter().position(|&x| x == ch)?;
        res = res.checked_mul(a.len())?.checked_add(i + 1)?;
    }
    Some(res - 1)
}

/// 위에서 아래 차례인 맞은 곳 `count`개의 라벨. 맨 아래가 가장 작은 번호(`HINT_OFFSET`)다(kitty 기본의 내림차순)
pub fn labels_for(count: usize, alphabet: &str) -> Vec<String> {
    (0..count)
        .map(|i| encode_hint(count - 1 - i + HINT_OFFSET, alphabet))
        .collect()
}

/// 지금까지 친 글로 시작하는 라벨의 자리들
pub fn matching(labels: &[String], input: &str) -> Vec<usize> {
    labels
        .iter()
        .enumerate()
        .filter(|(_, l)| l.starts_with(input))
        .map(|(i, _)| i)
        .collect()
}

/// 셸에 글자 그대로 넘기는 따옴표 글: 작은따옴표로 싸고 안의 작은따옴표는 `'\''`로
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// `경로:줄`을 편집기로 여는 명령 글(Enter는 붙이지 않는다). 경로는 따옴표로 싸서 글자 그대로 남기고,
/// `~/`만 셸이 집 폴더로 풀게 따옴표 밖에 둔다. `-`로 시작하는 이름은 `--`로 옵션과 가른다
pub fn editor_command(path: &str, line: u32) -> String {
    let (home, rest) = match path.strip_prefix("~/") {
        Some(rest) => ("~/", rest),
        None => ("", path),
    };
    let sep = if home.is_empty() && rest.starts_with('-') {
        "-- "
    } else {
        ""
    };
    format!("$EDITOR +{line} {sep}{home}{}", shell_quote(rest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::{Cell, Cursor, CursorShape, Modes};
    use bt_core::Rgb;

    /// 글을 칸에 채운 화면. 한글·한자는 두 칸(`WIDE` + `SPACER`). `wrap`에 든 줄은 다음 줄로 접힌다
    fn screen(rows: &[&str], cols: usize, wrap: &[usize]) -> Screen {
        let blank = Cell::blank(Rgb::new(255, 255, 255), Rgb::new(0, 0, 0));
        let mut cells = vec![blank; cols * rows.len()];
        for (r, line) in rows.iter().enumerate() {
            let mut c = 0;
            for ch in line.chars() {
                let wide = matches!(ch as u32, 0x1100..=0x11FF | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3);
                cells[r * cols + c].ch = ch;
                if wide {
                    cells[r * cols + c].flags.insert(CellFlags::WIDE);
                    cells[r * cols + c + 1].flags.insert(CellFlags::SPACER);
                    c += 2;
                } else {
                    c += 1;
                }
            }
            if wrap.contains(&r) {
                cells[r * cols + cols - 1].flags.insert(CellFlags::WRAPLINE);
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
            command_marks: Vec::new(),
        }
    }

    fn texts(line: &str, kind: HintKind) -> Vec<String> {
        let s = screen(&[line], 200, &[]);
        find_marks(&s, kind).into_iter().map(|m| m.text).collect()
    }

    // ---- 라벨 차례(kitty `hints_generator.go`·`hints_generator_test.go`) ----

    #[test]
    fn encode_hint_follows_kitty_breadth_first_order() {
        // kitty `TestEncodeDecodeHint`의 값 그대로
        for (num, want) in [(0, "a"), (1, "b"), (2, "c"), (3, "aa"), (11, "cc"), (12, "aaa")] {
            assert_eq!(encode_hint(num, "abc"), want, "num={num}");
        }
        // 낱말 글자 0~9a~z, 36번째부터 두 글자
        assert_eq!(encode_hint(0, DEFAULT_ALPHABET), "0");
        assert_eq!(encode_hint(35, DEFAULT_ALPHABET), "z");
        assert_eq!(encode_hint(36, DEFAULT_ALPHABET), "00");
    }

    #[test]
    fn decode_hint_reverses_encode_hint() {
        for alphabet in ["abc", "0123456789", DEFAULT_ALPHABET] {
            for num in 0..200 {
                let hint = encode_hint(num, alphabet);
                assert_eq!(decode_hint(&hint, alphabet), Some(num), "{alphabet} {num}");
            }
        }
        assert_eq!(decode_hint("", DEFAULT_ALPHABET), None);
        assert_eq!(decode_hint("A", DEFAULT_ALPHABET), None, "알파벳 밖 글자");
    }

    #[test]
    fn labels_count_down_from_the_top_and_skip_zero() {
        // 기본(descending, offset 1): 맨 아래 맞은 곳이 "1", 위로 갈수록 큰 번호(kitty `find_marks`의 Index 계산)
        assert_eq!(labels_for(3, DEFAULT_ALPHABET), ["3", "2", "1"]);
        let l = labels_for(40, DEFAULT_ALPHABET);
        assert_eq!(l.len(), 40);
        assert_eq!(l[39], "1");
        assert_eq!(l[5], "z", "index 35");
        // 36번째부터 "00": offset 1이 한 글자 "0"을 건너뛰어 두 글자 라벨이 "0"으로 시작한다
        assert_eq!(l[4], "00");
        assert_eq!(l[0], "04");
        assert!(labels_for(0, DEFAULT_ALPHABET).is_empty());
    }

    #[test]
    fn typing_narrows_candidates() {
        let l = labels_for(40, DEFAULT_ALPHABET);
        assert_eq!(matching(&l, "").len(), 40);
        assert_eq!(matching(&l, "0"), vec![0, 1, 2, 3, 4], "00~04");
        assert_eq!(matching(&l, "z"), vec![5]);
        assert_eq!(matching(&l, "04"), vec![0]);
        assert!(matching(&l, "q9").is_empty());
    }

    // ---- 종류 이름과 동작 ----

    #[test]
    fn kinds_and_actions_parse_by_name() {
        for (n, k) in [
            ("url", HintKind::Url),
            ("path", HintKind::Path),
            ("linenum", HintKind::Linenum),
            ("line", HintKind::Line),
            ("hash", HintKind::Hash),
            ("word", HintKind::Word),
            ("hyperlink", HintKind::Hyperlink),
        ] {
            assert_eq!(HintKind::parse(n), Some(k));
            assert_eq!(k.name(), n);
        }
        assert_eq!(HintKind::parse("regex"), None, "kitty의 regex·ip는 두지 않는다");
        assert_eq!(HintKind::parse("URL"), None);
        for (n, a) in [
            ("open", HintAction::Open),
            ("copy", HintAction::Copy),
            ("paste", HintAction::Paste),
        ] {
            assert_eq!(HintAction::parse(n), Some(a));
            assert_eq!(a.name(), n);
        }
        assert_eq!(HintAction::parse("run"), None);
    }

    #[test]
    fn open_is_only_for_kinds_that_can_open() {
        for k in [HintKind::Url, HintKind::Path, HintKind::Linenum, HintKind::Hyperlink] {
            assert!(k.allows(HintAction::Open), "{k:?}");
            assert_eq!(k.default_action(), HintAction::Open);
        }
        for k in [HintKind::Line, HintKind::Hash, HintKind::Word] {
            assert!(!k.allows(HintAction::Open), "{k:?}");
            assert_eq!(k.default_action(), HintAction::Paste);
        }
        for k in HintKind::ALL {
            assert!(k.allows(HintAction::Copy) && k.allows(HintAction::Paste), "{k:?}");
        }
    }

    // ---- url ----

    #[test]
    fn url_kind_finds_scheme_addresses_only() {
        assert_eq!(
            texts(
                "see https://a.b/c?d=1 and /usr/bin/ls and ssh://h and mailto:x@y.z",
                HintKind::Url
            ),
            ["https://a.b/c?d=1", "ssh://h", "mailto:x@y.z"]
        );
        // kitty `TestHintMarking`의 url 표: 따옴표·괄호·꺾쇠, 끝 문장부호
        for (input, want) in [
            ("\"https://example.com/x#frag\"", "https://example.com/x#frag"),
            ("(https://example.com/x)", "https://example.com/x"),
            ("<https://example.com/x>", "https://example.com/x"),
            ("https://example.com,", "https://example.com"),
            ("https://example.com.", "https://example.com"),
            ("https://example.com!", "https://example.com"),
            ("https://example.com?", "https://example.com"),
            ("“https://example.com/x”", "https://example.com/x"),
        ] {
            assert_eq!(texts(input, HintKind::Url), [want], "{input}");
        }
    }

    #[test]
    fn url_marks_follow_soft_wrapped_rows_and_wide_cells() {
        // 10칸에서 접힌 주소는 마크 하나, 구간은 줄마다
        let s = screen(&["go https:/", "/a.b/cdefg", "h done"], 10, &[0, 1]);
        let m = find_marks(&s, HintKind::Url);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].text, "https://a.b/cdefgh");
        assert_eq!(
            m[0].spans,
            vec![
                LinkSpan {
                    row: 0,
                    start: 3,
                    end: 10
                },
                LinkSpan {
                    row: 1,
                    start: 0,
                    end: 10
                },
                LinkSpan {
                    row: 2,
                    start: 0,
                    end: 1
                },
            ]
        );
        // 앞의 한글은 두 칸이라 구간 시작 칸이 글자 수가 아니라 칸 수를 따른다
        let s = screen(&["링크 https://a.b/c 끝"], 40, &[]);
        let m = find_marks(&s, HintKind::Url);
        assert_eq!(
            m[0].spans,
            vec![LinkSpan {
                row: 0,
                start: 5,
                end: 5 + 13
            }]
        );
        // 뒤에 붙은 조사는 주소가 아니다(ADR-086)
        let s = screen(&["https://github.com/x/b-terminal에 올렸다"], 60, &[]);
        assert_eq!(find_marks(&s, HintKind::Url)[0].text, "https://github.com/x/b-terminal");
    }

    #[test]
    fn marks_are_in_reading_order_and_ignore_short_text() {
        let s = screen(
            &["https://a.example/1", "x https://b.example/2 https://c.example/3"],
            60,
            &[],
        );
        let t: Vec<String> = find_marks(&s, HintKind::Url).into_iter().map(|m| m.text).collect();
        assert_eq!(t, ["https://a.example/1", "https://b.example/2", "https://c.example/3"]);
        assert!(texts("ab  c", HintKind::Word).is_empty(), "세 글자 미만은 건너뛴다");
    }

    // ---- path ----

    #[test]
    fn path_kind_matches_kitty_table() {
        // kitty `TestHintMarking`의 `opts.Type = "path"` 표
        for (input, want) in [
            ("file.c", "file.c"),
            ("file.c.", "file.c"),
            ("file.epub.", "file.epub"),
            ("(file.epub)", "file.epub"),
            ("some/path", "some/path"),
            ("“some/path”", "some/path"),
            ("Saving to: ‘index.html’", "index.html"),
        ] {
            assert_eq!(texts(input, HintKind::Path), [want], "{input}");
        }
        // 확장자도 슬래시도 없는 낱말은 경로가 아니다
        assert!(texts("hello world", HintKind::Path).is_empty());
        // 스킴 주소는 경로 종류에 들지 않는다(주소는 url)
        assert!(texts("https://a.b/c.html", HintKind::Path).is_empty());
    }

    #[test]
    fn path_kind_drops_line_and_column_suffix() {
        assert_eq!(texts("at /tmp/a/b.rs:12:5 now", HintKind::Path), ["/tmp/a/b.rs"]);
        assert_eq!(texts("src/main.rs:7", HintKind::Path), ["src/main.rs"]);
        let s = screen(&["x /tmp/a/b.rs:12:5"], 40, &[]);
        assert_eq!(
            find_marks(&s, HintKind::Path)[0].spans,
            vec![LinkSpan {
                row: 0,
                start: 2,
                end: 13
            }]
        );
    }

    #[test]
    fn path_kind_keeps_dotted_names_and_paths_with_slash_for_open() {
        assert_eq!(
            texts("./a.sh  ../b/c  ~/d.txt  $HOME/e.md  f/g", HintKind::Path),
            ["./a.sh", "../b/c", "~/d.txt", "$HOME/e.md", "f/g"]
        );
    }

    // ---- linenum ----

    #[test]
    fn linenum_kind_matches_kitty_table() {
        // kitty `TestHintMarking`의 `opts.Type = "linenum"` 표: 열(칸)은 줄 번호가 아니다
        for (input, path, line) in [
            ("file.c:23", "file.c", 23),
            ("file.c:23:32", "file.c", 23),
            ("file.cpp:23:1", "file.cpp", 23),
            ("a/file.c:23", "a/file.c", 23),
            ("a/file.c:23:32", "a/file.c", 23),
            ("~/file.c:23:32", "~/file.c", 23),
        ] {
            let s = screen(&[input], 80, &[]);
            let m = find_marks(&s, HintKind::Linenum);
            assert_eq!(m.len(), 1, "{input}");
            assert_eq!(m[0].path.as_deref(), Some(path), "{input}");
            assert_eq!(m[0].line, Some(line), "{input}");
        }
        // 줄 번호가 없는 경로, 시각, 숫자만은 해당 없다
        assert!(texts("file.c a/b.rs 12:30:45 foo:12", HintKind::Linenum).is_empty());
        let s = screen(&["error: src/lib.rs:9:1: boom"], 80, &[]);
        let m = find_marks(&s, HintKind::Linenum);
        assert_eq!((m[0].path.as_deref(), m[0].line), (Some("src/lib.rs"), Some(9)));
    }

    // ---- hash ----

    #[test]
    fn hash_kind_takes_seven_to_128_hex_digits() {
        assert_eq!(
            texts(
                "commit 3f2a9c1d4e5b6 fixes abc12 and DEADBEEF1234 and 0123456",
                HintKind::Hash
            ),
            ["3f2a9c1d4e5b6", "0123456"]
        );
        // 128자를 넘는 덩어리는 128자씩 끊고 7자 미만 나머지는 버린다(kitty 정규식 `{6,127}`)
        let long = "a".repeat(130);
        assert_eq!(texts(&long, HintKind::Hash), ["a".repeat(128)]);
        let long = "b".repeat(135);
        assert_eq!(texts(&long, HintKind::Hash), ["b".repeat(128), "b".repeat(7)]);
    }

    // ---- word ----

    #[test]
    fn word_kind_uses_alnum_and_kitty_word_characters() {
        // kitty `select_by_word_characters = @-./_~?&=%+#`, 최소 3자, 앞뒤 괄호·따옴표는 벗긴다
        assert_eq!(
            texts("fōtiz час (foo-bar) 'quoted' ab a.b_c", HintKind::Word),
            ["fōtiz", "час", "foo-bar", "quoted", "a.b_c"]
        );
        // 한글도 글자다
        assert_eq!(texts("안녕하세요 세계는", HintKind::Word), ["안녕하세요", "세계는"]);
        // 글자·숫자 없이 허용 글자만 있는 덩어리는 낱말이 아니다(kitty는 마크로 만든다)
        assert!(texts("--- ... ///", HintKind::Word).is_empty());
    }

    #[test]
    fn word_marks_use_cell_columns_for_wide_text() {
        let s = screen(&["a 안녕 b"], 20, &[]);
        let m = find_marks(&s, HintKind::Word);
        assert!(m.is_empty(), "'안녕'은 두 글자라 세 글자 미만");
        let s = screen(&["a 안녕하 b"], 20, &[]);
        let m = find_marks(&s, HintKind::Word);
        assert_eq!(
            (m[0].text.as_str(), m[0].spans.clone()),
            (
                "안녕하",
                vec![LinkSpan {
                    row: 0,
                    start: 2,
                    end: 8
                }]
            )
        );
    }

    // ---- line ----

    #[test]
    fn line_kind_marks_trimmed_logical_lines() {
        let s = screen(&["  first line  ", "", "ab", "   third   "], 30, &[]);
        let m = find_marks(&s, HintKind::Line);
        let t: Vec<&str> = m.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(t, ["first line", "third"]);
        assert_eq!(
            m[0].spans,
            vec![LinkSpan {
                row: 0,
                start: 2,
                end: 12
            }]
        );
        // 접힌 줄은 한 줄이다
        let s = screen(&["abcdefghij", "klm"], 10, &[0]);
        let m = find_marks(&s, HintKind::Line);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].text, "abcdefghijklm");
        assert_eq!(m[0].spans.len(), 2);
    }

    // ---- hyperlink ----

    fn with_link(mut s: Screen, id: u32, url: &str, row: usize, from: usize, to: usize) -> Screen {
        while s.links.len() <= id as usize {
            s.links.push(String::new());
        }
        s.links[id as usize] = url.into();
        for c in from..to {
            s.cells[row * s.cols + c].link = Some(id);
        }
        s
    }

    #[test]
    fn hyperlink_kind_marks_osc8_runs_by_address() {
        let s = screen(&["click here or there"], 30, &[]);
        let s = with_link(s, 0, "https://a.example/x", 0, 6, 10);
        let s = with_link(s, 1, "file:///tmp/b", 0, 14, 19);
        let m = find_marks(&s, HintKind::Hyperlink);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].text, "https://a.example/x");
        assert_eq!(
            m[0].spans,
            vec![LinkSpan {
                row: 0,
                start: 6,
                end: 10
            }]
        );
        assert_eq!(m[1].text, "file:///tmp/b");
    }

    #[test]
    fn hyperlink_run_that_wraps_is_one_mark() {
        let s = screen(&["0123456789", "abcdef"], 10, &[0]);
        let s = with_link(s, 0, "https://w.example/", 0, 6, 10);
        let s = with_link(s, 0, "https://w.example/", 1, 0, 3);
        let m = find_marks(&s, HintKind::Hyperlink);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].spans.len(), 2);
        // 접히지 않은 두 줄에 같은 주소가 있으면 둘이다
        let s = screen(&["0123456789", "abcdef"], 10, &[]);
        let s = with_link(s, 0, "https://w.example/", 0, 6, 10);
        let s = with_link(s, 0, "https://w.example/", 1, 0, 3);
        assert_eq!(find_marks(&s, HintKind::Hyperlink).len(), 2);
    }

    // ---- 상한과 서명 ----

    #[test]
    fn marks_are_capped_keeping_the_bottom_ones() {
        let rows: Vec<String> = (0..60).map(|r| format!("r{r:02} {}", "aaa ".repeat(40))).collect();
        let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
        let s = screen(&refs, 200, &[]);
        let m = find_marks(&s, HintKind::Word);
        assert_eq!(m.len(), MAX_MARKS);
        assert_eq!(m.last().unwrap().spans[0].row, 59, "맨 아래 줄은 남긴다");
    }

    #[test]
    fn signature_changes_when_text_under_the_mark_changes() {
        let a = screen(&["see https://a.b/c now"], 40, &[]);
        let m = find_marks(&a, HintKind::Url).remove(0);
        assert_eq!(spans_signature(&a, &m.spans), m.sig);
        let same = screen(&["xxx https://a.b/c yyy"], 40, &[]);
        assert_eq!(spans_signature(&same, &m.spans), m.sig, "표 밖이 바뀌는 것은 상관없다");
        let moved = screen(&["see https://a.b/d now"], 40, &[]);
        assert_ne!(spans_signature(&moved, &m.spans), m.sig);
        let scrolled = screen(&["", "see https://a.b/c now"], 40, &[]);
        assert_ne!(spans_signature(&scrolled, &m.spans), m.sig, "한 줄 밀리면 달라진다");
    }

    // ---- 편집기 명령(linenum) ----

    #[test]
    fn editor_command_quotes_every_path_byte() {
        assert_eq!(editor_command("src/main.rs", 12), "$EDITOR +12 'src/main.rs'");
        assert_eq!(editor_command("a b/c.rs", 3), "$EDITOR +3 'a b/c.rs'");
        // 따옴표·치환·세미콜론은 문자 그대로 남는다
        assert_eq!(editor_command("it's$(id);x.rs", 1), "$EDITOR +1 'it'\\''s$(id);x.rs'");
        // `~/`는 셸이 풀게 따옴표 밖에 둔다
        assert_eq!(editor_command("~/a b.rs", 9), "$EDITOR +9 ~/'a b.rs'");
        // `-`로 시작하는 이름은 옵션으로 읽히지 않게 `--`를 앞세운다
        assert_eq!(editor_command("-x.rs", 2), "$EDITOR +2 -- '-x.rs'");
    }
}
