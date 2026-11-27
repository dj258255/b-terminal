//! 탭 제목 틀(kitty `tab_title_template`·`active_tab_title_template`·`bell_on_tab`·`tab_activity_symbol`·`tab_title_max_length`의
//! 안전한 일부). 틀은 이름 바꾸기(`{title}`·`{index}`…)와 꾸밈(`{fmt.fg.red}`·`{fmt.bold}`…)만 받는다.
//! 파이썬 표현식·`{custom}`은 받지 않으므로 설정 파일이 프로그램을 실행하지 못한다(ADR-098).
//!
//! 프로그램이 OSC 0/2로 보낸 제목은 믿지 않는 문자열이다. `{title}` 자리에 값으로만 끼우고 다시 해석하지 않는다:
//! 제목 안의 `{fmt.fg.red}`는 글자 그대로 보이고 색을 바꾸지 못한다. 제어 문자는 지운다

use crate::theme::Rgb;

/// 설정이 없을 때의 틀: 지금 탭 제목 그대로(벨 기호, 활동 기호, 제목). kitty의 기본 틀에서 빨간 꾸밈·진행률 글자·
/// 보안 입력 기호를 뺀 것이다. 진행은 탭 아래 막대로 따로 그리고, 기호 앞뒤에 `{fmt.fg.red}`를 두지 않으므로 벨 기호는 탭 글자색이다
pub const DEFAULT_TEMPLATE: &str = "{bell_symbol}{activity_symbol}{title}";
/// kitty `bell_on_tab`의 기본값
pub const DEFAULT_BELL: &str = "🔔 ";

/// 탭 제목 설정 다섯 키
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabTitleConfig {
    pub template: String,
    /// `none`이면 `None`: 고른 탭도 `template`을 쓴다
    pub active_template: Option<String>,
    pub bell_symbol: String,
    pub activity_symbol: String,
    /// 제목이 쓸 수 있는 칸 수. 0이면 제한 없음
    pub max_length: u32,
}

impl Default for TabTitleConfig {
    fn default() -> Self {
        Self {
            template: DEFAULT_TEMPLATE.to_string(),
            active_template: None,
            bell_symbol: DEFAULT_BELL.to_string(),
            activity_symbol: String::new(),
            max_length: 0,
        }
    }
}

/// kitty `tab_title_template`: 한 쌍의 따옴표(`'`나 `"`)로 감쌌으면 벗긴다
fn unquote(v: &str) -> &str {
    for q in ['\'', '"'] {
        if v.len() >= 2 && v.starts_with(q) && v.ends_with(q) {
            return &v[1..v.len() - 1];
        }
    }
    v
}

impl TabTitleConfig {
    /// 키 하나를 적용한다. 이 모듈의 키가 아니면 `None`, 값이 틀리면 `Some(false)`(이전 값을 둔다), 받으면 `Some(true)`
    pub fn apply(&mut self, key: &str, v: &str) -> Option<bool> {
        // 설정 읽기가 바깥 큰따옴표를 이미 벗겼으므로 안쪽 공백을 지키려고 다듬지 않는다
        let ok = match key {
            "tab-title-template" => {
                // 따옴표 없이 비운 값은 기본 틀로 되돌린다. `""`는 빈 틀(제목을 그리지 않음)이다
                let t = if v.trim().is_empty() {
                    DEFAULT_TEMPLATE
                } else {
                    unquote(v)
                };
                if parse(t).is_some() {
                    self.template = t.to_string();
                    true
                } else {
                    false
                }
            }
            "active-tab-title-template" => {
                let t = unquote(v);
                if t == "none" || v.trim().is_empty() {
                    self.active_template = None;
                    true
                } else if parse(t).is_some() {
                    self.active_template = Some(t.to_string());
                    true
                } else {
                    false
                }
            }
            // kitty `bell_on_tab`: yes·y·true는 기본 기호, no·n·false·none은 빈 글자
            "bell-on-tab" => {
                let lower = v.trim().to_ascii_lowercase();
                let s = match lower.as_str() {
                    "yes" | "y" | "true" => DEFAULT_BELL.to_string(),
                    "no" | "n" | "false" | "none" => String::new(),
                    _ => unquote(v).to_string(),
                };
                self.bell_symbol = s;
                true
            }
            // kitty `tab_activity_symbol`: none이면 빈 글자
            "tab-activity-symbol" => {
                self.activity_symbol = if v.trim() == "none" {
                    String::new()
                } else {
                    unquote(v).to_string()
                };
                true
            }
            "tab-title-max-length" => match v.trim().parse::<u32>() {
                Ok(n) => {
                    self.max_length = n;
                    true
                }
                Err(_) => false,
            },
            _ => return None,
        };
        Some(ok)
    }

