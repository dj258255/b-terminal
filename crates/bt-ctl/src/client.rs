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

/// 이벤트 구독(`watch`). 첫 답이 오류면 그 오류를 돌려주고, 아니면 이벤트 줄마다 `on_line`을 부른다. `on_line`이 거짓을 돌려주거나
/// 앱이 연결을 닫으면 끝난다. 줄 하나의 길이에는 상한(`MAX_LINE`)을 건다
pub fn watch(socket: &std::path::Path, mut on_line: impl FnMut(&str) -> bool) -> Result<(), ClientError> {
    let mut stream =
        UnixStream::connect(socket).map_err(|e| ClientError::NoTerminal(format!("{}: {e}", socket.display())))?;
    let mut line = serde_json::to_vec(&Request {
        id: 1,
        call: Call::Watch,
    })
    .map_err(|e| ClientError::Remote(e.to_string()))?;
    line.push(b'\n');
    stream.write_all(&line).map_err(ClientError::Io)?;
    let mut reader = BufReader::new(stream);
    let mut first = true;
    loop {
        let mut buf = Vec::new();
        let n = std::io::Read::take(&mut reader, MAX_LINE as u64 + 1)
            .read_until(b'\n', &mut buf)
            .map_err(ClientError::Io)?;
        if n == 0 {
            return Ok(());
        }
        if buf.len() > MAX_LINE {
            return Err(ClientError::Remote("이벤트 줄이 너무 깁니다".into()));
        }
        let text = String::from_utf8_lossy(&buf);
        let text = text.trim_end_matches(['\n', '\r']);
        if first {
            first = false;
            let resp: Response =
                serde_json::from_str(text).map_err(|e| ClientError::Remote(format!("답을 읽지 못했습니다: {e}")))?;
            if !resp.ok {
                return Err(ClientError::Remote(
                    resp.error.unwrap_or_else(|| "알 수 없는 오류".into()),
                ));
            }
            continue;
        }
        if !on_line(text) {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod watch_tests {
    use super::*;
    use std::io::Read;
    use std::os::unix::net::UnixListener;

    fn serve_once(name: &str, reply: &'static str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("bt-watch-{}-{name}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let l = UnixListener::bind(&path).unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut req = [0u8; 64];
            let n = s.read(&mut req).unwrap();
            assert!(std::str::from_utf8(&req[..n]).unwrap().contains(r#""method":"watch""#));
            s.write_all(reply.as_bytes()).unwrap();
        });
        path
    }

    /// 첫 답을 확인한 뒤 이벤트 줄을 하나씩 넘기고, 앱이 닫으면 끝난다(PTY 없이 소켓만 쓴다)
    #[test]
    fn watch_passes_event_lines_after_the_first_reply() {
        let p = serve_once(
            "ok",
            "{\"id\":1,\"ok\":true,\"result\":{\"watching\":true}}\n{\"event\":\"ping\"}\n{\"event\":\"tab_closed\",\"tab\":2}\n",
        );
        let mut got = Vec::new();
        watch(&p, |l| {
            got.push(l.to_string());
            true
        })
        .unwrap();
        assert_eq!(got, vec![r#"{"event":"ping"}"#, r#"{"event":"tab_closed","tab":2}"#]);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn watch_reports_a_refusal_and_stops_when_the_callback_says_so() {
        let p = serve_once("full", "{\"id\":1,\"ok\":false,\"error\":\"너무 많습니다\"}\n");
        let e = watch(&p, |_| true).unwrap_err();
        assert!(e.to_string().contains("너무 많"), "{e}");
        let _ = std::fs::remove_file(&p);
        let p = serve_once(
            "stop",
            "{\"id\":1,\"ok\":true}\n{\"event\":\"ping\"}\n{\"event\":\"ping\"}\n",
        );
        let mut n = 0;
        watch(&p, |_| {
            n += 1;
            false
        })
        .unwrap();
        assert_eq!(n, 1);
        let _ = std::fs::remove_file(&p);
    }
}
