//! kitty 그래픽 명령 검사를 Ghostty 1.4.0처럼(#798, #802). 같은 입력을 `Graphics::handle`와 `Emulator::feed`에 먹인다.
//!
//! 견준 Ghostty 시험(`src/terminal/kitty/`):
//! - `graphics_command.zig`: "ensure proper overflow error for u32"(1493), "... for i32"(1504), "unknown flag key values still fail
//!   parsing"(1856), "transmission ignores 'm' if medium is not direct"(1315), "ignore very long values"(1458),
//!   "ignore unknown keys (long)"(1422), `Parser.finishValue`·`Parser.decodeData`·`Parser.complete`
//! - `apc.zig`: "Kitty command with overflow u32"(544), "... overflow i32"(556)
//! - `graphics_exec.zig`: "kittygfx image id and number are mutually exclusive for every action"(1188),
//!   "kittygfx more chunks with chunk increasing q"(1429), "kittygfx retransmit same id removes image on first chunk"(1820),
//!   "kittygfx query validates image data"(1117)
//! - `graphics_image.zig`: "image load with invalid RGB data"(1173, 길이는 `complete`가 본다), "image load: png rejects oversized
//!   decoder allocation"(2153), "image load: png rejects oversized Wuffs image before allocation"(2192)
//!
//! 오류 답의 코드는 kitty 명세 코드로 둔다(짧은 자료 `ENODATA`, PNG `EBADPNG`·`EFBIG`, 상한 `ENOSPC`). 설명 글은 넣지 않는다.

use bt_core::Theme;
use bt_term::graphics::{Cursor, Graphics};
use bt_term::{Emulator, GridSize};

// ───── 도우미 ─────

const CUR_POS: Cursor = Cursor {
    line: 100,
    col: 4,
    cell_w: 10,
    cell_h: 20,
    alt: false,
    top: 90,
    rows: 24,
    cols: 80,
};

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::new();
    for ch in data.chunks(3) {
        let n = (ch[0] as u32) << 16 | (*ch.get(1).unwrap_or(&0) as u32) << 8 | *ch.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            if i <= ch.len() {
                s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                s.push('=');
            }
        }
    }
    s
}

/// `Graphics`에 APC 본문(`ESC _ G` 뒤)을 먹이고 답을 글로 돌려준다(없으면 빈 글)
fn h(g: &mut Graphics, s: &str) -> String {
    let out = g.handle(s.as_bytes(), CUR_POS);
    out.reply
        .map(|r| String::from_utf8_lossy(&r).into_owned())
        .unwrap_or_default()
}

fn emu() -> Emulator {
    Emulator::new(
        GridSize {
            cols: 80,
            rows: 24,
            cell_width: 10,
            cell_height: 20,
        },
        Theme::default(),
    )
}

fn send(e: &mut Emulator, body: &str) -> String {
    e.feed(format!("\x1b_G{body}\x1b\\").as_bytes());
    String::from_utf8_lossy(&e.take_replies()).into_owned()
}

const OK1: &str = "\x1b_Gi=1;OK\x1b\\";

// ───────────── #798: 파싱 실패와 범위 밖 값은 명령을 버린다 ─────────────

/// apc "Kitty command with overflow u32", command 1493: `i=10000000000`은 명령을 버린다(답도 저장도 없다)
#[test]
fn overflow_u32_drops_the_command() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,i=10000000000,f=24,s=1,v=1;AAAA"), "");
    assert!(
        g.image(2_147_483_647).is_none(),
        "i=0으로 읽어 내부 번호로 저장하면 안 된다"
    );
    assert!(g.image(0).is_none());
}

/// apc "Kitty command with overflow i32", command 1504: `z=-9999999999`
#[test]
fn overflow_i32_drops_the_command() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=1,i=1;AAAA"), OK1);
    assert_eq!(h(&mut g, "a=p,i=1,z=-9999999999"), "");
    assert!(g.placements.is_empty(), "자리가 생기면 안 된다");
}

/// `Parser.finishValue`: 숫자가 아닌 여러 글자 값, 음수(부호 없는 키), 빈 값은 모두 파싱 오류다
#[test]
fn non_numeric_values_drop_the_command() {
    for body in [
        "a=t,i=abc,f=24,s=1,v=1;AAAA",
        "a=t,i=-5,f=24,s=1,v=1;AAAA",
        "a=t,i=,f=24,s=1,v=1;AAAA",
        "a=t,i=1,f=24,s=1,v=1,q=xx;AAAA",
        // 키를 쓰지 않는 명령에 붙은 값도 읽다가 틀리면 버린다
        "a=t,i=1,f=24,s=1,v=1,x=zz;AAAA",
        // 알 수 없는 키(글자가 아닌 키)의 값도 같은 규칙으로 읽는다
        "a=t,i=1,f=24,s=1,v=1,!=abc;AAAA",
        // 부호 있는 키(z, H, V)는 i32 범위
        "a=p,i=1,H=2147483648",
    ] {
        let mut g = Graphics::default();
        assert_eq!(h(&mut g, body), "", "{body}");
        assert!(g.image(1).is_none(), "{body}");
        assert!(g.placements.is_empty(), "{body}");
    }
}

