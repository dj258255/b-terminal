//! CSI 질의(XTVERSION, DA1·DA2, CSI 16·19 t, 모드 2048)의 답과 질의 읽기(#264, ADR-022)
//! 답은 고정 바이트와 검사한 숫자뿐이다. 요청에서 온 바이트는 되돌려 넣지 않는다
//! (Ghostty `stream_handler.zig` 826~850·980~995줄, `terminal/size_report.zig`)

use crate::notify::Signal;

/// DA1: 62 = 레벨 2, 22 = 색 글자, 52 = 클립보드 접근. 클립보드 쓰기가 deny면 52를 뺀다
pub const DA1_CLIPBOARD: &str = "\x1b[?62;22;52c";
pub const DA1_PLAIN: &str = "\x1b[?62;22c";
/// DA2: Ghostty와 같은 값. 셋째(DA3)는 답하지 않는다
pub const DA2: &str = "\x1b[>1;10;0c";

/// XTVERSION 답: `DCS > | 이름 버전 ST`. 버전은 빌드 때 정해진 값이다
pub fn xtversion() -> String {
    format!("\x1bP>|b-terminal {}\x1b\\", env!("CARGO_PKG_VERSION"))
}

/// 크기 보고의 꼴(Ghostty `size_report.zig`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeStyle {
    /// 모드 2048: `CSI 48 ; 행 ; 열 ; 높이px ; 너비px t`
    Mode2048,
    /// `CSI 14 t`: 글자 영역 픽셀
    Csi14,
    /// `CSI 16 t`: 칸 하나의 픽셀
    Csi16,
    /// `CSI 18 t`: 글자 영역 칸 수
    Csi18,
    /// `CSI 19 t`: 화면 크기(글자 수). 화면의 실제 크기를 모르니 Ghostty·VTE처럼 글자 영역 칸 수를 답한다
    Csi19,
}

/// 크기 보고에 쓰는 값
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeInfo {
    pub rows: u16,
    pub cols: u16,
    pub cell_width: u32,
    pub cell_height: u32,
}

/// 크기 보고를 만든다. 픽셀 합은 64비트로 계산해 넘치지 않는다
pub fn encode_size(style: SizeStyle, s: SizeInfo) -> String {
    let width_px = u64::from(s.cols) * u64::from(s.cell_width);
    let height_px = u64::from(s.rows) * u64::from(s.cell_height);
    match style {
        SizeStyle::Mode2048 => format!("\x1b[48;{};{};{height_px};{width_px}t", s.rows, s.cols),
        SizeStyle::Csi14 => format!("\x1b[4;{height_px};{width_px}t"),
        SizeStyle::Csi16 => format!("\x1b[6;{};{}t", s.cell_height, s.cell_width),
        SizeStyle::Csi18 => format!("\x1b[8;{};{}t", s.rows, s.cols),
        SizeStyle::Csi19 => format!("\x1b[9;{};{}t", s.rows, s.cols),
    }
}

/// DECRQM 답: 모드 2048. 켜짐이면 1, 꺼짐이면 2
pub fn mode_2048_report(on: bool) -> &'static str {
    if on { "\x1b[?2048;1$y" } else { "\x1b[?2048;2$y" }
}

/// DECRQM 답: 모드 2033. 켜짐이면 1, 꺼짐이면 2(#303)
pub fn mode_2033_report(on: bool) -> &'static str {
    if on { "\x1b[?2033;1$y" } else { "\x1b[?2033;2$y" }
}

/// 가시성 보고(Ghostty `device_status.zig` `encodeVisibilityReport`): 보이면 `CSI ? 999 ; 1 n`, 안 보이면 `CSI ? 999 ; 2 n`(#303)
pub fn visibility_report(visible: bool) -> &'static str {
    if visible { "\x1b[?999;1n" } else { "\x1b[?999;2n" }
}

