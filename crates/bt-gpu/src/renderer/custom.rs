//! 사용자 셰이더 패스(ADR-040): 합친 화면을 중간 텍스처에 그리고, 셰이더를 차례로 돌려 마지막 것을 대상에 그린다.
//! Ghostty `CustomShaderState`(`generic.zig`)처럼 앞뒤 텍스처를 번갈아 쓴다: 앞 출력이 다음 `iChannel0`이다.
//! 셰이더가 없으면 이 파일의 길을 타지 않는다(`encode_frame`이 `encode_compose`를 그대로 부른다).
//!
//! 칸마다 돈다(#441, Ghostty는 분할(surface)마다 렌더러가 따로다): 앱이 칸마다 `ShaderRegion`(칸 사각형과 그 칸의 유니폼)을 주면
//! 합친 화면에서 그 사각형을 칸 크기 텍스처로 떼어 셰이더 사슬을 돌리고 제자리에 그린다. 칸 밖(탭 막대, 칸 사이 틈)은 셰이더가
//! 덮지 않는다. 사각형이 창 전체와 같으면(칸 하나, 탭 막대 없음) 떼지 않고 지금처럼 곧바로 돈다

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Instant;

use super::GpuRenderer;
use crate::shadertoy::{self, CustomShaderSource, ShaderUniforms, UNIFORM_BLOCK_SIZE};

/// 화면 전체를 덮는 삼각형 하나(Ghostty `shaders.metal`의 `full_screen_vertex`와 같은 꼴). 위치만 낸다
const FULL_SCREEN_VS: &str = "@vertex
fn vs(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(p[vi], 0.0, 1.0);
}";

/// 창 픽셀의 사각형(왼쪽 위와 크기)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

/// 셰이더가 도는 칸 하나(#441): 칸을 가리키는 `key`(칸 id), 합친 화면에서 셰이더에 줄 영역(Ghostty의 surface 전체, 여백 포함),
/// 그 칸의 유니폼 값. `rect`가 없으면 창 전체다. `iResolution`은 영역 크기로 렌더러가 채운다
#[derive(Debug, Clone, Copy)]
pub struct ShaderRegion {
    pub key: u64,
    pub rect: Option<PixelRect>,
    pub uniforms: ShaderUniforms,
}

