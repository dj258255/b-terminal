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
    // fish·nushell·elvish는 XDG_DATA_DIRS 아래에서 찾는 이름 그대로 둔다(fish/vendor_conf.d, nushell/vendor/autoload, elvish/lib)
    (
        "fish/vendor_conf.d/b-terminal-shell-integration.fish",
        include_str!("../../../assets/shell-integration/fish/vendor_conf.d/b-terminal-shell-integration.fish"),
    ),
    (
        "nushell/vendor/autoload/b-terminal.nu",
        include_str!("../../../assets/shell-integration/nushell/vendor/autoload/b-terminal.nu"),
    ),
    (
        "elvish/lib/b-terminal-integration.elv",
        include_str!("../../../assets/shell-integration/elvish/lib/b-terminal-integration.elv"),
    ),
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
    Fish,
    Nushell,
    Elvish,
}

impl Shell {
    pub fn detect(program: &str) -> Option<Self> {
        match Path::new(program).file_name()?.to_str()?.trim_start_matches('-') {
            "zsh" => Some(Shell::Zsh),
            "bash" => Some(Shell::Bash),
            "fish" => Some(Shell::Fish),
            "nu" => Some(Shell::Nushell),
            "elvish" => Some(Shell::Elvish),
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

/// 셸이 `XDG_DATA_DIRS`에서 읽는 값: 우리 폴더를 맨 앞에 두고 원래 목록을 그대로 잇는다(Ghostty `setupXdgDataDirs`).
/// 변수가 없으면 XDG 규격의 기본값을 함께 둬 기본 폴더를 잃지 않게 한다. 절대 경로가 아니거나 `:`·제어 문자가 든 폴더는
/// 목록을 깨므로 쓰지 않는다(`None`)
pub fn xdg_data_dirs_with(existing: Option<&str>, dir: &Path) -> Option<String> {
    let text = dir.to_str()?;
    if !dir.is_absolute() || text.chars().any(|c| c == ':' || c.is_control()) {
        return None;
    }
    Some(match existing.unwrap_or("/usr/local/share:/usr/share") {
        "" => text.to_string(),
        cur => format!("{text}:{cur}"),
    })
}

/// 기본 셸을 띄우는 탭에 셸 통합을 싣는다. `mode`는 설정 shell-integration(detect·none·zsh·bash·fish·nushell·elvish)
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
        "fish" => Some(Shell::Fish),
        "nushell" => Some(Shell::Nushell),
        "elvish" => Some(Shell::Elvish),
        _ => Shell::detect(&shell_path),
    };
    let Some(shell) = shell else { return };
    // fish·nushell·elvish는 환경 변수 하나(XDG_DATA_DIRS)로만 싣는다. 폴더가 목록을 깨면 아무것도 싣지 않는다
    let xdg = match shell {
        Shell::Fish | Shell::Nushell | Shell::Elvish => {
            match xdg_data_dirs_with(std::env::var("XDG_DATA_DIRS").ok().as_deref(), dir) {
                Some(v) => Some(v),
                None => return,
            }
        }
        _ => None,
    };
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
        Shell::Fish | Shell::Nushell | Shell::Elvish => {
            // 셸이 시작할 때 XDG_DATA_DIRS 아래의 스크립트를 읽는다. 스크립트가 처음에 이 변수에서 우리 폴더를 빼 하위 프로세스에 새지 않게 한다
            opts.env.push((
                "BT_SHELL_INTEGRATION_XDG_DIR".into(),
                dir.to_string_lossy().into_owned(),
            ));
            opts.env.push(("XDG_DATA_DIRS".into(), xdg.unwrap_or_default()));
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
        assert_eq!(Shell::detect("/usr/bin/tcsh"), None);
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
        run_script_in(shell, features, bin_dir, None, body)
    }

    /// `run_script`를 `cwd` 폴더에서 돌린다
    fn run_script_in(
        shell: Shell,
        features: Option<&str>,
        bin_dir: Option<&str>,
        cwd: Option<&Path>,
        body: &str,
    ) -> String {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/shell-integration");
        let (prog, args, script): (&str, &[&str], &str) = match shell {
            Shell::Zsh => ("zsh", &["-f", "-c"], "zsh/b-terminal.zsh"),
            _ => ("bash", &["--norc", "--noprofile", "-c"], "bash/b-terminal.bash"),
        };
        let code = format!(". '{}'\n{body}", root.join(script).canonicalize().unwrap().display());
        let mut cmd = std::process::Command::new(prog);
        cmd.args(args)
            .arg(code)
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", "/nonexistent");
        if let Some(d) = cwd {
            cmd.current_dir(d);
        }
        if let Some(f) = features {
            cmd.env("BT_SHELL_FEATURES", f);
        }
        if let Some(b) = bin_dir {
            cmd.env("BT_BIN_DIR", b);
        }
        let out = cmd.output().unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
        // 셸이 실패하거나 아무것도 찍지 않으면 원인(종료 상태·stderr)을 남긴다. 전체 시험 중 한 번 bash 출력이 빈 채로
        // 실패했는데 원인이 남지 않았다(#642)
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "{shell:?}가 실패로 끝났다: {:?}\nstderr: {stderr}\nstdout: {stdout:?}",
            out.status
        );
        if stdout.is_empty() {
            eprintln!(
                "{shell:?}: stdout이 비었다. 종료 상태 {:?}, stderr: {stderr}",
                out.status
            );
        }
        stdout
    }

