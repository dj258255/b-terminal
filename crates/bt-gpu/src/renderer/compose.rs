//! 층 합치기(ADR-036): 아래층 텍스처 → 칸마다 격자(자리 옮김) → 위층 텍스처를 한 번에 그린다.
//!
//! 칸의 자리는 `Scene::build`가 만든 인스턴스를 build 뒤에 옮겨서 처리한다(build는 늘 격자의 (0, 0)에 만든다)

use std::ops::Range;
use std::time::Instant;

use bt_core::Rgb;
use bt_render::{Fonts, GridView};

use super::layer_diff::changed_rects;
use super::{FrameTimings, GpuRenderer, upload_instances};
use crate::scene::{GLYPH_STRIDE, GlyphInst, RECT_STRIDE, RectInst};

/// 합칠 칸 하나: 격자를 놓을 왼쪽 위 자리(창 픽셀)와 격자, 그리고 그 칸의 바탕색(`Scene::build`의 `clear`)
pub struct PaneDraw<'a> {
    pub x: i32,
    pub y: i32,
    pub view: &'a GridView<'a>,
    pub clear: Rgb,
}

/// 이번 프레임에 새로 올릴 층. `None`이면 앞서 올린 것을 그대로 쓴다(바뀐 프레임에만 올린다)
#[derive(Default, Clone, Copy)]
pub struct LayerUpdate<'a> {
    /// 격자 아래: 창 바탕, 탭 막대, 칸 바탕, 여백. 불투명, 창 크기, 0xAARRGGBB
    pub lower: Option<&'a [u32]>,
    /// 격자 위: kitty 그림, 흐림, 막대, 팔레트 등. 투명 바탕에 그린 곱한 알파, 창 크기, 0xAARRGGBB.
    /// 빈 조각(`Some(&[])`)은 "위층이 비었다"는 뜻이다: 위층 텍스처와 사본을 놓는다(#387)
    pub upper: Option<&'a [u32]>,
}

/// 층 합치기가 쥔 메모리(바이트). 시험과 측정에서 본다
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LayerMemory {
    /// 층 텍스처(GPU)
    pub texture_bytes: usize,
    /// 텍스처에 올린 픽셀의 사본(CPU)
    pub shadow_bytes: usize,
    /// 올릴 사각형을 옮겨 담는 임시 버퍼의 용량(CPU)
    pub scratch_bytes: usize,
}

/// 층 텍스처 하나. `Bgra8Unorm`이라 CPU 캔버스의 `u32`(0xAARRGGBB, 작은 끝 바이트 순서 B G R A)를 그대로 올린다
pub(super) struct LayerTex {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
    /// 텍스처에 올라가 있는 픽셀의 사본. 다음 층과 견줘 달라진 사각형만 올린다(ADR-037)
    shadow: Vec<u32>,
    /// `shadow`에서 0이 아닌 픽셀 수. 0이면 층이 비었다(그리지 않는다)
    nonzero: usize,
}

/// 칸 하나가 인스턴스 목록에서 차지하는 구간(글자 앞 사각형, 글자, 글자 뒤 사각형, 상자 커서 안 글자 차례)
struct Segment {
    /// 칸 바탕(아래층 위에 덮어쓴다)
    rects_bg: Range<usize>,
    rects_pre: Range<usize>,
    rects_post: Range<usize>,
    glyphs_pre: Range<usize>,
    glyphs_post: Range<usize>,
}

/// 층 합치기가 프레임 사이에 들고 있는 것
pub(super) struct Layers {
    layout: wgpu::BindGroupLayout,
    /// 아래층: 덮어쓴다
    lower_pipeline: wgpu::RenderPipeline,
    /// 위층: 곱한 알파로 얹는다
    upper_pipeline: wgpu::RenderPipeline,
    lower: Option<LayerTex>,
    upper: Option<LayerTex>,
    /// 이번 프레임의 크기. 크기가 다른 층은 그리지 않는다
    size: (u32, u32),
    scratch: Vec<u8>,
    rects: Vec<RectInst>,
    glyphs: Vec<GlyphInst>,
    segments: Vec<Segment>,
}

