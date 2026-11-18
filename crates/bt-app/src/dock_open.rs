//! Dock 아이콘에 놓은 폴더·파일 열기(#279). 창과 상관없는 판단은 순수 함수로 뺐다(ADR-026)

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use bt_core::config::{MacDockDropBehavior, MacTitlebarProxyIcon};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// 놓은 것의 종류(파일 시스템에서 본 값)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
    Missing,
}

/// 열지 않는 까닭
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Missing,
    NotAbsolute,
    NotUtf8,
    UnsafeChars,
}

/// 놓은 것으로 할 일
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// 그 폴더에서 새 칸을 띄운다(확인 없음)
    Folder {
        cwd: PathBuf,
    },
    /// 확인을 받은 뒤 부모 폴더에서 새 칸을 띄우고 `input`을 셸에 친다
    File {
        cwd: PathBuf,
        input: String,
        shown: String,
    },
    Refuse(Refusal),
}

/// 새 칸을 어디에 띄우는가
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    NewTab,
    NewWindow,
}

/// 허용 목록 글자인가: 글자·숫자(`char::is_alphanumeric`, 한글·일본어 이름을 막지 않으려고), 빈칸(U+0020),
/// `/ . _ - + , @ % : = ( ) [ ] # ~`
fn is_allowed_char(c: char) -> bool {
    c.is_alphanumeric() || c == ' ' || "/._-+,@%:=()[]#~".contains(c)
}

/// 셸에 친 글이 어떤 흔한 셸(sh·bash·zsh·fish·tcsh·nushell)에서도 작은따옴표 안의 글자 그대로가 되는가.
/// 작은따옴표 규칙이 셸마다 다르다(fish는 `\'`·`\\`를 해석하고 tcsh는 `!`를 펼친다). 그래서 따옴표를 빠져나갈
/// 수 있는 글자(`'`·`\`·`!`·`$`·`` ` ``)와 제어 문자·서식 문자·빈칸이 아닌 공백을 모두 허용 목록 밖으로 둔다
pub fn is_shell_safe(s: &str) -> bool {
    s.chars().all(is_allowed_char)
}

/// 셸에 한 낱말로 넘길 수 있게 작은따옴표로 감싼다. 허용 목록 밖의 글자가 있으면 `None`.
/// 허용 목록에는 작은따옴표가 없으므로 안을 따로 고치지 않는다
pub fn shell_quote(s: &str) -> Option<String> {
    is_shell_safe(s).then(|| format!("'{s}'"))
}

/// 셸에 칠 글: `<따옴표로 감싼 경로>; exit\n`
pub fn file_input(path: &str) -> Option<String> {
    shell_quote(path).map(|q| format!("{q}; exit\n"))
}

/// 확인 막대에 보일 경로: 제어 문자를 지우고, 너무 길면 앞을 줄여 파일 이름이 남게 한다
pub fn shown_path(path: &str) -> String {
    let clean = bt_core::proto::printable(path);
    let n = clean.chars().count();
    if n <= SHOWN_MAX {
        return clean;
    }
    let tail: String = clean.chars().skip(n - SHOWN_MAX).collect();
    format!("…{tail}")
}

/// 확인 막대에 보일 경로의 최대 글자 수
const SHOWN_MAX: usize = 200;

/// 제목 막대 폴더 아이콘(`representedURL`)이 가리킬 폴더: 보이게 두었고 작업 폴더를 알 때만. 모르면 없앤다(#279)
pub fn proxy_icon_path(setting: MacTitlebarProxyIcon, cwd: Option<&Path>) -> Option<PathBuf> {
    match setting {
        MacTitlebarProxyIcon::Visible => cwd.filter(|p| p.is_absolute()).map(Path::to_path_buf),
        MacTitlebarProxyIcon::Hidden => None,
    }
}

/// 파일 시스템에서 종류를 본다
pub fn kind_of(path: &Path) -> Kind {
    match std::fs::metadata(path) {
        Ok(m) if m.is_dir() => Kind::Dir,
        Ok(_) => Kind::File,
        Err(_) => Kind::Missing,
    }
}