/// 창에 맞게 잘라 낸 이번 프레임의 칸 하나
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Planned {
    pub key: u64,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// 칸들을 창 크기로 자르고(창 밖은 버린다) 빈 칸과 겹친 key를 걸러 낸다. 순수 함수
pub(super) fn plan_regions(window: (u32, u32), regions: &[ShaderRegion]) -> Vec<Planned> {
    let mut out: Vec<Planned> = Vec::new();
    for r in regions {
        let (x0, y0, x1, y1) = match r.rect {
            None => (0i64, 0i64, window.0 as i64, window.1 as i64),
            Some(p) => (
                (p.x as i64).clamp(0, window.0 as i64),
                (p.y as i64).clamp(0, window.1 as i64),
                (p.x as i64 + p.w as i64).clamp(0, window.0 as i64),
                (p.y as i64 + p.h as i64).clamp(0, window.1 as i64),
            ),
        };
        if x1 <= x0 || y1 <= y0 || out.iter().any(|o| o.key == r.key) {
            continue;
        }
        out.push(Planned {
            key: r.key,
            x: x0 as u32,
            y: y0 as u32,
            w: (x1 - x0) as u32,
            h: (y1 - y0) as u32,
        });
    }
    out
}

/// 칸 하나가 창 전체를 덮는가(그러면 떼지 않고 지금처럼 곧바로 돈다)
pub(super) fn covers_window(window: (u32, u32), plan: &[Planned]) -> bool {
    matches!(plan, [p] if (p.x, p.y, p.w, p.h) == (0, 0, window.0, window.1))
}

/// 같은 크기의 칸이 나눠 쓰는 중간 텍스처. 칸들은 한 인코더 안에서 차례로 돌므로(칸마다 떼어 오기 → 패스들이 읽고 쓰기 → 다음 칸)
/// 같은 크기끼리 한 벌을 돌려 써도 서로 덮지 않는다. 셰이더가 하나면 앞 텍스처만 있다(그 출력은 대상에 곧바로 그린다).
/// 둘 이상이면 앞·뒤를 번갈아 쓴다
struct SharedTex {
    size: (u32, u32),
    textures: Vec<wgpu::Texture>,
    views: Vec<wgpu::TextureView>,
}

/// 칸 하나의 바인드 그룹과 유니폼 버퍼. 칸 크기가 바뀔 때만 다시 만들고 칸이 사라지면 놓는다. 텍스처는 `SharedTex`가 쥔다
/// (칸마다 가진 것은 가리키는 바인드 그룹뿐이고, 칸 크기와 같은 `SharedTex`는 이 칸이 있는 동안 남는다)
struct RegionRes {
    key: u64,
    size: (u32, u32),
    /// 마지막 패스의 유니폼: `_bt_origin`이 칸의 대상 자리다. 칸마다 따로다: 한 버퍼를 쓰면 제출 전에 마지막으로 쓴 값이 모든 칸에 닿는다
    last_uniforms: wgpu::Buffer,
    /// 마지막 패스가 읽는 바인드 그룹(`views[(셰이더 수 - 1) % 2]`와 `last_uniforms`)
    last_group: wgpu::BindGroup,
    /// 가운데 패스(셰이더가 둘 이상일 때만)의 유니폼과 바인드 그룹: 칸 크기 텍스처에 그리므로 `_bt_origin`은 (0, 0)이다.
    /// `mid_groups[i]`는 `views[i]`를 `iChannel0`으로 읽는다
    mid: Option<(wgpu::Buffer, [wgpu::BindGroup; 2])>,
}

/// 합친 화면을 담는 창 크기 텍스처(칸이 창 전체를 덮지 않을 때만). 칸 영역을 여기서 뗀다
struct SceneTex {
    size: (u32, u32),
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    blit_group: wgpu::BindGroup,
}

/// 컴파일에 성공한 셰이더들(적은 순서)과 패스가 쓰는 것
pub(super) struct CustomChain {
    layout: wgpu::BindGroupLayout,
    pipelines: Vec<wgpu::RenderPipeline>,
    /// Ghostty와 같다: 선형 필터, 가장자리 고정(`Metal.zig` 292~300줄)
    sampler: wgpu::Sampler,
    blit_layout: wgpu::BindGroupLayout,
    blit_pipeline: wgpu::RenderPipeline,
    regions: Vec<RegionRes>,
    /// 칸 크기별 중간 텍스처(같은 크기 칸이 나눠 쓴다)
    pool: Vec<SharedTex>,
    scene: Option<SceneTex>,
    /// 이번 프레임에 도는 칸들(`prepare_custom`이 정한다)
    plan: Vec<Planned>,
    /// 이번 프레임의 칸이 창 전체를 덮는가
    full: bool,
}

const BLIT_WGSL: &str = "
@group(0) @binding(0) var src: texture_2d<f32>;
struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
}
@vertex
fn blit_vs(@builtin(vertex_index) vi: u32) -> VOut {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var o: VOut;
    o.pos = vec4<f32>(p[vi], 0.0, 1.0);
    o.uv = vec2<f32>(p[vi].x * 0.5 + 0.5, 0.5 - p[vi].y * 0.5);
    return o;
}
@fragment
fn blit_fs(i: VOut) -> @location(0) vec4<f32> {
    // 뷰포트가 칸 사각형이라 uv가 칸을 0~1로 덮는다. 칸 크기 텍스처를 1:1로 읽는다(늘이거나 섞지 않는다)
    let d = vec2<i32>(textureDimensions(src));
    return textureLoad(src, min(vec2<i32>(i.uv * vec2<f32>(d)), d - vec2<i32>(1)), 0);
}";

