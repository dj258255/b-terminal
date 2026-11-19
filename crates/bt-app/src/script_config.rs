//! 스크립트 `surface configuration` 레코드(#666, Ghostty `ScriptSurfaceConfiguration`·`embedded.zig`)의 검사와 새 칸 하나용 덮어쓰기.
//!
//! 레코드는 새 칸 하나에만 쓴다. 설정 파일의 값은 바꾸지 않고 다음 칸에 남지도 않는다. `btctl` 소켓 규약은 넓히지 않는다.
//! 값(명령·폴더·처음 입력·환경 변수)에는 비밀이 들어 있을 수 있어 `Debug`와 로그에는 길이와 개수만 낸다.
//!
//! 흐름: vendor가 Apple Event의 레코드를 읽어(`RawConfig`, 크기 상한을 복사 전에 본다) 넘기면 `validate`가 뜻을 검사하고(`SurfaceConfig`),
//! `overrides`가 새 칸 하나용 덮어쓰기(`SpawnOverrides`)로 만든다. 덮어쓰기는 앱이 칸을 만드는 길(`handle_call`의 New → `spawn_pane`)에서 쓰인다.

// macOS에서만 AppleScript 명령이 부른다(Linux 빌드에서는 시험만 쓴다)
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use std::path::{Path, PathBuf};

use bt_term::SpawnOptions;

/// `command`·폴더·환경 변수 한 개의 상한(바이트)
pub const MAX_TEXT_BYTES: usize = 64 << 10;
/// `initial input`의 상한(바이트). `input text`와 같다
pub const MAX_INPUT_BYTES: usize = 1 << 20;
/// 환경 변수 개수의 상한
pub const MAX_ENV_COUNT: usize = 256;

/// vendor가 읽어 넘긴 레코드(아직 검사 전). `None`은 레코드에 그 키가 없다는 뜻이다
#[derive(Clone, PartialEq, Default)]
pub struct RawConfig {
    pub font_size: Option<f64>,
    pub working_directory: Option<String>,
    pub command: Option<String>,
    pub initial_input: Option<String>,
    pub wait_after_command: Option<bool>,
    pub environment: Option<Vec<String>>,
}

impl std::fmt::Debug for RawConfig {
    /// 값은 내지 않고 길이와 개수만 낸다
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RawConfig")
            .field("font_size", &self.font_size)
            .field(
                "working_directory_bytes",
                &self.working_directory.as_ref().map(|s| s.len()),
            )
            .field("command_bytes", &self.command.as_ref().map(|s| s.len()))
            .field("initial_input_bytes", &self.initial_input.as_ref().map(|s| s.len()))
            .field("wait_after_command", &self.wait_after_command)
            .field("environment_count", &self.environment.as_ref().map(|e| e.len()))
            .finish()
    }
}

/// 검사를 마친 레코드. 빈 값은 "정하지 않음"이다
#[derive(Clone, PartialEq, Default)]
pub struct SurfaceConfig {
    /// 점. 범위로 자르기 전의 값이다(레코드를 그대로 돌려주려고). 0과 빈 값은 `None`
    pub font_size: Option<f32>,
    pub working_directory: Option<String>,
    pub command: Option<String>,
    pub initial_input: Option<String>,
    pub wait_after_command: bool,
    /// 같은 키는 뒤의 것이 이기고, 자리는 처음 나온 자리다
    pub env: Vec<(String, String)>,
}

impl std::fmt::Debug for SurfaceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SurfaceConfig")
            .field("font_size", &self.font_size)
            .field(
                "working_directory_bytes",
                &self.working_directory.as_ref().map(|s| s.len()),
            )
            .field("command_bytes", &self.command.as_ref().map(|s| s.len()))
            .field("initial_input_bytes", &self.initial_input.as_ref().map(|s| s.len()))
            .field("wait_after_command", &self.wait_after_command)
            .field("env_count", &self.env.len())
            .finish()
    }
}

