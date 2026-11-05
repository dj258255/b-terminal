# 에이전트 연결

b-terminal은 탭의 에이전트 상태를 세 길로 압니다. 아래로 갈수록 정확합니다.

| 길 | 설정 | 아는 것 |
|---|---|---|
| 전경 프로세스 이름 | 없음 | 이 탭에서 claude·codex·opencode·gemini·aider가 돈다 |
| OSC 알림(9·99·777)과 진행 표시(9;4) | 없음 | 끝났거나 사람을 기다린다(무엇을 기다리는지는 모름) |
| 훅 → `btctl hook` | 아래 | 작업 중(지금 도구와 대상), 권한·질문·입력 기다림, 끝남(마지막 답), 오류 |

훅이 한 번이라도 상태를 알려 온 탭은 OSC로 상태를 추측하지 않습니다.

## Claude Code

```bash
btctl setup claude            # 설정 조각 보기
btctl setup claude --write    # ~/.claude/settings.json(또는 $CLAUDE_CONFIG_DIR/settings.json)에 합치기
```

- `--write`는 기존 설정과 다른 훅을 그대로 두고, 같은 명령이 없는 이벤트에만 더합니다. 두 번 돌려도 늘지 않습니다. 덮어쓰기 전에 원본을 `settings.json.bt-backup`으로 남깁니다.
- 쓰는 이벤트와 상태

| 이벤트 | 상태 | 한 줄 |
|---|---|---|
| `SessionStart`·`SessionEnd` | 쉼 | |
| `UserPromptSubmit` | 작업 중 | `요청: <요청 첫 줄>` |
| `PreToolUse` | 작업 중 | `Edit: crates/app.rs`, `Bash: cargo test` |
| `SubagentStart` | 작업 중 | `서브에이전트: Explore` |
| `PermissionRequest`, `Notification`(`permission_prompt`) | 권한 기다림 | 허락을 묻는 도구와 대상 |
| `Notification`(`elicitation_dialog`·`agent_needs_input`) | 질문 기다림 | 알림 글 |
| `Notification`(`idle_prompt`) | 입력 기다림 | 알림 글 |
| `Stop` | 끝남 | 마지막 답 첫 줄(`last_assistant_message`) |
| `StopFailure` | 오류 | |

- 훅은 탭 밖(`BT_SOCKET`이 없음)이나 터미널이 꺼졌을 때, 입력이 깨졌을 때도 0으로 끝나 Claude Code를 막지 않습니다. 까닭을 보려면 `BTCTL_DEBUG=1`.

## Codex

```bash
btctl setup codex             # ~/.codex/config.toml에 넣을 한 줄
```

`notify = ["<btctl 경로>", "hook", "codex"]`를 설정 파일 맨 위(어떤 `[표]`보다 앞)에 둡니다. Codex의 `notify`는 턴이 끝날 때(`agent-turn-complete`)만 부르므로 "끝남"만 압니다. 승인 요청은 Codex TUI의 OSC 9 알림으로 옵니다(`[tui] notifications`).

## 직접 보내기

```bash
btctl status working --activity "마이그레이션 실행"
btctl status waiting-question --activity "어느 DB로 할까요?"
btctl status done
```
