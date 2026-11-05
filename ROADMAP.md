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

## 다음

- 범수 님이 직접 쳐 보고 확인할 것: 한글 2벌식 "안녕 하세요⏎"(ADR-004 패치), vim·htop 키와 마우스, Cmd 단축키, 붙여넣기, 설정 다시 읽기 단축키
- 화면 분할, OSC 8 링크, 색 이모지, 바뀐 줄만 그리기(실험 E1의 다음), 서명·공증된 macOS 앱 묶음
