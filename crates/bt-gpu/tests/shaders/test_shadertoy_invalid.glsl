// 출처: ghostty-org/ghostty src/renderer/shaders/test_shadertoy_invalid.glsl (커밋 1b22f331a58a5be5ede2f1d76b7793d4130b4072, MIT 라이선스).
// 원문 그대로이고 맨 위 이 주석 두 줄만 더했다
vec2 curve(vec2 uv)
{
	uv = (uv - 0.5) * 2.0;
	uv *= 1.1;
	uv.x *= 1.0 + pow((abs(uv.y) / 5.0), 2.0);
	uv.y *= 1.0 + pow((abs(uv.x) / 4.0), 2.0);
	uv  = (uv / 2.0) + 0.5;
	uv =  uv *0.92 + 0.04;
	return uv;
}

// Missing mainImage!
