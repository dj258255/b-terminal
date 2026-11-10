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

# 프롬프트 표지(#338, Ghostty ghostty-integration 124~190줄): PS1 맨 앞에 `133;A;cl=line`(프롬프트 시작, 셸이 줄 안 클릭 이동을 받는다고 밝힘),
# 맨 끝에 `133;B`(여기부터 입력 칸). zsh가 프롬프트를 다시 그릴 때마다(reset-prompt, 크기 변경) 같이 찍히려면 PS1 안에 있어야 한다.
# 다른 precmd 훅이 PS1을 바꾸기 전에 보이지 않게 이 훅은 맨 뒤에 둔다. 표지를 더하기 전의 PS1을 기억해 두었다가 명령 전에 되돌린다.
# 테마가 PS1을 다시 만들었으면(기억해 둔 값과 다르면) 그 값을 존중해 그 위에 더한다
typeset -g _bt_saved_ps1 _bt_saved_ps2 _bt_marked_ps1
typeset -gi _bt_marked=0  # 표지를 한 번이라도 더했는가(기억해 둔 PS1이 있는가)
_bt_precmd_prompt() {
  builtin local -i prompt_percent=0
  [[ -o prompt_percent ]] && prompt_percent=1
  builtin emulate -L zsh -o no_warn_create_global -o no_aliases
  builtin local mark1=$'%{\e]133;A;cl=line\a%}'
  if (( prompt_percent )); then
    # 맨 뒤가 아니면 다음 프롬프트부터는 맨 뒤에서 돌게 옮긴다(이번에는 그대로 더한다)
    if [[ ${precmd_functions[-1]} != _bt_precmd_prompt ]]; then
      precmd_functions=(${precmd_functions:#_bt_precmd_prompt} _bt_precmd_prompt)
    fi
    builtin local -i ps1_changed=0
    if (( _bt_marked )); then
      if [[ $PS1 == "$_bt_marked_ps1" ]]; then
        PS1=$_bt_saved_ps1
        PS2=$_bt_saved_ps2
      elif [[ $PS1 != "$_bt_saved_ps1" ]]; then
        ps1_changed=1
      fi
    fi
    _bt_saved_ps1=$PS1
    _bt_saved_ps2=$PS2
    builtin local mark2=$'%{\e]133;P;k=s\a%}'
    builtin local markB=$'%{\e]133;B\a%}'
    # PS1이 맨 끝에 홀로 `%`로 끝나면 `%{`가 되어 표지를 삼킨다: `%%`로 쓴다
    [[ $PS1 == *[^%]% || $PS1 == % ]] && PS1=$PS1%
    PS1=${mark1}${PS1}${markB}
    # 여러 줄 프롬프트: 줄바꿈 뒤 줄은 이어진 프롬프트(k=s). 테마가 PS1을 바꿨으면 건드리지 않는다
    if (( ! ps1_changed )) && [[ $PS1 == *$'\n'* ]]; then
      PS1=${PS1//$'\n'/$'\n'${mark2}}
    fi
    [[ $PS2 == *[^%]% || $PS2 == % ]] && PS2=$PS2%
    PS2=${mark2}${PS2}${markB}
    _bt_marked_ps1=$PS1
    _bt_marked=1
  elif ! builtin zle 2>/dev/null; then
    # prompt_percent가 꺼져 있으면 PS1에 못 넣는다. 프롬프트 시작만 찍는다
    builtin printf '\e]133;A;cl=line\a'
  fi
}

_bt_preexec() {
  # 다른 훅이 PS1을 바꾸지 않았으면 표지를 더하기 전 값으로 되돌린다(다른 preexec 훅이 표지 없는 PS1을 보게)
  if (( _bt_marked )) && [[ $PS1 == "$_bt_marked_ps1" ]]; then
    PS1=$_bt_saved_ps1
    PS2=$_bt_saved_ps2
  fi
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

# _bt_precmd는 맨 앞에 둔다: 다른 precmd가 $?를 바꾸기 전에 종료 코드를 읽는다. _bt_precmd_prompt는 맨 뒤
precmd_functions=(_bt_precmd ${precmd_functions[@]} _bt_precmd_prompt)
add-zsh-hook preexec _bt_preexec
