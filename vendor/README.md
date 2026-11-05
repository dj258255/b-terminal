# vendor

외부 크레이트를 고쳐 쓰는 곳입니다. 고친 것마다 이유와 되돌릴 조건을 적습니다.

## winit 0.30.13

- 원본: crates.io `winit 0.30.13`(Apache-2.0, `vendor/winit/LICENSE`). 예제·문서·테스트는 뺐습니다.
- 연결: 루트 `Cargo.toml`의 `[patch.crates-io]`.
- 고친 곳: `src/platform_impl/macos/view.rs`의 `insert_text`와 `do_command_by_selector` 두 군데. `b-terminal 패치`로 찾을 수 있습니다.

### 왜

macOS 한글 IME는 "안" 조합 중에 스페이스나 Enter를 누르면 한 번의 keyDown 안에서 확정(`insertText:"안"`)을 먼저 부르고, 이어서 스페이스(`insertText:" "`)나 줄 넘김(`doCommandBySelector:insertNewline:`)을 부릅니다. 원본 winit은 확정 직후의 이 두 번째 호출을 버리고, 그 키의 `KeyboardInput`도 보내지 않습니다. 그래서 터미널에서

- "안녕 하세요"를 치면 스페이스가 사라지고,
- "ls 폴더" 뒤 Enter를 한 번 누르면 확정만 되고 명령이 실행되지 않습니다(Enter를 두 번 눌러야 함).

winit 저장소의 main(0.31 베타)에도 같은 코드가 남아 있습니다(2026-11-05 확인, `winit-appkit/src/view.rs`).

### 되돌릴 조건

winit이 같은 문제를 고친 판을 내면 패치를 지우고 그 판으로 올립니다. 그 전에 한글 2벌식으로 "안녕 하세요⏎"를 쳐서 스페이스와 Enter가 한 번에 들어가는지 확인합니다.
