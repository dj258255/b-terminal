# 셸 통합

b-terminal이 탭에서 띄우는 셸(zsh·bash)에 자동으로 싣는 스크립트입니다(#43). 실행 파일에 묶여 있다가, 앱이 뜰 때 사용자 전용 폴더(`$TMPDIR/b-terminal-<uid>/shell-integration/<판>`, 0700)에 풀어 씁니다.

- zsh: `ZDOTDIR`을 이 `zsh/` 폴더로 바꿔 띄웁니다. `.zshenv`가 원래 `ZDOTDIR`을 돌리고 사용자의 `.zshenv`를 읽은 뒤 `b-terminal.zsh`를 싣습니다. 사용자의 `.zprofile`·`.zshrc`는 zsh가 그대로 읽습니다.
- bash: `bash --rcfile bash/rc -i`로 띄웁니다. `rc`가 로그인 셸처럼 `/etc/profile`과 `~/.bash_profile`(또는 `~/.bash_login`, `~/.profile`)을 읽은 뒤 `b-terminal.bash`를 싣습니다.

보내는 것: `OSC 133;A;cl=line`(프롬프트. `cl=line`은 이 셸이 프롬프트 줄 안의 클릭 이동을 ←·→로 받는다는 밝힘입니다. bash는 `OSC 133;A;redraw=last;cl=line`: 줄 편집기가 크기가 바뀔 때 마지막 줄만 다시 그리므로 터미널이 그 줄만 비웁니다. zsh는 `redraw` 기본값), `OSC 133;B`(입력 시작: 터미널이 이 뒤에 쓴 칸을 입력 칸으로 보고 클릭 이동이 그 칸만 셉니다), `OSC 133;C`(실행), `OSC 133;D;종료 코드`, `OSC 633;E;명령`, `OSC 7`(폴더). 끄려면 설정에 `shell-integration = none`.

A와 B는 Ghostty 셸 통합처럼 **PS1 안에** 넣습니다(#338). 프롬프트를 다시 그릴 때마다 같이 찍히려면 그래야 합니다. zsh는 `precmd_functions` 맨 뒤의 `_bt_precmd_prompt`가 PS1 맨 앞에 `%{ESC]133;A;cl=lineBEL%}`, 맨 끝에 `%{ESC]133;BBEL%}`를 붙이고(여러 줄 PS1의 줄바꿈 뒤와 PS2에는 `133;P;k=s`), 명령 앞(`preexec`)에 표지를 더하기 전의 PS1·PS2로 되돌립니다. 테마가 PS1을 다시 만들었으면 그 값을 존중하고, 명령 없이 프롬프트만 다시 오면 표지를 두 번 넣지 않습니다. 끝이 홀로 `%`인 PS1은 `%%`로 바꿔 표지를 삼키지 않게 합니다. bash는 `PROMPT_COMMAND` 끝의 함수가 PS1·PS2에 `133;P;k=i`(줄 편집기 다시 그리기가 줄을 바꾸지 않게 A가 아닌 P)와 끝의 `133;B`를 붙이고, DEBUG 덫이 명령 앞에서 되돌립니다. 표지가 이미 있는 프롬프트(빈 줄에서 Enter)에는 다시 붙이지 않습니다. 한계: Ghostty zsh가 `zle-line-init`으로 하는 "PS1에 표지가 없으면 B만 따로 찍기"와 `ssh`·`sudo` 감싸기는 옮기지 않았습니다. 첫 프롬프트에서 다른 훅이 PS1을 다시 만들면 그 프롬프트에는 B가 없을 수 있습니다.

기능별 켜고 끄기(`shell-integration-features`, #194): 켜진 이름을 쉼표로 이은 환경 변수 `BT_SHELL_FEATURES`로 스크립트에 옵니다(값은 이름 비교에만 쓰고 eval하지 않습니다). `cursor`는 프롬프트에서 `CSI 5 SP q`, 명령 전에 `CSI 0 SP q`. `title`은 프롬프트에서 폴더, 명령 중에는 명령 글을 OSC 2로. `path`는 `BT_BIN_DIR`(실행 파일 폴더)을 첫 프롬프트에서 PATH 끝에 한 번. `sudo`·`ssh-env`·`ssh-terminfo`는 받기만 합니다.
