//! 창 surface에 합친 프레임을 올린다(ADR-036). 안전한 API만 쓴다: `Instance::create_surface`에 `'static` 창 대상을 넘긴다

use std::time::{Duration, Instant};

use bt_render::Fonts;

use super::{GpuRenderer, LayerUpdate, PaneDraw, pick_adapter, srgb_twin, srgb_view_of};
use crate::p3::P3Convert;
use crate::pacing::{
    DEFAULT_REFRESH, FramePacer, PaceDecision, SurfaceVisibility, frame_latency_from_env, surface_latency,
};
use bt_core::AlphaBlending;

/// 창이 가려졌을 때 `present`가 돌려주는 이유
const OCCLUDED: &str = "창이 가려졌다";

/// 창 surface와 지금 설정
pub(super) struct WindowSurface {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    /// surface가 다시 설정하라고 했다
    stale: bool,
    /// 곱한 알파를 받는 알파 모드로 열었다(투명 창, #359)
    transparent: bool,
    /// 올린 때를 세어 그림판 얻기가 막힐 것 같으면 미룬다(ADR-038). 올리기 방식과 상관없이 쓴다
    pacer: FramePacer,
    /// 창이 보이는지 아는 것. 모르거나 가려졌으면 층을 올리기 전에 그림판부터 얻어 본다(ADR-038)
    visibility: SurfaceVisibility,
    /// 실험 E26(#466): surface를 Display P3로 두고 마지막에 sRGB 값을 P3 값으로 바꿔 쓴다. 꺼져 있으면 없다
    p3: Option<P3Out>,
}

/// E26 변환 패스와 그 앞 중간 텍스처(창 크기, surface와 같은 꼴). 안 (나): 모든 파이프라인(셀·층·사용자 셰이더·덧그림)이 이 텍스처에
/// 그대로 그리고, 마지막에 한 번 바꿔 surface에 쓴다. 그래서 섞기는 늘 sRGB 값에서 일어나 CPU `srgb`와 같다
pub(super) struct P3Out {
    convert: P3Convert,
    mid: Option<(wgpu::Texture, (u32, u32))>,
}

