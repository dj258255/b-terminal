// `window-colorspace = srgb`(#466): 중간 텍스처의 sRGB 값을 Display P3 값으로 바꿔 surface에 쓴다. `p3.rs`의 `srgb_to_display_p3`와 같은 식.
// 입력은 곱한 알파(창이 투명일 때)일 수 있으므로 알파로 나눠 바꾸고 다시 곱한다

@group(0) @binding(0) var src: texture_2d<f32>;

@vertex
fn p3_vs(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(p[vi], 0.0, 1.0);
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

fn from_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c * 12.92;
    let hi = pow(c, vec3<f32>(1.0 / 2.4)) * 1.055 - vec3<f32>(0.055);
    return select(hi, lo, c <= vec3<f32>(0.0031308));
}

// 선형 sRGB → 선형 Display P3(D65). 열 우선: 열 i는 행렬의 i번째 열(`SRGB_TO_DISPLAY_P3`의 [행][열]에서 [*][i])
const SRGB_TO_P3 = mat3x3<f32>(
    vec3<f32>(0.8224620, 0.0331942, 0.0170826),
    vec3<f32>(0.1775380, 0.9668058, 0.0723974),
    vec3<f32>(0.0000000, 0.0000000, 0.9105199),
);

@fragment
fn p3_fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let c = textureLoad(src, vec2<i32>(pos.xy), 0);
    var rgb = c.rgb;
    if c.a > 0.0 {
        rgb = rgb / c.a;
    }
    let out = from_linear(clamp(SRGB_TO_P3 * to_linear(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0))), vec3<f32>(0.0), vec3<f32>(1.0)));
    return vec4<f32>(out * c.a, c.a);
}