fn texture_2d(
    device: &wgpu::Device,
    label: &str,
    (w, h): (u32, u32),
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

fn layout_entries() -> [wgpu::BindGroupLayoutEntry; 3] {
    let fragment = wgpu::ShaderStages::FRAGMENT;
    [
        wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: fragment,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: fragment,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
        wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: fragment,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ]
}

/// 컴파일 한 번의 결과: 쓸 체인(성공한 것이 하나도 없으면 없다)과 건너뛴 셰이더의 오류 문구
pub(super) struct Compiled {
    chain: Option<CustomChain>,
    errors: Vec<String>,
}

/// naga 읽기·검증을 통과한 셰이더(메인 스레드에서 검사하고 컴파일 스레드로 넘긴다)
struct Checked {
    name: String,
    module: wgpu::naga::Module,
    naga_ms: f64,
}

/// 셰이더를 naga로 읽고 검증한다(1ms 안팎). 통과한 것과 건너뛴 것의 오류 문구로 나눈다
fn check_all(shaders: &[CustomShaderSource]) -> (Vec<Checked>, Vec<String>) {
    let mut ok = Vec::new();
    let mut errors = Vec::new();
    for s in shaders {
        let t = Instant::now();
        match shadertoy::check(&s.name, &s.glsl) {
            Ok(module) => ok.push(Checked {
                name: s.name.clone(),
                module,
                naga_ms: ms(t),
            }),
            Err(e) => {
                log::warn!("사용자 셰이더를 건너뛴다: {e}");
                errors.push(e);
            }
        }
    }
    (ok, errors)
}

/// 컴파일 한 번이 돌고 있는 일. 결과는 채널로 오고, 이 값을 버리면(다시 걸거나 렌더러가 사라지면) 늦게 온 결과는 버려진다
pub(super) struct CustomJob {
    rx: Receiver<Compiled>,
    started: Instant,
}

/// 컴파일이 끝났을 때 앱을 깨우는 함수(어느 스레드에서든 불린다)
pub type ShaderWaker = Arc<dyn Fn() + Send + Sync>;

/// 셰이더 하나를 컴파일하는 단계별 시간(ms). 어느 단계가 큰지 가르려고 재서 로그에 남긴다
#[derive(Debug, Clone, Copy, Default)]
pub struct CompileStages {
    /// naga 읽기와 검증
    pub naga_ms: f64,
    /// `create_shader_module`
    pub module_ms: f64,
    /// `create_render_pipeline`
    pub pipeline_ms: f64,
    /// 오류 범위를 닫고 결과를 기다림
    pub scope_ms: f64,
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

/// 레이아웃과 정점 모듈(셰이더마다 같다)
fn shared_parts(device: &wgpu::Device) -> (wgpu::BindGroupLayout, wgpu::PipelineLayout, wgpu::ShaderModule) {
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bt-gpu custom shader layout"),
        entries: &layout_entries(),
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("bt-gpu custom shader pipeline layout"),
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    let vs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bt-gpu custom shader vertex"),
        source: wgpu::ShaderSource::Wgsl(FULL_SCREEN_VS.into()),
    });
    (layout, pl, vs)
}

/// 검사를 통과한 셰이더들로 파이프라인과 체인을 만든다. 장치만 쓰므로 어느 스레드에서든 돈다
fn build_chain(device: &wgpu::Device, format: wgpu::TextureFormat, shaders: Vec<Checked>) -> Compiled {
    let mut errors = Vec::new();
    let (layout, pl, vs) = shared_parts(device);
    let mut pipelines = Vec::new();
    for s in shaders {
        let t = Instant::now();
        let mut st = CompileStages {
            naga_ms: s.naga_ms,
            ..CompileStages::default()
        };
        match compile_pass(device, format, &s.name, s.module, &pl, &vs, &mut st) {
            Ok(p) => {
                log::info!(
                    "사용자 셰이더 {}: 컴파일 {:.1}ms (읽기·검증 {:.1}, 모듈 {:.1}, 파이프라인 {:.1}, 오류 범위 {:.1})",
                    s.name,
                    ms(t) + st.naga_ms,
                    st.naga_ms,
                    st.module_ms,
                    st.pipeline_ms,
                    st.scope_ms
                );
                pipelines.push(p);
            }
            Err(e) => {
                log::warn!("사용자 셰이더를 건너뛴다: {e}");
                errors.push(e);
            }
        }
    }
    let chain = (!pipelines.is_empty()).then(|| {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bt-gpu custom shader sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bt-gpu custom blit layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let blit_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bt-gpu custom blit pipeline layout"),
            bind_group_layouts: &[Some(&blit_layout)],
            immediate_size: 0,
        });
        let blit_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bt-gpu custom blit"),
            source: wgpu::ShaderSource::Wgsl(BLIT_WGSL.into()),
        });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bt-gpu custom blit"),
            layout: Some(&blit_pl),
            vertex: wgpu::VertexState {
                module: &blit_module,
                entry_point: Some("blit_vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_module,
                entry_point: Some("blit_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    // 칸 그림이 그 자리를 덮는다(셰이더 출력의 알파 포함, 섞지 않는다)
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        CustomChain {
            layout,
            pipelines,
            sampler,
            blit_layout,
            blit_pipeline,
            regions: Vec::new(),
            pool: Vec::new(),
            scene: None,
            plan: Vec::new(),
            full: false,
        }
    });
    Compiled { chain, errors }
}

