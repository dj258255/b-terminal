//! 제어 소켓 손님

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use bt_core::proto::{
    Call, ENV_SOCKET, ENV_TAB, MAX_LINE, Request, Response, check_own_socket, check_private_dir, socket_dir,
};

#[derive(Debug)]
pub enum ClientError {
    /// 붙을 터미널이 없다(훅은 이때 조용히 끝난다)
    NoTerminal(String),
    Io(std::io::Error),
    Remote(String),
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::NoTerminal(why) => write!(f, "b-terminal을 찾지 못했습니다: {why}"),
            ClientError::Io(e) => write!(f, "소켓 오류: {e}"),
            ClientError::Remote(e) => write!(f, "{e}"),
        }
    }
}

/// 붙을 소켓을 찾는다: `BT_SOCKET` → 소켓 폴더에서 붙어지는 것이 하나뿐이면 그것.
/// 어느 쪽이든 폴더가 이 사용자 전용이고 소켓 주인이 이 사용자일 때만 붙는다(남이 만든 가짜 소켓에 상태·글을 보내지 않게)
pub fn find_socket() -> Result<PathBuf, ClientError> {
    let uid = rustix::process::getuid().as_raw();
    let verify = |p: &std::path::Path| -> Result<(), ClientError> {
        let dir = p
            .parent()
            .ok_or_else(|| ClientError::NoTerminal(format!("{}: 폴더가 없습니다", p.display())))?;
        check_private_dir(dir, uid).map_err(|e| ClientError::NoTerminal(e.to_string()))?;
        check_own_socket(p, uid).map_err(|e| ClientError::NoTerminal(e.to_string()))
    };
    if let Some(p) = std::env::var_os(ENV_SOCKET).map(PathBuf::from) {
        verify(&p)?;
        return Ok(p);
    }
    let dir = socket_dir(uid);
    check_private_dir(&dir, uid).map_err(|e| ClientError::NoTerminal(e.to_string()))?;
    let entries =
        std::fs::read_dir(&dir).map_err(|_| ClientError::NoTerminal(format!("{}가 없습니다", dir.display())))?;
    let live: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "sock"))
        .filter(|p| check_own_socket(p, uid).is_ok())
        .filter(|p| UnixStream::connect(p).is_ok())
        .collect();
    match live.len() {
        0 => Err(ClientError::NoTerminal("떠 있는 터미널이 없습니다".into())),
        1 => Ok(live.into_iter().next().unwrap_or_default()),
        n => Err(ClientError::NoTerminal(format!(
            "터미널이 {n}개 떠 있습니다. {ENV_SOCKET}로 고르세요"
        ))),
    }
}

/// 이 프로세스가 도는 탭(탭 안에서 불렸을 때)
pub fn own_tab() -> Option<u64> {
    std::env::var(ENV_TAB).ok()?.parse().ok()
}

/// 요청 하나를 보내고 답을 받는다
pub fn call(socket: &std::path::Path, call: Call) -> Result<serde_json::Value, ClientError> {
    let mut stream =
        UnixStream::connect(socket).map_err(|e| ClientError::NoTerminal(format!("{}: {e}", socket.display())))?;
    // run은 명령이 끝날 때까지 기다린다
    let wait = call.reply_timeout() + Duration::from_secs(2);
    stream
        .set_read_timeout(Some(wait.max(Duration::from_secs(10))))
        .map_err(ClientError::Io)?;
    let mut line = serde_json::to_vec(&Request { id: 1, call }).map_err(|e| ClientError::Remote(e.to_string()))?;
    line.push(b'\n');
    stream.write_all(&line).map_err(ClientError::Io)?;
    let mut reader = BufReader::new(stream);
    let mut buf = Vec::new();
    std::io::Read::take(&mut reader, MAX_LINE as u64 * 4)
        .read_until(b'\n', &mut buf)
        .map_err(ClientError::Io)?;
    let resp: Response =
        serde_json::from_slice(&buf).map_err(|e| ClientError::Remote(format!("답을 읽지 못했습니다: {e}")))?;
    if resp.ok {
        Ok(resp.result)
    } else {
        Err(ClientError::Remote(
            resp.error.unwrap_or_else(|| "알 수 없는 오류".into()),
        ))
    }
}
