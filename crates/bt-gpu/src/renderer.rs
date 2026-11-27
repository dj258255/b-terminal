//! 창 없는 wgpu 렌더러: 텍스처에 그리고 `Vec<u32>`로 읽어 낸다

use std::time::Instant;

use bt_core::{AlphaBlending, Rgb};
use bt_render::{Fonts, GridView};
use wgpu::util::DeviceExt;

use crate::atlas::Atlas;
use crate::glyphs::GlyphCache;
use crate::scene::{GLYPH_STRIDE, GlyphInst, RECT_STRIDE, RectInst, Scene};

mod compose;
mod custom;
pub use custom::{CompileStages, PixelRect, ShaderRegion, ShaderWaker};
mod layer_diff;
mod memory;
mod window;
pub use compose::{LayerMemory, LayerUpdate, PaneDraw};
pub use memory::GpuMemory;
pub use window::{LayerSrgbHook, PresentOutcome, SrgbPath, srgb_path};

/// 한 프레임의 구간별 시간(ms). 마지막으로 그린 프레임의 값이다
#[derive(Debug, Clone, Copy, Default)]
pub struct FrameTimings {
    /// 인스턴스 만들기와 새 글리프 올리기(CPU)
    pub prepare_ms: f64,
    /// 그리기 명령 제출부터 GPU가 끝낼 때까지
    pub gpu_ms: f64,
    /// 텍스처를 버퍼로 복사해 읽어 `Vec<u32>`로 바꾸기
    pub readback_ms: f64,
    /// 층(아래·위) 픽셀을 GPU 텍스처로 올리기. 올릴 층이 없는 프레임은 0(`compose`·`present`)
    pub upload_ms: f64,
    /// 이번 프레임에 층 텍스처로 올린 바이트(아래층 + 위층). 올리지 않았으면 0
    pub upload_bytes: usize,
    /// 창 길(`present`)에서 그림판을 얻은 시간(`get_current_texture`, 화면 주사율을 기다릴 수 있다)
    pub acquire_ms: f64,
    /// 창 길에서 그리기 명령을 만든 시간
    pub encode_ms: f64,
    /// 창 길에서 명령을 제출한 시간(`queue.submit`)
    pub submit_ms: f64,
    /// 창 길에서 `queue.present`에 든 시간
    pub present_ms: f64,
    /// 이번 프레임에 돈 사용자 셰이더 패스 수. 셰이더가 없으면 0(`compose`·`present`)
    pub shader_passes: usize,
}

/// 그림이 그려지는 텍스처 꼴. sRGB가 아니다: CPU 렌더러는 0~255 값을 그대로 섞으므로(감마 없이)
/// GPU도 그 값 그대로 섞어야 같은 픽셀이 나온다(ADR-035)
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// 같은 텍스처의 sRGB 꼴 뷰(선형 섞기용). 대상 꼴에 sRGB 짝이 없으면 없다
    srgb_view: Option<wgpu::TextureView>,
    readback: wgpu::Buffer,
    width: u32,
    height: u32,
    /// 읽기 버퍼 한 줄의 바이트(256 배수)
    padded_row: u32,
}

struct AtlasTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size: u32,
}