/// `present`의 결과
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentOutcome {
    /// 창에 올렸다
    Presented,
    /// 이번 프레임은 올리지 못했다(창이 가려졌거나 surface를 얻지 못했다). 이유
    Skipped(&'static str),
}

/// surface가 주는 알파 모드 중에서 고른다(#359). 불투명 창은 `Opaque`(없으면 `Auto`).
/// 투명 창은 곱한 알파로 합친 결과를 그대로 내므로 곱한 알파를 받는 `PreMultiplied`를 쓰고,
/// 없으면 `metal`일 때만 `PostMultiplied`를 쓴다: wgpu의 Metal 면은 이 모드에서 레이어를 불투명이 아니게만 하고
/// CoreAnimation이 곱한 알파로 합성한다. 다른 뒷단의 `PostMultiplied`는 곱하지 않은 알파를 받아 쓸 수 없다. 둘 다 없으면 `None`
pub(super) fn pick_alpha_mode(
    modes: &[wgpu::CompositeAlphaMode],
    transparent: bool,
    metal: bool,
) -> Option<wgpu::CompositeAlphaMode> {
    use wgpu::CompositeAlphaMode as M;
    if !transparent {
        return Some(if modes.contains(&M::Opaque) { M::Opaque } else { M::Auto });
    }
    if modes.contains(&M::PreMultiplied) {
        return Some(M::PreMultiplied);
    }
    (metal && modes.contains(&M::PostMultiplied)).then_some(M::PostMultiplied)
}

/// `window-vsync`에서 surface의 올리기 방식을 고른다. `true`면 `Fifo`(화면 갱신에 맞춤, 모든 곳이 지원).
/// `false`면 `Immediate`(맞추지 않고 올림)이고, surface가 주지 않으면 이유를 돌려주고 `Fifo`로 돌아간다
pub(super) fn present_mode_for(
    vsync: bool,
    supported: &[wgpu::PresentMode],
) -> (wgpu::PresentMode, Option<&'static str>) {
    if vsync {
        return (wgpu::PresentMode::Fifo, None);
    }
    if supported.contains(&wgpu::PresentMode::Immediate) {
        return (wgpu::PresentMode::Immediate, None);
    }
    (
        wgpu::PresentMode::Fifo,
        Some("window-vsync = false를 이 화면이 지원하지 않아(Immediate 없음) 화면 갱신에 맞춰 올린다"),
    )
}

/// `FramePacer`의 미룸을 쓰는가. 올리기 방식과 상관없이 늘 쓴다. `Immediate`에서 미룸을 끄고 화면 주기보다 빨리 그림판을 몰아 올리면
/// 마지막 올림이 화면에 붙지 않아 출력 뒤 앞 프레임에 멈췄다(#451, 0.46.0에서 16회 중 14회, 실험 E23). 합성기 안쪽의 까닭은 확인하지 못했다
pub(super) fn pacing_applies(_mode: wgpu::PresentMode) -> bool {
    true
}

impl GpuRenderer {
    /// 창 대상(`Arc<Window>` 등)에 그리는 렌더러를 연다. 어댑터·surface·꼴을 얻지 못하면 이유를 `log::warn!`로 남기고 없다.
    ///
    /// 꼴은 sRGB가 아닌 것(보통 `Bgra8Unorm`): CPU 렌더러는 0~255 값을 감마 없이 섞고 그 값이 그대로 화면에 간다(ADR-035).
    /// 올리기 방식(present mode)은 기본 `Fifo`(`window-vsync = false`면 `Immediate`): `Fifo`는 모든 곳이 지원하고, 화면 주사율에 맞춰 올려 찢김이 없고, 바뀐 것이 없으면 그리지 않으므로
    /// 지금 CPU 길(레이어 contents 교체, 합성기 주사율)과 비슷한 간격이다
    pub fn new_for_window(target: impl Into<wgpu::SurfaceTarget<'static>>, size: (u32, u32)) -> Option<Self> {
        Self::new_for_window_with(target, size, false, true)
    }

    /// `new_for_window`에 투명 창 여부를 더한다(#359). `transparent`면 surface가 곱한 알파를 받는 알파 모드를 주어야 하고,
    /// 주지 않으면 이유를 남기고 없다(앱이 CPU 길로 돌아간다).
    /// `vsync`는 `window-vsync`: `false`면 화면 갱신에 맞추지 않고 올린다(`Immediate`, `present_mode_for`)
    pub fn new_for_window_with(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        size: (u32, u32),
        transparent: bool,
        vsync: bool,
    ) -> Option<Self> {
        Self::open_window(target, size, transparent, vsync, false)
    }

    /// 실험 E26(#466): `new_for_window_with`에 surface를 Display P3로 설정하고 sRGB 값을 P3 값으로 바꿔 쓰는 길을 더한다
    /// (`window-colorspace = srgb`용). surface가 Display P3를 주지 않으면 경고를 남기고 없다(앱이 CPU 길로 돌아간다)
    pub fn new_for_window_srgb_p3(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        size: (u32, u32),
        transparent: bool,
        vsync: bool,
    ) -> Option<Self> {
        Self::open_window(target, size, transparent, vsync, true)
    }

    fn open_window(
        target: impl Into<wgpu::SurfaceTarget<'static>>,
        size: (u32, u32),
        transparent: bool,
        vsync: bool,
        p3: bool,
    ) -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(target)
            .map_err(|e| log::warn!("GPU 창 surface를 만들지 못했다: {e}"))
            .ok()?;
        let Some(adapter) = pick_adapter(&instance, Some(&surface)) else {
            log::warn!("이 창을 그릴 수 있는 GPU 어댑터가 없다");
            return None;
        };
        let caps = surface.get_capabilities(&adapter);
        let Some(format) = caps.formats.iter().copied().find(|f| !f.is_srgb()) else {
            log::warn!("창 surface가 sRGB가 아닌 꼴을 주지 않는다: {:?}", caps.formats);
            return None;
        };
        if p3 && !caps.color_spaces(format).contains(wgpu::SurfaceColorSpaces::DISPLAY_P3) {
            log::warn!(
                "E26 sRGB: 창 surface가 {format:?}에 Display P3를 주지 않는다({:?}): CPU 길로 돌아간다",
                caps.color_spaces(format)
            );
            return None;
        }
        log::info!("창 surface가 주는 알파 모드: {:?}", caps.alpha_modes);
        let Some(alpha_mode) = pick_alpha_mode(&caps.alpha_modes, transparent, cfg!(target_os = "macos")) else {
            log::warn!(
                "투명 창에 쓸 알파 모드(곱한 알파)를 surface가 주지 않는다: {:?}",
                caps.alpha_modes
            );
            return None;
        };
        // `FramePacer`의 줄 깊이와 surface에 청하는 `desired_maximum_frame_latency`는 다른 값이다(#447): surface는 줄 깊이 + 1(최대 2).
        // 다시 설정할 때는 `config`를 그대로 쓰므로 바뀌지 않는다
        let frame_latency = frame_latency_from_env(std::env::var("BT_FRAME_LATENCY").ok().as_deref());
        let surface_latency = surface_latency(frame_latency);
        log::info!(
            "줄 깊이 {frame_latency}, surface 지연 {surface_latency}(그림판 최대 {}장)",
            surface_latency + 1
        );
        let (present_mode, vsync_warning) = present_mode_for(vsync, &caps.present_modes);
        if let Some(why) = vsync_warning {
            log::warn!("{why}: {:?}", caps.present_modes);
        }
        if present_mode == wgpu::PresentMode::Immediate {
            log::info!("화면 갱신 맞춤 끔({present_mode:?})");
        }
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: if p3 {
                wgpu::SurfaceColorSpace::DisplayP3
            } else {
                wgpu::SurfaceColorSpace::Auto
            },
            width: size.0.max(1),
            height: size.1.max(1),
            present_mode,
            desired_maximum_frame_latency: surface_latency,
            alpha_mode,
            view_formats: vec![],
        };
        let me = Self::build(
            adapter,
            format,
            1024,
            Some(WindowSurface {
                surface,
                config,
                stale: false,
                transparent,
                pacer: FramePacer::new(frame_latency as usize, DEFAULT_REFRESH),
                visibility: SurfaceVisibility::default(),
                p3: None,
            }),
        )?;
        if let Some(w) = &me.window {
            w.surface.configure(&me.device, &w.config);
        }
        log::info!("GPU 창 길: {} ({format:?}, {alpha_mode:?})", me.adapter_name());
        if p3 {
            log::info!("E26 sRGB: GPU 길, surface DisplayP3, 변환 나(중간 텍스처 + 마지막 변환 패스)");
            let convert = P3Convert::new(&me.device, format);
            let mut me = me;
            if let Some(w) = me.window.as_mut() {
                w.p3 = Some(P3Out { convert, mid: None });
            }
            return Some(me);
        }
        Some(me)
    }

    /// E26: 중간 텍스처에 늘 하던 대로 그리고(`encode_frame`) 마지막에 변환 패스로 surface에 쓴다
    fn encode_frame_p3(&mut self, surface_view: &wgpu::TextureView, size: (u32, u32)) -> Vec<wgpu::CommandBuffer> {
        let format = self.format;
        let twin = srgb_twin(format);
        let device = self.device.clone();
        let p3 = self
            .window
            .as_mut()
            .and_then(|w| w.p3.as_mut())
            .expect("호출 앞에서 확인했다");
        // 창 크기가 바뀌면 다시 만든다. 선형 섞기용 sRGB 꼴 뷰를 늘 열 수 있게 짝 꼴을 허락해 둔다
        if p3.mid.as_ref().is_none_or(|(_, s)| *s != size) {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bt-gpu p3 mid"),
                size: wgpu::Extent3d {
                    width: size.0,
                    height: size.1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: twin.as_slice(),
            });
            p3.mid = Some((texture, size));
        }
        let Some(win) = self.window.as_ref() else {
            return Vec::new();
        };
        let Some(p3) = win.p3.as_ref() else {
            return Vec::new();
        };
        let Some((mid, _)) = p3.mid.as_ref() else {
            return Vec::new();
        };
        let mid_view = mid.create_view(&wgpu::TextureViewDescriptor::default());
        let srgb = self.frame_srgb_view(mid);
        let mut cbs = self.encode_frame(&mid_view, srgb.as_ref());
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bt-gpu p3 convert"),
        });
        p3.convert.encode(&device, &mut enc, &mid_view, surface_view);
        cbs.push(enc.finish());
        cbs
    }

    /// `alpha-blending`이 선형이면 surface를 sRGB 꼴 뷰도 열 수 있게 다시 설정하도록 표시한다(`present`가 다음 프레임 앞에서 한다).
    /// `native`일 때는 뷰 꼴 목록을 비워 둔다: 지금까지의 설정과 같다
    pub(super) fn sync_window_view_formats(&mut self) {
        let want: Vec<wgpu::TextureFormat> = if self.alpha_blending == AlphaBlending::Native {
            Vec::new()
        } else {
            srgb_twin(self.format).into_iter().collect()
        };
        if let Some(w) = self.window.as_mut() {
            if w.config.view_formats != want {
                w.config.view_formats = want;
                w.stale = true;
            }
        }
    }

    /// 이번 프레임의 surface 텍스처를 sRGB 꼴 뷰로 연다. 선형 섞기가 아니거나 surface가 그 뷰 꼴을 허락하지 않으면 없다
    pub(super) fn frame_srgb_view(&self, texture: &wgpu::Texture) -> Option<wgpu::TextureView> {
        let twin = srgb_twin(self.format)?;
        let allowed = self
            .window
            .as_ref()
            .is_some_and(|w| w.config.view_formats.contains(&twin));
        (self.alpha_blending != AlphaBlending::Native && self.glyph_linear.is_some() && allowed)
            .then(|| srgb_view_of(texture, twin))
    }

    /// 이 창 surface를 곱한 알파로 열었는가(투명 창). 열린 뒤에 설정이 바뀌어도 이 값은 그대로다
    pub fn window_transparent(&self) -> bool {
        self.window.as_ref().is_some_and(|w| w.transparent)
    }

    /// 화면 주사율이 한 장 오르는 간격(`pacing::interval_from_millihertz`)을 알린다. 앱이 창이 있는 화면에서 읽어 넘긴다
    pub fn set_refresh_interval(&mut self, interval: std::time::Duration) {
        if let Some(w) = self.window.as_mut() {
            // 화면이 알려 준 한 장 간격이 바뀔 때만 남긴다(ProMotion·저전력 모드에서 실제로 받는 주사율을 보려고, #429)
            if w.pacer.interval() != interval {
                log::info!("화면 한 장 간격: {:.2}ms", interval.as_secs_f64() * 1000.0);
            }
            w.pacer.set_interval(interval);
        }
    }

    /// 지금 그려 올려도 그림판을 기다리지 않을 것 같은가(ADR-038). `Wait`이면 그 시간 뒤에 다시 그리면 된다.
    /// 그리기 전에 부른다: 미룬 프레임은 아무것도 쓰지 않았으므로 나중에 그릴 때 그때의 최신 내용이 올라간다
    pub fn pace_check(&self, now: Instant) -> PaceDecision {
        self.window
            .as_ref()
            .filter(|w| pacing_applies(w.config.present_mode))
            .map_or(PaceDecision::Go, |w| w.pacer.check(now))
    }

    /// 그림판 하나를 얻는다. `Outdated`·`Lost`는 한 번 다시 설정하고 다시 얻는다. 얻는 데 든 시간을 `acquire_ms`에 더한다
    fn acquire_frame(&mut self, acquire_ms: &mut f64) -> Result<wgpu::SurfaceTexture, &'static str> {
        for attempt in 0..2 {
            let Some(win) = self.window.as_mut() else {
                return Err("창 surface가 없다");
            };
            // Fifo에서 여기서 화면 주사율을 기다릴 수 있어 따로 잰다(ADR-037). 앱이 `pace_check`로 미리 피한다(ADR-038)
            let t_acquire = Instant::now();
            let acquired = win.surface.get_current_texture();
            *acquire_ms += t_acquire.elapsed().as_secs_f64() * 1000.0;
            match acquired {
                wgpu::CurrentSurfaceTexture::Success(f) => return Ok(f),
                wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                    win.stale = true;
                    return Ok(f);
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost if attempt == 0 => {
                    win.surface.configure(&self.device, &win.config);
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    return Err("surface를 다시 설정해도 얻지 못했다");
                }
                wgpu::CurrentSurfaceTexture::Occluded => return Err(OCCLUDED),
                wgpu::CurrentSurfaceTexture::Timeout => return Err("surface를 얻는 데 시간이 넘었다"),
                wgpu::CurrentSurfaceTexture::Validation => return Err("surface 검증 오류"),
            }
        }
        Err("surface를 얻지 못했다")
    }

    /// 올리지 못한 프레임을 마친다. 보이는지 아는 것을 고친다
    fn skipped(&mut self, reason: &'static str) -> PresentOutcome {
        if let Some(w) = self.window.as_mut() {
            w.visibility = w.visibility.after(false, reason == OCCLUDED);
        }
        PresentOutcome::Skipped(reason)
    }

    /// 층과 칸을 합쳐 창에 올린다. 창 크기가 바뀌었으면 surface를 다시 설정한다.
    ///
    /// 창이 보이는지 모르거나 가려졌으면 그림판부터 얻어 본다: 가려진 창에서는 기다리지 않고 바로 돌아오므로(`wgpu-hal` metal
    /// `surface.rs` 359~368줄) 21MB 층을 올리고 나서 건너뛰지 않는다. 이때 `Skipped`면 `layers`는 올라가지 않았다.
    /// 앱은 다음 프레임에 층을 통째로 다시 넘겨야 한다(ADR-038)
    pub fn present(
        &mut self,
        fonts: &mut Fonts,
        size: (u32, u32),
        layers: LayerUpdate,
        panes: &[PaneDraw],
    ) -> PresentOutcome {
        self.present_with(&mut [fonts], size, layers, panes)
    }

    /// `present`. 칸마다 다른 글꼴 묶음(`PaneDraw::font`)으로 그린다(#422)
    pub fn present_with(
        &mut self,
        fonts: &mut [&mut Fonts],
        size: (u32, u32),
        layers: LayerUpdate,
        panes: &[PaneDraw],
    ) -> PresentOutcome {
        self.present_layers(fonts, size, layers, None, panes)
    }

    /// `present_with`. 셰이더 뒤에 얹는 층(`top`, #441)도 받는다. 셰이더가 있으면 셰이더가 돈 뒤에 얹고(칸 밖과 칸 위 덧그림은
    /// 셰이더가 덮지 않는다), 없으면 위층 바로 뒤에 얹는다. `None`이면 앞서 올린 것을 그대로 쓴다
    pub fn present_layers(
        &mut self,
        fonts: &mut [&mut Fonts],
        size: (u32, u32),
        layers: LayerUpdate,
        top: Option<&[u32]>,
        panes: &[PaneDraw],
    ) -> PresentOutcome {
        if size.0 == 0 || size.1 == 0 {
            return PresentOutcome::Skipped("창 크기가 0이다");
        }
        let Some(win) = self.window.as_mut() else {
            return PresentOutcome::Skipped("창 surface가 없다");
        };
        if (win.config.width, win.config.height) != size || win.stale {
            win.config.width = size.0;
            win.config.height = size.1;
            win.stale = false;
            win.surface.configure(&self.device, &win.config);
        }
        let mut acquire_ms = 0.0;
        let probe = win.visibility.probe_before_upload();
        let mut early = None;
        if probe {
            match self.acquire_frame(&mut acquire_ms) {
                Ok(f) => early = Some(f),
                Err(reason) => return self.skipped(reason),
            }
        }
        self.prepare_compose(fonts, size, layers, top, panes);
        let t = Instant::now();
        // 보이는 창에서는 그림판 얻기를 준비(층 올리기·인스턴스) 뒤로 미룬다
        let frame = match early {
            Some(f) => f,
            None => match self.acquire_frame(&mut acquire_ms) {
                Ok(f) => f,
                Err(reason) => return self.skipped(reason),
            },
        };
        let t_encode = Instant::now();
        let surface_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let cbs = if self.window.as_ref().is_some_and(|w| w.p3.is_some()) {
            self.encode_frame_p3(&surface_view, size)
        } else {
            let srgb = self.frame_srgb_view(&frame.texture);
            self.encode_frame(&surface_view, srgb.as_ref())
        };
        let encode_ms = t_encode.elapsed().as_secs_f64() * 1000.0;
        let t_submit = Instant::now();
        self.queue.submit(cbs);
        let submit_ms = t_submit.elapsed().as_secs_f64() * 1000.0;
        let t_present = Instant::now();
        self.queue.present(frame);
        self.timings.present_ms = t_present.elapsed().as_secs_f64() * 1000.0;
        self.timings.acquire_ms = acquire_ms;
        self.timings.encode_ms = encode_ms;
        self.timings.submit_ms = submit_ms;
        self.timings.gpu_ms = t.elapsed().as_secs_f64() * 1000.0;
        if let Some(w) = self.window.as_mut() {
            w.visibility = w.visibility.after(true, false);
            w.pacer
                .record_present_waited(Instant::now(), Duration::from_secs_f64(acquire_ms / 1000.0));
        }
        PresentOutcome::Presented
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wgpu::CompositeAlphaMode as M;
    use wgpu::PresentMode as P;

    #[test]
    fn vsync_on_uses_fifo_without_warning() {
        assert_eq!(present_mode_for(true, &[P::Fifo, P::Immediate]), (P::Fifo, None));
        assert_eq!(present_mode_for(true, &[]), (P::Fifo, None));
    }

    #[test]
    fn vsync_off_uses_immediate_when_supported() {
        assert_eq!(present_mode_for(false, &[P::Fifo, P::Immediate]), (P::Immediate, None));
    }

    #[test]
    fn vsync_off_warns_and_falls_back_to_fifo_when_unsupported() {
        let (mode, warning) = present_mode_for(false, &[P::Fifo, P::Mailbox]);
        assert_eq!(mode, P::Fifo);
        assert!(warning.is_some_and(|w| w.contains("window-vsync")), "{warning:?}");
    }

    #[test]
    fn pacing_applies_to_every_present_mode() {
        // `Immediate`에서 미룸을 끄면 마지막 프레임이 화면에 붙지 않았다(#451)
        for mode in [
            P::Fifo,
            P::FifoRelaxed,
            P::Mailbox,
            P::Immediate,
            P::AutoVsync,
            P::AutoNoVsync,
        ] {
            assert!(pacing_applies(mode), "{mode:?}");
        }
    }

    #[test]
    fn opaque_window_keeps_opaque_mode() {
        assert_eq!(
            pick_alpha_mode(&[M::Opaque, M::PostMultiplied], false, true),
            Some(M::Opaque)
        );
        assert_eq!(pick_alpha_mode(&[M::Inherit], false, false), Some(M::Auto));
    }

    #[test]
    fn transparent_window_prefers_premultiplied() {
        let all = [M::Opaque, M::PostMultiplied, M::PreMultiplied];
        assert_eq!(pick_alpha_mode(&all, true, false), Some(M::PreMultiplied));
        assert_eq!(pick_alpha_mode(&all, true, true), Some(M::PreMultiplied));
    }

    #[test]
    fn metal_post_multiplied_is_used_because_the_layer_composites_premultiplied() {
        // wgpu의 Metal 면은 PostMultiplied일 때 레이어를 불투명이 아니게만 하고, CoreAnimation은 곱한 알파로 합성한다
        assert_eq!(
            pick_alpha_mode(&[M::Opaque, M::PostMultiplied], true, true),
            Some(M::PostMultiplied)
        );
    }

    #[test]
    fn transparent_window_without_a_usable_mode_has_none() {
        // 다른 뒷단의 PostMultiplied는 곱하지 않은 알파를 받으므로 곱한 알파로 합친 결과를 낼 수 없다
        assert_eq!(pick_alpha_mode(&[M::Opaque, M::PostMultiplied], true, false), None);
        assert_eq!(pick_alpha_mode(&[M::Opaque], true, true), None);
        assert_eq!(pick_alpha_mode(&[], true, true), None);
    }
}
