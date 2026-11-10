//! 창 없는 wgpu 렌더러: 텍스처에 그리고 `Vec<u32>`로 읽어 낸다

use std::time::Instant;

use bt_core::Rgb;
use bt_render::{Fonts, GridView};
use wgpu::util::DeviceExt;

use crate::atlas::Atlas;
use crate::glyphs::GlyphCache;
use crate::scene::{GLYPH_STRIDE, GlyphInst, RECT_STRIDE, RectInst, Scene};

mod compose;
mod layer_diff;
mod window;
pub use compose::{LayerUpdate, PaneDraw};
pub use window::PresentOutcome;

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
}

/// 그림이 그려지는 텍스처 꼴. sRGB가 아니다: CPU 렌더러는 0~255 값을 그대로 섞으므로(감마 없이)
/// GPU도 그 값 그대로 섞어야 같은 픽셀이 나온다(ADR-035)
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

struct Target {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
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
                    visibility: wgpu::ShaderStages::VERTEX,
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
        let gray_tex = atlas_texture(&device, initial, wgpu::TextureFormat::R8Unorm, "bt-gpu gray atlas");
        let color_tex = atlas_texture(&device, initial, wgpu::TextureFormat::Rgba8Unorm, "bt-gpu color atlas");
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
            cache: GlyphCache::new(initial, max),
            scene: Scene::default(),
            bytes: Vec::new(),
            timings: FrameTimings::default(),
            drawn: false,
            layers,
            format,
            window,
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
        self.scene.build(&mut self.cache, fonts, view, clear);
        self.sync_atlases();

        self.queue.write_buffer(
            &self.globals,
            0,
            &[w as f32, h as f32, 0.0, 0.0]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<u8>>(),
        );
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
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bt-gpu cells"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        // 투명 창이면 투명으로 지우고 칸이 화면을 덮는다(#353, Scene::is_transparent)
                        load: wgpu::LoadOp::Clear(if self.scene.is_transparent() {
                            wgpu::Color::TRANSPARENT
                        } else {
                            wgpu::Color {
                                r: clear.r as f64 / 255.0,
                                g: clear.g as f64 / 255.0,
                                b: clear.b as f64 / 255.0,
                                a: 1.0,
                            }
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            let sc = &self.scene;
            let draw_rects = |pass: &mut wgpu::RenderPass<'_>, range: std::ops::Range<usize>| {
                if let (Some(buf), false) = (&self.rect_buf, range.is_empty()) {
                    pass.set_pipeline(&self.rect_pipeline);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..6, range.start as u32..range.end as u32);
                }
            };
            let draw_glyphs = |pass: &mut wgpu::RenderPass<'_>, range: std::ops::Range<usize>| {
                if let (Some(buf), false) = (&self.glyph_buf, range.is_empty()) {
                    pass.set_pipeline(&self.glyph_pipeline);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..6, range.start as u32..range.end as u32);
                }
            };
            draw_rects(&mut pass, 0..sc.rects_pre);
            draw_glyphs(&mut pass, 0..sc.glyphs_pre);
            draw_rects(&mut pass, sc.rects_pre..sc.rects.len());
            draw_glyphs(&mut pass, sc.glyphs_pre..sc.glyphs.len());
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
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
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
            readback,
            width,
            height,
            padded_row,
        });
        self.drawn = false;
    }

    /// 캐시에 새로 채운 글리프를 GPU 텍스처로 올린다. 아틀라스가 커졌으면 텍스처를 새로 만들어 통째로 올린다
    fn sync_atlases(&mut self) {
        let mut rebuilt = false;
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
                rebuilt = true;
            }
        }
        if rebuilt {
            self.bind_group = make_bind_group(
                &self.device,
                &self.layout,
                &self.globals,
                &self.gray_tex,
                &self.color_tex,
            );
        }
        for u in self.cache.uploads.drain(..) {
            let (atlas, tex, bpp) = if u.color {
                (&self.cache.color, &self.color_tex, 4usize)
            } else {
                (&self.cache.gray, &self.gray_tex, 1usize)
            };
            // 방금 통째로 올렸으면 이미 들어 있다
            if rebuilt && atlas.size() == tex.size {
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
