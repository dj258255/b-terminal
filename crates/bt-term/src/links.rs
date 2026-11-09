//! 화면의 링크(#38): 글에서 찾은 URL과 프로그램이 OSC 8로 단 링크.
//!
//! 터미널에 찍힌 글은 믿을 수 없다. 그래서 여는 것은 사용자가 Cmd-클릭했을 때만이고(앱),
//! 여기서는 열어도 되는 링크인지(`classify`)를 정한다: http·https는 브라우저로, file은 이 컴퓨터의
//! 실제 경로일 때만, 실행될 수 있는 파일(앱·스크립트)은 열지 않고 Finder에서 보이기만 한다

use std::path::{Path, PathBuf};

use crate::screen::{CellFlags, Screen};

/// 링크가 차지하는 한 줄 구간: `row` 줄의 `start..end` 칸
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkSpan {
    pub row: usize,
    pub start: usize,
    pub end: usize,
}

/// 마우스 아래의 링크
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkHit {
    pub url: String,
    /// 밑줄을 그을 칸들. 접힌 줄에 걸치면 여러 줄
    pub spans: Vec<LinkSpan>,
    /// 프로그램이 OSC 8로 단 링크(보이는 글과 주소가 다를 수 있다)
    pub explicit: bool,
}

const SCHEMES: [&str; 3] = ["https://", "http://", "file://"];

/// 한글·한자·전각 글자. URL 바로 뒤에 조사가 붙는 한국어 글("…/b-terminal에 올렸다")에서 URL을 끊는다
fn is_wide_script(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x1F000..=0x1FAFF)
}

/// URL 안에 올 수 있는 글자
fn url_char(c: char) -> bool {
    !c.is_whitespace()
        && !c.is_control()
        && !matches!(c, '<' | '>' | '"' | '`' | '{' | '}' | '|' | '\\' | '^')
        // 상자 선·블록 글자(TUI 테두리 │ 등)
        && !('\u{2500}'..='\u{259F}').contains(&c)
        && !is_wide_script(c)
}

fn starts_with_ci(chars: &[char], at: usize, prefix: &str) -> bool {
    let p: Vec<char> = prefix.chars().collect();
    chars.len() >= at + p.len()
        && chars[at..at + p.len()]
            .iter()
            .zip(&p)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
}

/// 글자 배열에서 URL 구간들(글자 자리 `start..end`)
pub fn find_urls(chars: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let scheme = SCHEMES.iter().find(|s| starts_with_ci(chars, i, s));
        // 낱말 가운데에서 시작하는 것은 URL이 아니다(xhttps://…)
        let boundary = i == 0 || !chars[i - 1].is_alphanumeric();
        let Some(scheme) = scheme.filter(|_| boundary) else {
            i += 1;
            continue;
        };
        let body = i + scheme.len();
        let mut j = body;
        while j < chars.len() && url_char(chars[j]) {
            j += 1;
        }
        // 끝의 문장 부호와 짝이 없는 닫는 괄호는 URL이 아니다("(https://a.b/c)."에서 ")." 빼기)
        while j > body {
            let last = chars[j - 1];
            let unbalanced = |open: char, close: char| {
                last == close && {
                    let s = &chars[i..j];
                    s.iter().filter(|&&c| c == close).count() > s.iter().filter(|&&c| c == open).count()
                }
            };
            if matches!(last, '.' | ',' | ';' | ':' | '!' | '?' | '\'' | '*')
                || unbalanced('(', ')')
                || unbalanced('[', ']')
            {
                j -= 1;
            } else {
                break;
            }
        }
        if j > body {
            out.push((i, j));
        }
        i = j.max(i + 1);
    }
    out
}

/// 마지막 칸이 줄 넘김(soft wrap)으로 다음 줄에 이어지는가
fn wraps(screen: &Screen, row: usize) -> bool {
    screen.cols > 0 && screen.cell(screen.cols - 1, row).flags.contains(CellFlags::WRAPLINE)
}

/// 어떤 링크를 찾을지(`link-url`·`link-osc8`, Ghostty와 같은 뜻). 기본은 둘 다 켬
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkOptions {
    /// 화면 글의 URL 자동 인식
    pub url: bool,
    /// 프로그램이 OSC 8로 단 링크
    pub osc8: bool,
}

impl Default for LinkOptions {
    fn default() -> Self {
        LinkOptions { url: true, osc8: true }
    }
}