    /// `+show-config`에 찍을 (키, 값)
    pub fn lines(&self) -> Vec<(&'static str, String)> {
        vec![
            ("tab-title-template", format!("\"{}\"", self.template)),
            (
                "active-tab-title-template",
                self.active_template
                    .as_ref()
                    .map_or_else(|| "none".to_string(), |t| format!("\"{t}\"")),
            ),
            ("bell-on-tab", format!("\"{}\"", self.bell_symbol)),
            (
                "tab-activity-symbol",
                if self.activity_symbol.is_empty() {
                    "none".to_string()
                } else {
                    format!("\"{}\"", self.activity_symbol)
                },
            ),
            ("tab-title-max-length", self.max_length.to_string()),
        ]
    }

    /// 이 탭이 쓸 틀
    fn template_for(&self, active: bool) -> &str {
        match (&self.active_template, active) {
            (Some(t), true) => t,
            _ => &self.template,
        }
    }
}

/// 꾸밈 조각의 색
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanColor {
    /// 탭의 글자색·바탕색(kitty `fmt.fg.tab`·`fmt.bg.tab`)
    Tab,
    /// 탭 줄의 기본 글자색(안 고른 탭 글자색)·바탕색(막대 바탕). kitty `fmt.fg.default`·`fmt.bg.default`
    Default,
    Rgb(Rgb),
    /// xterm 256색표(`fmt.fg.color0`~`color255`)
    Indexed(u8),
}

/// 제목의 한 조각: 같은 꾸밈으로 그리는 글
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub fg: SpanColor,
    pub bg: SpanColor,
    pub bold: bool,
    pub italic: bool,
}

impl Span {
    /// 탭 기본 꾸밈(`bold`·`italic`은 탭 글꼴 모양)의 조각인가
    pub fn is_plain(&self, base_bold: bool, base_italic: bool) -> bool {
        self.fg == SpanColor::Tab && self.bg == SpanColor::Tab && self.bold == base_bold && self.italic == base_italic
    }
}

/// 틀에 끼울 값. 글은 모두 믿지 않는 것이라 제어 문자를 지우고 끼운다
#[derive(Debug, Clone, Default)]
pub struct TitleData<'a> {
    pub title: &'a str,
    /// 탭 번호(1부터)
    pub index: usize,
    pub num_windows: usize,
    pub bell: bool,
    pub activity: bool,
    /// 고른 칸의 작업 폴더(OSC 7). 모르면 빈 글자
    pub active_wd: &'a str,
    /// 고른 칸의 전경 프로세스 이름. 모르면 빈 글자
    pub active_exe: &'a str,
    /// 가장 최근에 고른 칸의 진행 표시(OSC 9;4): (상태, 퍼센트)
    pub last_progress: Option<(u8, u8)>,
    /// 탭 칸들의 진행 표시 퍼센트 평균. 칸이 하나뿐이면 `last_progress`와 같은 글
    pub all_progress: Option<(u8, u8)>,
    /// 진행 표시가 있는 칸 수
    pub progress_panes: usize,
}

