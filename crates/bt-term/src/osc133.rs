//! OSC 133(의미 있는 프롬프트) 파서와 엔진 적용(#329).
//! 기준: Ghostty `osc/parsers/semantic_prompt.zig`(동작 8개 A·B·C·D·I·L·N·P, 옵션 읽기 규칙)와 `Terminal.semanticPrompt`.
//!
//! 옵션은 `;`로 나눈 `키=값`이고, 모르는 것·잘못된 것은 무시한다. 같은 키가 둘이면 먼저 나온 것을 읽고, 값이 틀렸으면 없는 것으로 본다
//! (Ghostty `Option.read`). `click_events`와 `cl`은 읽기만 하고 아무 동작도 하지 않는다(프롬프트 클릭은 `prompt_click` 모듈이 쓴다, #338).

use alacritty_terminal::event::EventListener;
use alacritty_terminal::term::Term;
pub use alacritty_terminal::term::semantic::{PromptKind, Redraw};

/// OSC 133 동작 글자
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// L
    FreshLine,
    /// A
    FreshLineNewPrompt,
    /// N
    NewCommand,
    /// P
    PromptStart,
    /// B
    EndPromptStartInput,
    /// I
    EndPromptStartInputTerminateEol,
    /// C
    EndInputStartOutput,
    /// D
    EndCommand,
}

pub use alacritty_terminal::term::semantic::{Click, ClickEvents, SemanticClick};

/// 읽어 낸 OSC 133 명령 하나
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticPrompt {
    pub action: Action,
    /// `aid=`. 길이를 자르고 제어 문자를 지운다. 지금은 쓰지 않는다(Ghostty도 의미 있게 쓰지 않는다)
    pub aid: Option<String>,
    /// `k=`
    pub kind: Option<PromptKind>,
    /// `redraw=`
    pub redraw: Option<Redraw>,
    pub click_events: Option<ClickEvents>,
    pub cl: Option<Click>,
    /// D의 종료 코드(첫 필드)
    pub exit_code: Option<i32>,
}

const MAX_AID: usize = 256;

/// `133;` 뒤의 글(예: `A;aid=14;cl=line`)을 읽는다. 잘못되면 None
pub fn parse(data: &str) -> Option<SemanticPrompt> {
    let (head, options) = match data.split_once(';') {
        Some((head, options)) => (head, options),
        None => (data, ""),
    };
    // 동작은 한 글자다. L은 옵션을 받지 않는다(`L;`도 잘못된 것이다)
    let action = match head {
        "A" => Action::FreshLineNewPrompt,
        "B" => Action::EndPromptStartInput,
        "C" => Action::EndInputStartOutput,
        "D" => Action::EndCommand,
        "I" => Action::EndPromptStartInputTerminateEol,
        "N" => Action::NewCommand,
        "P" => Action::PromptStart,
        "L" if data.len() == 1 => Action::FreshLine,
        _ => return None,
    };
    Some(SemanticPrompt {
        action,
        aid: read_option(options, "aid").map(clean_aid),
        kind: read_option(options, "k").and_then(|v| match v {
            "i" => Some(PromptKind::Initial),
            "r" => Some(PromptKind::Right),
            "c" => Some(PromptKind::Continuation),
            "s" => Some(PromptKind::Secondary),
            _ => None,
        }),
        redraw: read_option(options, "redraw").and_then(|v| match v {
            "0" => Some(Redraw::False),
            "1" => Some(Redraw::True),
            "last" => Some(Redraw::Last),
            _ => None,
        }),
        click_events: read_option(options, "click_events").and_then(|v| match v {
            "1" => Some(ClickEvents::Absolute),
            "2" => Some(ClickEvents::Relative),
            _ => None,
        }),
        cl: read_option(options, "cl").and_then(|v| match v {
            "line" => Some(Click::Line),
            "m" => Some(Click::Multiple),
            "v" => Some(Click::ConservativeVertical),
            "w" => Some(Click::SmartVertical),
            _ => None,
        }),
        exit_code: (action == Action::EndCommand)
            .then(|| options.split(';').next().and_then(|f| f.parse().ok()))
            .flatten(),
    })
}

