//! kitty 세션 파일(#934): 사람이 손으로 쓰는 시작 배치(탭·칸·작업 폴더·실행할 명령)를 읽어 "배치 계획"으로 만든다.
//! 파일 안의 글을 읽기만 하고 창·PTY를 모르는 순수 함수라 여기서 테스트한다. 문법과 우리 쪽 대응은 `docs/configuration.md`의
//! "kitty 세션 파일" 절에 있다. 원본은 kitty `session.py`의 `parse_session`·`Session`·`Tab`이다.
//!
//! 믿는 범위: 이 파일은 사용자가 쓴 것이다(설정과 같은 신뢰, ADR-104). 그래도 크기·개수 상한을 둔다(실수로 수천 개를 열지 않게).
//! `launch`의 명령은 글자 그대로 (프로그램, 인자 배열)이 되고 셸 문자열로 이어 붙이지 않는다

use std::path::{Path, PathBuf};

use bt_core::session_path::{expand_tilde, expand_vars};

use crate::session_layout::{LayoutSpec, PlanNode, SplitReq, build_tree};

/// 세션 파일 크기 상한(바이트)
pub const MAX_FILE_BYTES: u64 = 1 << 20;
/// 창(`new_os_window`) 수 상한
pub const MAX_WINDOWS: usize = 8;
/// 모든 창을 합친 탭 수 상한
pub const MAX_TABS: usize = 64;
/// 탭 하나의 칸 수 상한
pub const MAX_PANES_PER_TAB: usize = 32;
/// 모든 탭을 합친 칸 수 상한
pub const MAX_PANES: usize = 256;
/// 칸 하나에 줄 수 있는 `--env` 수 상한
const MAX_ENV: usize = 64;
/// 적어 두는 경고 수 상한(상한을 넘는 줄이 수만 개여도 경고가 쏟아지지 않게)
const MAX_WARNINGS: usize = 100;

/// 칸 하나: 띄울 프로그램과 처음 상태
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PanePlan {
    /// 없으면 기본 셸(설정 `command`가 있으면 그것)
    pub program: Option<String>,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    /// `launch --title`·`title` 줄. 쓰기 전에 앱이 다듬는다
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TabPlan {
    pub title: Option<String>,
    pub panes: Vec<PanePlan>,
    /// 칸 번호(`panes` 차례)로 만든 분할 트리
    pub tree: PlanNode,
    /// 한 칸만 크게(kitty `stack`)
    pub zoomed: bool,
    /// 처음에 고른 칸 번호
    pub focus: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WindowPlan {
    pub title: Option<String>,
    pub tabs: Vec<TabPlan>,
    pub active_tab: usize,
}

/// 읽은 결과: 창 계획과 경고(줄 번호가 앞에 붙은 글)
#[derive(Debug, Clone, PartialEq)]
pub struct SessionPlan {
    pub windows: Vec<WindowPlan>,
    pub warnings: Vec<String>,
}

/// 읽을 때 필요한 바깥 값
pub struct Ctx<'a> {
    /// 세션 파일이 있는 폴더. `cd`·`--cwd`의 상대 경로 기준(kitty `session_base_dir`)
    pub base_dir: &'a Path,
    pub home: &'a Path,
    pub env: &'a dyn Fn(&str) -> Option<String>,
}

/// 파일을 읽는다. 보통 파일이고 크기 상한 이하이며 UTF-8이어야 한다(FIFO·장치 파일에서 멈추지 않게 먼저 종류를 본다)
pub fn read_file(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !meta.is_file() {
        return Err("보통 파일이 아닙니다".into());
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(format!("{}바이트로 상한(1MiB)을 넘습니다", meta.len()));
    }
    let mut buf = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(MAX_FILE_BYTES + 1).read_to_end(&mut buf))
        .map_err(|e| e.to_string())?;
    if buf.len() as u64 > MAX_FILE_BYTES {
        return Err("읽는 동안 상한(1MiB)을 넘었습니다".into());
    }
    let text = String::from_utf8(buf).map_err(|_| "UTF-8 글이 아닙니다".to_string())?;
    Ok(text.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(text))
}

/// kitty가 쓰는 파이썬 `str.splitlines()`의 줄 경계(`session.py`의 `session_line_break_pat`)로 나눈다
fn split_lines(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c| {
        matches!(
            c,
            '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        )
    })
}

