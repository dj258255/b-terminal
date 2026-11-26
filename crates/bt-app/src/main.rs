//! b-terminal 창 앱

mod a11y;
mod app;
mod appearance;
mod bell_border;
mod bell_sound;
mod blur;
mod broadcast;
mod cliact;
mod cliinfo;
mod context_menu;
mod control;
mod copy;
mod default_terminal;
mod desktop;
mod divider_drag;
mod dock_open;
mod dump;
mod file_drop;
mod icon;
mod keymap;
mod keys;
mod lifecycle;
mod lookup;
mod menu;
mod mouse;
mod padding;
mod palette;
mod pane_drag;
mod pane_font;
mod pointer;
mod quick;
mod render_path;
mod script_config;
mod script_key;
mod script_mouse;
mod scrollback_store;
mod secure_input;
mod shellint;
mod split;
mod startup;
mod state;
mod status_note;
mod tabbar;
mod tabcolor;
mod text_blink;
mod toggles;
mod undo;
mod winact;
mod windeco;

use bt_term::SpawnOptions;
use winit::event_loop::EventLoop;

const USAGE: &str =
    "사용: b-terminal [--config-file <파일>] [--config-default-files=false] [--show-config] [--list-themes] [--cwd <폴더>] [-e <프로그램> [인자...]]

설정: ~/.config/b-terminal/config (Ghostty와 같은 문법). 테마 914개를 넣어 두었습니다(--list-themes).
--config-default-files=false: 위 기본 설정 파일을 읽지 않습니다(--config-file로 준 파일은 읽습니다).
--<설정 키>=<값>: 설정 파일의 그 키를 덮어씁니다(예: --font-size=20). 기본 설정 파일 다음, --config-file 앞에 적용합니다.";

/// 명령줄에서 읽은 것
#[derive(Debug, Default)]
struct Cli {
    first: SpawnOptions,
    config_file: Option<std::path::PathBuf>,
    show_config: bool,
    /// `--config-default-files=false`면 거짓(기본 설정 파일을 읽지 않는다)
    default_files: bool,
    /// `--키=값` 설정 덮어쓰기(Ghostty `loadCliArgs`, #743). 값 없는 `--키`는 `true`
    settings: Vec<(String, String)>,
    /// 설정 키가 아니라 무시한 인자(Ghostty의 "invalid field" 진단). 앱은 그대로 뜬다
    warnings: Vec<String>,
}

