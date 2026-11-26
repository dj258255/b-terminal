# b-terminal 셸 통합(nushell). b-terminal이 XDG_DATA_DIRS 맨 앞에 이 스크립트가 든 폴더를 더해 nu를 띄우면
# nushell이 vendor/autoload의 이 파일을 시작 때 읽는다(Ghostty와 같은 방식). 사용자의 config.nu·env.nu는 건드리지 않는다.
# 명령 경계(OSC 133)와 폴더(OSC 7)는 nushell이 `$env.config.shell_integration`으로 스스로 보내므로 여기서 다시 보내지 않는다.
# 여기서는 cursor·path 기능만 한다. 기능은 환경 변수 BT_SHELL_FEATURES(켜진 이름을 쉼표로 이은 글)에 이름이 정확히 있을 때만 켠다

# 하위 프로세스에 우리 폴더가 새지 않게 XDG_DATA_DIRS에서 뺀다
if 'BT_SHELL_INTEGRATION_XDG_DIR' in $env {
  let ours = $env.BT_SHELL_INTEGRATION_XDG_DIR
  if 'XDG_DATA_DIRS' in $env {
    $env.XDG_DATA_DIRS = ($env.XDG_DATA_DIRS | split row ':' | where {|d| $d != $ours } | str join ':')
  }
  hide-env BT_SHELL_INTEGRATION_XDG_DIR
}

if $nu.is-interactive {
  let features = ($env.BT_SHELL_FEATURES? | default '' | split row ',')

  # path: 실행 파일 폴더(BT_BIN_DIR)를 PATH 끝에. 절대 경로이고 `:`·제어 문자가 없을 때만, 이미 있으면 더하지 않는다
  if ('path' in $features) and (($env.BT_BIN_DIR? | default '') =~ '^/[^:\x00-\x1f\x7f]*$') {
    let bin = $env.BT_BIN_DIR
    if $bin not-in $env.PATH {
      $env.PATH = ($env.PATH | append $bin)
    }
  }

  # cursor: 프롬프트에서는 깜박이는 막대, 명령이 도는 동안은 원래 모양
  if 'cursor' in $features {
    let esc = (char escape)
    $env.config.hooks.pre_prompt = ($env.config.hooks.pre_prompt? | default [] | append {|| print -n $"($esc)[5 q" })
    $env.config.hooks.pre_execution = ($env.config.hooks.pre_execution? | default [] | append {|| print -n $"($esc)[0 q" })
  }
}