/// 새 칸 하나를 띄울 때 설정 위에 덮어쓸 값
#[derive(Clone, PartialEq, Default)]
pub struct SpawnOverrides {
    /// 절대 경로이면서 실제 폴더일 때만 있다
    pub cwd: Option<PathBuf>,
    /// `command`를 `/bin/sh -c "exec …"`로 푼 것(설정 `command`와 같다)
    pub program: Option<String>,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// 설정 `input` 목록을 대신할 바이트(이스케이프를 풀지 않은 그대로)
    pub input: Option<Vec<u8>>,
    /// 끝나도 칸을 남긴다(`command`가 있으면 켜진다)
    pub wait_after: bool,
    /// 이 칸만의 글자 크기(앱의 범위로 자른 값)
    pub font_size: Option<f32>,
}

impl std::fmt::Debug for SpawnOverrides {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SpawnOverrides")
            .field("cwd_bytes", &self.cwd.as_ref().map(|p| p.as_os_str().len()))
            .field("command", &self.program.as_ref().map(|_| "(있음)"))
            .field("env_count", &self.env.len())
            .field("input_bytes", &self.input.as_ref().map(|b| b.len()))
            .field("wait_after", &self.wait_after)
            .field("font_size", &self.font_size)
            .finish()
    }
}

/// 크기 상한만 본다. vendor가 같은 상한을 복사 전에 먼저 보고, 이것은 `admit`이 한 번 더 본다
pub fn check_raw_sizes(raw: &RawConfig) -> Result<(), String> {
    let text = |name: &str, v: &Option<String>| match v {
        Some(s) if s.len() > MAX_TEXT_BYTES => Err(format!(
            "{name}이 너무 깁니다({} 바이트, 상한 {MAX_TEXT_BYTES})",
            s.len()
        )),
        _ => Ok(()),
    };
    text("command", &raw.command)?;
    text("initial working directory", &raw.working_directory)?;
    if let Some(s) = &raw.initial_input {
        if s.len() > MAX_INPUT_BYTES {
            return Err(format!(
                "initial input이 너무 깁니다({} 바이트, 상한 {MAX_INPUT_BYTES})",
                s.len()
            ));
        }
    }
    if let Some(env) = &raw.environment {
        if env.len() > MAX_ENV_COUNT {
            return Err(format!(
                "environment variables가 너무 많습니다({}개, 상한 {MAX_ENV_COUNT})",
                env.len()
            ));
        }
        if env.iter().any(|e| e.len() > MAX_TEXT_BYTES) {
            return Err(format!(
                "environment variables의 한 항목이 너무 깁니다(상한 {MAX_TEXT_BYTES} 바이트)"
            ));
        }
    }
    Ok(())
}

/// 빈 글은 "정하지 않음"
fn non_empty(v: Option<String>) -> Option<String> {
    v.filter(|s| !s.is_empty())
}

/// `KEY=VALUE`를 첫 `=`에서 가른다. `=`가 없거나 키가 비었거나 NUL이 있으면 오류(Ghostty는 `=`가 없을 때만 오류다)
fn parse_env(item: &str) -> Result<(String, String), String> {
    let Some((key, value)) = item.split_once('=') else {
        return Err("environment variables는 KEY=VALUE여야 합니다(= 가 없는 항목이 있습니다)".into());
    };
    if key.is_empty() {
        return Err("environment variables의 키가 비어 있습니다".into());
    }
    if item.contains('\0') {
        return Err("environment variables에 NUL 문자가 있습니다".into());
    }
    Ok((key.to_string(), value.to_string()))
}