/// 놓은 것으로 할 일을 정한다
pub fn plan(path: &Path, kind: Kind) -> Plan {
    if !path.is_absolute() {
        return Plan::Refuse(Refusal::NotAbsolute);
    }
    match kind {
        Kind::Missing => Plan::Refuse(Refusal::Missing),
        // 폴더는 작업 폴더로만 쓰고 셸에 치지 않는다
        Kind::Dir => Plan::Folder {
            cwd: path.to_path_buf(),
        },
        Kind::File => {
            let Some(text) = path.to_str() else {
                return Plan::Refuse(Refusal::NotUtf8);
            };
            // 허용 목록 밖의 글자는 셸마다 따옴표 규칙이 달라 막을 수 없다. 실행하지 않고 거부한다
            let Some(input) = file_input(text) else {
                return Plan::Refuse(Refusal::UnsafeChars);
            };
            let cwd = path.parent().map_or_else(|| PathBuf::from("/"), Path::to_path_buf);
            Plan::File {
                cwd,
                input,
                shown: shown_path(text),
            }
        }
    }
}

/// `macos-dock-drop-behavior`와 창이 있는지로 새 칸을 어디에 띄울지 정한다
pub fn placement(behavior: MacDockDropBehavior, windowless: bool) -> Placement {
    match behavior {
        MacDockDropBehavior::NewTab if !windowless => Placement::NewTab,
        _ => Placement::NewWindow,
    }
}

/// Finder 서비스 한 번에 여는 폴더 수의 상한(한 번에 탭 수천 개가 뜨지 않게)
pub const SERVICE_MAX_DIRS: usize = 16;

/// Finder 서비스 한 번에 살펴보는(stat) 경로 수의 상한. 멈춘 네트워크 마운트 경로가 잔뜩 와도 메인 스레드가 오래 막히지 않게
pub const SERVICE_MAX_INSPECT: usize = SERVICE_MAX_DIRS * 4;

/// Finder 서비스(#631)가 넘긴 경로에서 열 폴더를 고른다. 폴더는 그대로, 파일은 부모 폴더. 상대 경로와 없는 경로는
/// 버리고, 겹치는 폴더는 한 번만(순서 유지) 담고, `SERVICE_MAX_DIRS`개에서 멈춘다. 파일을 셸에 치지 않는다
/// (실행은 Dock 드롭의 확인 막대 길(`plan`)에만 있다)
pub fn service_dirs(paths: Vec<PathBuf>, kind: impl Fn(&Path) -> Kind) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for path in paths.into_iter().filter(|p| p.is_absolute()).take(SERVICE_MAX_INSPECT) {
        let dir = match kind(&path) {
            Kind::Dir => path,
            Kind::File => path.parent().map_or_else(|| PathBuf::from("/"), Path::to_path_buf),
            Kind::Missing => continue,
        };
        if !out.contains(&dir) {
            out.push(dir);
        }
        if out.len() >= SERVICE_MAX_DIRS {
            break;
        }
    }
    out
}

/// 서비스가 탭을 달라고 했을 때: 열린 창이 없으면 창으로 띄운다(Ghostty `preferredParent`가 없을 때와 같다)
pub fn service_placement(want_tab: bool, windowless: bool) -> Placement {
    if want_tab && !windowless {
        Placement::NewTab
    } else {
        Placement::NewWindow
    }
}

/// 띄울 때 명령줄 인자로 온 경로를 AppKit이 열기 요청으로 다시 알린다. 그 요청을 한 번씩 걸러낸다
#[derive(Debug, Default)]
pub struct ArgFilter {
    paths: Vec<PathBuf>,
}

/// 띄운 뒤 이 시간이 지나면 거르지 않는다(그 뒤의 요청은 사용자가 놓은 것이다)
pub const ARG_FILTER_WINDOW: Duration = Duration::from_secs(10);

