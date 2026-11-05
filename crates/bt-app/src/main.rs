//! b-terminal 창 앱

mod app;
mod desktop;
mod keys;
mod mouse;
mod sidebar;

use bt_term::SpawnOptions;
use winit::event_loop::EventLoop;

const USAGE: &str = "사용: b-terminal [--cwd <폴더>] [-e <프로그램> [인자...]]";

/// 명령줄: `-e` 뒤는 모두 처음 탭에서 띄울 프로그램과 인자다(Ghostty·xterm과 같다)
fn parse_args(args: impl Iterator<Item = String>) -> Result<SpawnOptions, String> {
    let mut opts = SpawnOptions::default();
    let mut args = args.peekable();
    while let Some(a) = args.next() {
        match a.as_str() {
            "-e" => {
                opts.program = Some(args.next().ok_or("-e 뒤에 프로그램이 필요합니다")?);
                opts.args = args.by_ref().collect();
            }
            "--cwd" => opts.cwd = Some(args.next().ok_or("--cwd 뒤에 폴더가 필요합니다")?.into()),
            "-h" | "--help" => return Err(USAGE.into()),
            "--version" => return Err(format!("b-terminal {}", env!("CARGO_PKG_VERSION"))),
            other => return Err(format!("모르는 인자: {other}\n{USAGE}")),
        }
    }
    Ok(opts)
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let first = match parse_args(std::env::args().skip(1)) {
        Ok(o) => o,
        Err(msg) => {
            println!("{msg}");
            return Ok(());
        }
    };
    let event_loop = EventLoop::<app::UserEvent>::with_user_event().build()?;
    let mut app = app::App::new(app::AppConfig::default(), event_loop.create_proxy(), first);
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(v: &[&str]) -> Result<SpawnOptions, String> {
        parse_args(v.iter().map(|s| s.to_string()))
    }

    #[test]
    fn dash_e_takes_the_rest_as_command() {
        let o = parse(&["--cwd", "/tmp", "-e", "vim", "-u", "NONE"]).unwrap();
        assert_eq!(o.program.as_deref(), Some("vim"));
        assert_eq!(o.args, vec!["-u", "NONE"]);
        assert_eq!(o.cwd.as_deref(), Some(std::path::Path::new("/tmp")));
        assert!(parse(&["-e"]).is_err());
        assert!(parse(&["--nope"]).is_err());
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
