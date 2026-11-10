// 층 텍스처를 대상 전체에 1:1로 그린다(ADR-036). 창 크기 텍스처라 늘리거나 줄이지 않는다: 픽셀 좌표로 텍셀을 그대로 읽는다.
// 아래층은 블렌드 없이 덮어쓰고, 위층은 곱한 알파로 얹는다(블렌드는 파이프라인에서 정한다)

@group(0) @binding(0) var layer: texture_2d<f32>;

@vertex
fn layer_vs(@builtin(vertex_index) vi: u32) -> @builtin(position) vec4<f32> {
    // 화면을 덮는 큰 삼각형 하나
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    return vec4<f32>(p[vi], 0.0, 1.0);
}

@fragment
fn layer_fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(layer, vec2<i32>(pos.xy), 0);
}
