//! 명령줄 동작 `b-terminal +동작 [인자...]` (Ghostty의 `ghostty +동작`)
//!
//! 이 파일은 `+동작`을 찾고(`detect`), 이름으로 실행하고(`run`), 이 모듈의 여섯 동작을 둔다:
//! `+version` `+help` `+show-config` `+list-themes` `+validate-config` `+edit-config`.
//! 이름을 모르면 `run_unknown` 한 곳으로 간다. 다른 모듈의 동작은 거기서 잇는다.
//! 창을 띄우지 않고, 끝나면 종료 코드를 돌려준다.

use std::io::Write;
use std::path::{Path, PathBuf};

/// 출력 두 줄기(테스트에서 바꿔 끼운다)
pub struct Io<'a> {
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
}

/// 바깥 환경(테스트에서 주입한다. 실제 홈과 환경 변수를 건드리지 않게)
pub struct Env {
    /// `+edit-config`가 열 파일
    pub edit_path: PathBuf,
    pub visual: Option<String>,
    pub editor: Option<String>,
    /// 시스템이 어두운 모드인가(`theme = light:…,dark:…`에서 고를 쪽)
    pub dark: bool,
    /// 사용자 테마 폴더
    pub theme_dirs: Vec<PathBuf>,
}

impl Env {
    pub fn from_process() -> Env {
        Env {
            edit_path: bt_core::config::bterminal_config_path(),
            visual: std::env::var("VISUAL").ok(),
            editor: std::env::var("EDITOR").ok(),
            dark: crate::system_is_dark(),
            theme_dirs: bt_core::config::theme_dirs(),
        }
    }
}

/// 이 모듈이 아는 동작과 한 줄 설명(`+help`가 보인다)
const ACTIONS: &[(&str, &str)] = &[
    ("version", "버전을 보입니다"),
    ("help", "이 도움말을 보입니다"),
    ("show-config", "지금 적용된 설정을 보입니다 (--default: 기본값)"),
    ("list-themes", "테마 이름과 자리를 보입니다 (--plain)"),
    (
        "validate-config",
        "설정 파일을 검사합니다 (--config-file, --config-default-files)",
    ),
    ("edit-config", "설정 파일을 $VISUAL 또는 $EDITOR로 엽니다"),
];

/// 인자에서 `+동작`을 찾는다. Ghostty처럼 자리는 가리지 않고(`-e` 뒤는 프로그램의 인자라 보지 않는다),
/// 둘 이상이면 오류다. 돌려주는 것은 (이름, 동작 인자들: `+동작`만 뺀 나머지)
pub fn detect(args: &[String]) -> Result<Option<(String, Vec<String>)>, String> {
    let mut action: Option<String> = None;
    let mut rest = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            // 뒤는 프로그램과 그 인자
            "-e" => break,
            // 값을 받는 인자: 값이 `+`로 시작해도 동작이 아니다
            "--cwd" | "--config-file" => {
                rest.push(a.clone());
                rest.extend(it.next().cloned());
            }
            _ => match a.strip_prefix('+') {
                Some(name) => {
                    if let Some(first) = &action {
                        return Err(format!("동작은 하나만 줄 수 있습니다: +{first}, +{name}"));
                    }
                    action = Some(name.to_string());
                }
                None => rest.push(a.clone()),
            },
        }
    }
    Ok(action.map(|n| (n, rest)))
}

/// 동작 하나를 돌리고 종료 코드를 돌려준다. `launch`는 편집기를 띄우는 함수(인자 배열, 종료 코드)
pub fn run(
    name: &str,
    args: &[String],
    env: &Env,
    io: &mut Io,
    launch: &mut dyn FnMut(&[String]) -> std::io::Result<i32>,
) -> i32 {
    if ACTIONS.iter().any(|(n, _)| *n == name) && args.iter().any(|a| a == "-h" || a == "--help") {
        return action_help(name, io);
    }
    match name {
        "version" => {
            let _ = writeln!(io.out, "b-terminal {}", env!("CARGO_PKG_VERSION"));
            0
        }
        "help" => help(io),
        "show-config" => show_config(args, env, io),
        "list-themes" => list_themes(args, env, io),
        "validate-config" => validate_config(args, env, io),
        "edit-config" => edit_config(args, env, io, launch),
        _ => run_unknown(name, args, io),
    }
}