/// (col, row) 칸의 링크. OSC 8 링크가 먼저, 없으면 접힌 줄을 이어 붙인 글에서 URL을 찾는다
pub fn link_at(screen: &Screen, col: usize, row: usize) -> Option<LinkHit> {
    link_at_with(screen, col, row, LinkOptions::default())
}

/// `link_at`에 찾을 종류를 정해 준다. 끈 종류는 없는 것처럼 지나간다(OSC 8을 끄면 그 칸은 보통 글)
pub fn link_at_with(screen: &Screen, col: usize, row: usize, opts: LinkOptions) -> Option<LinkHit> {
    if col >= screen.cols || row >= screen.rows {
        return None;
    }
    // 넓은 글자의 뒤 칸이면 앞 칸
    let col = if screen.cell(col, row).flags.contains(CellFlags::SPACER) && col > 0 {
        col - 1
    } else {
        col
    };
    let osc8_id = screen.cell(col, row).link.filter(|_| opts.osc8);
    if let Some(id) = osc8_id {
        let url = screen.links.get(id as usize)?.clone();
        let mut spans = Vec::new();
        for r in 0..screen.rows {
            let mut c = 0;
            while c < screen.cols {
                if screen.cell(c, r).link == Some(id) {
                    let start = c;
                    while c < screen.cols && screen.cell(c, r).link == Some(id) {
                        c += 1;
                    }
                    spans.push(LinkSpan { row: r, start, end: c });
                } else {
                    c += 1;
                }
            }
        }
        return Some(LinkHit {
            url,
            spans,
            explicit: true,
        });
    }
    if !opts.url {
        return None;
    }
    // 접힌 줄 묶음(논리 줄)의 처음과 끝
    let mut top = row;
    while top > 0 && wraps(screen, top - 1) {
        top -= 1;
    }
    let mut bottom = row;
    while bottom + 1 < screen.rows && wraps(screen, bottom) {
        bottom += 1;
    }
    // 글자마다 (줄, 칸, 폭)
    let mut chars = Vec::new();
    let mut pos = Vec::new();
    for r in top..=bottom {
        for c in 0..screen.cols {
            let cell = screen.cell(c, r);
            if cell.flags.contains(CellFlags::SPACER) {
                continue;
            }
            chars.push(cell.ch);
            pos.push((r, c, if cell.flags.contains(CellFlags::WIDE) { 2 } else { 1 }));
        }
    }
    let here = pos.iter().position(|&(r, c, _)| r == row && c == col)?;
    let (start, end) = find_urls(&chars).into_iter().find(|&(s, e)| s <= here && here < e)?;
    let url: String = chars[start..end].iter().collect();
    let mut spans: Vec<LinkSpan> = Vec::new();
    for &(r, c, w) in &pos[start..end] {
        match spans.last_mut() {
            Some(s) if s.row == r => s.end = c + w,
            _ => spans.push(LinkSpan {
                row: r,
                start: c,
                end: c + w,
            }),
        }
    }
    Some(LinkHit {
        url,
        spans,
        explicit: false,
    })
}

/// 링크를 어떻게 열지
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpenTarget {
    /// 브라우저로(http·https)
    Web(String),
    /// 기본 앱으로 연다(이 컴퓨터의 폴더·알려진 문서)
    Open(PathBuf),
    /// 열지 않고 Finder에서 보이기만 한다(그 밖의 모든 파일, 앱 묶음)
    Reveal(PathBuf),
}

/// 기본 앱으로 열어도 되는 문서 확장자(허용 목록). 여기 없는 것은 모두 Finder에서 보이기만 한다.
/// 기본 앱이 실행기일 수 있는 것(.py, .sh, .js, .html 등)은 넣지 않는다
const SAFE_DOCS: &[&str] = &[
    "txt", "md", "markdown", "log", "csv", "tsv", "json", "toml", "yaml", "yml", "pdf", "png", "jpg", "jpeg", "gif",
    "webp", "heic", "bmp", "tif", "tiff", "mp3", "m4a", "wav", "flac", "mp4", "mov", "m4v", "rs", "go", "c", "h", "cc",
    "cpp", "hpp", "java", "kt", "swift", "ts", "tsx", "css", "diff", "patch",
];

/// 보이지 않는 서식 문자(방향 바꾸기, 폭 없는 공백 등). 화면에 보이는 주소와 실제 주소를 다르게 만든다
fn is_invisible_format(c: char) -> bool {
    matches!(c as u32,
        0x00AD | 0x061C | 0x180E | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2069 | 0xFEFF)
}

