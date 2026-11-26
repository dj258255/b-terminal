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

/// 글에서 찾는 주소 꼴(Ghostty 1.4.0 `url.zig`의 `url_schemes`): 앞 글자 하나만 보고 고른다.
/// 열 때는 `classify`가 따로 거른다(웹·mailto·file은 바로, 나머지는 한 번 묻고, 그 밖의 것은 열지 않는다)
const SCHEMES: [&str; 15] = [
    "https://",
    "http://",
    "mailto:",
    "ftp://",
    "file:",
    "ssh:",
    "git://",
    "ssh://",
    "tel:",
    "magnet:",
    "ipfs://",
    "ipns://",
    "gemini://",
    "gopher://",
    "news:",
];

/// 한글·한자·전각 글자. URL 바로 뒤에 조사가 붙는 한국어 글("…/b-terminal에 올렸다")에서 URL을 끊는다.
/// Ghostty의 `\w`는 이런 글자도 낱말 글자로 보지만 우리는 여기서 끊는다(ADR-086)
fn is_wide_script(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x1F000..=0x1FAFF)
}

/// Ghostty 정규식의 `\w`(글자·숫자·밑줄)에서 넓은 글자를 뺀 것
fn is_word(c: char) -> bool {
    (c.is_alphanumeric() || c == '_') && !is_wide_script(c)
}

/// 경로 글자(`path_chars`): `\w`와 `-.~:/?#@!$&*+;=%`. 쉼표는 없다
fn is_path_char(c: char) -> bool {
    is_word(c)
        || matches!(
            c,
            '-' | '.' | '~' | ':' | '/' | '?' | '#' | '@' | '!' | '$' | '&' | '*' | '+' | ';' | '=' | '%'
        )
}

/// 스킴 주소 글자(`scheme_url_chars`): 경로 글자와 쉼표. 따옴표·꺾쇠·괄호·대괄호는 없다
fn is_scheme_char(c: char) -> bool {
    is_path_char(c) || c == ','
}

fn starts_with_ci(chars: &[char], at: usize, prefix: &str) -> bool {
    let mut it = chars[at..].iter();
    prefix
        .chars()
        .all(|p| it.next().is_some_and(|c| c.eq_ignore_ascii_case(&p)))
}

/// `find_urls`가 한 번에 미리 센 값: 글자 자리마다 이어진 경로 글자의 끝, 그 안의 `.` 유무, 이어진 `[\w\-.]`의 끝
struct Runs {
    path_end: Vec<usize>,
    has_dot: Vec<bool>,
    seg_end: Vec<usize>,
    /// 이 자리 뒤 처음 나오는 `$`의 자리(없으면 글자 수)
    next_dollar: Vec<usize>,
}

impl Runs {
    fn new(chars: &[char]) -> Runs {
        let n = chars.len();
        let mut r = Runs {
            path_end: vec![n; n + 1],
            has_dot: vec![false; n + 1],
            seg_end: vec![n; n + 1],
            next_dollar: vec![n; n + 1],
        };
        for i in (0..n).rev() {
            let c = chars[i];
            let pc = is_path_char(c);
            r.path_end[i] = if pc { r.path_end[i + 1] } else { i };
            r.has_dot[i] = pc && (c == '.' || r.has_dot[i + 1]);
            r.next_dollar[i] = if c == '$' { i } else { r.next_dollar[i + 1] };
            r.seg_end[i] = if is_word(c) || c == '-' || c == '.' {
                r.seg_end[i + 1]
            } else {
                i
            };
        }
        r
    }
}

/// 한 번 시도에서 `$VAR/` 앞에 올 수 있는 `이름/` 토막 수. 긴 줄에서 시도마다 줄 끝까지 가지 않게 한다
const MAX_SEGS: usize = 32;

/// 가지 1: 스킴 주소. 스킴 뒤에 `[::1]:80` 꼴 IPv6나 주소 글자가 이어지고, 낱말 끝의 `(abc)`·`[abc]`는 한 번 들어간다.
/// 끝의 `.`·`,`는 뺀다(끝의 `)`는 괄호 짝이 아니면 애초에 주소 글자가 아니다)
fn scheme_branch(chars: &[char], i: usize) -> Option<usize> {
    // 낱말 가운데에서 시작하는 것은 URL이 아니다(xhttps://…)
    if i > 0 && chars[i - 1].is_alphanumeric() {
        return None;
    }
    let scheme = SCHEMES.iter().find(|s| starts_with_ci(chars, i, s))?;
    let body = i + scheme.len();
    let n = chars.len();
    let mut j = body;
    loop {
        if j < n && chars[j] == '[' {
            // IPv6: `[` 16진수·`:` 한 글자 이상 `]` 그리고 `:포트`
            let mut k = j + 1;
            while k < n && (chars[k] == ':' || chars[k].is_ascii_hexdigit()) {
                k += 1;
            }
            if k > j + 1 && k < n && chars[k] == ']' {
                k += 1;
                if k + 1 < n && chars[k] == ':' && chars[k + 1].is_ascii_digit() {
                    k += 1;
                    while k < n && chars[k].is_ascii_digit() {
                        k += 1;
                    }
                }
                j = k;
                continue;
            }
            break;
        }
        let start = j;
        while j < n && is_scheme_char(chars[j]) {
            j += 1;
        }
        if j == start {
            break;
        }
        // 낱말 끝의 `(abc)`·`[abc]`
        if j < n && matches!(chars[j], '(' | '[') {
            let mut k = j + 1;
            while k < n && is_word(chars[k]) {
                k += 1;
            }
            if k < n && matches!(chars[k], ')' | ']') {
                j = k + 1;
            }
        }
    }
    while j > body && matches!(chars[j - 1], '.' | ',') {
        j -= 1;
    }
    (j > body).then_some(j)
}

