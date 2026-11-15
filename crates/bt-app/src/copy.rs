//! 복사 형식에 따라 시스템 클립보드에 넘길 값을 만든다(`copy_to_clipboard:<plain|vt|html|mixed>`, #550).
//! 클립보드에 쓰는 것은 앱이 한다. 여기는 값만 만드는 순수 함수라 실제 클립보드를 건드리지 않고 시험한다

use bt_term::{CopyEmit, CopyFormat};

/// 클립보드에 넣을 값
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardPayload {
    /// text/plain만
    Text(String),
    /// text/html. `alt`가 있으면 text/plain도 함께(macOS는 `public.html`과 `public.utf8-plain-text`)
    Html { html: String, alt: Option<String> },
}

/// `plain`: 고른 글을 지금 함수가 만든 그대로(`clipboard-trim-trailing-spaces`·`clipboard-codepoint-map` 적용).
/// `styled(꼴, 바탕·글자 기본색을 넣는가)`: 같은 범위를 VT·HTML로 만드는 함수. 없으면 None.
/// Ghostty `copySelectionToClipboards`: vt는 이스케이프가 든 글(text/plain), html은 HTML만(기본색 포함),
/// mixed는 글 + HTML(기본색 없이)
pub fn payload(
    format: CopyFormat,
    plain: &str,
    mut styled: impl FnMut(CopyEmit, bool) -> Option<String>,
) -> ClipboardPayload {
    let text = || ClipboardPayload::Text(plain.to_owned());
    match format {
        CopyFormat::Plain => text(),
        CopyFormat::Vt => styled(CopyEmit::Vt, true).map_or_else(text, ClipboardPayload::Text),
        CopyFormat::Html => {
            styled(CopyEmit::Html, true).map_or_else(text, |html| ClipboardPayload::Html { html, alt: None })
        }
        CopyFormat::Mixed => styled(CopyEmit::Html, false).map_or_else(text, |html| ClipboardPayload::Html {
            html,
            alt: Some(plain.to_owned()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 어느 꼴을 어떤 기본색 옵션으로 불렀는지 적어 두는 가짜 만들기 함수
    fn fake(calls: &mut Vec<(CopyEmit, bool)>) -> impl FnMut(CopyEmit, bool) -> Option<String> + '_ {
        move |emit, colors| {
            calls.push((emit, colors));
            Some(format!("<{emit:?} colors={colors}>"))
        }
    }

    #[test]
    fn plain_is_the_text_as_is_and_asks_for_nothing_else() {
        let mut calls = vec![];
        let p = payload(CopyFormat::Plain, "a  b\n", fake(&mut calls));
        assert_eq!(p, ClipboardPayload::Text("a  b\n".into()));
        assert!(calls.is_empty(), "plain은 스타일 글을 만들지 않는다");
    }

    #[test]
    fn vt_is_text_plain_with_escapes_and_screen_colors() {
        let mut calls = vec![];
        let p = payload(CopyFormat::Vt, "x", fake(&mut calls));
        assert_eq!(p, ClipboardPayload::Text("<Vt colors=true>".into()));
        assert_eq!(calls, [(CopyEmit::Vt, true)]);
    }

    #[test]
    fn html_is_html_only_with_screen_colors() {
        let mut calls = vec![];
        let p = payload(CopyFormat::Html, "x", fake(&mut calls));
        assert_eq!(
            p,
            ClipboardPayload::Html {
                html: "<Html colors=true>".into(),
                alt: None
            }
        );
        assert_eq!(calls, [(CopyEmit::Html, true)]);
    }

    #[test]
    fn mixed_is_text_plus_html_without_screen_colors() {
        let mut calls = vec![];
        let p = payload(CopyFormat::Mixed, "a  b", fake(&mut calls));
        assert_eq!(
            p,
            ClipboardPayload::Html {
                html: "<Html colors=false>".into(),
                alt: Some("a  b".into())
            }
        );
        assert_eq!(calls, [(CopyEmit::Html, false)]);
    }

    #[test]
    fn falls_back_to_plain_text_when_styled_text_cannot_be_made() {
        for f in [CopyFormat::Vt, CopyFormat::Html, CopyFormat::Mixed] {
            assert_eq!(
                payload(f, "t", |_, _| None),
                ClipboardPayload::Text("t".into()),
                "{f:?}"
            );
        }
    }
}
