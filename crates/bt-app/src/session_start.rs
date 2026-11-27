//! 시작할 때 kitty 세션 파일(#934)을 쓸지 정한다. 창과 상관없는 순수 함수라 여기서 테스트한다

use std::path::{Path, PathBuf};

/// 어느 줄을 따를까.
/// - 세션 파일이 없으면 쓰지 않는다.
/// - 명령줄 `--session`은 늘 이긴다(`-e`·`--cwd`보다도. kitty `--session` 설명: 같이 준 실행할 프로그램은 무시한다).
/// - 설정 `startup-session`은 `-e`·`--cwd`로 띄울 때는 쓰지 않는다(kitty `startup_session` 설명: 명령줄에 실행할 프로그램이 있으면 이 옵션은 무시한다).
///
/// 쓰기로 하면 창 상태 복원(`window-save-state`)과 `initial-command`는 이 실행에서 건너뛴다(ADR-104). 끌 때의 창 상태 저장은 그대로다
pub fn startup_session_to_use(session: Option<&Path>, from_cli: bool, cli_command_or_cwd: bool) -> Option<PathBuf> {
    let path = session?;
    (from_cli || !cli_command_or_cwd).then(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_choice() {
        let p = Some(Path::new("/s/a.session"));
        let some = || Some(PathBuf::from("/s/a.session"));
        assert_eq!(
            startup_session_to_use(None, false, false),
            None,
            "세션이 없으면 쓰지 않는다"
        );
        assert_eq!(startup_session_to_use(None, true, true), None);
        assert_eq!(startup_session_to_use(p, false, false), some(), "설정만 있으면 쓴다");
        assert_eq!(
            startup_session_to_use(p, false, true),
            None,
            "설정의 startup-session은 -e·--cwd로 띄울 때 쓰지 않는다"
        );
        assert_eq!(startup_session_to_use(p, true, false), some(), "--session은 쓴다");
        assert_eq!(
            startup_session_to_use(p, true, true),
            some(),
            "--session은 -e보다 앞선다"
        );
    }
}