pub struct GpuRenderer {
    adapter_name: String,
    device: wgpu::Device,
    queue: wgpu::Queue,
    rect_pipeline: wgpu::RenderPipeline,
    /// 칸 바탕: 아래 픽셀을 덮어쓴다(곱한 알파로 낸다). 합칠 때 칸 바탕 알파가 아래층과 섞이지 않게(#359)
    rect_replace_pipeline: wgpu::RenderPipeline,
    glyph_pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    globals: wgpu::Buffer,
    gray_tex: AtlasTexture,
    color_tex: AtlasTexture,
    bind_group: wgpu::BindGroup,
    rect_buf: Option<wgpu::Buffer>,
    glyph_buf: Option<wgpu::Buffer>,
    target: Option<Target>,
    cache: GlyphCache,
    scene: Scene,
    bytes: Vec<u8>,
    timings: FrameTimings,
    /// 그리고 나서 아직 읽지 않은 프레임이 있는지
    drawn: bool,
    /// 그리는 대상(텍스처·창 surface)의 꼴
    format: wgpu::TextureFormat,
    layers: compose::Layers,
    window: Option<window::WindowSurface>,
    /// 사용자 셰이더(ADR-040). 없으면 지금 길 그대로
    custom: Option<custom::CustomChain>,
    /// 다음 프레임에 셰이더에 줄 유니폼 값
    shader_uniforms: Box<crate::shadertoy::ShaderUniforms>,
    /// 칸마다 셰이더를 돌릴 칸들과 유니폼(#441). 비었으면 `shader_uniforms`로 창 전체에 한 번
    shader_regions: Vec<custom::ShaderRegion>,
    /// 도는 사용자 셰이더 컴파일(ADR-042). 끝나면 `poll_custom_shaders`가 `custom`에 끼운다
    custom_job: Option<custom::CustomJob>,
    /// 컴파일이 끝났을 때 앱을 깨우는 함수
    shader_waker: Option<custom::ShaderWaker>,
    /// 글자를 섞을 색 공간(#405)
    alpha_blending: AlphaBlending,
    /// 글자 덮음 곡선(#911). 기본은 곡선 없음
    text_composition: bt_core::TextComposition,
    /// 선형 섞기 글리프 파이프라인(`linear`·`linear-corrected`를 처음 고를 때 만든다). 대상의 sRGB 꼴 뷰에 그린다
    glyph_linear: Option<LinearGlyphPipelines>,
    /// 글리프 파이프라인을 만드는 데 쓴 모듈과 레이아웃(선형 파이프라인을 나중에 만들려고 쥔다)
    shader: (wgpu::ShaderModule, wgpu::PipelineLayout),
}

/// 선형 섞기 글리프 파이프라인 둘: `linear`, `linear-corrected`
struct LinearGlyphPipelines {
    plain: wgpu::RenderPipeline,
    corrected: wgpu::RenderPipeline,
}

/// 대상 꼴의 sRGB 짝(같은 텍스처를 sRGB 꼴 뷰로 열어 하드웨어가 선형에서 섞게 한다). 짝이 없으면 선형 섞기를 못 한다
pub(super) fn srgb_twin(format: wgpu::TextureFormat) -> Option<wgpu::TextureFormat> {
    match format {
        wgpu::TextureFormat::Rgba8Unorm => Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        wgpu::TextureFormat::Bgra8Unorm => Some(wgpu::TextureFormat::Bgra8UnormSrgb),
        _ => None,
    }
}

/// 같은 텍스처에 이어 그리는 그리기 패스들. 섞기 공간이 다른 글리프는 sRGB 꼴 뷰에, 나머지는 원래 꼴 뷰에 그리므로
/// 둘이 번갈아 나오면 패스를 바꾼다(앞 패스의 그림은 `Load`로 이어 받는다). 선형 섞기가 아니면 패스는 하나다
pub(super) struct Passes<'a> {
    enc: &'a mut wgpu::CommandEncoder,
    label: &'static str,
    plain: &'a wgpu::TextureView,
    srgb: Option<&'a wgpu::TextureView>,
    bind: &'a wgpu::BindGroup,
    /// 첫 패스를 지울 색. 첫 패스가 열리면 비운다
    clear: Option<wgpu::Color>,
    cur: Option<(bool, wgpu::RenderPass<'static>)>,
}

impl<'a> Passes<'a> {
    pub(super) fn new(
        enc: &'a mut wgpu::CommandEncoder,
        label: &'static str,
        plain: &'a wgpu::TextureView,
        srgb: Option<&'a wgpu::TextureView>,
        bind: &'a wgpu::BindGroup,
        clear: wgpu::Color,
    ) -> Self {
        Self {
            enc,
            label,
            plain,
            srgb,
            bind,
            clear: Some(clear),
            cur: None,
        }
    }