/// 레코드를 검사한다(Ghostty `ScriptSurfaceConfiguration.init(scriptRecord:)`와 같은 뜻 + 더 엄격한 환경 변수·NUL 검사)
pub fn validate(raw: RawConfig) -> Result<SurfaceConfig, String> {
    check_raw_sizes(&raw)?;
    let mut cfg = SurfaceConfig::default();
    if let Some(v) = raw.font_size {
        if !v.is_finite() {
            return Err("font size는 유한한 수여야 합니다".into());
        }
        if v < 0.0 {
            return Err("font size는 0 이상이어야 합니다(0은 정하지 않음)".into());
        }
        let f = v as f32;
        if !f.is_finite() {
            return Err("font size가 너무 큽니다".into());
        }
        if f > 0.0 {
            cfg.font_size = Some(f);
        }
    }
    cfg.working_directory = non_empty(raw.working_directory);
    cfg.command = non_empty(raw.command);
    cfg.initial_input = non_empty(raw.initial_input);
    // 명령과 폴더는 프로세스를 띄우는 인자라 NUL이 있으면 띄울 수 없다(Ghostty는 C 문자열이라 거기서 잘린다)
    if cfg.command.as_deref().is_some_and(|s| s.contains('\0')) {
        return Err("command에 NUL 문자가 있습니다".into());
    }
    if cfg.working_directory.as_deref().is_some_and(|s| s.contains('\0')) {
        return Err("initial working directory에 NUL 문자가 있습니다".into());
    }
    cfg.wait_after_command = raw.wait_after_command.unwrap_or(false);
    for item in raw.environment.iter().flatten() {
        let (k, v) = parse_env(item)?;
        match cfg.env.iter_mut().find(|(ek, _)| *ek == k) {
            Some(slot) => slot.1 = v,
            None => cfg.env.push((k, v)),
        }
    }
    Ok(cfg)
}

/// 검사한 레코드를 스크립트에 돌려줄 레코드로. 모든 키가 있고 정하지 않은 값은 기본값이다(Ghostty `dictionaryRepresentation`:
/// 글자 크기 0, 빈 글, 거짓, 빈 목록)
pub fn to_record(cfg: &SurfaceConfig) -> RawConfig {
    RawConfig {
        font_size: Some(cfg.font_size.map_or(0.0, f64::from)),
        working_directory: Some(cfg.working_directory.clone().unwrap_or_default()),
        command: Some(cfg.command.clone().unwrap_or_default()),
        initial_input: Some(cfg.initial_input.clone().unwrap_or_default()),
        wait_after_command: Some(cfg.wait_after_command),
        environment: Some(cfg.env.iter().map(|(k, v)| format!("{k}={v}")).collect()),
    }
}

/// 검사한 레코드에서 새 칸 하나용 덮어쓰기를 만든다(Ghostty `embedded.zig`의 규칙). `is_dir`은 폴더 판단(시험에서 바꿔 끼운다)
pub fn overrides(cfg: &SurfaceConfig, is_dir: impl Fn(&Path) -> bool) -> SpawnOverrides {
    let mut ov = SpawnOverrides::default();
    // 폴더는 절대 경로이면서 실제 폴더일 때만 쓴다. 아니면 무시한다(경로는 로그에 남기지 않는다)
    if let Some(wd) = &cfg.working_directory {
        let p = Path::new(wd);
        if p.is_absolute() && is_dir(p) {
            ov.cwd = Some(p.to_path_buf());
        } else {
            log::warn!(
                "스크립트가 준 작업 폴더를 쓰지 않습니다(절대 경로가 아니거나 폴더가 없습니다, {}바이트)",
                wd.len()
            );
        }
    }
    // command는 셸 명령이다. 설정 command처럼 셸을 거쳐 띄우고 끝나도 칸을 남긴다
    if let Some(cmd) = &cfg.command {
        let (program, args) = crate::lifecycle::shell_command(cmd);
        ov.program = Some(program);
        ov.args = args;
        ov.wait_after = true;
    }
    if cfg.wait_after_command {
        ov.wait_after = true;
    }
    ov.env = cfg.env.clone();
    ov.input = cfg.initial_input.as_ref().map(|s| s.as_bytes().to_vec());
    ov.font_size = cfg.font_size.map(bt_core::config::clamp_font_size);
    ov
}

impl SpawnOverrides {
    /// 띄울 프로그램·인자·폴더를 덮는다. 설정 `command`·`working-directory`보다 이긴다(첫 칸의 `initial-command`는 첫 칸에만 쓰이므로 만나지 않는다)
    pub fn apply_launch(&self, o: &mut SpawnOptions) {
        if let Some(program) = &self.program {
            o.program = Some(program.clone());
            o.args = self.args.clone();
        }
        if let Some(cwd) = &self.cwd {
            o.cwd = Some(cwd.clone());
        }
    }

    /// 환경 변수를 덧붙인다. 설정 `env` 뒤에 불러야 이긴다(b-terminal이 넣는 변수와의 순서는 설정 `env`와 같다)
    pub fn apply_env(&self, o: &mut SpawnOptions) {
        o.env.extend(self.env.iter().cloned());
    }
}

