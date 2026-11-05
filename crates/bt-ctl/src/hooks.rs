//! 에이전트 CLI의 훅 입력을 탭 상태로 바꾼다. 문서에 있는 필드만 읽는다(b-studio ADR-189와 같은 원칙).
//!
//! - Claude Code: 훅이 표준 입력으로 JSON 하나를 준다(<https://code.claude.com/docs/en/hooks>)
//! - Codex: `notify` 프로그램의 마지막 인자로 JSON 하나를 준다. `agent-turn-complete`만 온다

use bt_core::{AgentState, WaitReason};
use serde_json::Value;

/// 탭에 보낼 상태 하나
#[derive(Debug, Clone, PartialEq)]
pub struct Update {
    pub state: AgentState,
    pub activity: Option<String>,
}

/// 한 줄로 줄이고 자른다
fn short(s: &str, max: usize) -> String {
    let line = s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let mut out: String = line.chars().take(max).collect();
    if line.chars().count() > max {
        out.push('…');
    }
    out
}

/// 도구와 대상을 한 줄로: `Bash: cargo test`, `Edit: src/app.rs`
pub fn tool_line(tool: &str, input: &Value) -> String {
    let target = ["command", "file_path", "path", "pattern", "url", "description", "query"]
        .iter()
        .find_map(|k| input.get(*k).and_then(Value::as_str))
        .map(|t| {
            // 파일 경로는 끝 두 마디만
            if t.starts_with('/') {
                let parts: Vec<&str> = t.rsplit('/').take(2).collect();
                parts.into_iter().rev().collect::<Vec<_>>().join("/")
            } else {
                t.to_string()
            }
        });
    match target {
        Some(t) => short(&format!("{tool}: {t}"), 80),
        None => tool.to_string(),
    }
}

/// Claude Code 훅 입력 하나 → 상태. 상태를 바꾸지 않는 이벤트면 없다
pub fn claude(input: &Value) -> Option<Update> {
    let event = input.get("hook_event_name")?.as_str()?;
    let tool = || {
        let name = input.get("tool_name").and_then(Value::as_str).unwrap_or("도구");
        tool_line(name, input.get("tool_input").unwrap_or(&Value::Null))
    };
    let up = |state, activity: Option<String>| Some(Update { state, activity });
    match event {
        "SessionStart" => up(AgentState::Idle, None),
        "UserPromptSubmit" => {
            let prompt = input.get("prompt").and_then(Value::as_str).map(|p| short(p, 60));
            up(AgentState::Working, prompt.map(|p| format!("요청: {p}")))
        }
        "PreToolUse" | "PostToolUse" | "PostToolUseFailure" => up(AgentState::Working, Some(tool())),
        "PermissionRequest" => up(AgentState::Waiting(WaitReason::Permission), Some(tool())),
        "SubagentStart" => {
            let ty = input
                .get("agent_type")
                .and_then(Value::as_str)
                .unwrap_or("서브에이전트");
            up(AgentState::Working, Some(format!("서브에이전트: {ty}")))
        }
        "Notification" => {
            let message = input.get("message").and_then(Value::as_str).map(|m| short(m, 80));
            let ty = input.get("notification_type").and_then(Value::as_str).unwrap_or("");
            let state = match ty {
                "permission_prompt" => AgentState::Waiting(WaitReason::Permission),
                "elicitation_dialog" | "elicitation_url_dialog" | "agent_needs_input" => {
                    AgentState::Waiting(WaitReason::Question)
                }
                "idle_prompt" => AgentState::Waiting(WaitReason::Input),
                // 로그인 성공·플러그인 알림 같은 것은 상태를 바꾸지 않는다
                _ => return None,
            };
            up(state, message)
        }
        "Stop" => {
            let last = input
                .get("last_assistant_message")
                .and_then(Value::as_str)
                .map(|m| short(m, 80));
            up(AgentState::Done, last)
        }
        "StopFailure" => up(AgentState::Error, Some("API 오류로 턴이 끝났습니다".into())),
        "SessionEnd" => up(AgentState::Idle, None),
        _ => None,
    }
}

/// Codex `notify` 입력 하나 → 상태
pub fn codex(input: &Value) -> Option<Update> {
    match input.get("type")?.as_str()? {
        "agent-turn-complete" => {
            let last = input
                .get("last-assistant-message")
                .and_then(Value::as_str)
                .map(|m| short(m, 80));
            Some(Update {
                state: AgentState::Done,
                activity: last,
            })
        }
        _ => None,
    }
}

/// `btctl setup claude`가 보이는 훅 설정 조각. 사용자 설정 파일에 그대로 붙일 수 있다
pub fn claude_settings(btctl: &str) -> Value {
    let cmd = format!("{btctl} hook claude");
    let entry = |matcher: Option<&str>| {
        let mut v = serde_json::json!({ "hooks": [{ "type": "command", "command": cmd, "timeout": 5 }] });
        if let Some(m) = matcher {
            v["matcher"] = Value::String(m.into());
        }
        v
    };
    serde_json::json!({
        "hooks": {
            "SessionStart": [entry(None)],
            "UserPromptSubmit": [entry(None)],
            "PreToolUse": [entry(Some("*"))],
            "PermissionRequest": [entry(Some("*"))],
            "SubagentStart": [entry(None)],
            "Notification": [entry(None)],
            "Stop": [entry(None)],
            "StopFailure": [entry(None)],
            "SessionEnd": [entry(None)],
        }
    })
}

