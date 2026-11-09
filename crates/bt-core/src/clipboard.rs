//! 프로그램의 OSC 52 클립보드 읽기·쓰기를 어떻게 할지 가르는 판단(#220, ADR-019). 창을 모른다

use std::time::Duration;

/// `clipboard-read`·`clipboard-write` 값(Ghostty와 같다)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardAccess {
    Allow,
    Deny,
    /// 프로그램이 할 때마다 창 안 확인 막대로 묻는다
    Ask,
}

impl ClipboardAccess {
    /// 설정 값 해석. 모르는 값은 없다
    pub fn parse(v: &str) -> Option<Self> {
        match v {
            "allow" | "true" => Some(Self::Allow),
            "deny" | "false" => Some(Self::Deny),
            "ask" => Some(Self::Ask),
            _ => None,
        }
    }

    /// VT 엔진이 OSC 52 사건을 앱에 넘겨야 하는가(deny면 엔진이 거기서 버린다)
    pub fn engine_enabled(self) -> bool {
        self != Self::Deny
    }
}

/// 요청 하나에 대한 판단
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipVerdict {
    Allow,
    Ask,
    Deny,
}

/// 막대의 허용 키가 듣기 시작하는 때까지(출력이 막대를 띄운 순간 사람이 치던 키가 허용이 되지 않게)
pub const CLIP_ARM_DELAY: Duration = Duration::from_millis(600);

/// 막대에 보이는 앞부분 글자 수
pub const CLIP_PREVIEW_CHARS: usize = 40;

/// 프로그램이 OSC 52로 클립보드에 쓰려 할 때. `watching`은 보고 있는 창의 고른 칸에서 온 것인가,
/// `bar_busy`는 이미 확인 막대가 떠 있는가(쌓지 않고 거절한다)
pub fn decide_write(
    access: ClipboardAccess,
    watching: bool,
    len: usize,
    limit: Option<usize>,
    bar_busy: bool,
) -> ClipVerdict {
    if !watching || !crate::config::clipboard_write_allowed(len, limit) {
        return ClipVerdict::Deny;
    }
    match access {
        ClipboardAccess::Allow => ClipVerdict::Allow,
        ClipboardAccess::Deny => ClipVerdict::Deny,
        ClipboardAccess::Ask if bar_busy => ClipVerdict::Deny,
        ClipboardAccess::Ask => ClipVerdict::Ask,
    }
}

/// 프로그램이 OSC 52로 클립보드를 읽으려 할 때. `readonly`면 답이 가지 않으므로 묻지도 않는다
pub fn decide_read(access: ClipboardAccess, watching: bool, readonly: bool, bar_busy: bool) -> ClipVerdict {
    match access {
        ClipboardAccess::Allow => ClipVerdict::Allow,
        ClipboardAccess::Deny => ClipVerdict::Deny,
        ClipboardAccess::Ask if !watching || readonly || bar_busy => ClipVerdict::Deny,
        ClipboardAccess::Ask => ClipVerdict::Ask,
    }
}

/// 막대 위에서 친 키
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerKey {
    Enter,
    Escape,
    Char(char),
}

/// 클립보드 막대에서 키 하나의 뜻: `Some(true)` 허용, `Some(false)` 거절, `None`은 삼키기만 한다.
/// 허용은 장식 없는 `y`를, 반복 입력이 아니고 막대가 뜬 지 `CLIP_ARM_DELAY`가 지난 뒤에만 받는다. Enter는 허용이 아니다
pub fn clipboard_answer(key: AnswerKey, plain: bool, repeat: bool, shown_for: Duration) -> Option<bool> {
    match key {
        AnswerKey::Escape => Some(false),
        AnswerKey::Char(c) if c.eq_ignore_ascii_case(&'n') => Some(false),
        AnswerKey::Char(c) if c.eq_ignore_ascii_case(&'y') && plain && !repeat && shown_for >= CLIP_ARM_DELAY => {
            Some(true)
        }
        _ => None,
    }
}

/// 제어 문자를 빈칸으로 바꾸고 앞의 `max`글자만. 잘렸으면 끝에 `…`
pub fn clip_preview(text: &str, max: usize) -> String {
    let mut out: String = crate::proto::printable(text).chars().take(max).collect();
    if text.chars().nth(max).is_some() {
        out.push('…');
    }
    out
}

/// 읽기 막대의 글: 무엇을 읽어 가는지 앞부분을 보인다
pub fn read_prompt(text: &str) -> String {
    if text.is_empty() {
        return "프로그램이 클립보드를 읽으려 합니다. 지금 클립보드는 비어 있습니다".to_string();
    }
    let n = text.chars().count();
    format!(
        "프로그램이 클립보드를 읽으려 합니다. 읽어 갈 글({n}글자): {}",
        clip_preview(text, CLIP_PREVIEW_CHARS)
    )
}