/// 틀 조각
#[derive(Debug, Clone, PartialEq, Eq)]
enum Piece {
    Lit(String),
    Field(Field),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Field {
    Title,
    Index,
    NumWindows,
    Bell,
    Activity,
    MaxTitleLength,
    ActiveWd,
    ActiveExe,
    LastProgress,
    AllProgress,
    /// 우리에게 값이 없는 이름: 빈 글자(`layout_name`·`session_name`·`keyboard_mode`·`custom`…)
    Empty,
    Sup(String),
    Sub(String),
    Fmt(Fmt),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Fmt {
    Reset,
    Bold,
    NoBold,
    Italic,
    NoItalic,
    Fg(SpanColor),
    Bg(SpanColor),
}

/// 필드 이름 하나를 읽는다. 모르는 이름이면 `None`(kitty는 이름 오류로 제목을 그대로 보인다)
fn field(name: &str) -> Option<Field> {
    let name = name.trim();
    Some(match name {
        "title" => Field::Title,
        "index" => Field::Index,
        "num_windows" | "num_window_groups" => Field::NumWindows,
        "bell_symbol" => Field::Bell,
        "activity_symbol" => Field::Activity,
        "max_title_length" => Field::MaxTitleLength,
        "tab.active_wd" => Field::ActiveWd,
        "tab.active_exe" => Field::ActiveExe,
        "tab.last_focused_progress_percent" => Field::LastProgress,
        "tab.progress_percent" => Field::AllProgress,
        // 우리에게 값이 없다: 빈 글자
        "layout_name"
        | "session_name"
        | "active_session_name"
        | "keyboard_mode"
        | "secure_input_symbol"
        | "custom"
        | "tab.active_oldest_wd"
        | "tab.active_oldest_exe"
        | "tab.program_status"
        | "tab.children_mem_usage" => Field::Empty,
        "fmt.reset" => Field::Fmt(Fmt::Reset),
        "fmt.bold" => Field::Fmt(Fmt::Bold),
        "fmt.nobold" => Field::Fmt(Fmt::NoBold),
        "fmt.italic" => Field::Fmt(Fmt::Italic),
        "fmt.noitalic" => Field::Fmt(Fmt::NoItalic),
        _ => {
            if let Some(c) = name.strip_prefix("fmt.fg.") {
                Field::Fmt(Fmt::Fg(fmt_color(c)?))
            } else if let Some(c) = name.strip_prefix("fmt.bg.") {
                Field::Fmt(Fmt::Bg(fmt_color(c)?))
            } else if let Some(n) = name.strip_prefix("sup.") {
                Field::Sup(ident(n)?)
            } else {
                Field::Sub(ident(name.strip_prefix("sub.")?)?)
            }
        }
    })
}

/// 파이썬 이름 하나(글자·숫자·밑줄)
fn ident(n: &str) -> Option<String> {
    let ok = !n.is_empty()
        && !n.starts_with(|c: char| c.is_ascii_digit())
        && n.chars().all(|c| c == '_' || c.is_alphanumeric());
    ok.then(|| n.to_string())
}

/// kitty `ColorFormatter`: `default`·`tab`·`color0`~`color255`·`_RRGGBB`(= `#RRGGBB`)·색 이름
fn fmt_color(name: &str) -> Option<SpanColor> {
    match name {
        "default" => Some(SpanColor::Default),
        "tab" => Some(SpanColor::Tab),
        _ => {
            if let Some(n) = name.strip_prefix("color") {
                if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) {
                    return n.parse::<u8>().ok().map(SpanColor::Indexed);
                }
            }
            let q = match name.strip_prefix('_') {
                Some(hex) => format!("#{hex}"),
                None => name.to_string(),
            };
            ident(name)?;
            Rgb::parse_color(&q).map(SpanColor::Rgb)
        }
    }
}

/// 틀을 읽는다. 파이썬 f-문자열처럼 `{{`·`}}`는 중괄호 글자다. 짝이 안 맞거나, 이름이 아닌 것(표현식·`!r`·`:형식`·모르는 이름)이
/// 들어 있으면 `None`이다. 이 글을 다시 해석하는 일은 없다
fn parse(template: &str) -> Option<Vec<Piece>> {
    let mut out = Vec::new();
    let mut lit = String::new();
    let mut it = template.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' if it.peek() == Some(&'{') => {
                it.next();
                lit.push('{');
            }
            '}' if it.peek() == Some(&'}') => {
                it.next();
                lit.push('}');
            }
            '}' => return None,
            '{' => {
                let mut name = String::new();
                loop {
                    match it.next() {
                        Some('}') => break,
                        Some('{') | None => return None,
                        Some(ch) => name.push(ch),
                    }
                }
                if !lit.is_empty() {
                    out.push(Piece::Lit(std::mem::take(&mut lit)));
                }
                out.push(Piece::Field(field(&name)?));
            }
            c => lit.push(c),
        }
    }
    if !lit.is_empty() {
        out.push(Piece::Lit(lit));
    }
    Some(out)
}

