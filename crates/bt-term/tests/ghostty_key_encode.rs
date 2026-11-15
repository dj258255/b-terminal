//! Ghostty `src/input/key_encode.zig`(main 7fa26e8b675f)의 `ctrlseq:` 시험 11개(2805~2860줄, #567).
//! `ctrlSeq`와 같은 함수 수준(bt-term `ctrl_seq`)에서 시험한다. 인자는 Ghostty와 같은 차례(물리 키의 글자, 글, unshifted, 수식 키)다.
//! Ghostty의 `.unidentified`는 `None`, `.key_c`는 `Some('c')`다. Caps Lock은 winit이 알려 주지 않고 `ctrlSeq`도 쓰지 않는다(`binding()`이 지운다).
//! `legacy:` 시험은 앱 입구에서 돌린다(`crates/bt-app/src/keys_ghostty_tests.rs`).

use bt_term::{Mods, ctrl_seq};

const NONE: Mods = Mods {
    shift: false,
    ctrl: false,
    alt: false,
    sup: false,
};
const CTRL: Mods = Mods { ctrl: true, ..NONE };
const SHIFT: Mods = Mods { shift: true, ..NONE };

fn c(ch: char) -> Option<u32> {
    Some(ch as u32)
}

// 2805줄
#[test]
fn ctrlseq_normal_ctrl_c() {
    assert_eq!(ctrl_seq(None, "c", 'c' as u32, CTRL), Some(0x03));
}

// 2810줄(좌우 수식 키 구분이 없다)
#[test]
fn ctrlseq_normal_ctrl_c_right_control() {
    assert_eq!(ctrl_seq(None, "c", 'c' as u32, CTRL), Some(0x03));
}

// 2815줄
#[test]
fn ctrlseq_alt_should_be_allowed() {
    assert_eq!(ctrl_seq(None, "c", 'c' as u32, Mods { alt: true, ..CTRL }), Some(0x03));
}

// 2820줄
#[test]
fn ctrlseq_no_ctrl_does_nothing() {
    assert_eq!(ctrl_seq(None, "c", 'c' as u32, NONE), None);
}

// 2824줄
#[test]
fn ctrlseq_shifted_non_character() {
    assert_eq!(
        ctrl_seq(None, "_", '-' as u32, Mods { shift: true, ..CTRL }),
        Some(0x1f)
    );
}

// 2829줄(Caps Lock으로 대문자가 온 경우. unshifted 소문자로 되돌린다)
#[test]
fn ctrlseq_caps_ascii_letter() {
    assert_eq!(ctrl_seq(None, "C", 'c' as u32, CTRL), Some(0x03));
}

// 2834줄
#[test]
#[ignore = "차이 C"]
fn ctrlseq_shift_does_not_generate_ctrl_seq() {
    assert_eq!(ctrl_seq(None, "C", 'c' as u32, SHIFT), None);
    assert_eq!(ctrl_seq(None, "C", 'c' as u32, Mods { shift: true, ..CTRL }), None);
}

// 2839줄
#[test]
#[ignore = "차이 C"]
fn ctrlseq_russian_ctrl_c() {
    assert_eq!(ctrl_seq(c('c'), "с", 0x0441, CTRL), Some(0x03));
}

// 2844줄
#[test]
fn ctrlseq_russian_shifted_ctrl_c() {
    assert_eq!(ctrl_seq(c('c'), "с", 0x0441, Mods { shift: true, ..CTRL }), None);
}

// 2849줄
#[test]
#[ignore = "차이 C"]
fn ctrlseq_russian_alt_ctrl_c() {
    assert_eq!(ctrl_seq(c('c'), "с", 0x0441, Mods { alt: true, ..CTRL }), Some(0x03));
}

// 2854줄
#[test]
#[ignore = "차이 C"]
fn ctrlseq_right_ctrl_c() {
    assert_eq!(ctrl_seq(c('c'), "с", 'c' as u32, CTRL), Some(0x03));
}
