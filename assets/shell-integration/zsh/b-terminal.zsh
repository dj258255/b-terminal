# b-terminal 셸 통합(zsh): 명령 경계를 OSC 133으로, 명령 글을 OSC 633;E로, 폴더를 OSC 7로 알린다.
# 터미널은 이것으로 프롬프트 사이 이동, 오래 걸린 명령이 끝나면 알림, btctl run을 한다.
# 기능별 켜고 끄기(설정 shell-integration-features)는 환경 변수 BT_SHELL_FEATURES(켜진 이름을 쉼표로 이은 글)로 온다.
# 그 값은 이름과 맞는지 비교만 한다(eval하지 않는다). 변수가 없으면 모두 꺼진 것으로 본다
[[ -n "$_BT_INTEGRATED" ]] && return
typeset -g _BT_INTEGRATED=1
typeset -g _bt_running=0
typeset -g _bt_path_done=0
typeset -gi _bt_f_cursor=0 _bt_f_title=0 _bt_f_path=0
[[ ",${BT_SHELL_FEATURES}," == *",cursor,"* ]] && _bt_f_cursor=1
[[ ",${BT_SHELL_FEATURES}," == *",title,"* ]] && _bt_f_title=1
[[ ",${BT_SHELL_FEATURES}," == *",path,"* ]] && _bt_f_path=1
autoload -Uz add-zsh-hook

_bt_precmd() {
  local ret=$?
  if (( _bt_running )); then
    builtin printf '\e]133;D;%d\a' "$ret"
    _bt_running=0
  fi
  builtin printf '\e]133;A\a'
  builtin printf '\e]7;file://%s%s\a' "${HOST}" "${PWD// /%20}"
  # path: 사용자의 .zprofile·.zshrc가 PATH를 다시 쓴 뒤인 첫 프롬프트에서 한 번, 없을 때만 끝에 더한다
  if (( _bt_f_path && ! _bt_path_done )); then
    _bt_path_done=1
    if [[ "$BT_BIN_DIR" == /* && "$BT_BIN_DIR" != *[:[:cntrl:]]* && ":$PATH:" != *":$BT_BIN_DIR:"* ]]; then
      PATH="$PATH:$BT_BIN_DIR"
    fi
  fi
  # cursor: 프롬프트에서는 깜박이는 막대
  (( _bt_f_cursor )) && builtin printf '\e[5 q'
  # title: 프롬프트에서는 폴더(길면 뒤 세 마디). 제어 문자는 공백으로
  if (( _bt_f_title )); then
    local d="${(%):-%(4~|…/%3~|%~)}"
    builtin printf '\e]2;%s\a' "${d//[[:cntrl:]]/ }"
  fi
}

_bt_preexec() {
  _bt_running=1
  # 제어 문자는 공백으로(OSC를 끊지 않게)
  local cmd="${1//[[:cntrl:]]/ }"
  # cursor: 명령이 도는 동안은 원래 모양(DECSCUSR 0)
  (( _bt_f_cursor )) && builtin printf '\e[0 q'
  # title: 명령이 도는 동안은 명령 글(200자까지)
  (( _bt_f_title )) && builtin printf '\e]2;%s\a' "${cmd[1,200]}"
  builtin printf '\e]633;E;%s\a' "$cmd"
  builtin printf '\e]133;C\a'
}

# 맨 앞에 둔다: 다른 precmd가 $?를 바꾸기 전에 종료 코드를 읽는다
precmd_functions=(_bt_precmd ${precmd_functions[@]})
add-zsh-hook preexec _bt_preexec