/// 기존 설정에 훅을 합친다. 같은 명령이 이미 있는 이벤트는 건드리지 않고, 다른 훅은 그대로 둔다
pub fn merge_claude_settings(existing: &mut Value, btctl: &str) -> usize {
    let wanted = claude_settings(btctl);
    let cmd = format!("{btctl} hook claude");
    if !existing.is_object() {
        *existing = serde_json::json!({});
    }
    let hooks = existing
        .as_object_mut()
        .map(|o| o.entry("hooks").or_insert_with(|| serde_json::json!({})));
    let Some(Value::Object(hooks)) = hooks else { return 0 };
    let mut added = 0;
    for (event, entries) in wanted["hooks"].as_object().into_iter().flatten() {
        let list = hooks.entry(event.clone()).or_insert_with(|| Value::Array(Vec::new()));
        let Value::Array(list) = list else { continue };
        let present = list.iter().any(|e| {
            e.get("hooks").and_then(Value::as_array).is_some_and(|hs| {
                hs.iter()
                    .any(|h| h.get("command").and_then(Value::as_str) == Some(cmd.as_str()))
            })
        });
        if !present {
            list.extend(entries.as_array().cloned().unwrap_or_default());
            added += 1;
        }
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_permission_request_waits_with_the_command() {
        let input = json!({
            "hook_event_name": "PermissionRequest",
            "tool_name": "Bash",
            "tool_input": { "command": "rm -rf node_modules", "description": "Remove node_modules directory" }
        });
        assert_eq!(
            claude(&input),
            Some(Update {
                state: AgentState::Waiting(WaitReason::Permission),
                activity: Some("Bash: rm -rf node_modules".into())
            })
        );
    }

    #[test]
    fn claude_notification_types_map_to_wait_reasons() {
        let n =
            |ty: &str| claude(&json!({ "hook_event_name": "Notification", "message": "m", "notification_type": ty }));
        assert_eq!(
            n("permission_prompt").unwrap().state,
            AgentState::Waiting(WaitReason::Permission)
        );
        assert_eq!(n("idle_prompt").unwrap().state, AgentState::Waiting(WaitReason::Input));
        assert_eq!(
            n("elicitation_dialog").unwrap().state,
            AgentState::Waiting(WaitReason::Question)
        );
        assert_eq!(n("auth_success"), None, "상태와 무관한 알림은 무시한다");
    }

    #[test]
    fn claude_tool_and_stop_and_failure() {
        let pre = claude(&json!({ "hook_event_name": "PreToolUse", "tool_name": "Edit", "tool_input": { "file_path": "/Users/me/dev/b-terminal/crates/app.rs" } })).unwrap();
        assert_eq!(
            pre,
            Update {
                state: AgentState::Working,
                activity: Some("Edit: crates/app.rs".into())
            }
        );
        let stop = claude(
            &json!({ "hook_event_name": "Stop", "last_assistant_message": "테스트 48개가 통과했습니다.\n자세히는..." }),
        )
        .unwrap();
        assert_eq!(
            stop,
            Update {
                state: AgentState::Done,
                activity: Some("테스트 48개가 통과했습니다.".into())
            }
        );
        assert_eq!(
            claude(&json!({ "hook_event_name": "StopFailure" })).unwrap().state,
            AgentState::Error
        );
        assert_eq!(claude(&json!({ "hook_event_name": "WhatIsThis" })), None);
        assert_eq!(claude(&json!({})), None);
    }

    #[test]
    fn codex_turn_complete_is_done() {
        let u =
            codex(&json!({ "type": "agent-turn-complete", "thread-id": "t", "last-assistant-message": "끝" })).unwrap();
        assert_eq!(
            u,
            Update {
                state: AgentState::Done,
                activity: Some("끝".into())
            }
        );
        assert_eq!(codex(&json!({ "type": "other" })), None);
    }

    #[test]
    fn merging_keeps_other_hooks_and_is_idempotent() {
        let mut settings = json!({
            "model": "opus",
            "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": "say done" } ] } ] }
        });
        let added = merge_claude_settings(&mut settings, "/usr/local/bin/btctl");
        assert_eq!(added, 9);
        assert_eq!(settings["model"], "opus");
        let stop = settings["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2, "기존 Stop 훅은 그대로 두고 더한다");
        assert_eq!(stop[0]["hooks"][0]["command"], "say done");
        assert_eq!(
            merge_claude_settings(&mut settings, "/usr/local/bin/btctl"),
            0,
            "두 번 합쳐도 늘지 않는다"
        );
    }

    #[test]
    fn long_text_is_cut_to_one_line() {
        assert_eq!(short("\n\n  첫 줄  \n둘째", 10), "첫 줄");
        assert_eq!(short(&"a".repeat(20), 5), "aaaaa…");
    }
}
