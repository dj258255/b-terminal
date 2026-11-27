//! 사용자가 설정에 쓴 정규식(kitty `detect_url_regex`, 마커 `regex`)을 컴파일하는 한 곳(#929, ADR-101).
//!
//! `regex` 크레이트는 역참조와 앞·뒤 보기가 없고 맞추는 시간이 글 길이에 비례한다(백트래킹이 없다). 그래서 사용자가 쓴
//! 정규식이 화면 글에 닿아도 `(a*)*b` 같은 꼴이 시간을 먹지 못한다. 남는 위험은 컴파일 결과의 크기이고 여기서 상한을 둔다.

use regex::{Regex, RegexBuilder};

/// 정규식 한 개의 글 길이 상한(바이트)
pub const MAX_PATTERN_BYTES: usize = 4096;
/// 컴파일한 프로그램 크기 상한(`RegexBuilder::size_limit`)
pub const SIZE_LIMIT: usize = 1 << 20;
/// 지연 DFA 캐시 크기 상한(`RegexBuilder::dfa_size_limit`)
pub const DFA_SIZE_LIMIT: usize = 1 << 20;
/// 괄호 겹침 깊이 상한
pub const NEST_LIMIT: u32 = 50;
/// 설정에서 받는 정규식 개수 상한(`link-url-regex`)
pub const MAX_PATTERNS: usize = 16;

/// 상한을 지켜 컴파일한다. 못 읽는 정규식은 사람이 읽을 이유를 돌려준다
pub fn compile(pattern: &str, ignore_case: bool) -> Result<Regex, String> {
    if pattern.is_empty() {
        return Err("정규식이 비었습니다".into());
    }
    if pattern.len() > MAX_PATTERN_BYTES {
        return Err(format!("정규식이 너무 깁니다({}바이트 넘음)", MAX_PATTERN_BYTES));
    }
    RegexBuilder::new(pattern)
        .case_insensitive(ignore_case)
        .size_limit(SIZE_LIMIT)
        .dfa_size_limit(DFA_SIZE_LIMIT)
        .nest_limit(NEST_LIMIT)
        .build()
        .map_err(|e| match e {
            regex::Error::CompiledTooBig(_) => format!("정규식이 너무 큽니다(컴파일 결과 {}바이트 상한)", SIZE_LIMIT),
            regex::Error::Syntax(s) => {
                let last = s
                    .lines()
                    .last()
                    .unwrap_or("")
                    .trim()
                    .trim_start_matches("error: ")
                    .to_string();
                format!("정규식을 읽지 못했습니다: {last}")
            }
            _ => "정규식을 읽지 못했습니다".into(),
        })
}

/// 정규식 여러 개를 `(?P<mcg0>..)|(?P<mcg1>..)`로 하나로 묶어 컴파일한다(kitty `marker_from_multiple_regex`와 같은 꼴).
/// 하나씩 먼저 컴파일해 괄호가 어긋난 식(`a)|(b`)이 묶음 밖으로 새지 못하게 한다. 같은 자리에서는 앞 식이 이긴다
pub fn compile_alternation(patterns: &[&str], ignore_case: bool) -> Result<Regex, String> {
    if patterns.is_empty() {
        return Err("정규식이 없습니다".into());
    }
    if patterns.len() > MAX_PATTERNS {
        return Err(format!("정규식은 {MAX_PATTERNS}개까지 받습니다"));
    }
    for p in patterns {
        compile(p, ignore_case)?;
    }
    let joined = patterns
        .iter()
        .enumerate()
        .map(|(i, p)| format!("(?P<mcg{i}>{p})"))
        .collect::<Vec<_>>()
        .join("|");
    RegexBuilder::new(&joined)
        .case_insensitive(ignore_case)
        .size_limit(SIZE_LIMIT)
        .dfa_size_limit(DFA_SIZE_LIMIT)
        .nest_limit(NEST_LIMIT)
        .build()
        .map_err(|_| format!("정규식이 너무 큽니다(컴파일 결과 {SIZE_LIMIT}바이트 상한)"))
}
