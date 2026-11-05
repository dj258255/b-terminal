# 변경 기록

이 파일은 [Keep a Changelog](https://keepachangelog.com/ko/1.1.0/) 형식을 따르고, 버전은 [유의적 버전](https://semver.org/lang/ko/)을 따릅니다.

## [아직 릴리스하지 않음]

- 저장소 기여 방식(이슈 양식, PR 템플릿과 본문 검사, 에이전트 작업 규칙)을 b-studio에서 옮겨 왔습니다.
- Cargo 작업 공간(크레이트 여섯 개)과 CI(fmt·clippy·테스트)를 더했습니다.
- 터미널 코어(bt-term): PTY로 셸을 띄우고 alacritty_terminal로 화면을 유지합니다. 키·붙여넣기를 바이트로 바꾸고, 괄호 붙여넣기 안의 끝 표시를 지웁니다.
- CPU 렌더러(bt-render): 시스템 글꼴(대체 글꼴로 한글)로 셀 격자를 그립니다. 상자 선·블록·막대 글자는 직접 그려 줄 사이가 끊기지 않습니다.