/// 셸처럼 낱말로 나눈다: 공백 구분, `'…'`, `"…"`(`\" \\ \$ \`` 만 벗김), 따옴표 밖의 `\글자`, `$'…'`(ANSI-C 따옴표).
/// kitty `shlex_split(text, True)`와 같은 뜻이다. 변수는 풀지 않고 NUL은 넣지 않는다
pub fn split_words(text: &str) -> Result<Vec<String>, String> {
    let cs: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c.is_whitespace() {
            if in_word {
                out.push(std::mem::take(&mut cur));
                in_word = false;
            }
            i += 1;
            continue;
        }
        in_word = true;
        match c {
            '\'' => {
                i += 1;
                loop {
                    match cs.get(i) {
                        None => return Err("작은따옴표가 닫히지 않았습니다".into()),
                        Some('\'') => break,
                        Some(&ch) => cur.push(ch),
                    }
                    i += 1;
                }
                i += 1;
            }
            '"' => {
                i += 1;
                loop {
                    match cs.get(i) {
                        None => return Err("큰따옴표가 닫히지 않았습니다".into()),
                        Some('"') => break,
                        Some('\\') if matches!(cs.get(i + 1), Some('"' | '\\' | '$' | '`')) => {
                            cur.push(cs[i + 1]);
                            i += 1;
                        }
                        Some(&ch) => cur.push(ch),
                    }
                    i += 1;
                }
                i += 1;
            }
            '$' if cs.get(i + 1) == Some(&'\'') => {
                i += 2;
                loop {
                    match cs.get(i) {
                        None => return Err("$'…'가 닫히지 않았습니다".into()),
                        Some('\'') => break,
                        Some('\\') => i = decode_ansi_c(&cs, i, &mut cur),
                        Some(&ch) => {
                            cur.push(ch);
                            i += 1;
                        }
                    }
                }
                i += 1;
            }
            '\\' => match cs.get(i + 1) {
                Some(&n) => {
                    cur.push(n);
                    i += 2;
                }
                None => {
                    cur.push('\\');
                    i += 1;
                }
            },
            ch => {
                cur.push(ch);
                i += 1;
            }
        }
    }
    if in_word {
        out.push(cur);
    }
    Ok(out)
}

/// `$'…'` 안의 `\`로 시작하는 한 덩어리를 풀어 `out`에 넣고 다음 자리를 돌려준다. NUL이 되는 것은 넣지 않는다
fn decode_ansi_c(cs: &[char], at: usize, out: &mut String) -> usize {
    let hex = |from: usize, max: usize| {
        let digits: String = cs[from..]
            .iter()
            .take(max)
            .take_while(|c| c.is_ascii_hexdigit())
            .collect();
        (u32::from_str_radix(&digits, 16).ok(), digits.len())
    };
    let push = |out: &mut String, v: u32| {
        if v != 0
            && let Some(ch) = char::from_u32(v)
        {
            out.push(ch);
        }
    };
    let Some(&e) = cs.get(at + 1) else {
        out.push('\\');
        return at + 1;
    };
    match e {
        'n' => out.push('\n'),
        't' => out.push('\t'),
        'r' => out.push('\r'),
        'a' => out.push('\u{7}'),
        'b' => out.push('\u{8}'),
        'e' | 'E' => out.push('\u{1b}'),
        'f' => out.push('\u{c}'),
        'v' => out.push('\u{b}'),
        '\\' | '\'' | '"' | '?' => out.push(e),
        'x' | 'u' | 'U' => {
            let max = match e {
                'x' => 2,
                'u' => 4,
                _ => 8,
            };
            return match hex(at + 2, max) {
                (Some(v), n) if n > 0 => {
                    push(out, v);
                    at + 2 + n
                }
                _ => {
                    out.push('\\');
                    out.push(e);
                    at + 2
                }
            };
        }
        '0'..='7' => {
            let digits: String = cs[at + 1..]
                .iter()
                .take(3)
                .take_while(|c| ('0'..='7').contains(c))
                .collect();
            push(out, u32::from_str_radix(&digits, 8).unwrap_or(0));
            return at + 1 + digits.len();
        }
        other => {
            out.push('\\');
            out.push(other);
        }
    }
    at + 2
}

