# 실험 E19: 출력 중 120Hz 화면이 60Hz 꼴로 그림판을 가져가는 까닭을 safe 코드로 고칠 수 있는가

- 날짜: 2026-11-11(조사만 함. 창을 띄워 재지 않았다)
- 관련 이슈: [#429](https://github.com/dj258255/b-terminal/issues/429), [실험 E11](2026-11-11-e11-shader-animation.md), [실험 E16](2026-11-11-e16-gpu-drawing-keys.md), [실험 E18](2026-11-11-e18-input-latency.md), [실험 E14](2026-11-11-e14-gpu-memory-step7.md)
- 원자료: 없음. 이 문서는 문서와 의존성 소스를 읽은 것만 근거로 쓴다. 효과는 재지 않았고 코드는 고치지 않았다

## 가설

> 계속 올리는 동안 화면이 16.6ms마다 한 장씩 그림판을 가져가는 까닭을 가리고, `unsafe` 없이 높은 주사율을 바랄 수 있으면 8.3ms 꼴로 바꿀 수 있다.

## 이 실험이 정할 것

> 이슈 #429의 결정 기준: 까닭을 확정하지 못하거나 safe 길이 없으면 배제한 가설과 함께 적고 닫는다. 이번에는 이쪽이다. 코드는 바꾸지 않았다.

## 조건

- 소스 판: objc2-quartz-core 0.3.2, objc2-app-kit 0.3.2, wgpu-hal 30.0.1, raw-window-metal 1.1.0, 이 저장소의 `vendor/winit`
- 기기 값(M2 Pro 내장 120Hz)은 E11·E16·E18에서 주 세션이 잰 것을 옮긴 것이다. 이번 조사에서 다시 재지 않았다

## 가설별 판정

### H1. 앱의 `FramePacer`가 원인이다: 배제
- E16 A절은 `window-vsync = false`(`Immediate`)에서도 얻기가 프레임마다 약 15ms 막혔다. `Immediate`면 미룸을 끈다(`crates/bt-gpu/src/renderer/window.rs`의 `pacing_applies`). 미룸이 없어도 같은 값이므로 미룸이 만든 간격이 아니다.
- 미룸은 오히려 이 현상에 맞춰 `learned` 간격을 배운다(`crates/bt-gpu/src/pacing.rs`).

### H2. 그림판 수(`maximumDrawableCount`)가 모자라다: 배제(한 가지 조건에서만)
- wgpu-hal은 `setMaximumDrawableCount(maximum_frame_latency + 1)`을 부른다(`wgpu-hal-30.0.1/src/metal/surface.rs`, `configure` 끝).
- E11은 그림판을 2장으로 줄인 #395(E14 A절) 앞의 빌드였다. 그때 기본 지연은 2(그림판 3장)였고 얻기가 12.1~19.4ms(가운데 90%는 15.4~16.6ms)였다. E16은 2장일 때 약 15ms였다. 3장과 2장에서 비슷하므로 장 수가 원인이라는 설명과 맞지 않는다. 다만 두 실험은 하는 일(셰이더 움직임 대 `seq`)이 다르다. 같은 일로 `BT_FRAME_LATENCY=3`과 1을 견주지는 않았다.

### H3. `displaySyncEnabled` 같은 layer 설정이 원인이다: 근거 없음, 배제하지 못함
- wgpu-hal은 `present_mode == Fifo`일 때 `setDisplaySyncEnabled(true)`, `Immediate`일 때 `false`로 둔다. `Immediate`에서도 막혔으므로(E16) 이 값 하나로는 설명되지 않는다.
- `presentsWithTransaction`은 wgpu-hal이 layer에서 읽을 뿐 기본값(`false`)이면 `presentDrawable`로 올린다(`metal/mod.rs`의 `present`). 이 저장소는 바꾸지 않는다. 합성기가 창 합성을 한 주기 늦게 되돌려준다는 설명(IOSurface 크기, 창 크기)은 확인하지 못했다.

### H4. ProMotion이 앱이 높은 주사율을 바라지 않아서 60Hz로 낮춘다: 남김(확정하지 못함)
- 앱이 화면에 알려 주는 높은 주사율 요청은 `CAFrameRateRange`(`preferredFrameRateRange`)다. 이 판 바인딩에서 그 속성이 있는 클래스는 `CADisplayLink`, `CAMetalDisplayLink`, `CAAnimation`뿐이다. `CAMetalLayer`·`CALayer`에는 없다(`objc2-quartz-core-0.3.2/src/generated/`의 `CADisplayLink.rs`, `CAMetalDisplayLink.rs`, `CAAnimation.rs` 113~121줄. `CAMetalLayer.rs`·`CALayer.rs`는 해당 이름이 없다).
- 이 기계에서 ProMotion이 이 창에 실제로 60Hz를 준다는 것을 아직 보이지 않았다. 반대로 120Hz를 줬는데 다른 까닭으로 16.6ms가 되는 경우도 가려내지 못했다. 가리려면 창이 있어야 한다(아래 "다음에 확인할 것").
- Info.plist 키 `CADisableMinimumFrameDurationOnPhone`은 iPhone용이다. 이 저장소에는 Info.plist가 없고 macOS용 대응 키도 찾지 못했다.
- 앱이 아는 주사율: `winit`의 `refresh_rate_millihertz`는 `CGDisplayModeGetRefreshRate`(0이면 CVDisplayLink 공칭 주기)에서 온다(`vendor/winit/src/platform_impl/macos/monitor.rs` 260~283줄). `gpu_frame.rs` 63줄이 이를 `interval_from_millihertz`에 넘긴다. 이 값이 ProMotion에서 120000인지 0 근처인지 로그로 보지 않았다.

## safe로 쓸 수 있는 길: 찾지 못함

| 길 | 판정 | 근거 |
|---|---|---|
| `CADisplayLink`로 `preferredFrameRateRange` 요청 | `unsafe` | `NSView`·`NSWindow`·`NSScreen`의 `displayLinkWithTarget:selector:`가 모두 `unsafe fn`(`objc2-app-kit-0.3.2/src/generated/NSView.rs` 1714줄, `NSWindow.rs` 2235줄, `NSScreen.rs` 212줄 근처). 대상 객체와 selector를 만들 클래스도 따로 정의해야 한다 |
| `CAMetalDisplayLink` | 하지 않음 | 그리기 구조를 바꾸는 큰 일이다(이슈의 "하지 않는 것") |
| `CAAnimation.preferredFrameRateRange`를 layer에 붙임 | 막힘 | setter는 safe지만 `CALayer.sublayers`가 `unsafe fn`이라 wgpu가 만든 `CAMetalLayer`(root layer의 sublayer, `raw-window-metal-1.1.0/src/observer.rs` 81줄)를 safe로 찾을 수 없다. 값을 움직이는 `CABasicAnimation`은 `setFromValue`·`setToValue`가 `unsafe fn`이다. 값이 안 움직이는 애니메이션이 화면 주사율을 올리는지도 모른다. 이 저장소는 `NSWindow`를 `NSApplication.keyWindow`로만 얻는다(`quick.rs`의 주석) |
| wgpu `SurfaceConfiguration` | 없음 | `present_mode`, `desired_maximum_frame_latency`, `color_space` 등. 주사율 힌트 필드가 없다 |
| winit | 없음 | 이 판(vendor)에는 창 주사율 힌트 API가 없다 |

의존성은 더하지 않았다.

## 결론과 적용 범위

- 이 조사 범위(위 소스 판들, 읽기만 함)에서 `unsafe` 없이 주사율을 바랄 safe 길을 찾지 못했고, 까닭도 H4 하나를 남긴 채 확정하지 못했다. 그래서 고치지 않았다. 이슈 #429의 결정 기준에 따른 결과다.
- 이 문서의 효과 값은 모두 재지 않았다. 새로 잰 숫자는 없다.

## 다음에 확인할 것

창을 띄워 쟀을 때만 가릴 수 있는 것이다(조사자는 창을 띄우지 않았다).
1. `RUST_LOG=debug`로 `seq 1 300000`을 돌리며 `다시 그리기` 줄의 `얻기`를 본다. 같은 일에서 `BT_FRAME_LATENCY=1`, `2`, `3`을 견준다(H2를 같은 일로 확정).
2. 출력 중 화면이 몇 Hz인지 직접 본다(시스템 설정의 화면 주사율, 저전력 모드 여부, 자동 전환 설정을 기록한다. 이 설정은 조사자가 건드리지 않았다).
3. 위 1·2에서 화면이 120Hz인데도 그림판이 16.6ms마다 돌아오면 H4는 틀린 것이고 합성기(H3) 쪽이다. 화면이 60Hz로 내려가 있다면 H4가 맞고, 길은 `CADisplayLink`(`unsafe` 필요)나 `CAMetalDisplayLink`(구조 변경)뿐이다. 그 길은 `unsafe_code = "deny"`를 정하는 사람이 정할 일이다.
4. 시작 로그에 `winit`이 준 `refresh_rate_millihertz` 값을 한 줄 더하면 앱이 아는 주사율을 바로 볼 수 있다(이번에는 코드를 고치지 않아 넣지 않았다).
