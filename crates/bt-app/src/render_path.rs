//! 그리기 길 고르기(ADR-036, ADR-039): macOS는 GPU 길이 기본이고 그 밖의 OS는 CPU 길이 기본이다.
//! 환경 변수 `BT_GPU=0`이면 CPU 길, `BT_GPU=1`이면 어느 OS든 GPU 길이다.
//! 장치나 창 surface를 열지 못하면 CPU 길로 돌아간다. 설정 때문에 CPU로 보내는 갈래는 없다(`window-colorspace = srgb`도 GPU 길, #466)

/// 이 OS의 기본 길이 GPU인가. 실제 창에서 재고 화면을 견준 것은 macOS(Metal)뿐이다(ADR-039)
pub const GPU_BY_DEFAULT: bool = cfg!(target_os = "macos");

/// `BT_GPU` 값과 OS 기본으로 GPU 길을 달라는 뜻인가. `0`·`1`만 고르고 다른 값은 OS 기본을 따른다
pub fn wants_gpu(env: Option<&str>, gpu_by_default: bool) -> bool {
    match env {
        Some("1") => true,
        Some("0") => false,
        _ => gpu_by_default,
    }
}

/// 길을 고른다. GPU를 열어 보는 `open`은 GPU 길을 쓰기로 했을 때(`wants_gpu`)만 부른다.
/// 투명 창에 쓸 알파 모드나 `window-colorspace = srgb`용 Display P3 surface를 주지 않으면 `open`이 `None`을 돌려 CPU로 돌아간다.
/// 돌려주는 것: 열린 GPU 렌더러(없으면 CPU 길)와, GPU를 달랐는데 CPU로 돌아간 이유
pub fn choose<T>(
    env: Option<&str>,
    gpu_by_default: bool,
    open: impl FnOnce() -> Option<T>,
) -> (Option<T>, Option<&'static str>) {
    if !wants_gpu(env, gpu_by_default) {
        return (None, None);
    }
    match open() {
        Some(gpu) => (Some(gpu), None),
        None => (None, Some("GPU 어댑터나 창 surface를 열지 못했다")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `open`이 불렸는지와 결과
    fn run(env: Option<&str>, gpu_by_default: bool) -> (Option<u8>, Option<&'static str>, bool) {
        let mut opened = false;
        let (gpu, why) = choose(env, gpu_by_default, || {
            opened = true;
            Some(7)
        });
        (gpu, why, opened)
    }

    #[test]
    fn zero_and_one_choose_and_anything_else_follows_the_os_default() {
        for default in [true, false] {
            assert!(wants_gpu(Some("1"), default), "1은 어느 OS든 GPU");
            assert!(!wants_gpu(Some("0"), default), "0은 어느 OS든 CPU");
            for v in [
                None,
                Some(""),
                Some("true"),
                Some("yes"),
                Some("11"),
                Some(" 1"),
                Some("00"),
            ] {
                assert_eq!(wants_gpu(v, default), default, "{v:?}는 OS 기본을 따른다");
            }
        }
    }

    #[test]
    fn default_is_gpu_where_the_os_default_is_gpu() {
        assert_eq!(run(None, true), (Some(7), None, true));
    }

    #[test]
    fn default_is_cpu_elsewhere_and_never_touches_the_gpu() {
        assert_eq!(run(None, false), (None, None, false));
    }

    #[test]
    fn bt_gpu_zero_never_opens_the_gpu_even_where_gpu_is_default() {
        assert_eq!(run(Some("0"), true), (None, None, false));
    }

    #[test]
    fn bt_gpu_one_opens_the_gpu_where_cpu_is_default() {
        assert_eq!(run(Some("1"), false), (Some(7), None, true));
    }

    #[test]
    fn no_setting_sends_the_window_to_the_cpu_before_opening() {
        // 투명 창·배경 그림·`window-colorspace = srgb`(#466)는 설정만으로 CPU로 보내지 않는다: 먼저 GPU를 열어 본다.
        // 알파 모드나 Display P3 surface를 못 얻으면 열기가 실패해 CPU로 돌아간다(아래 시험)
        assert_eq!(run(None, true), (Some(7), None, true));
    }

    #[test]
    fn open_failure_falls_back_with_a_reason() {
        // `srgb`에서 surface가 Display P3를 주지 않을 때도 이 길이다. 이유는 앱이 경고 로그로 남긴다
        let mut opened = false;
        let (gpu, why) = choose::<u8>(None, true, || {
            opened = true;
            None
        });
        assert!(opened);
        assert!(gpu.is_none());
        assert!(why.is_some());
    }
}