/// 검사를 통과한 셰이더 하나를 파이프라인으로 만든다. naga가 먼저 막았어도 wgpu가 오류를 내면(바인딩이 안 맞는 것 등)
/// 오류 범위로 잡아 문구로 돌려준다: 잘못된 셰이더가 앱을 패닉시키면 안 된다.
/// 오류 범위는 스레드마다 따로라(wgpu `ErrorScopeGuard`) 컴파일 스레드의 범위가 메인 스레드의 그리기 오류를 잡지 않는다
fn compile_pass(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    name: &str,
    module: wgpu::naga::Module,
    pl: &wgpu::PipelineLayout,
    vs: &wgpu::ShaderModule,
    st: &mut CompileStages,
) -> Result<wgpu::RenderPipeline, String> {
    let t = Instant::now();
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let fs = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(name),
        source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)),
    });
    st.module_ms = ms(t);
    let t = Instant::now();
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(name),
        layout: Some(pl),
        vertex: wgpu::VertexState {
            module: vs,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &fs,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                // 셰이더 출력이 그대로 대상이 된다(알파 포함, 섞지 않는다)
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    });
    st.pipeline_ms = ms(t);
    let t = Instant::now();
    let err = pollster::block_on(scope.pop());
    st.scope_ms = ms(t);
    match err {
        Some(e) => Err(format!("{name}: {e}")),
        None => Ok(pipeline),
    }
}

/// 측정용: 셰이더 하나의 단계별 시간
fn stages_of(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    s: &CustomShaderSource,
) -> Result<CompileStages, String> {
    let (_layout, pl, vs) = shared_parts(device);
    let t = Instant::now();
    let module = shadertoy::check(&s.name, &s.glsl)?;
    let mut st = CompileStages {
        naga_ms: ms(t),
        ..CompileStages::default()
    };
    compile_pass(device, format, &s.name, module, &pl, &vs, &mut st)?;
    Ok(st)
}

impl GpuRenderer {
    /// 사용자 셰이더를 건다(ADR-042). GLSL 읽기·검증(naga, 1ms 안팎)은 이 자리에서 하고, 건너뛴 셰이더의 오류 문구를 곧바로
    /// 돌려준다. 통과한 셰이더의 파이프라인 만들기(처음 보는 셰이더는 0.15~0.35초)는 다른 스레드에서 돌고, 결과는
    /// `poll_custom_shaders`가 받아 체인에 끼운다. 그 전에는 지금 길(셰이더가 없으면 셰이더 없이)로 그린다. 도는 컴파일이 있으면
    /// 그 결과는 버린다. 빈 목록이거나 통과한 셰이더가 없으면 곧바로 끄고 지금 길로 돌아간다
    pub fn set_custom_shaders(&mut self, shaders: &[CustomShaderSource]) -> Vec<String> {
        self.custom_job = None;
        if shaders.is_empty() {
            self.custom = None;
            return Vec::new();
        }
        // GLSL 읽기·검증은 여기서(1ms 안팎): 오류를 곧바로 돌려줘 설정 다시 읽기의 답에 보이게 한다
        let (checked, errors) = check_all(shaders);
        if checked.is_empty() {
            self.custom = None;
            return errors;
        }
        let (tx, rx) = mpsc::channel();
        let started = Instant::now();
        log::info!("사용자 셰이더 컴파일 시작: {}개", checked.len());
        let device = self.device.clone();
        let format = self.format;
        let shaders = checked;
        let wake = self.shader_waker.clone();
        self.custom_job = Some(CustomJob { rx, started });
        // wgpu `Device`는 `Send + Sync`이고 복제하면 같은 장치다(wgpu 30 `api/device.rs` 24줄 `assert_impl_all!(Device: Send, Sync)`,
        // 19줄 `#[derive(Clone)]`). 만든 파이프라인도 채널로 넘긴다. 스레드를 못 만들면 그 오류를 결과로 보낸다
        let run = move || {
            let r = build_chain(&device, format, shaders);
            if tx.send(r).is_ok() {
                if let Some(w) = wake {
                    w();
                }
            }
        };
        if let Err(e) = std::thread::Builder::new().name("bt-shader-compile".into()).spawn(run) {
            // 클로저(와 보내는 쪽)가 버려지므로 `poll_custom_shaders`가 `Disconnected`로 받아 오류로 알리고 셰이더를 끈다
            log::warn!("셰이더 컴파일 스레드를 만들지 못했다: {e}");
        }
        errors
    }