/// 폴더가 묶음(앱처럼 열면 실행되는 폴더)인가: 확장자가 있거나 `Contents/Info.plist`가 있으면 그렇게 본다
fn is_bundle(dir: &Path) -> bool {
    dir.extension().is_some() || dir.join("Contents").join("Info.plist").exists()
}

/// 이 파일을 기본 앱으로 열어도 되는가. 실제 경로(심볼릭 링크를 푼)를 받는다
fn safe_to_open(path: &Path) -> bool {
    if path.is_dir() {
        return !is_bundle(path);
    }
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 != 0 {
            return false;
        }
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    SAFE_DOCS.contains(&ext.as_str())
}

/// `%20` 같은 퍼센트 인코딩을 푼다. 잘못된 인코딩이나 UTF-8이 아니면 없다
fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = std::str::from_utf8(b.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// 웹 주소의 권한 부분(`host:port`). 사용자 정보(`@`)나 비ASCII가 있으면 없다
fn web_authority(rest: &str) -> Option<&str> {
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let ok = !authority.is_empty()
        && authority.is_ascii()
        && authority
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'));
    ok.then_some(authority)
}

/// 열어도 되는 링크인가, 어떻게 열지. `local_host`는 file URL의 호스트가 이 컴퓨터인지 본다
pub fn classify(url: &str, local_host: &dyn Fn(&str) -> bool) -> Option<OpenTarget> {
    // 제어 문자·공백·보이지 않는 서식 문자가 든 주소는 화면에 보이는 것과 다르게 열릴 수 있다
    if url.len() > 8192
        || url
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || is_invisible_format(c))
    {
        return None;
    }
    let lower = url.to_ascii_lowercase();
    for scheme in ["https://", "http://"] {
        if lower.starts_with(scheme) {
            // 호스트는 ASCII 이름·주소만, 사용자 정보(`github.com@evil.example`)는 받지 않는다
            web_authority(&url[scheme.len()..])?;
            return Some(OpenTarget::Web(url.to_string()));
        }
    }
    if lower.starts_with("file://") {
        let rest = &url["file://".len()..];
        let slash = rest.find('/')?;
        let (host, path) = rest.split_at(slash);
        if !(host.is_empty() || host.eq_ignore_ascii_case("localhost") || local_host(host)) {
            return None;
        }
        // 물음표·# 뒤는 경로가 아니다
        let path = path.split(['?', '#']).next().unwrap_or(path);
        let path = PathBuf::from(percent_decode(path)?);
        if !path.is_absolute() {
            return None;
        }
        // 심볼릭 링크를 풀어 실제 대상을 보고, 연다면 그 경로를 연다
        let real = path.canonicalize().ok()?;
        return Some(if safe_to_open(&real) {
            OpenTarget::Open(real)
        } else {
            OpenTarget::Reveal(real)
        });
    }
    None
}

