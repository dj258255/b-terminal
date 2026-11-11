# 실험 E16: GPU 길 그리기 키(window-vsync, alpha-blending)

- 날짜: 2026-11-11(주 세션이 잼)
- 관련 이슈: [#403](https://github.com/dj258255/b-terminal/issues/403)(추적), [#404](https://github.com/dj258255/b-terminal/issues/404), [#405](https://github.com/dj258255/b-terminal/issues/405)
- 원자료: 주 세션 scratchpad의 `e16/` 폴더(debug 로그, 화면 찍은 것). 저장소에는 넣지 않는다
- 구현은 서브에이전트가 각자 worktree에서 했고, 실제 창 측정은 주 세션이 했다

## 조건(공통)

- 기기: Apple M2 Pro, macOS 26, 내장 화면(120Hz), `--release`, 창 1000×680pt(2000×1360px), GPU 길(기본)
- 격리: `HOME`을 빈 폴더로, `TMPDIR`을 짧은 경로로. 설정은 `window-save-state = never`, `shell-integration = none`, `cursor-style-blink = false`
- 회마다 앱을 새로 띄우고, 견줄 두 쪽을 번갈아 돌린다
- 기준 빌드: origin/main 488c2c7(0.37.0)

## A. window-vsync(#404)

### 가설

> `window-vsync = false`면 surface를 `Immediate`(Metal `displaySyncEnabled = false`)로 열고 `FramePacer`의 미룸을 끈다. 화면 갱신을 기다리지 않으므로 빠른 출력이 빨리 끝나고 그림판 얻기 대기가 준다.

### 방법

- `seq 1 300000`을 `btctl send`로 보내고 `btctl wait`로 끝날 때까지의 시간을 잰다.
- debug 로그에서 올린 프레임마다 `얻기` 시간을 모은다.
- `true`·`false`를 세 쌍 번갈아 잰다. 기본값(`true`)과 기준 빌드도 세 쌍 견준다.
- 1분 부하 7~16

### 결과

| | 걸린 시간 | 얻기 합 | 프레임 얻기 중앙값 / 최대 | 1ms 넘는 프레임 |
|---|---|---|---|---|
| 기준 빌드(488c2c7) | 0.56~0.59초 | 14~16ms | - | - |
| `true`(기본) | 0.57~0.93초(첫 회 0.93, 나머지 0.57~0.60) | 3~19ms | 0.03ms / 4.4ms(한 회) | 3 / 35(한 회) |
| `false` | 0.39~0.43초 | 351~375ms | 14.3~15.6ms / 24.6~25.1ms(두 회) | 25 / 40, 23 / 27(두 회) |
| `false` + 미룸 켬(실험 빌드, 넣지 않음) | 0.56~0.58초 | 15~17ms | 0.01~0.02ms / 5.6~14.0ms | 1~5 / 27~36 |

- `false`면 로그에 `화면 갱신 맞춤 끔(Immediate)`이 찍힌다. 이 기기 어댑터는 `Immediate`를 준다.
- `false`는 빠른 출력이 약 30% 빨리 끝났다.
- 대신 출력 중 프레임마다 그림판을 얻느라 약 15ms씩 메인 스레드가 막혔다. 120Hz 주기(8.3ms)의 약 두 배로, 그림판이 60Hz 꼴로 돌아오는 것처럼 보인다. 까닭은 가리지 않았다.
- `Immediate`에서 미룸을 켜 두면(실험 빌드) 막힘은 사라지지만, 걸린 시간도 `true`와 같아져 키가 하는 일이 없어진다.
- 기본값(`true`)은 기준 빌드와 같은 범위다.

### 판정

넣는다. Ghostty의 뜻(맞추지 않고 올림)대로 `false`는 미룸도 끈다. 기본은 `true` 그대로라 동작이 바뀌지 않는다. 잰 트레이드오프(빠른 출력은 빠르지만 메인 스레드가 막힘)를 설정 문서에 적었다.

## B. alpha-blending(#405)

### 가설

> 글자 파이프라인만 대상 텍스처의 sRGB 꼴 뷰에 그리면, 하드웨어가 선형으로 풀어 섞고 다시 sRGB로 쓴다. 그러면 `linear`는 색 짝 글자 가장자리가 어두워지지 않는다. `linear-corrected`는 Ghostty의 보정 식(바탕·글자 밝기로 알파를 다시 구함)으로 `native`와 굵기가 비슷하다. `native`는 지금과 비트까지 같다.

### 방법

- 에이전트가 구현했다. 보정 식은 Ghostty `src/renderer/shaders/shaders.metal`의 `cell_text_fragment` 회색조 분기를 옮겼다(main a622196). Ghostty는 곱한 알파 색에 곱하고, b-terminal은 곱하지 않은 알파를 쓰므로 알파에만 적용했다.
- 창 없는 시험(`tests/alpha_blending.rs`)이 덮음 정도 0.2~0.8인 가장자리 595픽셀을 기대한 섞기 값과 견준다.
- 주 세션이 실제 창에서 쟀다.
  - 색 짝 글자 장면(빨강/초록, 초록/빨강, 검정/회색, 흰/검정 등 여덟 줄)을 기준 빌드, 설정 없음, `native`, `linear`, `linear-corrected`로 찍었다.
  - 열린 창에서 설정 파일을 `linear`로 바꿔 `btctl reload-config`, 다시 되돌려 찍었다.
  - 이모지 장면을 `linear-corrected`로 찍었다.
  - 탭 열고 닫기 열 번을 기준 / `native` / `linear-corrected`로 두 쌍 쟀다.
  - 1분 부하 2.6~5.0

### 결과

- 화면
  - 설정 없음과 `native`는 기준과 다른 픽셀이 0이었다.
  - `linear`은 24,439픽셀, `linear-corrected`은 19,298픽셀이 달랐다(글자 가장자리).
  - 확대해 보면 `linear`은 어두운 글자가 가늘고 밝은 글자가 굵다. `linear-corrected`는 `native`와 굵기가 거의 같다(Ghostty 설명과 같음).
  - 두 모드 모두 로그에 오류·검증 경고가 없었다.
- 설정 다시 읽기: 열린 창이 다음 프레임부터 바뀌었다. 창 오른쪽 아래 알림 띠를 뺀 위쪽 1250줄이 처음부터 `linear`로 띄운 화면, 되돌린 뒤 기준과 각각 다른 픽셀 0이었다.
- 이모지(`linear-corrected`): 색으로 그려지고 오류가 없었다.
- 통째 프레임(올린 프레임만, 두 쌍)

| | 그리기 중앙값 | 올리기까지 중앙값 | GPU 인코딩 중앙값(최대) |
|---|---|---|---|
| 기준 | 2.55~2.94ms | 5.38ms | 0.16~0.18ms(0.62) |
| `native` | 2.56~2.83ms | 5.38~5.39ms | 0.16~0.17ms(0.64) |
| `linear-corrected` | 2.49~2.55ms | 5.04~5.32ms | 0.20~0.24ms(1.08) |

- 창 없는 측정(에이전트, 200×60 글자 화면, 세 번 중앙값): `linear`은 `native` 대비 −0.1~+0.1ms, `linear-corrected`은 0~+0.35ms(한 번은 이슈 기준 +0.3ms를 넘음).

### 판정

넣는다. 기본(`native`)은 화면과 시간이 기준과 같다. CPU 길은 `native`로만 그리고, 다른 값이면 로그에 경고한다(설정 문서에 적음).

## 한계

- 한 기기(Apple M2 Pro 내장 120Hz 화면)의 값이다. 60Hz 외부 화면, ProMotion을 끈 화면은 재지 않았다.
- 키를 친 뒤 화면에 보이기까지의 지연(입력 지연)은 재지 않았다. 키 입력을 흉내 내지 않는다.
- `false`에서 화면이 찢어지는지, Ghostty가 적은 macOS 14.4+ 커널 패닉 위험은 확인하지 않았다.
- B는 글자(마스크 글리프, 이모지)만 선형으로 섞는다. 칸 바탕, 커서, 밑줄·선택, 층(kitty 그림, 배경 그림, 탭 막대)은 sRGB 값 그대로다. 사용자 셰이더가 도는 동안은 `native`로 그린다.
- B를 투명 창(`background-opacity` < 1)에서는 보지 않았다.
