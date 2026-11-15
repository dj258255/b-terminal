//! 무거운 kitty 그림 명령이 이어져도 화면 그리기(`Session::snapshot`)가 오래 기다리지 않는지(#557).
//! PTY는 `/bin/sh`와 `printf`·`sleep`만 쓴다(사용자 셸 설정을 건드리지 않는다)

use std::sync::Arc;
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

const SIDE: usize = 1024;
/// 한 번의 `printf`로 보내는 합성 명령 수. 한 번 읽기(약 1KB)에 다 들어가게 짧게 둔다
const COMPOSES: usize = 30;

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for ch in data.chunks(3) {
        let n =
            (u32::from(ch[0]) << 16) | (u32::from(*ch.get(1).unwrap_or(&0)) << 8) | u32::from(*ch.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= ch.len() {
                out.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// 반투명(알파 128) 한 색으로 채운 SIDE×SIDE RGBA를 zlib로 눌러 base64로
fn payload(r: u8, g: u8, b: u8) -> String {
    let raw: Vec<u8> = std::iter::repeat_n([r, g, b, 128], SIDE * SIDE).flatten().collect();
    b64(&miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6))
}

#[test]
fn heavy_graphics_commands_do_not_block_snapshots() {
    // 그림 1과 프레임 2를 올린다. 그다음 반투명 합성(a=c) 여러 개를 한 번에 보낸다
    let load = format!(
        "\x1b_Ga=t,f=32,s={SIDE},v={SIDE},o=z,i=1,q=2;{}\x1b\\\x1b_Ga=f,i=1,f=32,s={SIDE},v={SIDE},o=z,q=2;{}\x1b\\",
        payload(255, 0, 0),
        payload(0, 0, 255),
    );
    let batch = "\x1b_Ga=c,i=1,r=1,c=2,q=2\x1b\\".repeat(COMPOSES);
    let script = "printf '%s' \"$1\"; printf 'LOADED\\n'; sleep 0.5; printf '%s' \"$2\"; printf 'DONE\\n'; sleep 1";
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into(), "sh".into(), load, batch],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    let sink = Arc::new(|_: SessionEvent| {});
    let s = Session::spawn(o, GridSize::new(80, 24), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"));

    let deadline = Instant::now() + Duration::from_secs(120);
    while !s.snapshot().text().contains("LOADED") {
        assert!(Instant::now() < deadline, "그림을 올리지 못했다");
        std::thread::sleep(Duration::from_millis(5));
    }
    // 합성 명령을 처리하는 동안 화면을 계속 읽으며 가장 오래 기다린 시간을 잰다
    let started = Instant::now();
    let mut longest = Duration::ZERO;
    loop {
        assert!(Instant::now() < deadline, "합성 명령이 끝나지 않았다");
        let t = Instant::now();
        let done = s.snapshot().text().contains("DONE");
        longest = longest.max(t.elapsed());
        if done {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    let total = started.elapsed();
    eprintln!("합성 {COMPOSES}개 {total:?}, 화면 읽기가 가장 오래 기다린 시간 {longest:?}");
    // 앞의 0.5초는 쉬는 시간이라 명령 처리는 그 뒤다. 명령 처리가 짧으면 이 시험은 아무것도 보지 못한다
    let busy = total.saturating_sub(Duration::from_millis(500));
    assert!(
        busy >= Duration::from_millis(300),
        "합성 명령이 너무 빨라 잠금 점유를 볼 수 없다: {busy:?}"
    );
    assert!(
        longest * 4 < busy,
        "화면 읽기가 명령 처리 내내 기다렸다: 가장 오래 {longest:?}, 처리 {busy:?}"
    );
}
