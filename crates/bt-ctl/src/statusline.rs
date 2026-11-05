//! Claude Code 상태줄 입력에서 구독 한도 사용량을 꺼낸다.
//!
//! 상태줄 명령은 하나만 둘 수 있으므로, 이미 쓰는 상태줄이 있으면 `btctl statusline -- <기존 명령>`으로 감싼다.
//! 입력을 그 명령에 그대로 넘기고 출력도 그대로 돌려주며, 사용량만 터미널에 따로 보낸다

use bt_core::proto::UsageWindow;
use serde_json::Value;

/// `rate_limits`의 창들(five_hour·seven_day·spend_limit). 없거나 모양이 다르면 빈 목록
pub fn windows(input: &Value) -> Vec<UsageWindow> {
    let Some(limits) = input.get("rate_limits").and_then(Value::as_object) else {
        return Vec::new();
    };
    let mut out: Vec<UsageWindow> = limits
        .iter()
        .filter_map(|(kind, w)| {
            let used = w.get("used_percentage")?.as_f64()?;
            Some(UsageWindow {
                kind: kind.clone(),
                used_percent: used.clamp(0.0, 100.0),
                resets_at: w.get("resets_at").and_then(Value::as_i64),
            })
        })
        .collect();
    // 짧은 창부터
    let rank = |k: &str| match k {
        "five_hour" => 0,
        "seven_day" => 1,
        _ => 2,
    };
    out.sort_by_key(|w| rank(&w.kind));
    out
}

/// 감쌀 명령이 없을 때 찍는 한 줄: `Opus · 문맥 34% · 5시간 23% · 주간 41%`
pub fn default_line(input: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(m) = input.pointer("/model/display_name").and_then(Value::as_str) {
        parts.push(m.to_string());
    }
    if let Some(c) = input.pointer("/context_window/used_percentage").and_then(Value::as_f64) {
        parts.push(format!("문맥 {c:.0}%"));
    }
    for w in windows(input) {
        let label = match w.kind.as_str() {
            "five_hour" => "5시간",
            "seven_day" => "주간",
            "spend_limit" => "지출",
            other => other,
        };
        parts.push(format!("{label} {:.0}%", w.used_percent));
    }
    parts.join(" · ")
}

/// 기존 상태줄 명령을 `btctl statusline -- …`로 감싼 명령. 이미 감쌌으면 그대로
pub fn wrap_command(btctl: &str, existing: Option<&str>) -> String {
    match existing {
        Some(cmd) if cmd.contains(" statusline") && cmd.contains("btctl") => cmd.to_string(),
        Some(cmd) if !cmd.trim().is_empty() => format!("{btctl} statusline -- sh -c {}", shell_quote(cmd)),
        _ => format!("{btctl} statusline"),
    }
}

/// 작은따옴표로 감싼다(안의 작은따옴표는 '\'' 로)
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_windows_from_the_documented_shape() {
        let input = json!({
            "model": { "display_name": "Opus" },
            "context_window": { "used_percentage": 34.4 },
            "rate_limits": {
                "seven_day": { "used_percentage": 41.2, "resets_at": 1738857600 },
                "five_hour": { "used_percentage": 23.5, "resets_at": 1738425600 }
            }
        });
        let w = windows(&input);
        assert_eq!(w.len(), 2);
        assert_eq!(w[0].kind, "five_hour");
        assert_eq!(w[0].resets_at, Some(1738425600));
        assert_eq!(default_line(&input), "Opus · 문맥 34% · 5시간 24% · 주간 41%");
    }

    #[test]
    fn missing_or_odd_fields_are_skipped() {
        assert!(windows(&json!({})).is_empty());
        assert!(windows(&json!({ "rate_limits": { "five_hour": { "used_percentage": "x" } } })).is_empty());
        let w = windows(&json!({ "rate_limits": { "five_hour": { "used_percentage": 140 } } }));
        assert_eq!(w[0].used_percent, 100.0);
    }

    #[test]
    fn wrapping_keeps_the_existing_command() {
        assert_eq!(wrap_command("/b/btctl", None), "/b/btctl statusline");
        assert_eq!(
            wrap_command("/b/btctl", Some("node ~/hud.js --x 'a b'")),
            "/b/btctl statusline -- sh -c 'node ~/hud.js --x '\\''a b'\\'''"
        );
        let wrapped = wrap_command("/b/btctl", Some("node hud.js"));
        assert_eq!(wrap_command("/b/btctl", Some(&wrapped)), wrapped, "두 번 감싸지 않는다");
    }
}
