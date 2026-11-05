//! PTY의 전경 프로세스(지금 키 입력을 받는 프로세스)를 알아본다. 사이드바의 이름과 에이전트 알아보기에 쓴다

use std::path::PathBuf;

/// 전경 프로세스
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Foreground {
    pub pid: i32,
    /// 실행 파일 이름. node 스크립트면 스크립트 이름(예: vite)
    pub name: String,
}

/// 프로세스 이름. node·bun처럼 스크립트를 돌리는 실행 파일이면 명령줄에서 스크립트 이름을 꺼낸다
pub fn process_name(pid: i32) -> Option<String> {
    let name = raw_name(pid)?;
    if matches!(name.as_str(), "node" | "bun" | "deno" | "python" | "python3") {
        if let Some(script) = script_name(pid) {
            return Some(script);
        }
    }
    Some(name)
}

#[cfg(target_os = "macos")]
fn raw_name(pid: i32) -> Option<String> {
    libproc::proc_pid::name(pid).ok().filter(|s| !s.is_empty())
}

#[cfg(target_os = "linux")]
fn raw_name(pid: i32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn raw_name(_pid: i32) -> Option<String> {
    None
}

/// 명령줄 두 번째 낱말의 파일 이름(`node /…/bin/vite --port 3000` → `vite`)
fn script_name(pid: i32) -> Option<String> {
    let args = command_line(pid)?;
    let script = args.split_whitespace().nth(1)?;
    let base = script.rsplit('/').next()?;
    let base = base
        .strip_suffix(".js")
        .or_else(|| base.strip_suffix(".mjs"))
        .unwrap_or(base);
    (!base.is_empty() && !base.starts_with('-')).then(|| base.to_string())
}

#[cfg(target_os = "linux")]
fn command_line(pid: i32) -> Option<String> {
    let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    Some(
        raw.split(|&b| b == 0)
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect::<Vec<_>>()
            .join(" "),
    )
}

#[cfg(not(target_os = "linux"))]
fn command_line(pid: i32) -> Option<String> {
    let out = std::process::Command::new("ps")
        .args(["-o", "args=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// 프로세스의 작업 폴더. macOS는 libproc가 주지 않아 lsof를 부른다(새 탭이 폴더를 물려받을 때만 쓴다)
pub fn process_cwd(pid: i32) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let out = std::process::Command::new("lsof")
            .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
            .output()
            .ok()?;
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .find_map(|l| l.strip_prefix('n').map(PathBuf::from))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_process_has_a_name_and_cwd() {
        let pid = std::process::id() as i32;
        assert!(process_name(pid).is_some());
        let cwd = process_cwd(pid).expect("cwd");
        assert_eq!(
            cwd.canonicalize().unwrap(),
            std::env::current_dir().unwrap().canonicalize().unwrap()
        );
    }
}
