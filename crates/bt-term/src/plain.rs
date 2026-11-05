//! 터미널 출력 바이트에서 이스케이프를 빼고 글만 꺼낸다(명령 출력을 `btctl run`에 돌려줄 때)

use alacritty_terminal::vte::{Params, Parser, Perform};

#[derive(Default)]
struct Collect {
    out: String,
    /// CR을 받았고 아직 LF가 오지 않았다(진행 막대처럼 같은 줄을 덮어쓰는 출력)
    pending_cr: bool,
}

impl Collect {
    fn overwrite_line(&mut self) {
        // CR 뒤에 LF가 아닌 글자가 오면 그 줄을 처음부터 다시 쓰는 것이다
        if self.pending_cr {
            let start = self.out.rfind('\n').map(|i| i + 1).unwrap_or(0);
            self.out.truncate(start);
            self.pending_cr = false;
        }
    }
}

impl Perform for Collect {
    fn print(&mut self, c: char) {
        self.overwrite_line();
        self.out.push(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                self.pending_cr = false;
                self.out.push('\n');
            }
            b'\r' => self.pending_cr = true,
            b'\t' => {
                self.overwrite_line();
                self.out.push('\t');
            }
            0x08 => {
                self.out.pop();
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, _: &Params, _: &[u8], _: bool, action: char) {
        // 줄 지우기(EL) 뒤에 다시 쓰는 출력: 지금 줄을 비운다
        if action == 'K' && self.pending_cr {
            self.overwrite_line();
        }
    }
}

/// 바이트에서 글만 꺼낸다. 색·커서 이동·OSC는 버리고, CR로 같은 줄을 덮어쓰는 출력은 마지막 것만 남긴다
pub fn plain_text(bytes: &[u8]) -> String {
    let mut parser: Parser = Parser::new();
    let mut c = Collect::default();
    parser.advance(&mut c, bytes);
    // CR로 끝난 마지막 줄은 곧 덮인다(zsh가 프롬프트 앞에 쓰는 PROMPT_SP 공백 등). 버린다
    if c.pending_cr {
        let start = c.out.rfind('\n').map(|i| i + 1).unwrap_or(0);
        c.out.truncate(start);
    }
    c.out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_colors_and_keeps_lines() {
        assert_eq!(plain_text(b"\x1b[31mred\x1b[0m\r\nok\r\n"), "red\nok\n");
        assert_eq!(
            plain_text("한글 \x1b]8;;http://x\x07링크\x1b]8;;\x07\n".as_bytes()),
            "한글 링크\n"
        );
    }

    #[test]
    fn carriage_return_progress_keeps_the_last_state() {
        assert_eq!(plain_text(b"10%\r50%\r100%\ndone\n"), "100%\ndone\n");
        assert_eq!(plain_text(b"abc\x08d\n"), "abd\n");
        // zsh의 PROMPT_SP: 출력 뒤 공백들과 CR. 실제 zsh 탭에서 끝에 공백이 붙었다
        assert_eq!(plain_text(b"hello\r\n \r"), "hello\n");
    }
}
