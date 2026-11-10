//! 그리기 길 고르기(ADR-036, ADR-039): macOS는 GPU 길이 기본이고 그 밖의 OS는 CPU 길이 기본이다.
//! 환경 변수 `BT_GPU=0`이면 CPU 길, `BT_GPU=1`이면 어느 OS든 GPU 길이다.
//! GPU 길이 CPU와 같게 그리지 못하는 설정이거나 장치를 열지 못하면 CPU 길로 돌아간다

/// 길을 고를 때 보는 설정
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathInputs {
    /// `background-opacity`. 투명 창이면 곱한 알파를 받는 surface로 연다
    pub opacity: f32,
    /// `background-image`를 읽어 들였다(아래층에 그림 층을 깔아 맞춘다)
    pub backdrop_image: bool,
    /// `window-colorspace = srgb`
    pub colorspace_srgb: bool,
}

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

/// GPU 길이 아직 CPU와 같게 그리지 못하는 설정이면 그 이유(로그에 쓴다).
/// 투명 창(`background-opacity < 1`)과 배경 그림은 0.31부터 GPU 길이 곱한 알파 합치기와 아래층 그림으로 맞춘다.
/// 투명 창에 쓸 알파 모드를 surface가 주지 않으면 장치를 여는 쪽이 실패해 CPU로 돌아간다(`choose`의 두 번째 갈래).
/// `window-colorspace = srgb`는 안전한 API로 surface의 색 공간 태그를 정할 수 없어 맞추지 못한다(ADR-036)
pub fn cpu_only_reason(inputs: &PathInputs) -> Option<&'static str> {
    if inputs.colorspace_srgb {
        Some("window-colorspace = srgb")
    } else {
        None
    }
}

/// 길을 고른다. GPU를 열어 보는 `open`은 GPU 길을 쓰기로 했고(`wants_gpu`) 설정이 허락할 때만 부른다.
/// 돌려주는 것: 열린 GPU 렌더러(없으면 CPU 길)와, GPU를 달랐는데 CPU로 돌아간 이유
pub fn choose<T>(
    env: Option<&str>,
    gpu_by_default: bool,
    inputs: &PathInputs,
    open: impl FnOnce() -> Option<T>,
) -> (Option<T>, Option<&'static str>) {
    if !wants_gpu(env, gpu_by_default) {
        return (None, None);
    }
    if let Some(why) = cpu_only_reason(inputs) {
        return (None, Some(why));
    }
    match open() {
        Some(gpu) => (Some(gpu), None),
        None => (None, Some("GPU 어댑터나 창 surface를 열지 못했다")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: PathInputs = PathInputs {
        opacity: 1.0,
        backdrop_image: false,
        colorspace_srgb: false,
    };

    /// `open`이 불렸는지와 결과
    fn run(env: Option<&str>, gpu_by_default: bool, inputs: &PathInputs) -> (Option<u8>, Option<&'static str>, bool) {
        let mut opened = false;
        let (gpu, why) = choose(env, gpu_by_default, inputs, || {
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
        assert_eq!(run(None, true, &PLAIN), (Some(7), None, true));
    }

    #[test]
    fn default_is_cpu_elsewhere_and_never_touches_the_gpu() {
        assert_eq!(run(None, false, &PLAIN), (None, None, false));
    }

    #[test]
    fn bt_gpu_zero_never_opens_the_gpu_even_where_gpu_is_default() {
        assert_eq!(run(Some("0"), true, &PLAIN), (None, None, false));
    }

    #[test]
    fn bt_gpu_one_opens_the_gpu_where_cpu_is_default() {
        assert_eq!(run(Some("1"), false, &PLAIN), (Some(7), None, true));
    }

    #[test]
    fn transparent_window_and_background_image_open_the_gpu() {
        // 0.31: 곱한 알파 합치기와 아래층 배경 그림으로 GPU 길이 맞춘다. 알파 모드를 못 얻으면 열기가 실패해 CPU로 돌아간다
        let cases = [
            PathInputs { opacity: 0.8, ..PLAIN },
            PathInputs { opacity: 0.0, ..PLAIN },
            PathInputs {
                backdrop_image: true,
                ..PLAIN
            },
            PathInputs {
                opacity: 0.5,
                backdrop_image: true,
                ..PLAIN
            },
        ];
        for inputs in cases {
            assert_eq!(run(None, true, &inputs), (Some(7), None, true), "{inputs:?}");
        }
    }

    #[test]
    fn srgb_colorspace_falls_back_without_opening_even_by_default() {
        // 안전한 API로 surface의 색 공간 태그를 정할 수 없다: 그대로 CPU 길, 이유를 알린다
        let inputs = PathInputs {
            colorspace_srgb: true,
            ..PLAIN
        };
        for env in [None, Some("1")] {
            let (gpu, why, opened) = run(env, true, &inputs);
            assert!(gpu.is_none(), "{env:?}");
            assert!(why.is_some(), "돌아간 이유를 알려야 한다: {env:?}");
            assert!(!opened, "못 맞추는 설정이면 열지 않는다: {env:?}");
        }
    }

    #[test]
    fn open_failure_falls_back_with_a_reason_by_default() {
        let (gpu, why) = choose::<u8>(None, true, &PLAIN, || None);
        assert!(gpu.is_none());
        assert!(why.is_some());
    }
}
