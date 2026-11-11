//! 실험 E26(#466): sRGB 값을 Display P3 값으로 바꾼다. 화면(surface)을 Display P3로 설정하면 합성기가 P3에서 화면으로
//! 색을 맞추므로, 마지막에 바꿔 쓰면 CPU 길의 `window-colorspace = srgb`와 같은 색이 된다.

/// 선형 sRGB(D65) → 선형 Display P3(D65) 행 우선 3x3.
/// 출처: 두 색 공간의 CIE xy 원색(sRGB: R .640/.330 G .300/.600 B .150/.060, Display P3: R .680/.320 G .265/.690 B .150/.060)과
/// 공통 백색점 D65(.3127, .3290)에서 RGB→XYZ 행렬을 만들어 `P3⁻¹ · sRGB`로 계산한 값을 소수 7자리로 적었다.
/// CSS Color Module Level 4의 `lin_sRGB_to_lin_P3`(0.8224621 0.1775380 / 0.0331941 0.9668058 / 0.0170827 0.0723974 0.9105199)와
/// 소수 6자리까지 같다(행마다 합이 1: 흰색 보존). 전달 함수(감마)는 둘이 같은 sRGB 식이다
pub const SRGB_TO_DISPLAY_P3: [[f32; 3]; 3] = [
    [0.822_462, 0.177_538, 0.0],
    [0.033_194_2, 0.966_805_8, 0.0],
    [0.017_082_6, 0.072_397_4, 0.910_519_9],
];

fn to_linear(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn from_linear(v: f32) -> f32 {
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        v.powf(1.0 / 2.4) * 1.055 - 0.055
    }
}

/// sRGB 값(0~1) → Display P3 값(0~1). 셰이더(`p3.wgsl`)와 같은 식: 전달 함수를 풀고, 행렬을 곱하고, 다시 감는다
pub fn srgb_to_display_p3(rgb: [f32; 3]) -> [f32; 3] {
    let lin = rgb.map(|c| to_linear(c.clamp(0.0, 1.0)));
    let m = &SRGB_TO_DISPLAY_P3;
    std::array::from_fn(|i| from_linear((m[i][0] * lin[0] + m[i][1] * lin[1] + m[i][2] * lin[2]).clamp(0.0, 1.0)))
}

/// 0~255 값용. 가장 가까운 정수로 맞춘다
pub fn srgb_to_display_p3_u8(rgb: [u8; 3]) -> [u8; 3] {
    srgb_to_display_p3(rgb.map(|c| c as f32 / 255.0)).map(|c| (c * 255.0).round() as u8)
}

