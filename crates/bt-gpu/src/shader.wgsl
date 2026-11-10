// 셀 렌더러 셰이더(ADR-035). Ghostty의 shaders.metal과 같은 뼈대: 배경과 글리프를 인스턴스 사각형으로 그린다.
// 좌표는 모두 픽셀(왼쪽 위가 원점). 글리프는 아틀라스에서 텍셀을 그대로 읽는다(필터 없음, 사각형이 픽셀에 딱 맞는다)

struct Globals {
    viewport: vec2<f32>,
    pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var gray_atlas: texture_2d<f32>;
@group(0) @binding(2) var color_atlas: texture_2d<f32>;

fn corner(vi: u32) -> vec2<f32> {
    var c = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    return c[vi];
}

fn to_clip(p: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(p.x / g.viewport.x * 2.0 - 1.0, 1.0 - p.y / g.viewport.y * 2.0, 0.0, 1.0);
}

// 단색 사각형: 셀 배경, 커서
struct RectIn {
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct RectOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn rect_vs(in: RectIn) -> RectOut {
    var out: RectOut;
    out.clip = to_clip(in.pos + corner(in.vi) * in.size);
    out.color = in.color;
    return out;
}

@fragment
fn rect_fs(in: RectOut) -> @location(0) vec4<f32> {
    return in.color;
}

// 글리프: 회색조(kind 0)는 덮음 정도에 글자색을, 색(kind 1)은 RGBA를 그대로(곱하지 않은 알파) 낸다
struct GlyphIn {
    @builtin(vertex_index) vi: u32,
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) atlas: vec2<f32>,
    @location(3) color: vec4<f32>,
    @location(4) kind: u32,
};

struct GlyphOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) atlas: vec2<f32>,
    @location(2) @interpolate(flat) color: vec4<f32>,
    @location(3) @interpolate(flat) kind: u32,
};

@vertex
fn glyph_vs(in: GlyphIn) -> GlyphOut {
    var out: GlyphOut;
    let c = corner(in.vi);
    out.clip = to_clip(in.pos + c * in.size);
    out.local = c * in.size;
    out.atlas = in.atlas;
    out.color = in.color;
    out.kind = in.kind;
    return out;
}

@fragment
fn glyph_fs(in: GlyphOut) -> @location(0) vec4<f32> {
    let texel = vec2<i32>(floor(in.atlas + in.local));
    if in.kind == 0u {
        let a = textureLoad(gray_atlas, texel, 0).r;
        return vec4<f32>(in.color.rgb, a * in.color.a);
    }
    let c = textureLoad(color_atlas, texel, 0);
    return vec4<f32>(c.rgb, c.a * in.color.a);
}