/// `\w+://`로 시작하는가(공백 뒤 토막이 다른 주소의 시작이면 경로가 이어지지 않는다)
/// `at`에서 시작하는 글이 찾는 주소 스킴(`SCHEMES`)으로 시작하는가. 힌트가 스킴 주소와 경로를 가를 때 쓴다
pub(crate) fn starts_with_known_scheme(chars: &[char], at: usize) -> bool {
    at < chars.len() && SCHEMES.iter().any(|s| starts_with_ci(chars, at, s))
}

fn starts_with_scheme(chars: &[char], at: usize) -> bool {
    let mut k = at;
    while k < chars.len() && is_word(chars[k]) {
        k += 1;
    }
    k > at && chars.get(k..k + 3).is_some_and(|s| s == [':', '/', '/'])
}

/// 공백 뒤 토막이 이어질 수 있는가: 다른 주소·`/`·`./`·`../`·`~/`로 시작하지 않고, 앞 글자가 `:`가 아니다
fn space_segment_allowed(chars: &[char], space: usize) -> bool {
    if chars[space] != ' ' || space == 0 || chars[space - 1] == ':' {
        return false;
    }
    let at = space + 1;
    if starts_with_scheme(chars, at) {
        return false;
    }
    let dots = chars[at..].iter().take(2).take_while(|&&c| c == '.').count();
    // `\.{0,2}/`: 점 0~2개 뒤 `/`
    for d in 0..=dots {
        if chars.get(at + d) == Some(&'/') {
            return false;
        }
    }
    !(chars.get(at) == Some(&'~') && chars.get(at + 1) == Some(&'/'))
}

/// 가지 2의 몸통: `/`·`./`·`../`·`~/`·`$VAR/`·`.dir/` 뒤. 점이 든 이름이면 파일처럼 보고 공백 토막은 `/`나 `.`로 끝나는 것만 잇고,
/// 점이 없으면 디렉터리처럼 보고 공백 든 이름을 이어 붙인다. 끝의 `:`는 뺀다
fn path_body(chars: &[char], runs: &Runs, p: usize) -> Option<usize> {
    let n = chars.len();
    if p >= n || !is_path_char(chars[p]) {
        return None;
    }
    let mut end = runs.path_end[p];
    if runs.has_dot[p] {
        while end < n && space_segment_allowed(chars, end) {
            let from = end + 1;
            let run_end = runs.path_end[from.min(n)];
            // `[경로 글자]*[/.]`: 이어진 글자 중 마지막 `/`·`.`까지
            match (from..run_end).rev().find(|&k| matches!(chars[k], '/' | '.')) {
                Some(k) => {
                    end = k + 1;
                }
                None => break,
            }
        }
    } else {
        while end < n && space_segment_allowed(chars, end) && from_is_path(chars, end + 1) {
            end = runs.path_end[end + 1];
        }
    }
    let mut stripped = false;
    while end > p && chars[end - 1] == ':' {
        end -= 1;
        stripped = true;
    }
    // 토막이 `:`만 남아 비면 그 앞 공백도 뺀다
    if stripped && end > p && chars[end - 1] == ' ' {
        end -= 1;
    }
    (end > p).then_some(end)
}

fn from_is_path(chars: &[char], at: usize) -> bool {
    chars.get(at).is_some_and(|&c| is_path_char(c))
}

