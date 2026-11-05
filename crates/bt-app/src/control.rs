//! 제어 소켓 서버. 줄 단위 JSON 요청을 받아 창 스레드에 넘기고 응답을 돌려준다(bt_core::proto)

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, MetadataExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::time::Duration;

use bt_core::proto::{MAX_LINE, Request, Response, socket_dir, socket_path};
use winit::event_loop::EventLoopProxy;

use crate::app::UserEvent;

/// 창 스레드가 답할 때까지 기다리는 상한
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

/// 소켓 파일. 떨어뜨리면 지운다
pub struct ControlServer {
    pub path: PathBuf,
}

impl Drop for ControlServer {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn uid() -> u32 {
    rustix::process::getuid().as_raw()
}

/// 소켓 폴더를 만든다. 이미 있으면 링크가 아니고, 내 것이고, 다른 사람의 권한이 없을 때만 쓴다.
/// 남이 먼저 만든 폴더(예: /tmp)에 소켓을 두면 그 사람이 소켓을 바꿔치기할 수 있다
pub fn prepare_dir(dir: &Path, uid: u32) -> std::io::Result<()> {
    match std::fs::symlink_metadata(dir) {
        Ok(meta) => {
            let bad = |why: &str| {
                std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("{}: {why}", dir.display()),
                )
            };
            if !meta.file_type().is_dir() {
                return Err(bad("폴더가 아니다(링크일 수 있다)"));
            }
            if meta.uid() != uid {
                return Err(bad("다른 사용자의 폴더다"));
            }
            if meta.mode() & 0o077 != 0 {
                return Err(bad("다른 사용자에게 권한이 열려 있다"));
            }
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => std::fs::DirBuilder::new().mode(0o700).create(dir),
        Err(e) => Err(e),
    }
}

/// 소켓을 열고 받기 스레드를 띄운다
pub fn start(proxy: EventLoopProxy<UserEvent>) -> std::io::Result<ControlServer> {
    let dir = socket_dir(uid());
    prepare_dir(&dir, uid())?;
    let path = socket_path(&dir, std::process::id());
    // 같은 pid의 지난 소켓(앱이 죽은 뒤 pid가 다시 쓰인 경우)만 지운다
    if let Ok(meta) = std::fs::symlink_metadata(&path) {
        if meta.file_type().is_socket() {
            std::fs::remove_file(&path)?;
        }
    }
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    std::thread::Builder::new().name("bt-control".into()).spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let proxy = proxy.clone();
            let _ = std::thread::Builder::new()
                .name("bt-control-conn".into())
                .spawn(move || serve(stream, proxy));
        }
    })?;
    Ok(ControlServer { path })
}

fn serve(stream: UnixStream, proxy: EventLoopProxy<UserEvent>) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let Ok(mut writer) = stream.try_clone() else { return };
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = Vec::new();
        // 상한은 읽기 전에 건다: 한 바이트 더 읽혔으면 넘은 것이다
        match (&mut reader).take(MAX_LINE as u64 + 1).read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if line.len() > MAX_LINE {
            let _ = reply(&mut writer, &Response::err(0, "요청이 너무 깁니다"));
            return;
        }
        let response = match serde_json::from_slice::<Request>(&line) {
            Ok(req) => dispatch(req, &proxy),
            Err(e) => Response::err(0, format!("요청을 읽지 못했습니다: {e}")),
        };
        if reply(&mut writer, &response).is_err() {
            return;
        }
    }
}

fn dispatch(req: Request, proxy: &EventLoopProxy<UserEvent>) -> Response {
    let id = req.id;
    let (tx, rx): (SyncSender<Response>, _) = sync_channel(1);
    if proxy.send_event(UserEvent::Control(req, tx)).is_err() {
        return Response::err(id, "터미널이 끝나는 중입니다");
    }
    rx.recv_timeout(REPLY_TIMEOUT)
        .unwrap_or_else(|_| Response::err(id, "터미널이 답하지 않습니다"))
}

fn reply(w: &mut UnixStream, r: &Response) -> std::io::Result<()> {
    let mut line = serde_json::to_vec(r).unwrap_or_default();
    line.push(b'\n');
    w.write_all(&line)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("bt-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        let _ = std::fs::remove_file(&p);
        p
    }

    #[test]
    fn creates_private_dir_and_rejects_open_or_linked_ones() {
        let me = uid();
        let d = tmp("dir");
        prepare_dir(&d, me).unwrap();
        assert_eq!(std::fs::metadata(&d).unwrap().mode() & 0o777, 0o700);
        // 다시 불러도 된다
        prepare_dir(&d, me).unwrap();

        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(prepare_dir(&d, me).is_err(), "다른 사람에게 열린 폴더는 쓰지 않는다");
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(0o700)).unwrap();

        assert!(prepare_dir(&d, me + 1).is_err(), "주인이 다르면 쓰지 않는다");

        let link = tmp("link");
        std::os::unix::fs::symlink(&d, &link).unwrap();
        assert!(prepare_dir(&link, me).is_err(), "링크는 따라가지 않는다");
        std::fs::remove_file(&link).unwrap();
        std::fs::remove_dir_all(&d).unwrap();
    }
}