impl Layers {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bt-gpu layer layout"),
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
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bt-gpu layer pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bt-gpu layers"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../layer.wgsl").into()),
        });
        let pipeline = |label: &str, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("layer_vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("layer_fs"),
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
        // 위층 픽셀은 CPU 캔버스가 이미 알파를 곱해 둔 값이다: 결과 = 위층 + 바탕 * (1 - 위층 알파)
        let premultiplied_over = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let lower_pipeline = pipeline("bt-gpu lower layer", None);
        let upper_pipeline = pipeline(
            "bt-gpu upper layer",
            Some(wgpu::BlendState {
                color: premultiplied_over,
                alpha: premultiplied_over,
            }),
        );
        Self {
            layout,
            lower_pipeline,
            upper_pipeline,
            lower: None,
            upper: None,
            size: (0, 0),
            scratch: Vec::new(),
            rects: Vec::new(),
            glyphs: Vec::new(),
            segments: Vec::new(),
        }
    }

    /// 층 하나를 올린다. 같은 크기의 텍스처가 있으면 앞서 올린 것과 견줘 달라진 사각형만 올리고, 없으면(처음, 창 크기가
    /// 바뀜) 새로 만들어 통째로 올린다. 픽셀이 모자라면 앞의 것을 그대로 둔다. 올린 바이트를 돌려 준다
    fn upload(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        upper: bool,
        px: &[u32],
        (w, h): (u32, u32),
    ) -> usize {
        if upper && px.is_empty() {
            // 위층이 비었다: 텍스처와 사본을 놓는다. 다시 그리면 새로 만든다(#387)
            self.upper = None;
            return 0;
        }
        if w == 0 || h == 0 || px.len() < w as usize * h as usize {
            log::warn!("층 픽셀이 창 크기보다 모자란다: {} < {w}×{h}", px.len());
            return 0;
        }
        let px = &px[..w as usize * h as usize];
        let slot = if upper { &mut self.upper } else { &mut self.lower };
        let rects = match slot.as_ref().filter(|t| (t.width, t.height) == (w, h)) {
            Some(t) => changed_rects(&t.shadow, px, w, h),
            None => {
                let texture = device.create_texture(&wgpu::TextureDescriptor {
                    label: Some(if upper {
                        "bt-gpu upper layer"
                    } else {
                        "bt-gpu lower layer"
                    }),
                    size: wgpu::Extent3d {
                        width: w,
                        height: h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Bgra8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("bt-gpu layer bind group"),
                    layout: &self.layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    }],
                });
                *slot = Some(LayerTex {
                    texture,
                    bind_group,
                    width: w,
                    height: h,
                    shadow: vec![0; w as usize * h as usize],
                    nonzero: 0,
                });
                // wgpu는 새 텍스처를 0으로 채워 쓴다. 사본도 0이니 0이 아닌 자리만 올리면 된다
                // (놓았던 위층이 돌아올 때 층을 통째로 올리지 않는다, #387)
                match slot.as_ref() {
                    Some(t) => changed_rects(&t.shadow, px, w, h),
                    None => Vec::new(),
                }
            }
        };
        let Some(t) = slot else { return 0 };
        let mut bytes = 0;
        for r in rects {
            let (rw, rh) = (r.w as usize, r.h as usize);
            self.scratch.resize(rw * rh * 4, 0);
            let (mut old_nz, mut new_nz) = (0, 0);
            for (row, dst_row) in self.scratch.chunks_exact_mut(rw * 4).enumerate() {
                let at = (r.y as usize + row) * w as usize + r.x as usize;
                let src = &px[at..at + rw];
                let shadow = &mut t.shadow[at..at + rw];
                old_nz += shadow.iter().filter(|p| **p != 0).count();
                new_nz += src.iter().filter(|p| **p != 0).count();
                shadow.copy_from_slice(src);
                for (dst, p) in dst_row.chunks_exact_mut(4).zip(src) {
                    dst.copy_from_slice(&p.to_le_bytes());
                }
            }
            t.nonzero = t.nonzero + new_nz - old_nz;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &t.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: r.x, y: r.y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &self.scratch,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(r.w * 4),
                    rows_per_image: Some(r.h),
                },
                wgpu::Extent3d {
                    width: r.w,
                    height: r.h,
                    depth_or_array_layers: 1,
                },
            );
            bytes += rw * rh * 4;
        }
        bytes
    }

    /// 쥔 메모리
    fn memory(&self) -> LayerMemory {
        let mut m = LayerMemory {
            scratch_bytes: self.scratch.capacity(),
            ..LayerMemory::default()
        };
        for t in [&self.lower, &self.upper].into_iter().flatten() {
            m.texture_bytes += t.width as usize * t.height as usize * 4;
            m.shadow_bytes += t.shadow.capacity() * 4;
        }
        m
    }

    /// 지금 크기와 같은 층만
    fn visible(&self, upper: bool) -> Option<&LayerTex> {
        let t = if upper { &self.upper } else { &self.lower };
        // 비어 있는 위층은 그리지 않는다(칸마다 화면을 한 번 덮는 일을 아낀다)
        t.as_ref()
            .filter(|t| (t.width, t.height) == self.size && !(upper && t.nonzero == 0))
    }
}