/// `KV` 설명: 한 글자 숫자 아닌 값은 ASCII 코드다(`i=a`는 97)
#[test]
fn single_letter_value_is_its_ascii_code() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,i=a,f=24,s=1,v=1;AAAA"), "\x1b_Gi=97;OK\x1b\\");
    assert!(g.image(97).is_some());
}

/// command 1856 "unknown flag key values still fail parsing": 모르는 동작·전송 방식·압축·지우기 종류
#[test]
fn unknown_flag_values_drop_the_command() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=1,i=1;AAAA"), OK1);
    h(&mut g, "a=p,i=1,C=1");
    assert_eq!(g.placements.len(), 1);
    for body in [
        "a=z,i=31",
        "a=12,i=31",
        "a=1,i=31",
        "a=t,t=q,i=31,f=24,s=1,v=1;AAAA",
        "a=t,t=9,i=31,f=24,s=1,v=1;AAAA",
        "a=t,o=q,i=31,f=24,s=1,v=1;AAAA",
        "a=q,o=q,i=31,f=24,s=1,v=1;AAAA",
        "a=f,t=q,i=1,f=24,s=1,v=1;AAAA",
        "a=d,d=w,i=1",
        "a=d,d=12,i=1",
    ] {
        assert_eq!(h(&mut g, body), "", "{body}");
    }
    assert!(g.image(31).is_none());
    assert_eq!(g.placements.len(), 1, "지우기 종류를 몰라 버렸으니 자리는 그대로");
    assert!(g.image(1).is_some());
}

/// `t`와 `o`는 전송 명령에서만 읽는다(`Transmission.parse`): 다른 동작에서는 이 값이 명령을 버리지 않는다
#[test]
fn medium_and_compression_are_only_checked_for_transmissions() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=1,i=1;AAAA"), OK1);
    h(&mut g, "a=p,i=1,t=q,o=q,C=1");
    assert_eq!(g.placements.len(), 1);
}

/// `Parser.complete`: 키만 남고 끝나면 파싱 오류, `Parser.decodeData`: 잘못된 base64는 명령을 버린다
#[test]
fn dangling_key_and_bad_base64_drop_the_command() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=q,i=31,f=24,s=1,v=1,b"), "");
    assert_eq!(h(&mut g, "i=31,a=t,f=24,s=1,v=1;!!!!"), "");
    assert_eq!(h(&mut g, "i=31,a=q,f=24,s=1,v=1;!!!!"), "");
    assert!(g.image(31).is_none());
    // 이어지는 명령은 멀쩡히 처리된다
    assert_eq!(h(&mut g, "a=q,i=31,f=24,s=1,v=1;AAAA"), "\x1b_Gi=31;OK\x1b\\");
}

/// 버린 명령은 받던 조각 전송을 건드리지 않는다(`Handler.end`가 null만 돌려주고 `storage.loading`은 그대로)
#[test]
fn dropped_command_keeps_the_pending_chunks() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=2,i=1,m=1;AAAA"), "");
    assert_eq!(h(&mut g, "m=0;!!!!"), "", "깨진 base64를 담은 조각은 버린다");
    assert_eq!(h(&mut g, "m=0,i=99999999999;AAAA"), "", "숫자가 넘치는 조각도 버린다");
    assert_eq!(h(&mut g, "m=0;AAAA"), OK1);
    assert_eq!(g.image(1).unwrap().rgba.len(), 8);
}

/// 같은 줄에서 파싱 오류가 난 동작이 지우기였어도 버린다: 지우기가 받던 전송을 끊지 않는다
#[test]
fn dropped_delete_does_not_abort_the_pending_chunks() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=2,i=1,m=1;AAAA"), "");
    assert_eq!(h(&mut g, "a=d,d=w"), "");
    assert_eq!(h(&mut g, "m=0;AAAA"), OK1);
}