/// 가지 2: `/`·`./`·`../`·`~/`·`$VAR/`·`.dir/`로 시작하는 경로
fn path_branch(chars: &[char], runs: &Runs, i: usize) -> Option<usize> {
    let n = chars.len();
    let prev = i.checked_sub(1).map(|k| chars[k]);
    let mut starts: Vec<usize> = Vec::new();
    if chars[i..].starts_with(&['.', '.', '/']) {
        starts.push(i + 3);
    }
    if chars[i..].starts_with(&['.', '/']) {
        starts.push(i + 2);
    }
    if chars[i..].starts_with(&['~', '/']) && !prev.is_some_and(is_word) {
        starts.push(i + 2);
    }
    // `이름/…$VAR/`: `$`는 낱말 가운데가 아니어야 한다. 같은 경로 글자 덩어리에 `$`가 없으면 볼 것이 없다
    if runs.next_dollar[i] < runs.path_end[i] {
        let mut p = i;
        let mut segs = 0;
        while segs < MAX_SEGS && p < n && is_word(chars[p]) {
            let e = runs.seg_end[p];
            if e < n && chars[e] == '/' {
                p = e + 1;
                segs += 1;
            } else {
                break;
            }
        }
        let word_before = p.checked_sub(1).is_some_and(|k| is_word(chars[k]));
        if p + 1 < n && chars[p] == '$' && !word_before && (chars[p + 1].is_ascii_alphabetic() || chars[p + 1] == '_') {
            let mut q = p + 2;
            while q < n && is_word(chars[q]) {
                q += 1;
            }
            if q < n && chars[q] == '/' {
                starts.push(q + 1);
            }
        }
    }
    // `.dir/`
    if chars[i] == '.' && i + 1 < n && is_word(chars[i + 1]) {
        let e = runs.seg_end[i + 1];
        if e < n && chars[e] == '/' {
            starts.push(e + 1);
        }
    }
    // 홀로 선 `/`(`//`로 시작하는 주석은 경로가 아니다)
    if chars[i] == '/' && !prev.is_some_and(|c| is_word(c) || c == '~' || c == '/') && chars.get(i + 1) != Some(&'/') {
        starts.push(i + 1);
    }
    starts.into_iter().find_map(|p| path_body(chars, runs, p))
}

/// 가지 3: `src/config/url.zig` 같은, 점이 든 맨 상대 경로. 점이 없으면(`input/output`) 경로가 아니다
fn bare_branch(chars: &[char], runs: &Runs, i: usize) -> Option<usize> {
    let n = chars.len();
    if !is_word(chars[i]) || !runs.has_dot[i] {
        return None;
    }
    if i > 0 && is_word(chars[i - 1]) {
        return None;
    }
    // `$` 뒤에 숫자만 있는 자리(`$10/bar.txt`)는 경로가 아니다
    let mut k = i;
    while k > 0 && chars[k - 1].is_ascii_digit() {
        k -= 1;
    }
    if k > 0 && chars[k - 1] == '$' {
        return None;
    }
    let e = runs.seg_end[i];
    if e >= n || chars[e] != '/' {
        return None;
    }
    let p = e + 1;
    if p >= n || !is_path_char(chars[p]) {
        return None;
    }
    let mut end = runs.path_end[p];
    while end > p && chars[end - 1] == ':' {
        end -= 1;
    }
    (end > p).then_some(end)
}

/// 글자 배열에서 링크 구간들(글자 자리 `start..end`): Ghostty 1.4.0 `url.zig`의 정규식(스킴 주소, `/`·`./`·`../`·`~/`·`$VAR/`로 시작하는 경로,
/// 점이 든 맨 상대 경로)을 손으로 짠 스캐너로 옮긴 것이다. 정규식이 아니므로 시간이 글자 수에 거의 비례한다
pub fn find_urls(chars: &[char]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    if chars.is_empty() {
        return out;
    }
    let runs = Runs::new(chars);
    let mut i = 0;
    while i < chars.len() {
        let found = scheme_branch(chars, i)
            .or_else(|| path_branch(chars, &runs, i))
            .or_else(|| bare_branch(chars, &runs, i));
        match found {
            Some(end) if end > i => {
                out.push((i, end));
                i = end;
            }
            _ => i += 1,
        }
    }
    out
}

/// 마지막 칸이 줄 넘김(soft wrap)으로 다음 줄에 이어지는가
pub(crate) fn wraps(screen: &Screen, row: usize) -> bool {
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
    /// 메일 앱으로(`mailto:`, 받는 사람이 있는 것만)
    Mail(String),
    /// 다른 앱을 띄울 수 있는 주소(`ssh:`·`tel:`·`magnet:` 등). 앱이 한 번 물은 뒤에만 연다(Ghostty처럼)
    Confirm(String),
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
    if let Some(rest) = lower.strip_prefix("mailto:") {
        // 받는 사람이 없으면(`mailto:`, `mailto:?subject=x`) 빈 편지를 띄우지 않는다
        let to = rest.split(['?', '#']).next().unwrap_or(rest);
        if to.is_empty() {
            return None;
        }
        // 질의(`?attach=…`)가 있으면 메일 앱이 로컬 파일을 붙일 수 있어 바로 열지 않고 묻는다
        if !confirmable(url) {
            return None;
        }
        if !url.contains('?') {
            return Some(OpenTarget::Mail(url.to_string()));
        }
        return ask_if_visible(url);
    }
    for scheme in CONFIRM_SCHEMES {
        if let Some(rest) = lower.strip_prefix(scheme) {
            let rest = rest.strip_prefix("//").unwrap_or(rest);
            if rest.is_empty() || !confirmable(url) {
                return None;
            }
            // 호스트를 가진 스킴은 사용자 정보(`root@evil.example`)를 받지 않는다. 막대에서 호스트를 속일 수 있다
            let hosted = !matches!(scheme, "tel:" | "magnet:" | "news:");
            if hosted && rest.split(['/', '?', '#']).next().is_some_and(|a| a.contains('@')) {
                return None;
            }
            return ask_if_visible(url);
        }
    }
    None
}

