//! 창에 끌어다 놓은 파일의 경로를 셸 이스케이프해 붙여넣을 글로 만든다(Ghostty macOS `performDragOperation`, #566).
//!
//! winit은 파일마다 `WindowEvent::DroppedFile`을 따로 보내고 놓은 위치를 주지 않는다. 한 번 끌어다 놓은 파일들은
//! macOS의 `performDragOperation:` 한 번 안에서 이어서 오므로(vendor/winit macos/window_delegate.rs 405~415줄) 같은
//! 이벤트 고리 차례에 모이고, 그 차례가 끝나는 `about_to_wait`에서 한 번에 붙여넣으면 Ghostty처럼 한 덩어리가 된다.
//! 순수 함수만 두어 앱 상태 없이 시험한다.

use std::path::PathBuf;

/// 앞에 `\`를 붙이는 글자(Ghostty `Ghostty.Shell.escapeCharacters`: `\ ()[]{}<>"'`!#$&;|*?`와 탭)
const ESCAPED: &[char] = &[
    '\\', ' ', '(', ')', '[', ']', '{', '}', '<', '>', '"', '\'', '`', '!', '#', '$', '&', ';', '|', '*', '?', '\t',
];

/// 경로의 셸 예민한 글자 앞에 `\`를 붙인다(Ghostty `Ghostty.Shell.escape`). 줄 편집기에 경로를 넣는 용도다.
/// 제어 문자(줄바꿈·ESC·Ctrl-U 등)는 이스케이프로 막을 수 없다: 따옴표 앞의 Ctrl-U가 여는 따옴표를 지우는 식이다.
/// 그래서 여기서 손대지 않고, 붙여넣기 길(`App::paste`)이 위험 바이트를 공백으로 바꾸고 줄바꿈이 있으면 묻게 한다
pub fn escape(path: &str) -> String {
    let mut out = String::with_capacity(path.len() + 8);
    for c in path.chars() {
        if ESCAPED.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// 놓은 경로들을 이스케이프해 공백 하나로 잇는다(Ghostty `getOpinionatedStringContents`는 끝에 공백을 붙이지 않는다).
/// UTF-8이 아닌 경로는 건너뛴다. 넣을 것이 없으면 None
pub fn join(paths: &[PathBuf]) -> Option<String> {
    let parts: Vec<String> = paths
        .iter()
        .filter_map(|p| p.to_str())
        .filter(|s| !s.is_empty())
        .map(escape)
        .collect();
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// 같은 차례에 온 `DroppedFile`을 창별로 모은다. `W`는 창 id(시험에서는 정수)
#[derive(Debug)]
pub struct DropBatch<W> {
    window: Option<W>,
    paths: Vec<PathBuf>,
}

impl<W> Default for DropBatch<W> {
    fn default() -> Self {
        Self {
            window: None,
            paths: Vec::new(),
        }
    }
}

impl<W: Copy + PartialEq> DropBatch<W> {
    /// 파일 하나를 더한다. 앞에 모아 둔 것이 다른 창의 것이면 그 묶음을 먼저 돌려준다
    pub fn push(&mut self, window: W, path: PathBuf) -> Option<(W, String)> {
        let flushed = if self.window.is_some_and(|w| w != window) {
            self.take()
        } else {
            None
        };
        self.window = Some(window);
        self.paths.push(path);
        flushed
    }

    /// 모은 것을 비우고 붙여넣을 글로 돌려준다. 모은 것이 없거나 넣을 경로가 하나도 없으면 None
    pub fn take(&mut self) -> Option<(W, String)> {
        let window = self.window.take()?;
        let paths = std::mem::take(&mut self.paths);
        join(&paths).map(|text| (window, text))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn escape_prefixes_every_shell_special_char() {
        // Ghostty `escapeCharacters` 하나하나
        for c in ESCAPED {
            let got = escape(&format!("a{c}b"));
            assert_eq!(got, format!("a\\{c}b"), "{c:?}");
        }
        assert_eq!(ESCAPED.len(), 22);
    }

    #[test]
    fn escape_examples() {
        assert_eq!(escape("/tmp/a b"), "/tmp/a\\ b");
        assert_eq!(escape("/tmp/it's"), "/tmp/it\\'s");
        assert_eq!(escape("/tmp/say \"hi\""), "/tmp/say\\ \\\"hi\\\"");
        assert_eq!(escape("/tmp/$HOME"), "/tmp/\\$HOME");
        assert_eq!(escape("/tmp/a!b"), "/tmp/a\\!b");
        assert_eq!(escape("/tmp/a(1)[2]{3}"), "/tmp/a\\(1\\)\\[2\\]\\{3\\}");
        assert_eq!(
            escape("/tmp/a&b;c|d*e?f<g>h`i#j"),
            "/tmp/a\\&b\\;c\\|d\\*e\\?f\\<g\\>h\\`i\\#j"
        );
        assert_eq!(escape("/tmp/a\tb"), "/tmp/a\\\tb");
        assert_eq!(
            escape("/tmp/a\\b"),
            "/tmp/a\\\\b",
            "역슬래시도 이스케이프(두 번 이스케이프하지 않음)"
        );
    }

    #[test]
    fn escape_leaves_plain_and_non_ascii_alone() {
        assert_eq!(escape("/Users/me/file-1_2.txt"), "/Users/me/file-1_2.txt");
        assert_eq!(escape("/Users/범수/문서.txt"), "/Users/범수/문서.txt");
        assert_eq!(
            escape("/Users/범수/내 문서 (1).txt"),
            "/Users/범수/내\\ 문서\\ \\(1\\).txt"
        );
        assert_eq!(escape("/tmp/😀 파일"), "/tmp/😀\\ 파일");
        assert_eq!(escape(""), "");
    }

    #[test]
    fn escape_keeps_control_chars_for_the_paste_path() {
        // 줄바꿈은 이스케이프하지 않고 붙여넣기 길이 거른다
        assert_eq!(escape("/tmp/a\nb"), "/tmp/a\nb");
        assert_eq!(escape("/tmp/a\x15b"), "/tmp/a\x15b");
    }

    #[test]
    fn join_uses_single_spaces_without_trailing_space() {
        assert_eq!(join(&[p("/a")]), Some("/a".to_string()));
        assert_eq!(join(&[p("/a"), p("/b c"), p("/d")]), Some("/a /b\\ c /d".to_string()));
    }

    #[test]
    fn join_skips_non_utf8_paths() {
        #[cfg(unix)]
        {
            use std::ffi::OsString;
            use std::os::unix::ffi::OsStringExt;
            let bad = PathBuf::from(OsString::from_vec(b"/tmp/\xff\xfe".to_vec()));
            assert_eq!(join(&[bad.clone(), p("/ok")]), Some("/ok".to_string()));
            assert_eq!(join(&[p("/ok"), bad.clone(), p("/ok2")]), Some("/ok /ok2".to_string()));
            assert_eq!(join(&[bad]), None);
        }
    }

    #[test]
    fn join_of_nothing_is_none() {
        assert_eq!(join(&[]), None);
        assert_eq!(join(&[p("")]), None);
    }

    #[test]
    fn dropped_newline_name_goes_through_paste_protection_and_encoding() {
        let text = join(&[p("/tmp/a\nrm -rf ~")]).unwrap();
        // 괄호 모드가 아니면 확인을 거친다
        assert!(!bt_term::paste_is_safe(&text, false, true));
        // 확인 뒤에도 위험 바이트는 공백, 줄바꿈은 CR로만 바뀐다(이스케이프는 이미 끝남)
        assert_eq!(bt_term::encode_paste(&text, false), b"/tmp/a\rrm\\ -rf\\ ~");
        let text = join(&[p("/tmp/a\x15b\x1b[201~c")]).unwrap();
        assert_eq!(
            bt_term::encode_paste(&text, true),
            b"\x1b[200~/tmp/a b \\[201~c\x1b[201~"
        );
    }

    #[test]
    fn batch_joins_one_drop_into_one_text() {
        let mut b: DropBatch<u32> = DropBatch::default();
        assert!(b.push(1, p("/a")).is_none());
        assert!(b.push(1, p("/b c")).is_none());
        assert!(b.push(1, p("/d")).is_none());
        assert_eq!(b.take(), Some((1, "/a /b\\ c /d".to_string())));
        assert_eq!(b.take(), None, "한 번 꺼내면 빈다");
    }

    #[test]
    fn batch_does_not_merge_two_turns() {
        let mut b: DropBatch<u32> = DropBatch::default();
        b.push(1, p("/a"));
        assert_eq!(b.take(), Some((1, "/a".to_string())));
        b.push(1, p("/b"));
        assert_eq!(b.take(), Some((1, "/b".to_string())));
    }

    #[test]
    fn batch_flushes_when_window_changes() {
        let mut b: DropBatch<u32> = DropBatch::default();
        assert!(b.push(1, p("/a")).is_none());
        assert_eq!(b.push(2, p("/b")), Some((1, "/a".to_string())));
        assert_eq!(b.take(), Some((2, "/b".to_string())));
    }

    #[test]
    fn batch_of_only_bad_paths_yields_nothing() {
        let mut b: DropBatch<u32> = DropBatch::default();
        b.push(1, p(""));
        assert_eq!(b.take(), None);
        assert_eq!(b.take(), None);
    }
}