/// 칸에 보낼 처음 입력: 덮어쓰기에 `initial input`이 있으면 설정 `input` 목록 대신 그 글 하나(바이트 그대로, 이스케이프를 풀지 않는다).
/// 그때는 설정의 `input`(파일 포함)을 읽지도 않는다. 없으면 설정의 것
pub fn startup_input(ov: Option<&SpawnOverrides>, config: &[bt_core::config::InputSource]) -> Result<Vec<u8>, String> {
    match ov.and_then(|o| o.input.as_ref()) {
        Some(bytes) => Ok(bytes.clone()),
        None => bt_core::config::resolve_input(config),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::mpsc::channel;
    use std::time::{Duration, Instant};

    use bt_core::Theme;
    use bt_term::{GridSize, Session, SessionEvent};

    fn raw() -> RawConfig {
        RawConfig::default()
    }

    fn text(n: usize) -> String {
        "x".repeat(n)
    }

    #[test]
    fn empty_record_means_nothing_chosen() {
        assert_eq!(validate(raw()), Ok(SurfaceConfig::default()));
        // 빈 글은 "정하지 않음"
        let r = RawConfig {
            font_size: Some(0.0),
            working_directory: Some(String::new()),
            command: Some(String::new()),
            initial_input: Some(String::new()),
            wait_after_command: Some(false),
            environment: Some(vec![]),
        };
        assert_eq!(validate(r), Ok(SurfaceConfig::default()));
    }

    #[test]
    fn font_size_must_be_finite_and_not_negative() {
        let size = |v: f64| {
            validate(RawConfig {
                font_size: Some(v),
                ..raw()
            })
        };
        assert!(size(f64::NAN).is_err());
        assert!(size(f64::INFINITY).is_err());
        assert!(size(f64::NEG_INFINITY).is_err());
        assert!(size(-0.5).is_err());
        // f32로 줄이면 무한이 되는 값도 오류다
        assert!(size(1e300).is_err());
        // 0은 정하지 않음
        assert_eq!(size(0.0).unwrap().font_size, None);
        assert_eq!(size(13.5).unwrap().font_size, Some(13.5));
    }

    #[test]
    fn values_are_kept_as_given() {
        let r = RawConfig {
            font_size: Some(18.0),
            working_directory: Some("/tmp".into()),
            command: Some("echo hi".into()),
            initial_input: Some("ls\\n".into()),
            wait_after_command: Some(true),
            environment: Some(vec!["A=1".into()]),
        };
        let c = validate(r).unwrap();
        assert_eq!(c.font_size, Some(18.0));
        assert_eq!(c.working_directory.as_deref(), Some("/tmp"));
        assert_eq!(c.command.as_deref(), Some("echo hi"));
        assert_eq!(c.initial_input.as_deref(), Some("ls\\n"));
        assert!(c.wait_after_command);
        assert_eq!(c.env, vec![("A".to_string(), "1".to_string())]);
    }

    #[test]
    fn environment_needs_key_equals_value() {
        let env = |items: &[&str]| {
            validate(RawConfig {
                environment: Some(items.iter().map(|s| s.to_string()).collect()),
                ..raw()
            })
        };
        assert!(env(&["KEY"]).is_err(), "=가 없으면 오류");
        assert!(env(&["FOO=bar", "KEY"]).is_err());
        assert!(env(&["=value"]).is_err(), "키가 비었으면 오류(Ghostty보다 엄격)");
        assert!(env(&["A=b\0c"]).is_err(), "값의 NUL은 오류(Ghostty보다 엄격)");
        assert!(env(&["A\0B=c"]).is_err(), "키의 NUL은 오류(Ghostty보다 엄격)");
        // 첫 =에서 가른다. 값은 비어도 =를 품어도 된다
        assert_eq!(env(&["A="]).unwrap().env, vec![("A".into(), String::new())]);
        assert_eq!(env(&["A=b=c"]).unwrap().env, vec![("A".into(), "b=c".into())]);
        // 같은 키는 뒤의 것이 이긴다(Ghostty의 사전처럼). 자리는 처음 나온 자리다
        assert_eq!(
            env(&["A=1", "B=2", "A=3"]).unwrap().env,
            vec![("A".into(), "3".into()), ("B".into(), "2".into())]
        );
    }

    #[test]
    fn nul_in_command_or_directory_is_an_error() {
        let c = |command: &str| {
            validate(RawConfig {
                command: Some(command.into()),
                ..raw()
            })
        };
        assert!(c("echo\0hi").is_err());
        let d = validate(RawConfig {
            working_directory: Some("/tmp\0x".into()),
            ..raw()
        });
        assert!(d.is_err());
    }

    #[test]
    fn sizes_are_capped_before_anything_is_used() {
        let ok_and_over = |ok: RawConfig, over: RawConfig| {
            assert!(check_raw_sizes(&ok).is_ok());
            assert!(validate(ok).is_ok());
            assert!(check_raw_sizes(&over).is_err());
            assert!(validate(over).is_err());
        };
        ok_and_over(
            RawConfig {
                command: Some(text(MAX_TEXT_BYTES)),
                ..raw()
            },
            RawConfig {
                command: Some(text(MAX_TEXT_BYTES + 1)),
                ..raw()
            },
        );
        ok_and_over(
            RawConfig {
                working_directory: Some(text(MAX_TEXT_BYTES)),
                ..raw()
            },
            RawConfig {
                working_directory: Some(text(MAX_TEXT_BYTES + 1)),
                ..raw()
            },
        );
        ok_and_over(
            RawConfig {
                initial_input: Some(text(MAX_INPUT_BYTES)),
                ..raw()
            },
            RawConfig {
                initial_input: Some(text(MAX_INPUT_BYTES + 1)),
                ..raw()
            },
        );
        let many = |n: usize| (0..n).map(|i| format!("K{i}=v")).collect::<Vec<_>>();
        ok_and_over(
            RawConfig {
                environment: Some(many(MAX_ENV_COUNT)),
                ..raw()
            },
            RawConfig {
                environment: Some(many(MAX_ENV_COUNT + 1)),
                ..raw()
            },
        );
        ok_and_over(
            RawConfig {
                environment: Some(vec![format!("K={}", text(MAX_TEXT_BYTES - 2))]),
                ..raw()
            },
            RawConfig {
                environment: Some(vec![format!("K={}", text(MAX_TEXT_BYTES - 1))]),
                ..raw()
            },
        );
    }

    #[test]
    fn record_to_script_has_every_key_with_ghostty_defaults() {
        let d = to_record(&SurfaceConfig::default());
        assert_eq!(d.font_size, Some(0.0));
        assert_eq!(d.working_directory.as_deref(), Some(""));
        assert_eq!(d.command.as_deref(), Some(""));
        assert_eq!(d.initial_input.as_deref(), Some(""));
        assert_eq!(d.wait_after_command, Some(false));
        assert_eq!(d.environment, Some(vec![]));
        let c = SurfaceConfig {
            font_size: Some(14.5),
            working_directory: Some("/tmp".into()),
            command: Some("ls".into()),
            initial_input: Some("x".into()),
            wait_after_command: true,
            env: vec![("A".into(), "b=c".into())],
        };
        let r = to_record(&c);
        assert_eq!(r.font_size, Some(14.5));
        assert_eq!(r.working_directory.as_deref(), Some("/tmp"));
        assert_eq!(r.command.as_deref(), Some("ls"));
        assert_eq!(r.initial_input.as_deref(), Some("x"));
        assert_eq!(r.wait_after_command, Some(true));
        assert_eq!(r.environment, Some(vec!["A=b=c".to_string()]));
        // 돌려준 레코드를 다시 넣어도 같다(`new surface configuration from`)
        assert_eq!(validate(to_record(&c)), Ok(c));
    }

    #[test]
    fn debug_never_shows_values() {
        let secret = "hunter2-토큰";
        let raw = RawConfig {
            font_size: Some(12.0),
            working_directory: Some(format!("/{secret}")),
            command: Some(format!("echo {secret}")),
            initial_input: Some(secret.into()),
            wait_after_command: Some(true),
            environment: Some(vec![format!("TOKEN={secret}")]),
        };
        let cfg = validate(raw.clone()).unwrap();
        let ov = overrides(&cfg, |_| true);
        for shown in [format!("{raw:?}"), format!("{cfg:?}"), format!("{ov:?}")] {
            assert!(!shown.contains("hunter2") && !shown.contains("토큰"), "{shown}");
            assert!(!shown.contains("TOKEN") && !shown.contains("echo"), "{shown}");
        }
        // 길이와 개수는 나온다
        assert!(format!("{raw:?}").contains("command_bytes"), "{raw:?}");
        assert!(format!("{raw:?}").contains("environment_count"), "{raw:?}");
        assert!(format!("{ov:?}").contains("env_count"), "{ov:?}");
    }

    #[test]
    fn working_directory_is_used_only_when_absolute_and_a_directory() {
        let cfg = |p: &str| SurfaceConfig {
            working_directory: Some(p.into()),
            ..Default::default()
        };
        let dir = |p: &Path| p == Path::new("/tmp");
        assert_eq!(overrides(&cfg("/tmp"), dir).cwd, Some(PathBuf::from("/tmp")));
        // 상대 경로는 실제로 있어도 무시한다
        assert_eq!(overrides(&cfg("tmp"), |_| true).cwd, None);
        assert_eq!(overrides(&cfg("./tmp"), |_| true).cwd, None);
        // 절대 경로이지만 폴더가 아니면 무시한다
        assert_eq!(overrides(&cfg("/no/such"), dir).cwd, None);
        assert_eq!(overrides(&SurfaceConfig::default(), dir).cwd, None);
    }

    #[test]
    fn command_runs_through_sh_and_waits_after_exit() {
        let cfg = SurfaceConfig {
            command: Some("echo hi; sleep 1".into()),
            ..Default::default()
        };
        let ov = overrides(&cfg, |_| false);
        assert_eq!(ov.program.as_deref(), Some("/bin/sh"));
        assert_eq!(ov.args, vec!["-c".to_string(), "exec echo hi; sleep 1".to_string()]);
        assert!(ov.wait_after, "command가 있으면 wait-after-command도 켠다");
        // command 없이 wait만
        let w = overrides(
            &SurfaceConfig {
                wait_after_command: true,
                ..Default::default()
            },
            |_| false,
        );
        assert!(w.wait_after && w.program.is_none());
        // 아무것도 없으면 아무것도 덮지 않는다
        assert_eq!(
            overrides(&SurfaceConfig::default(), |_| false),
            SpawnOverrides::default()
        );
    }

    #[test]
    fn font_size_is_clamped_to_the_app_range() {
        let size = |v: f32| {
            overrides(
                &SurfaceConfig {
                    font_size: Some(v),
                    ..Default::default()
                },
                |_| false,
            )
            .font_size
        };
        assert_eq!(size(2.0), Some(6.0));
        assert_eq!(size(100.0), Some(72.0));
        assert_eq!(size(14.5), Some(14.5));
        assert_eq!(
            overrides(&SurfaceConfig::default(), |_| false).font_size,
            None,
            "정하지 않으면 window-inherit-font-size를 따른다"
        );
    }

    #[test]
    fn initial_input_replaces_the_input_list_and_is_not_unescaped() {
        use bt_core::config::InputSource;
        let config = vec![InputSource::Raw(b"from-config\n".to_vec())];
        // 덮어쓰기가 없으면 설정의 것
        assert_eq!(startup_input(None, &config).unwrap(), b"from-config\n");
        let none = overrides(&SurfaceConfig::default(), |_| false);
        assert_eq!(startup_input(Some(&none), &config).unwrap(), b"from-config\n");
        // 있으면 그 글 하나(설정의 input은 보내지 않는다). 이스케이프를 풀지 않는다
        let cfg = SurfaceConfig {
            initial_input: Some("ls\\n한글".into()),
            ..Default::default()
        };
        let ov = overrides(&cfg, |_| false);
        assert_eq!(startup_input(Some(&ov), &config).unwrap(), "ls\\n한글".as_bytes());
        // 설정에 읽을 수 없는 input 파일이 있어도 덮어썼으면 읽지 않는다
        let bad = vec![InputSource::Path("/no/such/input/file".into())];
        assert!(startup_input(None, &bad).is_err());
        assert_eq!(startup_input(Some(&ov), &bad).unwrap(), "ls\\n한글".as_bytes());
    }

    #[test]
    fn env_is_appended_after_the_config_env_so_it_wins() {
        let mut o = SpawnOptions::default();
        o.env.push(("BT_TAB_ID".into(), "7".into()));
        o.env.extend([("A".to_string(), "from-config".to_string())]);
        let cfg = SurfaceConfig {
            env: vec![("A".into(), "from-script".into())],
            ..Default::default()
        };
        overrides(&cfg, |_| false).apply_env(&mut o);
        assert_eq!(
            o.env,
            vec![
                ("BT_TAB_ID".to_string(), "7".to_string()),
                ("A".to_string(), "from-config".to_string()),
                ("A".to_string(), "from-script".to_string()),
            ]
        );
    }

    #[test]
    fn launch_override_beats_the_config_command() {
        let mut o = SpawnOptions {
            program: Some("/bin/sh".into()),
            args: vec!["-c".into(), "exec config-command".into()],
            cwd: Some(PathBuf::from("/var")),
            ..Default::default()
        };
        let cfg = SurfaceConfig {
            command: Some("script-command".into()),
            working_directory: Some("/tmp".into()),
            ..Default::default()
        };
        overrides(&cfg, |_| true).apply_launch(&mut o);
        assert_eq!(o.args, vec!["-c".to_string(), "exec script-command".to_string()]);
        assert_eq!(o.cwd, Some(PathBuf::from("/tmp")));
        // 덮을 것이 없으면 설정의 것이 그대로다
        let mut keep = o.clone();
        overrides(&SurfaceConfig::default(), |_| true).apply_launch(&mut keep);
        assert_eq!(keep.args, o.args);
        assert_eq!(keep.cwd, o.cwd);
    }

    /// 덮어쓴 env·command·input·폴더가 실제 PTY 자식에 닿는다. /bin/sh와 printf만 쓴다(사용자의 로그인 셸 설정이 끼지 않게)
    #[test]
    fn overrides_reach_the_real_pty_child() {
        let dir = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        let cfg = SurfaceConfig {
            command: Some(
                // 앞에 `exec`가 붙으므로 여러 문장은 `/bin/sh -c '…'`로 감싼다(이슈의 예와 같다)
                "/bin/sh -c 'printf \"env=%s pwd=%s other=%s\\n\" \"$BTSCRIPT_A\" \"$(pwd -P)\" \"$BTSCRIPT_CONFIG\"; read line; printf \"got=%s\\n\" \"$line\"; sleep 3'"
                    .into(),
            ),
            working_directory: Some(dir.to_string_lossy().into_owned()),
            // 설정 입력은 보내지 않고 이 글 하나만 간다
            initial_input: Some("typed\r".into()),
            env: vec![("BTSCRIPT_A".into(), "from-script".into())],
            ..Default::default()
        };
        let ov = overrides(&cfg, |p| p.is_dir());
        let mut options = SpawnOptions::default();
        // 설정 env가 먼저, 스크립트 env가 그 뒤(spawn_pane과 같은 차례)
        options.env.push(("BTSCRIPT_A".into(), "from-config".into()));
        options.env.push(("BTSCRIPT_CONFIG".into(), "kept".into()));
        ov.apply_launch(&mut options);
        ov.apply_env(&mut options);
        options.input = startup_input(Some(&ov), &[bt_core::config::InputSource::Raw(b"never\r".to_vec())]).unwrap();

        let (tx, _rx) = channel::<SessionEvent>();
        let tx = std::sync::Mutex::new(tx);
        let sink = Arc::new(move |e: SessionEvent| {
            let _ = tx.lock().unwrap().send(e);
        });
        let session = Session::spawn(options, GridSize::new(100, 10), Theme::default(), sink).unwrap();
        let want = format!("env=from-script pwd={} other=kept", dir.display());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let screen = session.snapshot().text();
            if screen.contains(&want) && screen.contains("got=typed") {
                assert!(!screen.contains("never"), "{screen}");
                break;
            }
            assert!(Instant::now() < deadline, "자식에 닿지 않았다:\n{screen}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