/// 묻는 주소는 막대에 전부 보이는 길이(`CONFIRM_SHOW`)일 때만 묻는다. 넘으면 뒤쪽(질의·받는 사람)이 가려지므로 묻지 않고 막는다
fn ask_if_visible(url: &str) -> Option<OpenTarget> {
    (url.chars().count() <= CONFIRM_SHOW).then(|| OpenTarget::Confirm(url.to_string()))
}

/// 묻는 주소의 길이 상한(`http(s)`와 달리 8KiB까지 보이게 둘 수 없다)
const CONFIRM_MAX: usize = 2048;

/// 묻거나 바로 여는 비웹 주소의 공통 조건: ASCII뿐이고 너무 길지 않다(비ASCII는 호스트를 흉내 낼 수 있다)
fn confirmable(url: &str) -> bool {
    url.is_ascii() && url.len() <= CONFIRM_MAX
}

/// 확인 막대에 보이는 글의 칸 수. 묻는 주소(`Confirm`)는 이 길이까지만 묻는다. 줄여 보이면 뒤쪽(질의)이 가려지기 때문이다
pub const CONFIRM_SHOW: usize = 80;

/// 확인 막대에 보일 글: 주소를 줄이지 않고 그대로 보이되 제어 문자·글 순서를 바꾸는 문자는 보이는 글로 바꾼다
pub fn confirm_text(url: &str) -> String {
    bt_core::proto::printable(url)
}

/// 열기 전에 한 번 묻는 스킴(`url.zig`의 스킴에서 웹·mailto·file을 뺀 것). OSC 8로 온 다른 스킴(`javascript:` 등)은 열지 않는다
const CONFIRM_SCHEMES: [&str; 10] = [
    "ftp:", "ssh:", "git:", "tel:", "magnet:", "ipfs:", "ipns:", "gemini:", "gopher:", "news:",
];

/// 글에서 찾은 경로를 열 때 필요한 것(Ghostty는 터미널의 작업 폴더로 상대 경로를 풀고 `~/`를 집으로 푼다)
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenContext<'a> {
    /// 칸의 작업 폴더(OSC 7). 없으면 상대 경로는 열지 않는다
    pub cwd: Option<&'a Path>,
    /// 집 폴더(`~/`용)
    pub home: Option<&'a Path>,
}

