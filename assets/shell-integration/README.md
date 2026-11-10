# 셸 통합

b-terminal이 탭에서 띄우는 셸(zsh·bash)에 자동으로 싣는 스크립트입니다(#43). 실행 파일에 묶여 있다가, 앱이 뜰 때 사용자 전용 폴더(`$TMPDIR/b-terminal-<uid>/shell-integration/<판>`, 0700)에 풀어 씁니다.

- zsh: `ZDOTDIR`을 이 `zsh/` 폴더로 바꿔 띄웁니다. `.zshenv`가 원래 `ZDOTDIR`을 돌리고 사용자의 `.zshenv`를 읽은 뒤 `b-terminal.zsh`를 싣습니다. 사용자의 `.zprofile`·`.zshrc`는 zsh가 그대로 읽습니다.
- bash: `bash --rcfile bash/rc -i`로 띄웁니다. `rc`가 로그인 셸처럼 `/etc/profile`과 `~/.bash_profile`(또는 `~/.bash_login`, `~/.profile`)을 읽은 뒤 `b-terminal.bash`를 싣습니다.

보내는 것: `OSC 133;A`(프롬프트. bash는 `OSC 133;A;redraw=last`: 줄 편집기가 크기가 바뀔 때 마지막 줄만 다시 그리므로 터미널이 그 줄만 비웁니다. zsh는 `redraw` 기본값), `OSC 133;C`(실행), `OSC 133;D;종료 코드`, `OSC 633;E;명령`, `OSC 7`(폴더). 끄려면 설정에 `shell-integration = none`.

기능별 켜고 끄기(`shell-integration-features`, #194): 켜진 이름을 쉼표로 이은 환경 변수 `BT_SHELL_FEATURES`로 스크립트에 옵니다(값은 이름 비교에만 쓰고 eval하지 않습니다). `cursor`는 프롬프트에서 `CSI 5 SP q`, 명령 전에 `CSI 0 SP q`. `title`은 프롬프트에서 폴더, 명령 중에는 명령 글을 OSC 2로. `path`는 `BT_BIN_DIR`(실행 파일 폴더)을 첫 프롬프트에서 PATH 끝에 한 번. `sudo`·`ssh-env`·`ssh-terminfo`는 받기만 합니다.
