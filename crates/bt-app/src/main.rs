//! b-terminal 창 앱

mod app;
mod control;
mod desktop;
mod dump;
mod keymap;
mod keys;
mod menu;
mod mouse;
mod padding;
mod palette;
mod quick;
mod shellint;
mod split;
mod state;
mod tabbar;

use bt_term::SpawnOptions;
use winit::event_loop::EventLoop;

const USAGE: &str =
    "사용: b-terminal [--config-file <파일>] [--show-config] [--list-themes] [--cwd <폴더>] [-e <프로그램> [인자...]]

설정: ~/.config/b-terminal/config (Ghostty와 같은 문법). 테마 905개를 넣어 두었습니다(--list-themes).";

/// 명령줄에서 읽은 것
#[derive(Debug, Default)]
struct Cli {
    first: SpawnOptions,
    config_file: Option<std::path::PathBuf>,
    show_config: bool,
}

/// 명령줄: `-e` 뒤는 모두 처음 탭에서 띄울 프로그램과 인자다(Ghostty·xterm과 같다)
fn parse_args(args: impl Iterator<Item = String>) -> Result<Cli, String> {
    let mut cli = Cli::default();
    let opts = &mut cli.first;
    let mut args = args.peekable();
    while let Some(a) = args.next() {
        match a.as_str() {
            "-e" => {
                opts.program = Some(args.next().ok_or("-e 뒤에 프로그램이 필요합니다")?);
                opts.args = args.by_ref().collect();
            }
            "--cwd" => opts.cwd = Some(args.next().ok_or("--cwd 뒤에 폴더가 필요합니다")?.into()),
            "--config-file" => {
                cli.config_file = Some(args.next().ok_or("--config-file 뒤에 파일이 필요합니다")?.into())
            }
            "--show-config" => cli.show_config = true,
            "--list-themes" => {
                return Err(bt_core::config::bundled_theme_names().collect::<Vec<_>>().join("\n"));
            }
            "-h" | "--help" => return Err(USAGE.into()),
            "--version" => return Err(format!("b-terminal {}", env!("CARGO_PKG_VERSION"))),
            other => return Err(format!("모르는 인자: {other}\n{USAGE}")),
        }
    }
    Ok(cli)
}

/// macOS가 어두운 모드인가(`theme = light:…,dark:…`에서 고를 쪽). 모르면 어두운 쪽
pub fn system_is_dark() -> bool {
    #[cfg(target_os = "macos")]
    {
        // 밝은 모드에서는 이 키가 없어 명령이 실패한다
        std::process::Command::new("defaults")
            .args(["read", "-g", "AppleInterfaceStyle"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().eq_ignore_ascii_case("dark"))
            .unwrap_or(true)
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = match parse_args(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(msg) => {
            println!("{msg}");
            return Ok(());
        }
    };
    let config = bt_core::Config::load(cli.config_file.as_deref(), system_is_dark());
    if cli.show_config {
        for src in &config.sources {
            println!("# 읽은 파일: {}", src.display());
        }
        for w in &config.warnings {
            println!("# 경고: {w}");
        }
        println!("{}", config.render());
        return Ok(());
    }
    for w in &config.warnings {
        log::warn!("설정: {w}");
    }
    let event_loop = EventLoop::<app::UserEvent>::with_user_event().build()?;
    let mut app = app::App::new(config, cli.config_file.clone(), event_loop.create_proxy(), cli.first);
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(v: &[&str]) -> Result<SpawnOptions, String> {
        parse_args(v.iter().map(|s| s.to_string())).map(|c| c.first)
    }

    #[test]
    fn dash_e_takes_the_rest_as_command() {
        let o = parse(&["--cwd", "/tmp", "-e", "vim", "-u", "NONE"]).unwrap();
        assert_eq!(o.program.as_deref(), Some("vim"));
        assert_eq!(o.args, vec!["-u", "NONE"]);
        assert_eq!(o.cwd.as_deref(), Some(std::path::Path::new("/tmp")));
        assert!(parse(&["-e"]).is_err());
        assert!(parse(&["--nope"]).is_err());
        let cli = parse_args(
            ["--config-file", "/tmp/x", "--show-config"]
                .iter()
                .map(|s| s.to_string()),
        )
        .unwrap();
        assert_eq!(cli.config_file.as_deref(), Some(std::path::Path::new("/tmp/x")));
        assert!(cli.show_config);
    }

    #[test]
    fn osc7_cwd_must_be_a_local_directory() {
        let tmp = std::env::temp_dir();
        assert!(app::osc7_is_local("", &tmp));
        assert!(app::osc7_is_local("localhost", &tmp));
        let me = gethostname::gethostname().to_string_lossy().into_owned();
        assert!(app::osc7_is_local(&me, &tmp));
        assert!(
            !app::osc7_is_local("evil.example.com", &tmp),
            "다른 호스트의 경로는 받지 않는다"
        );
        assert!(
            !app::osc7_is_local("", std::path::Path::new("/definitely/not/here")),
            "없는 폴더는 받지 않는다"
        );
        assert!(!app::osc7_is_local("", std::path::Path::new("relative")));
    }
}