/// 이 모듈이 모르는 동작 이름이 오는 단 한 곳. 다른 모듈의 동작은 여기서 이어 받는다
fn run_unknown(name: &str, _args: &[String], io: &mut Io) -> i32 {
    let _ = writeln!(io.err, "모르는 동작: +{name}\n동작 목록은 `b-terminal +help`로 봅니다.");
    1
}

fn help(io: &mut Io) -> i32 {
    let _ = writeln!(io.out, "사용: b-terminal [+동작] [옵션]\n");
    let _ = writeln!(
        io.out,
        "+동작 없이 부르면 터미널 창을 엽니다(`b-terminal --help`).\n\n쓸 수 있는 동작:"
    );
    for (n, d) in ACTIONS {
        let _ = writeln!(io.out, "  +{n:<16} {d}");
    }
    let _ = writeln!(io.out, "\n동작마다 `+<동작> --help`로 설명을 봅니다.");
    0
}

fn action_help(name: &str, io: &mut Io) -> i32 {
    let d = ACTIONS.iter().find(|(n, _)| *n == name).map_or("", |(_, d)| *d);
    let _ = writeln!(io.out, "b-terminal +{name}: {d}");
    0
}

fn unsupported(action: &str, opt: &str, io: &mut Io) -> i32 {
    let _ = writeln!(
        io.err,
        "+{action}: 지원하지 않는 인자: {opt}\n`b-terminal +{action} --help`를 보세요."
    );
    1
}

/// 설정을 읽는 동작이 함께 받는 인자(`--config-file`, `--config-default-files`)
#[derive(Default)]
struct LoadArgs {
    config_file: Option<PathBuf>,
    default_files: bool,
    /// `+show-config --default`
    defaults_only: bool,
}

/// 인자 하나를 `LoadArgs`에 읽는다. 읽었으면 `Ok(true)`, 이 인자가 아니면 `Ok(false)`
fn take_load_arg(a: &str, it: &mut std::slice::Iter<String>, out: &mut LoadArgs) -> Result<bool, String> {
    match a {
        "--config-file" => out.config_file = Some(it.next().ok_or("--config-file 뒤에 파일이 필요합니다")?.into()),
        "--config-default-files" => out.default_files = true,
        v if v.starts_with("--config-default-files=") => {
            out.default_files = match &v["--config-default-files=".len()..] {
                "true" => true,
                "false" => false,
                x => return Err(format!("--config-default-files는 true나 false여야 합니다: {x}")),
            }
        }
        _ => return Ok(false),
    }
    Ok(true)
}

fn show_config(args: &[String], env: &Env, io: &mut Io) -> i32 {
    let mut la = LoadArgs {
        default_files: true,
        ..LoadArgs::default()
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match take_load_arg(a, &mut it, &mut la) {
            Ok(true) => {}
            Ok(false) if a == "--default" => la.defaults_only = true,
            Ok(false) => return unsupported("show-config", a, io),
            Err(e) => {
                let _ = writeln!(io.err, "+show-config: {e}");
                return 1;
            }
        }
    }
    if la.defaults_only {
        let _ = writeln!(io.out, "{}", bt_core::Config::default().render());
        return 0;
    }
    let config = bt_core::Config::load_with(la.config_file.as_deref(), env.dark, la.default_files);
    for src in &config.sources {
        let _ = writeln!(io.out, "# 읽은 파일: {}", src.display());
    }
    for w in &config.warnings {
        let _ = writeln!(io.out, "# 경고: {w}");
    }
    let _ = writeln!(io.out, "{}", config.render());
    0
}

fn list_themes(args: &[String], env: &Env, io: &mut Io) -> i32 {
    for a in args {
        if a != "--plain" {
            return unsupported("list-themes", a, io);
        }
    }
    // (이름, 자리). 같은 이름이면 사용자 것이 이긴다(설정이 테마를 찾는 순서와 같다)
    let mut themes: Vec<(String, &str)> = Vec::new();
    for dir in &env.theme_dirs {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with('.') && e.path().is_file() && !themes.iter().any(|(n, _)| *n == name) {
                themes.push((name, "user"));
            }
        }
    }
    for n in bt_core::config::bundled_theme_names() {
        if !themes.iter().any(|(m, _)| m == n) {
            themes.push((n.to_string(), "resources"));
        }
    }
    themes.sort_by_key(|(n, _)| n.to_lowercase());
    for (n, loc) in themes {
        let _ = writeln!(io.out, "{n} ({loc})");
    }
    0
}

