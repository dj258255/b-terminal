# AI 작업용 터미널 조사 (2026-11-05)

b-terminal을 시작하기 전에 비슷한 도구가 무엇을 하고 무엇으로 만들었는지 조사했다. 웹 검색과 각 저장소 README·공식 문서를 읽은 결과이고, 직접 설치해 써 본 것은 없다. 출처가 블로그·리뷰인 항목은 그렇다고 적었다.

## 1. 지형

| 도구 | 형태 | 스택 | 핵심 | b-terminal이 가져갈 것 |
|---|---|---|---|---|
| [cmux](https://github.com/manaflow-ai/cmux) | macOS 터미널 앱 | Swift/AppKit + libghostty | OSC 9/99/777과 `cmux notify`로 알림, 기다리는 칸에 파란 테두리, 세로 탭에 브랜치·PR·포트·마지막 알림, Unix 소켓 API, 내장 브라우저, iOS 앱(베타), Ghostty 설정 파일을 그대로 읽음. GPL-3.0 | 알림 경로 세 가지(OSC·CLI·훅), 세로 탭에 무엇을 보일지, Ghostty 설정 읽기 |
| [herdr](https://github.com/herdrdev/herdr) | 터미널 안에서 도는 멀티플렉서 | Rust, `crates/ghostty-vt` | 칸마다 working·blocked·idle, 서버/클라이언트로 SSH가 끊겨도 유지, 소켓 API로 에이전트끼리 칸을 띄우고 기다림, 재시작 뒤 지원 에이전트 대화 이어 붙이기. Apache-2.0 | 상태 세 가지의 단순함, 다시 띄운 뒤 `--resume` |
| [Copad / Comux](https://github.com/marshallku/copad) | 터미널 앱 + 멀티플렉서 | Rust 코어, Linux GTK4+VTE4, macOS Swift+alacritty_terminal+Metal | 이벤트 버스, 플러그인 14개, `coctl` 제어 CLI, 상태 막대에 Claude 5시간·주간과 Codex 주간 사용량 | 코어/제어 CLI/데몬 분리, SwiftTerm을 버린 이유(한글 IME, 배경 위 커서, SGR 7) |
| [Orca](https://www.onorca.dev/) | 데스크톱 ADE | 웹 기반 | 에이전트마다 git worktree, 휴대폰 앱에서 대기 중인 에이전트에 답하기 | 휴대폰 알림·답하기는 나중의 선택 |
| [Warp](https://warp.dev) | 터미널 + 자체 에이전트 | Rust, GPU | 블록(명령+출력+종료 코드), 2026-04 클라이언트 공개(리뷰 기준) | 블록 개념은 셸 통합(OSC 133)으로 나중에 |
| [Kaku](https://github.com/tw93/Kaku) | macOS 터미널 | WezTerm 파생 | `kaku ai`로 외부 코딩 CLI 설정, 사용량 보기, Claude Code 스크롤백 튐 수정 | Claude Code가 화면을 다시 그릴 때 스크롤 위치를 지키기 |
| typeboard (GeekNews Show GN, github.com/GloryKim) | macOS 터미널 | Tauri + xterm.js + portable-pty | 탭, 스크롤백 찾기, 학습용으로 읽기 쉬운 코드 | 작은 스택으로 시작하는 순서 |
| Claude Squad·Conductor·Vibe Kanban | worktree 관리자 | tmux 기반, macOS 앱, 웹 보드 | 에이전트마다 worktree와 diff 검토 | 터미널이 아니라 b-studio가 할 일 |

출처가 리뷰·블로그인 것: Warp 공개 시점, Kaku 기능 목록, Claude Squad·Conductor·Vibe Kanban 비교([crystl.dev](https://crystl.dev/blog/best-ai-agent-orchestration-tools/), [nimbalyst.com](https://nimbalyst.com/blog/best-agent-management-tools-2026/)).

## 2. 이제 기본이 된 기능

조사한 도구 대부분에 있는 것. b-terminal도 이것부터 한다.

1. **에이전트 상태**: 작업 중 / 기다림(권한·질문) / 끝남 / 쉼. herdr는 프로세스 이름과 화면 출력으로 추정하고, cmux·Comux는 훅과 알림으로 받는다.
2. **알림**: OSC 9(iTerm2·WezTerm), OSC 99(kitty), OSC 777(Ghostty·urxvt·Warp), BEL. 보고 있지 않은 탭만 알린다.
3. **세로 사이드바**: 탭 이름, 상태 점, 폴더·브랜치, 마지막 알림 한 줄.
4. **제어 소켓 + CLI**: 같은 사용자만 닿는 Unix 소켓, 줄 단위 JSON. 탭 만들기·글 넣기·화면 읽기·알림.
5. **사용량**: Claude 5시간·주간, Codex 주간.
6. **세션 이어 붙이기**: `claude --resume <id>`, `codex resume <id>`.

## 3. 에이전트 신호를 얻는 공식 경로

### Claude Code 훅 ([공식 문서](https://code.claude.com/docs/en/hooks))

- 쓸 이벤트: `UserPromptSubmit`(작업 시작), `PreToolUse`·`PostToolUse`(지금 도구), `PermissionRequest`(권한 기다림), `Notification`(matcher: `permission_prompt`·`idle_prompt`·`agent_needs_input`·`agent_completed` 등), `Stop`(턴 끝), `StopFailure`(API 오류로 끝), `SessionStart`·`SessionEnd`, `SubagentStart`·`SubagentStop`.
- 모든 훅이 표준 입력 JSON으로 `session_id`·`transcript_path`·`cwd`·`hook_event_name`을 받는다. `Stop`은 `last_assistant_message`를 준다.
- 훅에는 제어 터미널이 없어 `/dev/tty`에 쓰지 못한다. 대신 JSON 출력의 `terminalSequence`로 OSC 0/1/2/9/99/777·BEL을 Claude Code가 대신 찍게 할 수 있다(대화형 세션에서만).
- `Notification` 훅은 막을 수 없다(종료 코드·stderr 무시).

### Claude Code 상태줄 ([공식 문서](https://code.claude.com/docs/en/statusline))

- 상태줄 명령이 받는 JSON의 `rate_limits.five_hour`·`seven_day`에 사용 %와 리셋 시각이 있다. Pro·Max 구독에서, 첫 응답 뒤부터 들어온다. API 키로 쓰면 없다.
- 쉬는 계정의 값은 `/usage`가 쓰는 문서 없는 엔드포인트로만 읽을 수 있다. b-studio ADR-187이 10분 간격·429 물러나기로 이미 읽고 있으므로, b-terminal은 b-studio가 떠 있으면 `/api/usage/limits`를 쓴다.

### Codex

- `~/.codex/config.toml`의 최상위 `notify = ["프로그램", ...]`가 턴이 끝날 때(`agent-turn-complete`) JSON 인자 하나로 프로그램을 부른다. 승인 요청(`approval-requested`)은 `notify`로 오지 않는다(openai/codex#19921, 2026-08 기준 열림. [블로그](https://codex.danielvaughan.com/2026/04/13/codex-cli-notification-pipeline-osc9-hooks-alerts/) 정리).
- `[tui] notifications`는 TUI가 데스크톱 알림을 직접 띄우는 설정이다. 터미널에는 OSC 9로 온다.
- 사용량은 `codex app-server`의 `account/rateLimits/read`(b-studio ADR-186).

## 4. VT 엔진 선택지

| 엔진 | 쓰는 곳 | 얻는 것 | 잃는 것 |
|---|---|---|---|
| libghostty(-vt) | Ghostty, cmux(전체 렌더러 포함), JetBrains 2026.3(해석만, 그리기는 IDE), herdr(`ghostty-vt` 크레이트) | Ghostty와 같은 해석, 그래픽 프로토콜, 1.4부터 Apple·Linux·Windows·WASM CI | Zig 툴체인 필요, API가 아직 안정이 아님(버전 고정 권장) |
| alacritty_terminal | Alacritty, Copad macOS, Zed(과거) | 순수 Rust, cargo만으로 빌드 | 외부 사용용 크레이트가 아님(API가 바뀜), Kitty 그래픽·Sixel 없음 |
| wezterm-term | WezTerm | 그래픽 프로토콜 | crates.io에 올라 있지 않음(git 의존) |
| xterm.js | VS Code, typeboard, b-studio desk(headless) | 가장 성숙 | 웹뷰가 필요, 유니코드 폭 표가 다름(b-studio 사진 글꼴 README에 폭 어긋남 기록) |
| VTE4 | GNOME Terminal, Copad Linux | 완성된 위젯 | Linux 전용, PTY 읽기가 그리기와 같은 스레드(Copad가 큰 배경 그림에서 1초 멈춤을 겪음) |

[JetBrains 블로그](https://blog.jetbrains.com/platform/2026/10/jetbrains-terminal-now-uses-ghostty/), [heise: Ghostling](https://www.heise.de/en/news/Ghostling-makes-terminal-emulation-a-C-library-11222728.html), [AkaraChen/2code#145](https://github.com/AkaraChen/2code/issues/145)(xterm.js 대안 평가).

## 5. b-studio와의 관계

- b-studio의 `studio desk`(apps/cli/src/desk)는 `@xterm/headless` + `@lydell/node-pty`로 직접 만든 TUI 렌더러다. herdr를 대신하려고 만들었다(b-studio ADR-183·186).
- 2026-11-05 b-studio를 Rust로 옮기기로 하면서 "b-studio는 관문, 터미널은 b-terminal"로 나눴다(b-studio ADR-191, dj258255/b-studio#997). desk는 옮기지 않는다.
- 그래서 b-terminal이 desk의 화면 역할(에이전트 목록, 세션 터미널, 사용량)을 GUI로 이어받는다. 쓰는 API는 desk와 같은 경로이고 b-studio가 계약 테스트로 고정한다.
- b-studio ADR-186이 받아들인 손실 중 GUI 터미널이 메울 수 있는 것: 데스크톱 알림(보고 있지 않은 창), 반칸 블록 대신 진짜 그림, 미리보기를 옆 칸 웹뷰로(나중), 휴대폰(나중).
