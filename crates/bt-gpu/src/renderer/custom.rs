//! 사용자 셰이더 패스(ADR-040): 합친 화면을 창 크기 중간 텍스처에 그리고, 셰이더를 차례로 돌려 마지막 것을 대상에 그린다.
//! Ghostty `CustomShaderState`(`generic.zig`)처럼 앞뒤 텍스처를 번갈아 쓴다: 앞 출력이 다음 `iChannel0`이다.
//! 셰이더가 없으면 이 파일의 길을 타지 않는다(`encode_frame`이 `encode_compose`를 그대로 부른다)

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

/// 창 크기 중간 텍스처 둘과 그 바인드 그룹(`bind_groups[i]`는 `views[i]`를 `iChannel0`으로 읽는다)
struct PingPong {
    size: (u32, u32),
    views: [wgpu::TextureView; 2],
    bind_groups: [wgpu::BindGroup; 2],
}

/// 컴파일에 성공한 셰이더들(적은 순서)과 패스가 쓰는 것
pub(super) struct CustomChain {
    layout: wgpu::BindGroupLayout,
    pipelines: Vec<wgpu::RenderPipeline>,
    /// Ghostty와 같다: 선형 필터, 가장자리 고정(`Metal.zig` 292~300줄)
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    textures: Option<PingPong>,
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
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bt-gpu custom shader uniforms"),
            size: UNIFORM_BLOCK_SIZE as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        CustomChain {
            layout,
            pipelines,
            sampler,
            uniforms,
            textures: None,
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

    /// 사용자 셰이더 유니폼 버퍼의 바이트
    pub(super) fn custom_uniform_bytes(&self) -> usize {
        self.custom.as_ref().map_or(0, |c| c.uniforms.size() as usize)
    }

    /// 사용자 셰이더 중간 텍스처 둘의 바이트
    pub(super) fn custom_texture_bytes(&self) -> usize {
        let bytes = |t: &PingPong| t.size.0 as usize * t.size.1 as usize * 4 * 2;
        self.custom.as_ref().and_then(|c| c.textures.as_ref()).map_or(0, bytes)
    }

    /// 사용자 셰이더가 하나라도 켜져 있는가
    pub fn has_custom_shaders(&self) -> bool {
        self.custom.is_some()
    }

    /// 다음 프레임부터 셰이더에 줄 유니폼 값(해상도는 렌더러가 채운다)
    pub fn set_shader_uniforms(&mut self, u: &ShaderUniforms) {
        *self.shader_uniforms = *u;
    }

    /// 그리기 전에: 중간 텍스처를 창 크기로 맞추고(크기가 바뀌었으면 다시 만든다) 유니폼 버퍼를 올린다
    pub(super) fn prepare_custom(&mut self, (w, h): (u32, u32)) {
        let Some(chain) = self.custom.as_mut() else { return };
        if chain.textures.as_ref().is_none_or(|t| t.size != (w, h)) {
            let make = |i: usize| {
                self.device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some(["bt-gpu custom front", "bt-gpu custom back"][i]),
                        size: wgpu::Extent3d {
                            width: w,
                            height: h,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: self.format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    })
                    .create_view(&wgpu::TextureViewDescriptor::default())
            };
            let views = [make(0), make(1)];
            let group = |view: &wgpu::TextureView| {
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
                            resource: chain.uniforms.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&chain.sampler),
                        },
                    ],
                })
            };
            let bind_groups = [group(&views[0]), group(&views[1])];
            chain.textures = Some(PingPong {
                size: (w, h),
                views,
                bind_groups,
            });
        }
        let bytes = self.shader_uniforms.to_std140([w as f32, h as f32, 1.0]);
        self.queue.write_buffer(&chain.uniforms, 0, &bytes);
        self.timings.shader_passes = chain.pipelines.len();
    }

    /// 한 프레임의 그리기 명령. 셰이더가 없으면 `encode_compose` 하나(지금 길 그대로)이고, 있으면 합친 화면을 앞 텍스처에 그린 뒤
    /// 셰이더를 차례로 돌려 마지막 것이 `view`에 그린다
    /// `srgb`는 `view`의 sRGB 꼴 뷰(선형 섞기용). 셰이더가 돌 때는 장면 텍스처에 그리므로 쓰지 않는다: 그때는 `alpha-blending`이 `native`로 그려진다
    pub(super) fn encode_frame(
        &self,
        view: &wgpu::TextureView,
        srgb: Option<&wgpu::TextureView>,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some((chain, tex)) = self.custom.as_ref().and_then(|c| c.textures.as_ref().map(|t| (c, t))) else {
            return vec![self.encode_compose(view, srgb)];
        };
        let scene = self.encode_compose(&tex.views[0], None);
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bt-gpu custom shaders"),
        });
        let last = chain.pipelines.len() - 1;
        for (i, pipeline) in chain.pipelines.iter().enumerate() {
            let target = if i == last { view } else { &tex.views[(i + 1) % 2] };
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bt-gpu custom shader"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &tex.bind_groups[i % 2], &[]);
            pass.draw(0..3, 0..1);
        }
        vec![scene, enc.finish()]
    }
}
