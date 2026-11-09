//! 창 뒤 흐림(`background-blur`, #238). 창을 띄우지 않고 테스트하려고 판단을 순수 함수로 뺐다

/// 창에 줄 흐림 반지름. `None`이면 비공개 API를 부르지 않는다.
/// Ghostty `ghostty_set_window_background_blur`처럼 `background-opacity`가 1 이상이면 부르지 않는다
/// (불투명하면 흐림이 보이지 않는다). 그 밖에는 세기(0이면 끔)를 그대로 반지름으로 쓴다
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn radius_to_apply(blur: u8, opacity: f32) -> Option<i64> {
    (opacity < 1.0).then_some(i64::from(blur))
}

/// 창에 흐림을 건다(끄기 포함). 비공개 API 호출은 vendor/winit 안에 있다(`set_blur_radius`)
#[cfg_attr(not(target_os = "macos"), allow(unused_variables))]
pub fn apply(window: &winit::window::Window, blur: u8, opacity: f32) {
    #[cfg(target_os = "macos")]
    if let Some(r) = radius_to_apply(blur, opacity) {
        use winit::platform::macos::WindowExtMacOS;
        window.set_blur_radius(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radius_follows_opacity() {
        assert_eq!(radius_to_apply(20, 0.7), Some(20));
        assert_eq!(radius_to_apply(255, 0.0), Some(255));
        // 0은 "끔": 열려 있는 흐림을 지우려고 0을 부른다
        assert_eq!(radius_to_apply(0, 0.5), Some(0));
        // 불투명이면 부르지 않는다(Ghostty와 같다)
        assert_eq!(radius_to_apply(20, 1.0), None);
        assert_eq!(radius_to_apply(0, 1.0), None);
    }
}