    /// `srgb`가 참이면 sRGB 꼴 뷰에 그리는 패스를 돌려준다. 새로 열린 패스에는 칸 바인드 그룹이 걸려 있다
    pub(super) fn pass(&mut self, srgb: bool) -> &mut wgpu::RenderPass<'static> {
        // 지우기는 원래 꼴 뷰에서 한다: sRGB 꼴 뷰는 지우는 색을 선형 값으로 읽어 sRGB로 바꿔 쓰므로 바탕색이 달라진다
        if srgb && self.clear.is_some() {
            let _ = self.pass(false);
        }
        if self.cur.as_ref().is_none_or(|(s, _)| *s != srgb) {
            self.cur = None;
            let view = if srgb {
                self.srgb.expect("sRGB 뷰 없이 sRGB 패스를 요청했다")
            } else {
                self.plain
            };
            let load = match self.clear.take() {
                Some(c) => wgpu::LoadOp::Clear(c),
                None => wgpu::LoadOp::Load,
            };
            let mut pass = self
                .enc
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(self.label),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            pass.set_bind_group(0, self.bind, &[]);
            self.cur = Some((srgb, pass));
        }
        &mut self.cur.as_mut().expect("방금 열었다").1
    }

    /// 아무것도 그리지 않았어도 첫 패스(지우기)는 연다. 그리기가 끝났으면 부른다
    pub(super) fn finish(mut self) {
        if self.clear.is_some() {
            let _ = self.pass(false);
        }
        self.cur = None;
    }
}

/// 어댑터를 고른다. 진짜 GPU가 없으면 소프트웨어 어댑터(lavapipe 등)를 찾는다. `surface`가 있으면 그것을 그릴 수 있는 것만
fn pick_adapter(instance: &wgpu::Instance, surface: Option<&wgpu::Surface<'_>>) -> Option<wgpu::Adapter> {
    let ask = |fallback: bool| {
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            force_fallback_adapter: fallback,
            compatible_surface: surface,
            ..Default::default()
        }))
    };
    ask(false).or_else(|_| ask(true)).ok()
}

fn atlas_texture(device: &wgpu::Device, size: u32, format: wgpu::TextureFormat, label: &str) -> AtlasTexture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    AtlasTexture { texture, view, size }
}

/// 텍스처를 `format` 꼴 뷰로 연다(같은 텍스처가 `view_formats`에 그 꼴을 가졌어야 한다)
pub(super) fn srgb_view_of(texture: &wgpu::Texture, format: wgpu::TextureFormat) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        format: Some(format),
        ..Default::default()
    })
}

fn blend_over() -> wgpu::BlendState {
    // 곱하지 않은 알파 위에 얹기. CPU의 `blend`와 같다: 결과 = 원본 * a + 바탕 * (1 - a)
    wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::SrcAlpha,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        },
    }
}

impl GpuRenderer {
    /// 창 없이 쓸 장치를 연다. GPU 어댑터가 없으면(소프트웨어 어댑터도 없으면) 없다
    pub fn new_headless() -> Option<Self> {
        Self::with_atlas_size(1024)
    }

