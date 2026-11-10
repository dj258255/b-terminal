//! vendor/vte의 SGR 해석 확인(#277). vendor는 작업 공간 밖이라 자체 테스트가 돌지 않아 여기서 본다

use alacritty_terminal::vte::ansi::{Attr, Handler, Processor};

#[derive(Default)]
struct Collect(Vec<Attr>);

impl Handler for Collect {
    fn terminal_attribute(&mut self, attr: Attr) {
        self.0.push(attr);
    }
}

fn parse(bytes: &[u8]) -> Vec<Attr> {
    let mut parser: Processor = Processor::new();
    let mut handler = Collect::default();
    parser.advance(&mut handler, bytes);
    handler.0
}

#[test]
fn sgr_53_is_overline_and_55_cancels_it() {
    assert_eq!(parse(b"\x1b[53m"), vec![Attr::Overline]);
    assert_eq!(parse(b"\x1b[55m"), vec![Attr::CancelOverline]);
}

#[test]
fn overline_mixes_with_other_attributes() {
    assert_eq!(parse(b"\x1b[1;53m"), vec![Attr::Bold, Attr::Overline]);
    assert_eq!(
        parse(b"\x1b[53;4:3;55m"),
        vec![Attr::Overline, Attr::Undercurl, Attr::CancelOverline]
    );
    // 색 매개변수 뒤에서도 갈라지지 않는다
    assert_eq!(parse(b"\x1b[38;5;1;53m").len(), 2);
}
