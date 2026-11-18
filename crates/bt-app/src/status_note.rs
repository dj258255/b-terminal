//! 프로그램 상태(OSC 7501)가 사용자를 부르는 데스크톱 알림의 글과 빈도 제한(#618). 창과 상관없는 순수 함수라 여기서 테스트한다

use std::time::Duration;

use bt_term::program_status::{Alert, Kind, State, safe_text};

/// 같은 칸의 상태 알림 사이 최소 간격. 프로그램이 보고를 몰아 보내도 알림이 쌓이지 않게 한다
pub const MIN_GAP: Duration = Duration::from_secs(5);

/// 제목·본문 상한(글자 수). 다른 OSC 알림(`MAX_TITLE`·`MAX_BODY`)과 같다
const MAX_TITLE: usize = 120;
const MAX_BODY: usize = 500;

/// 지난 알림에서 `since`가 지났으면(또는 처음이면) 알릴 수 있다
pub fn due(since: Option<Duration>) -> bool {
    since.is_none_or(|d| d >= MIN_GAP)
}

/// 알림의 (제목, 본문). 제목은 프로그램이 정한 제목, 없으면 탭 이름이다. 본문은 `앱: 상태 문구`에 메시지를 붙인다.
/// 프로그램이 보낸 글은 방향 바꿈·보이지 않는 서식 글자와 제어 문자를 지우고 길이를 자른다. 마크업으로 읽지 않는다
pub fn text(alert: &Alert, tab_title: &str) -> (String, String) {
    let title = alert
        .title
        .as_deref()
        .map(|t| safe_text(t, MAX_TITLE))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| safe_text(tab_title, MAX_TITLE));
    let phrase = match (alert.state, alert.kind) {
        (State::Blocked, Some(Kind::Permission)) => "허락을 기다립니다",
        (State::Blocked, Some(Kind::Question)) => "답을 기다립니다",
        (State::Blocked, Some(Kind::Auth)) => "로그인이 필요합니다",
        (State::Blocked, None) => "입력을 기다립니다",
        (State::Error, _) => "작업이 실패했습니다",
        _ => "작업이 끝났습니다",
    };
    let mut body = match alert.app.as_deref() {
        Some(app) => format!("{app}: {phrase}"),
        None => phrase.to_string(),
    };
    if let Some(msg) = alert
        .msg
        .as_deref()
        .map(|m| safe_text(m, MAX_BODY))
        .filter(|m| !m.is_empty())
    {
        body = format!("{body}\n{msg}");
    }
    (title, safe_text_keep_newline(&body, MAX_BODY))
}

/// 길이만 자른다(본문 가운데의 줄바꿈 하나는 우리가 넣은 것이다)
fn safe_text_keep_newline(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alert(state: State, kind: Option<Kind>) -> Alert {
        Alert {
            state,
            kind,
            app: None,
            title: None,
            msg: None,
        }
    }

    #[test]
    fn rate_limit_allows_the_first_and_spaced_notes_only() {
        assert!(due(None));
        assert!(!due(Some(Duration::from_secs(4))));
        assert!(due(Some(Duration::from_secs(5))));
    }

    #[test]
    fn text_names_the_state_and_falls_back_to_the_tab_title() {
        let (t, b) = text(&alert(State::Blocked, Some(Kind::Permission)), "작업 탭");
        assert_eq!((t.as_str(), b.as_str()), ("작업 탭", "허락을 기다립니다"));
        assert_eq!(
            text(&alert(State::Blocked, Some(Kind::Question)), "t").1,
            "답을 기다립니다"
        );
        assert_eq!(
            text(&alert(State::Blocked, Some(Kind::Auth)), "t").1,
            "로그인이 필요합니다"
        );
        assert_eq!(text(&alert(State::Blocked, None), "t").1, "입력을 기다립니다");
        assert_eq!(text(&alert(State::Done, None), "t").1, "작업이 끝났습니다");
        assert_eq!(text(&alert(State::Error, None), "t").1, "작업이 실패했습니다");
    }

    #[test]
    fn text_adds_app_title_and_message_without_hostile_characters() {
        let a = Alert {
            state: State::Done,
            kind: None,
            app: Some("claude-code".into()),
            title: Some("빌드\u{202E}".into()),
            msg: Some("모두\u{200B} 통과\u{7}".into()),
        };
        let (t, b) = text(&a, "탭");
        assert_eq!(t, "빌드");
        assert_eq!(b, "claude-code: 작업이 끝났습니다\n모두 통과");
    }

    #[test]
    fn text_is_length_capped() {
        let mut a = alert(State::Done, None);
        a.title = Some("가".repeat(300));
        a.msg = Some("나".repeat(2000));
        let (t, b) = text(&a, "탭");
        assert!(t.chars().count() <= MAX_TITLE);
        assert!(b.chars().count() <= MAX_BODY);
    }
}