fn validate_config(args: &[String], env: &Env, io: &mut Io) -> i32 {
    let mut la = LoadArgs {
        default_files: true,
        ..LoadArgs::default()
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match take_load_arg(a, &mut it, &mut la) {
            Ok(true) => {}
            Ok(false) => return unsupported("validate-config", a, io),
            Err(e) => {
                let _ = writeln!(io.err, "+validate-config: {e}");
                return 1;
            }
        }
    }
    // 읽는 쪽은 없는 파일을 조용히 넘기지만, 검사하라고 준 파일이 없으면 오류다(Ghostty도 오류로 끝낸다)
    if let Some(f) = &la.config_file {
        if !f.is_file() {
            let _ = writeln!(
                io.err,
                "+validate-config: 설정 파일을 읽을 수 없습니다: {}",
                f.display()
            );
            return 1;
        }
    }
    let config = bt_core::Config::load_with(la.config_file.as_deref(), env.dark, la.default_files);
    for w in &config.warnings {
        let _ = writeln!(io.out, "{w}");
    }
    i32::from(!config.warnings.is_empty())
}

fn edit_config(
    args: &[String],
    env: &Env,
    io: &mut Io,
    launch: &mut dyn FnMut(&[String]) -> std::io::Result<i32>,
) -> i32 {
    if let Some(a) = args.first() {
        return unsupported("edit-config", a, io);
    }
    let argv = match edit_command(env.visual.as_deref(), env.editor.as_deref(), &env.edit_path) {
        Ok(v) => v,
        Err(e) => {
            let _ = writeln!(io.err, "{e}\n설정 파일: {}", env.edit_path.display());
            return 1;
        }
    };
    if let Err(e) = ensure_config_file(&env.edit_path) {
        let _ = writeln!(
            io.err,
            "설정 파일을 만들지 못했습니다: {}: {e}",
            env.edit_path.display()
        );
        return 1;
    }
    match launch(&argv) {
        Ok(code) => code,
        Err(e) => {
            let _ = writeln!(
                io.err,
                "편집기를 실행하지 못했습니다: {e}\n명령: {}\n설정 파일: {}",
                argv.join(" "),
                env.edit_path.display()
            );
            1
        }
    }
}

/// 프로세스의 명령줄에 `+동작`이 있으면 돌리고 종료 코드를 돌려준다. 없으면 `None`(창 앱으로 간다)
pub fn try_run(args: &[String]) -> Option<i32> {
    let (name, rest) = match detect(args) {
        Ok(Some(x)) => x,
        Ok(None) => return None,
        Err(e) => {
            eprintln!("{e}");
            return Some(1);
        }
    };
    let (stdout, stderr) = (std::io::stdout(), std::io::stderr());
    let (mut out, mut err) = (stdout.lock(), stderr.lock());
    let code = run(
        &name,
        &rest,
        &Env::from_process(),
        &mut Io {
            out: &mut out,
            err: &mut err,
        },
        // 셸을 거치지 않고 인자 배열로 띄운다
        &mut |argv| {
            let status = std::process::Command::new(&argv[0]).args(&argv[1..]).status()?;
            Ok(status.code().unwrap_or(1))
        },
    );
    let _ = out.flush();
    Some(code)
}