/// 주소 띠에 보일 글: 스킴과 호스트는 늘 다 보이고, 넘치면 그 뒤 가운데를 `…`로 줄인다
pub fn hint_text(url: &str, max: usize) -> String {
    let chars: Vec<char> = url.chars().collect();
    if chars.len() <= max {
        return url.to_string();
    }
    // 스킴(`://`까지)과 호스트 뒤 첫 `/`까지
    let head_end = url
        .find("://")
        .map(|i| {
            let after = i + 3;
            url[after..].find('/').map(|j| after + j + 1).unwrap_or(url.len())
        })
        .unwrap_or(0);
    let head: Vec<char> = url[..head_end].chars().collect();
    let room = max.saturating_sub(head.len() + 1);
    if room == 0 {
        return head.iter().chain(['…'].iter()).collect();
    }
    let tail = &chars[chars.len() - room.min(chars.len() - head.len())..];
    head.iter().chain(['…'].iter()).chain(tail.iter()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::{Cell, Cursor, CursorShape, Modes};
    use bt_core::Rgb;

    fn urls(s: &str) -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        find_urls(&chars)
            .into_iter()
            .map(|(a, b)| chars[a..b].iter().collect())
            .collect()
    }

    #[test]
    fn finds_urls_and_trims_trailing_punctuation() {
        assert_eq!(urls("see https://a.b/c?d=1&e=2#f now"), vec!["https://a.b/c?d=1&e=2#f"]);
        assert_eq!(
            urls("(https://en.wikipedia.org/wiki/Rust_(language))."),
            vec!["https://en.wikipedia.org/wiki/Rust_(language)"]
        );
        assert_eq!(
            urls("링크: https://github.com/dj258255/b-terminal에 올렸다"),
            vec!["https://github.com/dj258255/b-terminal"]
        );
        assert_eq!(
            urls("<http://x.y/z>, file:///tmp/a%20b"),
            vec!["http://x.y/z", "file:///tmp/a%20b"]
        );
        assert_eq!(urls("│ https://box.example/x │"), vec!["https://box.example/x"]);
        assert!(urls("xhttps://no.pe https:// mailto:a@b").is_empty());
    }

    /// 글을 칸에 채운 화면. `wrap`에 든 줄은 다음 줄로 접힌다
    fn screen(rows: &[&str], cols: usize, wrap: &[usize]) -> Screen {
        let blank = Cell::blank(Rgb::new(255, 255, 255), Rgb::new(0, 0, 0));
        let mut cells = vec![blank; cols * rows.len()];
        for (r, line) in rows.iter().enumerate() {
            let mut c = 0;
            for ch in line.chars() {
                let wide = is_wide_script(ch);
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
            history: 0,
            background: Rgb::new(0, 0, 0),
            links: Vec::new(),
            images: Vec::new(),
        }
    }

    #[test]
    fn link_follows_soft_wrapped_lines() {
        // 10칸 화면에 접힌 URL(Claude Code 로그인 주소처럼 긴 것)
        let s = screen(&["go https:/", "/a.b/cdefg", "h done"], 10, &[0, 1]);
        let hit = link_at(&s, 4, 1).expect("둘째 줄에서도 찾는다");
        assert_eq!(hit.url, "https://a.b/cdefgh");
        assert_eq!(
            hit.spans,
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
        assert!(!hit.explicit);
        assert_eq!(link_at(&s, 3, 2), None, "URL 밖");
        // 접히지 않은 줄은 잇지 않는다
        let s = screen(&["go https:/", "/a.b/cdefg"], 10, &[]);
        assert_eq!(link_at(&s, 4, 1), None);
    }

    #[test]
    fn link_spans_count_wide_cells() {
        let s = screen(&["한 https://k.r 끝"], 20, &[]);
        let hit = link_at(&s, 5, 0).unwrap();
        assert_eq!(hit.url, "https://k.r");
        // "한"이 두 칸, 공백 한 칸 → URL은 3번 칸부터
        assert_eq!(
            hit.spans,
            vec![LinkSpan {
                row: 0,
                start: 3,
                end: 14
            }]
        );
    }

    #[test]
    fn link_options_turn_each_kind_off() {
        let mut s = screen(&["click here or https://shown.example"], 40, &[]);
        s.links = vec!["https://real.example/".into()];
        for c in 6..10 {
            s.cells[c].link = Some(0);
        }
        let all = LinkOptions::default();
        assert!(link_at_with(&s, 7, 0, all).unwrap().explicit);
        assert!(!link_at_with(&s, 20, 0, all).unwrap().explicit);
        let no_url = LinkOptions { url: false, ..all };
        assert!(
            link_at_with(&s, 7, 0, no_url).unwrap().explicit,
            "link-url=false여도 OSC 8은 그대로"
        );
        assert_eq!(link_at_with(&s, 20, 0, no_url), None);
        let no_osc8 = LinkOptions { osc8: false, ..all };
        assert_eq!(
            link_at_with(&s, 7, 0, no_osc8),
            None,
            "link-osc8=false면 OSC 8 칸은 보통 글"
        );
        assert!(
            !link_at_with(&s, 20, 0, no_osc8).unwrap().explicit,
            "화면 글의 URL은 그대로"
        );
        let none = LinkOptions {
            url: false,
            osc8: false,
        };
        assert_eq!(link_at_with(&s, 7, 0, none), None);
        assert_eq!(link_at_with(&s, 20, 0, none), None);
    }

    #[test]
    fn osc8_off_still_finds_url_text_under_the_link_cells() {
        // OSC 8 링크의 글이 그대로 URL이면, 끈 뒤에는 글에서 찾은 링크로 다룬다(주소는 보이는 글 그대로)
        let mut s = screen(&["https://shown.example"], 30, &[]);
        s.links = vec!["https://real.example/".into()];
        for c in 0..21 {
            s.cells[c].link = Some(0);
        }
        let hit = link_at_with(&s, 3, 0, LinkOptions { url: true, osc8: false }).unwrap();
        assert_eq!(
            hit.url, "https://shown.example",
            "숨은 주소(real.example)가 아니라 보이는 글"
        );
        assert!(!hit.explicit);
    }

    #[test]
    fn osc8_link_wins_and_spans_all_its_cells() {
        let mut s = screen(&["click here or https://shown.example"], 40, &[]);
        s.links = vec!["https://real.example/".into()];
        for c in 6..10 {
            s.cells[c].link = Some(0);
        }
        let hit = link_at(&s, 7, 0).unwrap();
        assert_eq!(hit.url, "https://real.example/");
        assert!(hit.explicit);
        assert_eq!(
            hit.spans,
            vec![LinkSpan {
                row: 0,
                start: 6,
                end: 10
            }]
        );
    }

    /// VT 엔진을 거친 실제 바이트: OSC 8 링크와 줄 끝에서 접힌 긴 URL
    #[test]
    fn engine_bytes_produce_links_and_wraps() {
        use crate::session::{Emulator, GridSize};
        let mut e = Emulator::new(GridSize::new(20, 4), bt_core::Theme::ghostty_default());
        e.feed(b"\x1b]8;;https://real.example/\x1b\\click\x1b]8;;\x1b\\ x\r\n");
        e.feed(b"see https://example.com/very/long/path ok");
        let s = e.snapshot();
        let hit = link_at(&s, 2, 0).expect("OSC 8");
        assert_eq!(hit.url, "https://real.example/");
        assert_eq!(
            hit.spans,
            vec![LinkSpan {
                row: 0,
                start: 0,
                end: 5
            }]
        );
        assert_eq!(link_at(&s, 6, 0), None, "링크 뒤 글자는 링크가 아니다");
        // 20칸에서 접힌 URL을 둘째 줄 어디서 짚어도 같은 주소
        let a = link_at(&s, 10, 1).expect("첫 줄 부분");
        let b = link_at(&s, 3, 2).expect("접힌 뒤 부분");
        assert_eq!(a.url, "https://example.com/very/long/path");
        assert_eq!(a, b);
    }

    /// 보안 검토 #54: 보이는 호스트와 가는 호스트가 다를 수 있는 주소는 열지 않는다
    #[test]
    fn classify_rejects_spoofable_web_urls() {
        let no = |_: &str| false;
        assert_eq!(classify("https://github.com@evil.example/x", &no), None, "사용자 정보");
        assert_eq!(classify("https://user:pw@host.example/", &no), None);
        assert_eq!(
            classify("https://good.example/\u{202e}txt.exe", &no),
            None,
            "방향 제어 문자"
        );
        assert_eq!(classify("https://a\u{200b}b.example/", &no), None, "폭 없는 공백");
        assert_eq!(
            classify("https://ex\u{0430}mple.com/", &no),
            None,
            "비ASCII 호스트(키릴 а)"
        );
        // 경로·쿼리의 @와 비ASCII는 괜찮다
        assert!(classify("https://host.example/@user/한글?q=a@b", &no).is_some());
        assert!(classify("https://host.example:8080/x", &no).is_some());
    }

    /// 보안 검토 #54: file 링크는 실제 경로로 풀고, 알려진 문서·폴더만 연다
    #[test]
    fn classify_file_links_use_an_allowlist_and_resolve_symlinks() {
        let no = |_: &str| false;
        let dir = std::env::temp_dir().join(format!("bt-links-sec-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let url = |p: &Path| format!("file://{}", p.display());
        // 목록에 없는 확장자는 실행 권한이 없어도 보이기만
        let odd = dir.join("thing.weirdext");
        std::fs::write(&odd, "x").unwrap();
        assert_eq!(classify(&url(&odd), &no), Some(OpenTarget::Reveal(odd.clone())));
        let none = dir.join("README");
        std::fs::write(&none, "x").unwrap();
        assert_eq!(
            classify(&url(&none), &no),
            Some(OpenTarget::Reveal(none.clone())),
            "확장자 없음"
        );
        // 문서처럼 보이는 이름의 심볼릭 링크가 스크립트를 가리키면 실제 대상을 보이기만
        let script = dir.join("payload.command");
        std::fs::write(&script, "echo").unwrap();
        let disguised = dir.join("notes.txt");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&script, &disguised).unwrap();
            assert_eq!(
                classify(&url(&disguised), &no),
                Some(OpenTarget::Reveal(script.clone()))
            );
        }
        // 묶음 폴더(.app 같은 것, Contents/Info.plist가 있는 폴더)는 보이기만
        let bundle = dir.join("Tool.app");
        std::fs::create_dir_all(bundle.join("Contents")).unwrap();
        std::fs::write(bundle.join("Contents/Info.plist"), "<plist/>").unwrap();
        assert_eq!(classify(&url(&bundle), &no), Some(OpenTarget::Reveal(bundle.clone())));
        let sneaky = dir.join("plainname");
        std::fs::create_dir_all(sneaky.join("Contents")).unwrap();
        std::fs::write(sneaky.join("Contents/Info.plist"), "<plist/>").unwrap();
        assert_eq!(classify(&url(&sneaky), &no), Some(OpenTarget::Reveal(sneaky.clone())));
        // 알려진 문서는 연다
        let pdf = dir.join("a.pdf");
        std::fs::write(&pdf, "%PDF").unwrap();
        assert_eq!(classify(&url(&pdf), &no), Some(OpenTarget::Open(pdf.clone())));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// 보안 검토 #54: 주소 띠는 호스트까지 늘 다 보이고 가운데를 줄인다
    #[test]
    fn hint_keeps_the_whole_host() {
        assert_eq!(hint_text("https://a.example/x", 40), "https://a.example/x");
        let long = format!("https://docs.example.com/{}/end", "p".repeat(80));
        let h = hint_text(&long, 40);
        assert!(h.starts_with("https://docs.example.com/"), "{h}");
        assert!(h.ends_with("/end"), "{h}");
        assert!(h.contains('…'));
        assert!(h.chars().count() <= 40, "{h}");
        // 호스트가 칸보다 길어도 호스트는 자르지 않는다
        let host = format!("https://{}.example/x", "h".repeat(60));
        assert!(hint_text(&host, 20).starts_with(&format!("https://{}.example", "h".repeat(60))));
    }

    #[test]
    fn classify_allows_only_web_and_local_files() {
        let no = |_: &str| false;
        assert_eq!(
            classify("https://a.b/c", &no),
            Some(OpenTarget::Web("https://a.b/c".into()))
        );
        assert_eq!(classify("HTTP://A.B", &no), Some(OpenTarget::Web("HTTP://A.B".into())));
        assert_eq!(classify("https://", &no), None);
        assert_eq!(classify("javascript:alert(1)", &no), None);
        assert_eq!(classify("ssh://host", &no), None);
        assert_eq!(classify("https://a.b/\u{1b}[2J", &no), None, "제어 문자");
        assert_eq!(classify("https://a.b/ x", &no), None, "공백");

        let dir = std::env::temp_dir().join(format!("bt-links-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // macOS의 /var는 /private/var의 링크다. 판정은 실제 경로를 돌려준다
        let dir = dir.canonicalize().unwrap();
        let doc = dir.join("메모 1.txt");
        std::fs::write(&doc, "hi").unwrap();
        let script = dir.join("run.command");
        std::fs::write(&script, "echo").unwrap();
        let enc = |p: &Path| p.to_str().unwrap().replace(' ', "%20");
        assert_eq!(
            classify(&format!("file://{}", enc(&doc)), &no),
            Some(OpenTarget::Open(doc.clone())),
            "퍼센트 인코딩을 푼 실제 경로"
        );
        assert_eq!(
            classify(&format!("file://{}", enc(&script)), &no),
            Some(OpenTarget::Reveal(script.clone())),
            "실행되는 꼴은 보이기만"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let bin = dir.join("tool");
            std::fs::write(&bin, "#!/bin/sh").unwrap();
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(
                classify(&format!("file://{}", bin.display()), &no),
                Some(OpenTarget::Reveal(bin))
            );
        }
        assert_eq!(
            classify(&format!("file://{}", dir.display()), &no),
            Some(OpenTarget::Open(dir.clone()))
        );
        assert_eq!(
            classify(&format!("file://other-host{}", enc(&doc)), &no),
            None,
            "남의 컴퓨터"
        );
        assert_eq!(
            classify(&format!("file://my-mac{}", enc(&doc)), &|h| h == "my-mac"),
            Some(OpenTarget::Open(doc.clone()))
        );
        assert_eq!(classify("file:///no/such/file", &no), None);
        assert_eq!(classify("file://relative/path", &no), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