const SUP: &str = "ᵃᵇᶜᵈᵉᶠᵍʰⁱʲᵏˡᵐⁿᵒᵖqʳˢᵗᵘᵛʷˣʸᶻᴬᴮᶜᴰᴱᶠᴳᴴᴵᴶᴷᴸᴹᴺᴼᴾQᴿˢᵀᵁⱽᵂˣʸᶻ⁰¹²³⁴⁵⁶⁷⁸⁹⁺⁻⁼⁽⁾";
const SUB: &str = "ₐbcdₑfgₕᵢⱼₖₗₘₙₒₚqᵣₛₜᵤᵥwₓyzₐbcdₑfgₕᵢⱼₖₗₘₙₒₚqᵣₛₜᵤᵥwₓyz₀₁₂₃₄₅₆₇₈₉₊₋₌₍₎";
const SUPSUB_FROM: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789+-=()";

/// kitty `SupSub`: 글자마다 위첨자·아래첨자 글자로 바꾼다(표에 없는 글자는 그대로)
pub fn super_sub(s: &str, subscript: bool) -> String {
    let to: Vec<char> = if subscript { SUB } else { SUP }.chars().collect();
    s.chars()
        .map(|c| SUPSUB_FROM.chars().position(|f| f == c).map_or(c, |i| to[i]))
        .collect()
}

/// 믿지 않는 글에서 제어 문자(C0·DEL·C1·줄바꿈)를 지운다
pub fn clean(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

/// kitty `tab.last_focused_progress_percent`의 글(`  `처럼 글꼴이 없을 수 있는 기호는 쓰지 않는다):
/// 보통은 `42% `, 오류는 `! `, 끝 모름은 `… `, 멈춤은 `|| 42% `. 상태 7(값 없는 오류)은 오류다. 없으면 빈 글자
pub fn progress_text(p: Option<(u8, u8)>) -> String {
    match p {
        None => String::new(),
        Some((2 | 7, _)) => "! ".into(),
        Some((3, _)) => "… ".into(),
        Some((4, pct)) => format!("|| {pct}% "),
        Some((_, pct)) => format!("{pct}% "),
    }
}

struct Eval<'a> {
    data: &'a TitleData<'a>,
    cfg: &'a TabTitleConfig,
    spans: Vec<Span>,
    cur: Span,
}