/// command 1458 "ignore very long values", 1422 "ignore unknown keys (long)": 값이 12글자를 넘으면 그 키는 없는 것이다.
/// 상태 기계는 무시한 값 다음 키들도 `;`까지 무시한다(`control_value_ignore`의 `,`는 `control_key_ignore`로 간다)
#[test]
fn long_values_and_unknown_keys_are_ignored_like_ghostty() {
    let mut g = Graphics::default();
    let ok = "\x1b_Gi=31;OK\x1b\\";
    assert_eq!(h(&mut g, "a=q,i=31,s=1,v=1,f=24,hello=world;AAAA"), ok);
    assert_eq!(h(&mut g, "a=q,i=31,s=1,v=1,f=24,!=1;AAAA"), ok);
    // 아주 긴 값은 키가 없는 것이다: v=0 → 크기 오류
    assert_eq!(
        h(
            &mut g,
            "a=q,i=31,s=1,f=24,v=2000000000000000000000000000000000000000;AAAA"
        ),
        "\x1b_Gi=31;EINVAL\x1b\\"
    );
    // 무시한 값 뒤의 키도 무시된다: i가 없으니 번호 없는 그림이라 답하지 않는다
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=1,hello=world,i=31;AAAA"), "");
    assert!(g.image(31).is_none());
}

/// graphics_command "no control data": `;QUFBQQ`는 전송 명령이다(번호가 없으니 답이 없다)
#[test]
fn payload_only_command_is_a_transmission() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, ";QUFBQQ"), "");
}

/// 같은 일을 PTY에서 읽는 길(`Emulator::feed`)로: 버린 명령은 답이 없다
#[test]
fn emulator_replies_nothing_for_dropped_commands() {
    let mut e = emu();
    for body in [
        "a=t,i=10000000000,f=24,s=1,v=1;AAAA",
        "a=p,i=1,z=-9999999999",
        "a=z,i=31",
        "a=t,t=q,i=31,f=24,s=1,v=1;AAAA",
        "a=t,o=q,i=31,f=24,s=1,v=1;AAAA",
        "i=31,a=t,f=24,s=1,v=1;!!!!",
    ] {
        assert_eq!(send(&mut e, body), "", "{body}");
    }
}

/// `Parser.complete` → `decodeData`: 조각마다 따로 base64를 푼다. `=` 채움을 조각마다 붙이는 클라이언트(조각 크기가 3의 배수가 아닐 때)
#[test]
fn each_chunk_is_decoded_on_its_own() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=32,s=1,v=1,i=5,m=1;AAA="), "");
    assert_eq!(
        h(&mut g, "m=0;AAA="),
        "\x1b_Gi=5;OK\x1b\\",
        "2바이트 + 2바이트 = 4바이트"
    );
    assert_eq!(g.image(5).unwrap().rgba.as_slice(), &[0, 0, 0, 0]);
    // 조각 셋, 채움이 모두 다른 길이
    let mut g = Graphics::default();
    let all: Vec<u8> = (1..=8).collect();
    let (a, b, c) = (b64(&all[..2]), b64(&all[2..5]), b64(&all[5..]));
    assert_eq!(h(&mut g, &format!("a=t,f=32,s=2,v=1,i=6,m=1;{a}")), "");
    assert_eq!(h(&mut g, &format!("m=1;{b}")), "");
    assert_eq!(h(&mut g, &format!("m=0;{c}")), "\x1b_Gi=6;OK\x1b\\");
    assert_eq!(g.image(6).unwrap().rgba.as_slice(), all.as_slice());
}

/// command 1429 "more chunks with chunk increasing q": 마지막 조각이 q=1을 주면 그것을 쓴다
#[test]
fn quiet_from_a_later_chunk_wins() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,t=d,s=1,v=2,c=10,r=1,m=1,i=1,q=0;////"), "");
    assert_eq!(h(&mut g, "m=0,q=1;////"), "", "마지막 조각의 q=1");
    assert!(g.image(1).is_some());
    // 가운데 조각이 올린 q도 이어진다
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,t=d,s=1,v=3,i=1,m=1;////"), "");
    assert_eq!(h(&mut g, "m=1,q=1;////"), "");
    assert_eq!(h(&mut g, "m=0;////"), "", "q=0인 조각은 앞서 올린 값을 쓴다");
    // q=2는 오류 답도 막는다(마지막 조각이 올려도): 모자란 자료의 ENODATA를 숨긴다
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=3,i=1,m=1;AAAA"), "");
    assert_eq!(h(&mut g, "m=0,q=2;AAAA"), "");
    // 첫 조각의 q=1은 그대로 이어진다(q=0이 오면 첫 조각 것을 쓴다)
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=2,i=1,m=1,q=1;AAAA"), "");
    assert_eq!(h(&mut g, "m=0;AAAA"), "");
    // q를 한 번도 안 주면 OK를 답한다
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=2,i=1,m=1;AAAA"), "");
    assert_eq!(h(&mut g, "m=0;AAAA"), OK1);
}