/// 격자 하나를 (0, 0)에 만든 인스턴스를 `(dx, dy)`만큼 옮긴다. `Scene::build`는 늘 격자의 (0, 0)에 만든다
pub fn shift_instances(rects: &mut [RectInst], glyphs: &mut [GlyphInst], dx: i32, dy: i32) {
    for r in rects {
        r.pos[0] += dx as f32;
        r.pos[1] += dy as f32;
    }
    for g in glyphs {
        g.pos[0] += dx as f32;
        g.pos[1] += dy as f32;
    }
}

impl GpuRenderer {
    /// 층 합치기가 쥔 메모리(바이트)
    pub fn layer_memory(&self) -> LayerMemory {
        self.layers.memory()
    }

    /// 층과 칸을 창 없이 합쳐 0RGB 픽셀로 읽어 낸다(시험과 CPU 프레임 비교용).
    /// 창에 올리는 `present`와 같은 길로 합친다: 달라지는 것은 어디에 그리느냐뿐이다
    pub fn compose(
        &mut self,
        fonts: &mut Fonts,
        size: (u32, u32),
        layers: LayerUpdate,
        panes: &[PaneDraw],
    ) -> Vec<u32> {
        let (w, h) = (size.0.max(1), size.1.max(1));
        self.ensure_target(w, h);
        self.prepare_compose(fonts, (w, h), layers, panes);
        let t = Instant::now();
        let Some(target) = self.target.as_ref() else {
            return Vec::new();
        };
        let cbs = self.encode_frame(&target.view);
        self.queue.submit(cbs);
        let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
        self.timings.gpu_ms = t.elapsed().as_secs_f64() * 1000.0;
        self.drawn = true;
        self.read_pixels()
    }