/// `launch`의 값을 받는 옵션(kitty `launch.py`의 옵션 목록에서 값 있는 것만. `-w`는 `--watcher`의 짧은 이름).
/// 여기에 없는 `--옵션`은 값 없는 깃발로 본다
const LAUNCH_VALUE_OPTS: &[&str] = &[
    "title",
    "window-title",
    "tab-title",
    "cwd",
    "env",
    "var",
    "location",
    "bias",
    "type",
    "source-window",
    "dock-edge",
    "dock-size",
    "focus-existing",
    "add-to-session",
    "next-to",
    "remote-control-password",
    "stdin-source",
    "marker",
    "os-window-class",
    "os-window-name",
    "os-window-title",
    "os-window-state",
    "os-window-position",
    "logo",
    "logo-position",
    "logo-alpha",
    "color",
    "spacing",
    "watcher",
    "os-panel",
];

#[derive(Debug, Default)]
struct LaunchOpts {
    title: Option<String>,
    cwd: Option<String>,
    env: Vec<String>,
    vars: Vec<String>,
    location: Option<String>,
    bias: Option<String>,
    kind: Option<String>,
    /// 알아듣지만 하지 않는 옵션 이름
    ignored: Vec<String>,
}

/// `launch` 낱말들에서 옵션을 읽고 첫 인자(프로그램)의 자리를 돌려준다
fn parse_launch(tokens: &[String]) -> Result<(LaunchOpts, usize), String> {
    let mut o = LaunchOpts::default();
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        if t == "--" {
            i += 1;
            break;
        }
        if let Some(body) = t.strip_prefix("--").filter(|b| !b.is_empty()) {
            let (name, inline) = match body.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (body, None),
            };
            if !LAUNCH_VALUE_OPTS.contains(&name) {
                o.ignored.push(format!("--{name}"));
                i += 1;
                continue;
            }
            let value = match inline {
                Some(v) => v,
                None => {
                    i += 1;
                    tokens.get(i).cloned().ok_or(format!("--{name}에 값이 없습니다"))?
                }
            };
            match name {
                "title" | "window-title" => o.title = Some(value),
                "cwd" => o.cwd = Some(value),
                "env" => o.env.push(value),
                "var" => o.vars.push(value),
                "location" => o.location = Some(value),
                "bias" => o.bias = Some(value),
                "type" => o.kind = Some(value),
                _ => o.ignored.push(format!("--{name}")),
            }
            i += 1;
        } else if t == "-w" {
            // `--watcher`의 짧은 이름(값이 있다)
            o.ignored.push(t.clone());
            i += 2;
        } else if t.len() > 1 && t.starts_with('-') {
            o.ignored.push(t.clone());
            i += 1;
        } else {
            break;
        }
    }
    Ok((o, i))
}

struct PaneB {
    plan: PanePlan,
    vars: Vec<(String, String)>,
    req: Option<SplitReq>,
}

struct TabB {
    title: Option<String>,
    layout: LayoutSpec,
    cwd: Option<PathBuf>,
    next_title: Option<String>,
    panes: Vec<PaneB>,
    /// 지금 고른 칸(다음 칸을 이 칸에 붙인다). 새 칸을 열면 그 칸으로 옮긴다
    active: usize,
    /// `focus`로 정한, 탭을 열었을 때 고를 칸
    focus: Option<usize>,
    /// 칸이 생기기 전에 온 `focus_matching_window`
    pending_match: Option<String>,
}

impl TabB {
    fn new(title: &str) -> Self {
        TabB {
            title: Some(title.trim().to_string()).filter(|t| !t.is_empty()),
            layout: LayoutSpec::default(),
            cwd: None,
            next_title: None,
            panes: Vec::new(),
            active: 0,
            focus: None,
            pending_match: None,
        }
    }
}

struct WinB {
    title: Option<String>,
    tabs: Vec<TabB>,
    active_tab: usize,
    /// `focus_tab` 줄의 번호와 값
    focus_tab: Option<(usize, String)>,
}

impl WinB {
    fn new(first_tab: &str) -> Self {
        WinB {
            title: None,
            tabs: vec![TabB::new(first_tab)],
            active_tab: 0,
            focus_tab: None,
        }
    }
}

struct Builder<'a> {
    ctx: &'a Ctx<'a>,
    done: Vec<WindowPlan>,
    cur: WinB,
    warnings: Vec<String>,
    line: usize,
    /// 상한을 넘어 다음 `new_tab`·`new_os_window` 전까지의 줄을 버린다
    sink: bool,
    /// 창 수 상한을 넘어 지금 창을 통째로 버린다
    drop_cur: bool,
    done_tabs: usize,
    panes: usize,
}