    /// 컴파일이 끝났으면 결과를 체인에 끼우고 건너뛴 셰이더의 오류 문구를 돌려준다(오류가 없으면 빈 목록). 아직이거나 돌리는 것이
    /// 없으면 `None`. 끼운 뒤부터 `has_custom_shaders`가 참이고 다음 프레임부터 셰이더가 돈다
    pub fn poll_custom_shaders(&mut self) -> Option<Vec<String>> {
        let job = self.custom_job.as_ref()?;
        let done = match job.rx.try_recv() {
            Ok(r) => r,
            Err(TryRecvError::Empty) => return None,
            // 스레드가 결과를 보내지 못하고 끝났다(패닉). 앱이 멈추지 않게 오류로 알리고 셰이더를 끈다
            Err(TryRecvError::Disconnected) => Compiled {
                chain: None,
                errors: vec!["셰이더 컴파일 스레드가 결과 없이 끝났다".to_string()],
            },
        };
        let job = self.custom_job.take()?;
        log::info!(
            "사용자 셰이더 끼움: 시작부터 {:.1}ms, 켜진 패스 {}",
            ms(job.started),
            done.chain.as_ref().map_or(0, |c| c.pipelines.len())
        );
        self.custom = done.chain;
        Some(done.errors)
    }

    /// 도는 컴파일의 결과가 올 때까지 기다려 끼운다. 시험용이다(앱은 기다리지 않고 `poll_custom_shaders`를 쓴다).
    /// 오류 문구를 돌려주고, 돌리는 컴파일이 없으면 빈 목록이다
    pub fn wait_custom_shaders(&mut self) -> Vec<String> {
        loop {
            if self.custom_job.is_none() {
                return Vec::new();
            }
            if let Some(e) = self.poll_custom_shaders() {
                return e;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    /// 컴파일이 돌고 있거나 결과가 아직 끼워지지 않았는가
    pub fn custom_shaders_pending(&self) -> bool {
        self.custom_job.is_some()
    }

    /// 컴파일이 끝났을 때 불릴 함수(컴파일 스레드에서 불린다). 앱이 이벤트 루프를 깨우는 데 쓴다. 다음에 거는 컴파일부터 적용
    pub fn set_custom_shader_waker(&mut self, wake: Option<ShaderWaker>) {
        self.shader_waker = wake;
    }

    /// 셰이더 하나의 컴파일을 단계별로 재서 돌려준다(측정용, 체인에 끼우지 않는다). `on_thread`면 장치를 복제해 다른 스레드에서 돈다
    #[doc(hidden)]
    pub fn measure_compile_stages(&self, s: &CustomShaderSource, on_thread: bool) -> Result<CompileStages, String> {
        if on_thread {
            let (device, format) = (self.device.clone(), self.format);
            std::thread::scope(|sc| {
                sc.spawn(|| stages_of(&device, format, s))
                    .join()
                    .map_err(|_| "스레드가 패닉했다".to_string())?
            })
        } else {
            stages_of(&self.device, self.format, s)
        }
    }

    /// 사용자 셰이더 유니폼 버퍼의 바이트(칸마다 마지막 패스 하나, 셰이더가 둘 이상이면 가운데 패스용 하나 더)
    pub(super) fn custom_uniform_bytes(&self) -> usize {
        self.custom.as_ref().map_or(0, |c| {
            c.regions
                .iter()
                .map(|r| r.last_uniforms.size() as usize + r.mid.as_ref().map_or(0, |(b, _)| b.size() as usize))
                .sum()
        })
    }

    /// 사용자 셰이더 중간 텍스처의 바이트: 칸 크기별로 한 벌(셰이더 하나면 한 장, 둘 이상이면 두 장), 칸이 창 전체를 덮지 않으면
    /// 합친 화면을 담는 창 크기 하나
    pub(super) fn custom_texture_bytes(&self) -> usize {
        let px = |s: (u32, u32)| s.0 as usize * s.1 as usize * 4;
        self.custom.as_ref().map_or(0, |c| {
            c.pool.iter().map(|t| px(t.size) * t.textures.len()).sum::<usize>()
                + c.scene.as_ref().map_or(0, |t| px(t.size))
        })
    }

    /// 사용자 셰이더 중간 텍스처의 장 수
    pub(super) fn custom_texture_count(&self) -> usize {
        self.custom.as_ref().map_or(0, |c| {
            c.pool.iter().map(|t| t.textures.len()).sum::<usize>() + usize::from(c.scene.is_some())
        })
    }

    /// 사용자 셰이더가 하나라도 켜져 있는가
    pub fn has_custom_shaders(&self) -> bool {
        self.custom.is_some()
    }

    /// 지금 셰이더 중간 텍스처를 쥔 칸의 key(시험·측정용)
    #[doc(hidden)]
    pub fn custom_region_keys(&self) -> Vec<u64> {
        self.custom
            .as_ref()
            .map_or_else(Vec::new, |c| c.regions.iter().map(|r| r.key).collect())
    }

    /// 다음 프레임부터 셰이더에 줄 유니폼 값(창 전체에 한 번 도는 길: 해상도는 렌더러가 채운다). 칸 목록(`set_shader_regions`)은 비운다
    pub fn set_shader_uniforms(&mut self, u: &ShaderUniforms) {
        *self.shader_uniforms = *u;
        self.shader_regions.clear();
    }

    /// 다음 프레임부터 칸마다 셰이더를 돌린다(#441). 칸마다 `rect`를 칸 크기 텍스처로 떼어 사슬을 돌리고 제자리에 그린다.
    /// 목록에 없는 칸의 텍스처는 다음 프레임에 놓는다. 빈 목록이면 `set_shader_uniforms`의 값으로 창 전체에 한 번 돈다
    pub fn set_shader_regions(&mut self, regions: &[ShaderRegion]) {
        self.shader_regions.clear();
        self.shader_regions.extend_from_slice(regions);
    }

    /// 그리기 전에: 칸 크기별 중간 텍스처와 칸마다 바인드 그룹·유니폼 버퍼를 맞추고(크기가 바뀐 칸만 다시 만든다, 사라진 칸은 놓는다)
    /// 유니폼 버퍼를 올린다
    pub(super) fn prepare_custom(&mut self, window: (u32, u32)) {
        let Some(chain) = self.custom.as_mut() else { return };
        let whole = [ShaderRegion {
            key: 0,
            rect: None,
            uniforms: *self.shader_uniforms,
        }];
        let regions: &[ShaderRegion] = if self.shader_regions.is_empty() {
            &whole
        } else {
            &self.shader_regions
        };
        let plan = plan_regions(window, regions);
        chain.full = covers_window(window, &plan);
        let passes = chain.pipelines.len();
        // 사라진 칸과 쓰는 칸이 없는 크기의 텍스처를 놓는다(GPU 메모리가 새지 않게)
        chain.regions.retain(|r| plan.iter().any(|p| p.key == r.key));
        chain.pool.retain(|t| plan.iter().any(|p| (p.w, p.h) == t.size));
        // 앞 텍스처는 합친 화면을 떼어 오는 자리(COPY_DST)이거나 창 전체 칸에서는 합친 화면을 곧바로 그리는 자리다.
        // 둘 이상의 셰이더면 가운데 패스가 앞·뒤 모두에 그린다
        let usage = wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST;
        for p in &plan {
            let size = (p.w, p.h);
            if !chain.pool.iter().any(|t| t.size == size) {
                let textures: Vec<wgpu::Texture> = (0..passes.min(2))
                    .map(|i| {
                        texture_2d(
                            &self.device,
                            ["bt-gpu custom front", "bt-gpu custom back"][i],
                            size,
                            self.format,
                            usage,
                        )
                    })
                    .collect();
                let views = textures
                    .iter()
                    .map(|t| t.create_view(&wgpu::TextureViewDescriptor::default()))
                    .collect();
                chain.pool.push(SharedTex { size, textures, views });
            }
            if chain.regions.iter().any(|r| r.key == p.key && r.size == size) {
                continue;
            }
            chain.regions.retain(|r| r.key != p.key);
            let Some(shared) = chain.pool.iter().find(|t| t.size == size) else {
                continue;
            };
            let new_buffer = || {
                self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("bt-gpu custom shader uniforms"),
                    size: UNIFORM_BLOCK_SIZE as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            };
            let group = |view: &wgpu::TextureView, uniforms: &wgpu::Buffer| {
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("bt-gpu custom shader bind group"),
                    layout: &chain.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: uniforms.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&chain.sampler),
                        },
                    ],
                })
            };
            let last_uniforms = new_buffer();
            let last_group = group(&shared.views[(passes - 1) % 2], &last_uniforms);
            let mid = (passes >= 2).then(|| {
                let buf = new_buffer();
                let groups = [group(&shared.views[0], &buf), group(&shared.views[1], &buf)];
                (buf, groups)
            });
            chain.regions.push(RegionRes {
                key: p.key,
                size,
                last_uniforms,
                last_group,
                mid,
            });
        }
        // 합친 화면 텍스처: 칸이 창 전체를 덮으면 칸의 앞 텍스처가 그 자리이므로 필요 없다
        if chain.full {
            chain.scene = None;
        } else if chain.scene.as_ref().is_none_or(|t| t.size != window) {
            let texture = texture_2d(
                &self.device,
                "bt-gpu custom scene",
                window,
                self.format,
                wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
            );
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let blit_group = blit_group(&self.device, &chain.blit_layout, &view);
            chain.scene = Some(SceneTex {
                size: window,
                texture,
                view,
                blit_group,
            });
        }
        for p in &plan {
            let Some(res) = chain.regions.iter().find(|r| r.key == p.key) else {
                continue;
            };
            let Some(r) = regions.iter().find(|r| r.key == p.key) else {
                continue;
            };
            let res_xyz = [p.w as f32, p.h as f32, 1.0];
            // 마지막 패스는 대상에 뷰포트로 그리므로 `gl_FragCoord`가 창 좌표다: 칸 왼쪽 위를 빼 칸 안 좌표로 되돌린다
            let bytes = r.uniforms.to_std140(res_xyz, [p.x as f32, p.y as f32]);
            self.queue.write_buffer(&res.last_uniforms, 0, &bytes);
            if let Some((buf, _)) = &res.mid {
                self.queue
                    .write_buffer(buf, 0, &r.uniforms.to_std140(res_xyz, [0.0, 0.0]));
            }
        }
        self.timings.shader_passes = passes * plan.len();
        chain.plan = plan;
    }

    /// 한 프레임의 그리기 명령. 셰이더가 없으면 `encode_compose` 하나(지금 길 그대로)이다. 있으면 합친 화면(격자와 kitty 그림까지,
    /// 셰이더 뒤에 얹는 층은 빼고)을 그린 뒤 칸마다 그 칸 영역을 떼어 셰이더를 차례로 돌려 제자리에 그리고, 마지막에 셰이더 뒤에
    /// 얹는 층을 얹는다. 칸 밖(탭 막대, 칸 사이 틈)은 합친 화면 그대로다. 마지막 셰이더 패스는 대상의 칸 사각형에 뷰포트와 가위로
    /// 곧바로 그린다(칸 크기 텍스처에 그렸다가 옮기지 않는다)
    /// `srgb`는 `view`의 sRGB 꼴 뷰(선형 섞기용). 셰이더가 돌 때는 장면 텍스처에 그리므로 쓰지 않는다: 그때는 `alpha-blending`이 `native`로 그려진다
    pub(super) fn encode_frame(
        &self,
        view: &wgpu::TextureView,
        srgb: Option<&wgpu::TextureView>,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(chain) = self.custom.as_ref().filter(|c| !c.plan.is_empty()) else {
            return vec![self.encode_compose(view, srgb, true)];
        };
        let planned: Vec<(&Planned, &RegionRes, &SharedTex)> = chain
            .plan
            .iter()
            .filter_map(|p| {
                let r = chain.regions.iter().find(|r| r.key == p.key)?;
                let t = chain.pool.iter().find(|t| t.size == r.size)?;
                Some((p, r, t))
            })
            .collect();
        if planned.is_empty() {
            return vec![self.encode_compose(view, srgb, true)];
        }
        // 합친 화면의 자리: 칸이 창 전체면 그 칸의 앞 텍스처, 아니면 창 크기 텍스처
        let scene_view = match (chain.full, chain.scene.as_ref()) {
            (false, Some(t)) => &t.view,
            _ => &planned[0].2.views[0],
        };
        let scene = self.encode_compose(scene_view, None, false);
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bt-gpu custom shaders"),
        });
        let split_scene = if chain.full { None } else { chain.scene.as_ref() };
        if let Some(t) = split_scene {
            // 칸 밖은 합친 화면 그대로 대상에 둔다
            blit(&mut enc, &chain.blit_pipeline, &t.blit_group, view, None);
        }
        let last = chain.pipelines.len() - 1;
        for (p, r, shared) in planned {
            if let Some(t) = split_scene {
                // 칸 영역을 칸 크기 텍스처로 뗀다
                enc.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &t.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x: p.x, y: p.y, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::TexelCopyTextureInfo {
                        texture: &shared.textures[0],
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    wgpu::Extent3d {
                        width: p.w,
                        height: p.h,
                        depth_or_array_layers: 1,
                    },
                );
            }
            for (i, pipeline) in chain.pipelines.iter().enumerate() {
                let is_last = i == last;
                // 마지막 패스는 대상에 곧바로 그린다(창 전체 칸이면 한 장을 다 덮으므로 읽어 오지 않고 지운다, 아니면 칸 밖을 둔다).
                // 앞 패스는 칸 크기 텍스처에 그린다
                let (target, load) = if is_last {
                    (
                        view,
                        if chain.full {
                            wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT)
                        } else {
                            wgpu::LoadOp::Load
                        },
                    )
                } else {
                    (
                        &shared.views[(i + 1) % 2],
                        wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    )
                };
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("bt-gpu custom shader"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: target,
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
                });
                if is_last && !chain.full {
                    pass.set_viewport(p.x as f32, p.y as f32, p.w as f32, p.h as f32, 0.0, 1.0);
                    pass.set_scissor_rect(p.x, p.y, p.w, p.h);
                }
                pass.set_pipeline(pipeline);
                match (&r.mid, is_last) {
                    (Some((_, groups)), false) => pass.set_bind_group(0, &groups[i % 2], &[]),
                    _ => pass.set_bind_group(0, &r.last_group, &[]),
                }
                pass.draw(0..3, 0..1);
            }
        }
        self.encode_top(&mut enc, view);
        vec![scene, enc.finish()]
    }
}