/// command 1820 "retransmit same id removes image on first chunk": 옛 그림과 자리를 첫 조각에서 지운다
#[test]
fn retransmit_removes_the_old_image_on_the_first_chunk() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=T,t=d,f=24,i=1,s=1,v=2,C=1;////////"), OK1);
    assert!(g.image(1).is_some());
    assert_eq!(g.placements.len(), 1);
    assert_eq!(h(&mut g, "a=t,t=d,f=24,i=1,s=1,v=2,m=1;AAAA"), "");
    assert!(g.image(1).is_none(), "첫 조각에서 옛 그림이 사라진다");
    assert!(g.placements.is_empty(), "자리도 사라진다");
    // 끝나면 새 그림이 선다
    assert_eq!(h(&mut g, "m=0;AAAA"), OK1);
    assert!(g.image(1).is_some());
}

/// 조각이 끝나기 전에 받던 전송이 끊겨도(지우기) 첫 조각에서 지운 옛 그림은 돌아오지 않는다
#[test]
fn aborted_retransmit_does_not_bring_the_old_image_back() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,t=d,f=24,i=1,s=1,v=2;////////"), OK1);
    assert_eq!(h(&mut g, "a=t,t=d,f=24,i=1,s=1,v=2,m=1;AAAA"), "");
    assert_eq!(h(&mut g, "a=d,d=a"), "");
    assert!(g.image(1).is_none());
    // 번호 없이 보내는 전송이 첫 조각에서 지우는 것은 없다
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=1,i=1;AAAA"), OK1);
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=2,m=1;AAAA"), "");
    assert!(g.image(1).is_some());
}

/// 질의(a=q)는 같은 번호를 지우지 않는다: 첫 조각 지우기는 t·T에만 걸린다
#[test]
fn query_with_same_id_does_not_delete() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=1,i=1;AAAA"), OK1);
    assert_eq!(h(&mut g, "a=q,f=24,s=1,v=1,i=1;AAAA"), OK1);
    assert!(g.image(1).is_some());
}

/// command 1315: 직접 전송이 아니면 `m`을 읽지 않는다. 곧바로 EINVAL이고 조각을 기다리지 않는다
#[test]
fn m_is_ignored_unless_the_medium_is_direct() {
    let mut g = Graphics::default();
    for t in ["f", "t", "s"] {
        assert_eq!(
            h(
                &mut g,
                &format!("a=t,t={t},m=1,i=7,f=24,s=1,v=1;{}", b64(b"/etc/hostname"))
            ),
            "\x1b_Gi=7;EINVAL\x1b\\",
            "t={t}"
        );
        // 다음 직접 전송이 조각의 이음으로 읽히지 않는다
        assert_eq!(h(&mut g, "a=t,f=24,s=1,v=1,i=8;AAAA"), "\x1b_Gi=8;OK\x1b\\", "t={t}");
        h(&mut g, "a=d,d=a");
    }
}

/// `m`은 조각을 받는 전송(t·T·f)에서만 읽는다: 놓기·질의·지우기에 붙은 `m=1`은 조각이 아니다
#[test]
fn m_on_non_transmission_actions_does_not_start_chunks() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=1,i=1;AAAA"), OK1);
    h(&mut g, "a=p,i=1,m=1,C=1");
    assert_eq!(g.placements.len(), 1);
    assert_eq!(h(&mut g, "a=q,f=24,s=1,v=1,i=9,m=1;AAAA"), "\x1b_Gi=9;OK\x1b\\");
    assert_eq!(h(&mut g, "a=d,d=a,m=1"), "");
    // 위 명령들이 조각 전송을 열어 두지 않았으므로 새 전송은 곧바로 끝난다
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=1,i=2;AAAA"), "\x1b_Gi=2;OK\x1b\\");
}

/// graphics_exec 1188 "image id and number are mutually exclusive for every action": a=f도 EINVAL이다(ENOENT가 아니다)
#[test]
fn id_and_number_are_exclusive_for_animation_frames_too() {
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=1,i=1;AAAA"), OK1);
    assert_eq!(
        h(&mut g, "a=f,i=1,I=2,p=3,f=24,s=1,v=1;AAAA"),
        "\x1b_Gi=1,I=2,p=3;EINVAL\x1b\\"
    );
    assert_eq!(g.image(1).unwrap().frame_count(), 1, "프레임이 붙지 않는다");
    // 없는 그림이어도 EINVAL이 먼저다
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=f,i=1,I=2,p=3;AAAA"), "\x1b_Gi=1,I=2,p=3;EINVAL\x1b\\");
    // 조각으로 시작하는 프레임도 첫 조각에서 거절하고 조각을 열지 않는다
    let mut g = Graphics::default();
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=1,i=1;AAAA"), OK1);
    assert_eq!(
        h(&mut g, "a=f,i=1,I=2,f=24,s=1,v=1,m=1;AAAA"),
        "\x1b_Gi=1,I=2;EINVAL\x1b\\"
    );
    assert_eq!(h(&mut g, "a=t,f=24,s=1,v=1,i=3;AAAA"), "\x1b_Gi=3;OK\x1b\\");
}