    /// 아틀라스 처음 크기(한 변, 픽셀)를 정해 연다. 시험에서 작게 잡아 넘침을 일으킨다
    pub fn with_atlas_size(initial: u32) -> Option<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pick_adapter(&instance, None)?;
        Self::build(adapter, TARGET_FORMAT, initial, None)
    }

    /// 어댑터에서 장치·파이프라인·아틀라스를 만든다. `format`은 그리는 대상의 꼴
    fn build(
        adapter: wgpu::Adapter,
        format: wgpu::TextureFormat,
        initial: u32,
        window: Option<window::WindowSurface>,
    ) -> Option<Self> {
        let info = adapter.get_info();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|e| log::warn!("GPU 장치를 열지 못했다: {e}"))
            .ok()?;
        let max = device.limits().max_texture_dimension_2d;
        let initial = initial.clamp(1, max);

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bt-gpu bind layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                tex_entry(1),
                tex_entry(2),
            ],
        });
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bt-gpu pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bt-gpu cells"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });
        let rect_attrs = wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Unorm8x4];
        let glyph_attrs =
            wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Unorm8x4, 4 => Uint32];
        let pipeline = |label: &str,
                        vs: &str,
                        fs: &str,
                        stride: usize,
                        attrs: &[wgpu::VertexAttribute],
                        blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: stride as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: attrs,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let rect_pipeline = pipeline(
            "bt-gpu rects",
            "rect_vs",
            "rect_fs",
            RECT_STRIDE,
            &rect_attrs,
            Some(blend_over()),
        );
        let rect_replace_pipeline = pipeline(
            "bt-gpu cell backgrounds",
            "rect_vs",
            "rect_fs_premul",
            RECT_STRIDE,
            &rect_attrs,
            None,
        );
        let glyph_pipeline = pipeline(
            "bt-gpu glyphs",
            "glyph_vs",
            "glyph_fs",
            GLYPH_STRIDE,
            &glyph_attrs,
            Some(blend_over()),
        );

        let globals = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("bt-gpu globals"),
            contents: &[0u8; 16],
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let cache = GlyphCache::new(initial, max);
        let gray_tex = atlas_texture(
            &device,
            cache.gray.size(),
            wgpu::TextureFormat::R8Unorm,
            "bt-gpu gray atlas",
        );
        let color_tex = atlas_texture(
            &device,
            cache.color.size(),
            wgpu::TextureFormat::Rgba8Unorm,
            "bt-gpu color atlas",
        );
        let bind_group = make_bind_group(&device, &layout, &globals, &gray_tex, &color_tex);
        let layers = compose::Layers::new(&device, format);
        Some(Self {
            adapter_name: info.name,
            device,
            queue,
            rect_pipeline,
            rect_replace_pipeline,
            glyph_pipeline,
            layout,
            globals,
            gray_tex,
            color_tex,
            bind_group,
            rect_buf: None,
            glyph_buf: None,
            target: None,
            cache,
            scene: Scene::default(),
            bytes: Vec::new(),
            timings: FrameTimings::default(),
            drawn: false,
            layers,
            format,
            window,
            custom: None,
            shader_uniforms: Box::default(),
            shader_regions: Vec::new(),
            custom_job: None,
            shader_waker: None,
            alpha_blending: AlphaBlending::Native,
            text_composition: bt_core::TextComposition::NONE,
            glyph_linear: None,
            shader: (module, pl),
        })
    }

    /// 글자를 섞을 색 공간을 정한다(`alpha-blending`). 다음 프레임부터 바뀐다.
    /// 대상 꼴에 sRGB 짝이 없으면 `native`로 남는다. 사용자 셰이더가 도는 동안에는 `native`로 그린다
    pub fn set_alpha_blending(&mut self, mode: AlphaBlending) {
        if mode != AlphaBlending::Native && self.glyph_linear.is_none() {
            match self.build_linear_glyphs() {
                Some(p) => self.glyph_linear = Some(p),
                None => {
                    log::warn!(
                        "alpha-blending: 이 대상 꼴({:?})은 선형 섞기를 못 한다. native로 그린다",
                        self.format
                    );
                    self.alpha_blending = AlphaBlending::Native;
                    return;
                }
            }
        }
        self.alpha_blending = mode;
        // 창 surface는 sRGB 꼴 뷰를 열 수 있게 다시 설정한다(`present`가 다음 프레임 앞에서 한다)
        self.sync_window_view_formats();
    }

    /// 글자 덮음 곡선을 정한다(`text-composition-strategy`, #911). 다음 프레임부터 바뀐다. 셰이더 유니폼 값만 바뀐다
    pub fn set_text_composition(&mut self, c: bt_core::TextComposition) {
        self.text_composition = c;
    }

    /// 전역 유니폼 16바이트: 그리는 크기와 글자 덮음 곡선(1/감마, 1 + 명암/100. 곡선 없음이면 둘 다 정확히 1)
    pub(super) fn globals_bytes(&self, w: f32, h: f32) -> Vec<u8> {
        let c = self.text_composition;
        let text = if c.is_identity() {
            [1.0, 1.0]
        } else {
            [1.0 / c.gamma, 1.0 + c.contrast * 0.01]
        };
        [w, h, text[0], text[1]].iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    /// 지금 고른 섞기 공간으로 글리프를 그릴 파이프라인과, 그것이 sRGB 꼴 뷰에 그려야 하는가
    pub(super) fn glyph_draw(&self, srgb_view_available: bool) -> (&wgpu::RenderPipeline, bool) {
        match (self.alpha_blending, &self.glyph_linear, srgb_view_available) {
            (AlphaBlending::Linear, Some(p), true) => (&p.plain, true),
            (AlphaBlending::LinearCorrected, Some(p), true) => (&p.corrected, true),
            _ => (&self.glyph_pipeline, false),
        }
    }

    /// 선형 섞기용 글리프 파이프라인(대상의 sRGB 꼴). 짝이 없으면 없다
    fn build_linear_glyphs(&self) -> Option<LinearGlyphPipelines> {
        let format = srgb_twin(self.format)?;
        let (module, pl) = &self.shader;
        let attrs =
            wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Unorm8x4, 4 => Uint32];
        let make = |label: &str, fs: &str| {
            self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(pl),
                vertex: wgpu::VertexState {
                    module,
                    entry_point: Some("glyph_vs"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: GLYPH_STRIDE as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &attrs,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(blend_over()),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        Some(LinearGlyphPipelines {
            plain: make("bt-gpu glyphs linear", "glyph_fs_linear"),
            corrected: make("bt-gpu glyphs linear-corrected", "glyph_fs_corrected"),
        })
    }

    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// 아틀라스(회색조·색)를 키운 횟수의 합
    pub fn atlas_grow_count(&self) -> u32 {
        self.cache.gray.grows() + self.cache.color.grows()
    }

    /// 마지막 프레임의 구간별 시간
    pub fn timings(&self) -> FrameTimings {
        self.timings
    }

    /// 격자를 그려 0RGB(맨 위 바이트는 불투명 알파 ff, CPU `Canvas`와 같은 꼴) 픽셀로 읽어 낸다.
    /// 그림 크기는 `글꼴 셀 크기 × (열, 줄)`이다. `clear`는 바탕색
    pub fn render(&mut self, fonts: &mut Fonts, view: &GridView, clear: Rgb) -> Vec<u32> {
        self.draw(fonts, view, clear);
        self.read_pixels()
    }

    /// 격자를 내부 텍스처에 그리고 GPU가 끝낼 때까지 기다린다. 읽지는 않는다(`read_pixels`)
    pub fn draw(&mut self, fonts: &mut Fonts, view: &GridView, clear: Rgb) {
        let t0 = Instant::now();
        let m = fonts.metrics();
        let (w, h) = (
            (m.width * view.cols as u32).max(1),
            (m.height * view.rows as u32).max(1),
        );
        self.ensure_target(w, h);
        self.cache.begin_frame(&[crate::glyphs::SizeSig::of(fonts)]);
        self.scene.build(&mut self.cache, fonts, view, clear);
        self.sync_atlases();

        self.queue
            .write_buffer(&self.globals, 0, &self.globals_bytes(w as f32, h as f32));
        self.bytes.clear();
        for r in &self.scene.rects {
            r.write(&mut self.bytes);
        }
        let rect_bytes = self.bytes.len();
        for g in &self.scene.glyphs {
            g.write(&mut self.bytes);
        }
        upload_instances(
            &self.device,
            &self.queue,
            &mut self.rect_buf,
            &self.bytes[..rect_bytes],
            "bt-gpu rect instances",
        );
        upload_instances(
            &self.device,
            &self.queue,
            &mut self.glyph_buf,
            &self.bytes[rect_bytes..],
            "bt-gpu glyph instances",
        );
        let prepare = t0.elapsed();

        let t1 = Instant::now();
        let target = self.target.as_ref().expect("ensure_target이 만들었다");
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bt-gpu frame"),
        });
        {
            // 투명 창이면 투명으로 지우고 칸이 화면을 덮는다(#353, Scene::is_transparent)
            let clear_color = if self.scene.is_transparent() {
                wgpu::Color::TRANSPARENT
            } else {
                wgpu::Color {
                    r: clear.r as f64 / 255.0,
                    g: clear.g as f64 / 255.0,
                    b: clear.b as f64 / 255.0,
                    a: 1.0,
                }
            };
            let (glyph_pipeline, glyphs_srgb) = self.glyph_draw(target.srgb_view.is_some());
            let mut passes = Passes::new(
                &mut enc,
                "bt-gpu cells",
                &target.view,
                target.srgb_view.as_ref(),
                &self.bind_group,
                clear_color,
            );
            let sc = &self.scene;
            let draw_rects = |passes: &mut Passes<'_>, range: std::ops::Range<usize>| {
                if let (Some(buf), false) = (&self.rect_buf, range.is_empty()) {
                    let pass = passes.pass(false);
                    pass.set_pipeline(&self.rect_pipeline);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..6, range.start as u32..range.end as u32);
                }
            };
            let draw_glyphs = |passes: &mut Passes<'_>, range: std::ops::Range<usize>| {
                if let (Some(buf), false) = (&self.glyph_buf, range.is_empty()) {
                    let pass = passes.pass(glyphs_srgb);
                    pass.set_pipeline(glyph_pipeline);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..6, range.start as u32..range.end as u32);
                }
            };
            draw_rects(&mut passes, 0..sc.rects_pre);
            draw_glyphs(&mut passes, 0..sc.glyphs_pre);
            draw_rects(&mut passes, sc.rects_pre..sc.rects.len());
            draw_glyphs(&mut passes, sc.glyphs_pre..sc.glyphs.len());
            passes.finish();
        }
        self.queue.submit([enc.finish()]);
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        self.timings = FrameTimings {
            prepare_ms: prepare.as_secs_f64() * 1000.0,
            gpu_ms: t1.elapsed().as_secs_f64() * 1000.0,
            readback_ms: 0.0,
            upload_ms: 0.0,
            ..FrameTimings::default()
        };
        self.drawn = true;
    }

    /// 마지막으로 그린 프레임을 읽어 낸다. 아직 그린 것이 없으면 빈 값
    pub fn read_pixels(&mut self) -> Vec<u32> {
        let t = Instant::now();
        let (Some(target), true) = (&self.target, self.drawn) else {
            return Vec::new();
        };
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bt-gpu readback"),
        });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &target.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(target.padded_row),
                    rows_per_image: Some(target.height),
                },
            },
            wgpu::Extent3d {
                width: target.width,
                height: target.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit([enc.finish()]);
        let slice = target.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        let transparent = self.scene.is_transparent();
        let mut out = Vec::new();
        let mapped = rx.recv().is_ok_and(|r| r.is_ok());
        if let (true, Ok(data)) = (mapped, slice.get_mapped_range()) {
            let (w, h) = (target.width as usize, target.height as usize);
            // 대상이 Bgra면 바이트 차례가 0xAARRGGBB 그대로(B, G, R)이고, Rgba면 R, G, B
            let bgra = matches!(
                self.format,
                wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
            );
            out.reserve(w * h);
            for row in 0..h {
                let line = &data[row * target.padded_row as usize..][..w * 4];
                out.extend(line.chunks_exact(4).map(|p| {
                    // 투명 창이면 텍스처의 곱한 알파를 그대로 낸다(CPU `Canvas`와 같은 꼴, #353)
                    let alpha = if transparent { p[3] as u32 } else { 0xff };
                    let (r, b) = if bgra { (p[2], p[0]) } else { (p[0], p[2]) };
                    alpha << 24 | (r as u32) << 16 | (p[1] as u32) << 8 | b as u32
                }));
            }
            drop(data);
        } else {
            log::warn!("그림을 읽어 내지 못했다");
        }
        target.readback.unmap();
        self.timings.readback_ms = t.elapsed().as_secs_f64() * 1000.0;
        out
    }

    fn ensure_target(&mut self, width: u32, height: u32) {
        let twin = srgb_twin(self.format);
        if self
            .target
            .as_ref()
            .is_some_and(|t| t.width == width && t.height == height)
        {
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bt-gpu target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: twin.as_slice(),
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let srgb_view = twin.map(|f| srgb_view_of(&texture, f));
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_row = (width * 4).div_ceil(align) * align;
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bt-gpu readback"),
            size: padded_row as u64 * height as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.target = Some(Target {
            texture,
            view,
            srgb_view,
            readback,
            width,
            height,
            padded_row,
        });
        self.drawn = false;
    }

    /// 캐시에 새로 채운 글리프를 GPU 텍스처로 올린다. 아틀라스가 커졌으면 텍스처를 새로 만들어 통째로 올린다
    fn sync_atlases(&mut self) {
        let mut rebuilt = [false; 2];
        for color in [false, true] {
            let (atlas, tex, format, label) = if color {
                (
                    &self.cache.color,
                    &mut self.color_tex,
                    wgpu::TextureFormat::Rgba8Unorm,
                    "bt-gpu color atlas",
                )
            } else {
                (
                    &self.cache.gray,
                    &mut self.gray_tex,
                    wgpu::TextureFormat::R8Unorm,
                    "bt-gpu gray atlas",
                )
            };
            if atlas.size() != tex.size {
                *tex = atlas_texture(&self.device, atlas.size(), format, label);
                write_whole(&self.queue, tex, atlas, if color { 4 } else { 1 });
                rebuilt[color as usize] = true;
            }
        }
        if rebuilt.contains(&true) {
            self.bind_group = make_bind_group(
                &self.device,
                &self.layout,
                &self.globals,
                &self.gray_tex,
                &self.color_tex,
            );
            self.log_memory("아틀라스를 새로 만들었다");
        }
        for u in self.cache.uploads.drain(..) {
            let (atlas, tex, bpp) = if u.color {
                (&self.cache.color, &self.color_tex, 4usize)
            } else {
                (&self.cache.gray, &self.gray_tex, 1usize)
            };
            // 방금 통째로 올린 아틀라스에는 이미 들어 있다
            if rebuilt[u.color as usize] {
                continue;
            }
            let stride = atlas.size() as usize * bpp;
            let mut px = Vec::with_capacity(u.w as usize * u.h as usize * bpp);
            for r in 0..u.h as usize {
                let at = (u.y as usize + r) * stride + u.x as usize * bpp;
                px.extend_from_slice(&atlas.data()[at..at + u.w as usize * bpp]);
            }
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tex.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: u.x, y: u.y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &px,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(u.w * bpp as u32),
                    rows_per_image: Some(u.h),
                },
                wgpu::Extent3d {
                    width: u.w,
                    height: u.h,
                    depth_or_array_layers: 1,
                },
            );
        }
    }
}

