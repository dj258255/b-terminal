# vendor

외부 크레이트를 고쳐 쓰는 곳입니다. 고친 것마다 이유와 되돌릴 조건을 적습니다.

## winit 0.30.13

- 원본: crates.io `winit 0.30.13`(Apache-2.0, `vendor/winit/LICENSE`). 예제·문서·테스트는 뺐습니다.
- 연결: 루트 `Cargo.toml`의 `[patch.crates-io]`.
- 고친 곳: `src/platform_impl/macos/view.rs`의 `insert_text`와 `do_command_by_selector` 두 군데. `b-terminal 패치`로 찾을 수 있습니다.
- 고친 곳(#110): `src/platform_impl/macos/window.rs`의 `WinitWindow`에 `constrainFrameRect:toScreen:`를 더했습니다. 테두리 없는 창(퀵 터미널)은 macOS가 메뉴 막대 아래로 밀어 넣지 않고 준 자리 그대로 둡니다. 제목 막대가 있는 창은 원래대로(부모 클래스의 것)입니다. 이것이 없으면 위쪽 퀵 터미널이 화면 위쪽 바깥에서 시작하지 못해 미끄러지는 움직임이 보이지 않습니다(실제로 `y=39`에 붙박였음).
- 고친 곳(#238): `src/platform/macos.rs`의 `WindowExtMacOS::set_blur_radius(i64)`와 `src/platform_impl/macos/window_delegate.rs`의 `WindowExtMacOS` 구현 안의 같은 이름 함수. 기존 `Window::set_blur(bool)`은 반지름을 80으로 고정해 그대로 두고, 반지름을 받는 것을 더했습니다(`CGSSetWindowBackgroundBlurRadius`에 그대로 넘김). `b-terminal(#238)`로 찾을 수 있습니다. 이유: Ghostty `background-blur`는 0 이상 정수(세기)를 그대로 반지름으로 씁니다. 되돌릴 조건: winit이 반지름을 받는 공개 함수를 내면 지우고 그것을 씁니다.

### 왜

macOS 한글 IME는 "안" 조합 중에 스페이스나 Enter를 누르면 한 번의 keyDown 안에서 확정(`insertText:"안"`)을 먼저 부르고, 이어서 스페이스(`insertText:" "`)나 줄 넘김(`doCommandBySelector:insertNewline:`)을 부릅니다. 원본 winit은 확정 직후의 이 두 번째 호출을 버리고, 그 키의 `KeyboardInput`도 보내지 않습니다. 그래서 터미널에서

- "안녕 하세요"를 치면 스페이스가 사라지고,
- "ls 폴더" 뒤 Enter를 한 번 누르면 확정만 되고 명령이 실행되지 않습니다(Enter를 두 번 눌러야 함).

winit 저장소의 main(0.31 베타)에도 같은 코드가 남아 있습니다(2026-11-05 확인, `winit-appkit/src/view.rs`).

### 되돌릴 조건

winit이 같은 문제를 고친 판을 내면 패치를 지우고 그 판으로 올립니다. 그 전에 한글 2벌식으로 "안녕 하세요⏎"를 쳐서 스페이스와 Enter가 한 번에 들어가는지 확인합니다.

## softbuffer 0.4.8

- 원본: crates.io `softbuffer 0.4.8`(MIT 또는 Apache-2.0, `vendor/softbuffer/LICENSE-*`). 벤치마크는 뺐습니다.
- 연결: 루트 `Cargo.toml`의 `[patch.crates-io]`.
- 고친 곳: `src/lib.rs` 끝의 `set_native_color_space`·`set_transparent`(스위치 둘)와 `src/backends/cg.rs`의 색 공간 만들기·알파 형식 두 군데. `b-terminal(ADR-009)`·`b-terminal(#78)`로 찾을 수 있습니다. 투명 창이면 그림을 곱한 알파(ARGB)로 넘깁니다. `b-terminal(#79)`: Core Animation이 다 쓰고 돌려준 그림 버퍼를 버리지 않고 다음 프레임에 다시 씁니다(같은 크기, 최대 3개, 실험 E4). `b-terminal(#97)`: 색 공간을 주 화면이 아니라 창이 있는 화면(`NSScreenNumber`)에서 가져오고, 올릴 때 창의 NSScreen이 바뀌었으면 그 화면의 색 공간으로 바꿉니다. 기능 플래그 `CGDirectDisplay`·`CGDisplayConfiguration`을 더했습니다(`Cargo.toml`).

### 왜

macOS 백엔드는 그림을 기기 RGB 색 공간으로 Core Animation에 넘깁니다. 그러면 Core Animation이 프레임마다 화면 색 공간으로 색 맞춤 변환을 하고, 이것이 큰 창(2000×1360px)에서 present 한 번에 약 14ms였습니다(실험 E3). 화면 자신의 색 공간(`CGDisplayCopyColorSpace`)을 주면 변환이 없어 약 0.1ms입니다. 설정 `window-colorspace = srgb`면 원래대로 둡니다.

### 되돌릴 조건

softbuffer가 색 공간을 고를 수 있게 되거나, GPU 렌더러로 바꿔 softbuffer를 쓰지 않게 되면 지웁니다.

## alacritty_terminal 0.26.0

- 원본: crates.io `alacritty_terminal 0.26.0`(Apache-2.0, `vendor/alacritty_terminal/LICENSE-APACHE`). 테스트(`tests/`, 참조 그림 46MB)는 뺐습니다.
- 연결: 루트 `Cargo.toml`의 `[patch.crates-io]`. 새 크레이트는 없습니다. `Cargo.toml`에 `unicode-segmentation = "1"`을 직접 의존성으로 더했고(잠금 파일에 이미 1.13.3이 있었음) `[[test]]`를 지웠습니다.
- 고친 곳(#180, ADR-014): `src/term/mod.rs` 한 파일, 더한 줄 125·지운 줄 2. `b-terminal 패치(#180)`로 찾을 수 있습니다.
  - `TermMode::GRAPHEME_CLUSTER`(DECSET 2027)와 `Config::grapheme_cluster`(처음 값, 기본 켬). 모드 켜기·끄기·DECRQM은 vte가 이름 없는 모드로 넘기는 2027을 `set_private_mode`·`unset_private_mode`·`report_private_mode`가 받게 했습니다.
  - `Handler::input`이 글자를 찍기 전에 `join_grapheme`을 부릅니다. 방금 찍은 칸(`grapheme_anchor`, 커서가 그 자리 그대로일 때만)의 글과 새 글자를 합쳐 `unicode-segmentation`이 한 묶음으로 보면 새 칸 없이 그 칸의 결합 문자로 잇습니다. 좁은 글자에 U+FE0F가 붙어 두 칸(`unicode-width`의 문자열 폭)이 되면 뒤 칸을 채웁니다. 지역 표시 글자(국기)는 한 글자로도 두 칸으로 셉니다.
  - 칸 내용을 바꾸는 `scroll_*_relative`·`insert_blank`·`erase_chars`·`delete_chars`·`clear_line`·`clear_screen`·`swap_alt`·`reset_state`는 `grapheme_anchor`를 지웁니다.

- 더한 곳(#259, ADR-021): 같은 파일에 `Term::scroll_region()`(스크롤 영역 줄 범위를 읽는 getter) 여섯 줄. `b-terminal 패치(#259)`로 찾을 수 있습니다. DECRQCRA(체크섬 보고)가 원점 모드의 위 여백을 읽으려고 더했고, 필드는 그대로 비공개입니다. 되돌릴 조건: alacritty_terminal이 같은 getter를 내놓으면 지웁니다.

- 고친 곳(#264, ADR-022): 같은 파일의 `identify_terminal`(DA1·DA2에 답하던 것)을 답하지 않게 했고, `report_private_mode`가 모드 2048에는 답하지 않게 한 줄을 더했습니다. `b-terminal(#264)`로 찾을 수 있습니다. 이유: DA1·DA2는 Ghostty 값(`?62;22;52c`·`>1;10;0c`)으로, 모드 2048은 켜짐·꺼짐(1·2)으로 bt-term 스캐너가 답합니다. 엔진이 옛 값(`?6c`, `>0;버전;1c`, 모르는 모드 0)으로 또 답하면 PTY에 답이 둘 나갑니다. 되돌릴 조건: alacritty_terminal이 장치 속성 답과 모르는 모드 보고를 이벤트로 바꿔 밖에서 정할 수 있게 하면 지웁니다.
- 고친 곳(#303): `report_private_mode`가 모드 2033(가시성 보고)에도 답하지 않게 한 줄을 더했습니다. `b-terminal(#303)`로 찾을 수 있습니다. 이유와 되돌릴 조건은 위 모드 2048과 같습니다.

- 더한 곳(#272, ADR-024): `src/event.rs`의 `Event::ColorRequest`에 두 번째 필드 `Option<Rgb>`(프로그램이 OSC 4·10·11·12·21로 바꾼 색, 없으면 `None`)를 더하고, `src/term/mod.rs`의 `dynamic_color_sequence`가 `self.colors[index]`를 실어 보냅니다. `b-terminal(#272)`로 찾을 수 있습니다. 이유: 이 사건은 읽기 스레드가 `Term`을 잠근 채 오므로 받는 쪽(`Listener`)이 `Term::colors()`를 읽을 수 없어 질의가 바꾼 색이 아니라 띄울 때의 테마 색으로 답했습니다. 되돌릴 조건: alacritty_terminal이 색 질의 사건에 지금 색을 실어 주거나 받는 쪽이 엔진을 읽을 수 있게 하면 지웁니다.
- 더한 곳(#286): 같은 `Event::ColorRequest`에 세 번째 필드 `Option<Rgb>`를 더합니다(`event.rs`, `term/mod.rs`의 `dynamic_color_sequence`). 커서 색(OSC 12) 질의일 때만 프로그램이 바꾼 글자색을 싣고 나머지는 `None`입니다. `b-terminal(#286)`로 찾을 수 있습니다. 이유: Ghostty의 커서 색은 정하지 않았으면 지금 글자색(`cursor orelse foreground`)인데 받는 쪽은 엔진을 읽을 수 없어 테마 글자색으로 답했습니다. 되돌릴 조건은 위와 같습니다.
- 더한 곳(#295): `src/term/mod.rs`에 `Handler`의 `insert_columns`·`delete_columns`(DECIC·DECDC, 스크롤 영역 줄마다 `insert_blank`·`delete_chars`)·`back_index`·`forward_index`(DECBI·DECFI)·`set_single_shift`(SS2·SS3)와 도우미 `for_each_region_row`, 필드 `single_shift`(다음 한 글자에만 쓰는 문자 집합, `input`이 쓰고 `reset_state`가 지움)를 더했습니다. `b-terminal(#295)`로 찾을 수 있습니다. 이유: vte가 버리던 열 삽입·삭제와 문자 집합 바꿈을 Ghostty처럼 받으려고. 되돌릴 조건: upstream이 같은 기능을 내놓으면 지웁니다. 한계: 칸 이동은 `insert_blank`·`delete_chars`를 그대로 써서 넓은 글자가 가운데에서 잘리는 경우를 Ghostty처럼 따로 다루지 않습니다.

### 왜

alacritty_terminal은 글자마다 `unicode-width`로 폭을 세고, 폭 0인 글자만 앞 칸에 붙입니다. 그래서 ZWJ 가족은 6칸, 피부색은 4칸을 차지해 Ghostty(`grapheme-width-method = unicode`)와 어긋나고 프로그램이 mode 2027로 묶음 폭을 요청해도 받지 못합니다. 엔진 바깥(바이트를 바꾸는 방식)에서는 줄바꿈·커서·지우기와 섞여 안전하게 할 수 없었습니다.

### 되돌릴 조건

alacritty_terminal이 글자 묶음 폭(mode 2027)을 지원하는 판을 내면 패치를 지우고 그 판으로 올립니다. 올릴 때는 `tests/grapheme.rs`(bt-term)가 그대로 통과해야 합니다. 새 판으로 올릴 때마다 이 패치를 새 `mod.rs`에 다시 얹어야 하므로, 판이 바뀌면 `diff`로 먼저 확인합니다.


## winit: 보안 키보드 입력(#258)

- 고친 곳: `src/platform/macos.rs` 끝의 `pub fn set_secure_event_input(bool) -> bool`, `src/platform_impl/macos/mod.rs`의 같은 이름 함수(Carbon `EnableSecureEventInput`·`DisableSecureEventInput`을 부르고 `noErr`면 참), `src/platform_impl/macos/ffi.rs`의 Carbon 선언 두 줄. `b-terminal(#258)`로 찾을 수 있습니다. 창에 묶이지 않는 프로세스 전역 호출이라 창 메서드가 아니라 자유 함수입니다. 이유: Ghostty `macos-auto-secure-input`·메뉴 `Secure Keyboard Entry`·`toggle_secure_input`. 작업 공간이 `unsafe_code = "deny"`라 앱 코드는 FFI를 못 부릅니다. 되돌릴 조건: winit이 보안 이벤트 입력 API를 내면 지우고 그것을 씁니다.

## winit: 오른쪽 클릭 메뉴(#278)

- 고친 곳: `src/platform/macos.rs` 끝의 `pub fn popup_context_menu(*mut c_void) -> bool`과 `src/platform_impl/macos/mod.rs`의 같은 이름 함수(`NSMenu`를 `NSEvent.mouseLocation` 자리에 띄우고 메뉴가 닫힐 때까지 돌아오지 않음. 주 스레드가 아니거나 null이면 거짓). `b-terminal(#278)`로 찾을 수 있습니다. 이유: Ghostty `right-click-action = context-menu`. muda의 `show_context_menu_for_nsview`는 unsafe라 작업 공간 `unsafe_code = "deny"`에서 앱 코드가 부를 수 없습니다. 앱은 muda `ContextMenu::ns_menu()`(안전한 함수)로 얻은 포인터를 이 함수에 넘깁니다. 포인터가 살아 있는 `NSMenu`여야 한다는 약속은 앱이 지킵니다(메뉴를 띄우는 호출이 끝날 때까지 muda `Menu`를 쥠). 되돌릴 조건: muda가 안전한 띄우기 함수를 내거나 winit이 메뉴 띄우기 API를 내면 지우고 그것을 씁니다.

## vte 0.15.0: 윗줄 SGR 53·55(#277)

- 원본: crates.io `vte 0.15.0`(Apache-2.0 또는 MIT, `vendor/vte/LICENSE-*`). 예제·문서·테스트 디렉터리는 뺐습니다. 원본 그대로 한 커밋, 패치는 다음 커밋입니다.
- 연결: 루트 `Cargo.toml`의 `[patch.crates-io]`. 새 크레이트는 없고, 잠금 파일에서 `vte`의 `source`·`checksum` 줄만 빠집니다. `alacritty_terminal`이 쓰는 `vte`가 이 사본입니다.
- 고친 곳: `src/ansi.rs` 한 파일. `Attr`에 `Overline`·`CancelOverline`을 끝에 더하고, `attrs_from_sgr_parameters`에 `[53]`·`[55]` 두 줄을 더했습니다. `b-terminal(#277)`로 찾을 수 있습니다.
- `alacritty_terminal` 쪽(같은 이슈): `src/term/cell.rs`의 `Flags::OVERLINE`(마지막 빈 비트 `0b1000_0000_0000_0000`, 당시 `u16`이 가득 참)과 `is_empty` 표시 목록, `src/term/mod.rs`의 `terminal_attribute`가 켜고 끕니다(Reset은 `flags = empty()`가 지웁니다). `b-terminal(#277)`로 찾을 수 있습니다.
- `alacritty_terminal` 쪽(#287): `src/term/cell.rs`의 `Flags`를 `u16`에서 `u32`로 넓히고 `BLINK`(`1 << 16`, 깜박임 SGR 5·6)를 더했으며, `src/term/mod.rs`의 `terminal_attribute`가 `BlinkSlow`·`BlinkFast`로 켜고 `CancelBlink`(25)로 끕니다(Reset은 `flags = empty()`가 지웁니다). `Cell`은 24바이트 그대로입니다(64비트, 한 번 잼). 이유: DECRQSS `m` 답에 5를 넣으려고 커서 템플릿이 깜박임을 기억해야 합니다. 그리지는 않습니다(Ghostty도 글자 깜박임은 그리지 않음). `b-terminal(#287)`로 찾을 수 있습니다. 되돌릴 조건은 위 #277과 같고, upstream이 `Flags`를 넓히거나 깜박임을 기억하면 지웁니다.
- `vte` 쪽(#295): `src/ansi.rs`의 `Handler`에 `insert_columns`·`delete_columns`·`back_index`·`forward_index`·`set_single_shift`(기본 구현은 빈 함수)를 더하고, `csi_dispatch`에 `W`(CTC)·`j`(HPB)·`k`(VPB)·`'}`(DECIC)·`'~`(DECDC), `esc_dispatch`에 `A`(영국)·`6`·`9`·`N`·`O`·`n`·`o`와 받기만 하는 `~`·`}`·`|`(LS1R~LS3R)를 더하고, `StandardCharset::British`(`#` → `£`)를 끝에 더했습니다. `b-terminal(#295)`로 찾을 수 있습니다. 해석·엔진 테스트는 `crates/bt-term/tests/column_ops.rs`에 있습니다. 되돌릴 조건은 위 #277과 같습니다.
- vte(#294): `src/ansi.rs`에 `ProtectedMode`와 `Handler`의 `selective_clear_screen`·`selective_clear_line`·`set_protected_mode`(기본 구현은 빈 함수)를 더하고, CSI match에 `('J', [b'?'])`·`('K', [b'?'])`(DECSED·DECSEL)·`('q', [b'"'])`(DECSCA), ESC match에 `V`·`W`(SPA·EPA)를 더했습니다. `b-terminal(#294)`로 찾을 수 있습니다. 해석 테스트는 `crates/bt-term/tests/protected.rs`에 있습니다. 되돌릴 조건: upstream vte가 이 시퀀스를 해석하는 판을 내면 지웁니다.
- `alacritty_terminal` 쪽(#294): `src/term/cell.rs`의 `Flags::PROTECTED`(`1 << 17`)와 `is_empty` 목록, `src/term/mod.rs`의 `Term.protected_iso`(마지막 보호 방식이 ISO인가)·`reset_protected()`·`clear_unprotected`·`erase_display_keeping_protected`·`erase_line`과 `clear_screen`·`clear_line`·`erase_chars`의 보호 칸 건너뛰기, SGR 0이 `PROTECTED`를 지우지 않게 한 줄, `reset_state`의 `protected_iso` 끄기. 보호는 커서 템플릿의 표시라 DECSC·DECRC가 따라 저장합니다. `Cell`은 24바이트 그대로입니다. `b-terminal(#294)`로 찾을 수 있습니다. 되돌릴 조건은 위 #277과 같습니다.
- 고친 곳(#296): `src/ansi.rs`의 `NamedPrivateMode`에 `SwapScreenLegacy`(47)·`KeypadApplication`(66)·`SwapScreenAndClear`(1047)·`SaveRestoreCursor`(1048)를 더하고 `PrivateMode::new`가 네 번호를 이 이름으로 읽습니다. `alacritty_terminal` 쪽: `src/term/mod.rs`의 `TermMode`에 `SYNC_UPDATE`(`1 << 24`)·`SAVE_CURSOR_MODE`(`1 << 25`), `swap_alt`를 둘로 갈라 지우지 않고 바꾸는 `switch_screen_legacy`(47·1047, 커서를 새 화면에 옮김, 1047은 나갈 때 대체 화면을 지움)를 더하고, `set_private_mode`·`unset_private_mode`·`report_private_mode`가 새 모드를 다루며(2026은 늘 꺼짐이던 DECRQM 답을 `SYNC_UPDATE` 표시로, 2031은 모드 2048처럼 답하지 않음 — bt-term 스캐너가 답합니다) `b-terminal(#296)`로 찾을 수 있습니다. 이유: 이 모드들이 모르는 모드라 47이 화면을 바꾸지 않았고 DECRQM이 0이었습니다. 되돌릴 조건은 위 #277과 같고, upstream이 같은 모드를 다루면 지웁니다. 해석 테스트는 `crates/bt-term/tests/emulator.rs`·`session.rs`에 있습니다.
- `alacritty_terminal` 쪽(#301): `src/term/mod.rs`의 `TermMode`에 `BACKARROW_KEY`(`1 << 36`, DECBKM 67)·`IGNORE_KEYPAD_NUMLOCK`(`1 << 37`, 1035)·`ALT_ESC_PREFIX`(`1 << 38`, 1036)·`ALT_SENDS_ESCAPE`(`1 << 39`, 1039)를 더하고, `Default`가 1035·1036을 켭니다(Ghostty `modes.zig`의 기본 켬). `set_private_mode`·`unset_private_mode`·`report_private_mode`가 `PrivateMode::Unknown(번호)` 갈래로 네 비트를 다루고(`key_input_mode_bit`), `reset_state`는 기본값으로 되돌립니다. `vte`는 고치지 않았습니다(번호가 `Unknown`으로 옵니다). `b-terminal(#301)`로 찾을 수 있습니다. 되돌릴 조건은 위 #277과 같습니다.
- `alacritty_terminal` 쪽(#304): `src/term/mod.rs`의 `TermMode`를 `u32`에서 `u64`로 넓혔습니다(`ANY`도 `u64::MAX`). 이유: 0.24에 더할 DEC 모드(마우스 형식·키 입력 모드·화면 반전 등)가 남은 6비트보다 많습니다. `TermMode`를 비트 값으로 읽는 곳은 없습니다. 되돌릴 조건은 upstream이 같은 모드를 다른 방식으로 들게 될 때입니다.
- 이유: vte 0.15가 SGR 53·55를 해석하지 않아 윗줄을 그릴 수 없었습니다(Ghostty는 그립니다).
- 되돌릴 조건: upstream vte가 53·55를 해석하는 판을 내면 `vendor/vte`와 `[patch.crates-io]`의 `vte` 줄을 지우고, `alacritty_terminal`의 `terminal_attribute`를 새 `Attr` 이름에 맞춥니다. 판이 바뀌면 `diff`로 `ansi.rs`를 먼저 비교합니다. `vendor/vte`는 작업 공간 밖이라 자체 테스트가 돌지 않으므로 해석 테스트는 `crates/bt-term/tests/sgr_parse.rs`에 있습니다.

## winit: 제목 막대 폴더 아이콘과 파일 열기 요청(#279)

- 고친 곳: `src/platform/macos.rs` 끝의 `pub fn set_represented_path(&Window, Option<&Path>)`와 `pub fn set_open_file_handler(Box<dyn Fn(PathBuf) + Send + Sync>)`, `src/platform_impl/macos/window_delegate.rs` 끝의 `impl WindowDelegate { set_represented_path }`(`NSWindow.setRepresentedURL`), `src/platform_impl/macos/app_state.rs`의 앱 대리자에 `application:openFile:`, `src/platform_impl/macos/mod.rs`의 `set_open_file_handler`·`deliver_open_file`(등록 전에 온 요청은 쌓아 두었다가 등록할 때 넘김), `Cargo.toml`의 `objc2-foundation` 기능 `NSURL`. `b-terminal(#279)`로 찾을 수 있습니다. 이유: Ghostty `macos-titlebar-proxy-icon`(작업 폴더 아이콘)과 `macos-dock-drop-behavior`(Dock에 놓은 폴더·파일). 작업 공간이 `unsafe_code = "deny"`라 앱 코드는 `NSWindow`와 앱 대리자를 만질 수 없고, winit 0.30에는 파일 열기 사건이 없습니다. 되돌릴 조건: winit이 `representedURL`과 파일 열기 사건을 내놓으면 지우고 그것을 씁니다.