/// 마지막 변환 패스: 중간 텍스처(sRGB 값)를 읽어 surface(Display P3 값)에 쓴다(안 E26-나)
pub(crate) struct P3Convert {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

impl P3Convert {
    /// `format`은 쓰는 대상(surface)의 꼴
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bt-gpu p3 layout"),
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
            label: Some("bt-gpu p3 pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bt-gpu p3"),
            source: wgpu::ShaderSource::Wgsl(include_str!("p3.wgsl").into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bt-gpu p3 convert"),
            layout: Some(&pl),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("p3_vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("p3_fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
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
        Self { pipeline, layout }
    }

    /// `src`를 `dst` 전체에 바꿔 쓴다(섞지 않고 덮어쓴다)
    pub(crate) fn encode(
        &self,
        device: &wgpu::Device,
        enc: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        dst: &wgpu::TextureView,
    ) {
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bt-gpu p3 group"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(src),
            }],
        });
        let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("bt-gpu p3 convert"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: dst,
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
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_and_black_are_unchanged() {
        assert_eq!(srgb_to_display_p3_u8([255, 255, 255]), [255, 255, 255]);
        assert_eq!(srgb_to_display_p3_u8([0, 0, 0]), [0, 0, 0]);
    }

    #[test]
    fn gray_stays_gray() {
        for v in [1u8, 17, 64, 128, 200, 254] {
            assert_eq!(srgb_to_display_p3_u8([v, v, v]), [v, v, v], "{v}");
        }
    }

    #[test]
    fn primaries_match_published_values() {
        // sRGB 빨강은 Display P3에서 약 (234, 51, 35): CSS Color 4의 color(srgb 1 0 0) = color(display-p3 0.9175 0.2003 0.1386)
        assert_eq!(srgb_to_display_p3_u8([255, 0, 0]), [234, 51, 35]);
        // 초록은 CSS Color 4의 color(display-p3 0.4631 0.9852 0.2979)(약 118,251,76)와 오차 1 이내여야 한다
        let g = srgb_to_display_p3_u8([0, 255, 0]);
        for (got, want) in g.iter().zip([118u8, 251, 76]) {
            assert!(got.abs_diff(want) <= 1, "{g:?}");
        }
        assert_eq!(srgb_to_display_p3_u8([0, 0, 255]), [0, 0, 245]);
    }

    #[test]
    fn terminal_background_282c34_matches_the_cpu_srgb_screen_pixel() {
        // 0.49.0 실측: CPU `window-colorspace = srgb`는 바탕 #282c34를 화면 픽셀 (41,44,51)로 냈다. GPU 길이 같은 색이 되려면
        // 레이어에 쓰는 P3 값이 (41,44,51)이어야 한다(합성기가 P3에서 화면으로 되돌리면 그 값이 나온다는 가설. 화면 실측은 주 세션)
        assert_eq!(srgb_to_display_p3_u8([0x28, 0x2c, 0x34]), [41, 44, 51]);
    }

    #[test]
    fn matrix_rows_sum_to_one_so_white_is_kept() {
        for row in SRGB_TO_DISPLAY_P3 {
            assert!((row.iter().sum::<f32>() - 1.0).abs() < 1e-6, "{row:?}");
        }
    }

    /// 변환 패스를 실제 GPU에서 돌려 CPU 함수와 견준다(GPU 어댑터가 없으면 알리고 끝낸다)
    #[test]
    fn gpu_pass_matches_the_cpu_function() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let Ok(adapter) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())) else {
            eprintln!("건너뜀: GPU 어댑터가 없다");
            return;
        };
        let Ok((device, queue)) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())) else {
            eprintln!("건너뜀: GPU 장치를 열지 못했다");
            return;
        };
        let format = wgpu::TextureFormat::Bgra8Unorm;
        let colors: [[u8; 4]; 8] = [
            [255, 255, 255, 255],
            [0, 0, 0, 255],
            [255, 0, 0, 255],
            [0, 255, 0, 255],
            [0, 0, 255, 255],
            [0x28, 0x2c, 0x34, 255],
            [128, 128, 128, 255],
            // 곱한 알파: 알파 128에서 (255,0,0)을 곱한 값 (128,0,0)
            [128, 0, 0, 128],
        ];
        let w = 64u32; // 한 줄 256바이트(복사 정렬)
        let mut data = vec![0u8; (w * 4) as usize];
        for (i, c) in colors.iter().enumerate() {
            data[i * 4..i * 4 + 4].copy_from_slice(&[c[2], c[1], c[0], c[3]]); // Bgra
        }
        let size = wgpu::Extent3d {
            width: w,
            height: 1,
            depth_or_array_layers: 1,
        };
        let make = |usage| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let src = make(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        let dst = make(wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC);
        queue.write_texture(
            src.as_image_copy(),
            &data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(1),
            },
            size,
        );
        let out = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: (w * 4) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let conv = P3Convert::new(&device, format);
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        conv.encode(
            &device,
            &mut enc,
            &src.create_view(&wgpu::TextureViewDescriptor::default()),
            &dst.create_view(&wgpu::TextureViewDescriptor::default()),
        );
        enc.copy_texture_to_buffer(
            dst.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &out,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(w * 4),
                    rows_per_image: Some(1),
                },
            },
            size,
        );
        queue.submit([enc.finish()]);
        let slice = out.slice(..);
        slice.map_async(wgpu::MapMode::Read, |_| {});
        let _ = device.poll(wgpu::PollType::wait_indefinitely());
        let got = slice.get_mapped_range().expect("읽기 범위").to_vec();
        for (i, c) in colors.iter().enumerate() {
            let px = &got[i * 4..i * 4 + 4];
            let (rgb, a) = ([px[2], px[1], px[0]], px[3]);
            let want = if c[3] == 255 {
                srgb_to_display_p3_u8([c[0], c[1], c[2]])
            } else {
                // 곱한 알파: 알파로 나눠 바꾸고 다시 곱한다
                let a = c[3] as f32 / 255.0;
                let un = [c[0], c[1], c[2]].map(|v| v as f32 / 255.0 / a);
                srgb_to_display_p3(un).map(|v| (v * a * 255.0).round() as u8)
            };
            assert_eq!(a, c[3], "알파는 그대로: {c:?}");
            for k in 0..3 {
                assert!(rgb[k].abs_diff(want[k]) <= 1, "{c:?}: GPU {rgb:?} CPU {want:?}");
            }
        }
    }
}