fn blit_group(device: &wgpu::Device, layout: &wgpu::BindGroupLayout, view: &wgpu::TextureView) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bt-gpu custom blit group"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(view),
        }],
    })
}

/// `src`를 대상의 `rect`(없으면 전체)에 1:1로 덮어 그린다
fn blit(
    enc: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::RenderPipeline,
    src: &wgpu::BindGroup,
    target: &wgpu::TextureView,
    rect: Option<(u32, u32, u32, u32)>,
) {
    let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("bt-gpu custom blit"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                // 사각형만 덮고 나머지는 앞에 그린 것을 그대로 둔다
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    if let Some((x, y, w, h)) = rect {
        pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
        pass.set_scissor_rect(x, y, w, h);
    }
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, src, &[]);
    pass.draw(0..3, 0..1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn region(key: u64, rect: Option<PixelRect>) -> ShaderRegion {
        ShaderRegion {
            key,
            rect,
            uniforms: ShaderUniforms::default(),
        }
    }

    fn rect(x: i32, y: i32, w: u32, h: u32) -> Option<PixelRect> {
        Some(PixelRect { x, y, w, h })
    }

    #[test]
    fn no_rect_is_the_whole_window_and_covers_it() {
        let plan = plan_regions((800, 600), &[region(7, None)]);
        assert_eq!(
            plan,
            vec![Planned {
                key: 7,
                x: 0,
                y: 0,
                w: 800,
                h: 600
            }]
        );
        assert!(covers_window((800, 600), &plan));
    }

    #[test]
    fn a_rect_equal_to_the_window_covers_it_but_a_smaller_one_does_not() {
        assert!(covers_window(
            (800, 600),
            &plan_regions((800, 600), &[region(1, rect(0, 0, 800, 600))])
        ));
        // 탭 막대 자리를 뺀 칸 하나는 창 전체가 아니다(탭 막대는 셰이더 밖)
        assert!(!covers_window(
            (800, 600),
            &plan_regions((800, 600), &[region(1, rect(0, 30, 800, 570))])
        ));
        // 칸이 둘이면 창 전체가 아니다
        assert!(!covers_window(
            (800, 600),
            &plan_regions(
                (800, 600),
                &[region(1, rect(0, 0, 400, 600)), region(2, rect(401, 0, 399, 600))]
            )
        ));
    }

    #[test]
    fn regions_are_clipped_to_the_window_and_empty_or_outside_ones_are_dropped() {
        let plan = plan_regions(
            (100, 100),
            &[
                region(1, rect(-10, -10, 50, 50)), // 왼쪽 위로 삐져나옴: (0,0) 40x40
                region(2, rect(90, 90, 50, 50)),   // 오른쪽 아래로 삐져나옴: (90,90) 10x10
                region(3, rect(200, 0, 10, 10)),   // 창 밖
                region(4, rect(10, 10, 0, 5)),     // 빈 칸
            ],
        );
        let got: Vec<_> = plan.iter().map(|p| (p.key, p.x, p.y, p.w, p.h)).collect();
        assert_eq!(got, vec![(1, 0, 0, 40, 40), (2, 90, 90, 10, 10)]);
    }

    #[test]
    fn duplicate_keys_keep_the_first() {
        let plan = plan_regions(
            (100, 100),
            &[region(1, rect(0, 0, 10, 10)), region(1, rect(20, 20, 10, 10))],
        );
        assert_eq!(plan.len(), 1);
        assert_eq!((plan[0].x, plan[0].y), (0, 0));
    }
}
