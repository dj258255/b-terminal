//! "기본 터미널로 지정"(#859). Ghostty 1.4.0 `AppDelegate.setAsDefaultTerminal`처럼 UTType `public.unix-executable`의
//! 기본 앱을 이 앱 묶음으로 정한다. 지금 기본인지는 `URLForApplicationToOpenContentType`(읽기만)으로 본다.
//! 시스템 설정을 바꾸는 호출(`set_default`)은 시험에서 부르지 않는다. 판단은 순수 함수로 나눠 시험한다.

#[cfg(any(target_os = "macos", test))]
use std::path::{Path, PathBuf};

/// 실행 파일 경로(`…/b-terminal.app/Contents/MacOS/b-terminal`)에서 앱 묶음 경로를 찾는다. 묶음 밖에서 돌면 없다
#[cfg(any(target_os = "macos", test))]
pub fn bundle_of(exe: &Path) -> Option<PathBuf> {
    exe.ancestors()
        .find(|p| p.extension().is_some_and(|e| e == "app") && p.is_dir())
        .map(Path::to_path_buf)
}

/// 지금 기본 앱(`default`)이 이 묶음(`me`)인가. 심볼릭 링크를 풀어 견주고, 풀 수 없으면 글자 그대로 견준다.
/// 기본 앱을 읽지 못했으면(없음) 아니라고 본다
#[cfg(any(target_os = "macos", test))]
pub fn is_default(default: Option<&Path>, me: &Path) -> bool {
    let Some(default) = default else { return false };
    if default == me {
        return true;
    }
    match (default.canonicalize(), me.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// 메뉴 항목을 켤지: 묶음으로 돌고 있고 아직 기본이 아닐 때만(Ghostty `validateMenuItem`은 기본이면 끈다)
#[cfg(any(target_os = "macos", test))]
pub fn menu_enabled(bundle: Option<&Path>, default: Option<&Path>) -> bool {
    bundle.is_some_and(|me| !is_default(default, me))
}

#[cfg(target_os = "macos")]
mod sys {
    use std::path::{Path, PathBuf};

    use block2::RcBlock;
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::{NSError, NSString, NSURL, ns_string};
    use objc2_uniform_type_identifiers::UTType;

    fn unix_executable() -> Option<objc2::rc::Retained<UTType>> {
        UTType::typeWithIdentifier(ns_string!("public.unix-executable"))
    }

    /// 실행 파일(unix-executable)의 지금 기본 앱 경로. 읽기만 한다
    pub fn current_default() -> Option<PathBuf> {
        let ty = unix_executable()?;
        let url = NSWorkspace::sharedWorkspace().URLForApplicationToOpenContentType(&ty)?;
        url.path().map(|p| PathBuf::from(p.to_string()))
    }

    /// `bundle`을 실행 파일의 기본 앱으로 정하게 시스템에 요청한다. 끝나면 어느 스레드에서든 `done(성공인가)`를 부른다.
    /// 오류 개체는 raw 포인터로 오고 읽으려면 unsafe라(작업 공간은 unsafe 금지) 성공 여부(널인가)만 쓴다.
    /// 요청 자체를 못 하면 false. **시스템 설정을 바꾼다**
    pub fn set_default(bundle: &Path, done: impl Fn(bool) + 'static) -> bool {
        let Some(ty) = unix_executable() else { return false };
        let url = NSURL::fileURLWithPath(&NSString::from_str(&bundle.to_string_lossy()));
        let handler = RcBlock::new(move |err: *mut NSError| done(err.is_null()));
        NSWorkspace::sharedWorkspace().setDefaultApplicationAtURL_toOpenContentType_completionHandler(
            &url,
            &ty,
            Some(&handler),
        );
        true
    }
}

#[cfg(target_os = "macos")]
pub use sys::{current_default, set_default};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_found_from_exe_path() {
        let dir = std::env::temp_dir().join(format!("bt-859-{}", std::process::id()));
        let exe_dir = dir.join("b-terminal.app/Contents/MacOS");
        std::fs::create_dir_all(&exe_dir).unwrap();
        assert_eq!(bundle_of(&exe_dir.join("b-terminal")), Some(dir.join("b-terminal.app")));
        assert_eq!(bundle_of(&dir.join("target/debug/b-terminal")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn default_judgement() {
        let me = Path::new("/Applications/b-terminal.app");
        assert!(is_default(Some(Path::new("/Applications/b-terminal.app/")), me));
        assert!(!is_default(Some(Path::new("/Applications/Ghostty.app")), me));
        assert!(!is_default(None, me));
    }

    #[test]
    fn menu_enabled_only_when_bundle_and_not_default() {
        let me = Path::new("/Applications/b-terminal.app");
        let other = Path::new("/System/Applications/Utilities/Terminal.app");
        assert!(menu_enabled(Some(me), Some(other)));
        assert!(menu_enabled(Some(me), None));
        assert!(!menu_enabled(Some(me), Some(me)));
        assert!(!menu_enabled(None, Some(other)));
    }
}