    // zsh는 훅 함수를 직접 부르고, bash는 DEBUG 덫이 명령 앞에서 preexec를 부르게 `: 글`을 실행한다
    fn prompt_then_command(shell: Shell) -> &'static str {
        match shell {
            Shell::Zsh => "_bt_precmd; _bt_precmd_prompt; _bt_preexec 'ls -l'",
            _ => "_bt_precmd; : ls -l",
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
                Shell::Zsh => "_bt_precmd; _bt_precmd_prompt; _bt_preexec $'x\\e]0;evil\\a\\ey'",
                _ => "_bt_precmd; : $'x\\e]0;evil\\ay'",
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
            // bash는 줄 편집기가 마지막 줄만 다시 그리므로 redraw=last를 알린다(Ghostty ghostty.bash 188줄, #329). 둘 다 cl=line으로
            // 줄 안 클릭 이동을 받는다고 밝힌다(#338). zsh는 A를 PS1 안에 넣으므로 표준 출력에는 나오지 않는다(아래 시험)
            if matches!(sh, Shell::Bash) {
                assert!(out.contains("\x1b]133;A;redraw=last;cl=line\x07"), "{sh:?}: {out:?}");
            }
            assert!(out.contains("\x1b]133;C\x07"), "{sh:?}: {out:?}");
            assert!(out.contains("\x1b]633;E;"), "{sh:?}: {out:?}");
        });
    }

    /// `PS1=<ps1>`로 둔 뒤 `body`를 돌리고 PS1을 찍어 되읽는다. 표지가 들어 있는지 본다
    fn ps1_after(sh: Shell, ps1: &str, body: &str) -> String {
        let show = match sh {
            Shell::Zsh => "print -r -- \"PS1=$PS1\"",
            // 찍는 명령이 사람이 친 명령으로 읽혀 PS1이 되돌려지지 않게 덫이 부르는 함수를 먼저 비운다(함수 정의는 덫에 걸리지 않는다)
            _ => "_bt_preexec() { :; }\nprintf 'PS1=%s\\n' \"$PS1\"",
        };
        let out = run_script(sh, Some(""), None, &format!("PS1='{ps1}'; {body}; {show}"));
        // PS1 안에 줄바꿈이 있을 수 있으니 마지막 `PS1=` 뒤를 통째로 읽는다
        out.rsplit_once("PS1=")
            .map(|(_, v)| v.strip_suffix('\n').unwrap_or(v).to_string())
            .unwrap_or_default()
    }

    fn precmd_only(sh: Shell) -> &'static str {
        match sh {
            Shell::Zsh => "_bt_precmd; _bt_precmd_prompt",
            _ => "_bt_precmd",
        }
    }

    /// 입력 칸을 가르는 표지(#338): PS1 끝에 `133;B`, 앞에 `133;A;cl=line`(zsh) 또는 `133;P;k=i`(bash는 A를 한 번 따로 찍는다)
    #[test]
    fn prompt_marks_open_the_input_area_at_the_end_of_ps1() {
        each_shell(|sh| {
            let ps1 = ps1_after(sh, "$ ", precmd_only(sh));
            match sh {
                Shell::Zsh => assert_eq!(ps1, "%{\x1b]133;A;cl=line\x07%}$ %{\x1b]133;B\x07%}"),
                _ => assert_eq!(ps1, "\\[\\e]133;P;k=i\\a\\]$ \\[\\e]133;B\\a\\]"),
            }
        });
    }

    #[test]
    fn prompt_marks_are_not_added_twice_and_are_removed_before_the_command() {
        each_shell(|sh| {
            let marked = ps1_after(sh, "$ ", precmd_only(sh));
            // 명령 없이 프롬프트가 다시 와도(빈 줄에서 Enter) 같은 값이다
            let again = ps1_after(sh, "$ ", &format!("{0}; {0}", precmd_only(sh)));
            assert_eq!(again, marked, "{sh:?}: 표지가 두 번 들어감");
            // 명령 앞에서는 표지를 더하기 전의 PS1로 돌아간다
            let body = match sh {
                Shell::Zsh => "_bt_precmd; _bt_precmd_prompt; _bt_preexec 'ls'",
                _ => "_bt_precmd; : ls",
            };
            assert_eq!(ps1_after(sh, "$ ", body), "$ ", "{sh:?}");
        });
    }

    #[test]
    fn multiline_prompts_mark_following_lines_as_continuation() {
        each_shell(|sh| match sh {
            Shell::Zsh => {
                let ps1 = ps1_after(sh, "~\n$ ", precmd_only(sh));
                assert!(ps1.contains("~\n%{\x1b]133;P;k=s\x07%}$ "), "{ps1:?}");
            }
            _ => {
                // bash는 글자 그대로의 `\n`(프롬프트 이스케이프)만 바꾼다
                let ps1 = ps1_after(sh, "~\\n$ ", precmd_only(sh));
                assert!(ps1.contains("\\n\\[\\e]133;P;k=s\\a\\]$ "), "{ps1:?}");
            }
        });
    }

    /// zsh: PS1이 홀로 `%`로 끝나면 표지의 `%{`와 붙어 표지를 삼킨다. `%%`로 쓴다
    #[test]
    fn zsh_trailing_percent_in_ps1_does_not_swallow_the_mark() {
        if !have("zsh") {
            return;
        }
        let ps1 = ps1_after(Shell::Zsh, "x%", precmd_only(Shell::Zsh));
        assert!(ps1.contains("x%%%{\x1b]133;B\x07%}"), "{ps1:?}");
    }

    /// bash: 빈 줄에서 Enter만 치면 명령 없이 `PROMPT_COMMAND`가 다시 돈다. 그 첫 명령 `_bt_status` 앞의 DEBUG 덫은
    /// 사람이 친 명령이 아니므로 명령 글(633;E)·시작(C)·끝(D)을 보내지 않는다(#342, Ghostty `bash-preexec`의 `__bp_in_prompt_command`)
    #[test]
    fn bash_empty_enter_is_not_a_command() {
        if !have("bash") {
            return;
        }
        // 프롬프트 → 빈 Enter(PROMPT_COMMAND의 `_bt_status`, `_bt_precmd`) → 다음 프롬프트
        let empty_enter = "_bt_precmd; _bt_status; _bt_precmd";
        let out = run_script(Shell::Bash, Some(""), None, empty_enter);
        assert!(!out.contains("\x1b]633;E"), "명령 글이 나옴: {out:?}");
        assert!(!out.contains("\x1b]133;C"), "명령 시작이 나옴: {out:?}");
        assert!(!out.contains("\x1b]133;D"), "명령 끝이 나옴: {out:?}");
        assert_eq!(out.matches("\x1b]133;A;").count(), 2, "프롬프트 표지는 두 번: {out:?}");
        // 표지는 한 벌만 남는다
        assert_eq!(
            ps1_after(Shell::Bash, "$ ", empty_enter),
            ps1_after(Shell::Bash, "$ ", "_bt_precmd")
        );
        // 그 뒤에 친 명령은 그대로 명령이다
        let out = run_script(Shell::Bash, Some(""), None, &format!("{empty_enter}; : ls"));
        assert_eq!(out.matches("\x1b]133;C").count(), 1, "{out:?}");
        assert!(out.contains("\x1b]633;E;: ls\x07"), "{out:?}");
    }

    /// 폴더 이름은 믿을 수 없는 글이다(Ghostty main #14859). 이름 안의 ESC·BEL이 OSC 7을 일찍 끝내고 다른 OSC(52 클립보드 쓰기 등)를 끼우면 안 된다
    #[test]
    fn cwd_report_drops_control_bytes_from_the_directory_name() {
        each_shell(|sh| {
            let base = std::env::temp_dir().join(format!("bt-cwd-{}-{sh:?}", std::process::id()));
            let _ = std::fs::remove_dir_all(&base);
            // 진짜 BEL·ESC가 든 이름과, 글자 그대로의 역슬래시 이름
            for (name, shown) in [
                ("x\u{7}\u{1b}]52;c;SGVsbG8=\u{7}", "x]52;c;SGVsbG8="),
                ("x\\a\\e]52;c;SGVsbG8=\\a", "x\\a\\e]52;c;SGVsbG8=\\a"),
            ] {
                let dir = base.join(name);
                std::fs::create_dir_all(&dir).unwrap();
                let out = run_script_in(sh, Some(""), None, Some(&dir), "_bt_precmd");
                assert!(!out.contains("\x1b]52"), "{sh:?} {name:?}: 다른 OSC가 끼었다: {out:?}");
                let (_, after) = out
                    .split_once("\x1b]7;")
                    .unwrap_or_else(|| panic!("{sh:?}: OSC 7 없음: {out:?}"));
                let (seg, _) = after.split_once('\x07').unwrap_or_else(|| panic!("{sh:?}: {out:?}"));
                assert!(seg.starts_with("kitty-shell-cwd://"), "{sh:?}: {seg:?}");
                assert!(seg.ends_with(shown), "{sh:?} {name:?}: {seg:?}");
                assert!(!seg.chars().any(char::is_control), "{sh:?}: {seg:?}");
            }
            let _ = std::fs::remove_dir_all(&base);
        });
    }

    /// zsh: 다른 훅이 PS1을 다시 만들어 표지가 빠져도 줄 편집기가 시작할 때(zle-line-init) B를 보강한다(Ghostty zsh). 사용자의 같은 이름 위젯은 그대로 부른다
    #[test]
    fn zsh_line_init_adds_b_when_ps1_has_no_marks_and_keeps_user_widgets() {
        if !have("zsh") {
            return;
        }
        let z = |body: &str| run_script(Shell::Zsh, Some(""), None, &format!("zmodload zsh/zle\n{body}"));
        // 사용자의 위젯이 없을 때: 우리 함수를 위젯으로 건다
        let out = z("_bt_precmd; print -r -- \"W=$widgets[zle-line-init]\"");
        assert!(out.contains("W=user:_bt_zle_line_init"), "{out:?}");
        // 사용자의 위젯이 있을 때: 이름을 옮겨 두고 우리 함수가 끝에 부른다
        let out = z("zle-line-init() { :; }; zle -N zle-line-init; _bt_precmd; \
                     print -r -- \"W=$widgets[zle-line-init]\"; print -r -- \"O=$widgets[._bt_orig_zle-line-init]\"; \
                     print -r -- \"F=$functions[_bt_zle_line_init]\"");
        assert!(out.contains("W=user:_bt_zle_line_init"), "{out:?}");
        assert!(out.contains("O=user:zle-line-init"), "{out:?}");
        assert!(out.contains("._bt_orig_zle-line-init"), "{out:?}");
        // add-zle-hook-widget으로 걸린 위젯이면 같은 방식으로 더한다(위젯을 감싸면 그 도구가 깨진다)
        let out = z(
            "autoload -Uz add-zle-hook-widget; f() { :; }; add-zle-hook-widget line-init f; _bt_precmd; \
                     print -r -- \"W=$widgets[zle-line-init]\"; print -r -- \"H=$zle_hook_widgets\"; \
                     print -r -- \"L=${(j: :)${(k)widgets[(R)*_bt_zle_line_init*]}}\"",
        );
        assert!(out.contains("W=user:azhw:zle-line-init"), "{out:?}");
        // 두 번째 프롬프트에서 다시 걸지 않는다(두 번 감싸면 B가 두 번 나간다)
        let out = z("zle-line-init() { :; }; zle -N zle-line-init; _bt_precmd; _bt_precmd; \
                     print -r -- \"F=$functions[_bt_zle_line_init]\"");
        assert_eq!(out.matches("._bt_orig_zle-line-init").count(), 1, "{out:?}");
        // 위젯 본문: PS1에 표지가 없으면 P와 B를 보내고, 있으면 보내지 않는다
        let out = z("PS1='$ '; _bt_zle_line_init");
        assert!(out.contains("\x1b]133;P;k=i\x07\x1b]133;B\x07"), "{out:?}");
        let out = z("PS1='$ '; _bt_precmd; _bt_precmd_prompt; _bt_zle_line_init");
        assert!(!out.contains("\x1b]133;B"), "표지가 있는데 B를 또 보냈다: {out:?}");
    }

    // ---- fish·nushell·elvish: XDG_DATA_DIRS로 싣는다(Ghostty setupXdgDataDirs) ----

    #[test]
    fn detects_fish_nushell_elvish() {
        assert_eq!(Shell::detect("/opt/homebrew/bin/fish"), Some(Shell::Fish));
        assert_eq!(Shell::detect("-fish"), Some(Shell::Fish));
        assert_eq!(Shell::detect("/opt/homebrew/bin/nu"), Some(Shell::Nushell));
        assert_eq!(Shell::detect("/usr/local/bin/elvish"), Some(Shell::Elvish));
        assert_eq!(Shell::detect("/usr/bin/nushell"), None);
    }

    #[test]
    fn xdg_data_dirs_gets_our_dir_first_and_keeps_the_rest() {
        let d = Path::new("/tmp/bt-x/shell-integration/0.1");
        assert_eq!(
            xdg_data_dirs_with(Some("/opt/share:/usr/share"), d).as_deref(),
            Some("/tmp/bt-x/shell-integration/0.1:/opt/share:/usr/share")
        );
        // 변수가 없으면 기본값(XDG 규격)을 잃지 않게 함께 둔다
        assert_eq!(
            xdg_data_dirs_with(None, d).as_deref(),
            Some("/tmp/bt-x/shell-integration/0.1:/usr/local/share:/usr/share")
        );
        // 빈 값이면 우리 폴더만(Ghostty prependEnv)
        assert_eq!(
            xdg_data_dirs_with(Some(""), d).as_deref(),
            Some("/tmp/bt-x/shell-integration/0.1")
        );
        // 목록 구분자(:)나 제어 문자가 든 폴더, 절대 경로가 아닌 폴더는 목록을 깨므로 싣지 않는다
        for bad in ["/tmp/a:b", "/tmp/a\nb", "/tmp/a\u{1b}b", "rel/dir"] {
            assert_eq!(xdg_data_dirs_with(Some("/opt/share"), Path::new(bad)), None, "{bad:?}");
        }
    }

    #[test]
    fn fish_nushell_elvish_get_xdg_data_dirs_and_no_other_changes() {
        let f = ShellFeatures::default();
        for mode in ["fish", "nushell", "elvish"] {
            let mut o = SpawnOptions::default();
            apply(&mut o, Path::new("/x/si"), mode, &f, Some(Path::new("/opt/bt")));
            let xdg = env_of(&o, "XDG_DATA_DIRS").unwrap_or_else(|| panic!("{mode}: XDG_DATA_DIRS 없음"));
            assert!(xdg.starts_with("/x/si"), "{mode}: 우리 폴더가 맨 앞: {xdg}");
            assert_eq!(env_of(&o, "BT_SHELL_INTEGRATION_XDG_DIR"), Some("/x/si"), "{mode}");
            assert_eq!(env_of(&o, "BT_SHELL_INTEGRATION_DIR"), Some("/x/si"), "{mode}");
            assert_eq!(env_of(&o, "BT_SHELL_FEATURES"), Some("cursor,title,path"), "{mode}");
            assert_eq!(env_of(&o, "BT_BIN_DIR"), Some("/opt/bt"), "{mode}");
            // 사용자의 시작 파일은 건드리지 않는다: ZDOTDIR·bash 인자도, 셸 프로그램·인자도 그대로
            assert!(env_of(&o, "ZDOTDIR").is_none(), "{mode}");
            assert!(o.program.is_none() && o.args.is_empty(), "{mode}");
        }
        // 목록을 깨는 폴더면 아무것도 싣지 않는다
        let mut o = SpawnOptions::default();
        apply(&mut o, Path::new("/x:y"), "fish", &f, None);
        assert!(o.env.is_empty(), "{:?}", o.env);
    }

    /// 파일이 모두 풀리고, 셸이 XDG_DATA_DIRS 아래에서 찾는 이름 그대로 놓인다
    #[test]
    fn fish_nushell_elvish_scripts_sit_where_the_shells_look() {
        for rel in [
            "fish/vendor_conf.d/b-terminal-shell-integration.fish",
            "nushell/vendor/autoload/b-terminal.nu",
            "elvish/lib/b-terminal-integration.elv",
        ] {
            assert!(FILES.iter().any(|(r, t)| *r == rel && !t.is_empty()), "{rel}");
        }
    }
}
