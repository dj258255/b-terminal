//! 칸에 쓴 입력과 그 뒤 PTY 출력이 입력 → 화면 지연 표에 적힌다(#417). PTY는 `/bin/sh`와 `printf`만 쓴다.

use std::sync::Arc;
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

fn spawn(script: &str) -> Session {
    let sink = Arc::new(|_: SessionEvent| {});
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into()],
        cwd: Some("/".into()),
        ..Default::default()
    };
    Session::spawn(o, GridSize::new(80, 24), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"))
}

/// 프레임을 떠 보며 값이 생길 때까지 기다린다(읽기 스레드가 출력을 적을 시간)
fn wait_sample(s: &Session) -> Option<bt_term::latency::Sample> {
    let end = Instant::now() + Duration::from_secs(5);
    while Instant::now() < end {
        if let Some(x) = s.latency_begin_frame(Instant::now()) {
            return Some(x);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    None
}

#[test]
fn typed_input_then_output_is_measured() {
    bt_term::latency::set_enabled(true);
    let s = spawn("read x; printf done");
    s.write_input(b"a\n", false);
    let sample = wait_sample(&s).expect("입력 뒤 출력이 적혀야 한다");
    let r = s.latency_presented(&sample, Instant::now()).expect("올라감");
    assert!(r.total_ms >= r.to_output_ms && r.to_output_ms >= 0.0, "{r:?}");
    assert!(r.total_ms < 1000.0, "{r:?}");
    // 지웠으니 같은 입력을 또 재지 않는다
    assert!(s.latency_begin_frame(Instant::now()).is_none());
}

#[test]
fn explicit_note_input_covers_control_socket_writes() {
    bt_term::latency::set_enabled(true);
    let s = spawn("read x; printf done");
    s.note_input();
    s.write(b"a\n");
    assert!(wait_sample(&s).is_some());
}

#[test]
fn output_without_input_is_not_measured() {
    bt_term::latency::set_enabled(true);
    let s = spawn("printf hello; sleep 1");
    std::thread::sleep(Duration::from_millis(300));
    assert!(s.latency_begin_frame(Instant::now()).is_none());
}
