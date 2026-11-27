// 셀 렌더러 셰이더(ADR-035). Ghostty의 shaders.metal과 같은 뼈대: 배경과 글리프를 인스턴스 사각형으로 그린다.
// 좌표는 모두 픽셀(왼쪽 위가 원점). 글리프는 아틀라스에서 텍셀을 그대로 읽는다(필터 없음, 사각형이 픽셀에 딱 맞는다)

struct Globals {
    viewport: vec2<f32>,
    // 글자 덮음 곡선(`text-composition-strategy`, #911): x = 1 / 감마, y = 1 + 명암 / 100. 둘 다 1이면 곡선 없음
    text: vec2<f32>,
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

// 칸 바탕(덮어쓰기): 곱한 알파로 낸다. 알파 255면 `rect_fs`와 같다
@fragment
fn rect_fs_premul(in: RectOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.color.rgb * in.color.a, in.color.a);
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
    // 글리프가 얹히는 칸의 바탕색(0~1, sRGB 값). `kind`의 위 24비트에 실려 온다
    @location(4) @interpolate(flat) bg: vec3<f32>,
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
    out.bg = vec3<f32>(
        f32((in.kind >> 8u) & 255u),
        f32((in.kind >> 16u) & 255u),
        f32((in.kind >> 24u) & 255u),
    ) / 255.0;
    return out;
}

// 글자 덮음 곡선: kitty `cell.slang`의 `foreground_contrast_new`와 같은 식(`bt_core::TextComposition::coverage`).
// 밝기는 sRGB 값에 곱한다(선형으로 바꾸지 않는다). 곡선이 없으면 덮음을 그대로 돌려줘 글자 픽셀이 바뀌지 않는다
fn text_curve(a: f32, fg: vec3<f32>, bg: vec3<f32>) -> f32 {
    if (g.text.x == 1.0 && g.text.y == 1.0) || a <= 0.0 {
        return a;
    }
    let y = vec3<f32>(0.2126, 0.7152, 0.0722);
    let w = (1.0 - dot(fg, y) + dot(bg, y)) * 0.5;
    return clamp(mix(a, pow(a, g.text.x), w) * g.text.y, 0.0, 1.0);
}

@fragment
fn glyph_fs(in: GlyphOut) -> @location(0) vec4<f32> {
    let texel = vec2<i32>(floor(in.atlas + in.local));
    if (in.kind & 255u) == 0u {
        let a = text_curve(textureLoad(gray_atlas, texel, 0).r, in.color.rgb, in.bg);
        return vec4<f32>(in.color.rgb, a * in.color.a);
    }
    let c = textureLoad(color_atlas, texel, 0);
    return vec4<f32>(c.rgb, c.a * in.color.a);
}

// 선형 섞기(`alpha-blending = linear`·`linear-corrected`, #405). 대상을 sRGB 꼴 뷰로 열어 하드웨어가 선형에서 섞게 한다:
// 셰이더는 글자색을 선형으로 바꿔 내고, 대상에서 읽은 바탕은 하드웨어가 선형으로 풀었다가 섞은 뒤 다시 sRGB로 쓴다
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn linear_to_srgb_f(v: f32) -> f32 {
    return select(pow(v, 1.0 / 2.4) * 1.055 - 0.055, v * 12.92, v <= 0.0031308);
}

fn srgb_to_linear_f(v: f32) -> f32 {
    return select(pow((v + 0.055) / 1.055, 2.4), v / 12.92, v <= 0.04045);
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
}

fn glyph_linear(in: GlyphOut, corrected: bool) -> vec4<f32> {
    let texel = vec2<i32>(floor(in.atlas + in.local));
    if (in.kind & 255u) == 0u {
        // 덮음 곡선을 먼저 걸고(kitty는 곡선을 건 덮음으로 섞는다), 그 덮음을 `linear-corrected`의 보정에 넘긴다
        var a = text_curve(textureLoad(gray_atlas, texel, 0).r, in.color.rgb, in.bg);
        let fg = srgb_to_linear(in.color.rgb);
        if corrected {
            // Ghostty(shaders.metal의 cell_text_fragment)와 같은 보정: 글자·바탕 밝기를 sRGB 값으로 섞은 밝기에서
            // 선형 섞기가 그 밝기를 내는 덮음 정도를 다시 구한다. 밝기가 거의 같으면(0.001 안) 보정하지 않는다
            let bg = srgb_to_linear(in.bg);
            let fg_l = luminance(fg);
            let bg_l = luminance(bg);
            if abs(fg_l - bg_l) > 0.001 {
                let blend_l = srgb_to_linear_f(linear_to_srgb_f(fg_l) * a + linear_to_srgb_f(bg_l) * (1.0 - a));
                a = clamp((blend_l - bg_l) / (fg_l - bg_l), 0.0, 1.0);
            }
        }
        return vec4<f32>(fg, a * in.color.a);
    }
    let c = textureLoad(color_atlas, texel, 0);
    return vec4<f32>(srgb_to_linear(c.rgb), c.a * in.color.a);
}

@fragment
fn glyph_fs_linear(in: GlyphOut) -> @location(0) vec4<f32> {
    return glyph_linear(in, false);
}

@fragment
fn glyph_fs_corrected(in: GlyphOut) -> @location(0) vec4<f32> {
    return glyph_linear(in, true);
}