/// 세션 파일 글을 읽어 배치 계획을 만든다. 모르는 줄은 경고하고 넘어간다(kitty는 모르는 줄이 있으면 파일 전체를 버린다)
pub fn parse(text: &str, ctx: &Ctx) -> SessionPlan {
    let mut b = Builder {
        ctx,
        done: Vec::new(),
        cur: WinB::new(""),
        warnings: Vec::new(),
        line: 0,
        sink: false,
        drop_cur: false,
        done_tabs: 0,
        panes: 0,
    };
    for (i, raw) in split_lines(text).enumerate() {
        b.line = i + 1;
        b.handle(raw);
    }
    b.finish()
}

impl Builder<'_> {
    fn warn(&mut self, msg: impl Into<String>) {
        match self.warnings.len() {
            n if n < MAX_WARNINGS => self.warnings.push(format!("{}번째 줄: {}", self.line, msg.into())),
            MAX_WARNINGS => self.warnings.push("경고가 너무 많아 이후는 적지 않습니다".into()),
            _ => {}
        }
    }

    fn expand(&self, s: &str) -> String {
        expand_vars(s, self.ctx.env)
    }

    /// `~`를 풀고 상대 경로는 세션 파일 폴더 기준으로 잇는다
    fn resolve_dir(&self, s: &str) -> PathBuf {
        let p = expand_tilde(s, self.ctx.home);
        if p.is_absolute() { p } else { self.ctx.base_dir.join(p) }
    }

    fn tab(&mut self) -> &mut TabB {
        self.cur.tabs.last_mut().expect("창에는 늘 탭이 하나 이상 있다")
    }

    fn handle(&mut self, raw: &str) {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            return;
        }
        let (cmd, rest) = match line.split_once(char::is_whitespace) {
            Some((c, r)) => (c, r.trim()),
            None => (line, ""),
        };
        // 변수는 launch의 인자와 set_layout_state를 빼고 푼다(kitty `parse_session`)
        let rest_expanded = if matches!(cmd, "launch" | "set_layout_state") {
            rest.to_string()
        } else {
            self.expand(rest)
        };
        let rest = rest_expanded.as_str();
        match cmd {
            "new_tab" => self.new_tab(rest),
            "new_os_window" => self.new_os_window(rest),
            _ if self.sink => {}
            "layout" => self.layout(rest),
            "launch" => self.launch(rest),
            "focus" => {
                let n = self.cur.tabs.len() - 1;
                self.cur.active_tab = n;
                let t = self.tab();
                t.focus = Some(t.panes.len().saturating_sub(1));
            }
            "focus_tab" => self.cur.focus_tab = Some((self.line, rest.to_string())),
            "cd" => {
                let dir = if rest.is_empty() {
                    self.ctx.base_dir.to_path_buf()
                } else {
                    self.resolve_dir(rest)
                };
                self.tab().cwd = Some(dir);
            }
            "title" => {
                let t = rest.trim().to_string();
                self.tab().next_title = Some(t).filter(|t| !t.is_empty());
            }
            "os_window_title" => self.cur.title = Some(rest.to_string()).filter(|t| !t.is_empty()),
            "focus_matching_window" => self.focus_matching(rest),
            // 알아듣지만 우리 창에는 대응이 없는 줄
            "enabled_layouts" | "set_layout_state" | "resize_window" | "focus_os_window" | "os_window_size"
            | "os_window_position" | "os_window_class" | "os_window_name" | "os_window_state" => {
                self.warn(format!("`{cmd}`는 지원하지 않아 무시했습니다"));
            }
            other => self.warn(format!("모르는 줄이라 무시했습니다: {other}")),
        }
    }

    fn new_tab(&mut self, title: &str) {
        // 상한을 넘은 뒤에는 새 탭을 만들지 않고 경고도 되풀이하지 않는다
        if self.sink {
            return;
        }
        if self.panes >= MAX_PANES {
            self.warn(format!("칸이 {MAX_PANES}개를 넘어 이후 탭은 만들지 않습니다"));
            self.sink = true;
            return;
        }
        // 칸이 없는 마지막 탭은 지운다(kitty `Session.add_tab`)
        if self.cur.tabs.last().is_some_and(|t| t.panes.is_empty()) {
            self.cur.tabs.pop();
        }
        if self.done_tabs + self.cur.tabs.len() >= MAX_TABS {
            self.warn(format!(
                "탭이 {MAX_TABS}개를 넘어 다음 new_tab·new_os_window 전까지의 줄을 무시했습니다"
            ));
            self.sink = true;
            // 이 창에 탭이 하나도 남지 않으면 창을 통째로 버린다. 자리는 두어 이어지는 줄이 있어도 안전하게 한다
            if self.cur.tabs.is_empty() {
                self.drop_cur = true;
                self.cur.tabs.push(TabB::new(""));
            }
            return;
        }
        self.sink = false;
        self.cur.tabs.push(TabB::new(title));
    }

    fn new_os_window(&mut self, title: &str) {
        let fresh = WinB::new(title);
        if self.drop_cur {
            // 이미 상한을 넘은 창이 이어진다
            self.cur = fresh;
            self.sink = true;
            return;
        }
        let old = std::mem::replace(&mut self.cur, fresh);
        self.done_tabs += old.tabs.len();
        self.sink = false;
        let plan = self.finish_window(old);
        self.done.push(plan);
        if self.done.len() >= MAX_WINDOWS {
            self.warn(format!("창이 {MAX_WINDOWS}개를 넘어 이후 창은 만들지 않습니다"));
            self.drop_cur = true;
            self.sink = true;
        }
    }

    fn layout(&mut self, text: &str) {
        match LayoutSpec::parse(text) {
            Ok((spec, notes)) => {
                self.tab().layout = spec;
                for n in notes {
                    self.warn(n);
                }
            }
            Err(e) => self.warn(format!("{e}. 앞의 레이아웃을 그대로 씁니다")),
        }
    }

    fn focus_matching(&mut self, spec: &str) {
        if !spec.starts_with("var:") {
            self.warn("`focus_matching_window`는 `var:이름=값`만 지원해 무시했습니다");
            return;
        }
        if self.tab().panes.is_empty() {
            self.tab().pending_match = Some(spec.to_string());
        } else {
            self.apply_match(spec);
        }
    }

    /// `var:이름=값`(값을 빼면 그 이름이 있는 칸)에 맞는 첫 칸을 고른 칸으로 한다
    fn apply_match(&mut self, spec: &str) {
        let body = &spec["var:".len()..];
        let (k, v) = match body.split_once('=') {
            Some((k, v)) => (k, Some(v)),
            None => (body, None),
        };
        let found = self
            .tab()
            .panes
            .iter()
            .position(|p| p.vars.iter().any(|(pk, pv)| pk == k && v.is_none_or(|v| v == pv)));
        match found {
            Some(i) => self.tab().active = i,
            None => self.warn(format!("`{spec}`에 맞는 칸이 없어 무시했습니다")),
        }
    }

    fn launch(&mut self, rest: &str) {
        let tokens = match split_words(rest) {
            Ok(t) => t,
            Err(e) => return self.warn(format!("launch: {e}. 이 줄을 무시했습니다")),
        };
        // 변수는 옵션 부분만 푼다(인자는 그대로, kitty와 같음). 풀고 나서 다시 읽는다
        let limit = match parse_launch(&tokens) {
            Ok((_, n)) => n,
            Err(e) => return self.warn(format!("launch: {e}. 이 줄을 무시했습니다")),
        };
        let toks: Vec<String> = tokens
            .iter()
            .enumerate()
            .map(|(i, t)| if i < limit { self.expand(t) } else { t.clone() })
            .collect();
        let (opts, at) = match parse_launch(&toks) {
            Ok(x) => x,
            Err(e) => return self.warn(format!("launch: {e}. 이 줄을 무시했습니다")),
        };
        if let Some(kind) = opts.kind.as_deref().filter(|k| *k != "window") {
            return self.warn(format!(
                "launch --type={kind}는 세션 파일에서 쓸 수 없습니다(탭은 new_tab, 창은 new_os_window). 이 줄을 무시했습니다"
            ));
        }
        if self.tab().panes.len() >= MAX_PANES_PER_TAB || self.panes >= MAX_PANES {
            return self.warn(format!(
                "칸이 탭마다 {MAX_PANES_PER_TAB}개, 전체 {MAX_PANES}개를 넘어 이 줄을 무시했습니다"
            ));
        }
        for name in &opts.ignored {
            self.warn(format!("launch 옵션 {name}는 지원하지 않아 무시했습니다"));
        }
        let (program, args) = match toks.get(at..) {
            Some([first, rest @ ..]) if !first.is_empty() => (Some(first.clone()), rest.to_vec()),
            Some([_, ..]) => return self.warn("launch: 프로그램 이름이 비어 있어 이 줄을 무시했습니다"),
            _ => (None, Vec::new()),
        };
        let mut env = Vec::new();
        for e in &opts.env {
            match e.split_once('=') {
                Some((k, v)) if !k.is_empty() && !k.contains('\0') && !v.contains('\0') => {
                    if env.len() < MAX_ENV {
                        env.push((k.to_string(), v.to_string()));
                    }
                }
                _ => self.warn(format!("launch --env는 `이름=값` 꼴만 지원해 무시했습니다: {e}")),
            }
        }
        let cwd = match opts.cwd.as_deref() {
            Some(c @ ("current" | "last_reported" | "oldest" | "root")) => {
                self.warn(format!("launch --cwd={c}는 지원하지 않아 `cd` 폴더를 씁니다"));
                None
            }
            Some(c) => Some(self.resolve_dir(c)),
            None => None,
        };
        let vars: Vec<(String, String)> = opts
            .vars
            .iter()
            .map(|v| match v.split_once('=') {
                Some((k, v)) => (k.to_string(), v.to_string()),
                None => (v.clone(), String::new()),
            })
            .collect();
        let bias = match opts.bias.as_deref() {
            None => None,
            Some(b) => match b.parse::<f32>() {
                Ok(n) if n.is_finite() && n != 0.0 => Some(n.abs().clamp(0.0, 100.0) / 100.0),
                Ok(_) => None,
                Err(_) => {
                    self.warn(format!("launch --bias 값이 숫자가 아니라 무시했습니다: {b}"));
                    None
                }
            },
        };
        let (horizontal, before) = match opts.location.as_deref() {
            None | Some("default" | "after" | "neighbor" | "last" | "vsplit" | "split") => (true, false),
            Some("hsplit") => (false, false),
            Some("before" | "first") => (true, true),
            Some(other) => {
                self.warn(format!("launch --location={other}는 지원하지 않아 무시했습니다"));
                (true, false)
            }
        };
        let tab = self.tab();
        let index = tab.panes.len();
        let title = opts.title.or_else(|| tab.next_title.take()).filter(|t| !t.is_empty());
        tab.next_title = None;
        let cwd = cwd.or_else(|| tab.cwd.clone());
        let req = (index > 0).then(|| SplitReq {
            parent: tab.active.min(index - 1),
            horizontal,
            bias,
            before,
        });
        tab.panes.push(PaneB {
            plan: PanePlan {
                program,
                args,
                cwd,
                env,
                title,
            },
            vars,
            req,
        });
        // 새 칸이 고른 칸이 된다. 칸이 생기기 전에 와 있던 focus_matching_window를 이제 적용한다
        tab.active = index;
        let pending = tab.pending_match.take();
        self.panes += 1;
        if let Some(spec) = pending {
            self.apply_match(&spec);
        }
    }

    fn finish_window(&mut self, w: WinB) -> WindowPlan {
        let mut tabs = Vec::new();
        for t in w.tabs {
            let mut panes: Vec<PaneB> = t.panes;
            // 칸이 없는 탭은 기본 셸 하나(kitty `finalize_session`)
            if panes.is_empty() {
                panes.push(PaneB {
                    plan: PanePlan::default(),
                    vars: Vec::new(),
                    req: None,
                });
            }
            let n = panes.len();
            let reqs: Vec<Option<SplitReq>> = panes.iter().map(|p| p.req).collect();
            let (tree, zoomed) = build_tree(&t.layout, n, &reqs);
            tabs.push(TabPlan {
                title: t.title,
                panes: panes.into_iter().map(|p| p.plan).collect(),
                tree,
                zoomed,
                focus: t.focus.unwrap_or(n - 1).min(n - 1),
            });
        }
        let mut active_tab = w.active_tab.min(tabs.len() - 1);
        if let Some((line, spec)) = w.focus_tab {
            self.line = line;
            let by_index = spec.parse::<usize>().ok().filter(|i| *i < tabs.len());
            let by_title = spec
                .strip_prefix("title:")
                .and_then(|t| tabs.iter().position(|tab| tab.title.as_deref() == Some(t)));
            match by_index.or(by_title) {
                Some(i) => active_tab = i,
                None => self.warn(format!(
                    "`focus_tab {spec}`에 맞는 탭이 없어 무시했습니다(번호와 `title:이름`만 지원)"
                )),
            }
        }
        WindowPlan {
            title: w.title,
            tabs,
            active_tab,
        }
    }

    fn finish(mut self) -> SessionPlan {
        let last = std::mem::replace(&mut self.cur, WinB::new(""));
        if !self.drop_cur {
            let plan = self.finish_window(last);
            self.done.push(plan);
        }
        SessionPlan {
            windows: self.done,
            warnings: self.warnings,
        }
    }
}