/// 편집기 값을 인자 배열로 가른다(셸을 거치지 않는다). 빈칸으로 가르고 따옴표와 `\`를 받는다
pub fn split_command(s: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_tok = false;
    let mut quote: Option<char> = None;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('"'), '\\') => match chars.next() {
                Some(n @ ('"' | '\\')) => cur.push(n),
                Some(n) => {
                    cur.push('\\');
                    cur.push(n);
                }
                None => return Err("편집기 값이 \\로 끝납니다".into()),
            },
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => {
                quote = Some(c);
                in_tok = true;
            }
            (None, '\\') => {
                cur.push(chars.next().ok_or("편집기 값이 \\로 끝납니다")?);
                in_tok = true;
            }
            (None, c) if c.is_whitespace() => {
                if in_tok {
                    out.push(std::mem::take(&mut cur));
                    in_tok = false;
                }
            }
            (None, c) => {
                cur.push(c);
                in_tok = true;
            }
        }
    }
    if quote.is_some() {
        return Err("편집기 값의 따옴표가 닫히지 않았습니다".into());
    }
    if in_tok {
        out.push(cur);
    }
    Ok(out)
}

/// `+edit-config`가 띄울 명령: `$VISUAL`을 먼저 보고 비었으면 `$EDITOR`, 끝에 파일을 붙인다
pub fn edit_command(visual: Option<&str>, editor: Option<&str>, path: &Path) -> Result<Vec<String>, String> {
    fn pick(v: Option<&str>) -> Option<&str> {
        v.filter(|s| !s.trim().is_empty())
    }
    let Some(cmd) = pick(visual).or_else(|| pick(editor)) else {
        return Err("$VISUAL이나 $EDITOR 환경 변수가 없거나 비어 있습니다.".into());
    };
    let mut argv = split_command(cmd)?;
    argv.push(path.to_string_lossy().into_owned());
    Ok(argv)
}