/// 키가 맞는 첫 필드의 값을 돌려준다. `=` 없는 필드와 다른 키는 건너뛴다(Ghostty `Option.read`).
/// 값이 틀린지는 부르는 쪽이 본다: 첫 필드가 틀리면 뒤의 같은 키는 보지 않는다
fn read_option<'a>(options: &'a str, key: &str) -> Option<&'a str> {
    options
        .split(';')
        .filter_map(|field| field.split_once('='))
        .find(|(k, _)| *k == key)
        .map(|(_, v)| v)
}

/// 믿을 수 없는 글이라 제어 문자를 공백으로 바꾸고 길이를 자른다
fn clean_aid(v: &str) -> String {
    let s: String = v.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    s.trim().chars().take(MAX_AID).collect()
}

impl SemanticPrompt {
    /// `click_events=`가 먼저, 없거나 틀렸으면 `cl=`(Ghostty `semanticPrompt`의 `click:` 블록). 둘 다 없으면 None
    fn click(&self) -> Option<SemanticClick> {
        self.click_events.map(SemanticClick::ClickEvents).or(self.cl.map(SemanticClick::Cl))
    }

    /// 엔진에 적용한다(Ghostty `Terminal.semanticPrompt`, 2784~2902줄).
    /// N은 A와 같다(Ghostty도 열린 명령을 따로 세지 않는다). `click_events`·`cl`은 쓰지 않는다
    pub fn apply<T: EventListener>(&self, term: &mut Term<T>) {
        match self.action {
            Action::FreshLine => term.semantic_fresh_line(),
            Action::FreshLineNewPrompt | Action::NewCommand => term.semantic_new_prompt(self.kind, self.redraw, self.click()),
            Action::PromptStart => term.semantic_prompt_start(self.kind),
            Action::EndPromptStartInput => term.semantic_input(false),
            Action::EndPromptStartInputTerminateEol => term.semantic_input(true),
            Action::EndInputStartOutput => term.semantic_end_input_start_output(),
            Action::EndCommand => term.semantic_end_command(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(data: &str) -> SemanticPrompt {
        parse(data).unwrap_or_else(|| panic!("읽혀야 한다: {data:?}"))
    }

    #[test]
    fn eight_actions_are_read() {
        let cases = [
            ("A", Action::FreshLineNewPrompt),
            ("B", Action::EndPromptStartInput),
            ("C", Action::EndInputStartOutput),
            ("D", Action::EndCommand),
            ("I", Action::EndPromptStartInputTerminateEol),
            ("L", Action::FreshLine),
            ("N", Action::NewCommand),
            ("P", Action::PromptStart),
        ];
        for (text, action) in cases {
            assert_eq!(p(text).action, action, "{text}");
        }
    }

    #[test]
    fn extra_contents_and_unknown_actions_are_invalid() {
        // Ghostty: "end_input_start_output extra contents", "fresh_line extra contents"
        for bad in [
            "", "Cx", "Ax", "Bx", "Dx", "Ix", "Nx", "Px", "Lx", "L;", "L;a=b", "Z", "a", "AA",
        ] {
            assert_eq!(parse(bad), None, "{bad:?}");
        }
        // 동작 글자만 있거나 ; 뒤에 옵션이 오면 된다
        assert!(parse("A;").is_some());
        assert!(parse("C;aid=foo").is_some());
    }

    #[test]
    fn aid_keeps_equals_and_ignores_bare_keys() {
        assert_eq!(p("A").aid, None);
        assert_eq!(p("A;aid=14").aid.as_deref(), Some("14"));
        assert_eq!(p("A;aid=a=b").aid.as_deref(), Some("a=b"));
        assert_eq!(p("C;aid=foo").aid.as_deref(), Some("foo"));
        assert_eq!(p("A;aid").aid, None, "= 없는 키");
        assert_eq!(p("A;aid;cl=line").cl, Some(Click::Line));
    }

    #[test]
    fn aid_is_cleaned_and_capped() {
        let long = format!("A;aid={}", "x".repeat(1000));
        assert!(p(&long).aid.unwrap().chars().count() <= MAX_AID);
        assert_eq!(p("A;aid=a\u{7}b").aid.as_deref(), Some("a b"));
    }

    #[test]
    fn cl_values() {
        assert_eq!(p("A;cl=line").cl, Some(Click::Line));
        assert_eq!(p("A;cl=m").cl, Some(Click::Multiple));
        assert_eq!(p("A;cl=v").cl, Some(Click::ConservativeVertical));
        assert_eq!(p("A;cl=w").cl, Some(Click::SmartVertical));
        assert_eq!(p("A;cl=invalid").cl, None);
        assert_eq!(p("A;aid=foo;cl=line").aid.as_deref(), Some("foo"));
        assert_eq!(p("A;aid=foo;cl=line").cl, Some(Click::Line));
    }

    #[test]
    fn click_events_values() {
        assert_eq!(p("A;click_events=1").click_events, Some(ClickEvents::Absolute));
        assert_eq!(p("A;click_events=2").click_events, Some(ClickEvents::Relative));
        assert_eq!(p("A;click_events=3").click_events, None);
        assert_eq!(p("A;click_events=12").click_events, None);
    }

    #[test]
    fn redraw_values() {
        assert_eq!(p("A").redraw, None);
        assert_eq!(p("A;redraw=0").redraw, Some(Redraw::False));
        assert_eq!(p("A;redraw=1").redraw, Some(Redraw::True));
        assert_eq!(p("A;redraw=last").redraw, Some(Redraw::Last));
        assert_eq!(p("A;redraw=2").redraw, None);
        assert_eq!(p("A;redraw=").redraw, None);
        assert_eq!(p("N;redraw=last").redraw, Some(Redraw::Last), "N도 A처럼 읽는다");
    }

    #[test]
    fn prompt_kinds() {
        assert_eq!(p("P").kind, None);
        assert_eq!(p("P;k=i").kind, Some(PromptKind::Initial));
        assert_eq!(p("P;k=r").kind, Some(PromptKind::Right));
        assert_eq!(p("P;k=c").kind, Some(PromptKind::Continuation));
        assert_eq!(p("P;k=s").kind, Some(PromptKind::Secondary));
        assert_eq!(p("P;k=x").kind, None);
        assert_eq!(p("P;k=ii").kind, None, "한 글자만 받는다");
        assert_eq!(p("A;k=c").kind, Some(PromptKind::Continuation));
        assert_eq!(p("N;aid=1;k=s").kind, Some(PromptKind::Secondary));
    }

    #[test]
    fn first_matching_key_wins_even_when_invalid() {
        // Ghostty `Option.read`: 키가 맞는 첫 필드의 값이 틀리면 없는 것으로 본다(뒤의 같은 키는 보지 않는다)
        assert_eq!(p("A;k=zz;k=i").kind, None);
        assert_eq!(p("A;k=i;k=c").kind, Some(PromptKind::Initial));
    }

    #[test]
    fn trailing_semicolons_and_unknown_options_are_ignored() {
        assert_eq!(p("A;").kind, None);
        assert_eq!(p("A;;;k=c;").kind, Some(PromptKind::Continuation));
        assert_eq!(p("A;foo=bar;k=c").kind, Some(PromptKind::Continuation));
    }

    #[test]
    fn exit_code_is_the_first_field_of_d() {
        assert_eq!(p("D").exit_code, None);
        assert_eq!(p("D;0").exit_code, Some(0));
        assert_eq!(p("D;127").exit_code, Some(127));
        assert_eq!(p("D;-1").exit_code, Some(-1));
        assert_eq!(p("D;abc").exit_code, None);
        assert_eq!(p("D;3;aid=x").exit_code, Some(3));
        assert_eq!(p("D;3;aid=x").aid.as_deref(), Some("x"));
        assert_eq!(p("D;;aid=x").exit_code, None);
    }
}
