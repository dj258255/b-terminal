//! 셸 통합을 탭의 셸에 싣는다(#43). 스크립트는 실행 파일에 묶여 있다가 사용자 전용 폴더에 풀린다

use std::path::{Path, PathBuf};

use bt_core::config::ShellFeatures;
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

/// b-terminal 실행 파일이 있는 폴더. `btctl`이 함께 있고, PATH 구분자(`:`)나 제어 문자가 없는
/// 절대 경로일 때만 쓴다(`path` 기능이 PATH에 더할 폴더)
pub fn bin_dir_of(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    let text = dir.to_str()?;
    if !dir.is_absolute() || text.chars().any(|c| c == ':' || c.is_control()) {
        return None;
    }
    dir.join("btctl").is_file().then(|| dir.to_path_buf())
}

/// 기본 셸을 띄우는 탭에 셸 통합을 싣는다. `mode`는 설정 shell-integration(detect·none·zsh·bash)
pub fn apply(opts: &mut SpawnOptions, dir: &Path, mode: &str, features: &ShellFeatures, bin_dir: Option<&Path>) {
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
    // 스크립트는 이 목록의 이름과 정확히 맞는 것만 켠다(eval하지 않는다). 비어 있어도 싣는다
    opts.env.push(("BT_SHELL_FEATURES".into(), features.to_list()));
    if let Some(b) = bin_dir {
        opts.env.push(("BT_BIN_DIR".into(), b.to_string_lossy().into_owned()));
    }
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
        apply(&mut o, Path::new("/x"), "detect", &ShellFeatures::default(), None);
        assert!(o.env.is_empty());
        let mut o = SpawnOptions::default();
        apply(&mut o, Path::new("/x"), "none", &ShellFeatures::default(), None);
        assert!(o.env.is_empty());
        let mut o = SpawnOptions::default();
        apply(&mut o, Path::new("/x"), "bash", &ShellFeatures::default(), None);
        assert_eq!(o.args[0], "--rcfile");
        assert!(o.env.iter().any(|(k, _)| k == "BT_BASH_LOGIN"));
    }

    fn env_of<'a>(o: &'a SpawnOptions, key: &str) -> Option<&'a str> {
        o.env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }

    #[test]
    fn features_and_bin_dir_go_to_the_shell_as_env() {
        let f = ShellFeatures {
            cursor: false,
            ..ShellFeatures::default()
        };
        let mut o = SpawnOptions::default();
        apply(&mut o, Path::new("/x"), "zsh", &f, Some(Path::new("/opt/bt")));
        assert_eq!(env_of(&o, "BT_SHELL_FEATURES"), Some("title,path"));
        assert_eq!(env_of(&o, "BT_BIN_DIR"), Some("/opt/bt"));
        // 폴더를 못 찾았으면 BT_BIN_DIR은 싣지 않는다
        let mut o = SpawnOptions::default();
        apply(&mut o, Path::new("/x"), "zsh", &f, None);
        assert_eq!(env_of(&o, "BT_BIN_DIR"), None);
        // 모두 꺼도 변수는 싣는다(비어 있음): 셸이 기본값으로 켜지 않게
        let mut o = SpawnOptions::default();
        let mut none = ShellFeatures::default();
        none.apply("false");
        apply(&mut o, Path::new("/x"), "zsh", &none, None);
        assert_eq!(env_of(&o, "BT_SHELL_FEATURES"), Some(""));
    }

    #[test]
    fn bin_dir_needs_btctl_and_a_plain_path() {
        let base = std::env::temp_dir().join(format!("bt-bindir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let exe = base.join("b-terminal");
        assert_eq!(bin_dir_of(&exe), None, "btctl이 없으면 더하지 않는다");
        std::fs::write(base.join("btctl"), "").unwrap();
        assert_eq!(bin_dir_of(&exe).as_deref(), Some(base.as_path()));
        // PATH 구분자(:)가 든 폴더는 PATH에 더하지 않는다
        let odd = base.join("a:b");
        std::fs::create_dir_all(&odd).unwrap();
        std::fs::write(odd.join("btctl"), "").unwrap();
        assert_eq!(bin_dir_of(&odd.join("b-terminal")), None);
        let _ = std::fs::remove_dir_all(&base);
    }

    // ---- 스크립트 테스트: zsh -f / bash --norc로 스크립트를 읽어 나온 바이트를 본다 ----
    // 사용자의 .zshrc·.bashrc를 읽지 않고(환경도 비운다), 그 셸이 없으면 건너뛴다

    fn have(sh: &str) -> bool {
        std::process::Command::new(sh)
            .arg("-c")
            .arg("exit 0")
            .env("PATH", "/usr/bin:/bin")
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// 스크립트를 읽고 `body`를 돌린 표준 출력
    fn run_script(shell: Shell, features: Option<&str>, bin_dir: Option<&str>, body: &str) -> String {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/shell-integration");
        let (prog, args, script): (&str, &[&str], &str) = match shell {
            Shell::Zsh => ("zsh", &["-f", "-c"], "zsh/b-terminal.zsh"),
            Shell::Bash => ("bash", &["--norc", "--noprofile", "-c"], "bash/b-terminal.bash"),
        };
        let code = format!(". '{}'\n{body}", root.join(script).canonicalize().unwrap().display());
        let mut cmd = std::process::Command::new(prog);
        cmd.args(args)
            .arg(code)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/nonexistent");
        if let Some(f) = features {
            cmd.env("BT_SHELL_FEATURES", f);
        }
        if let Some(b) = bin_dir {
            cmd.env("BT_BIN_DIR", b);
        }
        let out = cmd.output().unwrap();
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    // zsh는 훅 함수를 직접 부르고, bash는 DEBUG 덫이 명령 앞에서 preexec를 부르게 `: 글`을 실행한다
    fn prompt_then_command(shell: Shell) -> &'static str {
        match shell {
            Shell::Zsh => "_bt_precmd; _bt_preexec 'ls -l'",
            Shell::Bash => "_bt_precmd; : ls -l",
        }
    }

    fn each_shell(f: impl Fn(Shell)) {
        for (sh, name) in [(Shell::Zsh, "zsh"), (Shell::Bash, "bash")] {
            if have(name) {
                f(sh);
            } else {
                eprintln!("{name}이 없어 건너뜁니다");
            }
        }
    }

    #[test]
    fn cursor_feature_bar_at_prompt_and_reset_while_running() {
        each_shell(|sh| {
            let out = run_script(sh, Some("cursor"), None, prompt_then_command(sh));
            let at_prompt = out
                .find("\x1b[5 q")
                .unwrap_or_else(|| panic!("{sh:?} 막대 없음: {out:?}"));
            let running = out
                .find("\x1b[0 q")
                .unwrap_or_else(|| panic!("{sh:?} 되돌림 없음: {out:?}"));
            assert!(
                at_prompt < running,
                "{sh:?}: 프롬프트에서 막대, 그 뒤 명령이 돌 때 원래대로: {out:?}"
            );
            for off in [Some("title,path"), Some(""), None] {
                let out = run_script(sh, off, None, prompt_then_command(sh));
                assert!(
                    !out.contains(" q"),
                    "{sh:?} {off:?}: 커서 기능이 꺼졌는데 DECSCUSR가 나옴: {out:?}"
                );
            }
        });
    }

    #[test]
    fn title_feature_sets_dir_at_prompt_and_command_while_running() {
        each_shell(|sh| {
            let out = run_script(sh, Some("title"), None, prompt_then_command(sh));
            assert!(out.contains("\x1b]2;"), "{sh:?}: {out:?}");
            assert!(
                out.contains("\x1b]2;ls -l\x07") || out.contains("\x1b]2;: ls -l\x07"),
                "{sh:?}: {out:?}"
            );
            for off in [Some("cursor,path"), Some(""), None] {
                let out = run_script(sh, off, None, prompt_then_command(sh));
                assert!(
                    !out.contains("\x1b]2;"),
                    "{sh:?} {off:?}: 제목 기능이 꺼졌는데 제목이 나옴: {out:?}"
                );
            }
            // 명령 글에 든 제어 문자는 공백으로: 제목 OSC를 끊고 다른 OSC를 끼우지 못한다
            let body = match sh {
                Shell::Zsh => "_bt_precmd; _bt_preexec $'x\\e]0;evil\\a\\ey'",
                Shell::Bash => "_bt_precmd; : $'x\\e]0;evil\\ay'",
            };
            let out = run_script(sh, Some("title"), None, body);
            assert!(!out.contains("\x1b]0;"), "{sh:?}: {out:?}");
            assert!(!out.contains("\x1by"), "{sh:?}: {out:?}");
        });
    }

    #[test]
    fn path_feature_appends_bin_dir_once() {
        each_shell(|sh| {
            let show = "printf '\\nPATH=%s\\n' \"$PATH\"";
            let out = run_script(
                sh,
                Some("path"),
                Some("/opt/bt/bin"),
                &format!("_bt_precmd; _bt_precmd; {show}"),
            );
            assert!(out.contains("\nPATH=/usr/bin:/bin:/opt/bt/bin\n"), "{sh:?}: {out:?}");
            let out = run_script(
                sh,
                Some("cursor,title"),
                Some("/opt/bt/bin"),
                &format!("_bt_precmd; {show}"),
            );
            assert!(out.contains("\nPATH=/usr/bin:/bin\n"), "{sh:?}: 꺼졌는데 더함: {out:?}");
            // 이미 있으면 다시 더하지 않는다
            let body = format!("PATH=/opt/bt/bin:$PATH; _bt_precmd; {show}");
            let out = run_script(sh, Some("path"), Some("/opt/bt/bin"), &body);
            assert!(out.contains("\nPATH=/opt/bt/bin:/usr/bin:/bin\n"), "{sh:?}: {out:?}");
            // 폴더 값에 PATH 구분자가 있으면 더하지 않는다
            let out = run_script(sh, Some("path"), Some("/a:/b"), &format!("_bt_precmd; {show}"));
            assert!(out.contains("\nPATH=/usr/bin:/bin\n"), "{sh:?}: {out:?}");
        });
    }

    #[test]
    fn feature_list_is_never_evaluated() {
        each_shell(|sh| {
            let marker = std::env::temp_dir().join(format!("bt-eval-{}-{sh:?}", std::process::id()));
            let _ = std::fs::remove_file(&marker);
            let m = marker.display();
            let evil = format!("cursor;touch {m};title$(touch {m})`touch {m}`");
            let out = run_script(sh, Some(&evil), None, prompt_then_command(sh));
            assert!(!marker.exists(), "{sh:?}: 기능 목록이 실행됨");
            assert!(
                !out.contains(" q") && !out.contains("\x1b]2;"),
                "{sh:?}: 정확히 맞는 이름만 켠다: {out:?}"
            );
            let _ = std::fs::remove_file(&marker);
        });
    }

    #[test]
    fn command_marks_are_still_sent_without_features() {
        each_shell(|sh| {
            let out = run_script(sh, Some(""), None, prompt_then_command(sh));
            // bash는 줄 편집기가 마지막 줄만 다시 그리므로 redraw=last를 알린다(Ghostty ghostty.bash 188줄, #329). zsh는 기본값(1)
            let a = if matches!(sh, Shell::Bash) {
                "\x1b]133;A;redraw=last\x07"
            } else {
                "\x1b]133;A\x07"
            };
            assert!(out.contains(a), "{sh:?}: {out:?}");
            assert!(out.contains("\x1b]133;C\x07"), "{sh:?}: {out:?}");
            assert!(out.contains("\x1b]633;E;"), "{sh:?}: {out:?}");
        });
    }
}
