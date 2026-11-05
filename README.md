# b-terminal

> AI 코딩 에이전트와 함께 일하려고 만드는 Rust 터미널 에뮬레이터. 탭마다 에이전트가 무엇을 하고 무엇을 기다리는지 사이드바에 보이고, 기다리는 탭은 다른 탭을 보고 있어도 알려 줍니다.

아직 만드는 중입니다. 계획과 진행은 [ROADMAP.md](ROADMAP.md)에 있습니다.

- 에이전트 여러 개를 탭에 띄워 두면, 끝났는지 권한을 묻고 멈췄는지 탭을 하나씩 열어 봐야 압니다.
- b-terminal은 터미널이 그 신호(훅, OSC 알림, 프로세스 종료)를 직접 받아 사이드바에 보입니다.
- [b-studio](https://github.com/dj258255/b-studio)는 검증 관문이고 b-terminal은 그 화면입니다. b-studio 세션의 터미널을 탭으로 엽니다.

## 기여

[CONTRIBUTING.md](CONTRIBUTING.md)와 코딩 에이전트용 [AGENTS.md](AGENTS.md)를 봅니다.

## 라이선스

MIT
