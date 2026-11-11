//! wgpu로 셀 격자를 그린다(ADR-035). 창을 모르고, 텍스처에 그려 `Vec<u32>`(CPU 캔버스와 같은 꼴)로 읽어 낸다

pub mod atlas;
pub mod glyphs;
pub mod p3;
pub mod pacing;
mod renderer;
pub mod scene;
pub mod shadertoy;

pub use atlas::{Atlas, Full};
pub use renderer::{CompileStages, FrameTimings, GpuRenderer, PixelRect, ShaderRegion, ShaderWaker};
pub use renderer::{GpuMemory, LayerMemory, LayerUpdate, PaneDraw, PresentOutcome};
pub use shadertoy::{CustomShaderSource, ShaderUniforms};
