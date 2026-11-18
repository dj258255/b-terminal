//! 여러 스레드가 동시에 탭을 띄워도 openpty 일시 실패(macOS의 `Os { code: -6 }`)로 띄우기가 실패하지 않는다(#607).
//! PTY는 `/bin/sh`만 쓴다.

use std::sync::Arc;

use bt_core::Theme;
use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

/// macOS(Darwin 25.3)에서 스레드 32개가 동시에 `/dev/ptmx`를 열면 약 0.1%가 일시 실패한다.
/// 고치기 전에는 6400번 띄우는 동안 몇 번 실패한다(한 번 잰 값: 아래 troubleshooting 22번)
#[test]
fn concurrent_spawns_do_not_fail_on_transient_openpty_errors() {
    const THREADS: usize = 32;
    const PER_THREAD: usize = 200;
    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            std::thread::spawn(|| {
                let mut errors = Vec::new();
                for _ in 0..PER_THREAD {
                    let sink = Arc::new(|_: SessionEvent| {});
                    let o = SpawnOptions {
                        program: Some("/bin/sh".into()),
                        args: vec!["-c".into(), "exit 0".into()],
                        cwd: Some("/".into()),
                        ..Default::default()
                    };
                    if let Err(e) = Session::spawn(o, GridSize::new(80, 24), Theme::default(), sink) {
                        errors.push(format!("{e:#}"));
                    }
                }
                errors
            })
        })
        .collect();
    let errors: Vec<String> = handles.into_iter().flat_map(|h| h.join().unwrap()).collect();
    assert!(
        errors.is_empty(),
        "{}번 띄우는 동안 {}번 실패했다. 첫 오류: {}",
        THREADS * PER_THREAD,
        errors.len(),
        errors[0]
    );
}
