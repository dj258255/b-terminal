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
    (
        "show-config",
        "기본과 다른 설정을 보입니다 (--default, --changes-only=false, --docs)",
    ),
    (
        "list-themes",
        "테마 이름과 자리를 보입니다 (--path, --color=dark|light, --plain)",
    ),
    (
        "validate-config",
        "설정 파일을 검사합니다 (--config-file, --config-default-files)",
    ),
    ("edit-config", "설정 파일을 $VISUAL 또는 $EDITOR로 엽니다"),
];

/// `cliinfo`가 맡는 동작(도움말에만 보인다. 실행은 `run_unknown`이 넘긴다)
const INFO_ACTIONS: &[(&str, &str)] = &[
    ("list-keybinds", "키 묶음을 보입니다 (--default)"),
    ("list-actions", "키 묶음에 쓸 수 있는 동작 이름을 보입니다"),
    ("list-fonts", "글꼴 집안과 모양을 보입니다 (--family, --bold, --italic)"),
    ("show-face", "글자마다 그릴 글꼴을 보입니다 (--string, --cp)"),
    ("list-colors", "X11 색 이름과 값을 보입니다"),
    (
        "explain-config",
        "설정 키의 값·기본·설명을 보입니다 (<키>, --option, --keybind)",
    ),
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
fn run_unknown(name: &str, args: &[String], io: &mut Io) -> i32 {
    if let Some(code) = crate::cliinfo::run(name, args) {
        return code;
    }
    let _ = writeln!(io.err, "모르는 동작: +{name}\n동작 목록은 `b-terminal +help`로 봅니다.");
    1
}

fn help(io: &mut Io) -> i32 {
    let _ = writeln!(io.out, "사용: b-terminal [+동작] [옵션]\n");
    let _ = writeln!(
        io.out,
        "+동작 없이 부르면 터미널 창을 엽니다(`b-terminal --help`).\n\n쓸 수 있는 동작:"
    );
    for (n, d) in ACTIONS.iter().chain(INFO_ACTIONS) {
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
    /// `+show-config --changes-only`(Ghostty 기본 true)
    changes_only: bool,
    /// `+show-config --docs`
    docs: bool,
}

/// `--이름`, `--이름=true`, `--이름=false`를 읽는다. 이 인자가 아니면 `None`
fn bool_arg(a: &str, name: &str) -> Option<Result<bool, String>> {
    if a == format!("--{name}") {
        return Some(Ok(true));
    }
    let v = a.strip_prefix(&format!("--{name}="))?;
    Some(match v {
        "true" => Ok(true),
        "false" => Ok(false),
        x => Err(format!("--{name}는 true나 false여야 합니다: {x}")),
    })
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

/// `키 = 값` 글을 키별 줄 묶음으로 나눈다(처음 나온 순서). 주석·빈 줄은 건너뛴다
fn group_by_key(text: &str) -> Vec<(&str, Vec<&str>)> {
    let mut out: Vec<(&str, Vec<&str>)> = Vec::new();
    for l in text.lines() {
        let Some((k, _)) = l.split_once(" = ") else { continue };
        match out.iter_mut().find(|(key, _)| *key == k) {
            Some((_, v)) => v.push(l),
            None => out.push((k, vec![l])),
        }
    }
    out
}

/// `+show-config --changes-only`: Ghostty `formatter_file.zig`처럼 키 단위로 기본과 견주어,
/// 키의 줄 묶음이 기본과 다르면 그 키의 줄을 모두 남긴다(Ghostty가 `Config.changed`로 값 전체를 견주는 것과 같다).
/// 기본에는 있는데 지금 없는 키(지운 것)는 `키 = `(비움)으로 드러낸다
fn changed_keys(current: &str, default: &str) -> String {
    let cur = group_by_key(current);
    let def = group_by_key(default);
    let mut out: Vec<String> = Vec::new();
    for (k, lines) in &cur {
        if def.iter().find(|(dk, _)| dk == k).map(|(_, d)| d) != Some(lines) {
            out.extend(lines.iter().map(|l| (*l).to_string()));
        }
    }
    for (k, _) in &def {
        if !cur.iter().any(|(ck, _)| ck == k) {
            out.push(format!("{k} = "));
        }
    }
    out.join("\n")
}

fn show_config(args: &[String], env: &Env, io: &mut Io) -> i32 {
    let mut la = LoadArgs {
        default_files: true,
        changes_only: true,
        ..LoadArgs::default()
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let parsed = match take_load_arg(a, &mut it, &mut la) {
            Ok(true) => Ok(()),
            Ok(false) => {
                if let Some(r) = bool_arg(a, "default") {
                    r.map(|v| la.defaults_only = v)
                } else if let Some(r) = bool_arg(a, "changes-only") {
                    r.map(|v| la.changes_only = v)
                } else if let Some(r) = bool_arg(a, "docs") {
                    r.map(|v| la.docs = v)
                } else {
                    return unsupported("show-config", a, io);
                }
            }
            Err(e) => Err(e),
        };
        if let Err(e) = parsed {
            let _ = writeln!(io.err, "+show-config: {e}");
            return 1;
        }
    }
    let default_text = bt_core::Config::default().render();
    let (mut text, config) = if la.defaults_only {
        (default_text, None)
    } else {
        let config = bt_core::Config::load_with(la.config_file.as_deref(), env.dark, la.default_files);
        let mut text = config.render();
        // Ghostty처럼 키마다 기본과 견주어 바뀐 키만 남긴다(`--default`에는 효과가 없다)
        if la.changes_only {
            text = changed_keys(&text, &default_text);
        }
        (text, Some(config))
    };
    if let Some(config) = &config {
        for src in &config.sources {
            let _ = writeln!(io.out, "# 읽은 파일: {}", src.display());
        }
        for w in &config.warnings {
            let _ = writeln!(io.out, "# 경고: {w}");
        }
    }
    if la.docs {
        text = crate::cliinfo::annotate_with_docs(&text);
    }
    if !text.is_empty() {
        let _ = writeln!(io.out, "{text}");
    }
    0
}

/// 테마 배경색의 밝기(Ghostty `list-themes`와 같은 식: 0.2126 R + 0.7152 G + 0.0722 B, 0~1). 0.5보다 작으면 어둡다.
/// 배경 줄이 없거나 `#rrggbb`가 아니면 읽지 못한 것이라 `None`
fn background_luminance(theme_text: &str) -> Option<f64> {
    let mut found = None;
    for l in theme_text.lines() {
        let Some((k, v)) = l.split_once('=') else { continue };
        if k.trim() != "background" {
            continue;
        }
        let h = v.trim().trim_start_matches('#');
        if h.len() == 6 && h.chars().all(|c| c.is_ascii_hexdigit()) {
            let c = |i: usize| f64::from(u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0)) / 255.0;
            found = Some(0.2126 * c(0) + 0.7152 * c(2) + 0.0722 * c(4));
        }
    }
    found
}

fn list_themes(args: &[String], env: &Env, io: &mut Io) -> i32 {
    let (mut path, mut color) = (false, "all".to_string());
    for a in args {
        if let Some(r) = bool_arg(a, "path") {
            match r {
                Ok(v) => path = v,
                Err(e) => {
                    let _ = writeln!(io.err, "+list-themes: {e}");
                    return 1;
                }
            }
        } else if let Some(v) = a.strip_prefix("--color=") {
            if !matches!(v, "all" | "dark" | "light") {
                let _ = writeln!(
                    io.err,
                    "+list-themes: --color는 all, dark, light 중 하나여야 합니다: {v}"
                );
                return 1;
            }
            color = v.to_string();
        } else if a != "--plain" {
            return unsupported("list-themes", a, io);
        }
    }
    // (이름, 자리, 파일 경로(넣어 둔 테마는 없다)). 같은 이름이면 사용자 것이 이긴다(설정이 테마를 찾는 순서와 같다)
    let mut themes: Vec<(String, &str, Option<PathBuf>)> = Vec::new();
    for dir in &env.theme_dirs {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with('.') && e.path().is_file() && !themes.iter().any(|(n, ..)| *n == name) {
                themes.push((name, "user", Some(e.path())));
            }
        }
    }
    for n in bt_core::config::bundled_theme_names() {
        if !themes.iter().any(|(m, ..)| m == n) {
            themes.push((n.to_string(), "resources", None));
        }
    }
    themes.sort_by_key(|(n, ..)| n.to_lowercase());
    for (n, loc, file) in themes {
        if color != "all" {
            let text = match &file {
                Some(p) => std::fs::read_to_string(p).unwrap_or_default(),
                None => bt_core::config::bundled_theme_text(&n).unwrap_or_default().to_string(),
            };
            // 배경을 읽지 못한 테마는 기본 배경(어두움)으로 친다
            let dark = background_luminance(&text).is_none_or(|l| l < 0.5);
            if dark != (color == "dark") {
                continue;
            }
        }
        match (&file, path) {
            (Some(p), true) => {
                let _ = writeln!(io.out, "{n} ({loc}) {}", p.display());
            }
            _ => {
                let _ = writeln!(io.out, "{n} ({loc})");
            }
        }
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
    fn info_actions_are_reached_and_listed() {
        let d = tmpdir("info");
        // cliinfo의 동작은 run_unknown을 거쳐 그쪽에서 돈다(모르는 동작의 1이 아니다)
        let (code, _, err, _) = go("list-actions", &[], &env(&d));
        assert_eq!(code, 0, "{err}");
        assert!(!err.contains("모르는 동작"), "{err}");
        let (code, _, _, _) = go("help", &[], &env(&d));
        assert_eq!(code, 0);
        let (_, out, _, _) = go("help", &[], &env(&d));
        for (n, _) in INFO_ACTIONS {
            assert!(out.contains(&format!("+{n}")), "{n}: {out}");
        }
        // 정말 모르는 이름은 그대로 1
        let (code, _, err, _) = go("nope", &[], &env(&d));
        assert_eq!(code, 1);
        assert!(err.contains("모르는 동작"), "{err}");
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
    }

    fn show(d: &Path, extra: &[&str]) -> (i32, String, String) {
        let f = d.join("c");
        std::fs::write(&f, "font-size = 23\n").unwrap();
        let mut a = vec!["--config-default-files=false", "--config-file", f.to_str().unwrap()];
        a.extend_from_slice(extra);
        let (code, out, err, _) = go("show-config", &a, &env(d));
        (code, out, err)
    }

    #[test]
    fn show_config_changes_only_is_the_default_and_can_be_turned_off() {
        let d = tmpdir("changes");
        // 기본(Ghostty처럼 true): 기본과 다른 키만
        let (code, out, err) = show(&d, &[]);
        assert_eq!(code, 0, "{err}");
        let keys: Vec<&str> = out.lines().filter(|l| !l.starts_with('#')).collect();
        assert_eq!(keys, ["font-size = 23"], "{out}");
        assert_eq!(show(&d, &["--changes-only"]).1, out);
        assert_eq!(show(&d, &["--changes-only=true"]).1, out);
        // false면 전부
        let (code, all, _) = show(&d, &["--changes-only=false"]);
        assert_eq!(code, 0);
        assert!(all.contains("font-size = 23") && all.contains("background = "), "{all}");
        assert!(all.lines().count() > 20);
        // --default면 효과가 없다(기본값 전체)
        let (_, def, _) = show(&d, &["--default"]);
        assert!(
            def.contains("background = ") && !def.contains("font-size = 23"),
            "{def}"
        );
        // 잘못된 값
        assert_eq!(show(&d, &["--changes-only=maybe"]).0, 1);
    }

    #[test]
    fn changed_keys_compares_whole_keys_and_shows_cleared_ones() {
        let def = "a = 1\nb = 2\nb = 3\nc = 4\nd = 5";
        // 줄 비교로는 `b = 2`가 기본에 있어 숨지만, 키의 묶음이 다르므로 키의 줄을 모두 남긴다
        let cur = "a = 1\nb = 2\nc = 9";
        assert_eq!(changed_keys(cur, def), "b = 2\nc = 9\nd = ");
        // 같으면 아무것도 없다
        assert_eq!(changed_keys(def, def), "");
        // 기본에 없는 키는 바뀐 키다
        assert_eq!(changed_keys("a = 1\ne = 7\nb = 2\nb = 3\nc = 4\nd = 5", def), "e = 7");
    }

    #[test]
    fn show_config_prints_theme_line_and_the_colors_it_sets() {
        let d = tmpdir("theme251");
        let theme = d.join("MyLight");
        std::fs::write(&theme, "background = #fafafa\nforeground = #111111\n").unwrap();
        let f = d.join("c");
        std::fs::write(&f, format!("font-size = 15\ntheme = {}\n", theme.display())).unwrap();
        let run = |extra: &[&str]| {
            let mut a = vec!["--config-default-files=false", "--config-file", f.to_str().unwrap()];
            a.extend_from_slice(extra);
            go("show-config", &a, &env(&d)).1
        };
        let want = [
            "font-size = 15".to_string(),
            format!("theme = {}", theme.display()),
            "background = #fafafa".to_string(),
            "foreground = #111111".to_string(),
        ];
        let out = run(&[]);
        let keys: Vec<&str> = out.lines().filter(|l| !l.starts_with('#')).collect();
        let mut sorted: Vec<&str> = want.iter().map(String::as_str).collect();
        sorted.sort_unstable();
        let mut got = keys.clone();
        got.sort_unstable();
        assert_eq!(got, sorted, "{out}");
        assert!(run(&["--changes-only=false"]).contains(&want[1]));
    }

    #[test]
    fn show_config_docs_puts_the_table_row_above_each_key() {
        let d = tmpdir("docs");
        let (code, out, err) = show(&d, &["--docs"]);
        assert_eq!(code, 0, "{err}");
        let lines: Vec<&str> = out.lines().collect();
        let at = lines.iter().position(|l| *l == "font-size = 23").expect(&out);
        assert!(
            lines[..at].iter().rev().take(3).any(|l| l.starts_with("# 값:")),
            "{out}"
        );
        assert!(
            lines[..at].iter().rev().take(3).any(|l| l.starts_with("# 기본:")),
            "{out}"
        );
        assert!(
            lines[..at]
                .iter()
                .rev()
                .take(3)
                .all(|l| l.is_empty() || l.starts_with("# ")),
            "{out}"
        );
        // 설명 줄은 모두 주석이라 다시 읽어도 같은 설정이다
        assert!(
            out.lines()
                .all(|l| l.is_empty() || l.starts_with("# ") || l.contains(" = ")),
            "{out}"
        );
        // 같은 키가 여러 번 나와도 설명은 한 번
        let (_, all, _) = show(&d, &["--changes-only=false", "--docs"]);
        assert!(all.matches("# 기본:").count() > 5, "{all}");
        assert_eq!(all.lines().filter(|l| *l == "# font-size").count(), 1, "{all}");
        // --docs=false는 설명이 없다
        assert!(!show(&d, &["--docs=false"]).1.contains("# 값:"));
    }

    #[test]
    fn list_themes_path_and_color() {
        let d = tmpdir("themes2");
        std::fs::create_dir_all(d.join("themes")).unwrap();
        std::fs::write(d.join("themes/Zlight"), "background = #ffffff\n").unwrap();
        std::fs::write(d.join("themes/Zdark"), "background = #101010\n").unwrap();
        let e = env(&d);
        let (code, out, _, _) = go("list-themes", &["--path"], &e);
        assert_eq!(code, 0);
        let p = d.join("themes/Zlight");
        assert!(
            out.lines().any(|l| l == format!("Zlight (user) {}", p.display())),
            "{}",
            &out[out.len().saturating_sub(300)..]
        );
        // 넣어 둔 테마는 파일이 없어 경로를 붙이지 않는다
        let first = bt_core::config::bundled_theme_names().next().unwrap();
        assert!(out.lines().any(|l| l == format!("{first} (resources)")));
        let names = |a: &[&str]| -> Vec<String> { go("list-themes", a, &e).1.lines().map(|l| l.to_string()).collect() };
        let (all, dark, light) = (
            names(&["--color=all"]),
            names(&["--color=dark"]),
            names(&["--color=light"]),
        );
        assert_eq!(all, names(&[]));
        assert!(light.iter().any(|l| l.starts_with("Zlight (")) && !light.iter().any(|l| l.starts_with("Zdark (")));
        assert!(dark.iter().any(|l| l.starts_with("Zdark (")) && !dark.iter().any(|l| l.starts_with("Zlight (")));
        assert_eq!(
            dark.len() + light.len(),
            all.len(),
            "어두운 것과 밝은 것이 전부를 나눈다"
        );
        assert!(!dark.is_empty() && !light.is_empty());
        assert_eq!(go("list-themes", &["--color=x"], &e).0, 1);
        assert_eq!(go("list-themes", &["--path", "--color=dark", "--plain"], &e).0, 0);
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
        assert_eq!(go("list-themes", &["--bogus"], &env(&d)).0, 1);
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
