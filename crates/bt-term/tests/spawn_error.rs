//! 띄우기가 실패하면 오류 사슬만 보고도 어느 단계에서 무엇을 띄우려다 실패했는지 알 수 있다(#365).
//! PTY는 `/bin/sh`만 쓴다.

use std::sync::Arc;

use bt_core::Theme;
use bt_term::{GridSize, Session, SessionEvent, SpawnOptions};

fn try_spawn(program: &str, cwd: &str) -> String {
    let sink = Arc::new(|_: SessionEvent| {});
    let o = SpawnOptions {
        program: Some(program.into()),
        args: vec!["-c".into(), "exit 0".into()],
        cwd: Some(cwd.into()),
        ..Default::default()
    };
    match Session::spawn(o, GridSize::new(80, 24), Theme::default(), sink) {
        Ok(_) => panic!("띄우기가 성공하면 안 된다: {program} {cwd}"),
        Err(e) => format!("{e:#}"),
    }
}

#[test]
fn spawn_error_names_stage_program_and_cwd() {
    let msg = try_spawn("/nonexistent-bt-prog-365", "/");
    assert!(msg.contains("프로그램 띄우기"), "단계가 오류에 없다: {msg}");
    assert!(
        msg.contains("/nonexistent-bt-prog-365"),
        "프로그램이 오류에 없다: {msg}"
    );
    assert!(msg.contains("작업 폴더 /"), "작업 폴더가 오류에 없다: {msg}");
}

#[test]
fn successful_spawn_still_works() {
    let sink = Arc::new(|_: SessionEvent| {});
    let o = SpawnOptions {
        program: Some("/bin/sh".into()),
        args: vec!["-c".into(), "exit 0".into()],
        cwd: Some("/".into()),
        ..Default::default()
    };
    Session::spawn(o, GridSize::new(80, 24), Theme::default(), sink).unwrap_or_else(|e| panic!("spawn: {e:#}"));
}