    /// 올릴 층을 올리고, 칸마다 인스턴스를 만들어 자리로 옮기고, 아틀라스와 인스턴스 버퍼를 올린다
    pub(super) fn prepare_compose(
        &mut self,
        fonts: &mut Fonts,
        size: (u32, u32),
        layers: LayerUpdate,
        panes: &[PaneDraw],
    ) {
        let t0 = Instant::now();
        self.layers.size = size;
        let mut upload_bytes = 0;
        if let Some(px) = layers.lower {
            upload_bytes += self.layers.upload(&self.device, &self.queue, false, px, size);
        }
        if let Some(px) = layers.upper {
            upload_bytes += self.layers.upload(&self.device, &self.queue, true, px, size);
        }
        let upload = t0.elapsed();

        let t1 = Instant::now();
        let ls = &mut self.layers;
        ls.rects.clear();
        ls.glyphs.clear();
        ls.segments.clear();
        for p in panes {
            self.scene.build(&mut self.cache, fonts, p.view, p.clear);
            let (r0, g0) = (ls.rects.len(), ls.glyphs.len());
            ls.rects.extend_from_slice(&self.scene.rects);
            ls.glyphs.extend_from_slice(&self.scene.glyphs);
            shift_instances(&mut ls.rects[r0..], &mut ls.glyphs[g0..], p.x, p.y);
            ls.segments.push(Segment {
                rects_bg: r0..r0 + self.scene.rects_bg,
                rects_pre: r0 + self.scene.rects_bg..r0 + self.scene.rects_pre,
                rects_post: r0 + self.scene.rects_pre..ls.rects.len(),
                glyphs_pre: g0..g0 + self.scene.glyphs_pre,
                glyphs_post: g0 + self.scene.glyphs_pre..ls.glyphs.len(),
            });
        }
        self.sync_atlases();
        self.queue.write_buffer(
            &self.globals,
            0,
            &[size.0 as f32, size.1 as f32, 0.0, 0.0]
                .iter()
                .flat_map(|v| v.to_le_bytes())
                .collect::<Vec<u8>>(),
        );
        self.bytes.clear();
        for r in &self.layers.rects {
            r.write(&mut self.bytes);
        }
        let rect_bytes = self.bytes.len();
        for g in &self.layers.glyphs {
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
        self.timings = FrameTimings {
            prepare_ms: t1.elapsed().as_secs_f64() * 1000.0,
            upload_ms: upload.as_secs_f64() * 1000.0,
            upload_bytes,
            ..FrameTimings::default()
        };
        self.prepare_custom(size);
    }

    /// 한 번의 그리기 패스로 아래층 → 칸마다(바탕, 글자, 커서) → 위층을 `view`에 그리는 명령을 만든다
    pub(super) fn encode_compose(&self, view: &wgpu::TextureView) -> wgpu::CommandBuffer {
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bt-gpu compose"),
        });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("bt-gpu compose"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let ls = &self.layers;
            if let Some(t) = ls.visible(false) {
                pass.set_pipeline(&ls.lower_pipeline);
                pass.set_bind_group(0, &t.bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
            pass.set_bind_group(0, &self.bind_group, &[]);
            let draw_rects_with =
                |pass: &mut wgpu::RenderPass<'_>, pipeline: &wgpu::RenderPipeline, range: &Range<usize>| {
                    if let (Some(buf), false) = (&self.rect_buf, range.is_empty()) {
                        pass.set_pipeline(pipeline);
                        pass.set_vertex_buffer(0, buf.slice(..));
                        pass.draw(0..6, range.start as u32..range.end as u32);
                    }
                };
            let draw_rects = |pass: &mut wgpu::RenderPass<'_>, range: &Range<usize>| {
                draw_rects_with(pass, &self.rect_pipeline, range)
            };
            let draw_glyphs = |pass: &mut wgpu::RenderPass<'_>, range: &Range<usize>| {
                if let (Some(buf), false) = (&self.glyph_buf, range.is_empty()) {
                    pass.set_pipeline(&self.glyph_pipeline);
                    pass.set_vertex_buffer(0, buf.slice(..));
                    pass.draw(0..6, range.start as u32..range.end as u32);
                }
            };
            for s in &ls.segments {
                draw_rects_with(&mut pass, &self.rect_replace_pipeline, &s.rects_bg);
                draw_rects(&mut pass, &s.rects_pre);
                draw_glyphs(&mut pass, &s.glyphs_pre);
                draw_rects(&mut pass, &s.rects_post);
                draw_glyphs(&mut pass, &s.glyphs_post);
            }
            if let Some(t) = ls.visible(true) {
                pass.set_pipeline(&ls.upper_pipeline);
                pass.set_bind_group(0, &t.bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        enc.finish()
    }
}

// 인스턴스 꼴이 바뀌면 `shift_instances`와 셰이더를 함께 봐야 한다
const _: () = assert!(std::mem::size_of::<RectInst>() == RECT_STRIDE);
const _: () = assert!(std::mem::size_of::<GlyphInst>() == GLYPH_STRIDE);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shift_moves_every_instance_and_nothing_else() {
        let mut rects = vec![RectInst {
            pos: [1.0, 2.0],
            size: [3.0, 4.0],
            color: [5, 6, 7, 8],
        }];
        let mut glyphs = vec![GlyphInst {
            pos: [10.0, 20.0],
            size: [3.0, 4.0],
            atlas: [9.0, 9.0],
            color: [1, 2, 3, 4],
            kind: 1,
        }];
        shift_instances(&mut rects, &mut glyphs, 100, -7);
        assert_eq!(rects[0].pos, [101.0, -5.0]);
        assert_eq!(rects[0].size, [3.0, 4.0]);
        assert_eq!(rects[0].color, [5, 6, 7, 8]);
        assert_eq!(glyphs[0].pos, [110.0, 13.0]);
        assert_eq!(glyphs[0].atlas, [9.0, 9.0]);
        assert_eq!(glyphs[0].kind, 1);
    }
}
