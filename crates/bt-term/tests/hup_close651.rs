//! 칸(Session)을 떨어뜨리면 셸이 SIGHUP을 받고 끝나며 PTY 주 쪽 fd가 풀린다(#651).
//! PTY는 `/bin/sh`와 셸 내장만 쓴다.

use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use bt_core::Theme;
use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

fn spawn_sh(script: &str) -> (Session, mpsc::Receiver<SessionEvent>) {
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    let sink = Arc::new(move |e: SessionEvent| {
        let _ = tx.lock().unwrap().send(e);
    });
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), script.into()],
        cwd: Some("/".into()),
        ..Default::default()
    };
    (
        Session::spawn(o, GridSize::new(80, 24), Theme::default(), sink).unwrap(),
        rx,
    )
}

fn alive(pid: u32) -> bool {
    std::process::Command::new("/bin/sh")
        .args(["-c", &format!("kill -0 {pid}")])
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap()
        .success()
}

fn gone_within(pid: u32, limit: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    !alive(pid)
}

fn wait_exited(rx: &mpsc::Receiver<SessionEvent>, limit: Duration) -> bool {
    let start = Instant::now();
    while let Some(left) = limit.checked_sub(start.elapsed()) {
        match rx.recv_timeout(left) {
            Ok(SessionEvent::Exited(_)) => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
    false
}

#[test]
fn dropping_session_ends_shell_and_read_thread() {
    let (session, rx) = spawn_sh("read x");
    let pid = session.pid().expect("pid");
    std::thread::sleep(Duration::from_millis(200));
    assert!(alive(pid), "떨어뜨리기 전에는 셸이 살아 있다");
    drop(session);
    assert!(
        gone_within(pid, Duration::from_secs(5)),
        "Session을 떨어뜨린 뒤에도 셸 {pid}이 남았다"
    );
    assert!(
        wait_exited(&rx, Duration::from_secs(5)),
        "셸이 끝났는데 읽기 스레드가 끝나지(fd가 풀리지) 않았다"
    );
}

#[test]
fn held_session_keeps_shell_alive() {
    // 되살리기 시간 동안 들고 있는 칸은 셸이 살아 있어야 한다
    let (session, _rx) = spawn_sh("read x");
    let pid = session.pid().expect("pid");
    std::thread::sleep(Duration::from_millis(500));
    assert!(alive(pid));
    drop(session);
    assert!(gone_within(pid, Duration::from_secs(5)));
}

#[test]
fn dropping_after_exit_is_harmless() {
    let (session, rx) = spawn_sh("exit 0");
    assert!(wait_exited(&rx, Duration::from_secs(5)));
    assert!(session.exit_status().is_some());
    drop(session);
}