/// 명령줄: `-e` 뒤는 모두 처음 탭에서 띄울 프로그램과 인자다(Ghostty·xterm과 같다)
fn parse_args(args: impl Iterator<Item = String>) -> Result<Cli, String> {
    let mut cli = Cli {
        default_files: true,
        ..Cli::default()
    };
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
            v if v.starts_with("--config-file=") => cli.config_file = Some(v["--config-file=".len()..].into()),
            "--show-config" => cli.show_config = true,
            // 명령줄 전용 bool(Ghostty). 값 없이 쓰면 켬
            "--config-default-files" => cli.default_files = true,
            v if v.starts_with("--config-default-files=") => {
                cli.default_files = match &v["--config-default-files=".len()..] {
                    "true" => true,
                    "false" => false,
                    x => return Err(format!("--config-default-files는 true나 false여야 합니다: {x}")),
                }
            }
            "--list-themes" => {
                return Err(bt_core::config::bundled_theme_names().collect::<Vec<_>>().join("\n"));
            }
            "-h" | "--help" => return Err(USAGE.into()),
            "--version" => return Err(format!("b-terminal {}", env!("CARGO_PKG_VERSION"))),
            // 그 밖의 `--키=값`·`--키`는 설정 덮어쓰기(Ghostty `loadCliArgs`). 모르는 키는 설정이 경고로 적는다
            v if v.starts_with("--") && v.len() > 2 => {
                let (k, val) = v[2..].split_once('=').unwrap_or((&v[2..], "true"));
                cli.settings.push((k.to_string(), val.to_string()));
            }
            // `--`로 시작하지 않는 인자(Cocoa의 `-AppleInterfaceStyle Dark` 등)는 적어 두고 넘어간다(Ghostty "invalid field", #743)
            other => cli
                .warnings
                .push(format!("명령줄: 설정 키가 아닌 인자라 무시했습니다: {other}")),
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

/// 앱을 띄운 때. 창의 첫 프레임 로그가 이때부터 잰다(실험 E10)
pub(crate) static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn main() -> anyhow::Result<()> {
    STARTED.get_or_init(std::time::Instant::now);
    // `+동작`(Ghostty의 `ghostty +동작`)은 창 없이 돌고 끝난다. 출력에 로그가 섞이지 않게 로거보다 먼저
    let argv: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(target_os = "macos")]
    let launch_args = argv.clone();
    if let Some(code) = cliact::try_run(&argv) {
        std::process::exit(code);
    }
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    // 입력 → 화면 지연(#417): debug 로그가 켜졌을 때만 칸이 시각을 적는다
    bt_term::latency::set_enabled(log::log_enabled!(log::Level::Debug));
    let cli = match parse_args(argv.into_iter()) {
        Ok(o) => o,
        Err(msg) => {
            println!("{msg}");
            return Ok(());
        }
    };
    let mut config = bt_core::Config::load_full(
        cli.config_file.as_deref(),
        system_is_dark(),
        cli.default_files,
        &cli.settings,
    );
    config.warnings.splice(0..0, cli.warnings.iter().cloned());
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
    // `-e`로 띄우면 Ghostty처럼 명령이 끝날 때 앱도 끝난다(quit-after-last-window-closed를 켜고 delay를 지움)
    if cli.first.program.is_some() {
        config.apply_cli_command();
    }
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut builder = EventLoop::<app::UserEvent>::with_user_event();
    // Finder·`open`으로 뜨면 활성화는 macOS에 맡긴다(`open -g`가 뒤에 남게, #717). winit 기본은 언제나 스스로 활성화
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::EventLoopBuilderExtMacOS;
        builder.with_activate_ignoring_other_apps(lifecycle::activate_on_launch(std::os::unix::process::parent_id()));
    }
    let event_loop = builder.build()?;
    // Dock·Finder가 열라고 한 경로를 받는다(#279). 명령줄 인자로 온 경로가 되돌아온 것은 거른다
    #[cfg(target_os = "macos")]
    {
        let proxy = event_loop.create_proxy();
        let started = std::time::Instant::now();
        let cwd = std::env::current_dir().unwrap_or_default();
        let filter = std::sync::Mutex::new(dock_open::ArgFilter::new(&launch_args, &cwd, |p| p.exists()));
        winit::platform::macos::set_open_file_handler(Box::new(move |path| {
            let ignore = filter
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .should_ignore(&path, started.elapsed());
            if ignore {
                log::debug!("명령줄 인자로 온 경로의 열기 요청은 무시합니다: {path:?}");
                return;
            }
            let _ = proxy.send_event(app::UserEvent::OpenFile(path));
        }));
        // Finder 서비스 "New b-terminal Tab/Window Here"(#631). 앱 묶음이 아니면 달지 않는다
        let proxy = event_loop.create_proxy();
        winit::platform::macos::set_services_handler(Box::new(move |target, paths| {
            let want_tab = target == winit::platform::macos::ServiceTarget::Tab;
            let _ = proxy.send_event(app::UserEvent::OpenFolders(want_tab, paths));
        }));
        // 칸이 낸 데스크톱 알림을 누르면 그 칸으로 간다(#625). 앱 묶음이 아니면 처리 함수만 둔다(알림은 osascript로 나감)
        let proxy = event_loop.create_proxy();
        winit::platform::macos::set_user_notification_handler(Box::new(move |r| {
            if !r.clicked {
                return;
            }
            match desktop::pane_of_note(&r.id) {
                Some(pane) => {
                    let _ = proxy.send_event(app::UserEvent::NoteClicked(pane));
                }
                None => log::debug!("이 실행이 낸 칸 알림이 아닙니다: {:?}", r.id),
            }
        }));
    }
    // AppleScript 사전(#634). 앱 묶음이 아니면 처리 함수만 둔다(사전을 읽을 묶음이 없다)
    #[cfg(target_os = "macos")]
    app::install_scripting(event_loop.create_proxy(), config.macos_applescript);
    let mut app = app::App::new(
        config,
        app::ConfigArgs {
            extra: cli.config_file.clone(),
            default_files: cli.default_files,
            settings: cli.settings.clone(),
        },
        event_loop.create_proxy(),
        cli.first,
    );
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
    fn unknown_args_warn_and_dash_dash_key_value_sets_config_like_ghostty() {
        // `open --args -AppleInterfaceStyle Dark`처럼 Cocoa 인자가 와도 앱은 뜬다(Ghostty `cli/args.zig`의 "invalid field" 진단, #743)
        let args = [
            "-AppleInterfaceStyle",
            "Dark",
            "--font-size=20",
            "--window-vsync",
            "--nope=1",
        ];
        let cli = parse_args(args.iter().map(|s| s.to_string())).expect("모르는 인자로 끝나지 않는다");
        let kv = |k: &str, v: &str| (k.to_string(), v.to_string());
        assert_eq!(
            cli.settings,
            vec![kv("font-size", "20"), kv("window-vsync", "true"), kv("nope", "1")]
        );
        assert_eq!(cli.warnings.len(), 2, "{:?}", cli.warnings);
        assert!(cli.warnings[0].contains("-AppleInterfaceStyle"));
        // Ghostty의 `--config-file=경로` 꼴도 받는다
        let cli = parse_args(["--config-file=/tmp/x"].iter().map(|s| s.to_string())).unwrap();
        assert_eq!(cli.config_file.as_deref(), Some(std::path::Path::new("/tmp/x")));
        assert!(cli.settings.is_empty());
    }

    #[test]
    fn config_default_files_is_cli_only_and_defaults_on() {
        let cli = |v: &[&str]| parse_args(v.iter().map(|s| s.to_string()));
        assert!(cli(&[]).unwrap().default_files);
        assert!(!cli(&["--config-default-files=false"]).unwrap().default_files);
        assert!(cli(&["--config-default-files=true"]).unwrap().default_files);
        // 값 없이 쓰면 켬(Ghostty의 bool 인자)
        assert!(cli(&["--config-default-files"]).unwrap().default_files);
        // 나중 것이 이긴다
        assert!(
            cli(&["--config-default-files=false", "--config-default-files=true"])
                .unwrap()
                .default_files
        );
        assert!(cli(&["--config-default-files=maybe"]).is_err());
        // --config-file은 그대로 읽힌다
        let c = cli(&["--config-default-files=false", "--config-file", "/tmp/x"]).unwrap();
        assert!(!c.default_files);
        assert_eq!(c.config_file.as_deref(), Some(std::path::Path::new("/tmp/x")));
        assert!(USAGE.contains("--config-default-files"));
        assert!(USAGE.contains("--<설정 키>=<값>"));
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