/// DEC 장치 상태 질의(DECDSR, `CSI ? Ps n`) 가운데 "그 장치·기능이 없다"고 고정으로 답하는 것(#513, Ghostty `device_status.zig` `fixedReport`)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecStatus {
    /// `? 15 n`: 프린터 없음
    Printer,
    /// `? 25 n`: 사용자 정의 키(DECUDK) 잠김
    Udk,
    /// `? 26 n`: 키보드 상태(언어 모름)
    Keyboard,
    /// `? 55 n`: 로케이터 없음
    Locator,
    /// `? 56 n`: 로케이터 종류 모름
    LocatorType,
    /// `? 62 n`: 매크로 공간 없음(DECMSR)
    MacroSpace,
    /// `? 63 ; Pid n`: 매크로 메모리 체크섬(DECCKSR). 읽은 숫자 Pid를 되돌려 쓴다
    MemoryChecksum(u16),
    /// `? 75 n`: 통신 오류 없음
    DataIntegrity,
    /// `? 85 n`: 여러 세션 설정 없음
    MultipleSessions,
}

/// DECDSR 답. 고정 바이트와 읽은 숫자(Pid)뿐이고 요청의 바이트는 넣지 않는다(ADR-022).
/// 키보드 답은 DA1이 레벨 2(`62`)라서 매개변수가 하나다. Ghostty는 레벨에 따라 늘린다
pub fn dec_status_report(r: DecStatus) -> String {
    match r {
        DecStatus::Printer => "\x1b[?13n".into(),
        DecStatus::Udk => "\x1b[?21n".into(),
        DecStatus::Keyboard => "\x1b[?27;0n".into(),
        DecStatus::Locator => "\x1b[?50n".into(),
        DecStatus::LocatorType => "\x1b[?57;0n".into(),
        DecStatus::MacroSpace => "\x1b[0*{".into(),
        DecStatus::MemoryChecksum(id) => format!("\x1bP{id}!~0000\x1b\\"),
        DecStatus::DataIntegrity => "\x1b[?70n".into(),
        DecStatus::MultipleSessions => "\x1b[?83n".into(),
    }
}

/// `CSI ? 매개변수 n`의 매개변수(맨 앞 `?` 뒤)에서 DECDSR 질의를 읽는다. 규칙은 Ghostty `stream.zig`와 같다:
/// 매개변수가 0개거나 2개를 넘으면 답하지 않고, 둘째는 63(DECCKSR)일 때만 받는다.
/// 마지막의 빈 매개변수는 없는 것으로 치고(`? 63 ; n`은 Pid 0), 값은 u16에서 포화한다(`? 63 ; 70000 n`은 Pid 65535)
fn dec_dsr(text: &str) -> Option<Signal> {
    if text.contains('?') {
        return None;
    }
    let mut parts: Vec<&str> = text.split(';').collect();
    if parts.last() == Some(&"") {
        parts.pop();
    }
    let num = |p: &str| {
        p.bytes()
            .fold(0u32, |n, d| n.saturating_mul(10).saturating_add(u32::from(d - b'0')))
            .min(u32::from(u16::MAX)) as u16
    };
    if parts.is_empty() || parts.len() > 2 {
        return None;
    }
    let req = match num(parts[0]) {
        15 => DecStatus::Printer,
        25 => DecStatus::Udk,
        26 => DecStatus::Keyboard,
        55 => DecStatus::Locator,
        56 => DecStatus::LocatorType,
        62 => DecStatus::MacroSpace,
        63 => DecStatus::MemoryChecksum(parts.get(1).map_or(0, |p| num(p))),
        75 => DecStatus::DataIntegrity,
        85 => DecStatus::MultipleSessions,
        _ => return None,
    };
    // 둘째 매개변수는 DECCKSR만 받는다
    if parts.len() == 2 && !matches!(req, DecStatus::MemoryChecksum(_)) {
        return None;
    }
    Some(Signal::DecDsr(req))
}

fn size_param(params: &[u8]) -> Option<&str> {
    std::str::from_utf8(params).ok()
}

