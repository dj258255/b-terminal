//! kitty 그래픽 애니메이션(#554)의 저장소 시험. 명령 바이트를 `Graphics::handle`에 넣고 답과 프레임 픽셀을 본다.
//! 기대값은 Ghostty `graphics_exec.zig`의 `kittygfx animation:` 시험과 같은 명령·같은 픽셀이다

use super::*;
use crate::kitty_anim::{DEFAULT_GAP_MS, FRAME_OVERHEAD, State};

const CUR: Cursor = Cursor {
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
            s.push(if i <= ch.len() {
                T[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    s
}

/// 명령 한 줄(제어;자료)을 보내고 답을 글로 돌려준다
fn send(g: &mut Graphics, ctl: &str, data: &[u8]) -> Option<String> {
    let raw = if data.is_empty() {
        ctl.to_string()
    } else {
        format!("{ctl};{}", b64(data))
    };
    g.handle(raw.as_bytes(), CUR)
        .reply
        .map(|r| String::from_utf8(r).unwrap())
}

fn ok(i: u32) -> String {
    format!("\x1b_Gi={i};OK\x1b\\")
}

/// 그림 1: 1×1 RGB 빨강
fn red(g: &mut Graphics) {
    assert_eq!(send(g, "a=t,f=24,s=1,v=1,i=1", &[255, 0, 0]), Some(ok(1)));
}

/// 그림 1: 2×1 RGB 흰색 둘
fn white2(g: &mut Graphics) {
    assert_eq!(send(g, "a=t,f=24,s=2,v=1,i=1", &[255; 6]), Some(ok(1)));
}

fn px(g: &Graphics, id: u32, n: u32) -> Vec<u8> {
    g.image(id).unwrap().frame_data(n).expect("프레임이 있다").to_vec()
}

fn gap(g: &Graphics, id: u32, n: u32) -> u32 {
    let a = g.image(id).unwrap().animation().expect("애니메이션이 있다");
    a.gap_at(n - 1)
}

const BLUE: [u8; 4] = [0, 0, 255, 255];
const RED: [u8; 4] = [255, 0, 0, 255];

#[test]
fn new_frame_with_default_gap_answers_with_the_frame_number() {
    let mut g = Graphics::default();
    red(&mut g);
    let before = g.stored;
    // 1×1 RGB 파랑 프레임
    let reply = send(&mut g, "a=f,i=1,f=24,s=1,v=1", &[0, 0, 255]);
    assert_eq!(reply.as_deref(), Some("\x1b_Gi=1,r=2;OK\x1b\\"));
    let img = g.image(1).unwrap();
    assert_eq!(img.frame_count(), 2);
    assert_eq!(px(&g, 1, 1), RED);
    assert_eq!(px(&g, 1, 2), BLUE);
    let a = img.animation().unwrap();
    assert_eq!((a.root_gap_ms, a.frames[0].gap_ms), (0, DEFAULT_GAP_MS));
    // 지금 보이는 프레임은 아직 뿌리다
    assert_eq!(*img.frame_rgba(), RED);
    // 프레임 바이트가 저장량에 든다
    assert_eq!(g.stored, before + 4 + FRAME_OVERHEAD);
}

#[test]
fn frame_gap_normalization_on_create() {
    let mut g = Graphics::default();
    red(&mut g);
    for ctl in ["a=f,i=1,f=24,s=1,v=1,z=100", "a=f,i=1,f=24,s=1,v=1,z=-5"] {
        assert!(send(&mut g, ctl, &[0, 0, 255]).unwrap().ends_with(";OK\x1b\\"));
    }
    assert_eq!((gap(&g, 1, 2), gap(&g, 1, 3)), (100, 0));
}

#[test]
fn background_fill_and_offset_composition() {
    let mut g = Graphics::default();
    white2(&mut g);
    // 1×1 흰색을 x=1에, 바탕은 불투명 빨강(Y=0xff0000ff)
    let reply = send(&mut g, "a=f,i=1,f=24,s=1,v=1,x=1,Y=4278190335", &[255; 3]);
    assert!(reply.unwrap().ends_with(";OK\x1b\\"));
    assert_eq!(px(&g, 1, 2), [255, 0, 0, 255, 255, 255, 255, 255]);
}

#[test]
fn create_from_a_base_frame_with_overwrite() {
    let mut g = Graphics::default();
    red(&mut g);
    // 프레임 1을 바탕으로(c=1), 반투명 파랑(0000FF80)을 덮어쓰기(X=1): 알파까지 바뀐다
    send(&mut g, "a=f,i=1,f=32,s=1,v=1,c=1,X=1", &[0, 0, 255, 128]);
    assert_eq!(px(&g, 1, 2), [0, 0, 255, 128]);
}

#[test]
fn alpha_blend_composes_over_the_base_frame() {
    let mut g = Graphics::default();
    red(&mut g);
    // 불투명 파랑을 섞으면 복사와 같다
    send(&mut g, "a=f,i=1,f=32,s=1,v=1,c=1", &[0, 0, 255, 255]);
    // 완전 투명을 섞으면 바탕(빨강)이 그대로다
    send(&mut g, "a=f,i=1,f=32,s=1,v=1,c=1", &[0, 0, 255, 0]);
    assert_eq!(px(&g, 1, 2), BLUE);
    assert_eq!(px(&g, 1, 3), RED);
}

#[test]
fn edit_root_frame_changes_the_displayed_pixels() {
    let mut g = Graphics::default();
    red(&mut g);
    let reply = send(&mut g, "a=f,i=1,f=24,s=1,v=1,r=1", &[0, 0, 255]);
    assert_eq!(reply.as_deref(), Some("\x1b_Gi=1,r=1;OK\x1b\\"));
    assert_eq!(*g.image(1).unwrap().frame_rgba(), BLUE);
    assert_eq!(g.image(1).unwrap().frame_count(), 1, "고쳤을 뿐 프레임이 늘지 않는다");
}

#[test]
fn editing_a_frame_changes_its_gap_only_for_nonzero_z() {
    let mut g = Graphics::default();
    red(&mut g);
    send(&mut g, "a=f,i=1,f=24,s=1,v=1,z=77", &[0, 0, 255]);
    let stored = g.stored;
    // z 없이 고치면 간격은 그대로(40ms 기본은 새 프레임에만)이고 저장량도 그대로다
    send(&mut g, "a=f,i=1,f=24,s=1,v=1,r=2", &[255, 0, 0]);
    assert_eq!(gap(&g, 1, 2), 77);
    assert_eq!(px(&g, 1, 2), RED);
    assert_eq!(g.stored, stored);
    send(&mut g, "a=f,i=1,f=24,s=1,v=1,r=2,z=-1", &[255, 0, 0]);
    assert_eq!(gap(&g, 1, 2), 0);
}

#[test]
fn frame_errors_answer_codes_and_create_nothing() {
    let mut g = Graphics::default();
    red(&mut g);
    let cases: [(&str, &[u8], &str); 5] = [
        // 없는 그림
        ("a=f,i=9,f=24,s=1,v=1", &[0, 0, 255], "\x1b_Gi=9;ENOENT\x1b\\"),
        // 그림보다 큰 프레임
        ("a=f,i=1,f=24,s=2,v=1", &[255; 6], "\x1b_Gi=1;EINVAL\x1b\\"),
        // 없는 바탕 프레임
        ("a=f,i=1,f=24,s=1,v=1,c=5", &[0, 0, 255], "\x1b_Gi=1;EINVAL\x1b\\"),
        // 크기보다 짧은 자료
        ("a=f,i=1,f=24,s=1,v=1", &[0], "\x1b_Gi=1;ENODATA\x1b\\"),
        // 고치려던 프레임 번호(r)를 오류에도 되돌린다
        ("a=f,i=9,r=3,f=24,s=1,v=1", &[0, 0, 255], "\x1b_Gi=9,r=3;ENOENT\x1b\\"),
    ];
    for (ctl, data, want) in cases {
        assert_eq!(send(&mut g, ctl, data).as_deref(), Some(want), "{ctl}");
    }
    assert_eq!(g.image(1).unwrap().frame_count(), 1);
    // 그림 번호가 아무것도 없으면 답할 곳이 없다
    assert_eq!(send(&mut g, "a=f,f=24,s=1,v=1", &[0, 0, 255]), None);
}

#[test]
fn excess_frame_data_is_truncated() {
    let mut g = Graphics::default();
    red(&mut g);
    // 1×1 RGB에 6바이트: 남는 것은 버린다
    send(&mut g, "a=f,i=1,f=24,s=1,v=1", &[0, 0, 255, 9, 9, 9]);
    assert_eq!(px(&g, 1, 2), BLUE);
}

#[test]
fn frames_can_be_addressed_by_image_number() {
    let mut g = Graphics::default();
    // I=5로 보내 번호를 받는다
    let reply = send(&mut g, "a=t,f=24,s=1,v=1,I=5", &[255, 0, 0]).unwrap();
    let id: u32 = reply
        .trim_start_matches("\x1b_Gi=")
        .split(',')
        .next()
        .unwrap()
        .parse()
        .unwrap();
    let reply = send(&mut g, "a=f,I=5,f=24,s=1,v=1", &[0, 0, 255]);
    assert_eq!(reply, Some(format!("\x1b_Gi={id},I=5,r=2;OK\x1b\\")));
    assert_eq!(g.image(id).unwrap().frame_count(), 2);
}

#[test]
fn chunked_frame_waits_for_the_last_chunk() {
    let mut g = Graphics::default();
    white2(&mut g);
    // 2×1 프레임을 한 픽셀씩 두 조각으로. 끝나기 전에는 답하지 않는다
    assert_eq!(send(&mut g, "a=f,i=1,f=24,s=2,v=1,z=60,m=1", &[255, 0, 0]), None);
    assert!(g.pending.is_some());
    let reply = send(&mut g, "a=f,m=0", &[0, 0, 255]);
    assert_eq!(reply.as_deref(), Some("\x1b_Gi=1,r=2;OK\x1b\\"));
    assert!(g.pending.is_none());
    assert_eq!(gap(&g, 1, 2), 60);
    assert_eq!(px(&g, 1, 2), [255, 0, 0, 255, 0, 0, 255, 255]);
}

#[test]
fn chunked_frame_continues_without_repeating_a_f() {
    let mut g = Graphics::default();
    white2(&mut g);
    assert_eq!(send(&mut g, "a=f,i=1,f=24,s=2,v=1,m=1", &[255, 0, 0]), None);
    // 조각이 a=f를 되풀이하지 않아도(보통 전송으로 읽힌다) 프레임 전송을 이어 간다
    let reply = send(&mut g, "m=0", &[0, 0, 255]);
    assert_eq!(reply.as_deref(), Some("\x1b_Gi=1,r=2;OK\x1b\\"));
    assert_eq!(g.image(1).unwrap().frame_count(), 2);
    assert_eq!(g.image(1).unwrap().animation().unwrap().frames.len(), 1);
}

#[test]
fn non_direct_transmission_abandons_a_chunked_frame_without_deleting_the_image() {
    let mut g = Graphics::default();
    white2(&mut g);
    assert_eq!(send(&mut g, "a=f,i=1,f=24,s=2,v=1,m=1", &[255, 0, 0]), None);
    // 파일 전송(t=f)은 조각을 이을 수 없다: 받던 프레임을 버리고 새 프레임 명령으로 처리한다(우리는 파일을 받지 않아 EINVAL)
    let reply = send(&mut g, "a=f,t=f,i=1,f=24,s=2,v=1", b"/tmp/x");
    assert_eq!(reply.as_deref(), Some("\x1b_Gi=1;EINVAL\x1b\\"));
    assert!(g.pending.is_none());
    // 프레임 전송을 버려도 바탕 그림은 지우지 않는다(그림을 다시 보내는 전송과 다르다)
    assert!(g.image(1).is_some());
    assert_eq!(g.image(1).unwrap().frame_count(), 1);
}

#[test]
fn chunked_frame_aborted_by_a_delete_keeps_the_image() {
    let mut g = Graphics::default();
    white2(&mut g);
    assert_eq!(send(&mut g, "a=f,i=1,f=24,s=2,v=1,m=1", &[255, 0, 0]), None);
    g.handle(b"a=d,d=a", CUR);
    assert!(g.pending.is_none());
    assert!(g.image(1).is_some(), "프레임 전송을 버릴 때 바탕 그림을 지우면 안 된다");
}

#[test]
fn a_chunked_frame_whose_image_was_replaced_is_dropped() {
    let mut g = Graphics::default();
    white2(&mut g);
    assert_eq!(send(&mut g, "a=f,i=1,f=24,s=2,v=1,m=1", &[255, 0, 0]), None);
    // 직접 전송은 받던 프레임의 조각으로 읽히므로, 그림이 바뀌는 길은 쫓겨난 뒤 다시 들어오는 것이다(저장 상한)
    g.remove_image(1);
    let again = g
        .decode(&parse_control(b"f=24,s=2,v=1"), b64(&[255; 6]).as_bytes())
        .unwrap();
    g.store(1, again);
    // 마지막 조각이 오면 옛 그림 몫이라 버린다(Ghostty `image_generation` 검사). 새 그림은 그대로다
    let reply = send(&mut g, "a=f,m=0", &[0, 0, 255]);
    assert_eq!(reply.as_deref(), Some("\x1b_Gi=1;ENOENT\x1b\\"));
    assert_eq!(g.image(1).unwrap().frame_count(), 1);
}

#[test]
fn a_chunked_frame_whose_image_is_gone_answers_enoent() {
    let mut g = Graphics::default();
    white2(&mut g);
    assert_eq!(send(&mut g, "a=f,i=1,f=24,s=2,v=1,m=1", &[255, 0, 0]), None);
    g.remove_image(1);
    let reply = send(&mut g, "a=f,m=0", &[0, 0, 255]);
    assert_eq!(reply.as_deref(), Some("\x1b_Gi=1;ENOENT\x1b\\"));
}

#[test]
fn chunked_frame_start_checks_the_image_before_waiting() {
    let mut g = Graphics::default();
    // 없는 그림이면 첫 조각에서 바로 오류다
    let reply = send(&mut g, "a=f,i=4,f=24,s=2,v=1,m=1", &[255, 0, 0]);
    assert_eq!(reply.as_deref(), Some("\x1b_Gi=4;ENOENT\x1b\\"));
    assert!(g.pending.is_none());
}

#[test]
fn control_and_compose_run_while_a_frame_is_chunked() {
    let mut g = Graphics::default();
    white2(&mut g);
    send(&mut g, "a=f,i=1,f=24,s=2,v=1", &[0; 6]);
    assert_eq!(send(&mut g, "a=f,i=1,f=24,s=2,v=1,m=1", &[255, 0, 0]), None);
    // 재생 제어와 합성은 받던 전송을 건드리지 않고 바로 실행한다
    assert_eq!(send(&mut g, "a=a,i=1,s=3", &[]), None);
    assert_eq!(send(&mut g, "a=c,i=1,r=1,c=2,w=1,h=1", &[]), Some(ok(1)));
    assert!(g.pending.is_some());
    assert_eq!(g.image(1).unwrap().animation().unwrap().state, State::Running);
    let reply = send(&mut g, "a=f,m=0", &[0, 0, 255]);
    assert_eq!(reply.as_deref(), Some("\x1b_Gi=1,r=3;OK\x1b\\"));
}

#[test]
fn control_command_sets_gap_current_state_and_loops() {
    let mut g = Graphics::default();
    red(&mut g);
    send(&mut g, "a=f,i=1,f=24,s=1,v=1", &[0, 0, 255]);
    // 한 명령으로: 뿌리 간격(r=1,z=50), 지금 프레임(c=2), 상태(s=3), 반복(v=3, 두 바퀴). 성공하면 답하지 않는다
    assert_eq!(send(&mut g, "a=a,i=1,r=1,z=50,c=2,s=3,v=3", &[]), None);
    let img = g.image(1).unwrap();
    let a = img.animation().unwrap();
    assert_eq!(
        (a.root_gap_ms, a.current_index, a.state, a.max_loops),
        (50, 1, State::Running, 2)
    );
    assert_eq!(*img.frame_rgba(), BLUE, "지금 프레임이 바뀌어 보이는 픽셀도 바뀐다");
}

#[test]
fn control_command_ignores_invalid_values() {
    let mut g = Graphics::default();
    red(&mut g);
    assert_eq!(send(&mut g, "a=a,i=1,r=9,z=50,c=9,s=7", &[]), None);
    let a = g.image(1).unwrap().animation().unwrap();
    assert_eq!((a.root_gap_ms, a.current_index, a.state), (0, 0, State::Stopped));
}

#[test]
fn control_state_changes_reset_loops_and_restart_the_clock() {
    let mut g = Graphics::default();
    red(&mut g);
    send(&mut g, "a=f,i=1,f=24,s=1,v=1,z=10", &[0, 0, 255]);
    send(&mut g, "a=a,i=1,r=1,z=10,s=3", &[]);
    g.tick_image(1, 0);
    g.tick_image(1, 10);
    g.tick_image(1, 20);
    assert_eq!(g.image(1).unwrap().animation().unwrap().current_loop, 1);
    // 상태를 다시 정하면 반복 수를 0으로, 멈춤에서 나오면 시계를 다시 시작한다
    send(&mut g, "a=a,i=1,s=1", &[]);
    assert_eq!(g.image(1).unwrap().animation().unwrap().current_loop, 0);
    send(&mut g, "a=a,i=1,s=3", &[]);
    assert_eq!(g.image(1).unwrap().animation().unwrap().frame_shown_at_ms, None);
}

#[test]
fn control_negative_gap_makes_a_frame_gapless() {
    let mut g = Graphics::default();
    red(&mut g);
    send(&mut g, "a=f,i=1,f=24,s=1,v=1", &[0, 0, 255]);
    assert_eq!(send(&mut g, "a=a,i=1,r=2,z=-1", &[]), None);
    assert_eq!(gap(&g, 1, 2), 0);
}

#[test]
fn control_missing_image_answers_enoent_and_needs_an_id() {
    let mut g = Graphics::default();
    assert_eq!(
        send(&mut g, "a=a,i=9,s=3", &[]).as_deref(),
        Some("\x1b_Gi=9;ENOENT\x1b\\")
    );
    // 번호가 없으면 답할 곳이 없다
    assert_eq!(send(&mut g, "a=a,s=3", &[]), None);
    // q=2면 오류 답도 하지 않는다
    assert_eq!(send(&mut g, "a=a,i=9,s=3,q=2", &[]), None);
}

#[test]
fn compose_frames() {
    let mut g = Graphics::default();
    // 2×1 RGB: 빨강, 파랑
    assert_eq!(
        send(&mut g, "a=t,f=24,s=2,v=1,i=1", &[255, 0, 0, 0, 0, 255]),
        Some(ok(1))
    );
    // 프레임 2: 투명 바탕에 x=0 흰 픽셀
    send(&mut g, "a=f,i=1,f=24,s=1,v=1", &[255; 3]);
    // 뿌리(r=1, 원본)의 (1,0)을 프레임 2(c=2, 대상)의 (0,0)에: X=1은 원본의 왼쪽
    assert_eq!(send(&mut g, "a=c,i=1,r=1,c=2,w=1,h=1,X=1,x=0", &[]), Some(ok(1)));
    assert_eq!(px(&g, 1, 2), [0, 0, 255, 255, 0, 0, 0, 0]);
}

#[test]
fn compose_blends_unless_c_is_nonzero() {
    let mut g = Graphics::default();
    // 뿌리: 반투명 빨강(알파 128), 프레임 2: 불투명 파랑
    assert_eq!(send(&mut g, "a=t,f=32,s=1,v=1,i=1", &[255, 0, 0, 128]), Some(ok(1)));
    send(&mut g, "a=f,i=1,f=32,s=1,v=1,X=1", &[0, 0, 255, 255]);
    // 뿌리를 프레임 2 위에 섞는다: 반투명 빨강이 파랑 위에 얹힌다(섞이면 알파 255)
    send(&mut g, "a=c,i=1,r=1,c=2", &[]);
    let blended = px(&g, 1, 2);
    assert_eq!(blended[3], 255);
    assert!(blended[0] > 0 && blended[2] > 0, "{blended:?}");
    // C=1은 덮어쓴다(알파까지)
    send(&mut g, "a=f,i=1,f=32,s=1,v=1,X=1,r=2", &[0, 0, 255, 255]);
    send(&mut g, "a=c,i=1,r=1,c=2,C=1", &[]);
    assert_eq!(px(&g, 1, 2), [255, 0, 0, 128]);
}

#[test]
fn compose_works_on_images_without_animation_state() {
    let mut g = Graphics::default();
    white2(&mut g);
    // 두 프레임 모두 뿌리: 픽셀 0을 픽셀 1로 복사
    assert_eq!(send(&mut g, "a=c,i=1,r=1,c=1,w=1,h=1,x=1,C=1", &[]), Some(ok(1)));
    assert!(
        g.image(1).unwrap().animation().is_none(),
        "합성만으로 애니메이션이 생기지는 않는다"
    );
}

#[test]
fn compose_errors() {
    let mut g = Graphics::default();
    red(&mut g);
    for (ctl, want) in [
        ("a=c,i=9,r=1,c=1", "\x1b_Gi=9;ENOENT\x1b\\"),
        ("a=c,i=1,r=2,c=1", "\x1b_Gi=1;ENOENT\x1b\\"),
        ("a=c,i=1,r=1,c=2", "\x1b_Gi=1;ENOENT\x1b\\"),
        // 0은 없는 프레임 번호다
        ("a=c,i=1,r=0,c=1", "\x1b_Gi=1;ENOENT\x1b\\"),
        // 1×1 그림: 어떤 오프셋이든 사각형이 벗어난다
        ("a=c,i=1,r=1,c=1,x=1", "\x1b_Gi=1;EINVAL\x1b\\"),
        ("a=c,i=1,r=1,c=1,X=1", "\x1b_Gi=1;EINVAL\x1b\\"),
        // 같은 프레임의 같은(전체) 사각형은 겹친다
        ("a=c,i=1,r=1,c=1", "\x1b_Gi=1;EINVAL\x1b\\"),
        // 믿을 수 없는 값이 넘치지 않는다
        ("a=c,i=1,r=1,c=1,x=4294967295,w=4294967295", "\x1b_Gi=1;EINVAL\x1b\\"),
    ] {
        assert_eq!(send(&mut g, ctl, &[]).as_deref(), Some(want), "{ctl}");
    }
    assert_eq!(px(&g, 1, 1), RED, "오류는 픽셀을 바꾸지 않는다");
}

#[test]
fn delete_frame_promotes_the_root_and_credits_the_storage() {
    let mut g = Graphics::default();
    red(&mut g);
    // 프레임 2·3: 파랑(10ms), 흰색(20ms)
    send(&mut g, "a=f,i=1,f=24,s=1,v=1,z=10", &[0, 0, 255]);
    send(&mut g, "a=f,i=1,f=24,s=1,v=1,z=20", &[255; 3]);
    let total = g.stored;
    // r이 없으면 뿌리: 프레임 2가 뿌리가 되고 간격도 따라온다. 지우기는 답하지 않는다
    assert_eq!(send(&mut g, "a=d,d=f,i=1", &[]), None);
    let img = g.image(1).unwrap();
    assert_eq!(img.frame_count(), 2);
    assert_eq!(img.animation().unwrap().root_gap_ms, 10);
    assert_eq!(**img.frame_data(1).unwrap(), BLUE);
    assert_eq!(g.stored, total - 4 - FRAME_OVERHEAD);
    // 끝을 넘는 번호는 마지막 프레임
    send(&mut g, "a=d,d=f,i=1,r=9", &[]);
    assert_eq!(g.image(1).unwrap().frame_count(), 1);
    assert_eq!(g.stored, total - 2 * (4 + FRAME_OVERHEAD));
    // 하나 남으면 소문자는 아무것도 하지 않고 그림은 남는다
    send(&mut g, "a=d,d=f,i=1", &[]);
    assert!(g.image(1).is_some());
}

#[test]
fn uppercase_frame_delete_removes_a_non_animated_image_even_when_placed() {
    let mut g = Graphics::default();
    assert_eq!(send(&mut g, "a=T,f=24,s=1,v=1,i=1,C=1", &[255, 0, 0]), Some(ok(1)));
    assert_eq!(g.placements.len(), 1);
    assert_eq!(send(&mut g, "a=d,d=F,i=1", &[]), None);
    assert!(g.image(1).is_none());
    assert!(g.placements.is_empty());
    assert_eq!(g.stored, 0);
}

#[test]
fn lowercase_frame_delete_without_frames_keeps_the_image() {
    let mut g = Graphics::default();
    send(&mut g, "a=T,f=24,s=1,v=1,i=1,C=1", &[255, 0, 0]);
    send(&mut g, "a=d,d=f,i=1", &[]);
    assert!(g.image(1).is_some() && g.placements.len() == 1);
    // 번호가 없거나 없는 그림이면 아무것도 하지 않는다
    send(&mut g, "a=d,d=F", &[]);
    send(&mut g, "a=d,d=F,i=9", &[]);
    assert!(g.image(1).is_some());
}

#[test]
fn uppercase_frame_delete_on_an_animated_image_deletes_one_frame_only() {
    let mut g = Graphics::default();
    send(&mut g, "a=T,f=24,s=1,v=1,i=1,C=1", &[255, 0, 0]);
    send(&mut g, "a=f,i=1,f=24,s=1,v=1", &[0, 0, 255]);
    send(&mut g, "a=d,d=F,i=1,r=2", &[]);
    assert!(g.image(1).is_some() && g.placements.len() == 1);
    assert_eq!(g.image(1).unwrap().frame_count(), 1);
}

#[test]
fn deleting_the_current_frame_moves_the_display() {
    let mut g = Graphics::default();
    red(&mut g);
    send(&mut g, "a=f,i=1,f=24,s=1,v=1", &[0, 0, 255]);
    send(&mut g, "a=f,i=1,f=24,s=1,v=1", &[255; 3]);
    send(&mut g, "a=a,i=1,c=3", &[]);
    assert_eq!(g.image(1).unwrap().animation().unwrap().current_index, 2);
    send(&mut g, "a=d,d=f,i=1,r=3", &[]);
    let img = g.image(1).unwrap();
    assert_eq!(img.animation().unwrap().current_index, 1);
    assert_eq!(*img.frame_rgba(), BLUE);
}

#[test]
fn delete_frame_by_image_number() {
    let mut g = Graphics::default();
    send(&mut g, "a=t,f=24,s=1,v=1,I=3", &[255, 0, 0]);
    send(&mut g, "a=f,I=3,f=24,s=1,v=1", &[0, 0, 255]);
    send(&mut g, "a=d,d=f,I=3,r=2", &[]);
    assert!(g.images.values().all(|i| i.frame_count() == 1));
}

#[test]
fn retransmitting_the_base_image_resets_the_animation() {
    let mut g = Graphics::default();
    red(&mut g);
    send(&mut g, "a=f,i=1,f=24,s=1,v=1", &[0, 0, 255]);
    assert_eq!(g.stored, 4 + IMAGE_OVERHEAD + 4 + FRAME_OVERHEAD);
    red(&mut g);
    let img = g.image(1).unwrap();
    assert!(img.animation().is_none());
    assert_eq!(g.stored, 4 + IMAGE_OVERHEAD, "프레임 몫을 돌려받는다");
}

#[test]
fn removing_an_image_frees_its_frames() {
    let mut g = Graphics::default();
    red(&mut g);
    send(&mut g, "a=f,i=1,f=24,s=1,v=1", &[0, 0, 255]);
    send(&mut g, "a=d,d=I,i=1", &[]);
    assert_eq!(g.stored, 0);
}

#[test]
fn frames_evict_the_oldest_other_image_when_the_limit_is_hit() {
    // 그림 3장 + 프레임 하나가 안 들어가는 상한
    let per_image = 4 + IMAGE_OVERHEAD;
    let mut g = Graphics::with_limit(3 * per_image + 10);
    for id in 1..=3 {
        assert_eq!(
            send(&mut g, &format!("a=t,f=24,s=1,v=1,i={id}"), &[1, 2, 3]),
            Some(ok(id))
        );
    }
    // 그림 3에 프레임을 더하면 가장 오래된 다른 그림(1)부터 버린다
    let reply = send(&mut g, "a=f,i=3,f=24,s=1,v=1", &[0, 0, 255]);
    assert_eq!(reply.as_deref(), Some("\x1b_Gi=3,r=2;OK\x1b\\"));
    assert!(g.image(1).is_none(), "가장 오래된 그림을 버렸다");
    assert!(g.image(2).is_some() && g.image(3).is_some());
    assert!(g.stored <= 3 * per_image + 10);
    assert_eq!(g.image(3).unwrap().frame_count(), 2);
}

#[test]
fn a_refused_frame_does_not_evict_other_images() {
    // 40×30 RGBA 그림(4800바이트) 하나와 1×1 그림 하나는 들어가지만, 그림 하나에 프레임이 하나 더 붙으면 상한을 넘는다
    let big = 40 * 30 * 4;
    let limit = (big + IMAGE_OVERHEAD) + (4 + IMAGE_OVERHEAD);
    let mut g = Graphics::with_limit(limit);
    assert_eq!(send(&mut g, "a=t,f=32,s=40,v=30,i=1", &vec![7u8; big]), Some(ok(1)));
    assert_eq!(send(&mut g, "a=t,f=24,s=1,v=1,i=2", &[1, 2, 3]), Some(ok(2)));
    let reply = send(&mut g, "a=f,i=1,f=32,s=40,v=30", &vec![9u8; big]);
    assert_eq!(reply.as_deref(), Some("\x1b_Gi=1;ENOSPC\x1b\\"));
    // 어차피 안 들어가는 프레임 때문에 다른 그림을 버리지 않는다
    assert!(g.image(2).is_some(), "거절하면서 다른 그림을 버렸다");
    assert_eq!(g.image(1).unwrap().frame_count(), 1);
    assert_eq!(g.stored, limit);
}

#[test]
fn a_frame_larger_than_the_whole_limit_is_refused() {
    let mut g = Graphics::with_limit(4 + IMAGE_OVERHEAD + 8);
    red(&mut g);
    let reply = send(&mut g, "a=f,i=1,f=24,s=1,v=1", &[0, 0, 255]);
    assert_eq!(reply.as_deref(), Some("\x1b_Gi=1;ENOSPC\x1b\\"));
    assert_eq!(g.image(1).unwrap().frame_count(), 1);
}

#[test]
fn many_tiny_frames_cannot_dodge_the_limit() {
    // 1×1 프레임을 계속 보내도 프레임마다 고정 몫을 세므로 저장량 상한 안에서 멈춘다
    let mut g = Graphics::with_limit(10_000);
    red(&mut g);
    for _ in 0..200 {
        send(&mut g, "a=f,i=1,f=24,s=1,v=1,q=2", &[0, 0, 255]);
    }
    assert!(g.stored <= 10_000);
    let n = g.image(1).unwrap().frame_count();
    assert!(n > 1 && n < 200, "{n}");
}

#[test]
fn feature_query_answers_frame_control_and_compose_keys() {
    let mut g = Graphics::default();
    let q = |g: &mut Graphics, c: &str| {
        g.handle(c.as_bytes(), CUR)
            .reply
            .map(|r| String::from_utf8(r).unwrap())
            .unwrap()
    };
    // a=f: 전송 키와 프레임 키(x y c r z X Y)
    assert_eq!(
        q(&mut g, "a=q,i=2,e=f,f=24,s=3,v=4,t=d,x=1,y=2,c=1,r=2,z=-5,X=1,Y=7"),
        "\x1b_Gi=2,e=f,c=1,f=24,r=2,s=3,t=d,v=4,x=1,y=2,z=-5,X=1,Y=7;OK\x1b\\"
    );
    // a=a: s r c v z
    assert_eq!(
        q(&mut g, "a=q,i=2,e=a,s=3,r=1,c=2,v=4,z=9,x=5"),
        "\x1b_Gi=2,e=a,c=2,r=1,s=3,v=4,z=9;OK\x1b\\"
    );
    // a=c: r c x y w h X Y C
    assert_eq!(
        q(&mut g, "a=q,i=2,e=c,r=1,c=2,x=3,y=4,w=5,h=6,X=7,Y=8,C=1,s=1"),
        "\x1b_Gi=2,e=c,c=2,h=6,r=1,w=5,x=3,y=4,C=1,X=7,Y=8;OK\x1b\\"
    );
    // a=a의 s는 1..3만, 모르는 값은 뺀다
    assert_eq!(q(&mut g, "a=q,i=2,e=a,s=7"), "\x1b_Gi=2,e=a;OK\x1b\\");
}

#[test]
fn tick_moves_the_displayed_frame_in_time() {
    let mut g = Graphics::default();
    red(&mut g);
    send(&mut g, "a=f,i=1,f=24,s=1,v=1,z=100", &[0, 0, 255]);
    send(&mut g, "a=a,i=1,r=1,z=100,s=3", &[]);
    assert_eq!(g.tick_image(1, 0), Some(100));
    assert_eq!(*g.image(1).unwrap().frame_rgba(), RED);
    assert_eq!(g.tick_image(1, 100), Some(100));
    assert_eq!(*g.image(1).unwrap().frame_rgba(), BLUE);
    assert_eq!(g.tick_image(1, 200), Some(100));
    assert_eq!(*g.image(1).unwrap().frame_rgba(), RED);
    // 멈추면 더 나아가지 않는다
    send(&mut g, "a=a,i=1,s=1", &[]);
    assert_eq!(g.tick_image(1, 300), None);
    assert_eq!(*g.image(1).unwrap().frame_rgba(), RED);
    // 없는 그림·애니메이션 없는 그림은 없다
    assert_eq!(g.tick_image(9, 0), None);
}

#[test]
fn tick_visible_ticks_each_image_once_and_reports_the_earliest_next_frame() {
    let mut g = Graphics::default();
    red(&mut g);
    send(&mut g, "a=f,i=1,f=24,s=1,v=1,z=100", &[0, 0, 255]);
    send(&mut g, "a=a,i=1,r=1,z=100,s=3", &[]);
    let t0 = g.clock_start();
    // 같은 그림이 두 자리에 있어도 한 번만 센다
    g.tick_visible(&[1, 1], t0);
    assert_eq!(g.next_frame(), Some(t0 + std::time::Duration::from_millis(100)));
    // 보이는 그림이 없으면 다음 시각도 없다(보이지 않는 그림은 시계를 돌리지 않는다)
    g.tick_visible(&[], t0 + std::time::Duration::from_millis(50));
    assert_eq!(g.next_frame(), None);
    // 다시 보이면 이어서: 간격이 지났으니 한 프레임 나아간다
    g.tick_visible(&[1], t0 + std::time::Duration::from_millis(150));
    assert_eq!(*g.image(1).unwrap().frame_rgba(), BLUE);
    assert_eq!(g.next_frame(), Some(t0 + std::time::Duration::from_millis(250)));
}

#[test]
fn frame_rgba_is_the_root_without_animation_and_after_clamping() {
    let mut g = Graphics::default();
    red(&mut g);
    let img = g.image(1).unwrap();
    assert!(Arc::ptr_eq(&img.frame_rgba(), &img.rgba));
    // 범위 밖 지금 프레임 번호가 어떻게든 들어와도 panic하지 않는다
    let mut g2 = Graphics::default();
    red(&mut g2);
    g2.images.get_mut(&1).unwrap().anim = Some(Box::new(Animation {
        current_index: 5,
        ..Default::default()
    }));
    assert_eq!(*g2.image(1).unwrap().frame_rgba(), RED);
}
