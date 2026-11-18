# b-terminal 한눈에

처음 보는 분을 위한 안내입니다. 무엇인지, 무엇이 Ghostty와 같고 다른지, 어떻게 짜여 있는지, 어디까지 믿어도 되는지를 한 문서에 모았습니다. 자세한 설정은 [configuration.md](configuration.md), 이유와 근거는 [decisions.md](decisions.md)에 있습니다.

이 문서는 0.74.0 기준입니다. 이 문서에 없는 숫자는 재지 않은 것입니다.

## 무엇인가

b-terminal은 Ghostty 같은 순수 Rust 터미널 에뮬레이터입니다. PTY, VT 해석, 그리기, 탭과 분할, 한글 IME, 복사·붙여넣기, 설정을 맡습니다. macOS(14 이상)가 1순위이고, Linux는 CI에서 빌드와 시험만 합니다.

AI 에이전트를 다루는 화면(에이전트 목록·상태·훅·구독 사용량)은 이 저장소의 일이 아닙니다. 그것은 [b-studio](https://github.com/dj258255/b-studio)가 맡고, b-terminal 안에서 도는 프로그램 중 하나입니다([ADR-005](decisions.md#adr-005-b-terminal은-순수-터미널이고-ai-작업-화면은-b-studio가-맡는다)). 기능을 넣기 전에 "Ghostty에도 있을 기능인가"로 가립니다.

다만 Claude Code 같은 AI CLI가 쓰는 표준 신호는 빠짐없이 받습니다. Shift+Enter(kitty 키보드 프로토콜), 데스크톱 알림(OSC 9·99·777), 진행 표시(OSC 9;4), 클립보드 쓰기(OSC 52), 셸 통합입니다. 이것은 터미널이 할 일입니다([ADR-006](decisions.md#adr-006-ghostty는-본보기일-뿐-기대지-않는다-ai-cli가-가장-잘-도는-우리-터미널), [조사 기록](research/2026-11-06-agentic-terminal.md)).

## 누구를 위한가

- Ghostty를 쓰던 사람. 설정 문법과 키 묶음, 키 이름이 같아서 그대로 옮겨 올 수 있습니다.
- 한글을 매일 쓰는 사람. 한글 조합 입력이 깨지는 문제를 고쳐서 씁니다.
- 스크립트나 에이전트가 터미널을 조종해야 하는 사람. `btctl`이 탭 열기, 글 넣기, 화면 읽기를 합니다.
- 터미널 코드를 읽고 고치고 싶은 사람. 크레이트가 작게 나뉘어 있고, 창 없이 시험할 수 있는 부분이 많습니다.

아직 만드는 중입니다. 다른 Mac에 나눠 줄 서명·공증판은 없습니다([아래](#지금-상태와-한계)).

## Ghostty와 같은 점, 다른 점

### 같은 점

- **설정 문법과 키 이름.** `~/.config/b-terminal/config`에 `키 = 값`으로 씁니다. 키 묶음(`keybind`)의 문법, 키 순서(`ctrl+a>n`), 키 테이블, 전역 단축키(`global:`)도 Ghostty와 같습니다. Ghostty 설정을 옮겨 오려면 `import-ghostty = true`를 씁니다([configuration.md](configuration.md)).
- **기본 키.** 새 탭 ⌘T, 분할 ⌘D·⌘⇧D, 찾기 ⌘F, 명령 팔레트 ⌘⇧P처럼 Ghostty macOS 기본과 같습니다.
- **터미널 출력이 할 수 있는 일을 넓히는 설정은 기본으로 끕니다.** ENQ 답, 키보드 잠금, 출력이 창 크기 바꾸기, 체크섬 보고가 그렇습니다([ADR-013](decisions.md#adr-013-터미널-출력이-할-수-있는-일을-넓히는-vt-설정은-ghostty처럼-기본을-끈다)).
- **클립보드.** OSC 52 읽기는 기본으로 묻고([ADR-046](decisions.md#adr-046-clipboard-read-기본을-ghostty처럼-ask로-한다)), 줄바꿈이 든 붙여넣기는 괄호 붙여넣기가 아니면 먼저 묻습니다([ADR-011](decisions.md#adr-011-줄바꿈이-든-붙여넣기는-괄호-붙여넣기가-아니면-먼저-묻는다)). 붙여넣는 글의 위험 제어 바이트는 공백으로 바꿉니다([ADR-054](decisions.md#adr-054-붙여넣는-글의-위험-제어-바이트는-모드와-상관없이-공백으로-바꾸고-창에-끌어다-놓은-파일-경로는-셸-이스케이프해-같은-붙여넣기-길로-넣는다)). 고르면 복사하는 `copy-on-select`의 기본은 `primary`입니다. 고른 글은 앱 안 선택 클립보드에 두고 ⌘⇧V로 붙여넣으며, ⌘V의 시스템 클립보드는 덮지 않습니다([ADR-048](decisions.md#adr-048-고르기-복사는-기본으로-앱-안-선택-클립보드에-두고-시스템-클립보드는-덮지-않는다)).
- **키 바이트.** kitty 키보드를 켜지 않은 프로그램에는 Ghostty 앱과 같은 바이트를 보냅니다(Ctrl+[·Ctrl+i는 `CSI u`). Shift+Enter·Ctrl+Enter 같은 수식 키가 붙은 이름 키도 Ghostty 표 그대로입니다(`ESC[27;2;13~` 등). 기본 키 묶음은 Ghostty macOS 기본 목록과 같고, Option+←·→(단어 이동)와 Cmd+←·→·Backspace(줄 처음·끝, 줄 지우기)도 그 안에 있습니다([ADR-055](decisions.md#adr-055-레거시-키-인코딩은-ghostty-앱이-보내는-바이트에-맞추고-option은-글자-길에서만-설정을-따르며-macos-기본-키-묶음에-글-편집-다섯-개를-더한다)).
- **분할 칸 누르기.** 고르지 않은 칸을 누르면 그 칸만 고르고, 그 누름은 프로그램에 보내지 않습니다(0.57, [ROADMAP](../ROADMAP.md)).
- **프로토콜.** kitty 키보드·그래픽(직접 전송만)·색(OSC 21)·클립보드(OSC 5522), OSC 8 링크, OSC 133 셸 통합, 사용자 셰이더(`custom-shader`)를 받습니다.

### 다른 점

- **Ghostty 없이 섭니다.** 테마 907개와 글꼴(JetBrains Mono, Nerd Font 기호)을 앱에 넣어 두어 Ghostty가 없어도 같은 설정이 같은 화면을 냅니다([ADR-006](decisions.md#adr-006-ghostty는-본보기일-뿐-기대지-않는다-ai-cli가-가장-잘-도는-우리-터미널)).
- **VT 엔진이 다릅니다.** Ghostty의 엔진 대신 `alacritty_terminal`을 씁니다. 그래서 해석이 다른 곳이 있을 수 있습니다([ADR-001](decisions.md#adr-001-vt-엔진은-alacritty_terminal로-시작하고-bt-term-밖으로-엔진-타입을-내보내지-않는다)). Ghostty `Terminal.zig` 시험을 옮겨 견주고 다른 곳은 고친 사본을 씁니다. 우리 동작이 xterm과 같은 곳은 xterm을 따릅니다([ADR-056](decisions.md#adr-056-한-칸에-덧붙는-글자는-64개까지만-두고-엔진이-ghostty와-다른-곳은-xterm과-같은-쪽이-아니면-ghostty를-따른다)).
- **`TERM` 기본이 `xterm-256color`입니다.** Ghostty의 `xterm-ghostty` terminfo는 많은 시스템에 없어서 `vim`·`less`가 깨질 수 있기 때문입니다([ADR-016](decisions.md#adr-016-설정이-칸의-환경과-시작-입력을-정한다-term은-xterm-256color를-지키고-input에는-상한을-둔다)).
- **원격 조종 `btctl`이 있습니다.** kitty·WezTerm의 원격 조종과 같은 성격입니다. 탭 목록·새 탭·글 넣기·화면 읽기·알림, 셸 통합이 켜진 탭에서 명령 돌리고 결과 받기(`btctl run`)를 합니다. 제어 소켓은 같은 사용자만 닿습니다.
- **한글 IME를 고쳤습니다.** macOS 한글 입력기는 조합 중 스페이스·Enter에서 확정과 그 키를 한 번에 보내는데, 원본 winit이 뒤의 것을 버려서 스페이스가 사라졌습니다. winit을 고쳐 쓰고([ADR-004](decisions.md#adr-004-한글-ime를-위해-winit을-vendor로-고쳐-쓴다)), 조합 중 창·칸이 포커스를 잃으면 조합 글자를 옛 칸에 확정합니다([ADR-049](decisions.md#adr-049-한국어-입력기로-조합-중-창칸이-포커스를-잃으면-조합-글자를-옛-칸에-확정한다)). 마지막 것은 Ghostty에서 아직 병합되지 않은 고침을 따른 것이고, 실제 창에서 한글을 쳐 확인하지는 못했습니다([troubleshooting.md](troubleshooting.md) 12번).
- **그리기 길이 둘입니다.** macOS는 wgpu(GPU)가 기본이고, `BT_GPU=0`이면 CPU 길로 그립니다. Linux는 CPU 길이 기본입니다([ADR-039](decisions.md#adr-039-macos는-gpu-길을-기본으로-그리고-bt_gpu0이면-cpu-길로-그-밖의-os는-cpu-길을-기본으로-둔다)). 같은 날 한 기기(Apple M2 Pro)에서 잰 프레임 시간 중앙값이 바뀐 줄 1줄일 때 GPU 길 0.44~0.55ms, CPU 길 1.72~1.91ms였습니다([실험 E9](experiments/2026-11-11-e9-frame-pacing.md)). 다른 기기에서는 재지 않았습니다.
- **`window-colorspace` 기본이 `display-p3`입니다.** Ghostty 기본은 `srgb`입니다. 빠른 쪽을 기본으로 두었고, `srgb`로 바꾸면 테마 색이 Ghostty와 같아집니다([ADR-009](decisions.md#adr-009-macos에서-그림을-화면-자신의-색-공간으로-넘긴다softbuffer를-vendor로-고쳐-씀)).
- **없는 것.** 자동 업데이트, 터미널 검사기, Sixel은 없습니다. Ghostty 설정에 있어도 받기만 하고 하지 않는 키는 [configuration.md](configuration.md)에 따로 적었습니다.

## 구조

### 크레이트

| 크레이트 | 맡는 것 | 창을 아는가 |
|---|---|---|
| `bt-core` | 설정, 제어 프로토콜, 테마, 클립보드 정책 | 아니오 |
| `bt-term` | PTY, VT 해석, 알림·질의 스캐너, 키 → 바이트, 찾기 | 아니오 |
| `bt-render` | 글꼴, 글리프 캐시, 셀 격자를 CPU 버퍼에 그리기 | 아니오 |
| `bt-gpu` | wgpu로 셀 격자 그리기, 사용자 셰이더 | 아니오 |
| `bt-ctl` | `btctl` 실행 파일(제어 소켓 손님) | 아니오 |
| `bt-app` | `b-terminal` 실행 파일: 창, 입력, 탭·분할, 메뉴, 제어 소켓 서버 | 예(winit) |

창 라이브러리(winit)는 `bt-app`만 씁니다. 그래서 나머지는 창 없이 시험할 수 있습니다([ADR-003](decisions.md#adr-003-크레이트를-여섯으로-나누고-창-라이브러리는-bt-app만-쓴다)).

### 바이트가 화면이 되기까지

```
PTY 바이트 ──> bt-term 읽기 스레드 ──┬─> VT 엔진(alacritty_terminal) ──> 화면 모델(Screen·Cell)
                                      └─> 스캐너(알림·질의·OSC 133 …)           │
키·붙여넣기·IME 확정 ──> bt-app ──> 바이트 ──> PTY                              ▼
                                                      bt-render(CPU) 또는 bt-gpu(GPU) ──> 창
```

- 읽기 스레드가 PTY 바이트를 VT 엔진과 스캐너에 같은 순서로 먹입니다.
- 엔진 타입은 `bt-term` 밖으로 나가지 않습니다. 밖에서는 자체 화면 모델만 봅니다([ADR-001](decisions.md#adr-001-vt-엔진은-alacritty_terminal로-시작하고-bt-term-밖으로-엔진-타입을-내보내지-않는다)).
- 넓은 글자(한글·CJK·이모지)의 폭은 엔진이 정하고, 렌더러가 따로 세지 않습니다. 두 칸으로 셉니다.
- 입력은 모두 PTY에 쓰는 바이트입니다. IME는 확정된 글자만 보내고, 조합 중인 글자는 커서 자리에 그리기만 합니다.
- macOS 기본 그리기 길에서는 CPU가 창 바탕·탭 줄·덧그림을 두 층으로 그리고, GPU가 그 사이에 칸의 격자를 그려 합칩니다([ADR-036](decisions.md#adr-036-wgpu-렌더러는-bt_gpu1일-때만-앱-창에-연결하고-cpu가-그린-아래위-두-층-사이에-gpu가-격자를-그려-합친다)).

### 벤더로 고쳐 쓰는 크레이트

`vendor/`에 외부 크레이트를 넣어 고쳐 씁니다. 고친 곳마다 이유와 되돌릴 조건을 [vendor/README.md](../vendor/README.md)에 적었습니다. 원본이 고쳐 주면 사본을 지웁니다.

| 크레이트 | 고친 까닭 |
|---|---|
| `winit` | 한글 조합 확정, 퀵 터미널 창 위치, 배경 흐림, 보안 키보드 입력, 오른쪽 클릭 메뉴, Dock에 놓은 파일 |
| `softbuffer` | macOS 색 공간 지정과 IOSurface로 올리기(CPU 길) |
| `alacritty_terminal` | 글자 묶음 폭(모드 2027), 엔진이 멋대로 하던 답 끄기, OSC·DEC 모드 더하기 |
| `vte` | 윗줄(SGR 53·55) 등 해석 더하기 |

## 보안 모델 요약

터미널 출력은 믿지 않습니다. 원격 서버의 출력이나 남이 만든 파일을 `cat`한 글도 제어 열을 담을 수 있기 때문입니다. 전체 목록은 [SECURITY.md](../SECURITY.md)에 있고, 여기에는 줄기만 적습니다.

- **출력이 쓰는 메모리에 상한이 있습니다.** 한 칸에 덧붙는 결합 문자는 64개까지, 그림은 저장 총량까지입니다([ADR-056](decisions.md#adr-056-한-칸에-덧붙는-글자는-64개까지만-두고-엔진이-ghostty와-다른-곳은-xterm과-같은-쪽이-아니면-ghostty를-따른다)).
- **출력이 입력이 되는 길을 막습니다.** 터미널이 질의에 하는 답은 PTY로 돌아가 프로그램의 입력이 됩니다. 그래서 답은 고정 바이트와 검사한 숫자로만 만들고, 요청의 글은 답에 넣지 않습니다. 창 제목 보고는 기본으로 끕니다.
- **링크는 Cmd-클릭으로만 엽니다.** 열기 전에 실제 주소를 보입니다. `http`·`https`와 이 컴퓨터의 `file`만 열고, 실행 파일과 앱 묶음은 Finder에서 보이기만 합니다([ADR-007](decisions.md#adr-007-링크는-cmd-클릭으로만-열고-웹과-이-컴퓨터의-파일만-연다)).
- **클립보드는 사람의 동작이 있을 때만 읽힙니다.** 쓰기는 보고 있는 칸에서 온 것만 받고, 읽기는 창 안 막대로 묻습니다. 막대는 Enter로 허용되지 않고 `y`만 허용입니다([ADR-019](decisions.md#adr-019-클립보드-ask는-창-안-막대로-묻고-enter로는-허용하지-않으며-막대는-하나만-띄운다)).
- **붙여넣기.** 글 안의 ESC·Ctrl-C·Ctrl-U 같은 위험 제어 바이트 16종은 모드와 상관없이 공백으로 바꿔, 괄호 붙여넣기의 끝 표시가 깨집니다. 괄호 붙여넣기가 아닌 프로그램에는 줄바꿈이 든 글을 먼저 묻습니다. 창에 끌어다 놓은 파일 경로도 셸 이스케이프한 뒤 이 길로 들어갑니다.
- **제어 소켓.** 같은 사용자만 쓸 수 있는 폴더(0700)에 만듭니다. 요청 한 줄은 크기 상한(1MiB)을 먼저 봅니다.
- **그림과 파일.** kitty 그래픽은 직접 전송만 받습니다. 터미널이 찍힌 글의 지시로 파일을 읽지 않습니다. Dock에 놓은 파일은 확인 막대에서 `y`를 눌러야 실행합니다.
- **읽기 전용 칸.** 사람의 입력 길은 PTY 쓰기 한 곳에서 막습니다([ADR-018](decisions.md#adr-018-읽기-전용-칸은-pty-쓰기-한-곳에서-막고-btctl-send도-막으며-프로그램이-물은-것에-대한-터미널의-답은-막지-않는다)).
- **`unsafe` 금지.** 작업 공간에서 `unsafe_code = "deny"`입니다.

취약점은 공개 이슈가 아니라 [비공개 취약점 제보](https://github.com/dj258255/b-terminal/security/advisories/new)로 보냅니다.

## 핵심 결정

처음 읽는다면 이 열두 개부터 보면 됩니다. 나머지 ADR은 [decisions.md](decisions.md)에 있고, 대체되거나 일부 바뀐 ADR은 상태 줄에 적혀 있습니다.

| ADR | 한 줄 |
|---|---|
| [001](decisions.md#adr-001-vt-엔진은-alacritty_terminal로-시작하고-bt-term-밖으로-엔진-타입을-내보내지-않는다) | VT 엔진은 `alacritty_terminal`이고, 엔진 타입은 `bt-term` 밖으로 내보내지 않습니다 |
| [004](decisions.md#adr-004-한글-ime를-위해-winit을-vendor로-고쳐-쓴다) | 한글 IME를 위해 winit을 고쳐 씁니다 |
| [005](decisions.md#adr-005-b-terminal은-순수-터미널이고-ai-작업-화면은-b-studio가-맡는다) | b-terminal은 순수 터미널이고 AI 작업 화면은 b-studio가 맡습니다 |
| [006](decisions.md#adr-006-ghostty는-본보기일-뿐-기대지-않는다-ai-cli가-가장-잘-도는-우리-터미널) | Ghostty는 본보기일 뿐이고, 혼자 서며 AI CLI 신호를 빠짐없이 받습니다 |
| [007](decisions.md#adr-007-링크는-cmd-클릭으로만-열고-웹과-이-컴퓨터의-파일만-연다) | 링크는 Cmd-클릭으로만, 웹과 이 컴퓨터의 파일만 엽니다 |
| [011](decisions.md#adr-011-줄바꿈이-든-붙여넣기는-괄호-붙여넣기가-아니면-먼저-묻는다) | 줄바꿈이 든 붙여넣기는 괄호 붙여넣기가 아니면 먼저 묻습니다 |
| [013](decisions.md#adr-013-터미널-출력이-할-수-있는-일을-넓히는-vt-설정은-ghostty처럼-기본을-끈다) | 출력이 할 수 있는 일을 넓히는 설정은 Ghostty처럼 기본을 끕니다 |
| [016](decisions.md#adr-016-설정이-칸의-환경과-시작-입력을-정한다-term은-xterm-256color를-지키고-input에는-상한을-둔다) | `TERM`은 `xterm-256color`를 지킵니다(Ghostty와 일부러 다른 기본) |
| [019](decisions.md#adr-019-클립보드-ask는-창-안-막대로-묻고-enter로는-허용하지-않으며-막대는-하나만-띄운다) | 클립보드 `ask`는 창 안 막대로 묻고 Enter로는 허용하지 않습니다(읽기 기본은 ADR-046에서 `ask`) |
| [039](decisions.md#adr-039-macos는-gpu-길을-기본으로-그리고-bt_gpu0이면-cpu-길로-그-밖의-os는-cpu-길을-기본으로-둔다) | macOS는 GPU 길이 기본이고, 그 밖의 OS는 CPU 길입니다 |
| [048](decisions.md#adr-048-고르기-복사는-기본으로-앱-안-선택-클립보드에-두고-시스템-클립보드는-덮지-않는다) | 고르기 복사는 앱 안 선택 클립보드에 두고 시스템 클립보드를 덮지 않습니다 |
| [049](decisions.md#adr-049-한국어-입력기로-조합-중-창칸이-포커스를-잃으면-조합-글자를-옛-칸에-확정한다) | 한국어 입력기로 조합 중 포커스를 잃으면 조합 글자를 옛 칸에 확정합니다 |

기본값을 정하는 원칙은 이렇습니다. Ghostty와 같게 두는 것이 기본입니다(013, 046, 048). 같게 두면 쓸 수 없거나 위험한 곳에서만 다르게 두고 이유를 적습니다(016, 009).

## 지금 상태와 한계

- **판.** 0.74.0입니다. 1.0 전이라 기본 동작이 판마다 바뀔 수 있습니다. 바뀐 것은 [CHANGELOG.md](../CHANGELOG.md), 계획과 진행은 [ROADMAP.md](../ROADMAP.md)에 있습니다.
- **macOS가 1순위입니다.** 실제 창에서 확인한 것은 macOS(Apple M2 Pro 한 기기)가 거의 전부입니다.
- **Linux는 CPU 길입니다.** Linux GPU 길은 재지도 실제 창으로 보지도 않았습니다. CI는 Linux에서 전체를 빌드하고 시험하지만 GPU는 없습니다.
- **다른 Mac에 나눠 줄 수 없습니다.** Developer ID 서명과 공증 없이는 배포하지 않습니다. 계정이 없어서 서명·공증·다른 Mac에서 열기는 해 보지 못했습니다([release-macos.md](release-macos.md)). 지금은 직접 빌드해 이 컴퓨터에서 씁니다.
- **`unsafe`를 쓰지 않아서 못 하는 것.** ROADMAP의 "다음"에 사람이 정할 것으로 남아 있습니다. 출력 중에도 120Hz로 그리기(`CADisplayLink`), 레이어 색 공간을 sRGB로 직접 달기, Ghostty처럼 앱 사이에서 나누는 선택 클립보드입니다. 풀지 않으면 화면이 주는 주기를 따르고, 선택 클립보드는 앱 안에서만 씁니다.
- **엔진 차이.** Ghostty와 해석이 다른 곳이 있을 수 있습니다. 다르게 보이는 화면을 찾으면 [troubleshooting.md](troubleshooting.md)에 적습니다.
- **재지 않은 것이 많습니다.** 문서마다 "재지 않았다"고 적었습니다. 성능 숫자는 모두 [실험 보고서](experiments/)에 있는 한 번 잰 값입니다.

## 더 읽을 문서

| 문서 | 언제 |
|---|---|
| [README.md](../README.md) | 설치하고 띄우기, 기본 키 |
| [configuration.md](configuration.md) | 설정 키와 기본값, 키 묶음, 셸 통합 |
| [troubleshooting.md](troubleshooting.md) | 겪은 증상과 해결 |
| [SECURITY.md](../SECURITY.md) | 보안 경계 전체, 취약점 제보 |
| [decisions.md](decisions.md) | 설계 결정 기록(ADR) 전체. "먼저 볼 ADR"로 시작합니다 |
| [vendor/README.md](../vendor/README.md) | 고쳐 쓰는 외부 크레이트 |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | 기여하는 방법 |