impl Eval<'_> {
    fn text(&mut self, s: &str) {
        if s.is_empty() {
            return;
        }
        match self.spans.last_mut() {
            Some(last)
                if last.fg == self.cur.fg
                    && last.bg == self.cur.bg
                    && last.bold == self.cur.bold
                    && last.italic == self.cur.italic =>
            {
                last.text.push_str(s);
            }
            _ => self.spans.push(Span {
                text: s.to_string(),
                ..self.cur.clone()
            }),
        }
    }

    fn value(&self, f: &Field) -> String {
        let d = self.data;
        match f {
            Field::Title => clean(d.title),
            Field::Index => d.index.to_string(),
            Field::NumWindows => d.num_windows.to_string(),
            Field::Bell => {
                if d.bell {
                    self.cfg.bell_symbol.clone()
                } else {
                    String::new()
                }
            }
            Field::Activity => {
                if d.activity {
                    self.cfg.activity_symbol.clone()
                } else {
                    String::new()
                }
            }
            Field::MaxTitleLength => self.cfg.max_length.to_string(),
            Field::ActiveWd => clean(d.active_wd),
            Field::ActiveExe => clean(d.active_exe),
            Field::LastProgress => progress_text(d.last_progress),
            Field::AllProgress => progress_text(if d.progress_panes <= 1 {
                d.last_progress
            } else {
                d.all_progress
            }),
            Field::Empty | Field::Fmt(_) => String::new(),
            Field::Sup(n) | Field::Sub(n) => {
                let sub = matches!(f, Field::Sub(_));
                let raw = match n.as_str() {
                    "index" => d.index.to_string(),
                    "num_windows" | "num_window_groups" => d.num_windows.to_string(),
                    "title" => clean(d.title),
                    "layout_name" => String::new(),
                    other => other.to_string(),
                };
                super_sub(&raw, sub)
            }
        }
    }

    fn run(&mut self, pieces: &[Piece]) {
        for p in pieces {
            match p {
                Piece::Lit(s) => self.text(s),
                Piece::Field(Field::Fmt(f)) => match f {
                    Fmt::Reset => {
                        self.cur.fg = SpanColor::Default;
                        self.cur.bg = SpanColor::Default;
                        self.cur.bold = false;
                        self.cur.italic = false;
                    }
                    Fmt::Bold => self.cur.bold = true,
                    Fmt::NoBold => self.cur.bold = false,
                    Fmt::Italic => self.cur.italic = true,
                    Fmt::NoItalic => self.cur.italic = false,
                    Fmt::Fg(c) => self.cur.fg = *c,
                    Fmt::Bg(c) => self.cur.bg = *c,
                },
                Piece::Field(f) => {
                    let v = self.value(f);
                    self.text(&v);
                }
            }
        }
    }
}