fn tex_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: false },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

fn make_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    gray: &AtlasTexture,
    color: &AtlasTexture,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bt-gpu bind group"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&gray.view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&color.view),
            },
        ],
    })
}

fn write_whole(queue: &wgpu::Queue, tex: &AtlasTexture, atlas: &Atlas, bpp: u32) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &tex.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        atlas.data(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(atlas.size() * bpp),
            rows_per_image: Some(atlas.size()),
        },
        wgpu::Extent3d {
            width: atlas.size(),
            height: atlas.size(),
            depth_or_array_layers: 1,
        },
    );
}

/// 인스턴스 버퍼에 올린다. 모자라면 두 배씩 키워 새로 만든다. 빈 목록이면 아무것도 하지 않는다
fn upload_instances(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    buf: &mut Option<wgpu::Buffer>,
    data: &[u8],
    label: &str,
) {
    if data.is_empty() {
        return;
    }
    let need = data.len() as u64;
    if buf.as_ref().is_none_or(|b| b.size() < need) {
        let size = (buf.as_ref().map_or(0, |b| b.size()) * 2).max(need).next_multiple_of(4);
        *buf = Some(device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }
    if let Some(b) = buf {
        queue.write_buffer(b, 0, data);
    }
}

// 인스턴스 꼴이 바뀌면 셰이더와 맞춰야 한다
const _: () = assert!(std::mem::size_of::<RectInst>() == RECT_STRIDE);
const _: () = assert!(std::mem::size_of::<GlyphInst>() == GLYPH_STRIDE);

/// 칸 알파(#353). 설정은 `Scene`이 들고 있다
impl GpuRenderer {
    /// 투명 창(CPU `Canvas::set_see_through`와 같다): 앞으로 `bg` 바탕은 `alpha`만큼만 덮는다.
    /// 이 모드(`set_cells_alpha`도 같다)에서는 `render`의 `clear`를 쓰지 않고 투명으로 지우며,
    /// 읽어 낸 픽셀은 곱한 알파 ARGB(맨 앞 바이트가 알파, 색은 알파를 곱한 값)다
    pub fn set_see_through(&mut self, bg: Rgb, alpha: u8) {
        self.scene.set_see_through(bg, alpha);
    }

    /// 투명 창을 끈다(다시 불투명 `clear`로 지우고 알파는 ff)
    pub fn clear_see_through(&mut self) {
        self.scene.clear_see_through();
    }

    /// 배경 그림 층(CPU `Canvas::set_backdrop`와 같다): 아래층에 그림이 깔려 있으니 `bg` 바탕 칸은 그리지 않고 그림이 보이게 둔다.
    /// 없애려면 `None`
    pub fn set_backdrop(&mut self, bg: Option<Rgb>) {
        self.scene.set_backdrop(bg);
    }

    /// `background-opacity-cells`(CPU `Canvas::set_cells_alpha`와 같다): 색 바탕 칸의 알파
    pub fn set_cells_alpha(&mut self, alpha: Option<u8>) {
        self.scene.set_cells_alpha(alpha);
    }
}
