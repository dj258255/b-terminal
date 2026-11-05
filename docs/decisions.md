# 설계 결정 기록

입력 경로, 보안 경계, 장기 제약을 만드는 선택을 적습니다. 지난 판단은 지우지 않고 새 ADR에서 대체 관계를 적습니다.

## 목차

- [ADR-001 VT 엔진은 alacritty_terminal로 시작하고, bt-term 밖으로 엔진 타입을 내보내지 않는다](#adr-001-vt-엔진은-alacritty_terminal로-시작하고-bt-term-밖으로-엔진-타입을-내보내지-않는다)
- [ADR-002 렌더러는 CPU(softbuffer + fontdue)로 시작한다](#adr-002-렌더러는-cpusoftbuffer--fontdue로-시작한다)
- [ADR-003 크레이트를 여섯으로 나누고 창 라이브러리는 bt-app만 쓴다](#adr-003-크레이트를-여섯으로-나누고-창-라이브러리는-bt-app만-쓴다)
- [ADR-004 한글 IME를 위해 winit을 vendor로 고쳐 쓴다](#adr-004-한글-ime를-위해-winit을-vendor로-고쳐-쓴다)

## ADR-001 VT 엔진은 alacritty_terminal로 시작하고, bt-term 밖으로 엔진 타입을 내보내지 않는다

상태: 채택
관련: [#2](https://github.com/dj258255/b-terminal/issues/2), [조사 기록](research/2026-11-05-ai-terminal-landscape.md#4-vt-엔진-선택지)

### 맥락
- 범수 님은 Ghostty를 쓰고, b-terminal이 Ghostty만큼 동작하길 원한다(2026-11-05). Ghostty와 같은 해석을 얻는 가장 곧은 길은 libghostty다.
- libghostty는 Zig로 빌드한다. 이 기기에 Zig가 없고(`zig not found`), API가 아직 안정이 아니라 버전을 고정하라는 권고가 있다. herdr는 `crates/ghostty-vt`로 Rust에서 쓰고 있다.
- alacritty_terminal 0.26은 순수 Rust라 `cargo build` 하나로 된다. Copad가 macOS에서 SwiftTerm을 버리고 이것을 골랐다(한글 IME·배경 위 커서·SGR 7 문제).
- 엔진을 나중에 바꿀 수 있어야 한다. libghostty가 안정되면 옮길 이유가 크다(그래픽 프로토콜, Ghostty와 같은 해석).

### 검토한 선택지
| 방식 | 판단 |
|---|---|
| **alacritty_terminal + bt-term이 자체 화면 모델(`Screen`·`Cell`)로 감싸기** | 채택. cargo만으로 빌드하고, 렌더러·앱은 엔진 타입을 모른다 |
| libghostty-vt(ghostty-vt 크레이트) | 미룸. Zig 설치와 바인딩 빌드를 사람 없이 밤사이 확인하기 어렵다. 엔진 교체 이슈로 다시 연다 |
| xterm.js + 웹뷰 | 기각. 웹뷰 안의 한글 IME와 폭 표 차이, 그리고 범수 님이 웹 화면을 원하지 않았다(b-studio ADR-186) |
| 직접 해석기 작성 | 기각. 오래 걸리고 얻을 것이 없다 |

### 결정
1. bt-term이 alacritty_terminal의 `Term`을 가진다. 밖으로는 `Screen { cols, rows, cells, cursor, modes }` 같은 자체 값만 낸다.
2. PTY 바이트는 bt-term의 읽기 스레드가 받아 VT 해석기와 알림 스캐너에 같은 순서로 먹인다(alacritty의 EventLoop를 쓰지 않는다. 바이트를 가로챌 자리가 없다).
3. 넓은 글자 폭은 엔진의 판단(`WIDE_CHAR`·`WIDE_CHAR_SPACER`)을 그대로 따른다.

### 감수한 트레이드오프
- Kitty 그래픽·Sixel이 없다. b-studio desk의 계정 그림은 반칸 블록으로 보인다(Ghostty에서는 Kitty 그래픽으로 볼 수 있던 것).
- Ghostty와 해석이 다른 곳이 있을 수 있다. 다르게 보이는 화면을 찾으면 troubleshooting에 적고, 쌓이면 libghostty 교체의 근거로 쓴다.

## ADR-002 렌더러는 CPU(softbuffer + fontdue)로 시작한다

상태: 채택
관련: [#2](https://github.com/dj258255/b-terminal/issues/2), [#5](https://github.com/dj258255/b-terminal/issues/5)

### 맥락
- Ghostty·Warp·Copad macOS는 GPU로 그린다. Copad 제작기는 "CPU로 렌더하는 터미널에서 성능의 아쉬움을 느껴본 적은 크게 없다"고 적었고, Metal 글리프 아틀라스에서 많은 문제를 만났다.
- 렌더러를 창 없이 테스트하고 싶다. 버퍼에 그려 픽셀을 보면 CI에서 회귀를 잡을 수 있다.

### 검토한 선택지
| 방식 | 판단 |
|---|---|
| **softbuffer(창에 픽셀 버퍼 올리기) + fontdue(래스터) + fontdb(시스템 글꼴 찾기)** | 채택. 그리기가 `&mut [u32]`에 하는 순수 함수라 테스트할 수 있다 |
| wgpu + glyphon | 미룸. 셀 배경용 셰이더와 아틀라스 관리가 필요하고 창 없이 테스트하기 어렵다 |
| CoreText 직접 | 기각. macOS 전용, unsafe가 많다 |

### 결정
1. bt-render는 `Screen`을 받아 `&mut [u32]`(0RGB)에 그린다. 창을 모른다.
2. 글리프는 (글자, 굵게, 기울임) 단위로 캐시한다. 기본 글꼴에 없는 글자는 대체 글꼴 목록(한글: Apple SD Gothic Neo 등)에서 찾는다.
3. 프레임 시간을 재는 실험을 0.1 안에서 한 번 한다. 8ms를 넘으면 바뀐 줄만 그리기를 먼저 하고, 그래도 넘으면 GPU 이슈를 연다.

### 감수한 트레이드오프
- 4K·Retina에서 전체 다시 그리기가 느릴 수 있다(재지 않았다).
- 서브픽셀 안티에일리어싱 없이 회색조로 그린다.

## ADR-003 크레이트를 여섯으로 나누고 창 라이브러리는 bt-app만 쓴다

상태: 채택
관련: [#2](https://github.com/dj258255/b-terminal/issues/2), [#3](https://github.com/dj258255/b-terminal/issues/3)

### 결정
| 크레이트 | 맡는 것 | 창 의존 |
|---|---|---|
| bt-core | 설정, 제어 프로토콜, 에이전트 상태 모델, 테마 | 없음 |
| bt-term | PTY, VT 해석, 알림 스캐너, 키 → 바이트 | 없음 |
| bt-render | 글꼴, 글리프 캐시, 격자·사이드바 그리기 | 없음 |
| bt-studio | b-studio API 손님(HTTP·SSE) | 없음 |
| bt-ctl | `btctl` 실행 파일(제어 소켓 손님, 훅 받기) | 없음 |
| bt-app | `b-terminal` 실행 파일(winit 창, 입력, 탭, 제어 소켓 서버) | winit·softbuffer |

- 창 라이브러리는 winit 0.30(0.31은 베타)을 쓴다. IME 조합(`Ime::Preedit`·`Ime::Commit`)을 이벤트로 준다.
- Linux CI는 bt-app까지 빌드하지만 실제 창 확인은 macOS에서 사람이 한다.

## ADR-004 한글 IME를 위해 winit을 vendor로 고쳐 쓴다

상태: 채택
관련: [#6](https://github.com/dj258255/b-terminal/issues/6), [vendor/README.md](../vendor/README.md)

### 맥락
- macOS 한글 IME는 조합 중 스페이스·Enter를 누르면 한 keyDown 안에서 확정(`insertText`)과 그 키의 동작(두 번째 `insertText` 또는 `doCommandBySelector`)을 차례로 부른다.
- winit 0.30.13은 확정 직후의 두 번째 호출을 버리고 그 키의 `KeyboardInput`도 보내지 않는다. 코드 주석이 "한국어 IME에서 두 번 보내는 것"과 "IME 확정에 쓴 Enter"를 피하려는 의도라고 적었는데, 터미널에서는 스페이스가 사라지고 Enter를 두 번 눌러야 하는 결과가 된다. winit main(0.31 베타)에도 같은 코드가 있다.
- Copad는 SwiftTerm에서 한글 조합이 깨져 엔진을 바꿨다. 한글 입력은 범수 님이 매일 쓰는 길이라 미룰 수 없다.

### 검토한 선택지
| 방식 | 판단 |
|---|---|
| **winit 0.30.13을 vendor로 넣고 두 군데만 고친다** | 채택. 고친 줄이 적고(`b-terminal 패치`로 찾음), 되돌릴 조건이 분명하다 |
| winit에 PR을 내고 기다린다 | 병행할 수 있지만 그동안 쓸 수 없다 |
| AppKit 창을 직접 쓴다(objc2) | 창·입력 전체를 다시 짜야 한다. Linux를 잃는다 |
| 앱에서 추측해 보정(Commit 뒤 일정 시간 안의 키를 스페이스로 본다) | 키가 아예 오지 않으므로 추측할 근거가 없다 |

### 결정
1. `vendor/winit`(예제·문서·테스트 제외)을 루트 `[patch.crates-io]`로 연결한다.
2. 확정 직후 같은 keyDown 안의 `insertText`는 `Ime::Commit`으로 넘긴다. 확정 직후의 `doCommandBySelector`는 키를 앱에 넘긴다.
3. winit이 고친 판을 내면 패치를 지운다. 그 전에 한글 2벌식으로 "안녕 하세요⏎"가 한 번에 들어가는지 확인한다.

### 감수한 트레이드오프
- winit 판을 올릴 때 패치를 다시 대야 한다.
- 일본어·중국어 IME에서 확정 직후 명령이 오는 경우가 있다면 그 키가 앱에 간다. 확인하지 않았다.
- 이 패치는 한글을 실제로 쳐서 확인해야 하는데, 밤사이 사람 없이 IME 입력을 만들 수 없어 확인하지 못했다(#6 PR에 적음).
