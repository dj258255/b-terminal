# 셸 통합

b-terminal이 탭에서 띄우는 셸(zsh·bash)에 자동으로 싣는 스크립트입니다(#43). 실행 파일에 묶여 있다가, 앱이 뜰 때 사용자 전용 폴더(`$TMPDIR/b-terminal-<uid>/shell-integration/<판>`, 0700)에 풀어 씁니다.

- zsh: `ZDOTDIR`을 이 `zsh/` 폴더로 바꿔 띄웁니다. `.zshenv`가 원래 `ZDOTDIR`을 돌리고 사용자의 `.zshenv`를 읽은 뒤 `b-terminal.zsh`를 싣습니다. 사용자의 `.zprofile`·`.zshrc`는 zsh가 그대로 읽습니다.
- bash: `bash --rcfile bash/rc -i`로 띄웁니다. `rc`가 로그인 셸처럼 `/etc/profile`과 `~/.bash_profile`(또는 `~/.bash_login`, `~/.profile`)을 읽은 뒤 `b-terminal.bash`를 싣습니다.

보내는 것: `OSC 133;A`(프롬프트), `OSC 133;C`(실행), `OSC 133;D;종료 코드`, `OSC 633;E;명령`, `OSC 7`(폴더). 끄려면 설정에 `shell-integration = none`.
