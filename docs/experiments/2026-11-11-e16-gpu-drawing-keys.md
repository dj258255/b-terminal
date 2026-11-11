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
| `true`(기본) | 0.57~0.93초(첫 회 0.93, 나머지 0.57~0.60) | 3~19ms | 0.03ms / 4.4ms | 3 / 35 |
| `false` | 0.39~0.43초 | 351~375ms | 14.3~15.6ms / 24.6~25.1ms | 23~25 / 27~40 |
| `false` + 미룸 켬(실험 빌드, 넣지 않음) | 0.56~0.58초 | 15~17ms | 0.01~0.02ms / 5.6~14.0ms | 1~5 / 27~36 |

- `false`면 로그에 `화면 갱신 맞춤 끔(Immediate)`이 찍힌다. 이 기기 어댑터는 `Immediate`를 준다.
- `false`는 빠른 출력이 약 30% 빨리 끝났다.
- 대신 출력 중 프레임마다 그림판을 얻느라 약 15ms씩 메인 스레드가 막혔다. 120Hz 주기(8.3ms)의 약 두 배로, 그림판이 60Hz 꼴로 돌아오는 것처럼 보인다. 까닭은 가리지 않았다.
- `Immediate`에서 미룸을 켜 두면(실험 빌드) 막힘은 사라지지만, 걸린 시간도 `true`와 같아져 키가 하는 일이 없어진다.
- 기본값(`true`)은 기준 빌드와 같은 범위다.

### 판정

넣는다. Ghostty의 뜻(맞추지 않고 올림)대로 `false`는 미룸도 끈다. 기본은 `true` 그대로라 동작이 바뀌지 않는다. 잰 트레이드오프(빠른 출력은 빠르지만 메인 스레드가 막힘)를 설정 문서에 적었다.

## 한계

- 한 기기(Apple M2 Pro 내장 120Hz 화면)의 값이다. 60Hz 외부 화면, ProMotion을 끈 화면은 재지 않았다.
- 키를 친 뒤 화면에 보이기까지의 지연(입력 지연)은 재지 않았다. 키 입력을 흉내 내지 않는다.
- `false`에서 화면이 찢어지는지, Ghostty가 적은 macOS 14.4+ 커널 패닉 위험은 확인하지 않았다.
