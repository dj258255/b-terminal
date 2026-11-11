//! 렌더러 하나가 쥔 GPU·CPU 메모리를 형식과 크기에서 센다(#400). 드라이버가 실제로 잡은 양이 아니라 우리가 요청한 크기다

use super::GpuRenderer;

/// 렌더러 하나(창 하나)가 쥔 자원의 바이트. 텍스처·버퍼는 요청한 크기이고, 드라이버의 정렬·여분은 들어 있지 않다
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GpuMemory {
    /// 층 텍스처 (GPU, 아래·위)
    pub layer_textures: usize,
    /// 회색조 아틀라스 텍스처 (GPU)
    pub gray_atlas: usize,
    /// 색 아틀라스 텍스처 (GPU). 색 글리프(이모지)를 처음 볼 때까지는 1x1이다
    pub color_atlas: usize,
    /// 인스턴스 버퍼(칸 바탕·글자) 용량 (GPU)
    pub instance_buffers: usize,
    /// 유니폼 버퍼 (GPU): 셀 셰이더 전역값, 사용자 셰이더 유니폼
    pub uniform_buffers: usize,
    /// 창 없는 길의 그림 텍스처와 읽기 버퍼. 창 길에서는 0
    pub offscreen_target: usize,
    /// 사용자 셰이더 중간 텍스처 둘 (GPU). 셰이더가 없으면 0
    pub custom_textures: usize,
    /// 아틀라스의 CPU 쪽 사본(회색조 + 색)
    pub atlas_cpu: usize,
    /// 층 칸 해시(CPU)
    pub layer_hashes: usize,
    /// 층 올리기 임시 버퍼 용량(CPU)
    pub scratch: usize,
}

impl GpuMemory {
    /// GPU가 쥔 바이트의 합
    pub fn gpu_total(&self) -> usize {
        self.layer_textures
            + self.gray_atlas
            + self.color_atlas
            + self.instance_buffers
            + self.uniform_buffers
            + self.offscreen_target
            + self.custom_textures
    }

    /// CPU가 쥔 바이트의 합
    pub fn cpu_total(&self) -> usize {
        self.atlas_cpu + self.layer_hashes + self.scratch
    }
}

impl GpuRenderer {
    /// 이 렌더러가 쥔 메모리(바이트)
    pub fn gpu_memory(&self) -> GpuMemory {
        let layers = self.layers.memory();
        let buf = |b: &Option<wgpu::Buffer>| b.as_ref().map_or(0, |b| b.size() as usize);
        let tex = |t: &super::AtlasTexture| t.size as usize * t.size as usize;
        GpuMemory {
            layer_textures: layers.texture_bytes,
            gray_atlas: tex(&self.gray_tex),
            color_atlas: tex(&self.color_tex) * 4,
            instance_buffers: buf(&self.rect_buf) + buf(&self.glyph_buf),
            uniform_buffers: self.globals.size() as usize + self.custom_uniform_bytes(),
            offscreen_target: self.target.as_ref().map_or(0, |t| {
                t.width as usize * t.height as usize * 4 + t.readback.size() as usize
            }),
            custom_textures: self.custom_texture_bytes(),
            atlas_cpu: self.cache.gray.data().len() + self.cache.color.data().len(),
            layer_hashes: layers.shadow_bytes,
            scratch: layers.scratch_bytes,
        }
    }

    pub(super) fn log_memory(&self, why: &str) {
        let m = self.gpu_memory();
        log::debug!("GPU 메모리({why}): GPU {} CPU {} {m:?}", m.gpu_total(), m.cpu_total());
    }
}
