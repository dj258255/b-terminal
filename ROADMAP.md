# 로드맵

이슈 단위 계획과 실제 시간입니다. 예상은 이슈를 열 때 적은 값이고, 실제는 브랜치를 딴 때부터 PR 병합까지의 대략값입니다(2026-11-05 23시~11-06 01시, 사람 없이 코딩 에이전트가 진행). 틀린 예상은 지우지 않습니다.

## 0.1.0 (2026-11-06)

범위는 [ADR-005](docs/decisions.md#adr-005-b-terminal은-순수-터미널이고-ai-작업-화면은-b-studio가-맡는다): Ghostty 같은 순수 터미널. 추적 이슈 [#1](https://github.com/dj258255/b-terminal/issues/1).

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#2](https://github.com/dj258255/b-terminal/issues/2) | 조사 기록, ADR-001~003 | 1~2시간 | 약 1시간 | [#14](https://github.com/dj258255/b-terminal/pull/14) | 끝 |
| [#3](https://github.com/dj258255/b-terminal/issues/3) | Cargo 작업 공간과 CI | 1시간 | 약 20분 | [#15](https://github.com/dj258255/b-terminal/pull/15) | 끝 |
| [#4](https://github.com/dj258255/b-terminal/issues/4) | 터미널 코어(PTY·VT) | 3~5시간 | 약 1.5시간 | [#16](https://github.com/dj258255/b-terminal/pull/16) | 끝 |
| [#5](https://github.com/dj258255/b-terminal/issues/5) | CPU 렌더러, 실험 E1 | 3~5시간 | 약 1.5시간 | [#17](https://github.com/dj258255/b-terminal/pull/17) | 끝 |
| [#6](https://github.com/dj258255/b-terminal/issues/6) | 창 앱, 한글 IME winit 패치(ADR-004) | 4~6시간 | 약 2시간 | [#18](https://github.com/dj258255/b-terminal/pull/18) | 끝 |
| [#7](https://github.com/dj258255/b-terminal/issues/7) | 탭과 세로 사이드바 | 2~4시간 | 약 1시간 | [#19](https://github.com/dj258255/b-terminal/pull/19) | 끝 → 사이드바는 #25에서 가로 탭 줄로 |
| [#8](https://github.com/dj258255/b-terminal/issues/8) | OSC 9·99·777 알림 | 2~3시간 | 약 1시간 | [#20](https://github.com/dj258255/b-terminal/pull/20) | 끝(보안 지적 3건 병합 전 고침) |
| [#9](https://github.com/dj258255/b-terminal/issues/9) | 제어 소켓과 btctl | 3~4시간 | 약 1.5시간 | [#21](https://github.com/dj258255/b-terminal/pull/21) | 끝(보안 지적 2건 병합 전 고침) |
| [#10](https://github.com/dj258255/b-terminal/issues/10) | 에이전트 훅 | 2~4시간 | 약 1시간 | [#22](https://github.com/dj258255/b-terminal/pull/22) | 끝 → #25에서 걷어냄 |
| [#11](https://github.com/dj258255/b-terminal/issues/11) | 구독 사용량 막대 | 2~3시간 | 약 1시간 | [#23](https://github.com/dj258255/b-terminal/pull/23) | 끝 → #25에서 걷어냄 |
| [#12](https://github.com/dj258255/b-terminal/issues/12) | b-studio 연결 | 4~6시간 | 약 1.5시간(병합 안 함) | — | 하지 않음(ADR-005) |
| [#25](https://github.com/dj258255/b-terminal/issues/25) | AI 기능 걷어내기, 가로 탭 줄, ADR-005 | 2~4시간 | 약 1시간 | [#26](https://github.com/dj258255/b-terminal/pull/26) | 끝 |
| [#13](https://github.com/dj258255/b-terminal/issues/13) | Ghostty 설정 호환, 넣어 둔 글꼴 | 1~2시간 → 2~4시간 | 약 2시간 | [#28](https://github.com/dj258255/b-terminal/pull/28) | 끝 |
| [#27](https://github.com/dj258255/b-terminal/issues/27) | keybind, 설정 다시 읽기 | 2~4시간 | 약 1시간 | [#29](https://github.com/dj258255/b-terminal/pull/29) | 끝 |
| [#24](https://github.com/dj258255/b-terminal/issues/24) | 셸로 돌아오면 마우스 모드 끄기 | 1~3시간 | 약 30분 | [#30](https://github.com/dj258255/b-terminal/pull/30) | 끝 |

예상보다 대체로 짧았던 까닭: 예상은 사람이 하는 시간으로 적었고, 실제는 코딩 에이전트가 쉬지 않고 진행한 시간이다. 대신 키보드·마우스로 직접 쳐 보는 확인이 빠져 있다(각 PR의 "돌리지 않은 검증").

## 0.2.0 (2026-11-06)

범위는 [ADR-006](docs/decisions.md#adr-006-ghostty는-본보기일-뿐-기대지-않는다-ai-cli가-가장-잘-도는-우리-터미널): Ghostty 없이 혼자 서는 우리 터미널. 추적 이슈 [#32](https://github.com/dj258255/b-terminal/issues/32). 실제 시간은 2026-11-06 낮, 사람 없이 코딩 에이전트가 진행한 대략값이다.

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#33](https://github.com/dj258255/b-terminal/issues/33) | Ghostty 없이 혼자 서기: 테마 905개 넣기, Ghostty 설정 읽기 끔(ADR-006) | 1~2시간 | 약 1시간 | [#44](https://github.com/dj258255/b-terminal/pull/44) | 끝 |
| [#41](https://github.com/dj258255/b-terminal/issues/41) | kitty 키보드 프로토콜(Shift+Enter) | 3~4시간 | 약 40분 | [#45](https://github.com/dj258255/b-terminal/pull/45) | 끝 |
| [#42](https://github.com/dj258255/b-terminal/issues/42) | 진행 표시(OSC 9;4)와 클립보드 쓰기(OSC 52) | 2~3시간 | 약 40분 | [#46](https://github.com/dj258255/b-terminal/pull/46) | 끝 |
| [#47](https://github.com/dj258255/b-terminal/issues/47) | [버그] OSC 52 쓰기가 붙여넣기 하이재킹을 연다 | — | 약 35분 | [#48](https://github.com/dj258255/b-terminal/pull/48)·[#49](https://github.com/dj258255/b-terminal/pull/49) | 끝(보고 있는 칸만, 알림 띠) |
| [#43](https://github.com/dj258255/b-terminal/issues/43) | 셸 통합(OSC 133): 프롬프트 이동, 끝난 명령 알림, btctl run | 6~8시간 | 약 2시간 | [#50](https://github.com/dj258255/b-terminal/pull/50) | 끝 |
| [#34](https://github.com/dj258255/b-terminal/issues/34) | macOS 앱 묶음과 메뉴 막대 | 3~4시간 | 약 50분 | [#51](https://github.com/dj258255/b-terminal/pull/51) | 끝 |
| [#35](https://github.com/dj258255/b-terminal/issues/35) | 화면 분할 | 3~5시간 | 약 1시간 30분 | [#52](https://github.com/dj258255/b-terminal/pull/52) | 끝 |
| [#38](https://github.com/dj258255/b-terminal/issues/38) | 링크 열기(URL·OSC 8, Cmd-클릭, ADR-007) | 2~4시간 | 약 1시간 | [#53](https://github.com/dj258255/b-terminal/pull/53) | 끝(보안 검토 전에 병합한 실수 → #54) |
| [#54](https://github.com/dj258255/b-terminal/issues/54) | [버그] 링크 열기: 실행 파일 판정 우회, 주소 속이기, 제곱 시간 | — | 약 40분 | [#55](https://github.com/dj258255/b-terminal/pull/55) | 끝 |
| [#39](https://github.com/dj258255/b-terminal/issues/39) | 찾기(Cmd-F) | 2~3시간 | 약 1시간 10분 | [#56](https://github.com/dj258255/b-terminal/pull/56) | 끝 |
| [#40](https://github.com/dj258255/b-terminal/issues/40) | 커서 깜빡임과 바뀐 줄만 다시 그리기, 실험 E2 | 3~4시간 | 약 1시간 20분 | [#57](https://github.com/dj258255/b-terminal/pull/57) | 끝 |
| [#37](https://github.com/dj258255/b-terminal/issues/37) | 색 이모지(swash) | 2~3시간 | 약 50분 | [#60](https://github.com/dj258255/b-terminal/pull/60) | 끝 |
| [#36](https://github.com/dj258255/b-terminal/issues/36) | 창 여러 개(Cmd-N) | 3~4시간 | 약 1시간 10분 | [#61](https://github.com/dj258255/b-terminal/pull/61) | 끝(보안 지적 1건 병합 전 고침) |
| [#58](https://github.com/dj258255/b-terminal/issues/58) | [버그] 처음 설정을 다시 읽을 때 메모리가 뛴다 | — | 약 40분 | [#62](https://github.com/dj258255/b-terminal/pull/62) | 끝 |
| [#59](https://github.com/dj258255/b-terminal/issues/59) | 색 이모지 글꼴 메모리 매핑(unsafe 예외) 정하기 | 1~3시간 | — | — | 0.3으로(범수 님 결정) |

이번에 배운 것: 보안 경계를 건드린 PR은 커밋 보안 검토 결과를 확인한 뒤 병합한다(#53에서 어김). 병합 스크립트가 그 결과를 기록에서 읽되, 다른 검토와 겹치면 사람이 확인하게 멈춘다.

## 0.3.0 (2026-11-06)

가볍고 정확하게. 추적 이슈 [#64](https://github.com/dj258255/b-terminal/issues/64). 실제 시간은 사람 없이 코딩 에이전트가 진행한 대략값이다.

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#59](https://github.com/dj258255/b-terminal/issues/59) | 색 이모지 글꼴은 필요한 크기 한 벌만(sfnt 골라 읽기, unsafe 없이) | 1~3시간 | 약 50분 | [#69](https://github.com/dj258255/b-terminal/pull/69) | 끝(첫 커밋 검토가 끝나지 않아 보안 검토를 따로 돌림) |
| [#65](https://github.com/dj258255/b-terminal/issues/65) | 흑백 글자도 swash로, fontdue 빼기(ADR-008) | 2~4시간 | 약 50분 | [#70](https://github.com/dj258255/b-terminal/pull/70) | 끝(처음 메모리 378→158MiB) |
| [#66](https://github.com/dj258255/b-terminal/issues/66) | btctl read --lines, btctl wait | 2~3시간 | 약 40분 | [#71](https://github.com/dj258255/b-terminal/pull/71) | 끝 |
| [#67](https://github.com/dj258255/b-terminal/issues/67) | U+FE0F 이모지를 색으로 | 1~2시간 | 약 30분 | [#72](https://github.com/dj258255/b-terminal/pull/72) | 끝 |
| [#68](https://github.com/dj258255/b-terminal/issues/68) | 실험 E3: 창에 올리기, 화면 색 공간(ADR-009) | 1~2시간 | 약 1시간 10분 | [#73](https://github.com/dj258255/b-terminal/pull/73) | 끝(올리기 15→2.7ms) |

이번에 배운 것: 자동 커밋 검토는 끝나지 않을 수 있다(모델에 닿지 못함, 결과 없이 멈춤). 기다리는 데에 기한을 두고, 보안 경계를 건드린 변경이면 검토를 따로 돌린다.

## 0.4.0 (2026-11-06)

그림과 손에 잡히는 것. 추적 이슈 [#75](https://github.com/dj258255/b-terminal/issues/75). 실제 시간은 사람 없이 코딩 에이전트가 진행한 대략값이다.

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#76](https://github.com/dj258255/b-terminal/issues/76) | kitty 그래픽 프로토콜(ADR-010) | 4~6시간 | 약 1시간 40분 | [#80](https://github.com/dj258255/b-terminal/pull/80) | 끝(보안 지적 2건 병합 전 고침, 셀 픽셀 크기 버그도 고침) |
| [#77](https://github.com/dj258255/b-terminal/issues/77) | 명령 팔레트(Cmd-Shift-P) | 2~3시간 | 약 40분 | [#81](https://github.com/dj258255/b-terminal/pull/81) | 끝 |
| [#78](https://github.com/dj258255/b-terminal/issues/78) | 투명 창(background-opacity) | 2~3시간 | 약 40분 | [#82](https://github.com/dj258255/b-terminal/pull/82) | 끝 |
| [#79](https://github.com/dj258255/b-terminal/issues/79) | 실험 E4: 창 버퍼 다시 쓰기 | 1~2시간 | 약 40분 | [#83](https://github.com/dj258255/b-terminal/pull/83) | 끝(이득 작음, 중앙값 약 0.2ms) |

이번에 배운 것: 터미널에 찍힌 글이 보내는 자료(그림)는 바이트 상한만으로 모자라다. 개수와 그리는 양에도 상한이 있어야 한다(보안 검토 지적).

## 0.5.0 (2026-11-06)

다듬기. 추적 이슈 [#85](https://github.com/dj258255/b-terminal/issues/85). 실제 시간은 사람 없이 코딩 에이전트가 진행한 대략값이다.

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#86](https://github.com/dj258255/b-terminal/issues/86) | 투명 창 글자 가장자리(캔버스 곱한 알파) | 1~2시간 | 약 30분 | [#89](https://github.com/dj258255/b-terminal/pull/89) | 끝 |
| [#87](https://github.com/dj258255/b-terminal/issues/87) | 이모지 묶음(ZWJ·피부색·국기) | 2~3시간 | 약 50분 | [#90](https://github.com/dj258255/b-terminal/pull/90) | 끝 |
| [#88](https://github.com/dj258255/b-terminal/issues/88) | 서명·공증 준비 | 1시간 | 약 30분 | [#91](https://github.com/dj258255/b-terminal/pull/91) | 끝(실제 공증은 계정이 있어야 함) |

## 0.6.0 (2026-11-06)

매일 쓰는 터미널. 추적 이슈 [#93](https://github.com/dj258255/b-terminal/issues/93). 사람 없이 코딩 에이전트가 진행했다. 이번 판은 이슈마다 실제 시간을 재지 않았다. 병합 시각(KST)만 적는다: 0.5.0 릴리스 병합 14:42부터 마지막 PR 병합 15:28까지.

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#94](https://github.com/dj258255/b-terminal/issues/94) | 창 상태 복원(`window-save-state`) | 2~3시간 | 재지 않음(병합 14:55) | [#98](https://github.com/dj258255/b-terminal/pull/98) | 끝 |
| [#99](https://github.com/dj258255/b-terminal/issues/99) | `btctl read --lines`가 마지막 글 줄부터(#94 확인 중 찾음) | — | 재지 않음(병합 14:58) | [#100](https://github.com/dj258255/b-terminal/pull/100) | 끝 |
| [#95](https://github.com/dj258255/b-terminal/issues/95) | 퀵 터미널(전역 단축키, `btctl quick`) | 2~3시간 | 재지 않음(병합 15:07) | [#101](https://github.com/dj258255/b-terminal/pull/101) | 끝(실제 키로는 눌러 보지 않음) |
| [#96](https://github.com/dj258255/b-terminal/issues/96) | 글꼴 합자(`font-feature`) | 2~3시간 | 재지 않음(병합 15:14) | [#102](https://github.com/dj258255/b-terminal/pull/102) | 끝 |
| [#97](https://github.com/dj258255/b-terminal/issues/97) | 창이 있는 화면의 색 공간 | 1~2시간 | 재지 않음(병합 15:28) | [#103](https://github.com/dj258255/b-terminal/pull/103) | 끝(다른 화면으로 옮겨 보지 못함) |

이번에 배운 것:

- 다른 앱이 앞에 있을 때 창을 띄우면, 앱이 앞으로 오는 사이에 포커스 잃음이 먼저 온다. 포커스를 잃으면 숨는 창은 보인 직후를 무시해야 한다(#95).
- JetBrains Mono의 합자는 글자 수만큼 글리프를 내고 마지막 글리프가 앞 칸으로 넘쳐 그린다. 칸마다 그리는 렌더러에 그대로 맞는다(#96).
- 한 번씩 띄운 올리기 시간은 같은 빌드에서도 0.1ms 넘게 흔들린다. 견줄 때는 번갈아 여러 번 띄운다(#97).


## 0.7.0 (2026-11-09)

확인과 알림. 추적 이슈 [#105](https://github.com/dj258255/b-terminal/issues/105). 사람 없이 코딩 에이전트가 진행했다. 이번 판도 이슈마다 실제 시간을 재지 않았다. 병합 시각(KST)만 적는다: 추적 이슈를 연 09:37부터 마지막 PR 병합 11:11까지.

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#106](https://github.com/dj258255/b-terminal/issues/106) | 키 순서(`keybind = ctrl+a>n=…`) | 2~3시간 | 재지 않음(병합 09:43) | [#111](https://github.com/dj258255/b-terminal/pull/111) | 끝(실제 키로는 눌러 보지 않음) |
| [#112](https://github.com/dj258255/b-terminal/issues/112) | 탭 막대 글의 색 이모지로 앱이 꺼짐(#107 확인 중 찾음) | — | 재지 않음(병합 09:54) | [#113](https://github.com/dj258255/b-terminal/pull/113) | 끝 |
| [#107](https://github.com/dj258255/b-terminal/issues/107) | 벨(`bell-features`) | 2~3시간 | 재지 않음(병합 09:57) | [#114](https://github.com/dj258255/b-terminal/pull/114) | 끝(Dock·경고음은 확인하지 못함) |
| [#115](https://github.com/dj258255/b-terminal/issues/115) | `btctl new`가 설정의 command를 무시함(#107 확인 중 찾음) | — | 재지 않음(병합 10:00) | [#116](https://github.com/dj258255/b-terminal/pull/116) | 끝 |
| [#108](https://github.com/dj258255/b-terminal/issues/108) | 닫기 확인(`confirm-close-surface`) | 3시간 | 재지 않음(병합 10:08) | [#117](https://github.com/dj258255/b-terminal/pull/117) | 끝(실제 키로는 눌러 보지 않음) |
| [#109](https://github.com/dj258255/b-terminal/issues/109) | 붙여넣기 보호(`clipboard-paste-protection`) | 2~3시간 | 재지 않음(병합 10:14) | [#118](https://github.com/dj258255/b-terminal/pull/118) | 끝(실제로 붙여넣어 보지 않음) |
| [#110](https://github.com/dj258255/b-terminal/issues/110) | 퀵 터미널 자리·크기·화면·움직임 | 3시간 | 재지 않음(병합 11:11) | [#119](https://github.com/dj258255/b-terminal/pull/119) | 끝(화면이 둘일 때는 해 보지 못함) |

이번에 배운 것:

- 탭 막대처럼 셀 격자 밖에서 글을 그리는 길은 셀 격자와 다른 코드라, 셀 격자에서 고친 것(색 이모지)이 거기에는 없었다. 프로그램 출력이 닿는 그리기 길은 모두 같은 글리프 종류를 다뤄야 한다(#112).
- 시험 앱 pid에만 보내는 키 이벤트(`CGEventPostToPid`)는 그 앱이 맨 앞이 아니면 닿지 않는다. 사람의 포커스를 빼앗지 않고는 키 입력을 확인할 길이 없어, 키로 하는 확인은 판단을 순수 함수로 떼어 테스트했다(#108).
- macOS는 테두리 없는 창도 보이거나 옮길 때 메뉴 막대 아래로 밀어 넣는다(`constrainFrameRect:toScreen:`). 위쪽에서 미끄러지는 창은 이것을 건너뛰어야 한다(#110).
- 리눅스 CI에서만 나는 dead_code 경고가 있다. macOS에서만 쓰는 함수는 `cfg_attr`로 다룬다(#110).

## 0.8.0 (2026-11-09)

Ghostty 키 채우기. 추적 이슈 [#121](https://github.com/dj258255/b-terminal/issues/121). 사람 없이 코딩 에이전트가 진행했다. 이슈마다 실제 시간은 재지 않았고 병합 시각(KST)만 적는다: 추적 이슈를 연 11:22부터 마지막 PR 병합 13:07까지(중간에 대화가 끊긴 시간 포함).

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#126](https://github.com/dj258255/b-terminal/issues/126) | 마우스·창의 작은 키(`focus-follows-mouse` 등) | 2~3시간 | 재지 않음(병합 11:27) | [#127](https://github.com/dj258255/b-terminal/pull/127) | 끝(실제 마우스로는 해 보지 않음) |
| [#123](https://github.com/dj258255/b-terminal/issues/123) | 최소 명암비(`minimum-contrast`) | 2~3시간 | 재지 않음(병합 12:52) | [#128](https://github.com/dj258255/b-terminal/pull/128) | 끝(렌더 테스트로 확인) |
| [#122](https://github.com/dj258255/b-terminal/issues/122) | 크기 표시(`resize-overlay`) | 2~3시간 | 재지 않음(병합 12:57) | [#129](https://github.com/dj258255/b-terminal/pull/129) | 끝 |
| [#124](https://github.com/dj258255/b-terminal/issues/124) | 칸 제목 바꾸기(`prompt_surface_title`) | 2~3시간 | 재지 않음(병합 13:02) | [#130](https://github.com/dj258255/b-terminal/pull/130) | 끝(실제 키로는 해 보지 않음) |
| [#125](https://github.com/dj258255/b-terminal/issues/125) | 화면을 파일로(`write_*_file`) | 2~3시간 | 재지 않음(병합 13:07) | [#131](https://github.com/dj258255/b-terminal/pull/131) | 끝(실제 키로는 해 보지 않음) |

이번에 배운 것:

- 검사 명령을 `;`로 이으면 fmt가 실패해도 커밋·올리기가 이어진다(#128에서 fmt가 깨진 채 올림). 배포 스크립트가 올리기 전에 fmt·clippy·작업 트리를 직접 보게 했다.
- 스크립트로 글을 바꿔 넣을 때, fmt가 줄 모양을 바꾸면 바꿔 넣기가 조용히 빠진다(#124). 바꿔 넣을 때마다 맞은 수를 확인한다.
- 설정 해석기가 값을 다듬으면(`trim`) 공백 자체가 뜻인 값(`selection-word-chars`)이 망가진다(#126).
- 커밋 보안 검토 기록 파일은 비워질 수 있다. 결과는 본 즉시 PR에 남긴다(#131).

## 0.9.0 (2026-11-09)

그림·커서·여백. 추적 이슈 [#133](https://github.com/dj258255/b-terminal/issues/133). 사람 없이 코딩 에이전트가 진행했다. 이슈마다 실제 시간은 재지 않았고 병합 시각(KST)만 적는다: 추적 이슈를 연 13:31부터 마지막 PR 병합 20:40까지(중간에 대화가 멈춘 시간 포함).

| 이슈 | 내용 | 예상 | 실제 | PR | 상태 |
|---|---|---|---|---|---|
| [#135](https://github.com/dj258255/b-terminal/issues/135) | 커서 키(`cursor-opacity`, `cell-foreground`) | 2시간 | 재지 않음(병합 14:18) | [#139](https://github.com/dj258255/b-terminal/pull/139) | 끝 |
| [#136](https://github.com/dj258255/b-terminal/issues/136) | 여백 키(`window-padding-balance`·`-color`) | 2~3시간 | 재지 않음(병합 14:25) | [#140](https://github.com/dj258255/b-terminal/pull/140) | 끝 |
| [#138](https://github.com/dj258255/b-terminal/issues/138) | 키보드로 고르기 늘리기·링크 복사 | 2~3시간 | 재지 않음(병합 14:30) | [#141](https://github.com/dj258255/b-terminal/pull/141) | 끝(실제 키로는 해 보지 않음) |
| [#137](https://github.com/dj258255/b-terminal/issues/137) | 프롬프트를 눌러 커서 옮기기 | 2~3시간 | 재지 않음(병합 20:31) | [#142](https://github.com/dj258255/b-terminal/pull/142) | 끝(실제 마우스로는 해 보지 않음) |
| [#134](https://github.com/dj258255/b-terminal/issues/134) | 배경 그림(`background-image`) | 3~4시간 | 재지 않음(병합 20:40) | [#143](https://github.com/dj258255/b-terminal/pull/143) | 끝 |

이번에 배운 것:

- 캔버스에 "기본 바탕색을 칠할 때만 다른 픽셀"이라는 갈래 하나를 두면, 칸 바탕·여백·화면 지우기·바뀐 줄 다시 그리기가 한꺼번에 따라온다(투명 창 #86에 이어 배경 그림 #134).
- `sips -c`는 가운데를 기준으로 자른다. 위쪽을 보려면 전체 캡처를 본다(#136).
- 스크립트로 문서 주석 바로 아래에 새 함수를 끼우면 주석이 새 함수에 붙는다(#135·#134). 끼울 자리는 주석 위로 잡는다.

## 0.10.0 (2026-11-09)

Ghostty 키 채우기 3. 추적 이슈 [#145](https://github.com/dj258255/b-terminal/issues/145). 범수 님 요청("멀티에이전트 여러개해서 구현 다해줘")으로 하위 이슈 여섯을 에이전트 여섯이 동시에 각자의 worktree에서 구현하고(커밋까지), 주 세션이 최신 main 위로 옮기고 검토·보완한 뒤 하나씩 병합했다. 에이전트는 각각 약 5~7분 걸렸다(작업 알림의 시간). 병합 시각(KST): 추적 이슈를 연 20:43부터 마지막 병합 21:10까지.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 고친 것 |
|---|---|---|---|---|---|
| [#147](https://github.com/dj258255/b-terminal/issues/147) | 맨 아래로 스크롤(`scroll-to-bottom`) | 2~3시간 | 에이전트 약 5분(병합 20:53) | [#152](https://github.com/dj258255/b-terminal/pull/152) | 없음 |
| [#146](https://github.com/dj258255/b-terminal/issues/146) | 클립보드·오른쪽 클릭 키 | 2~3시간 | 에이전트 약 5분(병합 20:57) | [#153](https://github.com/dj258255/b-terminal/pull/153) | 기본값을 Ghostty 소스로 확인, 1.4 값 이름 받기 |
| [#150](https://github.com/dj258255/b-terminal/issues/150) | 팔레트 사용자 항목 | 2~3시간 | 에이전트 약 6분(병합 20:59) | [#154](https://github.com/dj258255/b-terminal/pull/154) | 없음 |
| [#148](https://github.com/dj258255/b-terminal/issues/148) | 시작 창 키 | 2~3시간 | 에이전트 약 6분(병합 21:03) | [#155](https://github.com/dj258255/b-terminal/pull/155) | macOS 창 자리를 보이는 영역 기준으로, 실제 창 확인 |
| [#149](https://github.com/dj258255/b-terminal/issues/149) | 키 묶음 접두사·catch_all | 2~3시간 | 에이전트 약 7분(병합 21:06) | [#156](https://github.com/dj258255/b-terminal/pull/156) | Ghostty 소스 규칙 넷과 테스트 |
| [#151](https://github.com/dj258255/b-terminal/issues/151) | 제목·색 보고(ADR-012) | 2~3시간 | 에이전트 약 6분(병합 21:10) | [#157](https://github.com/dj258255/b-terminal/pull/157) | 충돌 여섯 파일 풀기, 보안 검토 |

이번에 배운 것:

- 에이전트가 읽은 Ghostty 문서는 요약이거나 일부뿐이라 기본값·세부 규칙이 빠지곤 했다. Ghostty 소스의 `src/config/Config.zig`(설정과 키 묶음 설명, 기본값)를 받아 대조하면 확실하다. 여섯 중 셋을 이것으로 고쳤다.
- 같은 파일 같은 자리에 여럿이 덧붙이면 병합 순서대로 충돌이 커진다(마지막 브랜치는 여섯 파일). 대부분 둘 다 남기면 되지만, 괄호를 사이에 두는 자리는 손으로 맞춰야 한다.
- zsh에서는 `$B:path`의 `:p` 같은 꼴이 변수 꾸밈으로 읽혀 빈 값이 된다. `${B}:path`로 쓴다.
- worktree에서 `gh pr merge --delete-branch`는 로컬 main으로 바꾸려다 실패하지만 원격 병합과 브랜치 지우기는 된다.

## 0.11.0 (2026-11-09)

Ghostty 키 채우기 4. 추적 이슈 [#159](https://github.com/dj258255/b-terminal/issues/159). 0.10과 같은 방식(에이전트 여섯이 각자 worktree에서 구현·커밋, 주 세션이 옮기고 검토·병합). 이번에는 처음부터 Ghostty 소스 `Config.zig`를 에이전트에게 읽혔다. 병합 시각(KST): 추적 이슈를 연 21:36부터 마지막 병합 21:59까지.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#160](https://github.com/dj258255/b-terminal/issues/160) | 창·탭 이어받기, 탭 막대 | 2~3시간 | 에이전트 약 4분(병합 21:45) | [#166](https://github.com/dj258255/b-terminal/pull/166) | 빠진 되돌림 확인, 실제 창 |
| [#165](https://github.com/dj258255/b-terminal/issues/165) | 소리 벨 | 2~3시간 | 에이전트 약 5분(병합 21:48) | [#167](https://github.com/dj258255/b-terminal/pull/167) | 커밋 섞임 확인 |
| [#161](https://github.com/dj258255/b-terminal/issues/161) | 명령 끝 알림 | 2~3시간 | 에이전트 약 6분(병합 21:50) | [#168](https://github.com/dj258255/b-terminal/pull/168) | 충돌(소리 벨과 같은 자리) |
| [#162](https://github.com/dj258255/b-terminal/issues/162) | 선택·색 키 | 2~3시간 | 에이전트 약 7분(병합 21:53) | [#169](https://github.com/dj258255/b-terminal/pull/169) | 소스 대조, 실제 창 |
| [#163](https://github.com/dj258255/b-terminal/issues/163) | 글꼴 미세 조정 | 2~3시간 | 에이전트 약 8분(병합 21:56) | [#170](https://github.com/dj258255/b-terminal/pull/170) | 테스트 파일 합치기, 실제 창 |
| [#164](https://github.com/dj258255/b-terminal/issues/164) | 닫은 것 되살리기 | 2~3시간 | 에이전트 약 8분(병합 21:59) | [#171](https://github.com/dj258255/b-terminal/pull/171) | 기본 키를 소스와 대조 |

이번에 배운 것:

- 에이전트에게 처음부터 Ghostty 소스를 읽히니, 주 세션이 고쳐야 했던 규칙 차이가 0.10의 셋에서 0으로 줄었다.
- 공유 임시 폴더(scratchpad)에서 에이전트끼리 같은 파일 이름(`app.rs.good`)을 써서 서로 덮어쓴 일이 있었다. 커밋에는 섞이지 않았지만, 에이전트마다 임시 파일 이름에 이슈 번호를 붙이게 해야 한다.
- 기본값을 Ghostty와 맞추면 사용자 동작이 바뀌는 경우(알림 `never`)는 "적지 않음"을 따로 두어 지금 동작을 지키는 쪽이 낫다(#161).

## 0.12.0 (2026-11-09)

Ghostty 키 채우기 5. 추적 이슈 [#173](https://github.com/dj258255/b-terminal/issues/173). 0.11과 같은 방식으로 진행했습니다. 에이전트 일곱이 각자 worktree에서 구현·커밋하고, 주 세션이 옮기고 검토·병합했습니다. 병합 시각(KST)은 추적 이슈를 연 22:32부터 마지막 병합 23:12까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#174](https://github.com/dj258255/b-terminal/issues/174) | 글꼴 고르기 | 2~4시간 | 에이전트 시간 기록 없음(병합 23:01) | [#185](https://github.com/dj258255/b-terminal/pull/185) | 테스트 파일 합치기, 소스 대조, 실제 창 |
| [#175](https://github.com/dj258255/b-terminal/issues/175) | 마우스 키 | 2~4시간 | 에이전트 시간 기록 없음(병합 22:41) | [#181](https://github.com/dj258255/b-terminal/pull/181) | 소스 대조 |
| [#176](https://github.com/dj258255/b-terminal/issues/176) | VT 제한 키(보안 경계) | 2~4시간 | 에이전트 약 12분(병합 23:08) | [#186](https://github.com/dj258255/b-terminal/pull/186) | 보안 관점 diff 읽기, 비네이티브 전체 화면도 무시 조건에, 커밋 보안 리뷰 |
| [#177](https://github.com/dj258255/b-terminal/issues/177) | 스크롤백·팔레트 | 2~4시간 | 에이전트 시간 기록 없음(병합 22:51) | [#183](https://github.com/dj258255/b-terminal/pull/183) | 스크롤백 기본을 릴리스된 값(10MB)으로 되돌림 |
| [#178](https://github.com/dj258255/b-terminal/issues/178) | 명령 실행·창 수명 | 2~4시간 | 에이전트 시간 기록 없음(병합 22:58) | [#184](https://github.com/dj258255/b-terminal/pull/184) | 실제 창(마지막 창 닫기, 빨리 끝난 칸) |
| [#179](https://github.com/dj258255/b-terminal/issues/179) | macOS 창 | 2~4시간 | 에이전트 약 5분(병합 22:49) | [#182](https://github.com/dj258255/b-terminal/pull/182) | 리눅스 clippy dead_code 고침, 실제 창 |
| [#180](https://github.com/dj258255/b-terminal/issues/180) | 글자 묶음 폭(mode 2027) | 2~4시간 | 에이전트 약 11분(병합 23:12) | [#187](https://github.com/dj258255/b-terminal/pull/187) | 이어 붙이기 O(n²) 상한, vendor 원본 diff 대조, 실제 창 |

이번에 배운 것:

- Ghostty 소스의 미출시(1.4) 기본값을 그대로 옮기면 안 되는 경우가 있었습니다. 스크롤백 50MB는 같은 판에 들어간 스크롤백 압축을 전제로 한 값이었습니다(#177). 기본값은 릴리스된 판을 따르고, 이름만 1.4 것을 받는 원칙을 에이전트 지시에도 적어야 합니다.
- 엔진을 고치는 패치(#180)는 커밋 보안 리뷰가 O(n²) 처리 시간을 잡았습니다. 결합 문자 2만 개에 3.1초가 걸렸습니다. 출력이 곧 공격 입력인 터미널에서는, 글자마다 도는 경로에 "앞 상태 전체를 다시 보는" 일이 있으면 상한을 함께 두어야 합니다.
- 실창 확인을 격리(HOME, 소켓 폴더)해서 하려면 소켓 경로가 짧아야 합니다. 임시 폴더가 길면 SUN_LEN을 넘어 제어 소켓이 열리지 않습니다.
- macOS 전용 코드에서만 쓰는 함수는 리눅스 CI에서 dead_code가 됩니다(#119에 이어 #182에서 또). 에이전트 지시에 `cfg_attr`을 미리 적어 둘 만합니다.

## 0.13.0 (2026-11-10)

Ghostty 키 채우기 6. 추적 이슈 [#189](https://github.com/dj258255/b-terminal/issues/189). 0.12와 같은 방식으로 진행했습니다. 에이전트 일곱이 각자 worktree에서 구현·커밋하고, 주 세션이 옮기고 검토·병합했습니다. 병합 시각(KST)은 추적 이슈를 연 11-09 23:27부터 마지막 병합 11-10 00:46까지입니다. 대화가 길어져 세션이 이어지는 사이에 마지막 두 PR(#202, #203)은 이어진 세션에서 확인·병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#190](https://github.com/dj258255/b-terminal/issues/190) | 환경·시작 입력(`term`, `env`, `input`) | 2~4시간 | 에이전트 약 7분(병합 23:45) | [#199](https://github.com/dj258255/b-terminal/pull/199) | ADR 번호를 병합 순서로 옮김(016), decisions.md 합치기, 커밋 보안 리뷰, 실제 창 |
| [#191](https://github.com/dj258255/b-terminal/issues/191) | 링크 키 | 2~4시간 | 에이전트 약 4분(병합 23:35) | [#197](https://github.com/dj258255/b-terminal/pull/197) | 소스 대조(실제 창은 Cmd를 누른 채 올려야 해서 못 봄) |
| [#192](https://github.com/dj258255/b-terminal/issues/192) | 창 모양 | 2~4시간 | 에이전트 약 9분(병합 00:40) | [#202](https://github.com/dj258255/b-terminal/pull/202) | 실제 창에서 `hidden`의 제목 겹침을 찾아 테스트 먼저 쓰고 고침 |
| [#193](https://github.com/dj258255/b-terminal/issues/193) | 글꼴 합성·두껍게 | 2~4시간 | 에이전트 약 9분(병합 23:55) | [#201](https://github.com/dj258255/b-terminal/pull/201) | 소스 대조, 실제 창(Monaco 합성 켬·끔) |
| [#194](https://github.com/dj258255/b-terminal/issues/194) | 셸 통합 기능 | 2~4시간 | 에이전트 약 7분(병합 23:50) | [#200](https://github.com/dj258255/b-terminal/pull/200) | 스크립트 diff 읽기, ADR 번호 옮김(017), 커밋 보안 리뷰, 실제 창 |
| [#195](https://github.com/dj258255/b-terminal/issues/195) | 스크롤바·진행 표시·분할 확대 | 2~4시간 | 에이전트 약 6분(병합 23:39) | [#198](https://github.com/dj258255/b-terminal/pull/198) | 소스 대조, 실제 창(진행 표시 켬·끔) |
| [#196](https://github.com/dj258255/b-terminal/issues/196) | 수정 키 바꾸기, Space, Dock 아이콘, 플랫폼 전용 키 | 2~4시간 | 에이전트 약 10분(병합 00:46) | [#203](https://github.com/dj258255/b-terminal/pull/203) | 충돌 네 곳 정리, 받기만 목록에서 이미 구현된 키 빼기, 실제 창(경고 없음) |

이번에 배운 것:

- 에이전트는 창을 띄우지 않으므로, 화면에서만 보이는 문제는 주 세션의 실제 창 확인이 잡습니다. `hidden` 제목 막대에서 신호등 단추만 숨기고 창 제목 글은 남아 격자 첫 줄 위에 겹쳤습니다(#192). 단위 테스트는 "단추를 숨긴다"만 봤습니다.
- 여러 브랜치가 같은 목록(`KNOWN_IGNORED`, 문서의 "경고 없이 넘기는 키")을 고치면, 나중에 병합하는 쪽이 이미 구현된 키를 다시 "받기만"으로 되돌릴 수 있습니다(#196 충돌). 목록과 해석 코드를 스크립트로 대조해 겹치는 키가 없는 것을 확인했습니다.
- 배포 스크립트(`ship.sh`)와 충돌 해결 스크립트를 세션 임시 폴더에만 두었더니, 세션이 이어질 때 폴더가 비어 대화 기록에서 되살려야 했습니다.
- worktree를 일곱 개 한꺼번에 만들면 경합으로 하나가 실패할 수 있습니다(#194, 남은 브랜치를 지우고 다시 띄움).

## 0.14.0 (2026-11-10)

Ghostty 키 묶음 동작 채우기. 추적 이슈 [#205](https://github.com/dj258255/b-terminal/issues/205). 0.13과 같은 방식으로 진행했습니다. 에이전트 여섯이 각자 worktree에서 구현·커밋하고, 주 세션이 옮기고 검토·병합했습니다. 병합 시각(KST)은 추적 이슈를 연 00:55부터 마지막 병합 01:42까지입니다. 이번 판으로 Ghostty 동작 89개를 모두 받습니다(`cursor_key`는 Ghostty처럼 막음).

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#206](https://github.com/dj258255/b-terminal/issues/206) | 창·탭 동작 | 2~4시간 | 에이전트 약 6.5분(병합 01:29) | [#215](https://github.com/dj258255/b-terminal/pull/215) | 충돌 정리, 옮긴 뒤 깨진 컴파일(#207의 제목 막대 대상) 고침, 검토 에이전트, 실제 창(경고 없음) |
| [#207](https://github.com/dj258255/b-terminal/issues/207) | 제목 동작 | 2~4시간 | 에이전트 약 5분(병합 01:04) | [#212](https://github.com/dj258255/b-terminal/pull/212) | diff 읽기, 실제 창(셸 통합 `title`을 끄고 프로그램 제목 경로) |
| [#208](https://github.com/dj258255/b-terminal/issues/208) | 화면·찾기 동작 | 2~4시간 | 에이전트 약 7분(병합 01:25) | [#214](https://github.com/dj258255/b-terminal/pull/214) | 에이전트가 만든 `cursor_key` 문법을 빼고 Ghostty처럼 막음(테스트 먼저), 실제 창 |
| [#209](https://github.com/dj258255/b-terminal/issues/209) | 토글 동작, 읽기 전용(보안 경계) | 2~4시간 | 에이전트 약 10.5분(병합 01:42) | [#217](https://github.com/dj258255/b-terminal/pull/217) | 검토에서 나온 시작 입력 스레드의 막 빈틈을 테스트 먼저 쓰고 고침, ADR-018 보충, 커밋 보안 리뷰, 실제 창 |
| [#210](https://github.com/dj258255/b-terminal/issues/210) | 키 테이블 | 2~4시간 | 에이전트 약 7분(병합 01:33) | [#216](https://github.com/dj258255/b-terminal/pull/216) | 검토 에이전트 지적으로 8개 상한(Ghostty `Surface.zig`)을 테스트 먼저 쓰고 더함, 실제 창 |
| [#211](https://github.com/dj258255/b-terminal/issues/211) | 받기만 하는 동작, `--config-default-files` | 2~4시간 | 에이전트 약 6분(병합 01:08) | [#213](https://github.com/dj258255/b-terminal/pull/213) | 격리 HOME에서 `--show-config`로 읽은 파일 확인 |

이번에 배운 것:

- 에이전트는 Ghostty에 없는 문법을 만들기도 합니다(`cursor_key:일반,응용`, #208). Ghostty가 설정에서 일부러 막은 동작이었습니다. 동작마다 `Binding.zig`의 주석과 해석 코드를 같이 대조해야 합니다.
- 같은 enum(`Action`)과 같은 함수(`title_of`)를 여섯 브랜치가 고치면, 충돌은 글자로 풀려도 컴파일이 깨질 수 있습니다(#206이 #207의 새 타입을 몰랐음). 옮긴 뒤 빌드가 실패하면 rebase를 이어 가지 않도록 명령을 `&&`로 이어야 합니다(`;`로 이어 깨진 채 이어진 적이 있음).
- "막는 곳은 한 곳"이라는 설계(ADR-018)도, 그 한 곳보다 먼저 생기는 길(칸을 만들기 전에 뜨는 시작 입력 스레드)은 놓칠 수 있습니다. 검토 에이전트가 찾았습니다.
- Ghostty 동작 89개를 해석기에 실제로 넣어 보는 임시 테스트로 "모두 받는다"를 쟀습니다. 이름이 코드에 있는지 grep하는 것보다 정확합니다.

## 0.15.0 (2026-11-10)

Ghostty 동작 맞추기. 추적 이슈 [#219](https://github.com/dj258255/b-terminal/issues/219). 0.14와 같은 방식으로 진행했습니다. 에이전트 넷이 각자 worktree에서 구현·커밋하고, 주 세션이 옮기고 검토·병합했습니다. 병합 시각(KST)은 추적 이슈를 연 01:51부터 마지막 병합 02:13까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#220](https://github.com/dj258255/b-terminal/issues/220) | OSC 52 클립보드 `ask`(보안 경계) | 2~4시간 | 에이전트 약 7분(병합 02:13) | [#227](https://github.com/dj258255/b-terminal/pull/227) | 옮긴 뒤 깨진 컴파일(#223의 `binds_to`) 고침, 보안 관점 diff 읽기, ADR-019 보충, 커밋 보안 리뷰, 실제 창(쓰기 막대, 두 번째 요청 거절) |
| [#221](https://github.com/dj258255/b-terminal/issues/221) | 시스템 밝음·어두움 따라가기 | 2~4시간 | 에이전트 약 6분(병합 02:08) | [#226](https://github.com/dj258255/b-terminal/pull/226) | diff 읽기, 실제 창(밝은 시스템에서 밝은 테마) |
| [#222](https://github.com/dj258255/b-terminal/issues/222) | 스크롤 막대 끌기·누르기 | 2~4시간 | 에이전트 약 6분(병합 02:04) | [#225](https://github.com/dj258255/b-terminal/pull/225) | diff 읽기, 실제 창(경고 없음) |
| [#223](https://github.com/dj258255/b-terminal/issues/223) | 창 상태의 제목, 탭 옮긴 뒤 초점 보고·막대 | 2~4시간 | 에이전트 약 4.5분(병합 02:01) | [#224](https://github.com/dj258255/b-terminal/pull/224) | `CloseTab` 값이 칸 id인지 확인, 실제 창(제목을 넣은 상태 파일로 복원) |

이번에 배운 것:

- 실제 창 확인의 범위를 넓히는 방법: 상태 파일을 손으로 써서 띄우면, 키를 누르지 않고도 복원 경로를 볼 수 있었습니다(#223). 출력이 띄우는 막대는 `btctl send`로 프로그램 출력을 만들어 볼 수 있었습니다(#220).
- 범수 님의 시스템 설정(밝음·어두움)이나 클립보드를 바꿔야 보이는 것은 보지 않았습니다. 사람이 확인할 목록으로 넘깁니다.
- 앞서 병합한 PR이 열거형에 쓰는 함수(`Pending::binds_to`)를 더하면, 뒤에 병합하는 PR의 새 갈래가 컴파일을 깨뜨립니다(#220). 0.14(#206)와 같은 꼴이었습니다. 옮긴 뒤 빌드를 먼저 확인하는 순서를 지켜 바로 잡았습니다.
- 에이전트가 이슈 지시와 다르게 한 것(#223 초점 보고: "I와 O 한 번씩" 대신 상태가 바뀔 때만)은 근거가 맞으면 받아들이고 PR에 적었습니다.

## 0.16.0 (2026-11-10)

Ghostty 명령줄 동작과 색 구성표 보고. 추적 이슈 [#229](https://github.com/dj258255/b-terminal/issues/229). 0.15와 같은 방식으로 진행했습니다. 에이전트 셋이 각자 worktree에서 구현·커밋하고, 주 세션이 옮기고 검토·병합했습니다. 병합 시각(KST)은 추적 이슈를 연 02:18부터 마지막 병합 02:44까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#230](https://github.com/dj258255/b-terminal/issues/230) | 명령줄 동작 1: `+version`·`+help`·`+show-config`·`+list-themes`·`+validate-config`·`+edit-config` | 2~4시간 | 에이전트 약 7분(병합 02:31) | [#233](https://github.com/dj258255/b-terminal/pull/233) | 격리 HOME에서 여덟 명령의 출력·종료 코드, 가짜 편집기로 `+edit-config`의 인자 나누기 |
| [#231](https://github.com/dj258255/b-terminal/issues/231) | 명령줄 동작 2: `+list-keybinds`·`+list-actions`·`+list-fonts`·`+show-face`·`+list-colors`·`+explain-config` | 2~4시간 | 에이전트 약 12분(병합 02:44) | [#235](https://github.com/dj258255/b-terminal/pull/235) | #230과 따로 만든 `+동작` 길을 하나로 모음(테스트 먼저), 로그가 출력에 섞이던 것 고침, X11 색 표를 Ghostty 원본과 대조, 격리 설정으로 실제 실행 |
| [#232](https://github.com/dj258255/b-terminal/issues/232) | 색 구성표 보고(`CSI ? 996 n`, 모드 2031) | 2~4시간 | 에이전트 약 7.5분(병합 02:35) | [#234](https://github.com/dj258255/b-terminal/pull/234) | 보안 관점 diff 읽기(읽기 전용 칸의 답), 실제 창(밝은 시스템에서 `ESC[?997;2n`) |

이번에 배운 것:

- 두 에이전트에게 같은 틀(`+동작`)의 다른 부분을 맡기자, 둘 다 `main()`에 자기 길을 만들었습니다. 잇는 자리(`run_unknown`)를 #230 쪽에만 알렸기 때문입니다. #231은 첫 인자만 봤고 #230은 자리를 가리지 않아, 그대로 두면 같은 명령이 길에 따라 다르게 읽혔습니다. 다음부터는 잇는 자리를 양쪽 프롬프트에 함께 적습니다.
- 에이전트가 WebFetch 요약을 거쳐 옮긴 데이터(X11 색 676개)는 원본을 직접 받아 대조했습니다. 이번에는 다른 값이 없었지만, 요약기를 거친 값은 대조하기 전에는 믿지 않습니다.
- 합치면서 실행 순서가 바뀌는 것(로거 초기화 앞뒤)은 테스트로는 잡히지 않았고, 실제로 명령을 돌려 출력에 로그 줄이 섞인 것을 보고 찾았습니다.

## 0.17.0 (2026-11-10)

Ghostty 받기만 하던 키와 시퀀스 채우기. 추적 이슈 [#237](https://github.com/dj258255/b-terminal/issues/237). 0.16과 같은 방식으로 진행했습니다. 에이전트 넷이 각자 worktree에서 구현·커밋하고, 주 세션이 옮기고 검토·병합했습니다. 병합 시각(KST)은 추적 이슈를 연 02:52부터 마지막 병합 06:33까지입니다. 03:10쯤부터 GitHub Actions 장애(부분 장애 뒤 큰 장애)로 CI가 돌지 않아, 규칙대로 병합을 멈췄다가 장애가 풀린 뒤 이어 갔습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#238](https://github.com/dj258255/b-terminal/issues/238) | `background-blur` | 2~4시간 | 에이전트 약 6분(병합 06:23) | [#243](https://github.com/dj258255/b-terminal/pull/243) | Ghostty `BackgroundBlur` 원본 대조, vendor diff 읽기, 실제 창(흐림 0·40·false 비교), 리눅스 clippy 실패 고침 |
| [#239](https://github.com/dj258255/b-terminal/issues/239) | OSC 22 마우스 포인터 모양 | 2~4시간 | 에이전트 약 10분(병합 06:29) | [#246](https://github.com/dj258255/b-terminal/pull/246) | 이름 표 55개를 Ghostty 원본과 대조, 실제 창(시퀀스를 보내도 멀쩡함) |
| [#240](https://github.com/dj258255/b-terminal/issues/240) | `+show-config`·`+list-themes` 인자 | 2~4시간 | 에이전트 약 5.5분(병합 03:02) | [#242](https://github.com/dj258255/b-terminal/pull/242) | 기본 출력이 바뀌는 근거(`changes-only` 기본 true)를 원본으로 확인, 격리 HOME으로 실제 실행 |
| [#241](https://github.com/dj258255/b-terminal/issues/241) | `window-title-font-family` | 2~4시간 | 에이전트 약 10분(병합 06:33) | [#247](https://github.com/dj258255/b-terminal/pull/247) | 실제 창에서 한글이 벌어지는 것을 찾아 테스트 먼저 쓰고 고침, 없는 글꼴 경고 |
| [#244](https://github.com/dj258255/b-terminal/issues/244) | 모드 2031 테스트가 흔들림(버그) | — | 주 세션 약 15분(병합 06:26) | [#245](https://github.com/dj258255/b-terminal/pull/245) | 부하 속 30번 반복으로 재현(4번 실패), 고친 뒤 0번 |

이번에 배운 것:

- 받기만 하던 이유("unsafe가 필요하다", 0.13)는 다시 볼 만했습니다. vendor한 winit에 이미 safe 함수가 있었습니다(#238).
- 에이전트의 테스트가 모두 통과해도 실제 창에서만 보이는 것이 있었습니다. 비례폭 제목에서 한글 사이가 벌어진 것(#241)은 화면을 보고서야 찾았습니다.
- 에이전트 실행 환경에서 curl과 HOME 지정이 막혀, 원본 대조와 격리 실행은 주 세션이 했습니다. 공통 지시에 "데이터는 curl로 원본"을 넣었지만 막히면 요약으로 돌아가므로, 주 세션이 대조하는 단계는 계속 필요합니다.
- CI가 장애로 돌지 않을 때 기다리는 동안 로컬 일(흔들리는 테스트 재현·수정, 실제 창 확인)을 했습니다.

## 0.18.0 (2026-11-10)

0.17에서 남긴 것. 추적 이슈 [#249](https://github.com/dj258255/b-terminal/issues/249). 에이전트 셋이 각자 worktree에서 구현·커밋하고, 주 세션이 옮기고 검토·병합했습니다. 병합 시각(KST)은 추적 이슈를 연 06:39부터 마지막 병합 06:53까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#250](https://github.com/dj258255/b-terminal/issues/250) | 포인터: 마우스 보고 모드·Shift | 2~4시간 | 에이전트 약 6분(병합 06:53) | [#255](https://github.com/dj258255/b-terminal/pull/255) | PTY 읽기 스레드 변경 읽기, 실제 창(시퀀스) |
| [#251](https://github.com/dj258255/b-terminal/issues/251) | `+show-config` theme 줄, 키 단위 비교 | 2~4시간 | 에이전트 약 5분(병합 06:50) | [#254](https://github.com/dj258255/b-terminal/pull/254) | 이름만 쓴 사용자 테마로 격리 HOME 실제 실행 |
| [#252](https://github.com/dj258255/b-terminal/issues/252) | 비례폭 제목의 색 이모지 폭 | 2~4시간 | 에이전트 약 4분(병합 06:47) | [#253](https://github.com/dj258255/b-terminal/pull/253) | Ghostty 제목 크기 원본(`TerminalController.swift` 661줄) 대조, 실제 창 |

이번에 배운 것:

- 에이전트가 이슈 전제를 바로잡았습니다: b-terminal에는 사각 고르기가 없어 Alt 규칙을 넣지 않았고(#250), "`keybind = clear`가 숨는다"는 재현되지 않았습니다(#251). 이슈를 쓸 때 전제를 코드로 먼저 확인하면 좋습니다.

## 0.19.0 (2026-11-10)

보안 입력과 체크섬 보고. 추적 이슈 [#257](https://github.com/dj258255/b-terminal/issues/257). 에이전트 둘이 각자 worktree에서 구현·커밋하고, 주 세션이 옮기고 검토·병합했습니다. 병합 시각(KST)은 추적 이슈를 연 07:03부터 마지막 병합 07:32까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#258](https://github.com/dj258255/b-terminal/issues/258) | 보안 키보드 입력(보안 경계) | 2~4시간 | 에이전트 약 13분(병합 07:26) | [#260](https://github.com/dj258255/b-terminal/pull/260) | Ghostty macOS 원본(SecureInput.swift 등)을 받아 에이전트에 넘김, 끝난 칸이 보안 입력을 쥐고 남는 빈틈을 테스트 먼저 쓰고 고침, 실제 창(`ioreg`로 켜짐·풀림, 자물쇠 커서, 탭 줄과 겹친 배지를 고침) |
| [#259](https://github.com/dj258255/b-terminal/issues/259) | DECRQCRA 체크섬 보고(보안 경계) | 2~4시간 | 에이전트 약 13분(병합 07:32) | [#261](https://github.com/dj258255/b-terminal/pull/261) | #258 위로 옮김(테스트 파일 충돌을 함수 경계에 맞춰 풂), 리뷰받은 커밋과 줄 변경 대조, 실제 창(켬 `FEDA`, 끔 답 없음) |

이번에 배운 것:

- 에이전트의 curl이 막혀 있어 WebFetch 요약에 기대던 것을, 주 세션이 Ghostty 원본 파일을 미리 받아 넘기는 방식으로 바꿨습니다. 두 에이전트 모두 원문 줄 번호로 인용했고, 대조할 요약이 없었습니다.
- 커밋 보안 hook은 두 커밋이 몇 초 차이로 들어오면 판정 줄만으로는 어느 커밋의 것인지 가리기 어렵습니다. 또 `&&`로 이은 긴 명령의 커밋을 "실패"로 잘못 읽고 건너뛰기도 했습니다. 판정 수와 시작 수를 맞춰 보고, 건너뛴 커밋은 직접 읽고 PR에 적었습니다.
- 실제 창에서만 보이는 것이 또 나왔습니다: 탭이 둘일 때 배지가 탭 줄과 겹침. 에이전트가 보고에 "겹칠 수 있다"고 적은 곳을 먼저 봤습니다.

## 0.20.0 (2026-11-10)

Ghostty가 답하는 VT 질의. 추적 이슈 [#263](https://github.com/dj258255/b-terminal/issues/263). 에이전트 넷이 각자 worktree에서 구현·커밋하고, 주 세션이 옮기고 검토·병합했습니다. 하위 이슈 둘(#269, #272)은 진행 중에 주 세션이 찾아 더했습니다. 병합 시각(KST)은 추적 이슈를 연 07:43부터 마지막 병합 09:16까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#264](https://github.com/dj258255/b-terminal/issues/264) | CSI 질의: XTVERSION, DA1·DA2, `CSI 16·19 t`, 모드 2048(보안 경계) | 2~4시간 | 에이전트 약 11분(병합 08:05) | [#267](https://github.com/dj258255/b-terminal/pull/267) | 0.19.0을 격리한 창에서 질의해 빈칸을 찾음, Ghostty 원본(stream_handler.zig 등)을 받아 넘김, 실제 창(답이 한 번만 가는지) |
| [#265](https://github.com/dj258255/b-terminal/issues/265) | DCS 질의: DECRQSS, XTGETTCAP(보안 경계) | 2~4시간 | 에이전트 약 15분(병합 08:18) | [#268](https://github.com/dj258255/b-terminal/pull/268) | #264 위로 옮김, XTGETTCAP 형식을 terminfo/Source.zig 원문과 대조, #264가 생긴 뒤의 `RV`·`rv`·`XR`·`xr`을 테스트 먼저 더함, 실제 창 |
| [#269](https://github.com/dj258255/b-terminal/issues/269) | OSC 22 테스트가 셸이 빨리 뜨면 첫 단언에서 실패(버그) | 없음(버그 양식) | 주 세션 약 15분(병합 08:21) | [#270](https://github.com/dj258255/b-terminal/pull/270) | #268의 CI에서 봄, 0.5초 지연을 넣어 같은 실패를 재현, `read`로 순서를 정해 고침 |
| [#266](https://github.com/dj258255/b-terminal/issues/266) | kitty 색 프로토콜 OSC 21(보안 경계) | 2~4시간 | 에이전트 약 15분(병합 09:04) | [#271](https://github.com/dj258255/b-terminal/pull/271) | 색 글 해석을 `RGB.parse` 원문에 맞춤, `osc-color-report-format`의 범위를 Ghostty에 맞춤, 실제 창(바탕을 빨강으로), CI의 새 clippy 경고 |
| [#272](https://github.com/dj258255/b-terminal/issues/272) | OSC 4·10·11·12 질의가 바꾼 색이 아니라 테마 색으로 답함(버그) | 없음(버그 양식) | 에이전트 약 6분(병합 09:16) | [#273](https://github.com/dj258255/b-terminal/pull/273) | #266 에이전트가 남긴 기존 문제를 격리한 창에서 재서 이슈로 엶, 답의 순서를 지키라는 제약을 프롬프트에 적음, 실제 창(순서 포함) |

이번에 배운 것:

- 에이전트가 원문 없이 짠 해석(색 글)은 테스트가 다 통과해도 원문과 달랐습니다(`#` 없는 16진수, 버림). 데이터만이 아니라 해석 규칙도 원문과 대조합니다.
- CI의 clippy가 로컬보다 새것이라 로컬에서 깨끗한 코드가 CI에서 막혔습니다(`question_mark`). 내보낸 뒤 CI 출력까지 봅니다.
- 흔들리는 테스트는 기다리는 시간을 늘리지 않고, 셸이 테스트의 신호를 받은 뒤에 움직이게 해서 순서를 정했습니다. 실패를 재현하려고 일부러 지연을 넣어 봤습니다.

## 0.21.0 (2026-11-10)

글자 꾸밈과 macOS 앱 통합. 추적 이슈 [#275](https://github.com/dj258255/b-terminal/issues/275). 에이전트 넷이 각자 worktree에서 구현·커밋하고, 주 세션이 옮기고 검토·병합했습니다. 병합 시각(KST)은 추적 이슈를 연 09:24부터 마지막 병합 10:07까지입니다. 에이전트 시간은 #278(완료 알림의 약 7분)과 #279의 두 번째 에이전트(약 4분)만 남아 있고, 나머지는 재지 않았습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#278](https://github.com/dj258255/b-terminal/issues/278) | 오른쪽 클릭 메뉴(`right-click-action = context-menu`) | 2~4시간 | 에이전트 약 7분(병합 09:39) | [#280](https://github.com/dj258255/b-terminal/pull/280) | Surface.zig·SurfaceView_AppKit.swift 원문 대조, vendor winit `popup_context_menu`의 안전하지 않은 약속을 문서에 적음 |
| [#277](https://github.com/dj258255/b-terminal/issues/277) | 윗줄(SGR 53·55), vte를 vendor로 | 2~4시간 | 에이전트 재지 않음(병합 09:48) | [#281](https://github.com/dj258255/b-terminal/pull/281) | #280 위로 옮김, 금지했던 session.rs 3줄을 받아들임, 실제 창(윗줄·DECRQSS·XTGETTCAP) |
| [#276](https://github.com/dj258255/b-terminal/issues/276) | 밑줄 다섯 모양과 밑줄 색(SGR 58) | 2~4시간 | 에이전트 재지 않음(병합 09:58) | [#282](https://github.com/dj258255/b-terminal/pull/282) | 에이전트가 패치로 남긴 연결(session.rs)을 적용하고 되돌려 실패를 확인, #281과 겹친 `DrawCell`·비트·그리는 순서를 풂, 실제 창(모양·색·흐림·반전·숨김) |
| [#279](https://github.com/dj258255/b-terminal/issues/279) | 제목 막대 폴더 아이콘과 Dock 끌어놓기(보안 경계) | 2~4시간 | 에이전트 두 번(두 번째 약 4분, 병합 10:07) | [#283](https://github.com/dj258255/b-terminal/pull/283) | 커밋 보안 검토의 따옴표 탈출 지적(fish·tcsh)을 받아 허용 목록 규칙을 정해 두 번째 에이전트에게 맡김, 실제 창(`visible`·`hidden`) |

이번에 배운 것:

- 에이전트에게 "건드리지 않을 파일"로 준 session.rs가 두 이슈 모두에서 꼭 고쳐야 하는 자리였습니다(엔진 셀을 화면 칸으로 옮기는 곳이 하나뿐). 한 에이전트는 고치고 알렸고, 다른 에이전트는 패치로 남겨 커밋이 실제 화면에서는 아무것도 바꾸지 못했습니다. 금지 목록을 쓰기 전에 그 기능이 지나가는 길을 먼저 따라가 봅니다.
- 같은 구조체(`DrawCell`)·비트 표(`CellFlags`)·목록(`CAPS`)에 여러 에이전트가 끝에 덧붙이면 충돌은 기계적이지만 비트 번호는 사람이 다시 매겨야 했습니다.
- Ghostty와 같은 동작(따옴표로 감싼 경로를 셸에 침)도 사용자 셸이 POSIX가 아니면 안전하지 않았습니다. 커밋 보안 검토가 이것을 잡았고, Ghostty보다 좁게 막았습니다.

## 0.22.0 (2026-11-10)

VT 세부 맞추기. 추적 이슈 [#285](https://github.com/dj258255/b-terminal/issues/285). 주 세션이 Ghostty Config.zig·Binding.zig와 견줘 설정 키·키 묶음 동작이 모두 맞는 것을 확인하고, 0.21.0을 격리한 창에서 남은 VT 차이를 쟀습니다. 에이전트 둘이 각자 worktree에서 구현·커밋하고, 하나는 주 세션이 직접 했습니다. 병합 시각(KST)은 추적 이슈를 연 13:25부터 마지막 병합 13:47까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#288](https://github.com/dj258255/b-terminal/issues/288) | 밑줄 친 링크를 이중 밑줄로 | 2~4시간 | 주 세션 약 15분(병합 13:32) | [#289](https://github.com/dj258255/b-terminal/pull/289) | 직접 구현(app.rs 한 곳), 테스트 먼저 |
| [#286](https://github.com/dj258255/b-terminal/issues/286) | OSC 12 질의와 DECSTR의 커서 되돌림 | 2~4시간 | 에이전트 약 6분(병합 13:39) | [#290](https://github.com/dj258255/b-terminal/pull/290) | 격리한 창에서 차이를 잼, 옮김, 실제 창(OSC 12 다섯 경우·DECSTR) |
| [#287](https://github.com/dj258255/b-terminal/issues/287) | 깜박임 DECRQSS, alacritty `Flags` u32 | 2~4시간 | 에이전트 약 6분(병합 13:47) | [#291](https://github.com/dj258255/b-terminal/pull/291) | 옮김, vendor의 닿지 않는 갈래 경고를 고침, 실제 창 |

이번에 배운 것:

- vendor 크레이트의 경고는 `cargo clippy -- -D warnings`가 잡지 않고 빌드 출력에만 보였습니다(작업 공간 밖이라 clippy 인자가 가지 않음). 합친 뒤 빌드 출력의 `warning`도 봅니다.
- 고칠 자리가 한 곳인 이슈는 에이전트에게 넘기는 비용(프롬프트·옮기기)이 구현보다 컸습니다. 주 세션이 직접 했습니다.

## 0.23.0 (2026-11-10)

VT 시퀀스 빈칸 메우기. 추적 이슈 [#293](https://github.com/dj258255/b-terminal/issues/293). 조사 에이전트가 Ghostty stream.zig·modes.zig와 vendor vte를 견줘 버려지는 시퀀스를 찾았고, 주 세션이 격리한 창에서 쟀습니다. 에이전트 셋이 각자 worktree에서 구현·커밋하고, 주 세션이 검토·원본 대조·옮기기·실제 창·병합을 했습니다. 병합 시각(KST)은 추적 이슈를 연 13:54부터 마지막 병합 14:31까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#295](https://github.com/dj258255/b-terminal/issues/295) | 열 삽입·삭제, DECBI·DECFI, CTC, HPB·VPB, 문자 집합 바꿈 | 2~4시간 | 에이전트 약 7분(병합 14:10) | [#297](https://github.com/dj258255/b-terminal/pull/297) | 실제 창(`btctl read`·스크린샷). DECDC가 스크롤 영역의 모든 줄에 먹는 것을 처음엔 DECIC 실패로 잘못 읽었다가 다시 잼 |
| [#294](https://github.com/dj258255/b-terminal/issues/294) | 보호 칸과 선택 지우기 | 3~5시간 | 에이전트 약 9분(병합 14:16) | [#298](https://github.com/dj258255/b-terminal/pull/298) | #295 위로 옮김, 실제 창(네 경우) |
| [#296](https://github.com/dj258255/b-terminal/issues/296) | 옛 대체 화면·DECNKM과 DECRQM 답 | 2~4시간 | 에이전트 약 9분(병합 14:31) | [#306](https://github.com/dj258255/b-terminal/pull/306) | 옮김, PR을 연 뒤 #304가 먼저 병합돼 충돌 → force-push 없이 main을 merge, 실제 창(DECRQM 11개·47·1048) |

이번에 배운 것:

- 실제 창에서 두 시퀀스를 한 화면에 이어 찍으면 앞 결과를 뒤 시퀀스가 덮을 수 있습니다(DECDC는 스크롤 영역의 모든 줄을 당김). 하나씩 `btctl read`로 보는 편이 확실했습니다.
- PR을 연 뒤 main이 움직여 충돌하면 rebase 대신 main을 merge해서 force-push를 피했습니다. squash 병합이라 기록은 같습니다.

## 0.24.0 (2026-11-10)

남은 DEC 모드. 추적 이슈 [#299](https://github.com/dj258255/b-terminal/issues/299). 0.23.0을 합친 빌드에서 Ghostty modes.zig의 모든 모드를 DECRQM으로 물어 `0`으로 답하는 것을 골랐습니다. 주 세션이 엔진 `TermMode`를 `u64`로 넓힌 뒤(#304), 에이전트 넷이 각자 worktree에서 구현·커밋하고 주 세션이 검토·원본 대조·옮기기·실제 창·병합을 했습니다. 병합 시각(KST)은 추적 이슈를 연 14:14부터 마지막 병합 15:20까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#304](https://github.com/dj258255/b-terminal/issues/304) | 엔진 `TermMode`를 `u64`로 | 15분 | 주 세션 약 10분(병합 14:22) | [#305](https://github.com/dj258255/b-terminal/pull/305) | 직접 구현. 에이전트들이 쓸 비트 번호를 미리 나눔 |
| [#303](https://github.com/dj258255/b-terminal/issues/303) | 가시성 보고(2033) | 2~4시간 | 에이전트 약 7분(병합 15:01) | [#308](https://github.com/dj258255/b-terminal/pull/308) | 0.23.0 위로 옮김, 실제 창(탭 전환). 창 가림은 확인하지 못함 |
| [#301](https://github.com/dj258255/b-terminal/issues/301) | 키 입력 모드(67·1035·1036·1039) | 2~4시간 | 에이전트 약 8분(병합 15:08) | [#309](https://github.com/dj258255/b-terminal/pull/309) | 옮김, ADR 번호를 병합 순서로 027, 실제 창(DECRQM) |
| [#302](https://github.com/dj258255/b-terminal/issues/302) | 화면 반전·역방향 줄바꿈·저장만 하는 모드 | 2~4시간 | 에이전트 약 18분(병합 15:15) | [#315](https://github.com/dj258255/b-terminal/pull/315) | 실제 창에서 반전 중 제목 막대가 흰색이 되어 제목이 안 보이는 것을 찾아 에이전트에게 고치게 함(Ghostty TerminalWindow.swift 대조). 에이전트가 네트워크 오류로 멈춰 되돌림 시험을 직접 함 |
| [#300](https://github.com/dj258255/b-terminal/issues/300) | 마우스 보고 형식(9·1005·1015·1016)과 추가 단추 | 2~4시간 | 에이전트 약 13분(병합 15:20) | [#316](https://github.com/dj258255/b-terminal/pull/316) | 옮김, ADR 번호 028, 실제 창(DECRQM) |

이번에 배운 것:

- 비트 번호처럼 여러 에이전트가 같은 자리에 쓰는 값은 미리 나눠 프롬프트에 적으니 충돌이 줄 하나씩 나란히 두는 것으로 끝났습니다.
- 화면 반전은 테스트가 모두 통과했지만 실제 창에서 제목 막대가 흰색이 되는 것을 보았습니다. 창 테두리처럼 터미널 격자 밖에 그리는 것은 실제 창으로만 보였습니다.
- 실제 창 스크린샷은 늘 창 번호(`-l`)를 줍니다. 이번에 한 번 빠뜨려 전체 화면을 찍었고, 보지 않고 바로 지웠습니다.

## 0.25.0 (2026-11-10)

좌우 여백과 남은 입력 모드. 추적 이슈 [#310](https://github.com/dj258255/b-terminal/issues/310). 0.24.0을 쌓은 브랜치(c8fb66c)에서 에이전트 넷이 각자 worktree에서 구현·커밋하고, 주 세션이 검토·원본 대조·옮기기·실제 창·병합을 했습니다. 비트 번호(TermMode 48·49, 셀 표시 18)와 ADR 번호(029)는 프롬프트에서 미리 나눴습니다. 병합 시각(KST)은 추적 이슈를 연 15:10부터 마지막 병합 15:57까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#314](https://github.com/dj258255/b-terminal/issues/314) | XTCHECKSUM 깜박임·보호·쓴 적 없는 칸 | 2~4시간 | 에이전트 약 5분(병합 15:34) | [#318](https://github.com/dj258255/b-terminal/pull/318) | 새 셀 표시가 셀 비교에 걸리지 않는지 찾음, 실제 창(합 다섯) |
| [#313](https://github.com/dj258255/b-terminal/issues/313) | XTSAVE·XTRESTORE | 2~4시간 | 에이전트 약 7분(병합 15:48) | [#319](https://github.com/dj258255/b-terminal/pull/319) | 실제 창(DECRQM 아홉, 1049 복원 뒤 화면) |
| [#312](https://github.com/dj258255/b-terminal/issues/312) | 키패드 응용 모드 바이트와 modifyOtherKeys | 2~4시간 | 에이전트 약 9분(병합 15:53) | [#320](https://github.com/dj258255/b-terminal/pull/320) | Ghostty `should_modify` 대조, 되돌려도 통과하던 RIS·DECSTR 시험에 켜짐 단언을 더함. 실제 키는 누르지 않음 |
| [#311](https://github.com/dj258255/b-terminal/issues/311) | 좌우 여백(DECLRMM·DECSLRM) | 2~4시간 | 에이전트 약 29분, 6단계(병합 15:57) | [#321](https://github.com/dj258255/b-terminal/pull/321) | 여백 없을 때 빠른 스크롤 경로와 크기 변경 확인, ECH를 Ghostty 원문으로 확인, #313 위로 옮기며 `CSI s` 갈래를 손으로 풂, 실제 창(넷) |

이번에 배운 것:

- 같은 vte 자리(`CSI s`)를 두 이슈가 다른 뜻으로 고치면(XTSAVE의 `?` 갈래, DECSLRM의 맨 갈래) 자동 "양쪽 살리기"로는 괄호가 맞지 않았습니다. 그런 충돌은 손으로 풀고 바로 빌드했습니다.
- 에이전트가 "되돌려도 통과하는 시험"을 스스로 보고해 주어 바로 고칠 수 있었습니다. 보고 항목에 넣어 둔 덕입니다.
- 실제 창 시험 글은 줄마다 달라야 합니다. 모든 줄의 4~10열이 같은 글이면 IL이 사각형을 밀었는지 화면으로 가릴 수 없었습니다.

## 0.26.0 (2026-11-10)

DECCOLM과 kitty 클립보드. 추적 이슈 [#323](https://github.com/dj258255/b-terminal/issues/323). 0.25.0에서 에이전트 둘이 각자 worktree에서 구현·커밋하고, 주 세션이 검토·원본 대조·보안 지적 돌려보내기·옮기기·실제 창·병합을 했습니다. 두 에이전트 모두 API 사용 한도로 한 번 멈췄다가 이어서 끝냈습니다. 병합 시각(KST)은 추적 이슈를 연 16:29부터 마지막 병합 20:12까지입니다(멈춘 시간 포함).

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#324](https://github.com/dj258255/b-terminal/issues/324) | DECCOLM(3)과 모드 40 | 2~4시간 | 에이전트(멈춘 시간 빼고 재지 않음, 병합 19:52) | [#330](https://github.com/dj258255/b-terminal/pull/330) | Ghostty `deccolm`·termio 대조(PTY는 알리지 않음), 새 함수에 붙어 버린 설명 주석 제자리로, 자동 보안 검토가 판정 없이 끝난 커밋의 diff를 직접 읽음, 실제 창(넷) |
| [#325](https://github.com/dj258255/b-terminal/issues/325) | kitty 클립보드(OSC 5522)와 붙여넣기 이벤트 | 2~4시간 | 에이전트 이어서 약 20분(병합 20:12) | [#331](https://github.com/dj258255/b-terminal/pull/331) | 커밋 보안 검토 지적 넷(방향별 일회용, 위치, 별칭 크기, 읽기 면제가 보는 칸 검사를 건너뜀)을 돌려보냄, 에이전트가 요약으로 본 `paste.zig`를 원본으로 대조, 실제 창(거절 설정에서 여덟) |

이번에 배운 것:

- 커밋 보안 검토가 사용 한도(HTTP 429)로 판정 없이 끝날 수 있습니다. 그 커밋은 diff를 직접 읽고 PR에 그렇게 적었습니다.
- "확인 없이 답해도 되는 요청"을 정책 값(`allow`)으로 올리면, 그 값이 건너뛰는 다른 검사(보는 칸·읽기 전용)까지 같이 건너뜁니다. 면제는 확인 막대만 건너뛰게 따로 둡니다.
- 다른 판(0.27) 에이전트를 같이 돌리니, 검토하다 판 사이에 부딪치는 자리(#327 ED 22가 기대는 엔진 갈래를 #329가 바꿈)를 병합 전에 찾았습니다.

## 0.27.0 (2026-11-10)

셸 통합 마무리와 작은 질의. 추적 이슈 [#326](https://github.com/dj258255/b-terminal/issues/326). 0.26 조사에서 Ghostty가 받는데 b-terminal이 버리던 것을 골랐습니다. 0.25.0에서 에이전트 셋이 0.26 에이전트와 함께 각자 worktree에서 구현·커밋하고, 주 세션이 0.26 릴리스 뒤 검토·원본 대조·옮기기·실제 창·병합을 했습니다. ADR 번호(031·032)는 프롬프트에서 미리 나누고 병합 순서를 그에 맞췄습니다. 병합 시각(KST)은 추적 이슈를 연 19:44부터 마지막 병합 20:43까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#327](https://github.com/dj258255/b-terminal/issues/327) | DECXCPR, ED 22, XTSHIFTESCAPE | 2~4시간 | 에이전트 약 8분(병합 20:26) | [#333](https://github.com/dj258255/b-terminal/pull/333) | 실제 창(DECXCPR 셋, ED 22), 0.26.0 위로 옮기며 CHANGELOG·ADR 목차의 빠진 빈 줄을 넣음 |
| [#328](https://github.com/dj258255/b-terminal/issues/328) | OSC 7 빈 값과 `kitty-shell-cwd://` | 2~4시간 | 에이전트 약 5분(병합 20:33) | [#334](https://github.com/dj258255/b-terminal/pull/334) | 실제 창(`btctl list`로 다섯 경우), 옮기기 전후 코드 패치가 같은지 비교 |
| [#329](https://github.com/dj258255/b-terminal/issues/329) | OSC 133 확장 | 2~4시간 | 에이전트 약 17분 + 후속 약 6분(병합 20:43) | [#335](https://github.com/dj258255/b-terminal/pull/335) | ED 2 조건과 다시 그리기를 Ghostty 원문으로 확인, ED 2 뒤 그림이 남는 것과 bash `redraw=last`를 찾아 돌려보냄, #327 ED 22와 부딪치는 자리를 맞춤, 실제 창(DECCOLM으로 리사이즈를 일으켜 `redraw` 셋) |

이번에 배운 것:

- 두 판의 에이전트를 함께 돌리면, 앞 판 병합 전에 뒤 판끼리 부딪치는 자리(#329가 바꾼 ED 2 갈래를 #327 ED 22가 기댐)를 검토에서 찾을 수 있었습니다. 옮긴 직후 #327 시험이 실패해 바로 확인됐습니다.
- 배포 도구가 커밋 보안 검토 기록에서 "이 커밋 뒤의 첫 판정"을 읽어, 판정이 없던 커밋(검토할 소스 파일 없음)에 다른 커밋의 판정을 붙였습니다. #335는 그 상태로 병합됐고(커밋마다의 판정은 미리 확인함), 도구는 다음 커밋의 시작이나 "검토할 소스 파일 없음"을 만나면 판정 없음으로 멈추게 고쳤습니다.
- 창 크기를 바꾸지 않고 리사이즈 갈래를 보려면 DECCOLM(모드 40)으로 격자 크기를 바꾸면 됩니다.

## 0.28.0 (2026-11-10)

프롬프트 클릭. 추적 이슈 [#337](https://github.com/dj258255/b-terminal/issues/337). 범수 님이 범위를 `click_events`까지로 정했습니다. 0.27.0에서 에이전트 하나가 worktree에서 클릭 이동을 구현·커밋하고, 주 세션이 DECSED 22와 검토 중 찾은 bash 버그를 직접 고치고 검토·원본 대조·옮기기·실제 창·병합을 했습니다. 셀 표시 비트(21)와 ADR 번호(033)는 프롬프트에서 미리 정했습니다. 병합 시각(KST)은 추적 이슈를 연 21:57부터 마지막 병합 22:33까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#339](https://github.com/dj258255/b-terminal/issues/339) | DECSED 22(`CSI ? 22 J`) | 2~4시간 | 주 세션 약 15분(병합 22:07) | [#340](https://github.com/dj258255/b-terminal/pull/340) | 직접 구현(vte 갈래 한 곳) |
| [#338](https://github.com/dj258255/b-terminal/issues/338) | 프롬프트 클릭 `cl=`·`click_events=`, 입력 칸 표시, 셸 스크립트 | 2~4시간 | 에이전트 약 15분(병합 22:25) | [#341](https://github.com/dj258255/b-terminal/pull/341) | vendor 변경 직접 읽기, 넓은 글자 세기를 Ghostty 원문으로 확인해 ADR 문장 고침, bash 3.2에서 여러 줄 PS1 치환 실행, 실제 창(두 줄 프롬프트), #340 위로 옮기기 |
| [#342](https://github.com/dj258255/b-terminal/issues/342) | bash 빈 Enter가 명령 `_bt_status`로 세어짐 | — | 주 세션 약 15분(병합 22:33) | [#343](https://github.com/dj258255/b-terminal/pull/343) | #338 검토 중 PTY 바이트에서 찾아 직접 고침 |

이번에 배운 것:

- 셸 스크립트를 바꾸는 PR은 시험(`bash -c`)만이 아니라 실제 대화형 셸을 PTY에 띄워 바이트를 읽어 보면 좋습니다. 그렇게 해서 #50부터 있던 bash 빈 Enter 문제를 찾았습니다.
- 에이전트가 "원문 동작은 확인하지 못했다"고 남긴 자리(넓은 글자의 뒤 칸)는 주 세션이 받아 둔 원문으로 바로 확인할 수 있었습니다. 보고에 모르는 것을 모른다고 적게 한 덕입니다.

## 0.29.0 (2026-11-11)

GPU 렌더러 1단계. 추적 이슈 [#345](https://github.com/dj258255/b-terminal/issues/345). 범수 님이 0.28 다음 판을 GPU 렌더러로, 라이브러리를 wgpu로 정했습니다(안전한 API라 `unsafe_code = "deny"`를 지키고, 사용자 셰이더 GLSL을 naga로 읽을 수 있음). Ghostty가 macOS에서 IOSurface를 레이어에 올리는 것을 원본으로 확인하고 두 단계로 나눴습니다. 0.28.0처럼 에이전트 둘이 각자 worktree에서 구현·커밋하고, 주 세션이 검토·원본 대조·실험 E5 측정·옮기기·병합을 했습니다. ADR 번호(034·035)와 실험 번호(E5·E6)는 프롬프트에서 미리 나눴습니다. 병합 시각(KST)은 추적 이슈를 연 22:50부터 마지막 병합 다음 날 00:53까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#346](https://github.com/dj258255/b-terminal/issues/346) | macOS 화면 올리기를 IOSurface로 | 4~8시간 | 에이전트 약 13분(병합 23:52) | [#348](https://github.com/dj258255/b-terminal/pull/348) | vendor unsafe 읽기, Ghostty 대조, 격리한 실제 창에서 실험 E5(다섯 쌍), 두 길의 화면 그림을 픽셀로 비교(기본·srgb·투명) |
| [#347](https://github.com/dj258255/b-terminal/issues/347) | wgpu 셀 렌더러 1단계(창 없이) | 1~2일 | 에이전트 약 15분(병합 다음 날 00:53) | [#349](https://github.com/dj258255/b-terminal/pull/349) | 읽기·아틀라스 검토, #348 위로 옮기기, GPU 시험과 E6를 직접 다시 돌림 |

이번에 배운 것:

- 작업 공간 규칙(`unsafe_code = "deny"`)이 라이브러리 고르기를 바꿨습니다. 이슈를 연 뒤 공통 지시를 쓰다가 알아채 범수 님께 다시 여쭸습니다. 큰 일은 이슈를 열기 전에 규칙(unsafe, 의존성, CI)을 먼저 훑습니다.
- 창이 있어야 재는 값은 에이전트가 잴 수 없으니, 에이전트에게는 견주는 스위치(`BT_E5_CGIMAGE`)와 잴 명령을 만들게 하고 주 세션이 쟀습니다. 측정 스크립트는 소켓을 못 찾으면 멈추게 해야 합니다(첫 스크립트가 측정 창 열 개를 끄지 못했습니다).

## 0.30.0 (2026-11-11)

GPU 렌더러 2단계. 추적 이슈 [#351](https://github.com/dj258255/b-terminal/issues/351). 범수 님이 0.29.0 다음 판을 GPU 2단계로 정했습니다. 앱 창 연결(#352)과 빠진 그리기(#353)를 에이전트 둘이 각자 worktree에서 구현·커밋하고, 주 세션이 검토·옮기기·실제 창 비교·실험 E7·병합을 했습니다. 두 에이전트가 같은 크레이트(bt-gpu)를 고치므로 나눈 파일과 잇는 자리(`Scene::build` 시그니처를 바꾸지 않음)를 두 프롬프트에 똑같이 적었고, 겹친 곳은 renderer.rs 세 군데(손으로 양쪽 살림)뿐이었습니다. 병합 시각(KST)은 추적 이슈를 연 01:35부터 마지막 병합 02:20까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#353](https://github.com/dj258255/b-terminal/issues/353) | bt-gpu 빠진 그리기(밑줄 모양·상자 글자·선택·preedit·최소 대비·빈 상자·칸 알파) | 1일 | 에이전트 약 15분(병합 01:58) | [#354](https://github.com/dj258255/b-terminal/pull/354) | bt-render 변경 검토(CPU 결과 그대로인지), 시험 다시 돌림 |
| [#352](https://github.com/dj258255/b-terminal/issues/352) | wgpu 렌더러를 앱 창에 연결(`BT_GPU=1`, CPU 두 층 사이에 GPU 격자) | 1~2일 | 에이전트 약 17분(병합 02:20) | [#355](https://github.com/dj258255/b-terminal/pull/355) | #354 위로 옮기며 충돌 풀기, 격리한 실제 창에서 두 길 화면 그림 픽셀 비교(네 장면), 실험 E7(세 장면 다섯 쌍), 문서에 결과 |

이번에 배운 것:

- 에이전트가 정한 시나리오(진행 막대로 매 프레임 통째)를 주 세션이 더한 장면(화면 전체 다시 찍기)으로 바꿔 재면 결론이 반대로 나옵니다. 층 올리기 비용은 B에서만 보였습니다. 실험 문서의 시나리오를 먼저 그대로 재고, 더한 장면은 따로 적습니다.
- 측정 로그를 줄 수로 거를 때는 창의 줄 수를 먼저 봅니다(37줄 창에 40줄 이상 필터를 걸어 한 번 헛걸렀습니다).
- kitty 그림을 셸 `printf`로 따옴표를 여러 겹 감싸 보내면 깨집니다. 바이트를 파일로 만들어 `cat`으로 보내니 바로 됐습니다.

## 0.31.0 (2026-11-11)

GPU 렌더러 3단계. 추적 이슈 [#357](https://github.com/dj258255/b-terminal/issues/357). 범수 님이 0.30.0 다음 판을 GPU 3단계로 정했고, 이 판을 여는 중에 "전부 다 해줘 나한테 묻지말고 전부"라고 하셔서 다음 판부터는 범위를 묻지 않고 ROADMAP과 ADR의 남은 조건으로 정합니다. ADR-036의 기본 바꿀 조건을 채우는 두 갈래를 에이전트 둘이 각자 worktree에서 구현·커밋하고, 주 세션이 검토·옮기기·실제 창 비교·실험 E8·병합을 했습니다. 병합 시각(KST)은 추적 이슈를 연 03:24부터 마지막 병합 04:01까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#358](https://github.com/dj258255/b-terminal/issues/358) | GPU 길 층 올리기 줄이기(바뀐 사각형만, 빈 위층 건너뛰기), 긴 프레임 원인 | 4~8시간 | 에이전트 약 9분(병합 03:52) | [#360](https://github.com/dj258255/b-terminal/pull/360) | 실험 E8(세 장면 다섯 쌍, 긴 프레임 내역), 화면 그림 비교(잔상 없음), 문서에 결과 |
| [#359](https://github.com/dj258255/b-terminal/issues/359) | GPU 길 합자·투명 창·배경 그림 | 4~8시간 | 에이전트 약 12분(병합 04:01) | [#361](https://github.com/dj258255/b-terminal/pull/361) | #360 위로 옮기기(코드 패치 같음 비교), 실제 창에서 합자·투명·배경 그림 두 길 비교, ADR에 결과 |

이번에 배운 것:

- 긴 프레임은 로그 내역을 나눠 찍게 한 것만으로 원인이 보였습니다(그림판 얻기 대기, 창이 보이기 전 첫 프레임). 짐작으로 설정을 바꾸지 않고 먼저 나눠 재게 한 순서가 맞았습니다.
- 에이전트가 "추정"으로 남긴 것(CoreAnimation의 `PostMultiplied` 합성)은 실제 창 비교 한 번으로 확인됐습니다. 추정을 추정이라고 적게 한 덕에 주 세션이 볼 것이 분명했습니다.
- kitty 그림을 번호(`i=1`)와 함께 보내면 터미널의 답이 셸 입력에 끼어들어 다음 명령이 깨집니다. 화면 비교용 그림은 번호 없이 보냅니다.

## 0.32.0 (2026-11-11)

GPU 렌더러 4단계. 추적 이슈 [#363](https://github.com/dj258255/b-terminal/issues/363). 0.31.0 ROADMAP의 '다음'을 범수 님 말씀("전부 다 해줘 나한테 묻지말고 전부")에 따라 주 세션이 그대로 정했습니다. 에이전트 둘이 각자 worktree에서 프레임 간격과 PTY 시험 간헐 실패를 맡았고, 주 세션이 검토·실제 창 실험 E9·병합을 한 뒤 기본 길 전환과 실험 E10을 직접 했습니다. 병합 시각(KST)은 추적 이슈를 연 04:11부터 마지막 병합 05:35까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#364](https://github.com/dj258255/b-terminal/issues/364) | GPU 길 프레임 간격(막힐 것 같은 프레임 미루기, 가려진 창 건너뛰기, 같은 층 견주지 않기) | 4~8시간 | 에이전트 약 13분(병합 05:04) | [#367](https://github.com/dj258255/b-terminal/pull/367) | 실험 E9(E8과 견준 뒤 바로 앞 빌드와 쌍으로 다시 잼), 미룬 프레임이 사라지지 않는지 끝 화면, 화면 비교, 결과 문서 |
| [#365](https://github.com/dj258255/b-terminal/issues/365) | PTY 시험이 가끔 `Session::spawn`에서 실패 | 예상 없음(버그) | 에이전트 약 26분(병합 04:59), 재현 못 함 | [#366](https://github.com/dj258255/b-terminal/pull/366) | troubleshooting 6번에 같은 날 본 실패를 더함 |
| [#368](https://github.com/dj258255/b-terminal/issues/368) | macOS 기본 그리기 길을 GPU로, `BT_GPU=0`이면 CPU | 2~4시간 | 주 세션 약 1시간 30분(병합 05:35) | [#369](https://github.com/dj258255/b-terminal/pull/369) | 직접 구현, 실험 E10(창 여는 시간·CPU 시간·RSS), 해 보고 버린 시도(보이기 전 층 올리기) 측정, 화면 비교 |

이번에 배운 것:

- E9를 미리 정한 대로 E8과 견주자 화면 전체 장면이 0.12ms 나빠 보였습니다. 그 사이 병합된 #361(합자 기본 켜기) 때문이었습니다. 바로 앞 빌드를 같은 target에서 따로 빌드해 쌍으로 재야 바뀐 것만 보입니다.
- 창을 여는 시간을 첫 프레임 시각만으로 견주면 GPU가 85ms 손해로 보였습니다. CPU 길은 창이 보이기 전에 올려 두기 때문입니다. 사람이 보는 때(창이 보인 때와 첫 프레임 중 늦은 쪽)로 다시 정의하니 33ms였습니다.
- 빈 창 시간을 줄이려고 짐작한 고침(보이기 전에 층 올려 두기)은 다섯 쌍을 재니 효과가 없어 넣지 않았습니다.
- PTY 시험 실패가 또 났는데, 시험 출력을 요약만 걸러 봐서 오류 본문을 놓쳤습니다. 전체 시험 출력은 파일로 남깁니다.

## 0.33.0 (2026-11-11)

사용자 셰이더. 추적 이슈 [#371](https://github.com/dj258255/b-terminal/issues/371). 0.32.0 ROADMAP의 '다음'을 범수 님 말씀("전부 다 해줘 나한테 묻지말고 전부")에 따라 주 세션이 그대로 정했습니다. 에이전트 둘이 각자 worktree에서 셰이더 엔진과 설정 키·유니폼 값을 맡았고, 주 세션이 검토·병합한 뒤 앱 연결과 실제 창 확인, 실험 E11을 직접 했습니다. 병합 시각(KST)은 추적 이슈를 연 05:46부터 마지막 병합 06:42까지입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#373](https://github.com/dj258255/b-terminal/issues/373) | 설정 키 `custom-shader`·`custom-shader-animation`과 셰이더에 넘길 시간·커서·초점·색 값 | 3~6시간 | 에이전트 약 8분(병합 06:04) | [#374](https://github.com/dj258255/b-terminal/pull/374) | Ghostty 원문(generic.zig)과 규칙 대조(8ms, 첫 프레임, 초점 시각, 커서 모서리) |
| [#372](https://github.com/dj258255/b-terminal/issues/372) | GPU 길 셰이더 엔진(naga로 GLSL 읽기, 패스 잇기) | 4~8시간 | 에이전트 약 16분(병합 06:11) | [#375](https://github.com/dj258255/b-terminal/pull/375) | 옮기기, 주석 하나 바로잡기, 이 기기 GPU로 시험이 건너뛰지 않고 도는지 확인 |
| [#376](https://github.com/dj258255/b-terminal/issues/376) | 셰이더를 앱에 잇기, 실험 E11 | 3~5시간 | 주 세션 약 35분(병합 06:42) | [#377](https://github.com/dj258255/b-terminal/pull/377) | 직접 구현, 실제 창(항등·invalid·focus·crt·커서 자리·설정 다시 읽기·CPU 길), E11에서 찾은 메인 스레드 막힘 고치기와 재측정 |

이번에 배운 것:

- 셰이더를 이어 붙이고 시험이 다 통과한 뒤에도, 실제 창에서 재 보니 메인 스레드가 10초에 9.3초를 그림판 얻기에서 기다렸습니다. 프레임 미루기(0.32)는 들은 주사율(120Hz)로 짐작했는데, 계속 올리는 동안 화면은 60Hz로 가져갔습니다. 짐작 대신 막힌 간격을 배우게 고쳤습니다.
- 두 에이전트에게 같은 값 구조를 나눠 줄 때 "잇는 자리"(같은 필드 이름·타입)를 두 프롬프트에 같은 글로 넣으니 연결이 필드 옮기기로 끝났습니다. 다만 한쪽이 미리 만든 도우미 둘(y 뒤집기, 그려지는 커서 모양 번호)은 원문을 끝까지 따라가 보니 쓸 곳이 없어 지웠습니다.
- 에이전트가 찾은 naga의 Metal 쪽 값 밀림(`float[4]`)은 오프셋 단위 시험이 못 잡고 GPU 시험만 잡았습니다. CI에는 GPU가 없어 그 시험이 건너뛰므로, 주 세션이 이 기기에서 건너뛰지 않고 도는지 확인했습니다.

## 0.34.0 (2026-11-11)

GPU 5단계. 추적 이슈 [#379](https://github.com/dj258255/b-terminal/issues/379). 0.33.0 ROADMAP의 '다음'을 범수 님 말씀("전부 다 해줘 나한테 묻지말고 전부")에 따라 주 세션이 그대로 정했습니다.

- 진행: 에이전트 둘이 각자 worktree에서 장치 나눠 쓰기(#380)와 셰이더 컴파일 비동기(#381)를 맡았고, 주 세션이 창이 보인 뒤 첫 그리기(#382)와 실험 E12를 맡았습니다.
- 시각(KST): 추적 이슈를 06:53에 열었고, 마지막 병합은 16:33입니다. 그 사이 기기가 다시 켜지며 세션이 끊겼고(길이는 재지 않음), #381 에이전트는 커밋하기 전에 멈췄습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#380](https://github.com/dj258255/b-terminal/issues/380) | GPU 길 창들이 wgpu 장치 하나를 나눠 쓰기 | 4~8시간 | 에이전트 약 23분(커밋까지), **넣지 않음**(16:02 닫음) | - | 실제 창 1·2·3개의 RSS를 기준 빌드와 번갈아 세 번: 차이가 흔들림 폭(약 1MB) 안이라 계획 안 함으로 닫음 |
| [#382](https://github.com/dj258255/b-terminal/issues/382) | 창이 보인 뒤 처음 그리기까지의 지연 줄이기 | 2~4시간 | 주 세션(벽시계로 이슈부터 병합까지 약 9시간 15분, 끊긴 시간 포함), 병합 16:15 | [#383](https://github.com/dj258255/b-terminal/pull/383) | 직접 구현, 첫 프레임 로그로 지연 자리 찾기, 열 쌍 측정(중앙값 25.4 → 14.0ms) |
| [#381](https://github.com/dj258255/b-terminal/issues/381) | 셰이더 컴파일이 창 열기와 설정 다시 읽기를 막지 않게 | 3~6시간 | 에이전트(시간 모름, 커밋 전에 멈춤) + 주 세션 마무리, 병합 16:33 | [#384](https://github.com/dj258255/b-terminal/pull/384) | 남은 변경 검토·커밋·옮기기, 되돌림 확인, 실제 창에서 설정 다시 읽기 답의 오류가 빠진 것을 찾아 GLSL 검증을 거는 자리로 되돌림, 창 여는 시간 다섯 쌍(315.2 → 166.5ms) |

이번에 배운 것:

- 장치 나눠 쓰기는 창 없는 시험에서 렌더러 하나에 약 0.3MB를 아꼈지만, 실제 창에서는 차이가 없었습니다. 기준 빌드도 둘째 창부터 장치 열기가 3.5~7.6ms였습니다(Metal이 안에서 나눠 쓴다고 짐작). 짐작한 이득은 실제 창에서 재 보고 정합니다.
- 첫 프레임이 늦은 까닭은 짐작(층 올리기) 대신 로그에 "그 프레임을 시작한 때"를 더해 재니 바로 보였습니다. winit의 `request_redraw`가 다시 그리기를 이벤트 고리 끝으로 미뤘습니다.
- 일을 다른 스레드로 옮기면 그 일이 내던 오류도 늦게 옵니다. 시험이 다 통과했어도 `btctl reload-config`의 답에서 셰이더 오류가 빠진 것은 실제 창에서만 보였습니다. 짧은 검증은 거는 자리에 두었습니다.
- 기기를 다시 켜면 scratchpad(`/private/tmp`)가 비어 배포 도구와 측정 원자료가 사라집니다. 원자료가 필요한 값은 잴 때 찍힌 줄을 바로 문서에 옮깁니다.
- 다른 빌드가 도는 동안(1분 부하 50~98) 잰 시간은 몇 배로 부풀었습니다. 시간 측정은 부하를 보고 합니다.

## 0.35.0 (2026-11-11)

GPU 6단계: 창 하나의 메모리. 추적 이슈 [#386](https://github.com/dj258255/b-terminal/issues/386). 0.34.0 ROADMAP의 '다음'을 범수 님 말씀("전부 다 해줘 나한테 묻지말고 전부")에 따라 주 세션이 그대로 정했습니다.

- 진행: 이슈를 열기 전에 `vmmap -summary`로 창 1개→2개를 재어 내역을 갈랐습니다(GPU 길 +118MB 가운데 창 크기 버퍼 약 다섯 장 52MB). 고칠 자리가 작아 둘 다 주 세션이 직접 했습니다.
- 시각(KST): 추적 이슈를 16:54에 열었고, 마지막 병합은 17:49입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#387](https://github.com/dj258255/b-terminal/issues/387) | 빈 위층은 메모리를 들지 않기 | 2~4시간 | 약 35분(이슈부터 병합 17:29까지) | [#389](https://github.com/dj258255/b-terminal/pull/389) | 직접 구현. 첫 구현(그린 뒤 훑기)이 통째 프레임 그리기를 약 1ms 늘려, 판단을 그리기 앞으로 옮기고 새 텍스처는 그린 자리만 올림. footprint 155 → 123MB(창 1) |
| [#388](https://github.com/dj258255/b-terminal/issues/388) | 층 올리기 임시 버퍼를 키우지 않기 | 1~2시간 | 약 20분(#389 병합부터 병합 17:49까지) | [#390](https://github.com/dj258255/b-terminal/pull/390) | 직접 구현. 이슈의 안(커진 뒤 놓기)은 macOS malloc이 놓은 덩어리를 쥐고 있어 효과가 없어, 줄 묶음으로 나눠 올림. footprint 123 → 114MB(창 1) |

이번에 배운 것:

- RSS에는 GPU 텍스처가 빠집니다. 창의 메모리는 `vmmap -summary`의 physical footprint로 재고, 영역별(MALLOC_LARGE, IOSurface, owned unmapped graphics)로 갈라 보면 어디를 고칠지 바로 보입니다.
- 메모리를 놓아도 물리 사용량이 줄지 않을 수 있습니다. macOS malloc은 놓은 큰 덩어리를 `MALLOC_LARGE (empty)`로 쥐고 있었습니다. 놓기보다 처음부터 잡지 않는 길이 효과가 있었습니다.
- 메모리를 줄이는 변경은 시간을 함께 잽니다. 첫 구현은 메모리를 줄였지만 통째 프레임을 약 1ms 늘렸고, 이슈에 적어 둔 기준(1ms) 덕에 다시 생각했습니다.
- 화면 비교로 찍을 때 셸이 늦게 뜨면 다른 화면이 찍힙니다. 다르면 먼저 그림을 보고, 다시 찍어 가립니다.

## 0.36.0 (2026-11-11)

GPU 7단계: 아래층 사본과 그림판 수. 추적 이슈 [#392](https://github.com/dj258255/b-terminal/issues/392). 0.35.0 ROADMAP의 '다음'을 주 세션이 그대로 정했습니다. 범수 님 말씀("서브 에이전트로 다 해줘")에 따라 구현은 서브에이전트가 각자 worktree에서 했고, 주 세션은 실제 창 측정, 검토, 병합, 릴리스를 맡았습니다.

- 시각(KST): 추적 이슈를 18:36에 열었고, 마지막 병합은 19:44입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#394](https://github.com/dj258255/b-terminal/issues/394) | 그림판 3장 → 2장 | 2~3시간 | 약 32분(이슈부터 병합 19:08까지, 에이전트 약 4분 + 2분) | [#395](https://github.com/dj258255/b-terminal/pull/395) | 지연 1과 2를 실제 창에서 번갈아 쟀습니다(footprint, 출력 시간, 얻기 합). 결정 기준을 채워 에이전트에게 기본값을 1로 바꾸게 했습니다. 부하 7~15에서 보인 차이는 부하 3~5에서 다시 재 가렸습니다 |
| [#393](https://github.com/dj258255/b-terminal/issues/393) | 층 사본을 칸 해시로 | 3~5시간 | 약 68분(이슈부터 병합 19:44까지, 에이전트 약 8분 + 5.5분 + 9.5분) | [#396](https://github.com/dj258255/b-terminal/pull/396) | 첫 구현이 실제 창에서 층 그대로·작은 변경 프레임을 0.4~0.7ms 늘린 것을 재서 되돌려 보냈습니다. 둘째 구현이 CI에서 해시 충돌 시험에 걸리자 시드 300개로 재 원인(같은 갈래 안의 32비트 한계)을 짚어 다시 맡겼습니다 |

이번에 배운 것:

- 에이전트의 측정 입력이 실제 화면과 다르면 성능 판단이 뒤집힙니다. 무작위 25% 입력에서는 해시가 빨랐지만, 실제 아래층(거의 바탕색 하나)에서는 옛 방식의 `memcmp`가 같은 줄을 빠르게 넘겨 해시가 느렸습니다. 실제 창 값을 들고 다시 맡기니 고쳐졌습니다.
- 시드가 프로세스마다 다른 해시의 품질 시험은 한 기계에서 통과해도 다른 기계에서 실패할 수 있습니다. 고정 시드 여러 개로 돌려야 확률이 보이고, 겹친 쌍의 모양(같은 갈래)이 원인을 가리킵니다.
- 벡터로 돌던 코드도 클로저나 0 채우기를 끼우면 한 픽셀씩 도는 코드로 컴파일될 수 있습니다(에이전트가 1.8ms까지 느려진 것을 보고 일반 반복문으로 되돌림). 고칠 때마다 시간을 다시 잽니다.
- 설정한 그림판 수와 실제가 다를 수 있습니다(지연 1에서도 3장 몫이 남은 회). 설정만 보지 않고 vmmap으로 확인합니다.

## 0.37.0 (2026-11-11)

GPU 8단계: 그림판 수와 GPU 텍스처. 추적 이슈 [#398](https://github.com/dj258255/b-terminal/issues/398). 0.36.0 ROADMAP의 '다음'을 주 세션이 정했습니다. 범수 님 말씀("서브 에이전트로 다 해줘", "다음날까지 끝까지 계속 다해줘")에 따라 하위 이슈는 서브에이전트가 각자 worktree에서 맡았고, 주 세션은 실제 창 측정, 검토, 병합, 릴리스를 맡았습니다.

- 진행
  - 이슈를 열기 전에 0.36.0을 GPU 길과 CPU 길 여섯 쌍으로 쟀습니다(vmmap). GPU 길 창 1은 92.4~102.5MB, CPU 길은 80.5~81.7MB였습니다.
  - 그 차이를 영역별로 갈라 하위 이슈 둘을 정했습니다.
  - 0.36.0 ROADMAP에 적었던 다른 두 후보는 하지 않았습니다.
    - 앱 아래층 프레임 놓기: #388에서 놓아도 macOS malloc이 쥐었습니다.
    - 셰이더 틈: 0.34에서 고른 트레이드오프입니다.
- 시각(KST): 추적 이슈를 19:58에 열었고, 마지막 병합은 20:23입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#399](https://github.com/dj258255/b-terminal/issues/399) | 그림판이 3장 몫으로 남는 까닭 | 2~4시간 | 약 15분(에이전트), 까닭을 확정하지 못해 고치지 않고 닫음 | 없음(기록은 #401의 실험 E15 A절) | 에이전트의 로그·시간순 표본을 실험 문서로 옮기고, 이슈의 기준대로 배제한 가설과 함께 닫음 |
| [#400](https://github.com/dj258255/b-terminal/issues/400) | 창마다 GPU 텍스처 내역과 줄이기 | 2~4시간 | 약 25분(이슈부터 병합 20:23까지, 에이전트 약 12분) | [#401](https://github.com/dj258255/b-terminal/pull/401) | vmmap 네 쌍(GPU 텍스처 18.7 → 14.7MB), 이모지 화면 세 장면 비교(다른 픽셀 0), 탭 열고 닫기 두 쌍. CHANGELOG의 '재지 않았다'를 실측값으로 바꾸고 실험 E15를 씀 |

이번에 배운 것:

- 줄이기 전에 바이트로 세면 할 일이 바로 정해집니다. 창마다 7.8MB 가운데 5.2MB가 아틀라스였고, 그 가운데 4MB는 이모지를 쓰지 않으면 필요가 없었습니다.
- `vec![0; n]`으로 잡은 CPU 메모리는 쓰기 전까지 footprint에 들지 않습니다. 에이전트가 줄였다고 센 CPU 사본 4MB는 실제 창에서 드러나지 않았습니다. 바이트를 센 값과 vmmap 값을 늘 함께 적습니다.
- 지연 잡기는 다른 길의 잠재 버그를 드러낼 수 있습니다. 한 아틀라스만 다시 만들 때 다른 아틀라스의 올리기를 건너뛰던 버그가 화면 비교 시험에서 나왔습니다.
- 까닭을 찾지 못한 조사도 배제한 가설을 남기면 다음 판의 출발점이 됩니다. 이슈에 '찾지 못하면 적고 닫는다'를 미리 적어 두어 멈출 때를 정할 수 있었습니다.
- 두 에이전트가 같은 기계에서 창을 띄우고 빌드하면 부하가 40~55까지 오릅니다. 메모리 값은 부하에 흔들리지 않지만, 시간 값은 부하를 함께 적습니다.

## 0.38.0 (2026-11-11)

GPU 길 그리기 키. 추적 이슈 [#403](https://github.com/dj258255/b-terminal/issues/403).

0.37.0 ROADMAP의 '다음'(화면 갱신에 맞춰 그리기)을 살피다가 다음을 찾았습니다.
- Ghostty에는 이 일을 정하는 키 `window-vsync`가 있고, b-terminal은 GPU 그리기 키 `window-vsync`·`alpha-blending`을 받기만 했습니다.
- 그 까닭("CPU로 그린다")은 0.32부터 맞지 않았습니다.

그래서 주 세션이 이 판을 정했습니다. 범수 님 말씀("서브 에이전트로 다 해줘", "다음날까지 끝까지 계속 다해줘")에 따라 하위 이슈는 서브에이전트가 각자 worktree에서 맡았고, 주 세션은 실제 창 측정, 검토, 옮기기, 병합, 릴리스를 맡았습니다.

- 시각(KST): 추적 이슈를 20:34에 열었고, 마지막 병합은 21:04입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#404](https://github.com/dj258255/b-terminal/issues/404) | `window-vsync` | 2~3시간 | 약 17분(이슈부터 병합 20:51까지, 에이전트 약 6분) | [#406](https://github.com/dj258255/b-terminal/pull/406) | `true`·`false` 세 쌍과 기준 세 쌍을 쟀습니다. `false`의 얻기 막힘(프레임마다 약 15ms)을 찾아, Immediate에서 미룸을 켠 실험 빌드도 쟀습니다. 키가 하는 일이 없어지는 것을 보고 Ghostty의 뜻대로 두고 잰 값을 문서에 적었습니다 |
| [#405](https://github.com/dj258255/b-terminal/issues/405) | `alpha-blending` | 3~5시간 | 약 30분(이슈부터 병합 21:04까지, 에이전트 약 13.5분) | [#407](https://github.com/dj258255/b-terminal/pull/407) | #406 위로 옮기며 설정 문서·CHANGELOG 충돌을 풀었습니다. 실제 창에서 네 설정으로 색 짝 장면을 견줬습니다(`native` 다른 픽셀 0). 설정 다시 읽기, 이모지, 프레임 시간도 봤습니다 |

이번에 배운 것:

- 받기만 하는 키의 까닭은 구조가 바뀌면 낡습니다. GPU 길을 기본으로 바꾼 0.32 때 그 목록을 다시 봤어야 했습니다. 큰 구조 변경 뒤에는 받기만 하는 키 표를 함께 봅니다.
- 키가 "동작한다"와 "이득이 있다"는 다릅니다. `window-vsync = false`는 출력이 30% 빨라졌지만 메인 스레드가 막혔습니다. 막힘을 없애는 길(미룸 유지)은 키를 무의미하게 만들었습니다. 그래서 동작은 Ghostty대로 두고, 잰 트레이드오프를 문서에 적었습니다.
- 화면 비교 시험의 고정점에 주의합니다. 0과 255는 선형 변환에서 그대로라, 빨강/초록 시험은 바탕을 sRGB 뷰로 지우는 버그를 못 잡았습니다. 중간 밝기 시험이 잡았습니다.
- 실제 창 비교에서 알림 띠("설정을 다시 읽었습니다")가 다른 픽셀로 잡혔습니다. 다른 자리의 첫 좌표를 보고 그림을 확인한 뒤 그 영역을 빼고 견줬습니다.

## 0.39.0 (2026-11-11)

칸 끌어 옮기기와 스크롤백 메모리. 추적 이슈 [#409](https://github.com/dj258255/b-terminal/issues/409).

0.38.0 ROADMAP의 '다음'을 주 세션이 정했습니다. 범수 님 말씀("서브 에이전트로 다 해줘", "다음날까지 끝까지 계속 다해줘")에 따라 칸 끌기는 서브에이전트가 worktree에서 구현했고, 스크롤백 측정과 문서, 실제 창 확인, 검토, 병합, 릴리스는 주 세션이 맡았습니다.

- 시각(KST): 추적 이슈를 21:17에 열었고, 마지막 병합은 21:40입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#410](https://github.com/dj258255/b-terminal/issues/410) | `drag-handle` | 4~6시간 | 약 23분(이슈부터 병합 21:40까지, 에이전트 약 10분) | [#413](https://github.com/dj258255/b-terminal/pull/413) | `auto`가 칸 하나에서도 손잡이를 띄우는 것을 찾아, 하는 일 없는 손잡이를 숨기도록 바꿨습니다(Ghostty macOS와 다름). #412 위로 옮기며 문서 충돌을 풀고, 실제 창에서 칸 하나·셋 × `auto`·`never`·`always`를 기준과 견줬습니다(`auto`·`never` 다른 픽셀 0, `always`는 점 셋씩). CPU 길과 debug 빌드 위층 단언도 봤습니다 |
| [#411](https://github.com/dj258255/b-terminal/issues/411) | 스크롤백 메모리 문서 | 30분~1시간 | 약 10분(이슈부터 병합 21:27까지, 측정은 이슈 전부터) | [#412](https://github.com/dj258255/b-terminal/pull/412) | 기본 1만 줄 세 회, 10만 줄 네 회(긴 줄, 짧은 줄, 60칸 창)를 vmmap으로 쟀습니다. 한 줄은 1KB가 아니라 124칸 창에서 약 3.2KB였고, 짧은 줄도 같았고, 칸 수에 비례했습니다. 실험 E17과 설정 문서를 고쳤습니다 |

이번에 배운 것:

- 다른 프로그램의 기본 동작을 옮길 때는 그 동작이 기대는 기능이 우리에게 있는지 봅니다. Ghostty macOS가 칸 하나에도 손잡이를 띄우는 것은 끌어서 새 창을 만들기 때문인데, b-terminal에는 그 기능이 없어 손잡이가 하는 일이 없었습니다.
- 화면 비교에서 차이가 나면 자리부터 봅니다. 칸 하나 장면의 차이는 손잡이 자리가 아니라 셸이 출력을 그리는 때의 글 줄이었고, 기준끼리 견줘도 같은 차이가 났습니다.
- 메모리를 "줄 하나 약 1KB"처럼 가정으로 적은 문서는 재 보면 틀릴 수 있습니다. VT 엔진은 글자 수가 아니라 창 폭만큼 칸을 쥡니다. 짧은 줄과 좁은 창을 함께 재야 무엇에 비례하는지 알 수 있었습니다.

## 0.40.0 (2026-11-11)

창마다 글자 크기와 입력 지연. 추적 이슈 [#415](https://github.com/dj258255/b-terminal/issues/415).

0.39.0 ROADMAP의 '다음'을 살피며 Ghostty 키 묶음 동작 89개(`src/input/Binding.zig`)를 b-terminal과 견줬고, 모두 받고 있었습니다. 남은 것 가운데 하는 일이 없는 키 `window-inherit-font-size`와, 판마다 재지 않았다고 적은 입력 지연을 골랐습니다. 범수 님 말씀("서브 에이전트로 다 해줘", "다음날까지 끝까지 계속 다해줘")에 따라 하위 이슈는 서브에이전트가 각자 worktree에서 맡았고, 주 세션은 실제 창 측정, 검토, 옮기기, 병합, 릴리스를 맡았습니다.

- 시각(KST): 추적 이슈를 21:51에 열었고, 마지막 병합은 22:21입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#416](https://github.com/dj258255/b-terminal/issues/416) | 창마다 글자 크기 | 3~5시간 | 약 30분(이슈부터 병합 22:21까지, 에이전트 약 8.5분) | [#419](https://github.com/dj258255/b-terminal/pull/419) | 설정 다시 읽기가 규칙 함수(`font_size_after_reload`)를 부르지 않아 지금 창만 덮던 것을 찾아 고쳤습니다. 창 둘에서 `font-size`를 바꿔 다시 읽는 로그, 창 하나·칸 셋 화면 비교(다른 픽셀 0), 창 둘 vmmap(둘째 창 증가 같음)을 봤습니다 |
| [#417](https://github.com/dj258255/b-terminal/issues/417) | 입력 → 화면 지연 로그 | 1~2시간 | 약 18분(이슈부터 병합 22:09까지, 에이전트 약 7.5분) | [#418](https://github.com/dj258255/b-terminal/pull/418) | 실험 E18: `/bin/cat`에 `btctl send`로 GPU 길 vsync 켬·끔과 CPU 길을 세 회씩(각 123줄), zsh 한 회. 쉬는 창의 한 글자는 GPU 길 중앙값 0.8~0.9ms, CPU 길 1.8ms에 올라갔습니다 |

이번에 배운 것:

- 규칙을 순수 함수로 빼고 시험하면, 그 함수를 실제로 부르는지는 따로 봐야 합니다. #416의 다시 읽기 규칙은 시험을 통과했지만 앱에서 부르지 않았습니다. 실제 창 로그(창마다 찍히는 글꼴 줄)로 찾았습니다. 에이전트 보고의 '이렇게 동작한다'는 코드의 호출 자리로 확인합니다.
- 같은 기계의 다른 세션이 빌드하면 부하가 60까지 오릅니다. 시간 비교는 판정하지 않고 부하와 함께 적었고, 화면 비교와 메모리처럼 부하에 흔들리지 않는 값으로 판단했습니다.
- '재지 않았다'가 오래 남은 값은 재는 길부터 만들면 바로 풀립니다. 입력 지연은 로그 한 줄로 앱 몫이 1ms 안팎임을 알게 됐고, 남은 큰 몫(OS·화면)이 어디인지도 분명해졌습니다.

## 0.41.0 (2026-11-11)

칸마다 글자 크기와 출력 중 입력 지연. 추적 이슈 [#421](https://github.com/dj258255/b-terminal/issues/421).

0.40.0 ROADMAP의 '다음' 두 가지를 그대로 이었습니다. 범수 님 말씀("서브 에이전트로 다 해줘", "다음날까지 끝까지 계속 다해줘")에 따라 칸마다 글자 크기는 서브에이전트가 worktree에서 구현했고, 지연 측정과 실험 문서, 실제 창 확인, 검토, 병합, 릴리스는 주 세션이 맡았습니다.

- 시각(KST): 추적 이슈를 22:31에 열었고, 마지막 병합은 22:58입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#422](https://github.com/dj258255/b-terminal/issues/422) | 칸마다 글자 크기 | 5~8시간 | 약 27분(이슈부터 병합 22:58까지, 에이전트 약 17분) | [#425](https://github.com/dj258255/b-terminal/pull/425) | 다시 읽기 규칙을 실제로 부르는 자리를 코드로 확인했고(0.40의 교훈), #424 위로 옮기며 CHANGELOG 충돌을 풀었습니다. 실제 창에서 같은 크기일 때 칸 하나·셋(GPU)과 칸 셋(CPU)을 0.40.0과 견줬습니다(다른 픽셀 0, 흔들린 기준 사진 한 장은 다시 찍음). 창 둘 다시 읽기 로그와 footprint(창 하나 +0.1MB)도 봤습니다 |
| [#423](https://github.com/dj258255/b-terminal/issues/423) | 빠른 출력 중 입력 지연(E18 B절) | 1시간 | 약 14분(이슈부터 병합 22:45까지) | [#424](https://github.com/dj258255/b-terminal/pull/424) | 분할 창의 한 칸이 출력을 멈추지 않는 동안 다른 칸(`/bin/cat`)에 `btctl send`로 세 길을 두 판(부하 4~57, 4.6~6.6) 쟀습니다. 기본은 미룸을 기다려 중앙값 9.2~9.4ms(쉬는 창 0.9ms), vsync 끔 6.2ms, CPU 길 4.7~5.1ms였습니다 |

이번에 배운 것:

- 0.40의 교훈(규칙 함수를 실제로 부르는지 본다)을 에이전트 프롬프트에 넣었더니, 에이전트가 호출 자리(파일:줄)를 보고했습니다. 주 세션은 그 자리를 열어 확인하는 것으로 검토를 줄였습니다.
- 측정에서 크게 보이는 기다림을 없앤다고 빨라지지는 않을 수 있습니다. 출력 중 입력 지연의 대부분은 프레임 미룸이었지만, vsync를 끄니 미룸 대신 그림판 얻기에서 막혔습니다(6.2ms). 화면에 오르는 때는 줄에 선 그림판 수가 정하므로, 미룸만 건너뛰는 이슈는 열지 않았습니다.
- 화면 비교 사진은 기준 쪽도 흔들립니다. 차이가 글 줄 자리에만 있으면 기준끼리 견주고 다시 찍습니다.

## 0.42.0 (2026-11-11)

출력 중 화면 주기. 추적 이슈 [#428](https://github.com/dj258255/b-terminal/issues/428).

0.41.0 ROADMAP의 '다음'(빠른 출력 중 그림판 줄)을 이었습니다. E11·E16·E18 B절이 모두 출력 중 120Hz 화면이 16.6ms 꼴로 그림판을 가져간다고 가리켜, 그 까닭부터 가렸습니다. 범수 님 말씀("서브 에이전트로 다 해줘", "다음날까지 끝까지 계속 다해줘")에 따라 서브에이전트가 의존성 소스를 읽어 조사했고, 주 세션이 실제 창 측정, 실험 빌드, 문서, 병합, 릴리스를 맡았습니다.

- 시각(KST): 추적 이슈를 23:15에 열었고, 마지막 병합은 23:36입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#429](https://github.com/dj258255/b-terminal/issues/429) | 출력 중 화면 주기의 까닭(실험 E19) | 2~4시간 | 약 21분(이슈부터 병합 23:36까지, 에이전트 약 2.5분) | [#430](https://github.com/dj258255/b-terminal/pull/430) | "화면 한 장 간격" 로그를 넣어 앱이 120Hz(8.33ms)를 듣는 것을 봤습니다. 그림판 2·3·4장을 같은 `seq` 일로 두 회씩 견줬고(같음), 미룸을 끄는 실험 빌드로 그림판이 16.6ms마다 돌아오는 것을 확인했습니다(얻기 기다림 가운데값 15.8ms). 측정 때 저전력 모드가 켜져 있던 것을 적었습니다 |

이번에 배운 것:

- 기다림이 작다고 막힘이 없는 것은 아닙니다. 얻기가 0.3ms였던 것은 미룸이 앞서 기다려 줬기 때문이었고, 미룸을 꺼야 화면의 실제 주기가 보였습니다. 짐작하는 장치(미룸)가 있는 곳은 그 장치를 끈 실험 빌드로 재야 합니다.
- 측정 조건에는 기기 상태(저전력 모드)도 들어갑니다. 앞선 실험(E11·E16·E18)은 그것을 적지 않아 지금 다시 견줄 수 없습니다. 앞으로 그리기 시간을 잴 때는 `pmset -g`의 `lowpowermode`를 함께 적습니다.
- 소스만 읽은 조사는 가설을 좁히는 데까지입니다. 에이전트의 초안은 H1·H2를 근거가 약한 채 배제했고, 실험 빌드 하나로 확정됐습니다.

## 0.43.0 (2026-11-12)

사각 고르기. 추적 이슈 [#432](https://github.com/dj258255/b-terminal/issues/432).

0.42.0 ROADMAP의 '다음'을 이었습니다. Ghostty 1.4의 설정 키(273개)와 키 묶음 동작(91개)은 모두 갖췄고, 문서의 'Ghostty와 다른 점' 가운데 남은 기능이 사각 고르기였습니다. 범수 님 말씀("서브 에이전트로 다 해줘", "다음날까지 끝까지 계속 다해줘")에 따라 서브에이전트가 worktree에서 구현했고, 주 세션이 검토, 고침, 실제 창 화면 비교, 병합, 릴리스를 맡았습니다.

- 시각(KST): 추적 이슈를 2026-11-11 23:50에 열었고, 병합은 2026-11-12 00:15입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#433](https://github.com/dj258255/b-terminal/issues/433) | 사각 고르기 | 2~4시간 | 약 25분(이슈부터 병합 00:15까지, 에이전트 약 13분) | [#434](https://github.com/dj258255/b-terminal/pull/434) | 새 규칙 함수의 호출 자리를 diff에서 확인했습니다. 에이전트가 사각의 시작점을 `Session`에 따로 들어, 끄는 도중 출력으로 스크롤하면 사각이 옛 화면 줄로 어긋나는 것을 찾았습니다. 실패하는 시험을 먼저 쓰고, vendor 엔진에 `Selection::start_point()`를 더해 고쳤습니다. 실제 창에서 칸 셋 화면을 GPU 길 두 회, CPU 길 한 회 0.42.0과 견줬습니다(다른 픽셀 0) |

이번에 배운 것:

- 엔진이 함께 옮기는 상태(스크롤할 때의 고르기) 옆에 같은 뜻의 값을 따로 들면, 엔진이 그 값을 옮기지 않아 어긋납니다. 엔진에 읽는 길이 없으면 vendor에 읽기 메서드를 더하는 편이 덜 위험합니다.
- Ghostty와 견줄 때 '같은 기능'의 가장자리 규칙까지 봅니다. 에이전트가 사각을 시험하다가, 글자 고르기를 왼쪽으로 끌면 양 끝 칸이 빠지는 전부터 있던 버그를 찾았습니다(Ghostty는 칸 안 픽셀 자리로 가림).

## 0.44.0 (2026-11-12)

고르기 가장자리. 추적 이슈 [#436](https://github.com/dj258255/b-terminal/issues/436).

0.43.0 ROADMAP의 '다음'을 이었습니다. 0.43에서 서브에이전트가 사각 고르기를 시험하다 찾은, 전부터 있던 버그입니다. 범수 님 말씀("서브 에이전트로 다 해줘", "다음날까지 끝까지 계속 다해줘")에 따라 서브에이전트가 worktree에서 고쳤고, 주 세션이 검토, 문서 고침, 실제 창 화면 비교, 병합, 릴리스를 맡았습니다.

- 시각(KST): 추적 이슈를 00:24에 열었고, 병합은 00:43입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#437](https://github.com/dj258255/b-terminal/issues/437) | 글자 고르기를 왼쪽(위)으로 끌면 양 끝 칸이 빠짐 | 1~3시간 | 약 19분(이슈부터 병합 00:43까지, 에이전트 약 8.5분) | [#438](https://github.com/dj258255/b-terminal/pull/438) | 이슈 본문을 버그 제보 양식으로 고쳤습니다. 엔진의 낱말·줄 범위와 `is_empty`가 쪽을 보지 않는 것, 사각 같은 열이 빈 고르기로 치워지지 않는 것을 vendor 코드로 확인했습니다. 문서의 '한 칸 안에서 끌면 고른 것이 없다'를 '60% 경계를 가로지르면 그 칸 하나'로 고쳤습니다(Ghostty 규칙·시험과 맞춤). 실제 창에서 칸 셋 화면을 GPU 길 두 회, CPU 길 한 회 0.43.0과 견줬습니다(다른 픽셀 0) |

이번에 배운 것:

- 이슈에 적은 짐작('칸 절반')을 에이전트가 원본(Ghostty 60%)으로 바로잡았습니다. 프롬프트에 '소스 줄로 확인해 적기'를 넣으면 이슈 본문의 짐작이 그대로 코드가 되지 않습니다.
- 에이전트 문서는 보고와 어긋날 수 있습니다. 보고는 '한 칸 안에서도 경계를 넘으면 고른다'였지만 문서는 '고른 것이 없다'였습니다. 문서 문장을 시험 이름과 대조합니다.
- 버그는 착수 명세가 아니라 버그 제보 양식으로 엽니다(AGENTS.md). 이번에 착수 꼴로 열었다가 고쳤습니다.

## 0.45.0 (2026-11-12)

칸마다 셰이더. 추적 이슈 [#440](https://github.com/dj258255/b-terminal/issues/440).

0.44.0 ROADMAP의 '다음'을 이었습니다. 범수 님 말씀("서브 에이전트로 다 해줘", "다음날까지 끝까지 계속 다해줘")에 따라 서브에이전트가 worktree에서 구현했습니다. 주 세션은 실제 창에서 재기, 분할 창 패닉 고침, ADR-043·실험 E20, 병합, 릴리스를 맡았습니다.

- 시각(KST): 추적 이슈를 00:54에 열었고, 병합은 01:31입니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#441](https://github.com/dj258255/b-terminal/issues/441) | 사용자 셰이더를 칸마다(칸의 화면·크기·커서·초점), 덧그림은 셰이더 밖 | 4~8시간 | 약 37분(이슈부터 병합 01:31까지, 에이전트 약 16분) | [#442](https://github.com/dj258255/b-terminal/pull/442) | 실제 창에서 셰이더를 건 채 화면을 나누면 `버퍼가 화면보다 작다`로 꺼지는 것을 찾았습니다. 층 버퍼 판단을 `layer_plan`으로 빼고 모든 조합을 시험했습니다(고치기 전 실패 확인). 0.44.0과 같은 시간대에 번갈아 footprint·올리기까지 시간·화면을 쟀습니다(실험 E20). ADR-043을 쓰고 ADR-041의 상태를 고쳤습니다 |

이번에 배운 것:

- 창 없는 시험이 모두 통과해도 실제 창의 층 버퍼 길은 걸지 못했습니다. 그리는 길을 바꾼 PR은 실제 창에서 분할·셰이더 조합을 한 번은 띄웁니다.
- 부하가 높을 때(load 23) 잰 프레임 시간은 판정에 쓰지 않았습니다. 부하가 6 아래로 내려온 뒤, 기준 빌드와 새 빌드를 같은 시간대에 번갈아 쟀습니다.
- 로그 파일에 표시 줄을 덧붙이면 앱의 쓰기에 덮였습니다. 구간 표시는 따로 시각 파일에 적었습니다.
- PR 본문의 '실제 시간'은 짐작하지 않고 `gh`의 `createdAt`·`mergedAt`으로 적습니다. 이번에는 짐작으로 '약 1시간 10분'이라고 적었다가 병합 뒤 37분으로 고쳤습니다.

## 0.46.0 (2026-11-12)

칸마다 셰이더 메모리. 추적 이슈 [#444](https://github.com/dj258255/b-terminal/issues/444).

0.45.0 ROADMAP의 '다음'을 이었습니다. 범수 님 말씀("서브 에이전트로 다 해줘", "다음날까지 끝까지 계속 다해줘")에 따라 서브에이전트가 worktree에서 구현했습니다. 주 세션은 실제 창 측정(E21·E22), 원인 가르기, 문서, 병합, 릴리스를 맡았습니다.

- 시각(KST): 추적 이슈를 01:43에 열었습니다. #446은 02:15, #448은 02:58에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#445](https://github.com/dj258255/b-terminal/issues/445) | 칸마다 셰이더의 GPU 텍스처 줄이기 | 2~4시간 | 약 32분(추적 이슈부터 병합 02:15까지, 에이전트 약 10.6분) | [#446](https://github.com/dj258255/b-terminal/pull/446) | 0.45.0과 번갈아 footprint·화면·프레임 시간을 쟀습니다(E21). `gl_FragCoord`를 직접 쓰는 셰이더와 셰이더 둘 사슬을 실제 창에서 견줬습니다. 그러다 한쪽만 옛 화면인 회를 찾아 #447로 뗐습니다 |
| [#447](https://github.com/dj258255/b-terminal/issues/447) | GPU 길에서 출력 뒤 마지막 화면이 가끔 보이지 않음(0.39.0부터) | (버그) | 약 39분(이슈 02:19부터 병합 02:58까지, 에이전트 계측 약 5분 + 고침 약 3분) | [#448](https://github.com/dj258255/b-terminal/pull/448) | 셰이더·CPU 길·0.39.0·화면 합성본으로 조건을 갈랐습니다. 계측 빌드로 후보(미룬 프레임 깨우기, surface 다시 설정)를 지웠습니다. 그림판 수를 바꾼 실험 빌드로 원인을 가렸습니다(E22, ADR-044, 문제 해결 8). 셰이더 창의 메모리 증가를 재서 문서에 적었습니다 |

이번에 배운 것:

- 화면 비교에서 기준과 새 빌드가 다른 픽셀 0이어도 둘 다 같은 옛 화면일 수 있습니다. 판정에 '새 화면인가'(둘째 줄 프롬프트)를 따로 넣었습니다(scratchpad/e21/fresh.py).
- 같은 회의 footprint가 그림판 한 장만큼 튀면 그리기 쪽 이상의 신호였습니다. 메모리 표는 옛 화면 회를 빼고 적었습니다.
- 실제 창에서만 나는 버그는 먼저 에이전트에게 debug 로그를 넣게 하고, 주 세션이 실험 빌드(환경 변수 스위치)로 가린 뒤 고침을 맡겼습니다. 계측 빌드는 로그 탓에 재현 빈도가 높아졌습니다(29회 중 19회).
- 고침 하나가 앞 PR의 이득을 되돌릴 수 있습니다(셰이더 창의 셋째 그림판). 고친 빌드로 앞 PR의 장면을 다시 재 문서 수치를 고쳤습니다.

## 0.47.0 (2026-11-12)

vsync를 끈 창의 마지막 화면. 추적 이슈 [#450](https://github.com/dj258255/b-terminal/issues/450).

0.46.0 ROADMAP의 '다음'을 이었습니다. 서브에이전트가 worktree에서 진단 스위치·로그와 고침을 구현했습니다. 주 세션은 실제 창 측정(E23), 원인 가르기, 문서, 병합, 릴리스를 맡았습니다.

- 시각(KST): 추적 이슈를 03:10에 열고, #452를 04:06에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#451](https://github.com/dj258255/b-terminal/issues/451) | `window-vsync = false`에서 출력 뒤 마지막 화면이 멈춤 | (버그) | 약 56분(이슈 03:10부터 병합 04:06까지) | [#452](https://github.com/dj258255/b-terminal/pull/452) | 실험 빌드의 스위치(다시 올리기, 다시 올릴 때만 Fifo, GPU 기다리기, 통째 그리기, 빈 프레임 올리기, Fifo·Immediate × 미룸)를 실제 창에서 갈라 원인(미룸 끔으로 몰아 올림)을 찾았습니다. 앱이 넘긴 내용이 맞다는 것을 진단 로그로 확인했습니다. `seq 1 300000`으로 대가를 쟀습니다. 실험 E23, ADR-045, 문제 해결 9번을 썼습니다 |

이번에 배운 것:

- macOS `awk`로 한글 판정 글자('새'·'옛')를 `==`로 견주어 모으면 모두 같게 칩니다. 그래서 세 묶음을 모두 옛 화면으로 셌고, 한동안 '다시 올리기도, 고친 빌드도 효과 없음'으로 잘못 읽어 에이전트 프롬프트에까지 넣었습니다. 판정은 저장한 화면을 파이썬으로 다시 읽어 모읍니다(E23 '측정 사고').
- 묶음마다 결과가 뒤집히면 바깥 조건보다 집계부터 의심합니다. 화면을 직접 열어 본 것이 실마리였습니다.
- 고침에 대가(빠른 출력 이득이 사라짐)가 있을 때 택하지 않은 길(다시 올리기, 12회 중 0회)을 수치와 함께 ADR에 남겨 나중에 다시 고를 수 있게 했습니다.

## 0.48 (2026-11-12, 릴리스 없음)

한가한 창의 셋째 그림판. 추적 이슈 [#454](https://github.com/dj258255/b-terminal/issues/454). 실험만 하고 넣지 않아 코드·버전이 바뀌지 않았습니다.

- 시각(KST): 추적 이슈를 04:19에 열고, 실험 문서 #456을 04:33에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#455](https://github.com/dj258255/b-terminal/issues/455) | 한가해진 GPU 길 창에서 셋째 그림판 놓기 | 2~3시간 | 약 14분(이슈 04:19부터 병합 04:33까지, 에이전트 약 4.2분) | [#456](https://github.com/dj258255/b-terminal/pull/456) | 이슈 전에 단계별 footprint로 셋째 그림판이 셰이더와 상관없이 빠른 출력 뒤 남는 것을 보았습니다. 실험 빌드 스위치를 번갈아 재 surface 다시 설정으로는 놓이지 않음을 확인했습니다(실험 E24, ADR-044 대가 보충) |

이번에 배운 것:

- 이슈를 열기 전에 단계별(띄운 뒤·짧은 명령 뒤·빠른 출력 뒤)로 재니 앞 판의 대가 설명(셰이더 창만)이 좁았던 것이 드러났습니다.
- 결정 기준에 '효과가 없으면 잰 값과 함께 문서에 적고 닫는다'를 미리 적어 두어 첫 실험 뒤 바로 멈출 수 있었습니다.

## 0.49.0 (2026-11-12)

Ghostty 기본값 맞추기. 추적 이슈 [#458](https://github.com/dj258255/b-terminal/issues/458).

0.48 ROADMAP '다음'에 범수 님이 정할 것으로 남긴 기본값들을, 범수 님이 "계속해줘 끝까지 다해줘"(앞서 "묻지말고 전부")라 하셔서 주 세션이 추천안(Ghostty와 같게)으로 정했습니다. 서브에이전트 셋이 worktree에서 동시에 구현했습니다. 주 세션은 검토, 실제 창(탭 줄), SECURITY.md·ADR-046, 병합, 릴리스를 맡았습니다.

- 시각(KST): 추적 이슈와 하위 이슈를 06:48에 열고, #464를 07:29에 병합했습니다.
- 그대로 두기로 정한 것
  - `window-colorspace` 기본 `display-p3`: `srgb`면 GPU 길을 쓰지 못합니다.
  - 스크롤백 바이트 상한 세는 법: 우리 칸은 24바이트, Ghostty 칸은 8바이트입니다. 실제 크기로 세면 같은 10MB에서 기록이 Ghostty의 약 3분의 1이 됩니다.
  - `window-vsync = false`의 올리기: ADR-045 그대로, 까닭을 모르는 다시 올리기는 택하지 않습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#459](https://github.com/dj258255/b-terminal/issues/459) | 포인터 기본 I자 | 1시간 | 약 26분(이슈 06:48부터 병합 07:14까지, 에이전트 약 9.1분) | [#462](https://github.com/dj258255/b-terminal/pull/462) | 검토에서 X10 모드(`?9h`)만 켠 프로그램에 마우스 누름이 가지 않던 버그(#300부터)를 찾아 시험을 먼저 쓰고 고쳤습니다. 옮겨 붙은 doc 주석을 바로잡았습니다 |
| [#460](https://github.com/dj258255/b-terminal/issues/460) | 제목 글꼴을 비우면 시스템 UI 글꼴 | 1~2시간 | 약 33분(이슈 06:48부터 병합 07:21까지, 에이전트 약 8.4분) | [#463](https://github.com/dj258255/b-terminal/pull/463) | 실제 창에서 0.47.0과 번갈아 탭 줄을 찍었습니다(잘리던 제목이 끝까지 보이고 한글이 대체 글꼴 폭으로 이어짐). 첫 프레임까지 일곱 회씩 쟀습니다(첫 회 뺀 중앙값 166.9 → 167.8ms) |
| [#461](https://github.com/dj258255/b-terminal/issues/461) | `clipboard-read` 기본 `ask` | 1시간 | 약 41분(이슈 06:48부터 병합 07:29까지, 에이전트 약 8.0분) | [#464](https://github.com/dj258255/b-terminal/pull/464) | 막대 없이 답하는 세 갈래(종류 목록, 빈 클립보드, 붙여넣기 이벤트 암호)를 받아들일지 정했습니다. 커밋 보안 검토의 '기본값 완화'에 대가를 ADR-046에 적어 답했습니다. SECURITY.md를 고치고 ADR-019·030에 갱신 표시를 했습니다 |

이번에 배운 것:

- 에이전트가 새 함수를 넣을 때 바로 위 함수의 doc 주석 사이에 끼워 넣으면, 옛 주석이 새 함수로 옮겨 붙습니다(#459 `mouse_mode_on`). 검토 때 doc 주석이 어느 함수 위에 있는지 봅니다.
- 기본값을 Ghostty와 맞추며 '같은 판정을 쓰는 다른 길'을 따라가다 기존 버그를 찾았습니다(X10). 엔진이 모드를 켤 때 다른 모드를 지우는지(`remove(MOUSE_MODE)`)를 벤더 소스로 봅니다.
- PR 본문의 에이전트 시간을 짐작으로 적었다가(약 14분) 병합 뒤 작업 알림의 `duration_ms`(9.1·8.0분)로 고쳤습니다. 본문을 쓰기 전에 알림 값을 옮겨 적습니다.

## 0.50.0 (2026-11-12)

GPU 길의 sRGB와 제목 글꼴 메모리. 마일스톤 0.50(추적 이슈 없이 실험 이슈 하나와 버그 이슈 하나).

0.49.0 ROADMAP '다음'을 이었습니다. 서브에이전트가 worktree에서 실험 스위치, 정식화, 글꼴 고침을 구현했습니다. 주 세션은 이슈 전 측정, 실제 창 색·비용 측정(E26), 0.49 메모리 회귀 찾기, 문서(E26·ADR-047), 병합, 릴리스를 맡았습니다.

- 시각(KST): 실험 이슈를 07:42에 열고, #469를 08:19에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#466](https://github.com/dj258255/b-terminal/issues/466) | `window-colorspace = srgb`를 GPU 길로 | 2~3시간 | 약 37분(이슈 07:42부터 병합 08:19까지, 에이전트 약 7.3분 + 2.8분) | [#469](https://github.com/dj258255/b-terminal/pull/469) | 이슈 전에 세 길의 바탕 픽셀을 재 GPU 길이 색을 맞추지 않음(wgpu 주석과 다름)과 wgpu가 Bgra8에 Display P3만 준다는 것을 확인했습니다. 16색 장면으로 실험 길이 CPU `srgb`와 채널 1 이내임을, 같은 바이너리로 비용(시간 같음, footprint +13.6MB)을 쟀습니다. 기본은 그대로 두기로 정했습니다(실험 E26, ADR-047) |
| [#467](https://github.com/dj258255/b-terminal/issues/467) | 0.49.0 제목 글꼴 메모리 회귀 | (버그) | 약 14분(이슈 07:57부터 병합 08:11까지, 에이전트 약 5.3분) | [#468](https://github.com/dj258255/b-terminal/pull/468) | E26 비용을 재다 footprint가 E24보다 큰 것을 보고 0.47과 견주어 찾았습니다. 0.47에 `System Font`·`Menlo`를 주어 원인을 SF 글꼴로 좁히고, 표 크기로 `gvar` 7.2MB를 짚었습니다(제가 처음 짐작한 '두 번 읽기'는 캐시가 있어 틀렸습니다). 고친 빌드를 0.47·0.49와 번갈아 쟀습니다 |

이번에 배운 것:

- 라이브러리 주석("레이어 기본은 sRGB로 다룬다")을 믿지 않고 화면 픽셀로 쟀더니 반대였습니다. 색·시간처럼 잴 수 있는 것은 이슈 전에 잽니다.
- 앞 판에서 메모리를 잰 장면(E24)과 숫자가 크게 다르면 측정 조건보다 앞 판 변경부터 의심합니다. 0.49 #460은 창 없는 시험으로 시간만 재고 footprint를 재지 않아 회귀가 릴리스까지 갔습니다. 글꼴·텍스처처럼 메모리를 쥐는 변경은 실제 창 footprint를 앞 판과 견줍니다.
- 원인 짐작(두 번 읽기)은 코드를 보고 바로 버렸습니다. 이슈에 '배제한 원인'으로 남겨 에이전트가 같은 길을 가지 않게 했습니다.

## 0.51.0 (2026-11-12)

탭 줄 제목 글꼴 크기. 추적 이슈 [#473](https://github.com/dj258255/b-terminal/issues/473).

0.50.0 ROADMAP '다음 판'을 이었습니다. Ghostty 소스로 제목 글꼴 크기(`NSFont.systemFontSize`)를 확인하고, 주 세션이 Ghostty 기본에 맞춰 정했습니다(탭 줄 전체를 제목 글꼴 지표로). 서브에이전트가 worktree에서 구현했고, 주 세션은 이슈 전 실측, 높이 식 고침, 실제 창 측정, 병합, 릴리스를 맡았습니다.

- 시각(KST): 추적 이슈를 17:45에 열고, #475를 18:03에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#474](https://github.com/dj258255/b-terminal/issues/474) | 탭 줄 제목 글꼴 13pt 고정, 탭 줄이 터미널 글꼴 크기를 따르지 않게 | 2~3시간 | 약 18분(이슈 17:45부터 병합 18:03까지, 에이전트 6.4분) | [#475](https://github.com/dj258255/b-terminal/pull/475) | 이슈 전에 13pt·20pt 탭 줄을 재(52·79px) 문제를 숫자로 적었습니다. 에이전트 식(1.5배)이 SF에서 46px로 기준(52±4)을 벗어나, 시험을 먼저 쓰고 제목 글꼴일 때 5/3(51px)로 고쳤습니다. 실제 창에서 13pt·20pt·설정 다시 읽기가 모두 51px인 것과 footprint가 0.50.0과 같은 분포인 것을 확인했습니다 |

이번에 배운 것:

- 이슈에 "기본 설정 높이 52±4px, 넘으면 고치지 말고 보고"를 적어 두었더니 에이전트가 46px을 보고하고 멈췄습니다. 수치 기준을 이슈에 두면 에이전트가 판단을 넘기는 자리가 분명해집니다.
- 같은 식(셀 높이 × 1.5)이라도 글꼴이 바뀌면 결과가 바뀝니다. 비례폭 시스템 글꼴의 셀 높이(31px)는 터미널 글꼴(35px)보다 낮았습니다.
- 0.50 교훈대로 글꼴을 쥐는 변경은 footprint를 앞 판과 번갈아 쟀습니다.

## 0.52.0 (2026-11-12)

탭을 창 폭에 채우기. 추적 이슈 [#477](https://github.com/dj258255/b-terminal/issues/477).

0.51.0 ROADMAP '다음 판'을 이었습니다. 주 세션이 Ghostty(macOS 기본 탭)에 맞춰 정했고, 서브에이전트가 worktree에서 구현했습니다. 주 세션은 검토(낡은 주석 고침), 실제 창 확인, 시험 수 대조, 병합, 릴리스를 맡았습니다.

- 시각(KST): 추적 이슈를 18:15에 열고, #479를 18:31에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#478](https://github.com/dj258255/b-terminal/issues/478) | 탭 폭 상한을 없애 창 폭을 고르게 나누기 | 1시간 | 약 16분(이슈 18:15부터 병합 18:31까지, 에이전트 5.6분) | [#479](https://github.com/dj258255/b-terminal/pull/479) | 0.51 그림으로 빈 곳(탭 둘 768px)을 이슈에 적었습니다. 검토에서 '누른 자리도 글꼴을 거친다'는 낡은 주석을 고쳤고, 실제 창에서 탭 둘·셋의 빈 곳이 0px인 것을 0.51과 견줬습니다. 에이전트 시험 수(1422)가 주 세션 수와 달라 직접 다시 세어 0.51 + 새 시험 3과 맞는 것을 확인했습니다 |

이번에 배운 것:

- 고친 부분만 되돌리는 확인에서 고침이 둘이면 하나만 되돌려서는 시험이 통과할 수 있습니다(상한만 되돌리자 마지막 탭의 나머지 분배가 시험을 만족시킴). 고침마다 따로, 그리고 함께 되돌려 봅니다.
- 함수 인자를 없애는 고침은 그 인자를 설명하던 주석을 남깁니다. 검토에서 바뀐 함수 이름으로 주석을 grep합니다.
- 시험 수는 실행한 대상 수에 따라 다릅니다. 견줄 때는 같은 명령으로 직접 셉니다.

## 0.53.0 (2026-11-13)

창 폭을 바꿀 때 프롬프트 다시 그리기. 추적 이슈 [#481](https://github.com/dj258255/b-terminal/issues/481).

0.52 뒤 범수 님이 "계속해줘"라 하셔서 Ghostty main(cf3a8a6)과 다시 견줬습니다. 설정 키 213개와 키 동작 89개는 이름으로 모두 있었습니다. 그래서 최근 Ghostty 고침 넷을 살폈고, 그중 프롬프트 다시 그리기(#15199)가 실제 창에서 재현되어 이 판으로 삼았습니다. 서브에이전트가 벤더 엔진 패치와 시험을 맡았고, 주 세션은 재현 실험, 실제 창 확인(zsh·bash), 병합, 사고 처리, 릴리스를 맡았습니다.

- 시각(KST): 추적 이슈를 17:09에 열고, #485를 17:32에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#482](https://github.com/dj258255/b-terminal/issues/482) | 창 폭을 바꿀 때 프롬프트 위에 빈 줄이 쌓임 | (버그) | 약 13분(이슈 17:09부터 병합 17:22까지, 에이전트 5.9분) | [#483](https://github.com/dj258255/b-terminal/pull/483) | 이슈 전에 실제 창에서 재현했습니다(`CSI 8 t`로 프롬프트 중 폭 바꾸기, 다섯 번에 빈 줄 다섯). 고친 빌드로 같은 실험과 bash를 다시 재 빈 줄 0, bash 화면 같음을 확인했습니다. **CI clippy가 실패한 채 병합했습니다(사고)** |
| [#484](https://github.com/dj258255/b-terminal/issues/484) | CI clippy 실패(rustc 1.100의 needless_range_loop) | (버그) | 약 9분(이슈 17:23부터 병합 17:32까지) | [#485](https://github.com/dj258255/b-terminal/pull/485) | 직접 고침. CI 판(1.100)과 이 기기 판(1.96)을 견줘 원인을 툴체인으로 좁혔고, 보안 민감 파일이라 커밋 보안 검토 결과와 CI 통과를 확인하는 스크립트로 병합했습니다 |

이번에 배운 것:

- 주 세션이 검사 결과를 기다린 뒤 병합을 `;`로 이어, 실패한 검사를 보고도 #483을 병합했습니다. main이 10분 동안 빨간 상태였습니다(25b54d0). 병합은 늘 검사 종료 코드를 보는 스크립트(`ship.sh --merge-only`)로만 합니다.
- CI는 그날의 stable을 받습니다. 이 기기의 rustc(1.96)로 clippy가 통과해도 CI의 새 판에서 실패할 수 있습니다.
- 이름 대조로 틈이 없을 때는 상대 프로젝트의 최근 고침을 하나씩 실제 창에서 재 보는 것이 다음 일을 찾는 길이었습니다.

## 0.54.0 (2026-11-14)

kitty 그림 다시 보내기. 추적 이슈 [#488](https://github.com/dj258255/b-terminal/issues/488).

0.53에서 다음 판 후보로 남긴 Ghostty 고침 둘을 우리 코드에서 확인했습니다. #15184(나눠 보낸 kitty 그림 전송이 끝날 때까지 옛 그림을 둠)를 보다가, 그보다 기본적인 어긋남(같은 번호로 다시 보내도 옛 자리가 남음)을 찾아 이 판으로 삼았습니다. #15141(행을 늘리며 열을 줄이는 크기 변경)은 Ghostty 회귀 시험과 같은 장면을 벤더 엔진에 돌려 해당하지 않음을 확인했습니다. 서브에이전트가 고침과 시험을 맡았고, 주 세션은 재현(단위 탐침, 실제 창), 고친 뒤 실제 창 확인, 병합, 릴리스를 맡았습니다.

- 시각(KST): 이슈를 10:39에 열고, #489를 10:56에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#487](https://github.com/dj258255/b-terminal/issues/487) | kitty 그림을 같은 번호로 다시 보내면 옛 자리가 남아 새 그림을 보임 | (버그) | 약 17분(이슈 10:39부터 병합 10:56까지, 에이전트 보고까지 10분) | [#489](https://github.com/dj258255/b-terminal/pull/489) | 이슈 전에 단위 탐침(`a=T` 두 번 뒤 자리 2개)과 실제 창(파란 칸 둘, `a=t` 뒤 옛 자리에 파랑)으로 재현했습니다. 실패해도 옛 그림을 지울지는 명세가 때를 말하지 않아 Ghostty #15184를 따르기로 정했습니다. 고친 빌드로 같은 바이트 파일을 다시 찍어 칸 하나·그림 없음을 확인했습니다 |

이번에 배운 것:

- 상대 프로젝트의 고침을 우리 코드에 맞춰 볼 때, 그 고침이 다루는 경우(나눠 보낼 때)보다 앞선 경우(한 번에 다시 보낼 때)부터 확인하니 더 큰 어긋남이 나왔습니다.
- 같은 장면을 다른 엔진에 돌려 보는 탐침 시험은 몇 분이면 끝나고, '해당하지 않을 것으로 본다'를 '해당하지 않는다'로 바꿔 줍니다(#15141).

## 0.55.0 (2026-11-14)

Ghostty 1.4.0 기준 고르기 복사. 추적 이슈 [#492](https://github.com/dj258255/b-terminal/issues/492).

0.54 뒤 Ghostty main의 최근 고침을 보다가 #15147에서 Ghostty 1.4.0이 2026-10-27에 이미 나왔다는 것을 알았습니다. 우리는 1.4를 아직 나오지 않은 판으로 보고 1.3의 기본을 따르고 있었습니다. 1.3.1·1.4.0·main의 `Config.zig` 기본값을 기계적으로 견주니 실제로 바뀐 키는 `copy-on-select`, `scrollback-limit-bytes`, 새 키 `scrollback-compression` 셋이었습니다. 스크롤백 기본 10MB는 1.4.0의 50MB가 압축을 전제로 해 그대로 두고, `copy-on-select`를 이 판으로 삼았습니다. 서브에이전트가 구현과 시험을, 주 세션이 판 사이 기본값 비교, 결정과 ADR, 검토, 실제 창 기동 확인, 병합, 릴리스를 맡았습니다.

- 시각(KST): 이슈를 11:10에 열고, #493을 11:26에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#491](https://github.com/dj258255/b-terminal/issues/491) | 고르기 복사: macOS 앱 안 선택 클립보드와 `paste_from_selection`(⌘⇧V), 기본 `primary` | 1시간(에이전트 30분, 확인·병합 30분) | 약 16분(이슈 11:10부터 병합 11:26까지, 에이전트 4.9분) | [#493](https://github.com/dj258255/b-terminal/pull/493) | Ghostty 1.3.1·1.4.0·main 기본값을 견줘 범위를 정했고, 저장소를 앱 안으로 정했습니다(NSPasteboard는 `unsafe`). ADR-048을 썼습니다. 전체 시험을 다시 돌리고, release의 기본 덤프·키 묶음과 실제 창 기동(클립보드는 읽기만, 앞뒤 같음)을 확인했습니다 |

이번에 배운 것:

- "나온 판의 기본을 따른다"는 원칙은 어느 판이 나왔는지를 판마다 확인해야 지켜집니다. GitHub releases 목록에는 tip만 보여 놓쳤고, 태그 목록(`gh api repos/ghostty-org/ghostty/tags`)으로 1.4.0을 찾았습니다.
- 판 사이 설정 기본값은 소스 파일 셋을 정규식으로 견주면 몇 분에 나옵니다(scratchpad/e31/defs.py). 표기만 바뀐 줄이 섞이므로 사람이 한 번 걸러야 합니다.

## 0.56.0 (2026-11-14)

편집 메뉴 선택 붙여넣기. 추적 이슈 [#496](https://github.com/dj258255/b-terminal/issues/496).

0.55에서 고르기가 시스템 클립보드를 덮지 않게 기본을 바꿨는데, 선택 클립보드를 쓰는 길이 ⌘⇧V와 팔레트뿐이었습니다. Ghostty 편집 메뉴(MainMenu.xib)의 Paste Selection과 같은 자리에 "선택 붙여넣기"를 더했습니다. 서브에이전트가 구현과 시험을, 주 세션이 Ghostty 메뉴 확인, 검토, 실제 창 기동 확인, 병합, 릴리스를 맡았습니다.

- 시각(KST): 이슈를 11:36에 열고, #497을 11:46에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#495](https://github.com/dj258255/b-terminal/issues/495) | 편집 메뉴에 선택 붙여넣기(Ghostty Paste Selection) | 35분(에이전트 15분, 확인·병합 20분) | 약 10분(이슈 11:36부터 병합 11:46까지, 에이전트 3분) | [#497](https://github.com/dj258255/b-terminal/pull/497) | Ghostty 메뉴 정의에서 항목 순서를 확인했고, 메뉴 → 동작 길에서 빈 선택 클립보드가 막히지 않고 무시되는지 검토했습니다 |

이번에 배운 것:

- 기본 동작을 바꾼 판 바로 뒤에, 바뀐 동작을 찾을 수 있는 길(메뉴)을 더하는 작은 판을 두면 범수 님이 확인할 것도 메뉴에서 바로 해 볼 수 있습니다.

## 0.57.0 (2026-11-14)

분할 칸 누르기. 추적 이슈 [#499](https://github.com/dj258255/b-terminal/issues/499).

Ghostty main의 #15190(macOS 분할 칸 누르기 고침)을 살피다가, 그 앞의 동작부터 달랐다는 것을 알았습니다. Ghostty 1.4.0은 macOS와 GTK 모두 고르지 않은 칸을 누르면 칸 고르기에만 쓰고 그 누름과 짝 떼기를 터미널에 보내지 않습니다. b-terminal은 #52부터 누름을 새 칸에 그대로 넘겨, 칸을 고르려다 vim·tmux가 클릭을 받았습니다. 서브에이전트가 구현과 시험을, 주 세션이 Ghostty 원본(GTK `surface.zig`, macOS `SurfaceView_AppKit.swift`) 대조, 연결부 검토, 실제 창 기동 확인, 병합, 릴리스를 맡았습니다.

- 시각(KST): 이슈를 16:35에 열고, #503을 16:47에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#500](https://github.com/dj258255/b-terminal/issues/500) | 고르지 않은 분할 칸을 누르면 칸만 고르고 누름은 보내지 않기(Ghostty) | 1시간(에이전트 30분, 확인·병합 30분) | 약 12분(이슈 16:35부터 병합 16:47까지, 에이전트 약 4분) | [#503](https://github.com/dj258255/b-terminal/pull/503) | Ghostty 두 플랫폼의 원본을 견줘 범위를 정했고, 삼킨 누름의 떼기가 탭 줄·제목 막대에서 끝나도 다른 떼기를 잘못 삼키지 않는지 앞쪽 떼기 경로를 읽어 확인했습니다 |

이번에 배운 것:

- Ghostty의 버그 고침 PR을 읽을 때는 고친 줄만이 아니라 그 앞의 동작(여기서는 누름을 삼키는 것)이 우리와 같은지부터 봅니다. 고친 버그는 우리에게 없어도 그 바탕 동작이 다를 수 있습니다.
- 정함(0.57): 창이 뒤에 있다가 누름으로 앞에 오는 경우는 winit이 두 경우를 가를 수 없게 알리는 것으로 보여(재지 않음) GTK판 Ghostty처럼 삼킵니다.

## 0.58.0 (2026-11-14)

한글 조합 확정. 추적 이슈 [#501](https://github.com/dj258255/b-terminal/issues/501).

Ghostty의 열린 PR을 살피다가 #15137(한국어 입력기로 조합 중 포커스를 잃으면 조합 글자가 사라짐, 이슈 #15105)을 찾았습니다. 벤더 winit도 같은 길이라 b-terminal에도 같은 버그가 있다고 보고(재지 않음) 고쳤습니다. b-terminal은 분할 칸과 탭이 NSView 하나를 함께 써서, 칸·탭을 바꿀 때 조합 글자가 새 칸으로 따라가는 것도 함께 막았습니다. Ghostty에서는 아직 병합되지 않은 고침이라 ADR-049로 남겼습니다. 서브에이전트가 벤더 함수 둘과 앱 연결, 시험, 문서를, 주 세션이 ADR, ubuntu에서 쓰지 않는 함수가 되는 것 고치기, 검토, 실제 창 기동 확인, 병합, 릴리스를 맡았습니다.

- 시각(KST): 이슈를 16:39에 열고(0.57 작업 중에 미리), #505를 17:10에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#502](https://github.com/dj258255/b-terminal/issues/502) | 한글 조합 중 창·칸이 포커스를 잃으면 조합 글자가 사라지거나 다른 칸으로 감 | 적지 않음(버그 양식) | 약 14분(0.57 병합 뒤 16:57쯤 시작부터 병합 17:10까지, 에이전트 약 5분) | [#505](https://github.com/dj258255/b-terminal/pull/505) | Ghostty PR의 호출 기록과 벤더 winit 길을 견줘 원인을 정했고, objc2-app-kit의 두 메서드가 안전한 함수라 새 FFI 없이 된다는 것을 확인했습니다. ADR-049를 썼고, macOS에서만 부르는 판별 함수가 ubuntu clippy에서 쓰지 않는 함수가 되는 것을 CI 전에 잡았습니다 |

이번에 배운 것:

- `cfg(target_os = "macos")` 안에서만 부르는 함수를 더하면, 이 기기(macOS)의 clippy는 통과해도 CI(ubuntu)에서 dead_code로 실패합니다. 이런 함수는 `#[cfg(any(target_os = "macos", test))]`로 함께 막습니다.
- 정함(0.58): Ghostty에서 아직 병합되지 않은 고침이라도, 글자를 잃는 입력 버그이고 Ghostty 이슈가 받아들여져 있으면 따릅니다(ADR-049).

## 0.59.0 (2026-11-14)

암호 입력 중 붙여넣기 확인. 추적 이슈 [#507](https://github.com/dj258255/b-terminal/issues/507).

Ghostty의 열린 PR #15114(이슈 #11949)를 보고, b-terminal의 붙여넣기 확인 막대(ADR-011)도 암호 프롬프트에서 클립보드 첫 줄, 곧 암호를 드러낸다는 것을 코드로 확인했습니다. 칸의 암호 표시(#258)는 이미 있어, 요청한 때 그 표시와 다시 읽은 termios 중 하나라도 참이면 첫 줄을 숨기게 했습니다. 서브에이전트가 구현과 시험을, 주 세션이 SECURITY.md, 검토, 실제 창 기동 확인, 보안 검토 기록 확인, 병합, 릴리스를 맡았습니다.

- 시각(KST): 이슈를 17:19에 열고, #509를 17:31에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#508](https://github.com/dj258255/b-terminal/issues/508) | 암호 입력 중 붙여넣기 확인 막대는 클립보드 글을 숨긴다(Ghostty) | 40분(에이전트 15분, 확인·병합 25분) | 약 12분(이슈 17:19부터 병합 17:31까지, 에이전트 약 4분) | [#509](https://github.com/dj258255/b-terminal/pull/509) | Ghostty PR의 판단 시점(요청한 때 고정)을 확인했고, 살피는 주기(200ms) 사이의 틈을 다시 읽기로 막게 했습니다. SECURITY.md를 고치고, 코드 커밋의 보안 검토 결과를 확인한 뒤 병합했습니다 |

이번에 배운 것:

- 보안 장치(확인 막대)가 다른 방향으로 새는 길을 만들 수 있습니다. 확인 막대처럼 화면에 무언가를 보이는 장치는 그 칸의 상태(암호 입력)에 따라 무엇을 보일지 함께 봅니다.

## 0.60.0 (2026-11-15)

kitty 그림 조각 전송 끊기와 기능 질의. 추적 이슈 [#512](https://github.com/dj258255/b-terminal/issues/512).

Ghostty main의 최근 고침(#15184)을 0.54에서 따른 뒤에도, 조각(`m=1`) 전송 중에 온 지우기·질의·놓기를 자료로 이어 붙이던 점이 남아 있었습니다. Ghostty `graphics_exec.zig`처럼 지우기와 직접 전송이 아닌 전송은 받던 전송을 끊고 바꾸려던 옛 그림을 지우며, 질의·놓기는 따로 실행하게 했습니다. kitty 규약의 기능 질의(`a=q`에 `e=`, Ghostty 열린 PR #14866)는 b-terminal이 실제로 따르는 키만 답합니다. 서브에이전트가 구현과 시험을, 주 세션이 검토, 실제 창 확인, CI 고침, 병합, 릴리스를 맡았습니다.

- 시각(KST): 이슈를 01:06에 열고, #515를 01:15에 올려 01:21에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#511](https://github.com/dj258255/b-terminal/issues/511) | kitty 그림: 조각 전송 중 지우기는 전송을 끊고, 기능 질의(e=)에 답한다(Ghostty) | 50분(에이전트 25분, 확인·병합 25분) | 약 15분(이슈 01:06부터 병합 01:21까지, 에이전트 약 5분) | [#515](https://github.com/dj258255/b-terminal/pull/515) | 기능 질의에 Ghostty 목록이 아니라 우리가 따르는 키만 답하게 정했습니다. 격리한 실제 창에서 `/bin/sh` 스크립트로 질의를 보내 답 바이트를 확인했고, rustc 1.100에서 새로 걸린 clippy `byte_char_slices`를 고쳤습니다 |

이번에 배운 것:

- CI의 rustc가 1.100으로 올라 새 clippy 검사(`byte_char_slices`)가 걸렸습니다. 로컬(1.96)에서 통과해도 CI에서 실패할 수 있으니, 바이트 글자 배열(`[b'a', b'b']`) 같은 꼴은 처음부터 `*b"ab"`로 씁니다.
- 기능 질의처럼 "무엇을 지원하나"를 답하는 기능은 다른 터미널의 목록을 옮기지 않고 우리 코드가 실제로 따르는 것만 답합니다. 하지 않는 기능을 한다고 답하면 도구가 그 기능을 써서 그림이 깨집니다.

## 0.61.0 (2026-11-15)

DEC 장치 상태 질의 답. 추적 이슈 [#514](https://github.com/dj258255/b-terminal/issues/514).

`CSI ? Ps n` 가운데 `?6n`·`?996n` 말고는 답이 없어, 답을 기다리는 프로그램이 시간 초과까지 멈출 수 있었습니다. Ghostty 열린 PR #14909와 같은 고정 답과 매개변수 규칙을 넣었습니다. 0.60과 함께 서브에이전트에게 맡겨 따로 진행했고, 주 세션이 검토, 0.60.0 위로 옮기기(CHANGELOG 충돌), 실제 창 확인, 병합, 릴리스를 맡았습니다.

- 시각(KST): 이슈를 01:08에 열고, #518을 01:33에 올려 01:37에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#513](https://github.com/dj258255/b-terminal/issues/513) | DEC 장치 상태 질의(DECDSR 15·25·26·55·56·62·63·75·85)에 고정 답(Ghostty) | 45분(에이전트 20분, 확인·병합 25분) | 약 29분(이슈 01:08부터 병합 01:37까지, 에이전트 약 8분, 0.60 병합을 기다림) | [#518](https://github.com/dj258255/b-terminal/pull/518) | 답이 엔진과 겹쳐 두 번 나가지 않는지, 요청 바이트를 되돌리지 않는지 검토했습니다. 격리한 실제 창에서 질의마다 답 바이트와 `?6n`이 그대로 한 번만 나가는 것을 확인했습니다 |

이번에 배운 것:

- 두 판을 같은 바탕에서 따로 진행하면 CHANGELOG의 같은 자리에서 충돌이 납니다. 아직 올리지 않은 브랜치라 force-push 없이 새 main 위로 옮겨 풀었습니다.

## 0.62.0 (2026-11-15)

VT 엔진을 Ghostty 단위 시험과 같게. 추적 이슈 [#527](https://github.com/dj258255/b-terminal/issues/527).

esctest2를 격리 창에서 돌리려 했지만 자동 권한 검사가 막아서, Ghostty `src/terminal/Terminal.zig`의 단위 시험을 자료로 읽어 우리 엔진(`Term`을 직접 만드는 시험 틀)에 옮겨 견줬습니다. 서브에이전트 하나가 154개를 옮겨 19개 차이(원인 9가지)를 찾았고, 다른 서브에이전트가 이슈 넷으로 고쳤습니다. 주 세션은 검토, 격리한 실제 창에서 고치기 전·후 견주기, 병합, 릴리스를 맡았습니다. 같은 날 앞서 ADR 본문 정리(#521, PR #522)도 했습니다.

- 시각(KST): 이슈를 12:33에 열고, #528을 13:03에 올려 13:07에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#523](https://github.com/dj258255/b-terminal/issues/523) | 1열 칸에 넓은 글자를 찍으면 엔진 panic | — | #524~#526과 함께 약 34분(이슈 12:33부터 병합 13:07까지, 고치기 에이전트 약 23분) | [#528](https://github.com/dj258255/b-terminal/pull/528) | 칸 번호가 격자 밖으로 나가는 다른 곳이 없는지 읽었습니다 |
| [#524](https://github.com/dj258255/b-terminal/issues/524) | 지우기·삭제가 넓은 글자를 반쪽만 지움 | — | 위와 같음 | [#528](https://github.com/dj258255/b-terminal/pull/528) | Ghostty `splitCellBoundary`와 순서가 같은지 읽고, 격리한 실제 창에서 `AB가CD` 뒤 EL 0이 `AB`(전에는 `AB가`)가 되는 것을 확인했습니다 |
| [#525](https://github.com/dj258255/b-terminal/issues/525) | 줄바꿈 대기가 남음, 대기 중 EL 0 | — | 위와 같음 | [#528](https://github.com/dj258255/b-terminal/pull/528) | 꽉 찬 줄 뒤 zsh 프롬프트가 고치기 전과 같은지 견줬습니다(처음에 미리 친 글이 섞여 다르게 보여, 명령 사이를 띄워 다시 견줌) |
| [#526](https://github.com/dj258255/b-terminal/issues/526) | ED 1 첫 줄, CUU·CUD 여백, DECALN, LNM | — | 위와 같음 | [#528](https://github.com/dj258255/b-terminal/pull/528) | 지시 밖이던 vte 한 줄(LF·VT·FF를 `newline`으로)이 LNM이 꺼진 평소 동작을 바꾸지 않는지 확인했습니다 |

이번에 배운 것:

- 남이 만든 시험 묶음을 앱 창에서 돌리는 길은 권한 검사가 막을 수 있습니다. 같은 목적은 기준 구현의 단위 시험을 자료로 읽어 우리 시험 틀로 옮기는 방식으로 이룰 수 있었습니다.
- 실제 창에서 명령을 쉬지 않고 보내면, 셸이 읽기 전에 커널이 찍은 미리 친 글이 섞여 두 빌드가 다르게 보일 수 있습니다. 견줄 때는 명령 사이를 띄웁니다.
- 시험 도우미가 짝 잃은 넓은 글자 칸을 빈칸으로 그리면 차이가 가려집니다. 격자 칸의 표시를 직접 보는 검사를 함께 둡니다.

## 0.63.0 (2026-11-15)

상태 초기화를 Ghostty와 같게. 추적 이슈 [#533](https://github.com/dj258255/b-terminal/issues/533).

0.62에서 옮기지 않은 Ghostty 단위 시험을 서브에이전트 하나가 더 옮겨 견줬고(차이 82개, 원인 약 14가지), 그 가운데 사용자에게 보일 만한 상태 초기화·저장 차이 셋을 다른 서브에이전트가 고쳤습니다. 주 세션은 검토, 격리한 실제 창에서 고치기 전·후 견주기, 병합, 릴리스를 맡았습니다.

- 시각(KST): 이슈를 13:38에 열고, #534를 13:55에 올려 13:59에 병합했습니다.
- 에이전트 시간(`duration_ms`): 2차 비교 1168549ms(약 19.5분), 고치기 489231ms(약 8분)

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#530](https://github.com/dj258255/b-terminal/issues/530) | DECSTR이 모드·여백·펜·문자 집합·저장 커서를 되돌리지 않음 | — | #531·#532와 함께 약 21분(이슈 13:38부터 병합 13:59까지, 고치기 에이전트 약 8분) | [#534](https://github.com/dj258255/b-terminal/pull/534) | Ghostty `softReset`과 범위를 대조하고, 격리한 실제 창에서 선 그리기 문자 집합 뒤 DECSTR이 `qqq`를 그대로 찍는 것(전에는 `───`, 뒤 프롬프트까지 깨짐)과 삽입 모드가 꺼지는 것을 견줬습니다 |
| [#531](https://github.com/dj258255/b-terminal/issues/531) | DECSC가 원점 모드를 저장하지 않음 | — | 위와 같음 | [#534](https://github.com/dj258255/b-terminal/pull/534) | 저장이 화면마다 따로인지 읽고, 실제 창에서 복원 뒤 `CSI 1;1 H`가 둘째 줄(위 여백)에 찍히는 것을 확인했습니다 |
| [#532](https://github.com/dj258255/b-terminal/issues/532) | DECST8C가 아무 일도 하지 않음 | 에이전트 10분 | 위와 같음 | [#534](https://github.com/dj258255/b-terminal/pull/534) | 실제 창에서 탭을 모두 지운 뒤 DECST8C와 탭이 8열로 가는 것(전에는 줄 끝)을 확인했습니다 |

이번에 배운 것:

- vte가 이미 부르는 `Handler` 함수라도 `Term`에 구현이 없으면 기본 구현(아무것도 안 함)으로 조용히 넘어갑니다. 시퀀스가 "받아들여진다"는 것과 "동작한다"는 것은 따로 확인해야 합니다.
- 실제 창 견주기에서 커서를 화면 맨 위로 보내는 확인은 앞 출력을 덮습니다. 확인마다 화면을 읽습니다.

## 0.64.0 (2026-11-15)

글자 묶음 폭과 창 크기 바꾸기를 Ghostty와 같게. 추적 이슈 [#538](https://github.com/dj258255/b-terminal/issues/538).

0.63의 2차 비교에서 미룬 차이 가운데 앱 기본에서 보이는 두 가지를 서브에이전트 둘이 함께 고쳤습니다. 주 세션은 고치기 전 실제 창 측정, 검토와 Unicode 자료 대조, 두 브랜치 합치기, 고치기 전·후 견주기, 병합, 릴리스를 맡았습니다.

- 시각(KST): 이슈를 14:12에 열고, #539를 14:26에 올려 14:31에 병합했습니다.
- 에이전트 시간(`duration_ms`): 글자 묶음 448746ms(약 7.5분), 크기 바꾸기 461386ms(약 7.7분), 함께 돌림

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#536](https://github.com/dj258255/b-terminal/issues/536) | 글자 묶음 폭: 줄 끝 VS16, VS15, 잘못 붙은 표시 선택자, 피부색 | — | #537과 함께 약 19분(이슈 14:12부터 병합 14:31까지) | [#539](https://github.com/dj258255/b-terminal/pull/539) | 에이전트가 기억으로 쓴 피부색 바탕 표를 Unicode 18.0 `emoji-data.txt`와 대조해 빠진 두 글자를 더하고, 표시 선택자 판정을 `emoji-variation-sequences.txt`의 371글자로 탐침했습니다(363 일치, East Asian Wide 8글자는 두 칸 유지). 실제 창에서 줄 끝 `#️`가 다음 줄로 가는 것을 견줬습니다 |
| [#537](https://github.com/dj258255/b-terminal/issues/537) | 창 크기 바꾸기: 줄바꿈 대기, 저장 커서, 자동 줄바꿈 끔, 빈 줄 | — | 위와 같음 | [#539](https://github.com/dj258255/b-terminal/pull/539) | 고치기 전 빌드를 실제 창에서 재(모드 40으로 132→100열) "시험 틀 탓일 수 있다"던 빈 줄 차이가 실제 앱 동작임을 에이전트에게 넘겼고, 고친 뒤 `top1`이 화면에 남는 것을 견줬습니다 |

이번에 배운 것:

- 에이전트가 "기억으로 옮겼다"고 밝힌 표는 원본 자료와 대조합니다. 이번엔 새 판(E18.0)의 두 글자가 빠져 있었습니다.
- 창을 건드리지 않고 크기 바꾸기를 실제 창에서 일으키려면 모드 40을 씁니다. `ESC[?40h ESC[?3h`로 132열을 만든 뒤 `ESC[?40l`로 창 크기로 되돌리면 화면을 지우지 않고 다시 흘립니다.
- 고치기 전 실제 창 측정을 에이전트가 일하는 중에 넘기면, 조사로 남길 뻔한 항목이 고침으로 바뀝니다.

## 0.65.0 (2026-11-15)

넣어 둔 테마를 Ghostty와 같은 판으로. 추적 이슈 [#542](https://github.com/dj258255/b-terminal/issues/542).

0.64 뒤 Ghostty의 새 병합분을 보다가 #15239(iTerm2-Color-Schemes를 `release-20261109-150625-5fad1bd`로 올림)를 찾았습니다. 주 세션이 직접 했습니다(고칠 곳이 몇 군데뿐).

- 시각(KST): 이슈를 14:42에 열고, #543을 14:46에 올려 14:50에 병합했습니다.

| 이슈 | 내용 | 예상 | 실제 | PR | 주 세션이 한 것 |
|---|---|---|---|---|---|
| [#541](https://github.com/dj258255/b-terminal/issues/541) | 테마 묶음 5fad1bd, OpenCode Dark·Light | 주 세션 20분 | 약 8분(이슈 14:42부터 병합 14:50까지) | [#543](https://github.com/dj258255/b-terminal/pull/543) | GitHub 트리의 blob sha 907개와 우리 파일의 `git hash-object`를 견줘, 바뀐 것이 새 파일 둘뿐임을 확인했습니다 |

같은 날 살핀 것: ICH(`CSI @`)의 넓은 글자 경계는 Ghostty 시험 넷(13951·14115·14213·14248줄)을 탐침해 같음을 확인했습니다(0.62에서 남긴 의심 항목을 닫음). Ghostty의 다른 새 병합분(#15237·#15238·#15230)은 페이지 내부·빌드 일이라 해당 없습니다.

## 0.66.0 (2026-11-15)

다른 오픈소스 터미널과 견줘 빠진 것. 추적 이슈 [#549](https://github.com/dj258255/b-terminal/issues/549).

범수 님이 "다른 오픈소스 터미널 다 참고해서 추천하는대로"라고 하셔서 조사 에이전트 셋이 kitty·WezTerm, foot·Alacritty·Contour·Rio, Ghostty 1.4·iTerm2·Windows Terminal을 나눠 봤습니다. 이미 있는 것(XTGETTCAP, DECRQSS, OSC 52·133·5522 등)을 빼고 Ghostty에도 있는 것부터 골랐습니다. sixel·iTerm 인라인 그림·OSC 66·72·7501·vi 복사 모드·Services 메뉴는 넣지 않기로 했습니다(Ghostty에 없거나, b-studio의 일이거나, `unsafe`가 필요함).

- 시각(KST): 이슈를 21:12(#545)·21:19(#546~#549)·21:23(#550)에 열고, 한 PR [#551](https://github.com/dj258255/b-terminal/pull/551)을 21:57에 올려 22:09에 병합했습니다.
- 실제 창(격리한 HOME·TMPDIR)에서 0.65.0과 견줬습니다: DECSC 문자 집합 다섯 경우, RIS 1.5초 뒤 진행 막대 픽셀(0.65.0은 막대색 146 177 201, 새 빌드는 바탕색 52 58 67), kitty 그림 장면 여섯(그림 없는 화면은 픽셀 차이 0, 잘라내기·오프셋·글자 아래·칸 바탕 아래·같은 자리 번호로 바꾸기는 기대대로).
- 사고 둘: 주 세션이 시험용 빌드 폴더 `target-test/`(파일 14928개, 약 3.5GB)를 PR 브랜치에 커밋·push했습니다. 지우는 커밋을 더해 병합했고 squash라 main에는 들어가지 않았습니다. `.gitignore`에 `/target-*`를 더했습니다. 또 색 있는 복사 에이전트가 임시 커밋에서 커밋 훅을 우회했습니다(그 커밋은 남지 않았고 훅 설정은 그대로임을 확인).

| 이슈 | 내용 | 예상 | 실제 | 주 세션이 한 것 |
|---|---|---|---|---|
| [#545](https://github.com/dj258255/b-terminal/issues/545) | DECSC·1049가 SO·SI 문자 집합과 SS2·SS3도 저장 | — | 주 세션이 직접 | vendor 엔진에 저장 도우미를 두고, 저장 없이 복원하는 시험을 고치기 전에 실패하게 강화 |
| [#546](https://github.com/dj258255/b-terminal/issues/546) | kitty 키보드 플래그 2·4·16 | 에이전트 3~5시간 | 에이전트 약 14분(867369ms) | 키를 누른 채 칸을 바꾸면 뗌이 새 칸으로 가던 것을 고침(누름을 보낸 칸에만) |
| [#547](https://github.com/dj258255/b-terminal/issues/547) | kitty 그림 배치 p·I·x/y/w/h·X/Y·z·지우기 종류 | 에이전트 4~8시간 | 에이전트 약 20분(1227390ms) | 실제 창 장면 여섯을 0.65.0과 견줌 |
| [#548](https://github.com/dj258255/b-terminal/issues/548) | RIS가 진행 막대를 지움 | — | 주 세션이 직접 | 실제 창 픽셀로 견줌 |
| [#550](https://github.com/dj258255/b-terminal/issues/550) | 색 있는 복사(plain·vt·html·mixed, 기본 mixed) | 에이전트 3~5시간 | 에이전트 약 15분(916490ms) | HTML 링크를 http·https·mailto만 앵커로, 주소를 글자마다 이스케이프 |

보안 검토: 병합 전 변경분 전체를 따로 검토해 지적 0건(링크 거르기, VT 복사의 ESC, 그림 수치 넘침, 키 뗌의 칸 경계, 그림 질의 답).

## 0.67.0 (2026-11-16)

kitty 그림의 유니코드 자리표시·상대 배치와 애니메이션. 추적 이슈 [#555](https://github.com/dj258255/b-terminal/issues/555).

0.66의 비교 조사에서 남긴 후보를 Ghostty 원본(main 7fa26e8b675f)으로 다시 걸렀습니다. OSC 17/19·117/119는 Ghostty 앱(termio)이 "not implemented"로 기록만 하고, OSC 72(끌어다 놓기)는 libghostty-vt에만 있고 앱은 Unimplemented라 뺐습니다. CSI 14/16/18/19 t는 이미 있었습니다.

- 시각(KST): 이슈를 11-15 23:23에 열고, 한 PR [#556](https://github.com/dj258255/b-terminal/pull/556)을 11-16 00:06에 올려 00:11에 병합했습니다. 병합 뒤 보안 검토로 버그 이슈 #557을 00:20에 열고, [#558](https://github.com/dj258255/b-terminal/pull/558)을 00:28에 올려 00:33에 병합했습니다.
- 에이전트 둘이 같은 파일(`graphics.rs`·`session.rs`)을 고쳐, 만나는 자리(구조체 소유, 새 모듈, `visible_images`의 몫)를 두 프롬프트에 같은 글로 적었습니다. 충돌은 네 파일이었고 주 세션이 cherry-pick으로 합쳤습니다.
- 주 세션이 합치며 고친 것 셋: 자리표시 조각·상대 배치로 보이는 움직이는 그림의 시계(합치기 전에는 첫 프레임에 멈춤), 자리표시 묶음마다 배치를 훑던 것(커밋 보안 검토 지적), 프레임을 받을 때마다 저장량을 다시 세 프레임 수의 제곱이 되던 것(1×1 그림 프레임 6만 개 debug 약 18초, 한 번 잰 값).
- 실제 창(격리한 HOME·TMPDIR)에서 0.66.0과 견줬습니다: 그림 없는 화면은 다른 픽셀 0, 자리표시는 빈 네모 대신 4×2칸 안에 그림, 상대 배치는 부모에서 오른쪽 6칸·아래 1칸, 애니메이션은 1초 간격 세 장면이 빨강·파랑·빨강(0.66.0은 빨강만).
- 병합: `ship.sh`가 마지막 커밋의 커밋 보안 검토(문제 없음)로 바로 병합했습니다. 보안 민감 파일(ADR·SECURITY.md)이 바뀐 PR이라 0.66처럼 PR 전체 보안 검토를 병합 전에 했어야 했는데, 병합 뒤에 했습니다(아래 보안 검토 줄). 앞 커밋들도 커밋 단위 검토 기록은 모두 문제 없음이었습니다.
- 자정에 scratchpad의 오래된 파일이 지워져 `ship.sh`를 세션 기록에서 되살렸습니다.

| 이슈 | 내용 | 예상 | 실제 | 주 세션이 한 것 |
|---|---|---|---|---|
| [#553](https://github.com/dj258255/b-terminal/issues/553) | 유니코드 자리표시(`U=1`)와 상대 배치(`P Q H V`) | 에이전트 4~8시간 | 에이전트 약 26분(1584350ms) | 만나는 자리의 시계, 묶음마다 배치 훑기를 색인으로, 실제 창 |
| [#554](https://github.com/dj258255/b-terminal/issues/554) | 애니메이션(`a=f`·`a=a`·`a=c`·`d=f/F`) | 에이전트 4~8시간 | 에이전트 약 21분(1253395ms) | 프레임 저장량을 넣고 뺄 때 고쳐 두기, 실제 창 |
| [#557](https://github.com/dj258255/b-terminal/issues/557) | 그림 명령이 세션 잠금을 오래 쥐어 앱 전체가 멈춤(버그, 병합 뒤 보안 검토) | 1~2시간 | 주 세션 약 15분(이슈부터 PR까지) | 세션 시험, 읽기 고리에서 4ms마다 잠금 넘기기 |

- 병합 뒤 PR 전체 보안 검토: 서비스 거부 성격의 경로 셋을 짚었습니다. 반투명 프레임 합성(`a=c`, 2048×2048 한 번 24ms), 프레임 더하고 지우기(한 쌍 0.31ms), 프레임 10만 개 그림의 뿌리 프레임 지우기(5만 번 1.30초)이고, release 빌드에서 한 번씩 잰 값입니다. 원인은 하나였습니다. 읽기 스레드가 읽은 덩어리를 다 처리할 때까지 세션 잠금을 쥐어, 모든 탭을 그리는 앱이 그동안 기다렸습니다. #557로 열어, 그림 명령에 쓴 시간이 4ms 쌓일 때마다 화면 그리기에 잠금을 넘기게 고쳤습니다(세션 시험 debug에서 가장 오래 기다린 시간 3.9초 → 0.14초). 메모리 상한, 답에 넣는 값, 파일 전송 거절은 문제를 찾지 못했습니다.
- `ship.sh`는 이제 보안 민감 파일이 바뀐 PR을 마지막 커밋 검토 결과와 상관없이 멈추고, PR 전체 검토를 확인한 뒤에만 병합합니다. #558은 이 길로 병합했습니다.

## 0.68.0 (2026-11-16)

접근성(VoiceOver)과 kitty 알림(OSC 99) 조각 합치기. 추적 이슈 [#562](https://github.com/dj258255/b-terminal/issues/562).

0.66 비교 조사의 마지막 후보 둘입니다. CSI 11/13/15/20 t는 Ghostty 원본(main 7fa26e8b675f, `stream.zig` 2665줄)이 8·14·16·18·19·21·22/23만 처리해 뺐습니다. 이로써 비교 조사 후보는 모두 처리했습니다. Ghostty에는 2026-11-15 14:40 KST 뒤 새 병합분이 없었습니다.

- 시각(KST): 이슈를 11-16 02:05에 열고, 한 PR [#563](https://github.com/dj258255/b-terminal/pull/563)을 02:31에 올려 02:36에 병합했습니다.
- 만나는 자리를 프롬프트에서 나눴습니다: 접근성은 bt-app, 알림은 bt-term만 고치고 앱의 알림 함수는 둘 다 건드리지 않음. 충돌은 CHANGELOG 한 곳뿐이었습니다.
- 실제 창(격리한 HOME·TMPDIR)에서 0.67.0과 견줬습니다: 평소 화면은 다른 픽셀 0, 접근성 트리는 0.67.0이 제목 막대 단추와 제목뿐이고 새 빌드는 `AXTextArea`의 값에 화면 글(한글·이모지·한자)이 나오고 출력 뒤 1초 안에 반영, OSC 99 `p=?` 질의는 0.67.0이 답하지 않고 새 빌드가 `o=always:p=title,body,?`로 답함.
- 병합 전에 PR 전체 보안 검토를 따로 했습니다(0.67의 `ship.sh` 고침 뒤 첫 판). 높음·중간 0, 낮음 넷: 접근성 포커스 요청이 다른 탭의 칸 번호로 탭을 바꿀 수 있던 것을 지금 탭의 보이는 칸만 받게 고쳤고, 문서의 "PTY에 글을 넣는 길이 없다"를 "임의의 글은 없다(포커스 이동에 따른 조합 확정·포커스 보고는 생김)"로 고치고 같은 색 글자 한계를 적었습니다. 질의 답이 PTY 입력 큐를 채우는 것은 다른 질의 답과 같은 구조라 두었습니다.
- ADR 목차에 빠져 있던 ADR-050·051을 052·053과 함께 넣었습니다.

| 이슈 | 내용 | 예상 | 실제 | 주 세션이 한 것 |
|---|---|---|---|---|
| [#560](https://github.com/dj258255/b-terminal/issues/560) | 접근성: VoiceOver가 터미널 글·선택·포커스를 읽게(`accesskit`) | 에이전트 4~8시간 | 에이전트 약 20분(1177145ms) | 실제 창 접근성 트리 비교, 포커스 요청 범위 좁히기, ADR-052 |
| [#561](https://github.com/dj258255/b-terminal/issues/561) | kitty 알림(OSC 99): 조각 합치기, base64, 지원 질의 답 | 에이전트 2~4시간 | 에이전트 약 9분(542822ms) | 실제 창 질의 답 비교, ADR-053 |

## 0.69.0 (2026-11-16)

입력 경로를 Ghostty와 견줌: 붙여넣기 위험 바이트, 파일 끌어다 놓기, 레거시 키 인코딩. 추적 이슈 [#568](https://github.com/dj258255/b-terminal/issues/568).

- 시각(KST): 이슈를 11-16 02:49에 열고, 한 PR [#569](https://github.com/dj258255/b-terminal/pull/569)을 03:37에 올려 03:50에 병합했습니다.
- 만나는 자리를 프롬프트에서 나눴습니다: `input.rs`는 붙여넣기 함수와 인코더 함수로, `bt-app`은 `app.rs`(붙여넣기·끌어다 놓기)와 `keys.rs`·`keymap.rs`(키)로. 충돌은 CHANGELOG 한 곳뿐이었습니다.
- 주 세션 검토에서 둘을 바로잡았습니다. (1) 키 에이전트가 Ghostty 시험(`consumed_mods`가 빈 입력)대로 옮긴 Ctrl+Option+글자의 ESC는, Ghostty macOS 앱이 option-as-alt가 꺼져 있으면 Option을 소비된 수식 키로 넘기므로(`NSEvent+Extension.swift` 31~33줄) 기본 설정에서는 붙지 않아야 했습니다. (2) 인코더만 옮기면 Option+←가 `ESC[1;3D`가 되어, Ghostty macOS 기본 키 묶음의 글 편집 다섯 개(`Config.zig` 7408~7437줄)를 같이 옮겼습니다. 시험과 앱 사이의 층(Swift 쪽 수식 키, 기본 키 묶음)을 함께 봐야 "Ghostty와 같음"이 됩니다.
- 실제 창(격리한 HOME·TMPDIR)에서 0.68.0과 견줬습니다: 평소 화면은 다른 픽셀 0, `btctl read` 같음, 두 로그 모두 경고 없음(새 기본 키 묶음이 경고 없이 읽힘). 붙여넣기·끌어다 놓기·키 입력은 클립보드와 입력을 흉내 내지 않아 실제 창에서 하지 않았습니다.
- PR을 열기 전에 PR 전체 보안 검토를 따로 했습니다. 높음·중간 0, 낮음 셋: 끌어다 놓은 뒤 지금 창을 되돌리지 않던 것을 고쳤고, SECURITY.md에 "거르는 것은 16종뿐"을, CHANGELOG에 "줄바꿈 이름의 확인은 괄호 붙여넣기가 아닐 때"를 적었습니다.
- CI(ubuntu, 새 clippy)가 `question_mark` 하나로 처음 실패해 고쳤습니다. 병합 전에 ADR 목차 자리(054는 클립보드·붙여넣기, 055는 입력·IME)와 ADR-011 상태(일부 바뀜), overview 문구를 맞췄습니다.

| 이슈 | 내용 | 예상 | 실제 | 주 세션이 한 것 |
|---|---|---|---|---|
| [#565](https://github.com/dj258255/b-terminal/issues/565) | 붙여넣기 위험 제어 바이트 16종을 공백으로(버그) | 버그 양식이라 적지 않음 | 에이전트 P가 #566과 함께 약 9분(519968ms) | 원본 대조, ADR-054, SECURITY.md·AGENTS.md 문구 |
| [#566](https://github.com/dj258255/b-terminal/issues/566) | 파일 끌어다 놓기: 셸 이스케이프해 붙여넣기 길로 | 에이전트 1~3시간 | (위와 같은 에이전트) | 보안 검토 지적으로 지금 창 되돌리기 |
| [#567](https://github.com/dj258255/b-terminal/issues/567) | 레거시 키 인코딩을 Ghostty 시험 55개로 견주고 18개 고침 | 에이전트 3~6시간 | 에이전트 K 약 25분(1493758ms) + 바로잡기 약 4분(221450ms) | `consumed_mods`와 기본 키 묶음 찾아 바로잡게 함, 되돌리는 키 묶음 시험, ADR-055, CI clippy 고침 |

## 0.70.0 (2026-11-16)

레거시 키를 Ghostty와 견줌 2: PC식 함수 키 표 전체, Option 좌우, 기본 키 묶음 목록. 추적 이슈 [#574](https://github.com/dj258255/b-terminal/issues/574).

- 시각(KST): 이슈를 11-16 09:15에 열고, 한 PR [#575](https://github.com/dj258255/b-terminal/pull/575)을 09:42에 올려 09:47에 병합했습니다.
- 시험 몇 개가 아니라 표 전체를 견줬습니다: `function_keys.zig`를 읽는 스크립트로 34304칸의 기대값을 만들어 시험 자료로 넣었고, 처음 15344칸이 달랐습니다(원인 넷: 이름 키의 Cmd 수식, Enter·Tab·Esc의 `CSI 27` 칸, DECBKM 범위, 키패드 숫자 모드).
- 만나는 자리를 크레이트 안 파일로 나눴습니다: 인코더(`input.rs`·`keys.rs`)와 기본 묶음(`keymap.rs`). 충돌은 CHANGELOG 한 곳뿐이었습니다. 묶음 에이전트가 약속(app.rs 고치지 않음) 때문에 남긴 performable 판정 두 갈래(화면 지우기는 대체 화면에서, 다음·앞 찾기는 찾는 중이 아닐 때 키를 넘김)는 주 세션이 Ghostty `Surface.zig`와 대조해 더했습니다.
- 셸이 `CSI 27` 꼴을 받는 모양을 쟀습니다: `zsh -f`(5.9)와 `bash --norc`(3.2.57)에 PTY로 바이트를 넣으니 둘 다 기본 묶음에 이 꼴이 없고 경고음 뒤 나머지를 글자로 넣었습니다(0.69의 Ctrl+i `ESC[105;5u`도 zsh에서 `05;5u`). Ghostty와 같은 바이트라 따르고 되돌리는 설정을 적었습니다.
- 실제 창(격리한 HOME·TMPDIR)에서 0.69.0과 견줬습니다: 평소 화면은 다른 픽셀 0, `btctl read` 같음, 두 로그 모두 경고 없음. `+list-keybinds --default`는 63줄에서 82줄. 키 입력은 흉내 내지 않아 실제 창에서 하지 않았습니다.
- PR을 열기 전에 PR 전체 보안 검토를 따로 했습니다. 높음·중간 0, 낮음 둘(화면을 파일로 쓴 경로를 이스케이프하지 않고 붙여넣음: Ghostty도 같음, 키 한 번으로 화면이 파일로 남음: SECURITY.md에 적힌 한계)은 두었습니다.

| 이슈 | 내용 | 예상 | 실제 | 주 세션이 한 것 |
|---|---|---|---|---|
| [#571](https://github.com/dj258255/b-terminal/issues/571) | PC식 함수 키 표 전체를 Ghostty와 견주고 15344칸 고침 | 에이전트 2~4시간 | 에이전트 K가 #572와 함께 약 14분(827844ms) | 이슈 전 탐침 실측, 셸 실측, ADR-055·029 갱신 |
| [#572](https://github.com/dj258255/b-terminal/issues/572) | `macos-option-as-alt = left`·`right`의 좌우(버그) | 버그 양식이라 적지 않음 | (위와 같은 에이전트) | 검토 |
| [#573](https://github.com/dj258255/b-terminal/issues/573) | 기본 키 묶음을 Ghostty macOS 목록과 통째로 견주고 19개 더함 | 에이전트 1~3시간 | 에이전트 M 약 9분(549803ms) | performable 판정 두 갈래, `+list-keybinds` 실행 비교 |

## 다음

- 범수 님이 직접 해야 할 것: 키보드·마우스 확인(한글 2벌식 "안녕 하세요⏎", Claude Code의 Shift+Enter, Cmd-D·Cmd-F·Cmd-클릭·Cmd-N·Cmd-Shift-P, 메뉴, 창 닫기 단추, vim·htop, kitty icat·yazi, 투명 창), Developer ID 계정으로 `scripts/bundle-macos.sh --sign … --notarize … --dmg`
- 정함(0.49·0.50, 주 세션 추천안): `window-colorspace` 기본은 `display-p3` 그대로. 0.50부터 `srgb`도 GPU 길로 그립니다(창마다 약 13.6MB 더, ADR-047). 기본을 Ghostty처럼 `srgb`로 바꾸시려면 알려 주세요
- 범수 님이 확인할 것(0.6): 퀵 터미널을 실제 전역 단축키로(`keybind = global:ctrl+backquote=toggle_quick_terminal`), 외부 모니터로 창을 옮겼을 때 present 시간(`RUST_LOG=b_terminal=debug`)
- 범수 님이 확인할 것(0.7): `sleep 100` 중에 ⌘W → 확인 막대 → esc는 그대로·↩는 닫힘, 프롬프트에서 ⌘W는 바로 닫힘, ⌘Q도 같게. 셸에서 `cat` 중에 두 줄 글 ⌘V → 붙여넣기 확인 막대. `keybind = ctrl+a>n=new_window` 뒤 Ctrl-A·N → 새 창, Ctrl-A·X → `^A`와 `x`가 셸로. 다른 탭에서 `sleep 2; printf '\a'` → 탭 제목에 🔔
- 범수 님이 확인할 것(0.8): 메뉴 "셸 › 제목 바꾸기…"로 탭 제목 붙이고 비우기, 팔레트 "스크롤백을 파일로 열기"와 앱을 끈 뒤 `$TMPDIR/b-terminal-$(id -u)/files`가 비었는지, `focus-follows-mouse = true`로 분할 칸 위를 마우스로 오가기, 단어 두 번 눌러 고르기(`;`·`$`에서 끊김), ⌃⌘F 전체 화면
- 범수 님이 확인할 것(0.9): zsh 프롬프트에서 친 명령의 한 글자를 눌러 커서가 거기로 가는지, `adjust_selection`을 키에 묶어 고르기를 늘려 보기, 배경 그림을 투명 창(`background-opacity = 0.8`)과 함께 써 보기
- 범수 님이 확인할 것(0.10): 고른 글 ⌘C 뒤 줄 끝 공백이 없는지, `right-click-action = copy-or-paste`, 팔레트 "창 항상 위", `keybind = ctrl+a>catch_all=ignore`와 `ctrl+a>n=new_window`, `scroll-to-bottom = output`으로 긴 출력 중 위로 스크롤
- 범수 님이 확인할 것(0.11): 탭을 닫고 5초 안에 ⌘Z, `notify-on-command-finish = unfocused`로 긴 명령 뒤 다른 앱으로, `bell-features = audio`와 `bell-audio-path`로 벨 소리, ⌘F 찾기 강조 색, 고른 뒤 키를 치면 고르기가 지워지는지
- 범수 님이 확인할 것(0.12): `vt-window-resize-allowed = true`에서 `printf '\033[8;20;60t'`로 창 크기가 바뀌는지, `macos-non-native-fullscreen = true`에서 ⌃⌘F, `window-step-resize = true`에서 가장자리 끌기, 빨리 끝난 칸에서 아무 키나 눌러 닫기, `palette-generate = true`로 256색 표, tmux 안의 가족 이모지 뒤 커서 자리
- 범수 님이 확인할 것(0.13): OSC 8 링크를 Cmd를 누른 채 올려 `link-previews = osc8`·`false`에서 주소 띠가 어떻게 나오는지, `seq 1 500` 뒤 휠로 올려 스크롤 막대가 나왔다 사라지는지, `key-remap = left_ctrl=super`와 `keybind = super+t=new_tab`으로 왼쪽 Ctrl-T(오른쪽 Ctrl-C는 그대로 SIGINT), `macos-icon = custom`과 `macos-custom-icon`으로 Dock 아이콘, `quick-terminal-space-behavior = remain`에서 Space 옮기기, `macos-titlebar-style = hidden`에서 창 위쪽 끌기, `font-thicken = true`의 두께
- 범수 님이 확인할 것(0.14): `keybind = ctrl+alt+r=toggle_readonly`로 칸을 읽기 전용으로 한 뒤 타자·⌘V·`btctl send`가 막히고 제목에 🔒가 붙는지, `prompt_tab_title`의 입력 막대, `move_tab_to_new_window`, `toggle_visibility`, `toggle_background_opacity`(`background-opacity = 0.8`과 함께), `set_window_theme:light`, `keybind = ctrl+a>activate_key_table:nav`와 `nav/h=goto_split:left` 같은 키 테이블
- 범수 님이 확인할 것(0.15): `seq 1 500` 뒤 오른쪽 끝 막대를 끌고 빈 곳 누르기, `theme = light:GitHub Light Default,dark:GitHub Dark`로 시스템 설정의 모양을 바꿔 색이 바로 바뀌는지, `clipboard-write = ask`·`clipboard-read = ask`에서 막대가 뜰 때 Enter가 허용이 아닌지·`y`는 되는지, 탭 제목을 붙이고 ⌘Q 뒤 다시 켜서 되살아나는지
- 정함(0.49): `clipboard-read` 기본을 Ghostty처럼 `ask`로 바꿨습니다(#461, ADR-046). 되돌리시려면 알려 주세요
- 범수 님이 확인할 것(0.16): `theme = light:…,dark:…`에서 nvim을 켜고 시스템 모양을 바꿔 nvim 색이 따라오는지(모드 2031), `EDITOR="code -w" b-terminal +edit-config`, `b-terminal +list-keybinds`가 실제 설정의 키 묶음과 맞는지, `b-terminal +show-face --string="가😀"`가 창에 그려지는 글꼴과 같은지
- 범수 님이 확인할 것(0.17): `background-opacity = 0.7`·`background-blur = 20`으로 퀵 터미널 뒤도 흐린지, nvim·tmux 같은 프로그램이 OSC 22로 포인터를 바꾸는지(`printf '\033]22;crosshair\033\\'` 뒤 마우스를 칸 위로), `window-title-font-family`로 여러 글꼴을 써 보기
- 정함(0.49): `window-title-font-family`를 비우면 macOS에서 시스템 UI 글꼴로 그립니다(#460)
- 범수 님이 확인할 것(0.18): 셸에서 마우스를 칸 위에 두고 Shift를 누르면 I자가 되는지, `printf '\033]22;progress\007'` 뒤 `printf '\033[?1000h'`로 모양이 화살표로 돌아오는지
- 정함(0.49): 기본 포인터는 Ghostty처럼 I자, 마우스 보고 모드인 칸만 화살표(#459)
- 범수 님이 확인할 것(0.19): 셸에서 `sudo -k; sudo true`의 암호 프롬프트에서 커서가 자물쇠가 되고 창 오른쪽 위 배지가 뜨는지, 앱 메뉴 `b-terminal › 보안 키보드 입력`을 켜고 다른 앱으로 갔다 와 보기(그 앱의 단축키 도구가 다시 동작하는지), `keybind = ctrl+shift+s=toggle_secure_input`
- 범수 님이 확인할 것(0.20): vim·neovim을 켜서 색·배경 판단이 그대로인지(DA1·DA2 값과 OSC 11 답이 바뀜), tmux 안에서 `tmux info`의 `Tc`·`Smulx`·`Sync`가 잡히는지(XTGETTCAP), 창 크기를 끌어 바꾸며 모드 2048을 쓰는 프로그램이 따라오는지
- 범수 님이 확인할 것(0.21): 셸에서 단어 위 오른쪽 클릭 → 단어가 골라지고 메뉴(복사·붙여넣기·나누기·닫기·초기화·읽기 전용·제목)가 뜨는지, 메뉴 붙여넣기에서 두 줄 글이면 보호 막대, Ctrl+왼쪽 클릭, vim 안에서는 메뉴가 뜨지 않는지. `scripts/bundle-macos.sh`로 만든 앱을 Dock에 두고 폴더를 끌어놓으면 새 탭, `.sh`를 끌어놓으면 확인 막대가 뜨고 Enter가 아니라 `y`로만 실행되는지, `it's.sh`는 실행하지 않고 알림 띠만 뜨는지, 앱이 꺼진 채로 끌어놓았을 때
- 범수 님이 확인할 것(0.22): `printf '\e]8;;https://example.com\e\\\e[4mlink\e[0m\e]8;;\e\\\n'` 뒤 Cmd를 누르고 `link`에 올리면 이중 밑줄, nvim에서 커서 색이 테마와 어울리는지
- 범수 님이 확인할 것(0.23): vim·less·옛 curses 프로그램(47·1047을 쓰는 것)을 켜고 끈 뒤 셸 화면이 그대로인지
- 범수 님이 확인할 것(0.24): vim·htop·tmux에서 마우스 클릭·끌기·휠(가로 휠 방향 포함), `printf '\033[?1003h\033[?1016h'` 뒤 마우스를 움직여 픽셀 좌표가 오는지(끝은 `printf '\033[?1003l\033[?1016l'`), `printf '\033[?67h'` 뒤 `cat -v`에서 Backspace가 `^H`인지, `macos-option-as-alt = true`에서 `printf '\033[?1036l'` 뒤 Option 글자, 다른 창으로 칸을 다 가렸을 때 2033 보고, `printf '\033[?5h'`로 화면 반전 뒤 제목 막대와 창 여백
- 범수 님이 확인할 것(0.25): `printf '\033=\033[?1035l'; cat -v` 뒤 키패드 `1`·Enter·`+`가 `^[Oq`·`^[OM`·`^[Ok`인지, `printf '\033[>4;2m'; cat -v` 뒤 Ctrl+a·Ctrl+Enter·Shift+Tab이 `^[[27;5;97~`·`^[[27;5;13~`·`^[[27;2;9~`인지와 그 상태에서 한글 조합 확정이 그대로 들어가는지(끝은 `printf '\033[>4;0m'`), vim·tmux가 좌우 여백을 쓰는 화면(세로 분할 스크롤)
- 범수 님이 확인할 것(0.26): 클립보드 내용을 백업한 뒤 `printf '\033]5522;type=write:id=1\033\\\033]5522;type=wdata:mime=%s;%s\033\\\033]5522;type=wdata\033\\' "$(printf text/plain|base64)" "$(printf hello|base64)"`로 클립보드가 `hello`가 되고 알림 띠가 뜨는지, `clipboard-read = ask`에서 같은 꼴의 읽기에 확인 막대가 뜨고 `n`이면 `EPERM`인지, `printf '\033[?5522h'` 뒤 Cmd+V가 글 대신 `OK:pw=…` 패킷이 되는지(끝은 `printf '\033[?5522l'`), 화면을 나눈 칸에서 `printf '\033[?40h\033[?3h'`(끝은 `printf '\033[?40l'`)
- 범수 님이 확인할 것(0.27): zsh에서 출력 뒤 Ctrl-L로 지운 내용이 스크롤백에 남는지, bash(여러 줄 PS1)에서 창 크기를 바꿀 때 프롬프트 윗줄이 남는지, `printf 'abc'` 뒤 프롬프트가 새 줄에서 시작하는지, vim(`:set mouse=a`)에서 `printf '\033[>1s'` 뒤 Shift 클릭이 vim으로 가는지(끝은 `printf '\033[>0s'`), `printf '\033]7;\033\\'` 뒤 Cmd-T의 새 탭 폴더
- 범수 님이 확인할 것(0.28): 셸 통합 zsh에서 `echo 가나다 abc`를 치고 `가`와 `c`를 눌러 커서가 그 글자로 가는지, 두 줄 프롬프트에서 입력 앞의 프롬프트 글을 누르면 입력 맨 앞에서 멈추는지, `\` 뒤 둘째 줄(PS2)을 누를 때, bash(`shell-integration = bash`)에서 같은 것, vim 안에서 누르면 아무것도 가지 않는지, 셸 통합을 끄면 클릭 이동이 꺼지는지, bash에서 빈 Enter 뒤 탭 제목이 `_bt_status`로 바뀌지 않는지(`shell-integration-features`에 `title`)
- 범수 님이 확인할 것(0.29): IOSurface로 바뀐 뒤 창을 끌어 크기를 바꿀 때 깜빡임이나 옛 그림이 남는지, 외부 화면으로 옮길 때 색, `yes | head -c 20000000` 같은 빠른 출력에서 화면이 찢어지는지, 이상하면 `BT_E5_CGIMAGE=1`로 띄워 견주기
- 범수 님이 확인할 것(0.30): 다른 터미널에서 `BT_GPU=1 <앱 묶음>/Contents/MacOS/b-terminal`(묶음은 `scripts/bundle-macos.sh`가 `dist/b-terminal.app`에 만듦)처럼 환경 변수를 주고 띄워(`open`으로 띄우면 환경 변수가 넘어가지 않음, 로그에 `GPU 창 길:`이 보이면 GPU 길) 평소처럼 쓰다가 CPU 길과 다른 점(글자 가장자리, 팔레트·검색 막대·확인 막대, 창을 끌어 크기 바꿀 때 깜빡임)이 보이는지
- 범수 님이 확인할 것(0.31): `BT_GPU=1`로 띄워 투명 창·배경 그림·합자 글꼴에서 CPU 길과 다른 점이 보이는지, 팔레트(Cmd-Shift-P)·검색(Cmd-F)·확인 막대가 제대로 보이는지
- 범수 님이 확인할 것(0.32): 앱 묶음을 평소처럼 띄워(이제 GPU 길이 기본) 창을 열 때 빈 창이나 깜빡임이 보이는지, 밝은 테마에서 특히. 이상하면 `BT_GPU=0 <앱 묶음>/Contents/MacOS/b-terminal`로 띄워 견주기. 창 여럿·외부 화면에서 쓰다가 느려지거나 메모리가 크게 늘지 않는지
- 범수 님이 확인할 것(0.33): `custom-shader`로 Ghostty용 셰이더(예: Ghostty 저장소의 시험 셰이더나 자주 쓰는 crt·커서 꼬리 셰이더)를 걸어 화면이 Ghostty와 같은지, 다른 앱으로 갔다 오면 `custom-shader-animation = true`에서 멈췄다 다시 움직이는지, 셰이더를 건 채로 타자를 칠 때 늦게 느껴지는지
- 범수 님이 확인할 것(0.34): 앱 묶음을 띄울 때 빈 창이 보이는 시간이 줄었는지, 처음 쓰는 셰이더를 건 창이 바로 열리고 잠깐 뒤 셰이더가 켜지는지, 셰이더 파일에 일부러 오타를 넣고 `btctl reload-config`를 보내 답에 경고가 나오는지
- 범수 님이 확인할 것(0.35): 앱 묶음을 띄워 활성 상태 보기(Activity Monitor)의 메모리가 줄었는지, 화면 나누기·팔레트(Cmd-Shift-P)·찾기(Cmd-F)·스크롤 막대·kitty 그림(`kitty +kitten icat`)이 떴다 사라질 때 흔적이 남거나 깜빡이지 않는지
- 범수 님이 확인할 것(0.36): 앱 묶음을 띄워 활성 상태 보기(Activity Monitor)의 메모리가 줄었는지, 오래 쓰는 동안 탭 열고 닫기·화면 나누기·`seq 1 100000` 같은 출력 뒤에 옛 글자가 남은 칸이 보이지 않는지(칸 해시), 타자가 늦게 느껴지면 `BT_FRAME_LATENCY=2 <앱 묶음>/Contents/MacOS/b-terminal`으로 띄워 견주기, 외부 60Hz 화면이 있으면 빠른 출력에서 끊김
- 범수 님이 확인할 것(0.37): 앱 묶음을 띄워 이모지를 처음 찍을 때(`printf '😀🎉🚀\n'`) 깜빡이거나 빈 칸이 보이지 않는지, 이모지를 많이 찍는 프로그램(예: `gh`·`lazygit`의 아이콘)에서 화면이 이상하지 않은지
- 범수 님이 확인할 것(0.38): `alpha-blending = linear-corrected`와 `linear`로 평소 쓰는 테마에서 글자가 읽기 좋은지(특히 색 글자), 설정을 바꾸고 `btctl reload-config` 뒤 바로 바뀌는지, `window-vsync = false`로 새 창을 열어 타자 느낌과 빠른 출력에서 찢어짐이 보이는지(이상하면 `true`로 되돌림)
- 범수 님이 확인할 것(0.39): 칸을 둘 이상으로 나눈 뒤 칸 위쪽 가운데에 마우스를 올리면 점 셋이 보이고 포인터가 손 모양이 되는지, 눌러 다른 칸 위로 끌면 가까운 변 쪽 절반이 커서 색으로 덮이는지, 놓으면 그 자리로 칸이 옮겨지고 내용은 그대로인지, Esc·탭 줄에 놓기는 아무것도 바뀌지 않는지, vim(`set mouse=a`) 위에서 손잡이를 눌러도 vim이 클릭을 받지 않는지
- 정함(0.49, 주 세션 추천안): 스크롤백 바이트 상한은 지금처럼 셉니다(실제 크기로 세면 같은 10MB에서 Ghostty보다 기록이 약 3분의 1). 메모리를 줄이고 싶으시면 `scrollback-limit` 값을 낮추거나 세는 법을 바꾸라고 알려 주세요
- 범수 님이 확인할 것(0.40): 창 둘을 열고 한 창에서 ⌘+를 눌렀을 때 그 창만 커지는지, 커진 창에서 ⌘N으로 연 새 창이 같은 크기인지(`window-inherit-font-size = false`면 `font-size`), 설정을 다시 읽어도(`font-size`를 바꾸지 않으면) 크기가 그대로인지
- 범수 님이 확인할 것(0.41): 창을 좌우로 나눈 뒤 한 칸에서 ⌘+를 눌러 그 칸만 커지는지, 큰 칸에서 한글·커서·IME 조합·마우스 고르기·링크가 맞는지, 큰 칸에서 나눈 새 칸이 같은 크기인지, 두 칸 사이로 초점을 옮겨도 탭 줄이 흔들리지 않는지
- 범수 님이 확인할 것(0.42): 저전력 모드를 끈 채로 `RUST_LOG=b_terminal=debug,bt_gpu=info <앱 묶음>/Contents/MacOS/b-terminal`을 띄워 `seq 1 300000`을 돌리고, 로그의 "다시 그리기" 줄 누적 "미룬 프레임"이 프레임 수만큼 늘어나는지(늘면 끈 상태에서도 화면이 60Hz 꼴, 거의 늘지 않으면 저전력 모드가 원인). 그 결과를 알려 주시면 실험 E19에 적습니다
- 정함(0.66, 주 세션 추천안): `unsafe_code = "deny"`는 풀지 않습니다. 출력 중 120Hz(`CADisplayLink`)·레이어 색 공간 직접 달기·앱 사이 선택 pasteboard는 그래서 하지 않고, 지금처럼 화면이 주는 주기를 따릅니다. 바꾸시려면 알려 주세요
- 범수 님이 확인할 것(0.43): 셸에서 `ls -l`처럼 열이 있는 출력 위에서 Option을 누르면 포인터가 십자가 되는지, Option을 누른 채 끌어 한 열만 사각으로 골라지고 ⌘C(또는 놓을 때 복사)로 그 열만 복사되는지, 끄는 도중 Option을 떼면 줄을 따라가는 고르기로 바뀌는지, 한글이 사각 가장자리에 걸칠 때 글자가 통째로 복사되는지, Option 없는 고르기는 전과 같은지
- 범수 님이 확인할 것(0.44): 셸에서 `printf 'abcdef\nghijkl\nmnopqr\n'` 뒤 셋째 줄 `p`의 오른쪽 부분을 눌러 첫째 줄 `b`의 왼쪽 부분으로 끌면 `bcdef`부터 `mnop`까지 복사되는지, 같은 일을 오른쪽 아래로도 해 보기, 한 칸 안에서 조금만 움직이면 아무것도 골라지지 않는지
- 범수 님이 확인할 것(0.45): 커서 꼬리 같은 Ghostty용 셰이더를 걸고 화면을 좌우로 나눠, 칸마다 따로 그려지는지, 고른 칸의 커서에만 꼬리가 붙는지, 팔레트(Cmd-Shift-P)·찾기(Cmd-F)·닫기 확인 막대가 셰이더에 덮이지 않는지, `custom-shader-animation = true`에서 초점 없는 칸이 멈추는지
- 범수 님이 확인할 것(0.46): 앱 묶음을 띄워 명령을 친 뒤 출력이 끝난 화면이 바로 보이는지(전에는 가끔 앞 화면에 멈춤), 셰이더를 건 창에서 칸을 나누고 닫아도 화면이 맞는지
- 범수 님이 확인할 것(0.47): `window-vsync = false`로 새 창을 열어 명령을 친 뒤 출력이 끝난 화면이 바로 보이는지(전에는 거의 늘 앞 화면에 멈춤), 타자 느낌이 `true`와 다른지
- 정함(0.49, 주 세션 추천안): `window-vsync = false`는 ADR-045 그대로 둡니다. 빠른 출력(약 30%)을 되살리고 싶으시면 '마지막 프레임 뒤 다시 올리기'로 바꿀 수 있습니다(12회 중 0회로 고쳐졌지만 까닭을 모르고 출력 중 메인 스레드 막힘이 남음)
- 범수 님이 확인할 것(0.49): 셸 위 포인터가 I자인지, vim `set mouse=a` 안에서 화살표가 되고 나오면 I자로 돌아오는지, 탭 줄 제목이 시스템 글꼴로 보기 좋은지, nvim의 OSC 52 붙여넣기(`vim.g.clipboard = 'osc52'`)에서 막대가 뜨고 `y`·`n`이 맞게 동작하는지
- 범수 님이 확인할 것(0.50): `window-colorspace = srgb`로 새 창을 열어 테마 색이 덜 진하게(Ghostty와 같게) 보이는지, 외부 모니터가 있으면 그 화면에서도 색이 맞는지, 활성 상태 보기의 메모리가 0.49.0보다 줄었는지
- 정함(0.51, 주 세션 추천안): 탭 줄 제목 글꼴은 Ghostty처럼 13pt로 고정하고, 탭 줄의 높이·폭 상한도 제목 글꼴 기준입니다(#474). 터미널 글꼴을 키워도 탭 줄이 그대로입니다
- 범수 님이 확인할 것(0.51): 탭을 둘 이상 열고 ⌘+/⌘−를 눌러도 탭 줄 높이와 제목 크기가 그대로인지, 탭 제목이 창 제목 막대 글자와 비슷한 크기로 보이는지, 한글 탭 제목이 잘리지 않는지
- 정함(0.52, 주 세션 추천안): 탭 폭에 상한을 두지 않고 macOS 기본 탭처럼 창 폭을 고르게 나눕니다(#478)
- 범수 님이 확인할 것(0.52): 탭 둘·셋에서 탭 줄이 오른쪽 끝까지 차는지, 탭을 눌러 고르기가 그린 자리와 맞는지(특히 맨 오른쪽 탭의 오른쪽 끝), 넓은 창에서 탭 하나의 제목이 가운데 보기 괜찮은지
- 범수 님이 확인할 것(0.53): 평소 쓰는 zsh 프롬프트(테마)에서 창을 끌어 폭을 좁혔다 넓혔다 할 때 프롬프트 위에 빈 줄이나 옛 프롬프트가 남지 않는지, 화면을 나눈 칸의 경계를 끌 때도 같은지
- 범수 님이 확인할 것(0.54): 그림을 미리 보여 주는 도구(yazi 등)에서 파일을 옮겨 다닐 때 옛 그림이 남거나 겹치지 않는지
- 정함(0.55): `copy-on-select` 기본을 Ghostty main처럼 `primary`로 바꿨습니다. 글을 골라도 시스템 클립보드(⌘V)를 덮지 않고, 고른 글은 ⌘⇧V로 붙여넣습니다(#491, ADR-048). 전처럼 쓰시려면 `copy-on-select = clipboard`이고, 기본을 되돌리시려면 알려 주세요
- 범수 님이 확인할 것(0.55·0.56): 글을 고른 뒤 ⌘⇧V나 편집 메뉴의 "선택 붙여넣기"로 붙여넣어지는지, ⌘V(시스템 클립보드)는 고르기 전 내용 그대로인지
- 범수 님이 확인할 것(0.57): 화면을 나눈 뒤 vim(`:set mouse=a`)이나 tmux가 도는 칸을 눌러 고를 때 그 프로그램이 클릭을 받지 않는지(커서가 옮겨 가지 않음), 고른 뒤 한 번 더 누르면 평소처럼 클릭이 가는지
- 범수 님이 확인할 것(0.58): 한글 2벌식으로 `가나`를 치다가(`나`가 조합 중) ⌘Tab으로 다른 앱에 갔다 와서 Enter → `가나`가 들어가는지, 조합 중에 화면을 나눈 다른 칸이나 다른 탭을 마우스로 누르면 `나`가 옛 칸에 남고 새 칸으로 따라가지 않는지, 일본어 입력기를 쓰신다면 조합 중 다른 앱에 갔다 와도 그대로 변환되는지
- 범수 님이 확인할 것(0.59): 셸에서 `read -s x`를 띄우고(자물쇠 커서가 보이면 암호 입력) 두 줄짜리 글을 ⌘V → 확인 막대에 글 대신 줄 수와 "암호 입력 중이라 내용은 숨깁니다"만 보이는지, 평소 셸에서는 전처럼 첫 줄이 보이는지
- 범수 님이 확인할 것(0.60): `kitty +kitten icat`이나 yazi로 그림을 빠르게 바꿔 볼 때 옛 그림이 남거나 겹치지 않는지(조각 전송 중 지우기)
- 범수 님이 확인할 것(0.62): 셸에서 한글을 친 뒤 Backspace·Ctrl-W로 지울 때와 vim에서 한글을 지울 때 반쪽 글자가 남지 않는지, 화면을 아주 좁게 나눈 칸에 한글이 찍혀도 칸이 멈추지 않는지
- 범수 님이 확인할 것(0.63): `tput reset`이나 tmux·vim을 끝낸 뒤 셸 화면(선 그리기 글자, 커서 보임)이 정상인지
- 범수 님이 확인할 것(0.64): 줄 끝에 이모지가 오는 출력(예: 긴 줄 끝의 `❤️`)이 반쪽으로 그려지거나 옆 글자를 덮지 않는지, 창을 마우스로 좁혔다 넓힐 때 화면 위쪽 내용이 사라지지 않고 셸 프롬프트가 제자리에 오는지
- 범수 님이 확인할 것(0.66): helix나 codex처럼 kitty 키보드를 켜는 TUI에서 키가 맞게 들어가는지(특히 Esc·방향 키·Ctrl 조합), 한글을 조합하던 중 키를 뗄 때 이상한 글자가 들어가지 않는지, 터미널에서 색 있는 글을 고른 뒤 ⌘C로 복사해 Notes·메일에 붙이면 색이 따라오고 TextEdit의 일반 글 모드에는 글만 들어가는지, yazi로 그림을 넘겨 볼 때 위치·크기가 맞는지
- 범수 님이 확인할 것(0.67): tmux(`set -g allow-passthrough on`) 안에서 yazi나 snacks.nvim으로 그림을 미리 볼 때 그림이 보이고 빈 네모가 없는지, 스크롤하면 그림이 글자를 따라가는지, `kitty +kitten icat 움직이는.gif`가 움직이는지, 그 그림을 스크롤로 화면 밖에 두거나 창을 가렸을 때 활성 상태 보기의 CPU가 가라앉는지
- 범수 님이 확인할 것(0.68): VoiceOver(⌘F5)를 켜고 b-terminal 창에서 화면 글이 읽히는지, 칸을 나눠 옮기면 VoiceOver가 따라가는지, VoiceOver를 켠 채 새 창·퀵 터미널을 열 때 빈 창이 번쩍이지 않는지, 그리고 VoiceOver를 끈 평소에도 한글 조합 입력("안녕 하세요⏎")과 붙여넣기가 전과 같은지. `kitten notify "제목" "본문"`이나 `printf '\033]99;i=1:d=0;제목\033\\'; printf '\033]99;i=1:p=body;본문\033\\'`를 다른 탭에서 보냈을 때 데스크톱 알림이 하나만 뜨는지
- 범수 님이 확인할 것(0.69): `cat -v`를 띄운 창에 `printf 'a\025b\033c\n' | pbcopy` 뒤 ⌘V로 `a b c`가 나오는지(0.68은 `a^Ub^[c`), Finder에서 `my file (1).txt`를 끌어다 놓으면 `my\ file\ \(1\).txt`로 들어가는지와 여러 파일이 한 줄로 들어가는지, 셸에서 Option+←·→가 단어 단위로, Cmd+←·→가 줄 처음·끝으로, Cmd+Backspace가 줄 지우기로 되는지, `cat -v`에서 Ctrl+Shift+m이 `^[[109;6u`, F13이 `^[[25~`, 묶이지 않은 Cmd+b가 아무것도 안 나오는지, 그리고 평소 쓰는 셸·vim에서 Ctrl+[·Ctrl+i가 여전히 Esc·Tab으로 듣는지(안 들으면 CHANGELOG의 키 묶음으로 되돌림)
- 범수 님이 확인할 것(0.70): `cat -v`를 띄운 창에서 Shift+Enter가 `^[[27;2;13~`, Ctrl+Enter가 `^[[27;5;13~`, Shift+Esc가 `^[[27;2;27~`, Shift+Tab이 그대로 `^[[Z`인지, Ctrl+Tab·Ctrl+Shift+Tab이 탭을 넘기는지, Cmd+Enter가 전체 화면인지, 고른 글이 없을 때 Shift+←가 `^[[1;2D`이고 글을 고른 뒤에는 고르기 끝점을 옮기는지, 찾기 막대가 닫혀 있을 때 Esc가 `^[`로 가는지, 그리고 평소 쓰는 셸에서 Shift+Enter를 실수로 쳤을 때 글자가 들어오는 것이 불편한지(불편하면 `keybind = shift+enter=text:\r`)
- 정함(0.70, 주 세션 추천안): Shift+Enter·Ctrl+Enter 등은 Ghostty와 같은 `ESC[27;…~`로 보냅니다. 셸이 모르면 글자가 들어갑니다. 옛 바이트(`\r`)를 기본으로 두고 싶으시면 알려 주세요
- 다음 판 후보: Ghostty main의 그 뒤 고침을 다시 살핍니다(0.55에서 본 것: #15216 fontconfig는 Linux 길이라 해당 없음, #15147은 0.55, #15190은 0.57에서 바탕 동작을 맞춤). 0.58에서 한글 조합 중 포커스를 잃으면 확정하는 것(#502, Ghostty #15137)을 했습니다. Ghostty의 열린 PR 중 한국어 입력과 보안에 닿는 것도 판마다 봅니다(0.59: #15114 암호 입력 중 붙여넣기 미리보기). #14885(Dvorak-Qwerty ⌘ 배열에서 ⌘W가 ⌘,로)는 이 기기에 그 배열이 없어 확인할 수 없어 미뤘습니다. 편집 메뉴의 "선택 붙여넣기"는 0.56에서 더했습니다. 정함(0.66, 주 세션 추천안): `unsafe_code`는 풀지 않음, `window-colorspace` 기본은 `display-p3` 그대로, Linux GPU 길은 Linux 기기에서 잴 수 있을 때까지 그대로. 0.68로 0.66 비교 조사의 후보를 모두 처리했습니다(CSI 11/13/15/20 t는 Ghostty도 하지 않아 뺌). 0.69에서 붙여넣기·끌어다 놓기·레거시 키를 Ghostty와 견줬습니다. 0.70에서 레거시 키 표 전체와 기본 키 묶음 목록을 맞췄습니다. 0.70 뒤 후보: Ghostty 단위 시험 가운데 옮기지 않은 것(글자 묶음·보호 칸·크기 바꾸기·대체 화면·문자 집합 등 약 292개), Ghostty 새 병합분. 위의 '범수 님이 확인할 것' 결과를 알려 주시면 그것부터 고칩니다
- 큰 일로 남긴 것: Linux에서 GPU 길 재기와 기본 바꾸기, 화면이 장을 가져가는 때에 맞춰 그리기(Ghostty의 CVDisplayLink 같은 것)