/// 쓰기 막대의 글: 무엇을 쓰려는지 앞부분을 보인다
pub fn write_prompt(text: &str) -> String {
    let n = text.chars().count();
    format!(
        "프로그램이 클립보드에 쓰려 합니다. 쓸 글({n}글자): {}",
        clip_preview(text, CLIP_PREVIEW_CHARS)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ClipVerdict::*;
    use ClipboardAccess as A;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn access_values_parse() {
        assert_eq!(A::parse("ask"), Some(A::Ask));
        assert_eq!(A::parse("allow"), Some(A::Allow));
        assert_eq!(A::parse("true"), Some(A::Allow));
        assert_eq!(A::parse("deny"), Some(A::Deny));
        assert_eq!(A::parse("false"), Some(A::Deny));
        assert_eq!(A::parse("maybe"), None);
        assert!(A::Ask.engine_enabled() && A::Allow.engine_enabled() && !A::Deny.engine_enabled());
    }

    #[test]
    fn write_rules_keep_watching_and_size_limits() {
        assert_eq!(decide_write(A::Allow, true, 10, None, false), Allow);
        assert_eq!(decide_write(A::Deny, true, 10, None, false), Deny);
        assert_eq!(decide_write(A::Ask, true, 10, None, false), Ask);
        // 보고 있지 않은 칸은 ask여도 묻지 않고 버린다(막대로 하이재킹을 시도하지 못하게)
        assert_eq!(decide_write(A::Ask, false, 10, None, false), Deny);
        assert_eq!(decide_write(A::Allow, false, 10, None, false), Deny);
        // 크기: 고정 1MiB와 설정 상한 중 작은 쪽. 묻기 전에 본다
        assert_eq!(
            decide_write(A::Ask, true, crate::config::OSC52_WRITE_CAP + 1, None, false),
            Deny
        );
        assert_eq!(decide_write(A::Ask, true, 11, Some(10), false), Deny);
        assert_eq!(decide_write(A::Allow, true, 11, Some(10), false), Deny);
        // 막대가 이미 떠 있으면 쌓지 않고 거절. allow는 막대가 필요 없으니 상관없다
        assert_eq!(decide_write(A::Ask, true, 10, None, true), Deny);
        assert_eq!(decide_write(A::Allow, true, 10, None, true), Allow);
    }

    #[test]
    fn read_rules() {
        assert_eq!(decide_read(A::Allow, true, false, false), Allow);
        assert_eq!(decide_read(A::Deny, true, false, false), Deny);
        assert_eq!(decide_read(A::Ask, true, false, false), Ask);
        // 보고 있지 않은 칸의 읽기 물음은 묻지 않고 거절
        assert_eq!(decide_read(A::Ask, false, false, false), Deny);
        // 읽기 전용 칸에는 답이 가지 않으니 묻지도 않는다
        assert_eq!(decide_read(A::Ask, true, true, false), Deny);
        // 막대가 떠 있으면 쌓지 않는다
        assert_eq!(decide_read(A::Ask, true, false, true), Deny);
    }

    #[test]
    fn enter_never_allows_and_y_needs_a_calm_moment() {
        let late = MS(2000);
        assert_eq!(clipboard_answer(AnswerKey::Enter, true, false, late), None);
        assert_eq!(clipboard_answer(AnswerKey::Char('y'), true, false, late), Some(true));
        assert_eq!(clipboard_answer(AnswerKey::Char('Y'), true, false, late), Some(true));
        // 막대가 뜬 직후의 y, 누르고 있어 반복되는 y, 장식 키가 눌린 y는 허용이 아니다
        assert_eq!(clipboard_answer(AnswerKey::Char('y'), true, false, MS(100)), None);
        assert_eq!(clipboard_answer(AnswerKey::Char('y'), true, true, late), None);
        assert_eq!(clipboard_answer(AnswerKey::Char('y'), false, false, late), None);
        // 거절은 언제나 된다
        assert_eq!(clipboard_answer(AnswerKey::Escape, true, false, MS(0)), Some(false));
        assert_eq!(clipboard_answer(AnswerKey::Char('n'), true, false, MS(0)), Some(false));
        assert_eq!(clipboard_answer(AnswerKey::Char('N'), true, true, MS(0)), Some(false));
        // 다른 키는 삼킬 뿐 아무것도 정하지 않는다
        assert_eq!(clipboard_answer(AnswerKey::Char('x'), true, false, late), None);
    }

    #[test]
    fn previews_strip_controls_and_truncate() {
        assert_eq!(clip_preview("a\x1b[31mb\nc", 40), "a [31mb c");
        assert_eq!(clip_preview("abcdef", 3), "abc…");
        assert_eq!(clip_preview("abc", 3), "abc");
        assert_eq!(clip_preview("한글입니다", 2), "한글…");
        let r = read_prompt("비밀번호\x07123");
        assert!(
            r.contains("읽") && r.contains("비밀번호 123") && r.contains("(8글자)"),
            "{r}"
        );
        assert!(!r.contains('\x07'));
        assert!(read_prompt("").contains("비어"));
        let w = write_prompt(&("x".repeat(100)));
        assert!(w.contains("쓰") && w.contains("(100글자)") && w.contains('…'), "{w}");
    }
}