/// 탭 하나의 제목을 틀로 그린다: 꾸밈이 같은 글끼리 묶은 조각들. `base_bold`·`base_italic`은 탭 글꼴 모양(처음 상태)이다.
/// 틀을 읽지 못하면(설정은 읽을 때 거르지만 방어로) 제목만 한 조각으로 돌려준다.
/// kitty처럼 `{bell_symbol}`·`{activity_symbol}`이 값이 있는데 틀에 없으면 앞에 붙인다(빨강 꾸밈으로 감싼다)
pub fn render(cfg: &TabTitleConfig, active: bool, data: &TitleData, base_bold: bool, base_italic: bool) -> Vec<Span> {
    let cur = Span {
        text: String::new(),
        fg: SpanColor::Tab,
        bg: SpanColor::Tab,
        bold: base_bold,
        italic: base_italic,
    };
    let Some(mut pieces) = parse(cfg.template_for(active)) else {
        let mut e = Eval {
            data,
            cfg,
            spans: Vec::new(),
            cur,
        };
        let t = clean(data.title);
        e.text(&t);
        return e.spans;
    };
    let has = |pieces: &[Piece], f: &Field| pieces.iter().any(|p| matches!(p, Piece::Field(x) if x == f));
    let mut prefix = Vec::new();
    if data.bell && !cfg.bell_symbol.is_empty() && !has(&pieces, &Field::Bell) {
        prefix.push(Piece::Field(Field::Bell));
    }
    if data.activity && !cfg.activity_symbol.is_empty() && !has(&pieces, &Field::Activity) {
        prefix.push(Piece::Field(Field::Activity));
    }
    if !prefix.is_empty() {
        let mut head = vec![Piece::Field(Field::Fmt(Fmt::Fg(SpanColor::Rgb(Rgb::new(255, 0, 0)))))];
        head.extend(prefix);
        head.push(Piece::Field(Field::Fmt(Fmt::Fg(SpanColor::Tab))));
        head.append(&mut pieces);
        pieces = head;
    }
    let mut e = Eval {
        data,
        cfg,
        spans: Vec::new(),
        cur,
    };
    e.run(&pieces);
    e.spans
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data(title: &str) -> TitleData<'_> {
        TitleData {
            title,
            index: 1,
            num_windows: 2,
            ..TitleData::default()
        }
    }

    fn flat(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    fn cfg(template: &str) -> TabTitleConfig {
        TabTitleConfig {
            template: template.to_string(),
            ..TabTitleConfig::default()
        }
    }

    #[test]
    fn default_template_is_the_current_title() {
        let c = TabTitleConfig::default();
        let s = render(&c, false, &data("zsh"), false, false);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].text, "zsh");
        assert!(s[0].is_plain(false, false));
        // 벨이 있으면 지금처럼 앞에 `🔔 `, 같은 조각이다(빨강 꾸밈이 붙지 않는다)
        let mut d = data("zsh");
        d.bell = true;
        let s = render(&c, true, &d, false, false);
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].text, "🔔 zsh");
        // 활동 기호는 기본이 없다
        d.activity = true;
        assert_eq!(flat(&render(&c, true, &d, false, false)), "🔔 zsh");
    }

    #[test]
    fn index_and_title_template_renders_like_the_issue() {
        let c = cfg("{index}: {title}");
        assert_eq!(flat(&render(&c, false, &data("zsh"), false, false)), "1: zsh");
        let c = cfg("{sup.index}{title}{{}}");
        assert_eq!(flat(&render(&c, false, &data("a"), false, false)), "¹a{}");
        assert_eq!(super_sub("12ab", true), "₁₂ₐb");
        assert_eq!(SUP.chars().count(), 67);
        assert_eq!(SUB.chars().count(), 67);
        assert_eq!(SUPSUB_FROM.chars().count(), 67);
    }

    /// 프로그램이 보낸 제목은 값일 뿐이다: 안의 꾸밈·중괄호를 다시 해석하지 않는다
    #[test]
    fn program_title_is_never_reinterpreted() {
        for t in [
            "{fmt.fg.red}x",
            "{fmt.bold}{title}{{}}",
            "}{ {index} {bell_symbol}",
            "\x1b[31mx\x07",
        ] {
            let c = cfg("[{title}]");
            let s = render(&c, false, &data(t), false, false);
            assert_eq!(s.len(), 1, "{t:?}: {s:?}");
            assert!(s[0].is_plain(false, false), "{t:?}");
            assert_eq!(s[0].text, format!("[{}]", clean(t)), "{t:?}");
        }
        // 기본 틀에서도
        let s = render(&TabTitleConfig::default(), false, &data("{fmt.fg.red}x"), false, false);
        assert_eq!(
            s,
            vec![Span {
                text: "{fmt.fg.red}x".into(),
                fg: SpanColor::Tab,
                bg: SpanColor::Tab,
                bold: false,
                italic: false
            }]
        );
        // 작업 폴더·실행 파일 이름도 값이다
        let mut d = data("t");
        d.active_wd = "/tmp/{fmt.bold}";
        d.active_exe = "a\nb";
        let s = render(&cfg("{tab.active_wd} {tab.active_exe}"), false, &d, false, false);
        assert_eq!(flat(&s), "/tmp/{fmt.bold} ab");
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn fmt_pieces_change_color_and_font() {
        let c = cfg(
            "{fmt.fg.red}a{fmt.fg.tab}{fmt.bold}b{fmt.nobold}{fmt.bg._00ff00}c{fmt.reset}d{fmt.fg.color9}e{fmt.italic}f",
        );
        let s = render(&c, false, &data("t"), false, false);
        let red = SpanColor::Rgb(Rgb::new(255, 0, 0));
        assert_eq!(
            s[0],
            Span {
                text: "a".into(),
                fg: red,
                bg: SpanColor::Tab,
                bold: false,
                italic: false
            }
        );
        assert_eq!(s[1].text, "b");
        assert!(s[1].bold && s[1].fg == SpanColor::Tab);
        assert_eq!(s[2].bg, SpanColor::Rgb(Rgb::new(0, 255, 0)));
        assert_eq!(
            (s[3].text.as_str(), s[3].fg, s[3].bg),
            ("d", SpanColor::Default, SpanColor::Default)
        );
        assert_eq!(s[4].fg, SpanColor::Indexed(9));
        assert!(s[5].italic);
        // 탭 글꼴 모양이 처음 상태이고 `fmt.nobold`가 끈다
        let s = render(&cfg("a{fmt.nobold}b"), false, &data("t"), true, false);
        assert!(s[0].bold && !s[1].bold);
    }

    #[test]
    fn unknown_names_and_expressions_are_rejected_when_read() {
        let mut c = TabTitleConfig::default();
        for bad in [
            "\"{layout_name[:2].upper()}\"",
            "\"{nosuchname}\"",
            "\"{title!r}\"",
            "\"{title:>5}\"",
            "\"{fmt.fg.nosuchcolor}\"",
            "\"{title\"",
            "\"title}\"",
            "\"{tab.nosuch}\"",
            "\"{1+1}\"",
        ] {
            assert_eq!(c.apply("tab-title-template", bad), Some(false), "{bad}");
            assert_eq!(c, TabTitleConfig::default(), "{bad}");
        }
        // 값이 없는 이름은 빈 글자
        assert_eq!(
            c.apply("tab-title-template", "\"{layout_name}{custom}{title}\""),
            Some(true)
        );
        assert_eq!(flat(&render(&c, false, &data("zsh"), false, false)), "zsh");
        // 읽지 못하는 틀이 어떻게든 들어와도 그릴 때는 제목만
        let broken = cfg("{title!r}");
        assert_eq!(flat(&render(&broken, false, &data("zsh"), false, false)), "zsh");
    }

    #[test]
    fn missing_bell_and_activity_symbols_are_prepended_in_red_like_kitty() {
        let mut c = cfg("{index}:{title}");
        c.activity_symbol = "*".into();
        let mut d = data("zsh");
        d.bell = true;
        d.activity = true;
        let s = render(&c, false, &d, false, false);
        assert_eq!(flat(&s), "🔔 *1:zsh");
        assert_eq!(s[0].fg, SpanColor::Rgb(Rgb::new(255, 0, 0)));
        assert_eq!(s[1].fg, SpanColor::Tab);
        // 틀에 있으면 붙이지 않는다
        let mut c2 = cfg("{activity_symbol}{title}");
        c2.activity_symbol = "*".into();
        d.bell = true;
        // 벨 기호는 틀에 없으니 앞에
        assert_eq!(flat(&render(&c2, false, &d, false, false)), "🔔 *zsh");
        // 벨이 없으면 아무것도 붙지 않는다
        d.bell = false;
        d.activity = false;
        assert_eq!(flat(&render(&c, false, &d, false, false)), "1:zsh");
    }

    #[test]
    fn active_template_applies_only_to_the_active_tab() {
        let mut c = cfg("{title}");
        assert_eq!(
            c.apply("active-tab-title-template", "\"[{index}] {title}\""),
            Some(true)
        );
        assert_eq!(flat(&render(&c, true, &data("a"), false, false)), "[1] a");
        assert_eq!(flat(&render(&c, false, &data("a"), false, false)), "a");
        assert_eq!(c.apply("active-tab-title-template", "none"), Some(true));
        assert_eq!(flat(&render(&c, true, &data("a"), false, false)), "a");
    }

    #[test]
    fn keys_parse_like_kitty() {
        let mut c = TabTitleConfig::default();
        assert_eq!(c.apply("bell-on-tab", "no"), Some(true));
        assert_eq!(c.bell_symbol, "");
        assert_eq!(c.apply("bell-on-tab", "Yes"), Some(true));
        assert_eq!(c.bell_symbol, "🔔 ");
        assert_eq!(c.apply("bell-on-tab", "\"! \""), Some(true));
        assert_eq!(c.bell_symbol, "! ");
        assert_eq!(c.apply("bell-on-tab", "none"), Some(true));
        assert_eq!(c.bell_symbol, "");
        assert_eq!(c.apply("tab-activity-symbol", "\"* \""), Some(true));
        assert_eq!(c.activity_symbol, "* ");
        assert_eq!(c.apply("tab-activity-symbol", "none"), Some(true));
        assert_eq!(c.activity_symbol, "");
        assert_eq!(c.apply("tab-title-max-length", "12"), Some(true));
        assert_eq!(c.max_length, 12);
        assert_eq!(c.apply("tab-title-max-length", "-1"), Some(false));
        assert_eq!(c.apply("tab-title-max-length", "x"), Some(false));
        assert_eq!(c.max_length, 12);
        assert_eq!(c.apply("tab-title-template", "'{index} {title}'"), Some(true));
        assert_eq!(c.template, "{index} {title}");
        assert_eq!(c.apply("tab-title-template", ""), Some(true));
        assert_eq!(c.template, DEFAULT_TEMPLATE);
        assert_eq!(c.apply("tab-title-template", "\"\""), Some(true));
        assert_eq!(c.template, "");
        assert!(render(&c, false, &data("zsh"), false, false).is_empty());
        assert_eq!(c.apply("something-else", "x"), None);
    }

    #[test]
    fn progress_fields() {
        let mut d = data("t");
        d.last_progress = Some((1, 42));
        d.all_progress = Some((1, 30));
        d.progress_panes = 2;
        let s = render(
            &cfg("{tab.last_focused_progress_percent}|{tab.progress_percent}"),
            false,
            &d,
            false,
            false,
        );
        assert_eq!(flat(&s), "42% |30% ");
        d.last_progress = Some((2, 0));
        assert_eq!(
            flat(&render(
                &cfg("{tab.last_focused_progress_percent}"),
                false,
                &d,
                false,
                false
            )),
            "! "
        );
        d.last_progress = None;
        assert_eq!(
            flat(&render(
                &cfg("{tab.last_focused_progress_percent}"),
                false,
                &d,
                false,
                false
            )),
            ""
        );
    }

    #[test]
    fn config_reads_and_prints_back_and_warns_on_bad_values() {
        use crate::config::Config;
        let text = "tab-title-template = \"{index}: {title}\"\nactive-tab-title-template = \"{fmt.bold}{title}\"\n\
                    bell-on-tab = \"! \"\ntab-activity-symbol = \"* \"\ntab-title-max-length = 20\n";
        let c = Config::from_text(text, true);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.tab_title.template, "{index}: {title}");
        assert_eq!(c.tab_title.active_template.as_deref(), Some("{fmt.bold}{title}"));
        assert_eq!(
            (c.tab_title.bell_symbol.as_str(), c.tab_title.activity_symbol.as_str()),
            ("! ", "* ")
        );
        assert_eq!(c.tab_title.max_length, 20);
        let again = Config::from_text(&c.render(), true);
        assert!(again.warnings.is_empty(), "{:?}", again.warnings);
        assert_eq!(again.tab_title, c.tab_title);
        assert_eq!(Config::from_text("", true).tab_title, TabTitleConfig::default());
        for line in [
            "tab-title-template = \"{nosuch}\"",
            "active-tab-title-template = \"{1+1}\"",
            "tab-title-max-length = many",
        ] {
            let c = Config::from_text(&format!("{line}\n"), true);
            assert_eq!(c.warnings.len(), 1, "{line}: {:?}", c.warnings);
            assert_eq!(c.tab_title, TabTitleConfig::default(), "{line}");
        }
    }
}
