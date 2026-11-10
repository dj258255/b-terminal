//! 그리기 길 고르기(ADR-036): 환경 변수 `BT_GPU=1`일 때만 GPU 길, 기본은 CPU 길.
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

/// `BT_GPU` 값이 GPU 길을 달라는 뜻인가. `1`만 그렇다(시험용 스위치라 다른 값은 모두 끈 것으로 본다)
pub fn wants_gpu(env: Option<&str>) -> bool {
    env == Some("1")
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

/// 길을 고른다. GPU를 열어 보는 `open`은 GPU를 달라고 했고 설정이 허락할 때만 부른다.
/// 돌려주는 것: 열린 GPU 렌더러(없으면 CPU 길)와, GPU를 달랐는데 CPU로 돌아간 이유
pub fn choose<T>(
    env: Option<&str>,
    inputs: &PathInputs,
    open: impl FnOnce() -> Option<T>,
) -> (Option<T>, Option<&'static str>) {
    if !wants_gpu(env) {
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

    #[test]
    fn only_one_asks_for_gpu() {
        assert!(wants_gpu(Some("1")));
        for v in [
            None,
            Some(""),
            Some("0"),
            Some("true"),
            Some("yes"),
            Some("11"),
            Some(" 1"),
        ] {
            assert!(!wants_gpu(v), "{v:?}");
        }
    }

    #[test]
    fn default_is_cpu_and_never_touches_the_gpu() {
        let mut opened = false;
        let (gpu, why) = choose(None, &PLAIN, || {
            opened = true;
            Some(1)
        });
        assert_eq!((gpu, why), (None, None));
        assert!(!opened, "BT_GPU가 없으면 GPU를 열어 보지도 않는다");
        let (gpu, why) = choose(Some("0"), &PLAIN, || Some(1));
        assert_eq!((gpu, why), (None, None));
    }

    #[test]
    fn bt_gpu_one_opens_the_gpu() {
        let (gpu, why) = choose(Some("1"), &PLAIN, || Some(7));
        assert_eq!((gpu, why), (Some(7), None));
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
            let mut opened = false;
            let (gpu, why) = choose(Some("1"), &inputs, || {
                opened = true;
                Some(1)
            });
            assert_eq!((gpu, why), (Some(1), None), "{inputs:?}");
            assert!(opened, "{inputs:?}");
        }
    }

    #[test]
    fn srgb_colorspace_still_falls_back_without_opening() {
        // 안전한 API로 surface의 색 공간 태그를 정할 수 없다: 그대로 CPU 길
        let inputs = PathInputs {
            colorspace_srgb: true,
            ..PLAIN
        };
        let mut opened = false;
        let (gpu, why) = choose(Some("1"), &inputs, || {
            opened = true;
            Some(1)
        });
        assert!(gpu.is_none());
        assert!(why.is_some(), "돌아간 이유를 알려야 한다");
        assert!(!opened, "못 맞추는 설정이면 열지 않는다");
    }

    #[test]
    fn open_failure_falls_back_with_a_reason() {
        let (gpu, why) = choose::<u8>(Some("1"), &PLAIN, || None);
        assert!(gpu.is_none());
        assert!(why.is_some());
    }
}