/// 끝난 CSI(중간 바이트·`<=>` 표시 없음)에서 뽑는 신호. 매개변수 바이트는 숫자·`;`·`?`만 모았다
pub fn plain(fin: u8, params: &[u8]) -> Option<Signal> {
    // DA1은 매개변수를 보지 않는다(Ghostty와 같다). `?`가 낀 것(`CSI ? c`)은 다른 질문이다
    if params.contains(&b'?') {
        return match fin {
            b'h' | b'l' => {
                let text = size_param(params.strip_prefix(b"?")?)?;
                if text.contains('?') {
                    return None;
                }
                text.split(';')
                    .any(|p| p == "2048")
                    .then_some(Signal::InBandSizeReports(fin == b'h'))
            }
            b'n' => dec_dsr(size_param(params.strip_prefix(b"?")?)?),
            _ => None,
        };
    }
    match fin {
        b'c' => Some(Signal::Da1),
        // 매개변수가 하나뿐일 때만(Ghostty `stream.zig` 2680~2705줄). 14 t·18 t는 VT 엔진이 답한다
        b't' => match size_param(params)? {
            "16" => Some(Signal::SizeReport(SizeStyle::Csi16)),
            "19" => Some(Signal::SizeReport(SizeStyle::Csi19)),
            _ => None,
        },
        _ => None,
    }
}

/// `CSI > …`(표시 `>` 하나가 맨 앞에 있고 중간 바이트는 없음). 매개변수는 보지 않는다(Ghostty와 같다)
pub fn private_gt(fin: u8, params: &[u8]) -> Option<Signal> {
    if params.contains(&b'?') {
        return None;
    }
    match fin {
        b'c' => Some(Signal::Da2),
        b'q' => Some(Signal::XtVersion),
        _ => None,
    }
}

/// DECRQM 답: 모드 2031. 켜짐이면 1, 꺼짐이면 2(#296)
pub fn mode_2031_report(on: bool) -> &'static str {
    if on { "\x1b[?2031;1$y" } else { "\x1b[?2031;2$y" }
}

/// 가시성 보고 모드(2033)를 켜고 끄는 CSI(`CSI ? … 2033 … h|l`). 여러 모드 중 하나여도 읽는다(#303)
pub fn visibility(fin: u8, params: &[u8]) -> Option<Signal> {
    if !matches!(fin, b'h' | b'l') {
        return None;
    }
    let text = size_param(params.strip_prefix(b"?")?)?;
    if text.contains('?') {
        return None;
    }
    text.split(';')
        .any(|p| p == "2033")
        .then_some(Signal::VisibilityReports(fin == b'h'))
}

/// DECRQM 답: 모드 5522(kitty 붙여넣기 이벤트). 켜짐이면 1, 꺼짐이면 2(#325)
pub fn mode_5522_report(on: bool) -> &'static str {
    if on { "\x1b[?5522;1$y" } else { "\x1b[?5522;2$y" }
}

/// 붙여넣기 이벤트 모드(5522)를 켜고 끄는 CSI(`CSI ? … 5522 … h|l`). 여러 모드 중 하나여도 읽는다(#325)
pub fn kitty_paste(fin: u8, params: &[u8]) -> Option<Signal> {
    if !matches!(fin, b'h' | b'l') {
        return None;
    }
    let text = size_param(params.strip_prefix(b"?")?)?;
    if text.contains('?') {
        return None;
    }
    text.split(';')
        .any(|p| p == "5522")
        .then_some(Signal::KittyPasteEvents(fin == b'h'))
}

/// 스캐너가 쥔 모드(색 구성표 알림 2031, 가시성 보고 2033, 크기 알림 2048, 붙여넣기 이벤트 5522)인가
fn scanner_mode(p: &str) -> Option<u16> {
    match p {
        "2031" => Some(2031),
        "2033" => Some(2033),
        "2048" => Some(2048),
        "5522" => Some(5522),
        _ => None,
    }
}

/// XTSAVE(`CSI ? Pm s`)·XTRESTORE(`CSI ? Pm r`) 중 스캐너가 쥔 모드의 몫. 엔진은 이 모드들을 모르므로 여기서 읽는다.
/// 여러 모드가 한 줄에 있으면 적힌 차례로 하나씩 신호를 낸다(Ghostty stream.zig 2560~2630줄) (#313)
pub fn save_restore(fin: u8, params: &[u8]) -> Vec<Signal> {
    if !matches!(fin, b's' | b'r') {
        return Vec::new();
    }
    let Some(text) = params.strip_prefix(b"?").and_then(size_param) else {
        return Vec::new();
    };
    if text.contains('?') {
        return Vec::new();
    }
    text.split(';')
        .filter_map(scanner_mode)
        .map(|m| {
            if fin == b's' {
                Signal::ModeSave(m)
            } else {
                Signal::ModeRestore(m)
            }
        })
        .collect()
}