impl ArgFilter {
    /// `args`: 프로그램 이름을 뺀 명령줄. `-e` 뒤의 인자 중 실제로 있는 경로만 담는다
    pub fn new(args: &[String], cwd: &Path, exists: impl Fn(&Path) -> bool) -> Self {
        let after_e = args.iter().skip_while(|a| a.as_str() != "-e").skip(1);
        let paths = after_e
            .filter(|a| !a.is_empty() && !a.starts_with('-'))
            .map(|a| cwd.join(a))
            .filter(|p| exists(p))
            .collect();
        Self { paths }
    }

    /// 이 요청이 명령줄 인자가 되돌아온 것이면 참(한 번만)
    pub fn should_ignore(&mut self, filename: &Path, since_launch: Duration) -> bool {
        if since_launch > ARG_FILTER_WINDOW {
            return false;
        }
        match self.paths.iter().position(|p| p == filename) {
            Some(i) => {
                self.paths.remove(i);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_wraps_allowed_paths_literally() {
        assert_eq!(shell_quote("/a/b.sh").as_deref(), Some("'/a/b.sh'"));
        assert_eq!(shell_quote("/a b/c d.sh").as_deref(), Some("'/a b/c d.sh'"));
        assert_eq!(
            shell_quote("/Users/범수/스크립트 (1).sh").as_deref(),
            Some("'/Users/범수/스크립트 (1).sh'")
        );
        assert_eq!(
            shell_quote("/Users/me/~backup/a#1 [x]+y,z@w%v:u=t_s-r.sh").as_deref(),
            Some("'/Users/me/~backup/a#1 [x]+y,z@w%v:u=t_s-r.sh'")
        );
        assert_eq!(
            shell_quote("/日本語/ファイル.sh").as_deref(),
            Some("'/日本語/ファイル.sh'")
        );
    }

    #[test]
    fn quote_refuses_chars_outside_the_allow_list() {
        // 셸마다 작은따옴표 안의 규칙이 다르다: fish는 \'와 \\를 해석하고, tcsh는 !를 펼친다
        let bad = [
            "/a/x\\'; touch /tmp/pwned; echo \\'.sh",
            "/a/it's.sh",
            "/a/a\\b.sh",
            "/a/a!b.sh",
            "/a/$HOME",
            "/a/`id`",
            "/a/a\"b",
            "/a/a;b",
            "/a/a&b.sh",
            "/a/a|b",
            "/a/a<b>c",
            "/a/a*b?",
            "/a/{a,b}",
            "/a/\u{202e}gnp.sh",
            "/a/a\u{a0}b",
            "/a/a\u{3000}b",
            "/a/a\u{200b}b",
            "/a/a\u{301}b",
            "/a/a\tb",
            "/a/a\nb",
            "/a/\x1b[2J",
        ];
        for p in bad {
            assert_eq!(shell_quote(p), None, "{p:?}");
            assert_eq!(file_input(p), None, "{p:?}");
            assert!(!is_shell_safe(p), "{p:?}");
        }
    }

    #[test]
    fn file_input_is_quoted_path_then_exit() {
        assert_eq!(file_input("/a b/c.sh").as_deref(), Some("'/a b/c.sh'; exit\n"));
        assert_eq!(file_input("/it's.sh"), None);
    }

    #[test]
    fn shown_path_strips_controls_and_keeps_the_tail() {
        assert_eq!(shown_path("/a/b.sh"), "/a/b.sh");
        assert_eq!(shown_path("/a\x1b[2Jb"), "/a [2Jb");
        let long = format!("/{}/run.sh", "d".repeat(500));
        let s = shown_path(&long);
        assert!(s.starts_with('…') && s.ends_with("/run.sh"), "{s}");
        assert!(s.chars().count() <= 201, "{}", s.chars().count());
    }

    #[test]
    fn plan_splits_folder_and_file() {
        assert_eq!(
            plan(Path::new("/Users/me/proj"), Kind::Dir),
            Plan::Folder {
                cwd: "/Users/me/proj".into()
            }
        );
        assert_eq!(
            plan(Path::new("/Users/me/run it.sh"), Kind::File),
            Plan::File {
                cwd: "/Users/me".into(),
                input: "'/Users/me/run it.sh'; exit\n".into(),
                shown: "/Users/me/run it.sh".into(),
            }
        );
        // 폴더는 따옴표로 셸에 치지 않고 작업 폴더로만 쓰므로 이름에 제어 문자가 있어도 된다
        assert!(matches!(plan(Path::new("/a\nb"), Kind::Dir), Plan::Folder { .. }));
    }

    #[test]
    fn plan_refuses() {
        assert_eq!(plan(Path::new("/nope"), Kind::Missing), Plan::Refuse(Refusal::Missing));
        assert_eq!(
            plan(Path::new("rel.sh"), Kind::File),
            Plan::Refuse(Refusal::NotAbsolute)
        );
        assert_eq!(plan(Path::new("rel"), Kind::Dir), Plan::Refuse(Refusal::NotAbsolute));
        assert_eq!(
            plan(Path::new("/a/b\nls"), Kind::File),
            Plan::Refuse(Refusal::UnsafeChars)
        );
        assert_eq!(
            plan(Path::new("/a/it's.sh"), Kind::File),
            Plan::Refuse(Refusal::UnsafeChars)
        );
        assert_eq!(
            plan(Path::new("/a/a&b.sh"), Kind::File),
            Plan::Refuse(Refusal::UnsafeChars)
        );
        assert_eq!(
            plan(Path::new("/a/\x15ls\r.sh"), Kind::File),
            Plan::Refuse(Refusal::UnsafeChars)
        );
        // 루트 바로 아래 파일의 부모는 /
        assert_eq!(
            plan(Path::new("/run.sh"), Kind::File),
            Plan::File {
                cwd: "/".into(),
                input: "'/run.sh'; exit\n".into(),
                shown: "/run.sh".into()
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn plan_refuses_non_utf8_file() {
        use std::os::unix::ffi::OsStrExt;
        let p = Path::new(std::ffi::OsStr::from_bytes(b"/a/\xff.sh"));
        assert_eq!(plan(p, Kind::File), Plan::Refuse(Refusal::NotUtf8));
        // 폴더는 글자로 바꾸지 않고 그대로 작업 폴더가 된다
        assert!(matches!(plan(p, Kind::Dir), Plan::Folder { .. }));
    }

    #[test]
    fn proxy_icon_follows_setting_and_known_folder() {
        use MacTitlebarProxyIcon::{Hidden, Visible};
        let cwd = Path::new("/Users/me/proj");
        assert_eq!(proxy_icon_path(Visible, Some(cwd)), Some(cwd.to_path_buf()));
        assert_eq!(proxy_icon_path(Hidden, Some(cwd)), None, "hidden이면 없앤다");
        assert_eq!(proxy_icon_path(Visible, None), None, "폴더를 모르면 없앤다");
        assert_eq!(proxy_icon_path(Visible, Some(Path::new("rel"))), None, "절대 경로만");
    }

    #[test]
    fn placement_follows_behavior_and_windows() {
        use MacDockDropBehavior::{NewTab, NewWindow};
        assert_eq!(placement(NewTab, false), Placement::NewTab);
        assert_eq!(placement(NewTab, true), Placement::NewWindow);
        assert_eq!(placement(NewWindow, false), Placement::NewWindow);
        assert_eq!(placement(NewWindow, true), Placement::NewWindow);
    }

    #[test]
    fn kind_of_reads_the_file_system() {
        let dir = std::env::temp_dir();
        assert_eq!(kind_of(&dir), Kind::Dir);
        assert_eq!(kind_of(&dir.join("bt-no-such-279")), Kind::Missing);
        let f = dir.join(format!("bt-279-{}", std::process::id()));
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(kind_of(&f), Kind::File);
        std::fs::remove_file(&f).unwrap();
    }

    #[test]
    fn service_dirs_maps_folders_files_and_drops_the_rest() {
        let kind = |p: &Path| match p.to_str().unwrap() {
            "/w/proj" | "/w/other" => Kind::Dir,
            "/w/proj/a.txt" | "/w/b.sh" => Kind::File,
            _ => Kind::Missing,
        };
        let got = service_dirs(
            vec![
                "/w/proj".into(),
                "/w/proj/a.txt".into(), // 파일은 부모 폴더, 이미 있는 폴더와 겹치면 한 번만
                "/w/b.sh".into(),
                "/w/none".into(), // 없는 경로는 버린다
                "rel/dir".into(), // 상대 경로는 버린다
                "/w/other".into(),
            ],
            kind,
        );
        assert_eq!(
            got,
            vec![PathBuf::from("/w/proj"), PathBuf::from("/w"), PathBuf::from("/w/other")]
        );
        assert!(service_dirs(vec![], kind).is_empty());
        assert!(service_dirs(vec!["/w/none".into()], kind).is_empty());
    }

    #[test]
    fn service_dirs_stats_a_bounded_number_of_paths() {
        // 없는 경로(멈춘 네트워크 마운트일 수 있음)만 잔뜩 와도 메인 스레드에서 stat하는 수는 상한에서 멈춘다
        let seen = std::cell::Cell::new(0usize);
        let paths: Vec<PathBuf> = (0..256).map(|i| PathBuf::from(format!("/net/m{i}"))).collect();
        let got = service_dirs(paths, |_| {
            seen.set(seen.get() + 1);
            Kind::Missing
        });
        assert!(got.is_empty());
        assert_eq!(seen.get(), SERVICE_MAX_INSPECT);
    }

    #[test]
    fn service_dirs_caps_the_count() {
        let paths: Vec<PathBuf> = (0..SERVICE_MAX_DIRS + 10)
            .map(|i| PathBuf::from(format!("/d/{i}")))
            .collect();
        assert_eq!(service_dirs(paths, |_| Kind::Dir).len(), SERVICE_MAX_DIRS);
    }

    #[test]
    fn service_dirs_never_turns_a_file_into_input() {
        // 서비스는 실행하지 않는다: 글자에 상관없이 폴더 경로만 나온다
        let got = service_dirs(vec!["/a/b\nls; rm -rf ~.sh".into()], |_| Kind::File);
        assert_eq!(got, vec![PathBuf::from("/a")]);
    }

    #[test]
    fn service_placement_opens_a_window_when_none_exists() {
        assert_eq!(service_placement(true, false), Placement::NewTab);
        assert_eq!(service_placement(true, true), Placement::NewWindow);
        assert_eq!(service_placement(false, false), Placement::NewWindow);
        assert_eq!(service_placement(false, true), Placement::NewWindow);
    }

    fn filter(args: &[&str]) -> ArgFilter {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        ArgFilter::new(&args, Path::new("/work"), |p| {
            matches!(p.to_str(), Some("/work/a.sh") | Some("/abs/b.sh"))
        })
    }

    #[test]
    fn arg_filter_ignores_paths_after_dash_e_once() {
        let mut f = filter(&["-e", "vim", "a.sh", "/abs/b.sh", "--flag"]);
        let early = Duration::from_millis(300);
        assert!(f.should_ignore(Path::new("/work/a.sh"), early));
        assert!(!f.should_ignore(Path::new("/work/a.sh"), early), "한 번만");
        assert!(f.should_ignore(Path::new("/abs/b.sh"), early));
        assert!(!f.should_ignore(Path::new("/other/c.sh"), early));
    }

    #[test]
    fn arg_filter_leaves_unrelated_requests_and_non_e_launches() {
        let mut f = filter(&["--cwd", "/abs/b.sh"]);
        assert!(
            !f.should_ignore(Path::new("/abs/b.sh"), Duration::ZERO),
            "-e가 없으면 걸러내지 않는다"
        );
        let mut f = filter(&["-e", "vim", "a.sh"]);
        assert!(
            !f.should_ignore(Path::new("/work/a.sh"), ARG_FILTER_WINDOW + Duration::from_millis(1)),
            "시간이 지나면 사용자가 놓은 것"
        );
    }
}
