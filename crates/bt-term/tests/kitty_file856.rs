//! 셸이 `printf`로 보낸 kitty 파일 전송(`t=f`)이 PTY를 거쳐 그림이 되는지(#856).
//! PTY는 `/bin/sh`와 `printf`·`sleep`만 쓴다(사용자 셸 설정을 건드리지 않는다)

use std::sync::Arc;
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for ch in data.chunks(3) {
        let n =
            (u32::from(ch[0]) << 16) | (u32::from(*ch.get(1).unwrap_or(&0)) << 8) | u32::from(*ch.get(2).unwrap_or(&0));
        for i in 0..4 {
            out.push(if i <= ch.len() {
                T[(n >> (18 - 6 * i) & 63) as usize] as char
            } else {
                '='
            });
        }
    }
    out
}

fn run(script: &str, arg: String) -> Session {
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into(), "sh".into(), arg],
        cwd: Some(std::env::temp_dir()),
        ..Default::default()
    };
    let sink = Arc::new(|_: SessionEvent| {});
    Session::spawn(o, GridSize::new(80, 24), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"))
}

fn wait_for(s: &Session, word: &str) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !s.snapshot().text().contains(word) {
        assert!(Instant::now() < deadline, "{word}가 보이지 않는다");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn file_transmission_from_the_shell_draws_the_image() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().canonicalize().unwrap().join("pic.png");
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, 2, 1);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().unwrap();
    w.write_image_data(&[255, 0, 0, 255, 0, 0, 255, 255]).unwrap();
    w.finish().unwrap();
    std::fs::write(&path, out).unwrap();

    let apc = format!(
        "\x1b_Ga=T,t=f,f=100,i=7,q=2;{}\x1b\\",
        b64(path.as_os_str().as_encoded_bytes())
    );
    let s = run("printf '%s' \"$1\"; printf 'SENT\\n'; sleep 1", apc);
    wait_for(&s, "SENT");
    assert_eq!(s.snapshot().images.len(), 1, "t=f 그림이 생기지 않았다");
}

#[test]
fn refused_file_transmissions_draw_nothing() {
    // 폴더, /dev/zero, 임시 파일(t=t)은 그림을 만들지 않는다
    let dir = tempfile::tempdir().unwrap();
    let cmds = [
        format!(
            "a=T,t=f,f=100,i=1,q=2;{}",
            b64(dir.path().as_os_str().as_encoded_bytes())
        ),
        format!("a=T,t=f,f=100,i=2,q=2;{}", b64(b"/dev/zero")),
        format!("a=T,t=t,f=100,i=3,q=2;{}", b64(b"/etc/hosts")),
    ];
    let apc: String = cmds.iter().map(|c| format!("\x1b_G{c}\x1b\\")).collect();
    let s = run("printf '%s' \"$1\"; printf 'SENT\\n'; sleep 1", apc);
    wait_for(&s, "SENT");
    assert!(s.snapshot().images.is_empty());
}
