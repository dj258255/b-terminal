//! 사용자 셰이더 패스(ADR-040): 합친 화면을 창 크기 중간 텍스처에 그리고, 셰이더를 차례로 돌려 마지막 것을 대상에 그린다.
//! Ghostty `CustomShaderState`(`generic.zig`)처럼 앞뒤 텍스처를 번갈아 쓴다: 앞 출력이 다음 `iChannel0`이다.
//! 셰이더가 없으면 이 파일의 길을 타지 않는다(`encode_frame`이 `encode_compose`를 그대로 부른다)

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

impl GpuRenderer {
    /// 사용자 셰이더를 컴파일해 적은 순서대로 쓴다. 컴파일에 실패한 것은 건너뛰고(Ghostty와 같다) 오류 문구를 돌려준다.
    /// 빈 목록이면 셰이더를 끄고 지금 길로 돌아간다. 성공한 것이 하나도 없어도 꺼진다
    pub fn set_custom_shaders(&mut self, shaders: &[CustomShaderSource]) -> Vec<String> {
        let mut errors = Vec::new();
        if shaders.is_empty() {
            self.custom = None;
            return errors;
        }
        let layout = self.device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bt-gpu custom shader layout"),
            entries: &layout_entries(),
        });
        let pl = self.device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bt-gpu custom shader pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let vs = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bt-gpu custom shader vertex"),
            source: wgpu::ShaderSource::Wgsl(FULL_SCREEN_VS.into()),
        });
        let mut pipelines = Vec::new();
        for s in shaders {
            let t = Instant::now();
            match self.compile_pass(s, &pl, &vs) {
                Ok(p) => {
                    log::info!(
                        "사용자 셰이더 {}: 컴파일 {:.1}ms",
                        s.name,
                        t.elapsed().as_secs_f64() * 1000.0
                    );
                    pipelines.push(p);
                }
                Err(e) => {
                    log::warn!("사용자 셰이더를 건너뛴다: {e}");
                    errors.push(e);
                }
            }
        }
        self.custom = if pipelines.is_empty() {
            None
        } else {
            let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("bt-gpu custom shader sampler"),
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            });
            let uniforms = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bt-gpu custom shader uniforms"),
                size: UNIFORM_BLOCK_SIZE as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            Some(CustomChain {
                layout,
                pipelines,
                sampler,
                uniforms,
                textures: None,
            })
        };
        errors
    }

    /// 셰이더 하나를 읽고 검증하고 파이프라인으로 만든다. 먼저 naga로 막고, 그래도 wgpu가 오류를 내면(바인딩이 안 맞는 것 등)
    /// 오류 범위로 잡아 문구로 돌려준다: 잘못된 셰이더가 앱을 패닉시키면 안 된다
    fn compile_pass(
        &self,
        s: &CustomShaderSource,
        pl: &wgpu::PipelineLayout,
        vs: &wgpu::ShaderModule,
    ) -> Result<wgpu::RenderPipeline, String> {
        let module = shadertoy::check(&s.name, &s.glsl)?;
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let fs = self.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(&s.name),
            source: wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module)),
        });
        let pipeline = self.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(&s.name),
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
                    format: self.format,
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
        match pollster::block_on(scope.pop()) {
            Some(e) => Err(format!("{}: {e}", s.name)),
            None => Ok(pipeline),
        }
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
    pub(super) fn encode_frame(&self, view: &wgpu::TextureView) -> Vec<wgpu::CommandBuffer> {
        let Some((chain, tex)) = self.custom.as_ref().and_then(|c| c.textures.as_ref().map(|t| (c, t))) else {
            return vec![self.encode_compose(view)];
        };
        let scene = self.encode_compose(&tex.views[0]);
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