/// 설정 파일이 없으면 만든다(폴더까지). 이미 있으면 건드리지 않는다. 만들었으면 `true`
pub fn ensure_config_file(path: &Path) -> std::io::Result<bool> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    match std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut f) => {
            f.write_all("# b-terminal 설정 (Ghostty와 같은 문법)\n".as_bytes())?;
            Ok(true)
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bt-cliact-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn env(dir: &Path) -> Env {
        Env {
            edit_path: dir.join("home/config"),
            visual: None,
            editor: None,
            dark: true,
            theme_dirs: vec![dir.join("themes")],
        }
    }

    /// (종료 코드, 표준 출력, 표준 오류, 띄운 명령들)
    fn go(name: &str, args: &[&str], env: &Env) -> (i32, String, String, Vec<Vec<String>>) {
        let (mut o, mut e) = (Vec::new(), Vec::new());
        let launched = RefCell::new(Vec::new());
        let code = run(
            name,
            &s(args),
            env,
            &mut Io {
                out: &mut o,
                err: &mut e,
            },
            &mut |argv| {
                launched.borrow_mut().push(argv.to_vec());
                Ok(0)
            },
        );
        (
            code,
            String::from_utf8(o).unwrap(),
            String::from_utf8(e).unwrap(),
            launched.into_inner(),
        )
    }

    #[test]
    fn detect_finds_action_anywhere_but_after_dash_e() {
        assert_eq!(detect(&s(&[])).unwrap(), None);
        assert_eq!(detect(&s(&["--show-config"])).unwrap(), None);
        assert_eq!(
            detect(&s(&["+validate-config", "--config-file", "/x"])).unwrap(),
            Some(("validate-config".into(), s(&["--config-file", "/x"])))
        );
        // Ghostty는 자리를 가리지 않는다
        assert_eq!(
            detect(&s(&["--config-file", "/x", "+validate-config"])).unwrap(),
            Some(("validate-config".into(), s(&["--config-file", "/x"])))
        );
        // -e 뒤는 프로그램의 인자
        assert_eq!(detect(&s(&["-e", "git", "+x"])).unwrap(), None);
        assert!(detect(&s(&["+version", "+help"])).is_err(), "동작이 둘이면 오류");
        // 이름 없는 `+`는 빈 이름의 모르는 동작으로 오류가 난다
        assert_eq!(detect(&s(&["+"])).unwrap(), Some((String::new(), vec![])));
    }

    #[test]
    fn unknown_action_is_an_error_with_code_1() {
        let d = tmpdir("unknown");
        let (code, out, err, _) = go("nope", &[], &env(&d));
        assert_eq!(code, 1);
        assert!(out.is_empty());
        assert!(err.contains("+nope"), "{err}");
        assert!(err.contains("+help"), "{err}");
    }

    #[test]
    fn version_and_help() {
        let d = tmpdir("vh");
        let (code, out, _, _) = go("version", &[], &env(&d));
        assert_eq!(code, 0);
        assert_eq!(out, format!("b-terminal {}\n", env!("CARGO_PKG_VERSION")));
        let (code, out, _, _) = go("help", &[], &env(&d));
        assert_eq!(code, 0);
        for (n, _) in ACTIONS {
            assert!(out.contains(&format!("+{n}")), "{n}: {out}");
        }
        // `+동작 --help`는 그 동작 설명
        let (code, out, _, _) = go("validate-config", &["--help"], &env(&d));
        assert_eq!(code, 0);
        assert!(out.contains("+validate-config"), "{out}");
    }

    #[test]
    fn validate_config_exit_code_follows_warnings() {
        let d = tmpdir("validate");
        let good = d.join("good");
        std::fs::write(&good, "font-size = 15\n# 주석\n").unwrap();
        let (code, out, _, _) = go(
            "validate-config",
            &["--config-default-files=false", "--config-file", good.to_str().unwrap()],
            &env(&d),
        );
        assert_eq!((code, out.as_str()), (0, ""));

        let bad = d.join("bad");
        std::fs::write(&bad, "font-size = 15\nthis line is not a setting\n").unwrap();
        let (code, out, _, _) = go(
            "validate-config",
            &["--config-default-files=false", "--config-file", bad.to_str().unwrap()],
            &env(&d),
        );
        assert_eq!(code, 1);
        assert!(out.contains("bad:2"), "경고에 파일과 줄이 있다: {out}");
        assert_eq!(out.lines().count(), 1, "경고 하나가 한 줄: {out}");

        // 없는 파일은 오류 (조용히 통과하지 않는다)
        let (code, out, err, _) = go(
            "validate-config",
            &[
                "--config-default-files=false",
                "--config-file",
                d.join("nope").to_str().unwrap(),
            ],
            &env(&d),
        );
        assert_eq!(code, 1);
        assert!(out.is_empty());
        assert!(err.contains("nope"), "{err}");

        // 모르는 인자
        let (code, _, err, _) = go("validate-config", &["--bogus"], &env(&d));
        assert_eq!(code, 1);
        assert!(err.contains("--bogus"), "{err}");
        let (code, _, _, _) = go("validate-config", &["--config-file"], &env(&d));
        assert_eq!(code, 1);
    }

    #[test]
    fn show_config_reads_the_given_file_and_default_flag_ignores_it() {
        let d = tmpdir("show");
        let f = d.join("c");
        std::fs::write(&f, "font-size = 23\n").unwrap();
        let a = ["--config-default-files=false", "--config-file", f.to_str().unwrap()];
        let (code, out, _, _) = go("show-config", &a, &env(&d));
        assert_eq!(code, 0);
        assert!(out.contains("font-size = 23"), "{out}");
        let (code, out, _, _) = go("show-config", &["--default"], &env(&d));
        assert_eq!(code, 0);
        assert!(!out.contains("font-size = 23"), "--default는 사용자 설정을 읽지 않는다");
        assert!(out.contains("font-size"), "{out}");
        let (code, _, err, _) = go("show-config", &["--docs"], &env(&d));
        assert_eq!(code, 1);
        assert!(err.contains("--docs"), "{err}");
    }

    #[test]
    fn list_themes_lists_bundled_and_user_themes() {
        let d = tmpdir("themes");
        std::fs::create_dir_all(d.join("themes")).unwrap();
        std::fs::write(d.join("themes/Zzz My Theme"), "background = #000000\n").unwrap();
        let (code, out, _, _) = go("list-themes", &[], &env(&d));
        assert_eq!(code, 0);
        assert!(
            out.lines().any(|l| l == "Zzz My Theme (user)"),
            "{}",
            &out[out.len().saturating_sub(200)..]
        );
        let first = bt_core::config::bundled_theme_names().next().unwrap();
        assert!(out.lines().any(|l| l == format!("{first} (resources)")));
        let names: Vec<String> = out
            .lines()
            .map(|l| l.rsplit_once(" (").unwrap().0.to_lowercase())
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted, "이름순(대소문자 무시)");
        assert_eq!(go("list-themes", &["--plain"], &env(&d)).0, 0);
        assert_eq!(go("list-themes", &["--color=dark"], &env(&d)).0, 1);
    }

    #[test]
    fn split_command_takes_args_without_a_shell() {
        assert_eq!(split_command("vim").unwrap(), ["vim"]);
        assert_eq!(split_command("code -w").unwrap(), ["code", "-w"]);
        assert_eq!(split_command("  a   b ").unwrap(), ["a", "b"]);
        assert_eq!(
            split_command(r#""/Applications/My Editor/ed" --flag 'x y'"#).unwrap(),
            ["/Applications/My Editor/ed", "--flag", "x y"]
        );
        // 셸 문법은 풀지 않는다: 글자 그대로 인자가 된다
        assert_eq!(split_command("vim; rm -rf ~").unwrap(), ["vim;", "rm", "-rf", "~"]);
        assert_eq!(split_command("ed $(id)").unwrap(), ["ed", "$(id)"]);
        assert!(split_command("ed \"unclosed").is_err());
        assert!(split_command("").unwrap().is_empty());
    }

    #[test]
    fn edit_command_prefers_visual_and_skips_empty() {
        let p = Path::new("/tmp/cfg");
        assert_eq!(
            edit_command(Some("a -w"), Some("b"), p).unwrap(),
            ["a", "-w", "/tmp/cfg"]
        );
        assert_eq!(edit_command(Some(""), Some("b"), p).unwrap(), ["b", "/tmp/cfg"]);
        assert_eq!(edit_command(Some("  "), Some("b"), p).unwrap(), ["b", "/tmp/cfg"]);
        assert_eq!(edit_command(None, Some("b"), p).unwrap(), ["b", "/tmp/cfg"]);
        assert!(edit_command(None, None, p).is_err());
        assert!(edit_command(Some(""), Some(""), p).is_err());
    }

    #[test]
    fn edit_config_creates_file_and_launches_without_shell() {
        let d = tmpdir("edit");
        let mut e = env(&d);
        e.visual = Some("code -w".into());
        assert!(!e.edit_path.exists());
        let (code, _, _, launched) = go("edit-config", &[], &e);
        assert_eq!(code, 0);
        assert!(e.edit_path.is_file(), "없으면 만든다");
        assert_eq!(launched, vec![s(&["code", "-w", e.edit_path.to_str().unwrap()])]);

        // 있는 파일은 건드리지 않는다
        std::fs::write(&e.edit_path, "font-size = 9\n").unwrap();
        go("edit-config", &[], &e);
        assert_eq!(std::fs::read_to_string(&e.edit_path).unwrap(), "font-size = 9\n");
        assert!(!ensure_config_file(&e.edit_path).unwrap());

        // 편집기가 없으면 오류 1, 띄우지 않고 파일 자리를 알린다
        let mut e2 = env(&d);
        e2.edit_path = d.join("other/config");
        let (code, _, err, launched) = go("edit-config", &[], &e2);
        assert_eq!(code, 1);
        assert!(launched.is_empty());
        assert!(err.contains("$VISUAL") && err.contains("$EDITOR"), "{err}");
        assert!(err.contains(e2.edit_path.to_str().unwrap()), "{err}");
    }

    #[test]
    fn edit_config_reports_launch_failure_and_passes_exit_code() {
        let d = tmpdir("editfail");
        let mut e = env(&d);
        e.editor = Some("ed".into());
        let (mut o, mut er) = (Vec::new(), Vec::new());
        let code = run(
            "edit-config",
            &[],
            &e,
            &mut Io {
                out: &mut o,
                err: &mut er,
            },
            &mut |_| Err(std::io::Error::new(std::io::ErrorKind::NotFound, "없음")),
        );
        assert_eq!(code, 1);
        assert!(String::from_utf8(er).unwrap().contains("ed"));
        let code = run(
            "edit-config",
            &[],
            &e,
            &mut Io {
                out: &mut o,
                err: &mut Vec::new(),
            },
            &mut |_| Ok(3),
        );
        assert_eq!(code, 3, "편집기의 종료 코드를 따른다");
    }
}
