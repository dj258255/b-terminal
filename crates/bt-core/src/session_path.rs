//! kitty 세션 파일(#934)의 경로와 `$변수` 풀기. 설정 `startup-session`·`--session`과 세션 파일 줄이 같이 쓴다.
//! 환경과 폴더를 인자로 받는 순수 함수라 여기서 테스트한다

use std::path::{Path, PathBuf};

/// `$NAME`·`${NAME}`을 푼다. `$$`는 글자 `$`. 값이 없는 변수는 그대로 둔다(kitty `expandvars`와 같음)
pub fn expand_vars(val: &str, env: &dyn Fn(&str) -> Option<String>) -> String {
    if !val.contains('$') {
        return val.to_string();
    }
    let chars: Vec<char> = val.chars().collect();
    let mut out = String::with_capacity(val.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        // `$$` → `$`
        if chars.get(i + 1) == Some(&'$') {
            out.push('$');
            i += 2;
            continue;
        }
        let (name, end) = if chars.get(i + 1) == Some(&'{') {
            match chars[i + 2..].iter().position(|&c| c == '}') {
                Some(n) if n > 0 => (chars[i + 2..i + 2 + n].iter().collect::<String>(), i + 3 + n),
                _ => (String::new(), i),
            }
        } else {
            let n = chars[i + 1..]
                .iter()
                .take_while(|c| c.is_alphanumeric() || **c == '_')
                .count();
            (chars[i + 1..i + 1 + n].iter().collect::<String>(), i + 1 + n)
        };
        match (!name.is_empty()).then(|| env(&name)).flatten() {
            Some(v) => {
                out.push_str(&v);
                i = end;
            }
            None => {
                out.push('$');
                i += 1;
            }
        }
    }
    out
}

/// 글 앞의 `~`(또는 `~/`)를 집 폴더로 바꾼다. `~이름`은 풀지 않는다
pub fn expand_tilde(p: &str, home: &Path) -> PathBuf {
    if p == "~" {
        return home.to_path_buf();
    }
    match p.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(p),
    }
}

/// `--session`·`startup-session` 값을 경로로 바꾼다(kitty `config_or_absolute_path`): 빈 값과 `none`(대소문자 무시)은 없음,
/// `~`와 `$변수`를 풀고, 상대 경로는 설정 폴더(`config_dir`) 기준으로 잇는다
pub fn resolve_session_arg(
    raw: &str,
    home: &Path,
    config_dir: &Path,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("none") {
        return None;
    }
    let p = expand_tilde(raw, home);
    let p = PathBuf::from(expand_vars(&p.to_string_lossy(), env));
    Some(if p.is_absolute() { p } else { config_dir.join(p) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(k: &str) -> Option<String> {
        match k {
            "PROJ" => Some("/work/proj".into()),
            "X" => Some("x".into()),
            _ => None,
        }
    }

    #[test]
    fn expand_vars_like_kitty() {
        assert_eq!(expand_vars("$PROJ/a", &env), "/work/proj/a");
        assert_eq!(expand_vars("${PROJ}a", &env), "/work/proja");
        assert_eq!(expand_vars("$$X $X", &env), "$X x");
        assert_eq!(
            expand_vars("$NOPE ${NOPE} $", &env),
            "$NOPE ${NOPE} $",
            "없는 변수는 그대로"
        );
        assert_eq!(expand_vars("${} ${X", &env), "${} ${X", "닫히지 않은 중괄호는 그대로");
        assert_eq!(expand_vars("가나 $X 다", &env), "가나 x 다");
    }

    #[test]
    fn session_arg_resolution() {
        let home = Path::new("/home/u");
        let cfg = Path::new("/home/u/.config/b-terminal");
        let r = |s: &str| resolve_session_arg(s, home, cfg, &env);
        assert_eq!(r(""), None);
        assert_eq!(r("none"), None);
        assert_eq!(r("NONE"), None, "대소문자 무시");
        assert_eq!(r("~/s/a.session"), Some(PathBuf::from("/home/u/s/a.session")));
        assert_eq!(r("$PROJ/a.session"), Some(PathBuf::from("/work/proj/a.session")));
        assert_eq!(
            r("rel/a.session"),
            Some(PathBuf::from("/home/u/.config/b-terminal/rel/a.session")),
            "상대 경로는 설정 폴더 기준(kitty와 같음)"
        );
        assert_eq!(r("/abs/a"), Some(PathBuf::from("/abs/a")));
        assert_eq!(r("~"), Some(PathBuf::from("/home/u")));
    }
}
