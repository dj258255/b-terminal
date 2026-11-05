# AI 에이전트 시대에 터미널이 갖출 것 (2026-11-06)

범수 님 요청: "AI 에이전틱한 요즘 시대에 제일 걸맞는 터미널, 나만의 터미널". 경계는 ADR-005 그대로다. 에이전트 대시보드(목록·상태·사용량)는 b-studio가 맡고, b-terminal은 **AI CLI가 가장 잘 도는 터미널 엔진**을 맡는다. 웹 검색과 각 문서를 읽은 결과이고, 직접 설치해 비교하지는 않았다.

## AI CLI가 터미널에 바라는 것

| 기능 | 왜 | 출처 | b-terminal |
|---|---|---|---|
| kitty 키보드 프로토콜 | Claude Code의 Shift+Enter 줄바꿈. 없으면 Enter와 같은 바이트가 간다. Claude Code v2.1.269부터 지원을 자동 감지한다 | [shellrs](https://shellrs.com/articles/kitty-keyboard-protocol), [agensi](https://www.agensi.io/learn/claude-code-terminal-setup) | #41 |
| 데스크톱 알림(OSC 9·99·777) | 끝났거나 기다릴 때. Ghostty·kitty·iTerm2만 기본으로 켜져 있다 | [agensi](https://www.agensi.io/learn/claude-code-terminal-setup) | 있음(0.1) |
| 진행 표시(OSC 9;4) | 긴 작업의 진행률. Ghostty 1.3.1이 막대로 보인다 | [Ghostty 릴리스 노트](https://releasebot.io/updates/ghostty) | #42 |
| 클립보드 쓰기(OSC 52) | `/copy` 같은 복사 | [moltamp](https://moltamp.com/blog/best-terminal-for-claude-code-on-mac/) | #42 |
| 셸 통합(OSC 133) | 명령 경계·종료 코드: 프롬프트 사이 이동, 명령 출력 고르기, 끝난 명령 알림 | [terminfo-dev](https://terminfo-dev.pages.dev/extensions/osc-133-semantic-prompts), [WezTerm](https://wezterm.org/shell-integration.html) | #43 |
| 에이전트가 조종하는 API | 소켓으로 탭을 열고, 명령을 넣고, 출력을 읽기. herdr·cmux가 내세운다 | [agentsroom](https://agentsroom.dev/blog/best-terminal-for-agentic-coding) | btctl(0.1) + `btctl run`(#43) |
| 스크롤백 찾기 | 긴 출력에서 오류 찾기. Ghostty 1.3이 넣었다 | [heise](https://www.heise.de/en/news/Stop-broken-SSH-sessions-Ghostty-updates-its-modern-terminal-11503590.html) | #39 |
| 큰 스크롤백, 빠른 해석 | 에이전트는 출력이 많다. Ghostty 1.4는 스크롤백 압축으로 70~90% 메모리를 줄이고 기본 한도를 50MB로 올렸다 | [nexgismo](https://www.nexgismo.com/blog/ghostty-1-4-scrollback-compression-breaking-changes) | 다음 판 |
| 링크(OSC 8), 그림(Kitty 그래픽) | 출력 속 링크 열기, 그림 보기. Ghostty 1.4가 Kitty 그래픽을 전부 지원 | [alternativeto](https://alternativeto.net/news/2026/10/ghostty-1-4-brings-full-kitty-graphics-support-and-performance-and-memory-improvements/) | 링크 #38, 그림은 다음 판 |
| 분할·창 여러 개 | 에이전트와 셸을 나란히 | 모든 비교 글 | #35·#36 |

## 정리

- 비교 글들은 "빠른 에뮬레이터(Ghostty·kitty) + 그 위의 에이전트 런타임(herdr·tmux)"을 흔한 조합으로 든다([agentsroom](https://agentsroom.dev/blog/best-terminal-for-agentic-coding)). b-terminal + b-studio가 바로 그 두 층이다.
- 그래서 b-terminal이 할 일은 에이전트 화면이 아니라, AI CLI가 쓰는 표준 신호를 빠짐없이 받고(키보드·알림·진행·클립보드·셸 통합), 에이전트가 터미널을 도구로 쓰게 하는 것(btctl)이다.