/// `.`·`..`을 글자로만 접는다(Ghostty `std.fs.path.resolve`처럼 심볼릭 링크는 따라가지 않는다)
fn lexical_normalize(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// 화면 글에서 찾은 링크(OSC 8이 아닌 것)를 어떻게 열지. 스킴이 있으면 `classify`와 같고, 없으면 경로다:
/// `~/`는 집 폴더로, 상대 경로는 `ctx.cwd`(OSC 7) 기준으로 풀어 실제로 있는 것만 연다. 그 뒤는 `file` 링크와 같은 허용 목록이다.
/// `:줄:칸`은 떼지 않는다(Ghostty도 그대로 열려 해서 없는 파일이 된다)
pub fn classify_text(text: &str, local_host: &dyn Fn(&str) -> bool, ctx: &OpenContext) -> Option<OpenTarget> {
    let lower = text.to_ascii_lowercase();
    if SCHEMES.iter().any(|s| lower.starts_with(s)) {
        return classify(text, local_host);
    }
    // 경로의 이름에는 공백이 들 수 있다. 그 밖의 공백·제어·보이지 않는 서식 문자는 안 된다
    if text.is_empty()
        || text.len() > 8192
        || text
            .chars()
            .any(|c| (c != ' ' && c.is_whitespace()) || c.is_control() || is_invisible_format(c))
    {
        return None;
    }
    let path = if let Some(rest) = text.strip_prefix("~/") {
        ctx.home?.join(rest)
    } else if text.starts_with('/') {
        PathBuf::from(text)
    } else {
        ctx.cwd.filter(|c| c.is_absolute())?.join(text)
    };
    let real = lexical_normalize(&path).canonicalize().ok()?;
    Some(if safe_to_open(&real) {
        OpenTarget::Open(real)
    } else {
        OpenTarget::Reveal(real)
    })
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
    fn long_run_of_closers_is_trimmed() {
        // 끝의 긴 닫는 괄호: 짝이 있는 하나만 남기고 벗긴다(#597, 전에는 벗길 때마다 구간을 다시 세어 제곱 시간)
        let s = format!("https://a.b/(x){}", ")".repeat(20_000));
        assert_eq!(urls(&s), vec!["https://a.b/(x)"]);
        let s = format!("see https://a.b/{} end", "]".repeat(20_000));
        assert_eq!(urls(&s), vec!["https://a.b/"]);
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
        // 낱말 가운데의 스킴과 빈 스킴은 아니다. mailto는 이제 링크다(#850, Ghostty처럼)
        assert_eq!(urls("xhttps://no.pe https:// mailto:a@b"), vec!["mailto:a@b"]);
    }

    /// Ghostty 1.4.0 `src/config/url.zig`의 시험 표 그대로(#850). 입력의 첫 일치가 기대와 같아야 한다
    #[test]
    fn ghostty_url_table() {
        let cases: &[(&str, &str)] = &[
            ("hello https://example.com world", "https://example.com"),
            ("https://example.com/foo(bar) more", "https://example.com/foo(bar)"),
            (
                "https://example.com/foo(bar)baz more",
                "https://example.com/foo(bar)baz",
            ),
            ("Link inside (https://example.com) parens", "https://example.com"),
            ("Link period https://example.com. More text.", "https://example.com"),
            (
                "Link trailing comma https://example.com, more text.",
                "https://example.com",
            ),
            (
                "Link in double quotes \"https://example.com\" and more",
                "https://example.com",
            ),
            (
                "Link in single quotes 'https://example.com' and more",
                "https://example.com",
            ),
            (
                "some file with https://google.com https://duckduckgo.com links.",
                "https://google.com",
            ),
            (
                "and links in it. links https://yahoo.com mailto:test@example.com ssh://1.2.3.4",
                "https://yahoo.com",
            ),
            ("also match http://example.com non-secure links", "http://example.com"),
            ("match tel://+12123456789 phone numbers", "tel://+12123456789"),
            (
                "match with query url https://example.com?query=1&other=2 and more text.",
                "https://example.com?query=1&other=2",
            ),
            (
                "url with dashes [mode 2027](https://github.com/contour-terminal/terminal-unicode-core) for better unicode support",
                "https://github.com/contour-terminal/terminal-unicode-core",
            ),
            ("dot.http://example.com", "http://example.com"),
            (
                "weird characters https://example.com/~user/?query=1&other=2#hash and more",
                "https://example.com/~user/?query=1&other=2#hash",
            ),
            (
                "square brackets https://example.com/[foo] and more",
                "https://example.com/[foo]",
            ),
            (
                "[13]:TooManyStatements: TempFile#assign_temp_file_to_entity has approx 7 statements [https://example.com/docs/Too-Many-Statements.md]",
                "https://example.com/docs/Too-Many-Statements.md",
            ),
            ("match ftp://example.com ftp links", "ftp://example.com"),
            ("match file://example.com file links", "file://example.com"),
            ("match ssh://example.com ssh links", "ssh://example.com"),
            ("match git://example.com git links", "git://example.com"),
            ("/tmp/test.txt http://www.google.com", "/tmp/test.txt"),
            ("match tel:+18005551234 tel links", "tel:+18005551234"),
            (
                "match magnet:?xt=urn:btih:1234567890 magnet links",
                "magnet:?xt=urn:btih:1234567890",
            ),
            ("match ipfs://QmSomeHashValue ipfs links", "ipfs://QmSomeHashValue"),
            ("match ipns://QmSomeHashValue ipns links", "ipns://QmSomeHashValue"),
            ("match gemini://example.com gemini links", "gemini://example.com"),
            ("match gopher://example.com gopher links", "gopher://example.com"),
            (
                "match news:comp.infosystems.www.servers.unix news links",
                "news:comp.infosystems.www.servers.unix",
            ),
            (
                "/Users/ghostty.user/code/example.py",
                "/Users/ghostty.user/code/example.py",
            ),
            (
                "/Users/ghostty.user/code/../example.py",
                "/Users/ghostty.user/code/../example.py",
            ),
            (
                "/Users/ghostty.user/code/../example.py hello world",
                "/Users/ghostty.user/code/../example.py",
            ),
            ("../example.py", "../example.py"),
            ("../example.py ", "../example.py"),
            ("first time ../example.py contributor ", "../example.py"),
            (
                "[link](/home/user/ghostty.user/example)",
                "/home/user/ghostty.user/example",
            ),
            ("Serving HTTP on :: port 8000 (http://[::]:8000/)", "http://[::]:8000/"),
            (
                "IPv6 address https://[2001:db8::1]:8080/path",
                "https://[2001:db8::1]:8080/path",
            ),
            ("IPv6 localhost http://[::1]:3000", "http://[::1]:3000"),
            (
                "Complex IPv6 https://[2001:db8:85a3:8d3:1319:8a2e:370:7348]:443/",
                "https://[2001:db8:85a3:8d3:1319:8a2e:370:7348]:443/",
            ),
            (
                "IPv6 with path https://[2001:db8::1]/path/to/resource",
                "https://[2001:db8::1]/path/to/resource",
            ),
            (
                "IPv6 with query https://[2001:db8::1]:8080/api?param=value&other=123",
                "https://[2001:db8::1]:8080/api?param=value&other=123",
            ),
            ("IPv6 compressed http://[2001:db8::]:80/", "http://[2001:db8::]:80/"),
            (
                "IPv6 multiple zeros http://[2001:0:0:0:0:0:0:1]",
                "http://[2001:0:0:0:0:0:0:1]",
            ),
            (
                "IPv6 link-local https://[fe80::1234:5678:9abc]",
                "https://[fe80::1234:5678:9abc]",
            ),
            ("IPv6 multicast http://[ff02::1]/stream", "http://[ff02::1]/stream"),
            (
                "IPv6 in markdown [link](http://[2001:db8::1]/docs)",
                "http://[2001:db8::1]/docs",
            ),
            ("./spaces-end.   ", "./spaces-end."),
            ("./space middle", "./space middle"),
            ("../test folder/file.txt", "../test folder/file.txt"),
            ("/tmp/test folder/file.txt", "/tmp/test folder/file.txt"),
            ("/tmp/test  folder/file.txt", "/tmp/test"),
            (
                "diff --git a/src/font/shaper/harfbuzz.zig b/src/font/shaper/harfbuzz.zig",
                "a/src/font/shaper/harfbuzz.zig",
            ),
            ("/tmp/foo /tmp/bar", "/tmp/foo"),
            ("/tmp/foo.txt /tmp/bar.txt", "/tmp/foo.txt"),
            ("src/config/url.zig", "src/config/url.zig"),
            ("app/folder/file.rb:1", "app/folder/file.rb:1"),
            ("modified:   src/config/url.zig", "src/config/url.zig"),
            ("lib/ghostty/terminal.zig:42:10", "lib/ghostty/terminal.zig:42:10"),
            ("some-pkg/src/file.txt more text", "some-pkg/src/file.txt"),
            ("src/foo.c,baz.txt", "src/foo.c"),
            ("~/foo/bar.txt", "~/foo/bar.txt"),
            ("open ~/Documents/notes.md please", "~/Documents/notes.md"),
            ("~/.config/ghostty/config", "~/.config/ghostty/config"),
            ("directory: ~/src/ghostty-org/ghostty", "~/src/ghostty-org/ghostty"),
            ("$HOME/src/config/url.zig", "$HOME/src/config/url.zig"),
            ("project dir: $PWD/src/ghostty/main.zig", "$PWD/src/ghostty/main.zig"),
            ("foo/$BAR/baz", "foo/$BAR/baz"),
            (".foo/bar/$VAR", ".foo/bar/$VAR"),
            (".config/ghostty/config", ".config/ghostty/config"),
            (
                "loaded from .local/share/ghostty/state.db now",
                ".local/share/ghostty/state.db",
            ),
            ("../some/where", "../some/where"),
            (
                "  - shared/src/foo/SomeItem.m:12, shared/src/",
                "shared/src/foo/SomeItem.m:12",
            ),
            ("foo.local/share", "foo.local/share"),
            ("2024/report.txt", "2024/report.txt"),
            ("./foo bar,baz", "./foo bar"),
            ("/tmp/foo bar,baz", "/tmp/foo bar"),
            ("./.config/ghostty: Needs upstream (main)", "./.config/ghostty"),
            ("./Downloads: Operation not permitted", "./Downloads"),
        ];
        for (input, expect) in cases {
            let found = urls(input);
            assert_eq!(found.first().map(String::as_str), Some(*expect), "입력 {input:?}");
        }
        let none: &[&str] = &[
            "input/output",
            "foo/bar",
            "$10/bar",
            "$10/$20",
            "$10/bar.txt",
            "foo/bar,baz.txt",
            "foo$BAR/baz.txt",
            "foo~/bar.txt",
            "// foo bar",
            "//foo",
        ];
        for input in none {
            assert!(urls(input).is_empty(), "입력 {input:?}: {:?}", urls(input));
        }
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
            foreground: Rgb::new(255, 255, 255),
            cursor_color: None,
            chrome_background: Rgb::new(0, 0, 0),
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
        // (둘째 줄 `/a.b/cdefg`는 그 자체로 절대 경로라 링크다. 앞 줄과 이어지지 않는다는 것만 본다, #850)
        assert_eq!(link_at(&s, 4, 1).map(|h| h.url), Some("/a.b/cdefg".to_string()));
        assert_eq!(link_at(&s, 4, 0), None, "첫 줄의 `https:/`는 주소가 아니다");
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
        for cell in &mut s.cells[6..10] {
            cell.link = Some(0);
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
        for cell in &mut s.cells[0..21] {
            cell.link = Some(0);
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
        for cell in &mut s.cells[6..10] {
            cell.link = Some(0);
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

    #[test]
    fn classify_mail_and_confirm_schemes() {
        let no = |_: &str| false;
        assert_eq!(
            classify("mailto:a@b.example", &no),
            Some(OpenTarget::Mail("mailto:a@b.example".into()))
        );
        // 질의가 있으면(`attach=`로 파일을 붙이는 메일 앱이 있다) 바로 열지 않고 묻는다
        for url in ["mailto:a@b.example?subject=hi", "mailto:a@b.example?attach=/etc/passwd"] {
            assert_eq!(classify(url, &no), Some(OpenTarget::Confirm(url.into())), "{url}");
        }
        assert_eq!(
            classify("MAILTO:a@b.example", &no),
            Some(OpenTarget::Mail("MAILTO:a@b.example".into()))
        );
        assert_eq!(classify("mailto:", &no), None);
        assert_eq!(classify("mailto:?subject=x", &no), None);
        for url in [
            "ftp://h/x",
            "ssh://h.example",
            "ssh:h.example",
            "git://h/r.git",
            "tel:+18005551234",
            "tel://+12123456789",
            "magnet:?xt=urn:btih:1",
            "ipfs://Qm",
            "ipns://Qm",
            "gemini://h",
            "gopher://h",
            "news:comp.lang.c",
        ] {
            assert_eq!(classify(url, &no), Some(OpenTarget::Confirm(url.into())), "{url}");
        }
        // 사용자 정보(`@`)·비ASCII·너무 긴 주소는 묻지도 않는다(막대에서 호스트를 속일 수 있다)
        assert_eq!(classify("ssh://root@evil.example", &no), None, "사용자 정보");
        assert_eq!(classify("ssh:root@evil.example", &no), None, "사용자 정보(//없음)");
        assert_eq!(
            classify("ssh://git.example/@x", &no),
            Some(OpenTarget::Confirm("ssh://git.example/@x".into())),
            "경로의 @는 괜찮다"
        );
        assert_eq!(classify("ssh://h\u{43a}.example", &no), None, "비ASCII");
        assert_eq!(
            classify(&format!("ssh://h.example/{}", "p".repeat(3000)), &no),
            None,
            "너무 긴 주소"
        );
        // 묻는 주소는 어떤 스킴이든 막대에 다 보이는 길이(80)까지만 묻는다. 넘으면 묻지 않는다
        for long in [
            format!("ssh://host.example/{}?x=1", "p".repeat(100)),
            format!("gemini://h.example/q?{}", "p".repeat(100)),
            format!("ftp://h.example/{}", "p".repeat(100)),
        ] {
            assert_eq!(classify(&long, &no), None, "{long}");
        }
        let at80 = format!("ssh://h.example/{}", "p".repeat(80 - "ssh://h.example/".len()));
        assert_eq!(classify(&at80, &no), Some(OpenTarget::Confirm(at80.clone())));
        // `://`가 없는 주소(받는 사람·질의가 곧 내용)도 같다
        let fits = format!("tel:+{}", "1".repeat(70));
        assert_eq!(classify(&fits, &no), Some(OpenTarget::Confirm(fits.clone())));
        for long in [
            format!("tel:+{}", "1".repeat(100)),
            format!("magnet:?xt=urn:btih:{}", "a".repeat(100)),
            format!("news:{}", "a".repeat(100)),
            format!("mailto:a@b.example?subject=hi&attach=/{}", "p".repeat(100)),
        ] {
            assert_eq!(classify(&long, &no), None, "{long}");
        }
        assert!(classify("mailto:a@b.example?attach=/etc/hosts", &no).is_some());
        // 목록에 없는 스킴과 숨은 글자는 그대로 열지 않는다
        for url in [
            "javascript:alert(1)",
            "vscode://x",
            "x-man-page://ls",
            "ssh://h/\u{202e}x",
            "tel:1 2",
        ] {
            assert_eq!(classify(url, &no), None, "{url}");
        }
    }

    #[test]
    fn confirm_text_shows_the_whole_address() {
        // 묻는 주소는 80칸 이하라 줄이지 않고 그대로 보인다
        for url in ["tel:+1800", "mailto:a@b.example?subject=hi", "gemini://h.example/q?a=1"] {
            assert_eq!(confirm_text(url), url);
        }
        let at_limit = format!(
            "ssh://host.example/{}",
            "p".repeat(CONFIRM_SHOW - "ssh://host.example/".len())
        );
        assert_eq!(at_limit.chars().count(), CONFIRM_SHOW);
        assert_eq!(confirm_text(&at_limit), at_limit);
        assert!(!confirm_text(&at_limit).contains('…'));
        // 제어 문자·방향 바꾸기 문자는 보이는 글로 바꾼다
        assert_eq!(confirm_text("ssh://h\u{1b}[2J.example"), "ssh://h [2J.example");
        assert_eq!(confirm_text("ssh://h.example/\u{202e}x"), "ssh://h.example/x");
    }

    #[test]
    fn classify_text_paths_follow_ghostty() {
        let no = |_: &str| false;
        let dir = std::env::temp_dir().join(format!("bt-links-path-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("home/sub dir")).unwrap();
        std::fs::create_dir_all(dir.join("work/src")).unwrap();
        let dir = dir.canonicalize().unwrap();
        let (home, work) = (dir.join("home"), dir.join("work"));
        std::fs::write(home.join("notes.txt"), "x").unwrap();
        std::fs::write(home.join("sub dir/a b.md"), "x").unwrap();
        std::fs::write(work.join("src/main.rs"), "x").unwrap();
        std::fs::write(work.join("run.sh"), "x").unwrap();
        let ctx = OpenContext {
            cwd: Some(&work),
            home: Some(&home),
        };
        let at = |t: &str| classify_text(t, &no, &ctx);
        assert_eq!(at("~/notes.txt"), Some(OpenTarget::Open(home.join("notes.txt"))));
        assert_eq!(
            at("~/sub dir/a b.md"),
            Some(OpenTarget::Open(home.join("sub dir/a b.md"))),
            "공백 든 이름"
        );
        assert_eq!(at("./src/main.rs"), Some(OpenTarget::Open(work.join("src/main.rs"))));
        assert_eq!(
            at("src/main.rs"),
            Some(OpenTarget::Open(work.join("src/main.rs"))),
            "맨 상대 경로"
        );
        assert_eq!(
            at("../work/src/main.rs"),
            Some(OpenTarget::Open(work.join("src/main.rs")))
        );
        assert_eq!(
            at(&work.join("src/main.rs").display().to_string()),
            Some(OpenTarget::Open(work.join("src/main.rs")))
        );
        assert_eq!(
            at("./run.sh"),
            Some(OpenTarget::Reveal(work.join("run.sh"))),
            "스크립트는 보이기만"
        );
        assert_eq!(at("./src"), Some(OpenTarget::Open(work.join("src"))), "폴더");
        // 없는 파일, 줄·칸이 붙은 것, $VAR는 열지 않는다(Ghostty도 풀지 않는다)
        assert_eq!(at("./src/none.rs"), None);
        assert_eq!(at("./src/main.rs:42:10"), None);
        assert_eq!(at("$HOME/notes.txt"), None);
        // 작업 폴더를 모르면 상대 경로는 열지 않는다. 집 폴더를 모르면 `~/`도
        let blind = OpenContext::default();
        assert_eq!(classify_text("./src/main.rs", &no, &blind), None);
        assert_eq!(classify_text("~/notes.txt", &no, &blind), None);
        // 숨은 글자·줄바꿈이 든 경로
        assert_eq!(at("./src/main.rs\u{202e}"), None);
        assert_eq!(at("./src/main.rs\n"), None);
        // 스킴이 있으면 classify와 같다
        assert_eq!(
            at("mailto:a@b.example"),
            Some(OpenTarget::Mail("mailto:a@b.example".into()))
        );
        assert_eq!(at("ssh://h"), Some(OpenTarget::Confirm("ssh://h".into())));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn screen_text_paths_and_mail_are_links() {
        // 이슈 #850의 재현: `printf './src/main.rs ~/notes.txt mailto:a@b.example\n'`
        let s = screen(&["./src/main.rs ~/notes.txt mailto:a@b.example"], 60, &[]);
        assert_eq!(link_at(&s, 3, 0).map(|h| h.url).as_deref(), Some("./src/main.rs"));
        assert_eq!(link_at(&s, 13, 0), None, "사이 공백");
        // Ghostty 정규식 그대로: 점이 든 경로 뒤 공백 토막이 `.`로 끝나면(`mailto:a@b.`) 이어 붙는다. 앞에 두면 따로 잡힌다
        assert_eq!(
            link_at(&s, 16, 0).map(|h| h.url).as_deref(),
            Some("~/notes.txt mailto:a@b.")
        );
        let s = screen(&["mailto:a@b.example ./src/main.rs ~/notes.txt"], 60, &[]);
        assert_eq!(link_at(&s, 3, 0).map(|h| h.url).as_deref(), Some("mailto:a@b.example"));
        assert_eq!(link_at(&s, 22, 0).map(|h| h.url).as_deref(), Some("./src/main.rs"));
        assert_eq!(link_at(&s, 36, 0).map(|h| h.url).as_deref(), Some("~/notes.txt"));
    }

    #[test]
    fn korean_particles_still_end_links() {
        // Ghostty의 `\w`는 한글도 먹지만 우리는 조사에서 끊는다(ADR-086)
        assert_eq!(urls("메일은 mailto:a@b.example로 보냈다"), vec!["mailto:a@b.example"]);
        assert_eq!(urls("파일은 ~/notes.txt에 있다"), vec!["~/notes.txt"]);
        assert_eq!(urls("경로 ./src/main.rs를 보자"), vec!["./src/main.rs"]);
        assert_eq!(urls("a/b.rs에서"), vec!["a/b.rs"]);
    }

    /// 긴 줄에서 느려지지 않는다(손으로 짠 스캐너라 줄 길이에 거의 비례한다)
    #[test]
    fn scanner_is_fast_on_hostile_long_lines() {
        let t = std::time::Instant::now();
        let cases = [
            "a".repeat(200_000),
            "a/".repeat(100_000),
            "./a ".repeat(50_000),
            format!("{}$X/", "a/".repeat(50_000)),
            "https://".repeat(25_000),
            "a.".repeat(100_000),
            "/ ".repeat(100_000),
            format!("./{}:", "a b ".repeat(50_000)),
            "$1/".repeat(60_000),
            "https://a.b/(".repeat(20_000),
        ];
        for (n, c) in cases.iter().enumerate() {
            let chars: Vec<char> = c.chars().collect();
            let t1 = std::time::Instant::now();
            let _ = find_urls(&chars);
            assert!(t1.elapsed().as_millis() < 1000, "경우 {n}: {:?}", t1.elapsed());
        }
        assert!(t.elapsed().as_secs() < 5, "{:?}", t.elapsed());
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
        assert_eq!(
            classify("ssh://host", &no),
            Some(OpenTarget::Confirm("ssh://host".into()))
        );
        assert_eq!(classify("ssh://", &no), None, "받는 곳이 없다");
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
