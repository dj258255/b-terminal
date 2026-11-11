# 실험 E19: 출력 중 120Hz 화면이 60Hz 꼴로 그림판을 가져가는 까닭을 safe 코드로 고칠 수 있는가

- 날짜: 2026-11-11(서브에이전트가 소스를 읽어 조사했고, 주 세션이 실제 창에서 쟀다)
- 관련 이슈: [#429](https://github.com/dj258255/b-terminal/issues/429), [실험 E11](2026-11-11-e11-shader-animation.md), [실험 E16](2026-11-11-e16-gpu-drawing-keys.md), [실험 E18](2026-11-11-e18-input-latency.md), [실험 E14](2026-11-11-e14-gpu-memory-step7.md)
- 원자료: 주 세션 scratchpad의 `e16/out/e19-*.log`, `e19x-*.log`. 저장소에는 넣지 않는다

## 가설

> 계속 올리는 동안 화면이 16.6ms마다 한 장씩 그림판을 가져가는 까닭을 가리고, `unsafe` 없이 높은 주사율을 바랄 수 있으면 8.3ms 꼴로 바꿀 수 있다.

## 이 실험이 정할 것

> 이슈 #429의 결정 기준: 까닭을 확정하지 못하거나 safe 길이 없으면 배제한 가설과 함께 적고 닫는다. 이번에는 이쪽이다. 고친 것은 화면 주기가 바뀔 때 남기는 로그 한 줄뿐이다.

## 조건

- 소스 판: objc2-quartz-core 0.3.2, objc2-app-kit 0.3.2, wgpu-hal 30.0.1, raw-window-metal 1.1.0, 이 저장소의 `vendor/winit`
- 기기: Apple M2 Pro 내장 120Hz(ProMotion), macOS 26, `--release`, 0.41.0(51c4ebd) 위에 로그 한 줄을 더한 빌드
- 측정 때 **저전력 모드가 켜져 있었다**(`pmset -g`의 `lowpowermode 1`). 주 세션은 시스템 설정을 바꾸지 않으므로 끈 상태는 재지 않았다
- 일: 창 하나에서 `seq 1 300000`(E16과 같은 `out.sh`), 회마다 앱을 새로 띄움. 1분 부하는 4.9~10.4
- E11·E16·E18의 값은 그 실험에서 잰 것을 옮겼다. 그때 저전력 모드였는지는 적어 두지 않았다

## 주 세션 측정

### 앱이 들은 주사율
새 로그 "화면 한 장 간격"이 여섯 회 모두 8.33ms(120Hz)를 찍었다. winit(`CGDisplayModeGetRefreshRate`)은 저전력 모드에서도 120Hz를 알린다.

### 그림판 수(`BT_FRAME_LATENCY`)
| 그림판 | 회 | 걸린 시간 | 프레임 | 미룬 프레임 | 얻기 평균 |
|---|---|---|---|---|---|
| 2장(지연 1, 기본) | 1 | 0.98s | 34 | 32 | 0.34ms |
| 2장 | 2 | 0.57s | 35 | 33 | 0.38ms |
| 3장(지연 2) | 1 | 0.57s | 34 | 30 | 0.63ms |
| 3장 | 2 | 0.57s | 36 | 33 | 0.64ms |
| 4장(지연 3) | 1 | 0.56s | 36 | 31 | 1.06ms |
| 4장 | 2 | 0.55s | 36 | 32 | 1.00ms |

- 장 수를 바꿔도 걸린 시간과 프레임 수가 같았다(첫 회 0.98s는 첫 띄움으로 보지만 까닭은 가리지 않았다). 0.57초에 35장이면 한 장에 약 16ms다.
- 얻기가 작은 것은 미룸이 거의 모든 프레임을 미뤄 막힘을 앞서 피했기 때문이다. 그래서 이 표만으로는 화면이 어떤 주기로 가져가는지 알 수 없다.

### 미룸을 끈 실험 빌드
실험 빌드(저장소에 넣지 않음)로 프레임마다 앞 장과의 간격, 얻기에서 기다린 시간, 배운 간격을 찍었다. 환경 변수로 미룸을 끌 수 있게 했다. 그림판 2장, 두 회씩이다. 간격은 처음 두 장을 뺐다.

| 미룸 | 회 | 걸린 시간 | 프레임 | 앞 장과 간격(가운데, 10~90%) | 얻기 기다림(가운데) | 배운 간격(끝) |
|---|---|---|---|---|---|---|
| 켬 | 1 | 1.06s | 34 | 17.61ms(16.83~19.11) | 0.02ms | 15.78ms |
| 켬 | 2 | 0.56s | 35 | 16.73ms(16.18~17.66) | 0.02ms | 16.73ms |
| 끔 | 1 | 0.38s | 32 | 16.57ms(1.06~17.31) | 15.75ms | 16.42ms |
| 끔 | 2 | 0.37s | 29 | 16.63ms(1.33~17.13) | 15.80ms | 16.37ms |

- 미룸을 끄면 처음 몇 장(그림판이 비어 있는 동안)은 1ms 간격으로 올라가다가, 그 뒤로는 얻기가 프레임마다 약 16ms 막히며 16.6ms 간격으로 줄을 섰다. **앱이 120Hz를 들었는데 화면은 그림판을 16.6ms마다 돌려줬다.**
- 미룸을 켜면 같은 16.6ms 간격으로 올리면서 얻기 막힘은 0.02ms였다. 미룸은 16ms를 만든 것이 아니라 화면이 돌려주는 간격을 배워 따라갔다.
- 미룸을 끈 회가 더 빨리 끝났다(0.37~0.38s 대 0.56s). 까닭은 가리지 않았다. 셸 출력과 그리기의 순서가 달라진 것으로 보지만 재지 않았다.

### 저전력 모드
- 저전력 모드에서 ProMotion 맥북 프로가 60Hz까지로 묶인다는 사용자 보고가 여럿 있다(MacRumors 포럼 등). Apple 공식 문서는 찾지 못했다.
- 이번 측정은 이 보고와 맞는다. 하지만 저전력 모드를 끈 상태를 재지 않았으므로 원인을 저전력 모드로 확정하지 않는다.

### E18 B절과의 관계
E18 B절에서 출력 중 입력 지연의 "그리기 시작까지"는 약 8.5ms였다. 미룸이 16.6ms 주기를 따르면, 입력이 주기의 아무 때나 들어올 때 다음 올림까지 평균 약 8.3ms를 기다린다. 값이 맞지만 셈으로 낸 것이고, E18 B 조건(분할 창)에서 간격을 함께 재지는 않았다.

## 가설별 판정

### H1. 앱의 `FramePacer`가 원인이다: 배제(측정으로 확정)
- 미룸을 끈 실험 빌드에서도 그림판은 16.6ms마다 돌아왔다(위 표).
- E16 A절은 `window-vsync = false`(`Immediate`)에서도 얻기가 프레임마다 약 15ms 막혔다. `Immediate`면 미룸을 끈다(`crates/bt-gpu/src/renderer/window.rs`의 `pacing_applies`). 미룸이 없어도 같은 값이므로 미룸이 만든 간격이 아니다.
- 미룸은 오히려 이 현상에 맞춰 `learned` 간격을 배운다(`crates/bt-gpu/src/pacing.rs`).

### H2. 그림판 수(`maximumDrawableCount`)가 모자라다: 배제(같은 일로 확정)
- 같은 `seq` 일에서 그림판 2·3·4장의 걸린 시간과 프레임 수가 같았다(위 표).
- wgpu-hal은 `setMaximumDrawableCount(maximum_frame_latency + 1)`을 부른다(`wgpu-hal-30.0.1/src/metal/surface.rs`, `configure` 끝).
- E11은 그림판을 2장으로 줄인 #395(E14 A절) 앞의 빌드였다. 그때 기본 지연은 2(그림판 3장)였고 얻기가 12.1~19.4ms(가운데 90%는 15.4~16.6ms)였다. E16은 2장일 때 약 15ms였다. 3장과 2장에서 비슷하므로 장 수가 원인이라는 설명과 맞지 않는다. 다만 두 실험은 하는 일(셰이더 움직임 대 `seq`)이 다르다. 이번에 같은 일로 견줬다.

### H3. `displaySyncEnabled` 같은 layer 설정이 원인이다: 근거 없음, 배제하지 못함
- wgpu-hal은 `present_mode == Fifo`일 때 `setDisplaySyncEnabled(true)`, `Immediate`일 때 `false`로 둔다. `Immediate`에서도 막혔으므로(E16) 이 값 하나로는 설명되지 않는다.
- `presentsWithTransaction`은 wgpu-hal이 layer에서 읽을 뿐 기본값(`false`)이면 `presentDrawable`로 올린다(`metal/mod.rs`의 `present`). 이 저장소는 바꾸지 않는다. 합성기가 창 합성을 한 주기 늦게 되돌려준다는 설명(IOSurface 크기, 창 크기)은 확인하지 못했다.

### H4. ProMotion이 60Hz로 낮춘다: 저전력 모드에서는 맞음, 끈 상태는 확정하지 못함
- 저전력 모드에서 앱은 120Hz를 듣지만 화면은 16.6ms마다 돌려줬다(위 측정). 낮춘 까닭이 저전력 모드인지, 앱이 높은 주사율을 바라지 않아서인지는 저전력 모드를 끈 측정이 있어야 가린다.
- 앱이 화면에 알려 주는 높은 주사율 요청은 `CAFrameRateRange`(`preferredFrameRateRange`)다. 이 판 바인딩에서 그 속성이 있는 클래스는 `CADisplayLink`, `CAMetalDisplayLink`, `CAAnimation`뿐이다. `CAMetalLayer`·`CALayer`에는 없다(`objc2-quartz-core-0.3.2/src/generated/`의 `CADisplayLink.rs`, `CAMetalDisplayLink.rs`, `CAAnimation.rs` 113~121줄. `CAMetalLayer.rs`·`CALayer.rs`는 해당 이름이 없다).
- Info.plist 키 `CADisableMinimumFrameDurationOnPhone`은 iPhone용이다. 이 저장소에는 Info.plist가 없고 macOS용 대응 키도 찾지 못했다.
- 앱이 아는 주사율: `winit`의 `refresh_rate_millihertz`는 `CGDisplayModeGetRefreshRate`(0이면 CVDisplayLink 공칭 주기)에서 온다(`vendor/winit/src/platform_impl/macos/monitor.rs` 260~283줄). `gpu_frame.rs` 63줄이 이를 `interval_from_millihertz`에 넘긴다. 이번에 더한 로그로 120000(8.33ms)임을 봤다.

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

- 출력 중 16.6ms 주기는 앱 밖(화면·합성기)에서 온다. 이 기계에서는 저전력 모드가 켜진 상태에서 잰 값이다. 미룸(ADR-038)은 그 주기를 배워 얻기 막힘을 없애고 있으며, 고칠 것이 없다.
- 이 조사 범위(위 소스 판들)에서 `unsafe` 없이 주사율을 바랄 safe 길을 찾지 못했다. 그래서 그리기 코드는 고치지 않았다. 이슈 #429의 결정 기준에 따른 결과다.
- 고친 것: 화면이 알려 준 한 장 간격이 바뀔 때 info 로그 "화면 한 장 간격: …ms"를 남긴다. 앱이 듣는 값과 실제 주기를 견줄 때 쓴다.
- 적용 범위: 이 기계 하나(내장 화면, 저전력 모드 켬), `seq` 출력 한 가지 일, 회마다 두 번이다.

## 다음에 확인할 것

1. 저전력 모드를 끄고(범수 님이 시스템 설정에서) 같은 `seq` 일을 `RUST_LOG=b_terminal=debug,bt_gpu=info`로 돌린다. "다시 그리기" 줄의 `얻기`와 미룬 프레임 수를 본다. 미룬 프레임이 거의 없고 프레임 수가 두 배쯤 되면 저전력 모드가 원인이다.
2. 끈 상태에서도 16.6ms면 ProMotion이 앱을 60Hz로 둔 것(H4)이거나 합성기(H3)다. 그때 길은 `CADisplayLink`(`unsafe` 필요)나 `CAMetalDisplayLink`(구조 변경)뿐이고, `unsafe_code = "deny"`를 풀지는 범수 님이 정한다.
3. E18 B절(분할 창, 출력 중 입력)을 저전력 모드를 끄고 다시 재면, "그리기 시작까지"가 약 4ms로 줄어드는지 본다.
