//! 배경 그림 층(#134): CPU 길과 GPU 길(#359)이 같은 층을 만든다. 창 크기·바탕색이 바뀌면 다시 합성한다

use super::*;

/// 이 창이 들고 있는 배경 그림 층: (창 너비, 높이, 기본 바탕색 RGB)로 만든 키와, 창 크기의 곱한 알파 픽셀
pub(super) type Backdrop = ((u32, u32, u32), Vec<u32>);

/// `slot`을 지금 창 크기·바탕색·불투명도의 층으로 맞춘다. 그림이 없으면 비운다.
/// 화면 반전(DECSCNM, #302)이면 기본 바탕색이 맞바뀌므로 `win_bg`가 그 색을 따른다
pub(super) fn refresh(
    slot: &mut Option<Backdrop>,
    image: Option<&bt_render::background::BgImage>,
    config: &AppConfig,
    size: (u32, u32),
    win_bg: bt_core::Rgb,
    opacity: f32,
) {
    let key = (size.0, size.1, win_bg.to_u32());
    match image {
        Some(img) if slot.as_ref().map(|b| b.0) != Some(key) => {
            let bg_alpha = opacity.clamp(0.0, 1.0);
            let a = (bg_alpha * 255.0).round() as u32;
            let mul = |c: u8| (c as u32 * a + 127) / 255;
            let bg = win_bg;
            let base = (a << 24) | (mul(bg.r) << 16) | (mul(bg.g) << 8) | mul(bg.b);
            // 1보다 크면 background-opacity보다 진하게(Ghostty와 같은 뜻)
            let alpha = (config.background_image_opacity * bg_alpha).min(1.0);
            let layer = bt_render::background::compose(
                img,
                size.0,
                size.1,
                config.background_image_fit,
                config.background_image_position,
                config.background_image_repeat,
                alpha,
                base,
            );
            *slot = Some((key, layer));
        }
        Some(_) => {}
        None => *slot = None,
    }
}
