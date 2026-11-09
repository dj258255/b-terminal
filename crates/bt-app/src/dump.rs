//! 화면·스크롤백·고른 글을 파일로 쓴다(Ghostty `write_*_file`, #125).
//! 화면 내용이 디스크에 남으므로 이 사용자 전용 폴더(0700, 제어 소켓과 같은 검사)에 0600으로 새로 만든다

use std::io;
use std::path::{Path, PathBuf};

/// 파일을 둘 폴더: 제어 소켓 폴더 아래 `files`. 둘 다 이 사용자 전용인지 보고, 없으면 0700으로 만든다
pub fn dir() -> io::Result<PathBuf> {
    let uid = crate::control::uid();
    let base = bt_core::proto::socket_dir(uid);
    crate::control::prepare_dir(&base, uid)?;
    let files = base.join("files");
    crate::control::prepare_dir(&files, uid)?;
    Ok(files)
}

/// `dir`에 `kind-pid-n.txt`로 `text`를 쓴다. 이미 있는 이름은 건너뛰고(링크를 따라가지 않게 새로 만들기만), 권한은 0600
pub fn write(dir: &Path, kind: &str, text: &str) -> io::Result<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    for n in 0..10_000 {
        let path = dir.join(format!("{kind}-{}-{n}.txt", std::process::id()));
        // create_new(O_EXCL)는 그 이름에 링크가 있어도 따라가지 않고 실패한다
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(mut f) => {
                f.write_all(text.as_bytes())?;
                return Ok(path);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("쓸 파일 이름이 남지 않았습니다"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    fn private_dir(name: &str, mode: u32) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bt-dump-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::DirBuilder::new().mode(mode).create(&d).unwrap();
        std::fs::set_permissions(&d, std::fs::Permissions::from_mode(mode)).unwrap();
        d
    }

    #[test]
    fn writes_private_files_with_fresh_names() {
        let d = private_dir("ok", 0o700);
        let a = write(&d, "screen", "한글 OK\nline 2\n").unwrap();
        let b = write(&d, "screen", "두 번째").unwrap();
        assert_ne!(a, b, "같은 이름을 다시 쓰지 않는다");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "한글 OK\nline 2\n");
        let mode = std::fs::metadata(&a).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "나만 읽는다");
        assert!(a.file_name().unwrap().to_string_lossy().starts_with("screen-"));
        assert_eq!(a.extension().unwrap(), "txt");
        // 같은 이름의 링크가 미리 있어도 따라가지 않고 다음 이름으로
        let next = d.join(format!("scrollback-{}-0.txt", std::process::id()));
        std::os::unix::fs::symlink("/etc/hosts", &next).unwrap();
        let c = write(&d, "scrollback", "x").unwrap();
        assert_ne!(c, next);
        assert!(
            !std::fs::read_to_string("/etc/hosts").is_ok_and(|s| s == "x"),
            "링크 대상에 쓰지 않는다"
        );
        std::fs::remove_dir_all(&d).unwrap();
    }
}
