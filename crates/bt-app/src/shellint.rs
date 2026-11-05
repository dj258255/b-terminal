//! 셸 통합을 탭의 셸에 싣는다(#43). 스크립트는 실행 파일에 묶여 있다가 사용자 전용 폴더에 풀린다

use std::path::{Path, PathBuf};

use bt_term::SpawnOptions;

const FILES: &[(&str, &str)] = &[
    (
        "zsh/.zshenv",
        include_str!("../../../assets/shell-integration/zsh/.zshenv"),
    ),
    (
        "zsh/b-terminal.zsh",
        include_str!("../../../assets/shell-integration/zsh/b-terminal.zsh"),
    ),
    ("bash/rc", include_str!("../../../assets/shell-integration/bash/rc")),
    (
        "bash/b-terminal.bash",
        include_str!("../../../assets/shell-integration/bash/b-terminal.bash"),
    ),
];

/// 셸 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Zsh,
    Bash,
}

impl Shell {
    pub fn detect(program: &str) -> Option<Self> {
        match Path::new(program).file_name()?.to_str()?.trim_start_matches('-') {
            "zsh" => Some(Shell::Zsh),
            "bash" => Some(Shell::Bash),
            _ => None,
        }
    }
}

/// 스크립트를 사용자 전용 폴더(제어 소켓과 같은 0700 폴더 아래)에 풀고 그 폴더를 돌려준다. 앱이 뜰 때마다 다시 쓴다
pub fn install() -> std::io::Result<PathBuf> {
    let uid = crate::control::uid();
    let base = bt_core::proto::socket_dir(uid);
    crate::control::prepare_dir(&base, uid)?;
    let dir = base.join("shell-integration").join(env!("CARGO_PKG_VERSION"));
    for (rel, text) in FILES {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, text)?;
    }
    Ok(dir)
}

/// 기본 셸을 띄우는 탭에 셸 통합을 싣는다. `mode`는 설정 shell-integration(detect·none·zsh·bash)
pub fn apply(opts: &mut SpawnOptions, dir: &Path, mode: &str) {
    if opts.program.is_some() || mode == "none" {
        return;
    }
    let shell_path = std::env::var("SHELL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/bin/zsh".into());
    let shell = match mode {
        "zsh" => Some(Shell::Zsh),
        "bash" => Some(Shell::Bash),
        _ => Shell::detect(&shell_path),
    };
    let Some(shell) = shell else { return };
    opts.env
        .push(("BT_SHELL_INTEGRATION_DIR".into(), dir.to_string_lossy().into_owned()));
    match shell {
        Shell::Zsh => {
            if let Ok(orig) = std::env::var("ZDOTDIR") {
                opts.env.push(("BT_ORIG_ZDOTDIR".into(), orig));
            }
            opts.env
                .push(("ZDOTDIR".into(), dir.join("zsh").to_string_lossy().into_owned()));
            // 셸은 그대로 로그인 셸로(SpawnOptions 기본)
        }
        Shell::Bash => {
            opts.program = Some(shell_path);
            opts.args = vec![
                "--rcfile".into(),
                dir.join("bash/rc").to_string_lossy().into_owned(),
                "-i".into(),
            ];
            opts.env.push(("BT_BASH_LOGIN".into(), "1".into()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_shells_by_name() {
        assert_eq!(Shell::detect("/bin/zsh"), Some(Shell::Zsh));
        assert_eq!(Shell::detect("-zsh"), Some(Shell::Zsh));
        assert_eq!(Shell::detect("/opt/homebrew/bin/bash"), Some(Shell::Bash));
        assert_eq!(Shell::detect("/usr/bin/fish"), None);
    }

    #[test]
    fn explicit_programs_are_left_alone() {
        let mut o = SpawnOptions {
            program: Some("htop".into()),
            ..Default::default()
        };
        apply(&mut o, Path::new("/x"), "detect");
        assert!(o.env.is_empty());
        let mut o = SpawnOptions::default();
        apply(&mut o, Path::new("/x"), "none");
        assert!(o.env.is_empty());
        let mut o = SpawnOptions::default();
        apply(&mut o, Path::new("/x"), "bash");
        assert_eq!(o.args[0], "--rcfile");
        assert!(o.env.iter().any(|(k, _)| k == "BT_BASH_LOGIN"));
    }
}