/// 중간 바이트가 하나인 CSI: `CSI ? 2048 $ p`·`CSI ? 2031 $ p`·`CSI ? 2033 $ p`(DECRQM)만
pub fn inter(inter: u8, fin: u8, params: &[u8]) -> Option<Signal> {
    if inter != b'$' || fin != b'p' {
        return None;
    }
    match params {
        b"?2048" => Some(Signal::InBandSizeQuery),
        b"?2031" => Some(Signal::ColorSchemeModeQuery),
        b"?2033" => Some(Signal::VisibilityQuery),
        b"?5522" => Some(Signal::KittyPasteQuery),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify::Scanner;

    fn dsr(input: &str) -> Vec<Signal> {
        Scanner::default().feed(input.as_bytes())
    }

    #[test]
    fn dec_dsr_reads_each_fixed_query() {
        let cases = [
            ("\x1b[?15n", DecStatus::Printer, "\x1b[?13n"),
            ("\x1b[?25n", DecStatus::Udk, "\x1b[?21n"),
            ("\x1b[?26n", DecStatus::Keyboard, "\x1b[?27;0n"),
            ("\x1b[?55n", DecStatus::Locator, "\x1b[?50n"),
            ("\x1b[?56n", DecStatus::LocatorType, "\x1b[?57;0n"),
            ("\x1b[?62n", DecStatus::MacroSpace, "\x1b[0*{"),
            ("\x1b[?63;123n", DecStatus::MemoryChecksum(123), "\x1bP123!~0000\x1b\\"),
            ("\x1b[?63n", DecStatus::MemoryChecksum(0), "\x1bP0!~0000\x1b\\"),
            ("\x1b[?75n", DecStatus::DataIntegrity, "\x1b[?70n"),
            ("\x1b[?85n", DecStatus::MultipleSessions, "\x1b[?83n"),
        ];
        for (input, req, reply) in cases {
            assert_eq!(dsr(input), vec![Signal::DecDsr(req)], "{input:?}");
            assert_eq!(dec_status_report(req), reply, "{input:?}");
        }
    }

    #[test]
    fn dec_dsr_follows_ghostty_parameter_rules() {
        // 매개변수 0개·3개 이상·63이 아닌 질의의 둘째 매개변수·모르는 값·`?` 없는 꼴은 답이 없다
        for bad in [
            "\x1b[?n",
            "\x1b[?15;1n",
            "\x1b[?63;1;2n",
            "\x1b[?99n",
            "\x1b[?;n",
            "\x1b[?15;;n",
            "\x1b[15n",
            "\x1b[?15?n",
            "\x1b[?15$n",
            "\x1b[>15n",
            "\x1b[?26;0n",
        ] {
            assert_eq!(dsr(bad), vec![], "{bad:?}");
        }
        // 빈 끝 매개변수는 없는 것으로 친다. 값은 u16에서 포화한다(Ghostty Parser.zig)
        assert_eq!(dsr("\x1b[?15;n"), vec![Signal::DecDsr(DecStatus::Printer)]);
        assert_eq!(dsr("\x1b[?63;n"), vec![Signal::DecDsr(DecStatus::MemoryChecksum(0))]);
        assert_eq!(dsr("\x1b[?63;;n"), vec![Signal::DecDsr(DecStatus::MemoryChecksum(0))]);
        assert_eq!(
            dsr("\x1b[?63;70000n"),
            vec![Signal::DecDsr(DecStatus::MemoryChecksum(65535))]
        );
        assert_eq!(dsr("\x1b[?63;99999999999999999999n").len(), 1);
    }

    #[test]
    fn dec_dsr_keyboard_reply_matches_da1_level_two() {
        assert!(DA1_PLAIN.starts_with("\x1b[?62;") && DA1_CLIPBOARD.starts_with("\x1b[?62;"));
        assert_eq!(dec_status_report(DecStatus::Keyboard), "\x1b[?27;0n");
    }

    /// Ghostty `size_report.zig`의 testSize
    fn test_size() -> SizeInfo {
        SizeInfo {
            rows: 24,
            cols: 80,
            cell_width: 9,
            cell_height: 18,
        }
    }

    #[test]
    fn size_reports_match_ghosttys_test_values() {
        let s = test_size();
        assert_eq!(encode_size(SizeStyle::Mode2048, s), "\x1b[48;24;80;432;720t");
        assert_eq!(encode_size(SizeStyle::Csi14, s), "\x1b[4;432;720t");
        assert_eq!(encode_size(SizeStyle::Csi16, s), "\x1b[6;18;9t");
        assert_eq!(encode_size(SizeStyle::Csi18, s), "\x1b[8;24;80t");
        assert_eq!(encode_size(SizeStyle::Csi19, s), "\x1b[9;24;80t");
    }

    #[test]
    fn size_reports_do_not_overflow_at_the_maximum() {
        // Ghostty의 "encode max values for all fields": 칸 수는 16비트, 칸 픽셀은 32비트, 픽셀 합은 64비트로 계산한다
        let s = SizeInfo {
            rows: u16::MAX,
            cols: u16::MAX,
            cell_width: u32::MAX,
            cell_height: u32::MAX,
        };
        assert_eq!(
            encode_size(SizeStyle::Mode2048, s),
            "\x1b[48;65535;65535;281470681677825;281470681677825t"
        );
        assert_eq!(
            encode_size(SizeStyle::Csi14, s),
            "\x1b[4;281470681677825;281470681677825t"
        );
        assert_eq!(encode_size(SizeStyle::Csi16, s), "\x1b[6;4294967295;4294967295t");
        assert_eq!(encode_size(SizeStyle::Csi18, s), "\x1b[8;65535;65535t");
        assert_eq!(encode_size(SizeStyle::Csi19, s), "\x1b[9;65535;65535t");
    }

    #[test]
    fn fixed_replies_are_ghosttys() {
        assert_eq!(DA1_CLIPBOARD, "\x1b[?62;22;52c");
        assert_eq!(DA1_PLAIN, "\x1b[?62;22c");
        assert_eq!(DA2, "\x1b[>1;10;0c");
        assert_eq!(
            xtversion(),
            format!("\x1bP>|b-terminal {}\x1b\\", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(mode_2048_report(true), "\x1b[?2048;1$y");
        assert_eq!(mode_2048_report(false), "\x1b[?2048;2$y");
    }

    fn signals(input: &[u8]) -> Vec<Signal> {
        Scanner::default().feed(input)
    }

    #[test]
    fn device_attributes_and_xtversion_are_parsed() {
        assert_eq!(signals(b"\x1b[c"), vec![Signal::Da1]);
        assert_eq!(signals(b"\x1b[0c"), vec![Signal::Da1]);
        assert_eq!(signals(b"\x1b[>c"), vec![Signal::Da2]);
        assert_eq!(signals(b"\x1b[>0c"), vec![Signal::Da2]);
        assert_eq!(signals(b"\x1b[>q"), vec![Signal::XtVersion]);
        assert_eq!(signals(b"\x1b[>0q"), vec![Signal::XtVersion]);
    }

    #[test]
    fn other_shapes_are_not_queries() {
        // DA3(`= c`), `?` 표시, 중간 바이트, 둘 이상의 표시, 표시 앞의 숫자는 질의가 아니다
        for bad in [
            &b"\x1b[=c"[..],
            b"\x1b[?c",
            b"\x1b[ c",
            b"\x1b[>>c",
            b"\x1b[>>q",
            b"\x1b[>?c",
            b"\x1b[<c",
            b"\x1b[1>c",
            b"\x1b[> q",
        ] {
            assert_eq!(signals(bad), vec![], "{bad:?}");
        }
        // 앞의 질의가 뒤의 모양에 끌려가지 않는다
        assert_eq!(signals(b"\x1b[>c\x1b[=c"), vec![Signal::Da2]);
        assert_eq!(signals(b"\x1b[=c\x1b[>c"), vec![Signal::Da2]);
    }

    #[test]
    fn size_report_queries_need_exactly_one_parameter() {
        assert_eq!(signals(b"\x1b[16t"), vec![Signal::SizeReport(SizeStyle::Csi16)]);
        assert_eq!(signals(b"\x1b[19t"), vec![Signal::SizeReport(SizeStyle::Csi19)]);
        assert_eq!(signals(b"\x1b[16;1t"), vec![]);
        assert_eq!(signals(b"\x1b[19;1;1t"), vec![]);
        assert_eq!(signals(b"\x1b[?16t"), vec![]);
        assert_eq!(signals(b"\x1b[116t"), vec![]);
    }

    #[test]
    fn mode_2048_set_reset_and_query_are_parsed() {
        assert_eq!(signals(b"\x1b[?2048h"), vec![Signal::InBandSizeReports(true)]);
        assert_eq!(signals(b"\x1b[?2048l"), vec![Signal::InBandSizeReports(false)]);
        assert_eq!(signals(b"\x1b[?2048$p"), vec![Signal::InBandSizeQuery]);
        // 여러 모드를 한꺼번에 켜도 2031과 2048 둘 다 읽는다
        assert_eq!(
            signals(b"\x1b[?2031;2048h"),
            vec![Signal::ColorSchemeUpdates(true), Signal::InBandSizeReports(true)]
        );
        assert_eq!(signals(b"\x1b[?20480h"), vec![]);
        assert_eq!(signals(b"\x1b[?2048;1$p"), vec![]);
        assert_eq!(signals(b"\x1b[?2031$p"), vec![Signal::ColorSchemeModeQuery]);
        assert_eq!(signals(b"\x1b[?2031;1$p"), vec![]);
        assert_eq!(mode_2031_report(true), "\x1b[?2031;1$y");
        assert_eq!(mode_2031_report(false), "\x1b[?2031;2$y");
        assert_eq!(signals(b"\x1b[2048$p"), vec![]);
    }

    #[test]
    fn visibility_report_bytes_are_ghosttys() {
        assert_eq!(visibility_report(true), "\x1b[?999;1n");
        assert_eq!(visibility_report(false), "\x1b[?999;2n");
        assert_eq!(mode_2033_report(true), "\x1b[?2033;1$y");
        assert_eq!(mode_2033_report(false), "\x1b[?2033;2$y");
    }

    #[test]
    fn xtsave_xtrestore_of_scanner_modes_are_parsed() {
        assert_eq!(signals(b"\x1b[?2048s"), vec![Signal::ModeSave(2048)]);
        assert_eq!(signals(b"\x1b[?2031r"), vec![Signal::ModeRestore(2031)]);
        // 여러 모드는 적힌 차례로, 스캐너가 쥐지 않은 모드는 엔진 몫이라 건너뛴다
        assert_eq!(
            signals(b"\x1b[?7;2033;25;2031s"),
            vec![Signal::ModeSave(2033), Signal::ModeSave(2031)]
        );
        // `?`가 없는 `s`·`r`(커서 저장, DECSTBM)과 중간 바이트가 낀 것은 아니다
        assert_eq!(signals(b"\x1b[2048s\x1b[2048r\x1b[?2048$r"), vec![]);
    }

    #[test]
    fn mode_2033_set_reset_and_query_are_parsed() {
        assert_eq!(signals(b"\x1b[?2033h"), vec![Signal::VisibilityReports(true)]);
        assert_eq!(signals(b"\x1b[?2033l"), vec![Signal::VisibilityReports(false)]);
        assert_eq!(signals(b"\x1b[?2033$p"), vec![Signal::VisibilityQuery]);
        assert_eq!(
            signals(b"\x1b[?2048;2033h"),
            vec![Signal::InBandSizeReports(true), Signal::VisibilityReports(true)]
        );
        assert_eq!(signals(b"\x1b[?20330h"), vec![]);
        assert_eq!(signals(b"\x1b[?2033;1$p"), vec![]);
        assert_eq!(signals(b"\x1b[2033$p"), vec![]);
    }

    #[test]
    fn a_query_split_across_reads_is_still_one_signal() {
        let mut s = Scanner::default();
        assert_eq!(s.feed(b"\x1b[>"), vec![]);
        assert_eq!(s.feed(b"q"), vec![Signal::XtVersion]);
        assert_eq!(s.feed(b"\x1b[?20"), vec![]);
        assert_eq!(s.feed(b"48$p"), vec![Signal::InBandSizeQuery]);
    }

    #[test]
    fn queries_inside_strings_are_not_signals() {
        assert_eq!(signals(b"\x1b]0;\x1b[>q\x07"), vec![]);
        assert_eq!(signals(b"\x1b_Gq\x1b[c\x1b\\"), vec![]);
    }
}
