# 설계 결정 기록

입력 경로, 보안 경계, 장기 제약을 만드는 선택을 적습니다. 지난 판단은 지우지 않고 새 ADR에서 대체 관계를 적습니다.

## 목차

- [ADR-001 VT 엔진은 alacritty_terminal로 시작하고, bt-term 밖으로 엔진 타입을 내보내지 않는다](#adr-001-vt-엔진은-alacritty_terminal로-시작하고-bt-term-밖으로-엔진-타입을-내보내지-않는다)
- [ADR-002 렌더러는 CPU(softbuffer + fontdue)로 시작한다](#adr-002-렌더러는-cpusoftbuffer--fontdue로-시작한다)
- [ADR-003 크레이트를 여섯으로 나누고 창 라이브러리는 bt-app만 쓴다](#adr-003-크레이트를-여섯으로-나누고-창-라이브러리는-bt-app만-쓴다)
- [ADR-004 한글 IME를 위해 winit을 vendor로 고쳐 쓴다](#adr-004-한글-ime를-위해-winit을-vendor로-고쳐-쓴다)
- [ADR-005 b-terminal은 순수 터미널이고, AI 작업 화면은 b-studio가 맡는다](#adr-005-b-terminal은-순수-터미널이고-ai-작업-화면은-b-studio가-맡는다)
- [ADR-006 Ghostty는 본보기일 뿐 기대지 않는다: AI CLI가 가장 잘 도는 우리 터미널](#adr-006-ghostty는-본보기일-뿐-기대지-않는다-ai-cli가-가장-잘-도는-우리-터미널)
- [ADR-007 링크는 Cmd-클릭으로만 열고, 웹과 이 컴퓨터의 파일만 연다](#adr-007-링크는-cmd-클릭으로만-열고-웹과-이-컴퓨터의-파일만-연다)
- [ADR-008 글리프 래스터를 fontdue에서 swash로 바꾸고, 글꼴 파일은 필요한 부분만 읽는다](#adr-008-글리프-래스터를-fontdue에서-swash로-바꾸고-글꼴-파일은-필요한-부분만-읽는다)
- [ADR-009 macOS에서 그림을 화면 자신의 색 공간으로 넘긴다(softbuffer를 vendor로 고쳐 씀)](#adr-009-macos에서-그림을-화면-자신의-색-공간으로-넘긴다softbuffer를-vendor로-고쳐-씀)
- [ADR-010 kitty 그래픽은 직접 전송만 받고, 크기에 상한을 둔다](#adr-010-kitty-그래픽은-직접-전송만-받고-크기에-상한을-둔다)

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

## ADR-005 b-terminal은 순수 터미널이고, AI 작업 화면은 b-studio가 맡는다

상태: 채택(ADR-003의 `bt-studio` 크레이트를 대체)
관련: [#25](https://github.com/dj258255/b-terminal/issues/25), [#12](https://github.com/dj258255/b-terminal/issues/12)(닫음), b-studio [#1009](https://github.com/dj258255/b-studio/issues/1009)

### 맥락
- 처음 구상은 "AI 코딩 에이전트와 함께 일하는 터미널"이었다. 그래서 #7·#10·#11에서 에이전트 상태 사이드바(상태 점, 프로세스 이름으로 claude·codex 알아보기), 에이전트 훅(`btctl hook`·`setup claude`), 구독 사용량 막대(`btctl statusline`)를 넣었고, #12에서 b-studio 세션 목록과 터미널 다리를 만들고 있었다.
- b-studio 세션이 "b-studio는 관문, 터미널은 b-terminal이 맡는다"고 했을 때 이것을 "desk(b-studio의 터미널 작업대) 역할까지 b-terminal로"라고 넓게 읽었다.
- 2026-11-06 범수 님이 경계를 못 박았다: "b-terminal은 이제 그냥 터미널처럼 작동하는거임 ghostty나 그런것처럼. 이제 ai틱 스러운 herdr 같은건 b-studio고 이걸 구분을 해야해. 확실하게 해야함".

### 검토한 선택지
| 방식 | 판단 |
|---|---|
| **AI 기능을 걷어내고 b-terminal은 순수 터미널로** | 채택. 범수 님이 골랐다 |
| 남기되 기본으로 끄기 | 경계가 흐리다 |
| b-studio에 옮겨 달라고 이슈를 연다 | 범수 님이 고르지 않았다. b-studio가 필요하면 스스로 만든다 |

### 결정
1. b-terminal = Ghostty 같은 범용 터미널 에뮬레이터: PTY, VT 해석, 렌더링, 탭, 한글 IME, 복사·붙여넣기, 설정. b-studio가 꺼져 있거나 없어도 그대로 터미널이다.
2. b-studio = herdr 같은 AI 작업 화면: 에이전트 목록·상태·훅·구독 사용량·세션. b-terminal 안에서 도는 프로그램 중 하나다.
3. 기능을 넣을 때 "Ghostty에도 있을 기능인가"로 거른다. 남기는 것: OSC 9·99·777 데스크톱 알림과 안 읽음 점(Ghostty도 알림을 띄운다), 제어 소켓과 btctl의 탭 목록·새 탭·글 넣기·화면 읽기·알림(kitty·WezTerm의 원격 조종과 같은 성격), 전경 프로세스 이름을 탭 이름으로 쓰기.
4. 걷어내는 것: `bt-core::agent`, 제어 프로토콜의 `status`·`usage`, `btctl hook|setup|status|statusline`, 사이드바(→ 가로 탭 줄), `bt-studio` 크레이트, `docs/agents.md`.
5. b-studio가 b-terminal에 바라는 것은 표준 터미널 기능뿐이다(truecolor, 마우스 보고, 괄호 붙여넣기, 대체 화면, 크기 변경). 표준 밖이 필요하면 b-studio가 먼저 알린다(b-studio 세션과 2026-11-06에 맞춤).

### 감수한 트레이드오프
- 만든 기능(테스트 포함)을 버린다. b-studio 연결 코드는 로컬 브랜치 `feature/studio-connection`에만 남겼다.
- "탭마다 에이전트가 무엇을 기다리는지" 같은 정보는 b-terminal 혼자서는 보이지 않는다. b-studio를 b-terminal 안에서 띄워야 보인다.

## ADR-006 Ghostty는 본보기일 뿐 기대지 않는다: AI CLI가 가장 잘 도는 우리 터미널

상태: 채택(ADR-005를 보탠다)
관련: [#32](https://github.com/dj258255/b-terminal/issues/32), [#33](https://github.com/dj258255/b-terminal/issues/33), [조사 기록](research/2026-11-06-agentic-terminal.md)

### 맥락
- 0.1은 "Ghostty 설정 호환"을 앞세웠다. 테마는 `/Applications/Ghostty.app` 안의 파일을, 설정은 Ghostty 설정 파일을 먼저 읽었다. Ghostty를 깔지 않은 컴퓨터에서는 `theme = Dracula`가 동작하지 않았다.
- 범수 님(2026-11-06): "Ghostty 설정호환이라기보단 Ghostty같은 터미널 근데 Ghostty를 안써도 되는 우리 터미널", "AI 에이전틱한 요즘 시대에 제일 걸맞는 터미널, 나만의 터미널".

### 결정
1. b-terminal은 혼자 선다. Ghostty가 없어도 같은 설정이 같은 화면을 낸다. 테마 905개(iTerm2-Color-Schemes, MIT)를 실행 파일에 넣는다.
2. Ghostty 설정 읽기는 옮겨 오는 사람을 위한 선택으로 둔다(`import-ghostty = true`, 기본 끔). 설정 문법은 Ghostty와 같게 유지한다(배우기 쉽고 옮기기 쉽다).
3. "AI 에이전트 시대에 걸맞음"은 ADR-005의 경계 안에서 정한다: 에이전트 대시보드는 넣지 않고, AI CLI가 쓰는 표준 신호(kitty 키보드, 알림, 진행 표시, OSC 52, 셸 통합)를 빠짐없이 받고, 에이전트가 터미널을 도구로 쓰는 API(btctl)를 키운다.

### 감수한 트레이드오프
- 실행 파일이 테마만큼(약 0.4MB) 커진다.
- Ghostty 설정을 그대로 쓰던 사람은 `import-ghostty = true`를 한 줄 써야 한다.

## ADR-007 링크는 Cmd-클릭으로만 열고, 웹과 이 컴퓨터의 파일만 연다

상태: 채택(2026-11-06 #54로 3·4·2를 좁힘)
관련: [#38](https://github.com/dj258255/b-terminal/issues/38), [#54](https://github.com/dj258255/b-terminal/issues/54)

### 맥락
- 화면의 URL과 프로그램이 OSC 8로 단 링크를 열 수 있어야 한다(Ghostty·iTerm2의 기본 기능). AI CLI는 로그인 주소처럼 여러 줄로 접히는 긴 URL을 자주 찍는다.
- 링크는 터미널에 찍힌 글에서 온다. 원격 서버의 출력, 남이 만든 파일의 `cat`도 링크를 만들 수 있다. OSC 8은 보이는 글과 실제 주소가 다를 수 있다(`Ghostty 문서`라고 보이지만 다른 곳으로 가는 링크).
- macOS의 `open`은 받은 것을 기본 앱으로 연다. `.app`·`.command`·실행 권한이 있는 파일을 열면 그대로 실행된다.

### 결정
1. 여는 것은 사용자가 Cmd(Linux는 Ctrl)를 누른 채 누를 때만이다. 링크 위에 올리기만 해서는 열리지 않는다.
2. Cmd를 누른 채 올리면 밑줄과 함께 **실제 주소를 창 왼쪽 아래에 보인다**. OSC 8 링크도 열기 전에 어디로 가는지 볼 수 있다. 주소가 길면 끝이 아니라 가운데를 줄이고, **스킴과 호스트는 늘 다 보인다**.
3. 열 수 있는 것은 `http`·`https`와 `file`(호스트가 이 컴퓨터이고 실제로 있는 절대 경로)뿐이다. 제어 문자·공백·보이지 않는 서식 문자(방향 바꾸기 U+202E, 폭 없는 공백 등)가 든 주소, 8KiB를 넘는 주소는 열지 않는다. 웹 주소의 호스트는 ASCII 이름·주소·포트만 받는다: 사용자 정보(`https://github.com@evil.example/`)와 비ASCII 호스트(키릴 글자로 흉내 낸 이름)는 열지 않는다. 그 밖의 스킴(`javascript:`, `ssh:`, 앱 스킴 등)은 열지 않는다.
4. `file` 링크는 심볼릭 링크를 풀어 실제 대상을 보고, **허용 목록**으로 정한다: 묶음이 아닌 폴더, 실행 권한이 없고 확장자가 알려진 문서·그림·글(`txt`·`md`·`pdf`·`png`·`rs` 등)만 기본 앱으로 연다. 그 밖의 모든 것(모르는 확장자, 확장자 없음, 실행 권한, 앱 묶음처럼 확장자가 있거나 `Contents/Info.plist`가 있는 폴더)은 Finder에서 보이기만 한다(`open -R`). 기본 앱이 실행기일 수 있는 확장자(`.py`·`.sh`·`.js`·`.html`)는 목록에 넣지 않는다.
5. 여는 프로그램(`open`·`xdg-open`)에는 셸을 거치지 않고 인자로 넘긴다.

### 감수한 트레이드오프
- 실행 파일과 목록에 없는 문서는 링크로 바로 열 수 없다(Finder에서 한 번 더 열어야 한다).
- 한국어 도메인 같은 국제화 도메인 링크는 열지 않는다.
- 다른 컴퓨터의 `file` 링크(ssh 안에서 `ls --hyperlink`)는 열지 않는다.
- `mailto:` 같은 다른 스킴은 지금은 열 수 없다. 필요해지면 목록에 더한다.

## ADR-008 글리프 래스터를 fontdue에서 swash로 바꾸고, 글꼴 파일은 필요한 부분만 읽는다

상태: 채택(ADR-002의 래스터 선택을 바꾼다)
관련: [#59](https://github.com/dj258255/b-terminal/issues/59), [#65](https://github.com/dj258255/b-terminal/issues/65)

### 맥락
- 0.2.0의 처음 메모리가 약 380MiB였다(release, 셸 한 칸). fontdue는 글꼴을 읽을 때 모든 글리프 윤곽을 풀어 두므로, 수천 글리프인 한글 대체 글꼴이 크다.
- 색 이모지(#37)는 이미 swash로 그렸다. 래스터 엔진이 둘이었다.
- 시스템 글꼴 파일이 크다. Apple SD Gothic Neo는 여러 굵기를 담은 53MiB 모음이고, Apple Color Emoji는 180MiB 중 179MiB가 크기별 비트맵 9벌이다. 메모리 매핑은 작업 공간의 `unsafe_code = "deny"`에 걸린다.

### 결정
1. 흑백 글리프도 swash로 그린다. 글리프는 처음 그릴 때 풀고 캐시한다. fontdue를 뺀다.
2. 글꼴 파일은 통째로 읽지 않는다. `sfnt` 모듈이 파일에서 한 꼴의 표만, 색 이모지의 sbix는 셀 크기에 맞는 한 벌만 찾아 읽어 홀로 서는 글꼴 바이트를 만든다(unsafe 없음).
3. 읽은 글꼴은 이름·굵기·꼴로 기억해 다시 쓴다(설정 다시 읽기, 글자 크기 바꾸기).

### 결과(한 번 잰 값)
- 처음 메모리 378MiB → 158MiB, 이모지를 그린 뒤 558MiB → 134~170MiB, 설정을 다시 읽어도 늘지 않음.
- 그리기 시간은 비슷하다(실험 E2 조건 통째 중앙값 6.56ms, 한 줄 0.11ms).

### 감수한 트레이드오프
- 글꼴 파일 구조를 우리가 직접 읽는다(sfnt 표 목록, ttc, sbix). 읽지 못하면 그 글꼴을 건너뛴다.
- 새로 깐 글꼴은 앱을 다시 띄워야 보인다(전과 같음).

## ADR-009 macOS에서 그림을 화면 자신의 색 공간으로 넘긴다(softbuffer를 vendor로 고쳐 씀)

상태: 채택
관련: [#68](https://github.com/dj258255/b-terminal/issues/68), [실험 E3](experiments/2026-11-06-e3-present.md), [vendor/README.md](../vendor/README.md)

### 맥락
- 실험 E2에서 한 줄만 바뀐 프레임도 창에 올리기까지 약 10~15ms였다. 실험 E3에서 나눠 재니 거의 전부가 softbuffer의 `present`였고, 창이 작으면 줄었다(픽셀 수를 따르는 일).
- softbuffer(macOS)는 그림을 기기 RGB로 넘긴다. Core Animation이 프레임마다 화면 색 공간으로 색 맞춤 변환을 한다. sRGB·Display P3를 이름으로 줘도 변환은 남았고(13.7·22.4ms), 화면 자신의 색 공간 객체를 줄 때만 사라졌다(0.11ms).
- 테마 색은 sRGB로 적혀 있다. 화면 색 공간으로 그대로 넘기면 P3 화면에서 조금 더 진하게 보인다(바탕 #282c34가 화면에 (40,44,52) 대신 (39,44,53)). Alacritty·kitty 같은 OpenGL 터미널의 기본 동작과 같다. Ghostty는 `window-colorspace = srgb | display-p3`(기본 srgb)로 고르게 하고 GPU로 변환한다.

### 결정
1. softbuffer를 vendor로 고쳐(ADR-004의 winit과 같은 방식) macOS에서 화면 자신의 색 공간으로 넘길 수 있게 한다.
2. 설정 키는 Ghostty와 같은 `window-colorspace`. `display-p3`(기본)는 화면 색 공간으로 그대로 넘기고(빠름), `srgb`는 원래대로 색을 맞춘다(정확하지만 큰 창에서 프레임마다 약 10~15ms).
3. 기본을 빠른 쪽으로 둔다. 바뀐 줄만 그리기(ADR 없음, #40)로 그리기는 0.2ms인데 올리기가 15ms면 키 입력이 늦게 보인다. 범수 님이 정확한 색을 먼저 원하면 기본을 `srgb`로 바꾼다.

### 감수한 트레이드오프
- 기본 설정에서 색이 sRGB 정의보다 조금 진하게 보일 수 있다(P3 화면).
- ~~색 공간은 창을 만들 때의 주 화면 것이다.~~ 갱신(#97): 창이 있는 화면의 색 공간을 쓰고, 올릴 때 창의 화면(NSScreen)이 바뀌었으면 그 화면의 색 공간으로 바꾼다. 창이 옮겨지면 다시 그리지 않고 한 번 올린다. 실제로 다른 화면으로 옮겨 보지는 못했다(외부 모니터 없음). 설정을 바꾸면 새 창부터 따른다.
- vendor로 고친 크레이트가 둘이 된다.

## ADR-010 kitty 그래픽은 직접 전송만 받고, 크기에 상한을 둔다

상태: 채택
관련: [#76](https://github.com/dj258255/b-terminal/issues/76)

### 맥락
- kitty 그래픽 프로토콜(APC `ESC _ G … ESC \`)로 터미널 안에 그림을 보인다. Ghostty·WezTerm·Konsole이 지원하고 kitty `icat`, term-image, yazi 미리보기 등이 쓴다.
- 그림은 터미널에 찍힌 글에서 온다(원격 서버, 남이 만든 파일의 `cat` 포함). 프로토콜에는 터미널이 파일·임시 파일·공유 메모리를 읽어 그림을 가져오는 전송 방식(t=f·t·s)이 있다.

### 결정
1. 직접 전송(t=d, base64)만 받는다. 파일·임시 파일·공유 메모리 전송은 `EINVAL`로 거절한다. 터미널이 찍힌 글의 지시로 임의 파일을 열지 않는다.
2. 상한: APC 하나 8MiB(넘으면 버림), 한 명령의 base64 48MiB, 그림 한 장을 푼 RGBA 64MiB, 한 변 10000px, 저장 총량 256MiB와 그림 수 4096개(그림마다 4KiB의 고정 몫을 더해 셈, 넘으면 오래된 그림부터 버림), 놓인 그림 1024개. 화면에 그리는 그림은 같은 자리의 중복을 빼고 최근 것부터 보이는 넓이의 합이 화면 칸 수의 4배까지만(그림을 겹쳐 놓아 그리기를 느리게 하지 못하게). PNG는 크기 상한을 걸고 풀고, zlib(o=z)도 상한까지만 푼다.
3. 답(`ESC _ G i=…;OK ESC \`)에는 그림 번호와 ASCII 오류 코드만 넣는다. 찍힌 글의 문자열을 되돌려 보내지 않는다.
4. 놓인 그림은 스크롤백 맨 위를 0으로 센 줄에 붙는다. 화면 지우기(ED 2)는 VT 엔진이 내용을 스크롤백으로 올리므로 그림도 남기고, 스크롤백 지우기(ED 3)는 주 화면의 그림을, 초기화(RIS)는 모두 지운다. 대체 화면의 그림은 대체 화면을 나오면 지운다.

### 감수한 트레이드오프
- 파일 전송을 쓰는 도구(같은 컴퓨터에서 빠르게 보내려는 경우)는 직접 전송으로 다시 시도해야 그림이 보인다.
- 스크롤백이 상한에 닿아 오래된 줄이 버려지면 그림 자리가 어긋날 수 있다(셸 통합의 프롬프트 줄과 같은 한계).
- 애니메이션, 유니코드 자리표시, 자리 번호(p)·z 순서로 지우기는 하지 않는다.

## ADR-011 줄바꿈이 든 붙여넣기는 괄호 붙여넣기가 아니면 먼저 묻는다

상태: 채택
관련: [#109](https://github.com/dj258255/b-terminal/issues/109)

### 맥락
- 괄호 붙여넣기(`ESC[?2004h`)를 켜지 않은 프로그램(괄호 붙여넣기를 끈 셸, `cat`, 오래된 REPL)에 붙여넣으면, 글 안의 줄바꿈은 그대로 Enter가 된다. 웹 페이지는 보이지 않는 줄바꿈을 복사 글에 넣을 수 있어, 사람이 본 적 없는 명령이 붙여넣는 순간 돈다.
- 괄호 붙여넣기 모드에서는 프로그램이 글을 붙여넣기로 받고 바로 실행하지 않는다. 구간을 빠져나오는 끝 표시(`ESC[201~`)는 `encode_paste`가 이미 지운다(0.1부터).
- Ghostty는 `clipboard-paste-protection = true`, `clipboard-paste-bracketed-safe = true`가 기본이다.

### 결정
1. 사람이 붙여넣는 길(⌘V, 메뉴·팔레트의 붙여넣기)에서, 괄호 붙여넣기가 꺼져 있고 글에 LF나 CR이 있으면 창 안 확인 막대(닫기 확인과 같은 것, #108)로 묻는다. ↩·`y`면 붙여넣고 esc·`n`이면 버린다.
2. CR 혼자도 위험으로 본다. 괄호 붙여넣기가 꺼져 있으면 LF를 CR로 바꿔 보내므로 둘은 같은 Enter다(Ghostty는 LF만 본다).
3. 막대에는 글이 든 줄 수와 첫 줄(제어 문자를 빈칸으로 바꾸고 30칸까지)을 보인다. 붙여넣을 바이트는 묻기로 한 때가 아니라 붙여넣는 때의 그 칸 모드로 만든다.
4. 설정: `clipboard-paste-protection = false`면 묻지 않는다. `clipboard-paste-bracketed-safe = false`면 괄호 붙여넣기여도 줄바꿈이 있으면 묻는다.

### 감수한 트레이드오프
- 괄호 붙여넣기를 쓰지 않는 프로그램에 여러 줄을 붙여넣을 때마다 한 번 더 누른다.
- `btctl send`와 키 묶음의 `text:` 동작은 묻지 않는다. 사람이 클립보드에서 가져오는 길이 아니고, 부르는 쪽이 글을 정한다.
- 막대는 글 전체를 보이지 않는다. 첫 줄 뒤에 숨은 명령은 줄 수로만 알린다.
