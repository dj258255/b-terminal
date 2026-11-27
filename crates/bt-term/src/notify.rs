//! PTY 바이트에서 VT 엔진이 다루지 않는 신호를 뽑는다: 알림(OSC 9·99·777), 작업 폴더(OSC 7), 진행 표시(OSC 9;4).
//!
//! 바이트는 읽기 조각 경계에서 잘려 온다. 스캐너는 OSC 하나가 끝날 때까지(BEL 또는 ESC \) 모아 두었다가 해석한다.
//! 터미널에 찍히는 글은 누구나 만들 수 있으므로(원격 서버, 남이 만든 파일의 cat) 길이를 자르고 제어 문자를 지운다

use std::path::PathBuf;

/// 프로그램이 터미널에 보낸 알림
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

/// 스캐너가 뽑은 신호
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signal {
    Notify(Notification),
    /// 셸이 알린 작업 폴더(OSC 7). 호스트가 이 컴퓨터인지는 받는 쪽이 본다(ssh 안의 셸도 보낸다)
    Cwd {
        host: String,
        path: PathBuf,
    },
    /// 작업 폴더를 모른다고 되돌린다(빈 OSC 7, #328). Ghostty처럼 알려진 폴더를 지운다
    CwdReset,
    /// 셸 통합 표지(OSC 133, 또는 VS Code의 OSC 633 A~D)
    Prompt(PromptMark),
    /// 셸이 알린, 지금 실행하는 명령 글(OSC 633;E)
    CommandLine(String),
    /// 진행 표시(OSC 9;4;상태;값). 상태 0은 지움, 1 보통, 2 오류, 3 끝 모름, 4 멈춤.
    /// 값이 없거나 읽을 수 없으면 `None`(Ghostty `progress: ?u8`, #764). 읽으면 0~100으로 자른다
    Progress {
        state: u8,
        percent: Option<u8>,
    },
    /// kitty 그래픽 명령(APC `ESC _ G … ESC \`)의 `G` 뒤 바이트(#76)
    Graphics(Vec<u8>),
    /// 글리프 프로토콜(APC `ESC _ 25a1 ; … ESC \`)의 `25a1;` 뒤 바이트(#779). 요청 한 줄 상한(1MiB)을 넘은 것은 내지 않는다
    Glyph(Vec<u8>),
    /// 화면 지우기(ED 2: 보이는 화면, ED 3: 스크롤백)나 초기화(RIS). 놓인 그림을 함께 지운다
    Erase {
        visible: bool,
        scrollback: bool,
    },
    /// 창 제목 보고 질의(CSI 21 t). VT 엔진은 답하지 않으므로 세션이 설정(title-report)을 보고 답한다(#151)
    TitleReport,
    /// ENQ(0x05). 세션이 설정(enquiry-response)을 보고 답한다(#176)
    Enquiry,
    /// 키보드 잠금 모드(KAM, `CSI 2 h`는 true, `CSI 2 l`은 false). 세션이 설정(vt-kam-allowed)을 보고 따른다(#176)
    Kam(bool),
    /// 창 크기 바꾸기 요청(`CSI 8 ; 행 ; 열 t`). 빠졌거나 0인 쪽은 0. 세션이 설정(vt-window-resize-allowed)을 본다(#176)
    WindowResize {
        rows: u16,
        cols: u16,
    },
    /// 색 구성표 질문(`CSI ? 996 n`). 세션이 지금 밝음·어두움을 답한다(#232)
    ColorSchemeQuery,
    /// DEC 장치 상태 질의(`CSI ? Ps n`) 가운데 고정 답을 하는 것(#513). 엔진은 답하지 않아 세션이 답한다
    DecDsr(crate::csi_query::DecStatus),
    /// 색 구성표 변경 알림 모드(DEC 모드 2031)를 켜거나(`CSI ? 2031 h`) 껐다(`CSI ? 2031 l`)(#232)
    ColorSchemeUpdates(bool),
    /// 마우스 포인터 모양(OSC 22). `Some`은 그 모양을 정하고, `None`은 이름이 비어 있어 되돌린다(#239)
    MouseShape(Option<crate::mouse_shape::MouseShape>),
    /// DECRQCRA(`CSI Pi ; Pg ; Pt ; Pl ; Pb ; Pr * y`): 화면 사각 영역의 체크섬을 묻는다. 세션이 설정(vt-xt-checksum-report)을 보고 답한다(#259)
    XtChecksumReport(crate::xt_checksum::Request),
    /// XTCHECKSUM(`CSI Ps # y`): 체크섬 계산법을 고른다. 아래 다섯 비트만 남긴다. 설정이 꺼져 있으면 세션이 무시한다(#259)
    XtChecksumExtension(u8),
    /// 소프트 리셋(DECSTR, `CSI ! p`). 체크섬 계산법이 설정의 기본으로 돌아간다(#259)
    SoftReset,
    /// DA1(`CSI c`). 세션이 설정(clipboard-write)을 보고 답한다(#264)
    Da1,
    /// DA2(`CSI > c`)(#264)
    Da2,
    /// XTVERSION(`CSI > q`)(#264)
    XtVersion,
    /// 크기 보고 질문(`CSI 16 t`, `CSI 19 t`). 14 t·18 t는 VT 엔진이 답한다(#264)
    SizeReport(crate::csi_query::SizeStyle),
    /// 크기 알림 모드(DEC 모드 2048)를 켜거나(`CSI ? 2048 h`) 껐다(`CSI ? 2048 l`)(#264)
    InBandSizeReports(bool),
    /// 모드 2048의 DECRQM(`CSI ? 2048 $ p`)(#264)
    InBandSizeQuery,
    /// 모드 2031의 DECRQM(`CSI ? 2031 $ p`). 켜 둔 상태는 세션이 안다(#296)
    ColorSchemeModeQuery,
    /// 가시성 보고 모드(DEC 모드 2033)를 켜거나(`CSI ? 2033 h`) 껐다(`CSI ? 2033 l`)(#303)
    VisibilityReports(bool),
    /// 모드 2033의 DECRQM(`CSI ? 2033 $ p`)(#303)
    VisibilityQuery,
    /// 보이는지 묻기(`CSI ? 998 n`, Ghostty 1.4). 모드 2033과 상관없이 지금 상태를 답한다(#675)
    VisibilityStatusQuery,
    /// XTSAVE(`CSI ? Pm s`)가 스캐너가 쥔 모드(2031·2033·2048) 하나를 저장하라고 한다(#313). 엔진은 이 모드를 모른다
    ModeSave(u16),
    /// XTRESTORE(`CSI ? Pm r`)가 스캐너가 쥔 모드 하나를 복원하라고 한다. 값은 세션이 쥔다(#313)
    ModeRestore(u16),
    /// DCS 질의(DECRQSS, XTGETTCAP). 세션이 그 자리의 상태로 답한다(#265)
    DcsQuery(crate::dcs::Query),
    /// kitty 색 프로토콜(OSC 21)의 요청들. `bel`은 요청이 BEL로 끝났는가(답도 같은 끝맺음으로 한다)(#266)
    KittyColor {
        requests: Vec<crate::kitty_color::Request>,
        bel: bool,
    },
    /// kitty 클립보드 프로토콜(OSC 5522)의 패킷 하나: `5522;` 뒤의 `메타데이터[;페이로드]`. 해석은 `kitty_clip`이 한다(#325)
    KittyClipboard {
        packet: String,
        bel: bool,
    },
    /// 붙여넣기 이벤트 모드(DEC 모드 5522)를 켜거나(`CSI ? 5522 h`) 껐다(`CSI ? 5522 l`)(#325)
    KittyPasteEvents(bool),
    /// 모드 5522의 DECRQM(`CSI ? 5522 $ p`)(#325)
    KittyPasteQuery,
    /// kitty 알림(OSC 99)의 지원 질의(`p=?`)에 돌려줄 답 바이트(끝맺음까지). 세션이 PTY에 쓴다(#561)
    KittyNotifyReply(Vec<u8>),
    /// OSC 133의 명령 하나를 옵션까지 읽은 것(#329). 같은 OSC에서 `Prompt`보다 먼저 나온다. 세션이 엔진 줄 표시·fresh-line에 적용한다
    Semantic(crate::osc133::SemanticPrompt),
    /// 프로그램 상태 보고(OSC 7501)의 지원 질의. `bel`은 질의가 BEL로 끝났는가(답도 같은 끝맺음으로 한다)(#618)
    ProgramStatusQuery {
        bel: bool,
    },
    /// 검증을 통과한 프로그램 상태 보고(OSC 7501). 세션이 칸의 기록에 적용한다(#618)
    ProgramStatus(Box<crate::program_status::Report>),
}

/// OSC 22 이름의 길이 상한(바이트). 가장 긴 이름("bottom_right_corner")보다 넉넉하다
const MAX_SHAPE_NAME: usize = 64;

/// `22;이름`의 이름을 신호로 바꾼다. 비어 있으면 되돌림, 모르거나 너무 길거나 제어 문자가 있으면 무시한다
fn mouse_shape_signal(name: &str) -> Option<Signal> {
    if name.is_empty() {
        return Some(Signal::MouseShape(None));
    }
    if name.len() > MAX_SHAPE_NAME || name.chars().any(char::is_control) {
        return None;
    }
    crate::mouse_shape::MouseShape::from_name(name).map(|s| Signal::MouseShape(Some(s)))
}

/// 셸 통합 표지. <https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md>
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptMark {
    /// A: 프롬프트가 시작된다
    PromptStart,
    /// B: 사람이 명령을 치기 시작한다(프롬프트 끝)
    CommandStart,
    /// C: 명령이 실행된다(출력 시작)
    CommandExecuted,
    /// D: 명령이 끝났다(종료 코드)
    CommandFinished(Option<i32>),
}

/// OSC 하나의 상한. 넘으면 버린다(상한은 모으기 전에 본다)
const MAX_OSC: usize = 8 * 1024;
/// OSC 133에서 `133;` 뒤로 받는 바이트 상한(Ghostty `osc.zig`의 `MAX_BUF`). 넘으면 명령 전체를 버린다(#822)
const MAX_OSC133_TRAILING: usize = 2048;
pub(crate) const MAX_TITLE: usize = 120;
pub(crate) const MAX_BODY: usize = 500;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum St {
    #[default]
    Ground,
    Esc,
    /// `ESC` 뒤 중간 바이트(0x20~0x2F)를 지난 Escape(`ESC (`, `ESC #` 등, #793): 끝 바이트까지. 8비트 C1은 엔진 표를 따른다
    EscInter,
    Osc,
    /// OSC 안에서 ESC를 봤다(다음이 \면 끝)
    OscEsc,
    /// 상한을 넘은 OSC를 끝까지 버리는 중
    Skip,
    SkipEsc,
    /// SOS(`ESC X`)·PM(`ESC ^`) 안: 엔진처럼 ST·CAN·SUB·ESC까지 버린다(#598)
    SosPm,
    /// APC(`ESC _`) 안
    Apc,
    ApcEsc,
    /// CSI(`ESC [`) 안: 화면 지우기(ED)와 제목 보고(CSI 21 t)만 알아본다
    Csi,
    /// DCS(`ESC P`)의 머리: 매개변수·중간 바이트를 지나 끝 바이트까지(#265)
    DcsHdr,
    /// DCS 본문: DECRQSS·XTGETTCAP이면 모으고, 아니면 ST까지 흘려보낸다
    DcsBody,
    /// DCS 안에서 ESC를 봤다(다음이 `\`면 끝)
    DcsEsc,
    /// 엔진이 무시하는 DCS 머리(중간 바이트 뒤의 매개변수, 첫 자리가 아닌 `< = > ?`): ESC·CAN·SUB까지 버린다.
    /// 0x9C로는 끝나지 않는다(vte `DcsIgnore`는 `anywhere`만 본다)
    DcsIgnore,
    /// tmux 패스스루 안쪽의 Escape(겹친 ESC 뒤, #603): 엔진처럼 C0를 실행하고(ENQ에는 답한다) 0x7F·0x80 이상·ESC에 머문다.
    /// `X ^ _ ] P`는 신호 없는 문자열로 들어가고 CAN·SUB는 Ground이며, 그 밖의 바이트(`[ c` 등)는 안쪽 글이라 읽지 않고 Ground로 돌린다.
    /// 조용한 것은 겹친 ESC 바로 뒤의 첫 시퀀스 하나다: 끝나면 Ground라 패스스루 안의 둘째 시퀀스부터는 보통처럼 읽는다(엔진도 처리한다)
    EscQuiet,
    /// 안쪽 SOS·PM·APC: 엔진처럼 ESC(→ `EscQuiet`)·CAN·SUB까지 신호 없이 버린다
    StrQuiet,
    /// 안쪽 OSC: 엔진처럼 BEL·CAN·SUB·ESC(→ `EscQuiet`)까지 신호 없이 버린다
    OscQuiet,
}

/// 우리가 답하는 DCS의 종류
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DcsKind {
    Decrqss,
    XtGetTcap,
}

/// DCS 머리를 모으는 상한. 머리는 `$q`나 `+q` 몇 글자뿐이라 넘으면 우리 모양이 아니다
const MAX_DCS_HEADER: usize = 16;

/// 조각난 바이트를 이어 가며 신호를 찾는다
#[derive(Debug, Default)]
pub struct Scanner {
    st: St,
    buf: Vec<u8>,
    /// APC 바이트(kitty 그래픽 조각). OSC와 따로 둔다
    apc: Vec<u8>,
    /// APC가 상한을 넘어 버리는 중
    apc_skip: bool,
    /// CSI 매개변수(짧게만)
    csi: Vec<u8>,
    /// 이 CSI에 중간 바이트(0x20~0x2f)나 `: < = >`가 끼었다: 우리가 아는 모양이 아니다
    csi_taint: bool,
    /// 이 CSI의 중간 바이트(0x20~0x2f)가 딱 하나이고 그 뒤로 매개변수가 오지 않았을 때 그 바이트: `* y`·`# y`·`! p`를 알아보는 데 쓴다(#259)
    csi_inter: Option<u8>,
    /// 중간 바이트가 둘 이상이거나, 중간 바이트 뒤에 매개변수가 오거나, `: < = >`가 끼었다: 중간 바이트로 읽는 신호도 아니다
    csi_bad: bool,
    /// 이 CSI는 맨 앞 매개변수 자리에 `>` 하나만 끼었다(`CSI > q`·`CSI > c`를 알아보는 데 쓴다, #264)
    csi_gt: bool,
    /// DCS 질의의 머리(`ESC P` 뒤 끝 문자까지)와 본문 바이트(#265). OSC·APC와 따로 둔다
    dcs: Vec<u8>,
    /// 지금 DCS가 우리가 답하는 것이면 그 종류. 아니면 ST까지 버린다
    dcs_kind: Option<DcsKind>,
    /// 본문이 상한을 넘어 버리는 중(DECRQSS는 두 바이트, XTGETTCAP은 `MAX_XTGETTCAP`)
    dcs_over: bool,
    /// 이 DCS 안에서 ESC가 겹쳐 왔다(tmux 패스스루가 안의 ESC를 두 번 쓴다). 그 뒤 `\`가 아닌 바이트는 안쪽 글이라 `EscQuiet`로 따라가며 신호를 내지 않는다(#603)
    dcs_double: bool,
    /// 이 DCS는 tmux 패스스루 안쪽에서 열렸다(`EscQuiet`의 `P`). 질의에 답하지 않고 끝까지 버린다(#603)
    dcs_silent: bool,
    /// 이 DCS 머리에 중간 바이트(0x20~0x2f)가 나왔다. 머리 상한과 상관없이 센다
    dcs_inter: bool,
    /// 시험 전용: 켜면 끝 바이트 거르기(#938)를 건너뛰어 모든 CSI가 옛 길로 간다. 옛 길과 새 길의 결과를 견주는 데 쓴다
    #[cfg(test)]
    pub(crate) no_fast_csi: bool,
    /// kitty 알림(OSC 99)의 쌓인 조각. 스캐너는 세션마다 하나라 쌓임도 세션마다 하나다(#561)
    pub(crate) kitty_notify: crate::kitty_notify::Assembler,
}

/// CSI 매개변수를 모으는 상한. 이만큼 차면 잘린 것으로 보고 믿지 않는다
const MAX_CSI: usize = 40;

/// 엔진 vte가 모으는 매개변수의 상한(`params::MAX_PARAMS`). 세미콜론이 이만큼 나오면 엔진은 그 CSI를 `ignore`로 dispatch한다(#605).
/// 엔진은 `:`(하위 매개변수)도 같은 칸을 채우지만, 스캐너는 `:`가 끼면 `csi_taint`로 신호를 내지 않으므로 세미콜론만 세면 된다.
/// `:`를 받는 신호를 더하면 `:`도 함께 세어야 한다
const MAX_PARAMS: usize = 32;

/// 글리프 프로토콜 APC의 머리(`ESC _` 뒤)
const GLYPH_APC_PREFIX: &[u8] = b"25a1;";

/// APC 하나의 상한. kitty는 조각을 4096바이트로 보내지만 한 번에 보내는 도구도 있다
const MAX_APC: usize = 8 * 1024 * 1024;

/// 8비트 C1이 문자열(APC·SOS·PM)과 Escape·CSI·DCS 머리 안에서 하는 일. Ghostty 1.4.0 `parse_table.zig` 51~66줄
/// "어디서나" 전이와 같다(#776·#793). 엔진(vendor/vte `anywhere`)이 같은 표를 쓴다.
/// OSC 문자열과 DCS 통과·무시는 표가 0x80~0xFF를 자료로 덮어 여기에 오지 않는다
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StrC1 {
    /// 0x80~0x8F·0x91~0x97·0x99·0x9A(실행)와 0x9C(ST): Ground로 간다
    Ground,
    /// 0x9B
    Csi,
    /// 0x90
    Dcs,
    /// 0x9D
    Osc,
    /// 0x9F(APC)
    Apc,
    /// 0x98(SOS)·0x9E(PM)
    SosPm,
}

/// 0x80~0x9F 한 바이트가 상태를 바꾸는 곳. 0xA0 이상은 표에 없어 `None`(머문다)
fn c1(b: u8) -> Option<StrC1> {
    match b {
        0x80..=0x8f | 0x91..=0x97 | 0x99 | 0x9a | 0x9c => Some(StrC1::Ground),
        0x9b => Some(StrC1::Csi),
        0x90 => Some(StrC1::Dcs),
        0x9d => Some(StrC1::Osc),
        0x9f => Some(StrC1::Apc),
        0x98 | 0x9e => Some(StrC1::SosPm),
        _ => None,
    }
}

/// 문자열 안의 C1 한 바이트가 그 문자열을 끝내면 `Some`. 문자열 시작 C1(0x98·0x9E·0x9F)은 같은 상태라 머문다
fn str_c1(b: u8) -> Option<StrC1> {
    c1(b).filter(|k| !matches!(k, StrC1::Apc | StrC1::SosPm))
}

impl Scanner {
    /// 시험에서 거르기를 끈 스캐너인가(제품 빌드에서는 늘 거짓)
    fn slow_csi(&self) -> bool {
        #[cfg(test)]
        {
            self.no_fast_csi
        }
        #[cfg(not(test))]
        {
            false
        }
    }

    /// `ESC [`나 8비트 CSI(0x9B) 뒤의 CSI 상태를 연다
    fn begin_csi(&mut self) -> St {
        self.csi.clear();
        self.csi_taint = false;
        self.csi_inter = None;
        self.csi_bad = false;
        self.csi_gt = false;
        St::Csi
    }

    /// `ESC P`나 8비트 DCS(0x90) 뒤의 DCS 머리 상태를 연다. `silent`는 tmux 패스스루 안쪽 DCS(신호를 내지 않음)
    fn begin_dcs(&mut self, silent: bool) -> St {
        self.dcs.clear();
        self.dcs_kind = None;
        self.dcs_over = false;
        self.dcs_double = false;
        self.dcs_silent = silent;
        self.dcs_inter = false;
        St::DcsHdr
    }

    /// C1로 옮겨 간 뒤의 상태. `quiet`는 tmux 패스스루 안쪽이다(신호를 내지 않고, 조용히 따라갈 CSI 상태가 없어 Ground로 둔다)
    fn leave_c1(&mut self, to: StrC1, quiet: bool) -> St {
        match to {
            StrC1::Ground => St::Ground,
            StrC1::Csi if quiet => St::Ground,
            StrC1::Csi => self.begin_csi(),
            StrC1::Dcs => self.begin_dcs(quiet),
            StrC1::Osc if quiet => St::OscQuiet,
            StrC1::Osc => {
                self.buf.clear();
                St::Osc
            }
            StrC1::Apc if quiet => St::StrQuiet,
            StrC1::Apc => {
                self.apc.clear();
                self.apc_skip = false;
                St::Apc
            }
            StrC1::SosPm if quiet => St::StrQuiet,
            StrC1::SosPm => St::SosPm,
        }
    }

    /// 문자열(APC·SOS·PM)을 C1로 끝낸 뒤의 상태
    fn leave_string(&mut self, end: StrC1) -> St {
        self.leave_c1(end, false)
    }

    /// 바이트를 먹이고 끝난 OSC에서 뽑은 신호를 돌려준다
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Signal> {
        self.feed_at(bytes).into_iter().map(|(_, s)| s).collect()
    }

    /// `feed`와 같고, 신호마다 그 OSC가 끝난 바로 뒤의 자리(이번 조각 안)를 함께 돌려준다.
    /// 읽기 스레드가 그 자리까지 VT에 먹인 뒤 커서 줄을 적거나, 명령 출력을 C와 D 사이에서 잘라 낸다
    pub fn feed_at(&mut self, bytes: &[u8]) -> Vec<(usize, Signal)> {
        let mut out = Vec::new();
        for (i, &b) in bytes.iter().enumerate() {
            // 문자열(OSC·APC·DCS) 안에서 ESC 뒤가 `\`가 아니면 그 문자열은 거기서 끝나고, 이 바이트는 새 이스케이프의 둘째 바이트다
            // (엔진 vte와 Ghostty `Parser.zig`가 같다). OSC·APC는 끝난 것으로 내보내고 DCS는 버린다(#595)
            match self.st {
                St::OscEsc if b != b'\\' => {
                    self.finish(&mut out, i, false);
                    self.st = St::Esc;
                }
                St::ApcEsc if b != b'\\' => {
                    self.finish_apc(&mut out, i);
                    self.st = St::Esc;
                }
                St::DcsEsc if b != b'\\' && b != 0x1b => {
                    self.dcs = Vec::new();
                    self.dcs_kind = None;
                    // 겹친 ESC(tmux 패스스루) 뒤의 바이트는 안쪽 글이라 신호를 내지 않는다. 다만 엔진처럼 Escape에 있는 것으로 따라가
                    // ENQ와 문자열 경계(`X ^ _ ] P`)는 엔진과 같게 읽는다(#603). 안쪽 DCS(`dcs_silent`) 안의 ESC도 같다
                    self.st = if self.dcs_double || self.dcs_silent {
                        St::EscQuiet
                    } else {
                        St::Esc
                    };
                }
                St::SkipEsc if b != b'\\' => self.st = St::Esc,
                _ => {}
            }
            self.st = match (self.st, b) {
                (St::Ground, 0x1b) => St::Esc,
                (St::Ground, 0x9d) => {
                    // 8비트 OSC는 UTF-8 글의 이어지는 바이트와 겹치므로 보지 않는다
                    St::Ground
                }
                (St::Ground, 0x05) => {
                    out.push((i + 1, Signal::Enquiry));
                    St::Ground
                }
                (St::Ground, _) => St::Ground,
                (St::Esc, b']') => {
                    self.buf.clear();
                    St::Osc
                }
                (St::Esc, b'_') => {
                    self.apc.clear();
                    self.apc_skip = false;
                    St::Apc
                }
                // SOS·PM은 엔진이 문자열 상태(무시)로 들어간다(vte `advance_esc` 359·377줄). 그 안의 ENQ는 엔진이 실행하지 않으니 우리도 읽지 않는다.
                // APC(`ESC _`)는 kitty 그래픽 때문에 위에서 따로 다룬다(#598)
                (St::Esc, b'X' | b'^') => St::SosPm,
                // ESC는 문자열을 끝내고 새 이스케이프를 연다(`ESC \`의 `\`는 Escape 상태에서 Ground로 간다). CAN·SUB는 Ground
                (St::SosPm, 0x1b) => St::Esc,
                (St::SosPm, 0x18 | 0x1a) => St::Ground,
                // 8비트 C1은 문자열을 끝내거나 다른 문자열을 연다(APC와 같은 상태, #776)
                (St::SosPm, 0x80..=0x9f) => match str_c1(b) {
                    Some(end) => self.leave_string(end),
                    None => St::SosPm,
                },
                (St::SosPm, _) => St::SosPm,
                (St::Esc, b'[') => self.begin_csi(),
                (St::Esc, b'P') => self.begin_dcs(false),
                (St::EscQuiet, b'P') => self.begin_dcs(true),
                (St::DcsHdr, 0x1b) => St::DcsEsc,
                // b-terminal(#793): DCS 머리(진입·매개변수·중간 바이트)의 8비트 C1도 표의 "어디서나" 전이다. 끝 바이트 전이라
                // 아직 연 DCS가 아니라서 모은 머리를 버린다
                (St::DcsHdr, 0x80..=0x9f) => match c1(b) {
                    Some(to) => {
                        let quiet = self.dcs_silent;
                        self.dcs = Vec::new();
                        self.dcs_kind = None;
                        self.leave_c1(to, quiet)
                    }
                    None => St::DcsHdr,
                },
                (St::DcsHdr | St::DcsBody | St::DcsIgnore, 0x18 | 0x1a) => {
                    self.dcs = Vec::new();
                    self.dcs_kind = None;
                    St::Ground
                }
                // vte: 중간 바이트 뒤의 0x30~0x3F(`advance_dcs_intermediate` 291줄)와
                // 첫 자리가 아닌 0x3C~0x3F(`advance_dcs_param` 309줄)는 무시 상태로 간다
                (St::DcsHdr, 0x20..=0x3f)
                    if (self.dcs_inter && b >= 0x30)
                        || ((0x3c..=0x3f).contains(&b) && (self.dcs_over || !self.dcs.is_empty())) =>
                {
                    self.dcs = Vec::new();
                    self.dcs_kind = None;
                    St::DcsIgnore
                }
                (St::DcsHdr, 0x20..=0x3f) => {
                    if b <= 0x2f {
                        self.dcs_inter = true;
                    }
                    if self.dcs.len() >= MAX_DCS_HEADER {
                        self.dcs_over = true;
                    } else {
                        self.dcs.push(b);
                    }
                    St::DcsHdr
                }
                (St::DcsHdr, 0x40..=0x7e) => {
                    // 중간 바이트가 `$` 하나나 `+` 하나이고 끝 바이트가 `q`일 때만 우리 것이다. 숫자 매개변수는 Ghostty처럼 보지 않는다.
                    // 맨 앞의 `< = > ?`는 엔진과 Ghostty가 중간 바이트로 모으므로 함께 센다
                    let inter: Vec<u8> = self
                        .dcs
                        .iter()
                        .enumerate()
                        .filter(|&(k, c)| (0x20..=0x2f).contains(c) || (k == 0 && (0x3c..=0x3f).contains(c)))
                        .map(|(_, &c)| c)
                        .collect();
                    self.dcs_kind = match (self.dcs_silent, self.dcs_over, inter.as_slice(), b) {
                        (false, false, b"$", b'q') => Some(DcsKind::Decrqss),
                        (false, false, b"+", b'q') => Some(DcsKind::XtGetTcap),
                        _ => None,
                    };
                    self.dcs.clear();
                    self.dcs_over = false;
                    St::DcsBody
                }
                (St::DcsHdr, _) => St::DcsHdr,
                (St::DcsIgnore, 0x1b) => St::DcsEsc,
                (St::DcsIgnore, _) => St::DcsIgnore,
                // b-terminal(#792): 통과 상태에서 0x80~0xFF는 모두 자료이고 0x9C도 DCS를 끝내지 않는다(엔진과 Ghostty 1.4.0
                // `parse_table.zig` 시험 `dcs_passthrough: high bytes are payload data`). 끝은 `ESC \`와 CAN·SUB·ESC뿐이다.
                (St::DcsBody, 0x1b) => St::DcsEsc,
                (St::DcsBody, b) => {
                    if let Some(kind) = self.dcs_kind.filter(|_| !self.dcs_over) {
                        let cap = match kind {
                            DcsKind::Decrqss => 2,
                            DcsKind::XtGetTcap => crate::dcs::MAX_XTGETTCAP,
                        };
                        if self.dcs.len() >= cap {
                            // 상한은 모으기 전에 본다. 넘으면 이 DCS는 답하지 않는다
                            self.dcs = Vec::new();
                            self.dcs_over = true;
                        } else {
                            self.dcs.push(b);
                        }
                    }
                    St::DcsBody
                }
                (St::DcsEsc, b'\\') => {
                    self.finish_dcs(&mut out, i + 1);
                    St::Ground
                }
                // tmux 패스스루 안의 ESC는 두 번 쓴다: 겹친 ESC는 아직 DCS 안이다.
                // 일부러 엔진과 다르다: 엔진은 `ESC ESC`의 둘째 ESC를 새 이스케이프로 읽어 그 뒤 `[ c`를 DA1로 처리하지만,
                // 우리는 겹친 ESC 뒤의 바이트를 tmux가 감싼 안쪽 글로 보고(위 `dcs_double`) 읽지 않는다. 질의 답과 알림은 빠지는 쪽이다.
                // 다만 ENQ와 문자열 경계는 엔진과 같게 읽는다(#603): 겹친 ESC 뒤는 `EscQuiet`가 되어 C0를 실행하고(ENQ는 Enquiry),
                // `X ^ _ ] P`이면 엔진처럼 그 문자열이 끝날 때까지 신호 없이 버린다. 그 밖의 바이트(`[` 등)만 Ground로 돌려 읽지 않는다
                (St::DcsEsc, 0x1b) => {
                    self.dcs_double = true;
                    St::DcsEsc
                }
                (St::DcsEsc, _) => St::Ground,
                // 안쪽 Escape(#603). 엔진 `advance_esc`와 같게 C0는 실행하고(ENQ만 신호), 0x7F·0x80 이상은 무시하고 머문다
                (St::EscQuiet, 0x05) => {
                    out.push((i + 1, Signal::Enquiry));
                    St::EscQuiet
                }
                (St::EscQuiet, 0x18 | 0x1a) => St::Ground,
                // b-terminal(#793): 안쪽 Escape의 8비트 C1도 엔진의 표를 따른다
                (St::EscQuiet, 0x80..=0x9f) => match c1(b) {
                    Some(to) => self.leave_c1(to, true),
                    None => St::EscQuiet,
                },
                (St::EscQuiet, 0x00..=0x17 | 0x19 | 0x1b..=0x1f | 0x7f..=0xff) => St::EscQuiet,
                (St::EscQuiet, b'X' | b'^' | b'_') => St::StrQuiet,
                (St::EscQuiet, b']') => St::OscQuiet,
                (St::EscQuiet, _) => St::Ground,
                (St::StrQuiet, 0x1b) => St::EscQuiet,
                (St::StrQuiet, 0x18 | 0x1a) => St::Ground,
                // 안쪽 문자열도 엔진처럼 C1로 끝난다(#776)
                (St::StrQuiet, 0x80..=0x9f) => match str_c1(b) {
                    None => St::StrQuiet,
                    Some(to) => self.leave_c1(to, true),
                },
                (St::StrQuiet, _) => St::StrQuiet,
                (St::OscQuiet, 0x1b) => St::EscQuiet,
                (St::OscQuiet, 0x07 | 0x18 | 0x1a) => St::Ground,
                (St::OscQuiet, _) => St::OscQuiet,
                // ENQ는 VT 엔진도 ESC 사이에서 실행한다
                (St::Esc, 0x05) => {
                    out.push((i + 1, Signal::Enquiry));
                    St::Esc
                }
                (St::Esc, b'c') => {
                    out.push((
                        i + 1,
                        Signal::Erase {
                            visible: true,
                            scrollback: true,
                        },
                    ));
                    St::Ground
                }
                (St::Esc, 0x1b) => St::Esc,
                // CAN·SUB는 Ground로 간다(vte `advance_esc` 372줄)
                (St::Esc, 0x18 | 0x1a) => St::Ground,
                // 그 밖의 C0는 엔진이 실행하고 Escape에 머문다(vte `advance_esc` 342줄, Ghostty의 escape 상태). `ESC LF [ c`의 DA1을 읽어야 한다(#598)
                (St::Esc, 0x00..=0x17 | 0x19 | 0x1c..=0x1f) => St::Esc,
                // 0x7F와 0x80 이상은 엔진이 무시하고 Escape에 머문다(vte `advance_esc`의 `_ => ()`). Ground로 보내면 뒤의 ENQ에 더 답한다(#603)
                (St::Esc, 0x20..=0x2f) => St::EscInter,
                // EscapeIntermediate: 엔진처럼 C0를 실행하고(ENQ에는 답한다) 중간 바이트를 더 모으며 끝 바이트(0x30~0x7E)에서 Ground로 간다
                (St::EscInter, 0x1b) => St::Esc,
                (St::EscInter, 0x18 | 0x1a) => St::Ground,
                (St::EscInter, 0x05) => {
                    out.push((i + 1, Signal::Enquiry));
                    St::EscInter
                }
                (St::EscInter, 0x30..=0x7e) => St::Ground,
                (St::EscInter, 0x80..=0x9f) => match c1(b) {
                    Some(to) => self.leave_c1(to, false),
                    None => St::EscInter,
                },
                (St::EscInter, _) => St::EscInter,
                // b-terminal(#793): Escape 안의 8비트 C1은 표의 "어디서나" 전이다. 0x7F와 0xA0 이상은 머문다
                (St::Esc, 0x80..=0x9f) => match c1(b) {
                    Some(to) => self.leave_c1(to, false),
                    None => St::Esc,
                },
                (St::Esc, 0x7f..=0xff) => St::Esc,
                (St::Esc, _) => St::Ground,
                (St::Apc, 0x1b) => St::ApcEsc,
                // CAN·SUB는 문자열을 끝내고 Ground로 돌린다. 끝난 APC는 내보낸다(Ghostty `Parser.zig` 281줄 `apc_end`)(#595)
                (St::Apc, 0x18 | 0x1a) => {
                    self.finish_apc(&mut out, i + 1);
                    St::Ground
                }
                // 8비트 C1: 0x80~0x9F 가운데 문자열을 끝내는 것은 끝난 APC를 내보낸다. 앱(Ghostty `stream_handler.zig` `apcEnd`)은
                // 끝난 까닭(`terminated`)을 보지 않고 실행한다(CAN·SUB와 같다). 머무는 C1과 0xA0 이상은 자료가 아니다(#776)
                (St::Apc, 0x80..=0xff) => match str_c1(b) {
                    Some(end) => {
                        self.finish_apc(&mut out, i + 1);
                        self.leave_string(end)
                    }
                    None => St::Apc,
                },
                (St::Apc, b) => {
                    // 글리프 APC는 Ghostty처럼 식별자 뒤 1MiB까지만 모은다(`apc.zig` 409줄). 넘으면 통째로 버린다
                    if self.apc.len() >= MAX_APC
                        || (self.apc.len() >= GLYPH_APC_PREFIX.len() + crate::glyph::MAX_REQUEST
                            && self.apc.starts_with(GLYPH_APC_PREFIX))
                    {
                        self.apc = Vec::new();
                        self.apc_skip = true;
                    } else if !self.apc_skip {
                        self.apc.push(b);
                    }
                    St::Apc
                }
                (St::ApcEsc, b'\\') => {
                    self.finish_apc(&mut out, i + 1);
                    St::Ground
                }
                (St::ApcEsc, _) => St::Ground,
                (St::Csi, b'0'..=b'9' | b';' | b'?') => {
                    // 중간 바이트 뒤의 매개변수는 VT 문법 밖이다
                    if self.csi_inter.is_some() {
                        self.csi_bad = true;
                    }
                    if self.csi.len() < MAX_CSI {
                        self.csi.push(b);
                    }
                    St::Csi
                }
                (St::Csi, 0x1b) => St::Esc,
                // b-terminal(#793): CSI 안의 8비트 C1도 표의 "어디서나" 전이다(`ESC [ 1 0x9B 2 m`은 `2m`)
                (St::Csi, 0x80..=0x9f) => match c1(b) {
                    Some(to) => self.leave_c1(to, false),
                    None => St::Csi,
                },
                // CAN·SUB는 CSI를 끊는다(엔진도 그렇다)(#595)
                (St::Csi, 0x18 | 0x1a) => St::Ground,
                (St::Csi, 0x05) => {
                    out.push((i + 1, Signal::Enquiry));
                    St::Csi
                }
                (St::Csi, 0x40..=0x7e) => {
                    // 끝 바이트가 신호가 될 수 있는 것(`CSI_WATCHED_FINALS`)이 아니면 아래 어느 길도 신호를 내지 않는다. 그 밖의 CSI(특히 SGR `m`)는
                    // 매개변수 문자열을 만들지도 읽지도 않고 지나간다(#938)
                    if csi_final_is_watched(b) || self.slow_csi() {
                        // 엔진이 `ignore` 없이 dispatch할 수 있는 모양이어야 신호를 낸다: 모은 매개변수가 상한 안이고 세미콜론이 31개 이하다
                        // (엔진은 매개변수 32개를 넘으면 `ignore`로 dispatch하고 `ansi.rs`가 버린다, #605)
                        let fits =
                            self.csi.len() < MAX_CSI && self.csi.iter().filter(|&&c| c == b';').count() < MAX_PARAMS;
                        if !self.csi_taint && fits {
                            if let Some(sig) = csi_signal(b, &self.csi) {
                                out.push((i + 1, sig));
                            }
                            if let Some(sig) = crate::csi_query::plain(b, &self.csi) {
                                out.push((i + 1, sig));
                            }
                            if let Some(sig) = crate::csi_query::visibility(b, &self.csi) {
                                out.push((i + 1, sig));
                            }
                            if let Some(sig) = crate::csi_query::kitty_paste(b, &self.csi) {
                                out.push((i + 1, sig));
                            }
                            for sig in crate::csi_query::save_restore(b, &self.csi) {
                                out.push((i + 1, sig));
                            }
                            // 화면 지우기(ED)도 다른 신호처럼 중간 바이트·표시가 없는 CSI에서만 읽는다. 엔진은 중간 바이트 뒤의 매개변수·
                            // 매개변수 뒤의 표시를 CsiIgnore로 보내고, 맨 앞의 `< = >`나 매개변수 뒤의 중간 바이트는 `ED`가 아닌 다른 명령으로 dispatch한다(#605).
                            // `?`가 붙은 `CSI ? 2 J`(DECSED)는 모은 매개변수가 `?2`라 여기서 읽지 않는다
                            if b == b'J' {
                                match self.csi.as_slice() {
                                    // ED 22(scroll_complete)도 Ghostty는 화면 지우기와 같이 그림을 정리한다(`clearScreen`, #799)
                                    b"2" | b"22" => out.push((
                                        i + 1,
                                        Signal::Erase {
                                            visible: true,
                                            scrollback: false,
                                        },
                                    )),
                                    b"3" => out.push((
                                        i + 1,
                                        Signal::Erase {
                                            visible: false,
                                            scrollback: true,
                                        },
                                    )),
                                    _ => {}
                                }
                            }
                        } else if self.csi_gt && self.csi_inter.is_none() && fits {
                            if let Some(sig) = crate::csi_query::private_gt(b, &self.csi) {
                                out.push((i + 1, sig));
                            }
                        } else if let (Some(inter), false, true) = (self.csi_inter, self.csi_bad, fits) {
                            if let Some(sig) = csi_inter_signal(inter, b, &self.csi) {
                                out.push((i + 1, sig));
                            }
                            if let Some(sig) = crate::csi_query::inter(inter, b, &self.csi) {
                                out.push((i + 1, sig));
                            }
                        }
                    }
                    St::Ground
                }
                (St::Csi, 0x20..=0x2f | 0x3a | 0x3c..=0x3e) => {
                    // `>`가 맨 앞의 첫 표시일 때만 DA2·XTVERSION 후보. 뒤에 표시가 또 끼면 아니다
                    self.csi_gt = !self.csi_taint && b == b'>' && self.csi.is_empty();
                    self.csi_taint = true;
                    if (0x20..=0x2f).contains(&b) && self.csi_inter.is_none() {
                        self.csi_inter = Some(b);
                    } else {
                        self.csi_bad = true;
                    }
                    St::Csi
                }
                (St::Csi, _) => St::Csi,
                (St::Osc, 0x07) => {
                    self.finish(&mut out, i + 1, true);
                    St::Ground
                }
                (St::Osc, 0x1b) => St::OscEsc,
                // CAN·SUB는 OSC를 끝내되 내보내지 않는다(Ghostty 1.4.0 `osc.zig` `Parser.end`가 끝 바이트가 CAN·SUB면 null, xterm과 같음)(#763).
                // 끊긴 문자열을 알림·폴더·클립보드로 쓰지 않는다. 엔진(vendor/vte)도 같다
                (St::Osc, 0x18 | 0x1a) => {
                    self.buf.clear();
                    St::Ground
                }
                // 나머지 C0(BEL·CAN·ESC·SUB 말고)는 버린다. 엔진도 OSC 본문에 넣지 않는다(vte `advance_osc_string` 407줄).
                // 본문에 쌓으면 엔진이 읽은 문자열과 달라지고, ENQ 같은 것을 우리가 실행하지도 않는다(#598)
                (St::Osc, 0x00..=0x06 | 0x08..=0x17 | 0x19 | 0x1c..=0x1f) => St::Osc,
                (St::Osc, b) => {
                    if self.buf.len() >= MAX_OSC {
                        self.buf.clear();
                        St::Skip
                    } else {
                        self.buf.push(b);
                        St::Osc
                    }
                }
                (St::OscEsc, b'\\') => {
                    self.finish(&mut out, i + 1, false);
                    St::Ground
                }
                (St::OscEsc, _) => St::Ground,
                // 일부러 엔진과 다르다: 상한(MAX_OSC)을 넘긴 OSC는 BEL·CAN·SUB·ESC로 끝나도 내보내지 않는다. 엔진(vte)은 잘린 OSC를 내보낼 수 있다.
                // 끝난 문자열의 앞부분만 믿고 알림·폴더·클립보드로 쓰지 않으려는 보안 결정이다(docs/troubleshooting.md, #598)
                (St::Skip, 0x07 | 0x18 | 0x1a) => St::Ground,
                (St::Skip, 0x1b) => St::SkipEsc,
                (St::Skip, _) => St::Skip,
                (St::SkipEsc, b'\\') => St::Ground,
                (St::SkipEsc, _) => St::Ground,
            };
        }
        out
    }

    /// 끝난 DCS(`ESC \`나 0x9C)를 마무리한다. 우리가 답하는 질의면 신호를 내고, 아니면 버린다
    fn finish_dcs(&mut self, out: &mut Vec<(usize, Signal)>, at: usize) {
        let body = std::mem::take(&mut self.dcs);
        let kind = self.dcs_kind.take();
        if !self.dcs_over {
            match kind {
                Some(DcsKind::Decrqss) => {
                    if let Some(d) = crate::dcs::parse_decrqss(&body) {
                        out.push((at, Signal::DcsQuery(crate::dcs::Query::Decrqss(d))));
                    }
                }
                Some(DcsKind::XtGetTcap) => {
                    out.push((at, Signal::DcsQuery(crate::dcs::Query::XtGetTcap(body))));
                }
                None => {}
            }
        }
    }

    /// 끝난 APC를 내보낸다(상한을 넘겨 버리는 중이거나 kitty 그래픽(`G`)·글리프(`25a1;`)가 아니면 버린다)
    fn finish_apc(&mut self, out: &mut Vec<(usize, Signal)>, at: usize) {
        let raw = std::mem::take(&mut self.apc);
        if self.apc_skip {
            return;
        }
        if raw.first() == Some(&b'G') {
            out.push((at, Signal::Graphics(raw[1..].to_vec())));
        } else if let Some(rest) = raw.strip_prefix(GLYPH_APC_PREFIX) {
            out.push((at, Signal::Glyph(rest.to_vec())));
        }
    }

    fn finish(&mut self, out: &mut Vec<(usize, Signal)>, at: usize, bel: bool) {
        let raw = std::mem::take(&mut self.buf);
        // kitty 알림(OSC 99, #561): 조각을 쌓는 상태가 필요해 여기서 다룬다. 명세는 UTF-8을 요구하므로(Ghostty 파서도 버린다)
        // 깨진 바이트는 글자로 바꾸지 않고 조각째 버린다
        if let Some(rest) = raw.strip_prefix(b"99;") {
            if let Ok(rest) = std::str::from_utf8(rest) {
                match self.kitty_notify.packet(rest, bel) {
                    crate::kitty_notify::Outcome::Nothing => {}
                    crate::kitty_notify::Outcome::Notify(n) => out.push((at, Signal::Notify(n))),
                    crate::kitty_notify::Outcome::Reply(r) => out.push((at, Signal::KittyNotifyReply(r))),
                }
            }
            return;
        }
        // 프로그램 상태(OSC 7501, #618): 본문은 글자 범위가 정해진 바이트라 lossy로 바꾸지 않고 그대로 읽는다.
        // 틀린 보고는 파서가 통째로 버리고, 모르는 OSC처럼 조용하다
        if let Some(body) = raw.strip_prefix(b"7501;") {
            match crate::program_status::parse(body) {
                Some(crate::program_status::Command::Query) => out.push((at, Signal::ProgramStatusQuery { bel })),
                Some(crate::program_status::Command::Report(r)) => out.push((at, Signal::ProgramStatus(Box::new(r)))),
                None => {}
            }
            return;
        }
        // iTerm2 세션 상태(OSC 21337, #926): 7501의 뿌리 기록 보고와 같은 신호로 보낸다. 틀린 것은 조용히 버린다
        if let Some(body) = raw.strip_prefix(b"21337;") {
            if let Some(r) = crate::program_status::parse_iterm2(body) {
                out.push((at, Signal::ProgramStatus(Box::new(r))));
            }
            return;
        }
        // OSC 133은 `133;` 뒤가 2048바이트를 넘으면 명령 전체를 버린다(#822, Ghostty `captureTrailing(.fixed)`의 `MAX_BUF`).
        // 같은 OSC를 읽는 `133;A` 등의 옵션 신호와 아래의 옛 표지 모두 이 앞에서 걸러진다
        if raw.strip_prefix(b"133;").is_some_and(|t| t.len() > MAX_OSC133_TRAILING) {
            return;
        }
        let text = String::from_utf8_lossy(&raw);
        // OSC 133(#329): 옵션까지 읽은 신호를 먼저 낸다. 엔진이 fresh-line으로 커서를 옮긴 뒤에 `Prompt`가 그 줄을 적는다
        if let Some(sp) = text.strip_prefix("133;").and_then(crate::osc133::parse) {
            out.push((at, Signal::Semantic(sp)));
        }
        // 9;12(ConEmu 프롬프트 표시)는 133;A와 같다(Ghostty `.semantic_prompt = .init(.fresh_line_new_prompt)`, #764)
        if text.strip_prefix("9;").is_some_and(|d| osc9(d) == Osc9::Prompt) {
            if let Some(sp) = crate::osc133::parse("A") {
                out.push((at, Signal::Semantic(sp)));
            }
            out.push((at, Signal::Prompt(PromptMark::PromptStart)));
            return;
        }
        if let Some(mut sig) = parse_osc(&text) {
            if let Signal::KittyColor { bel: b, .. } | Signal::KittyClipboard { bel: b, .. } = &mut sig {
                *b = bel;
            }
            out.push((at, sig));
        }
    }
}

/// 스캐너가 CSI에서 신호를 뽑을 수 있는 끝 바이트(#938). `csi_signal`(`t` `h` `l` `n`), `csi_query::plain`(`c` `t` `h` `l` `n`),
/// `private_gt`(`c` `q`), `visibility`·`kitty_paste`(`h` `l`), `save_restore`(`s` `r`), 화면 지우기(`J`),
/// 중간 바이트 길 `csi_inter_signal`·`csi_query::inter`(`y` `p`)를 모은 것이다. 신호를 내는 길을 더하면 여기에 그 끝 바이트를 넣는다.
/// 빠뜨리면 그 신호가 조용히 사라진다(`fast_csi_path_matches_full_dispatch` 시험이 지킨다)
const CSI_WATCHED_FINALS: &[u8] = b"chlnpqrstyJ";

fn csi_final_is_watched(fin: u8) -> bool {
    CSI_WATCHED_FINALS.contains(&fin)
}

/// 매개변수 하나를 수로 읽는다. 비면 0, 넘치면 최댓값
fn csi_num(p: &str) -> u32 {
    p.bytes()
        .fold(0u32, |n, d| n.saturating_mul(10).saturating_add((d - b'0') as u32))
}

/// 중간 바이트가 하나 붙은 CSI(`ESC [ 매개변수 중간 끝`)에서 뽑는 신호: DECRQCRA(`* y`), XTCHECKSUM(`# y`), DECSTR(`! p`)(#259).
/// 매개변수는 숫자와 `;`만 받는다
fn csi_inter_signal(inter: u8, fin: u8, params: &[u8]) -> Option<Signal> {
    if params.contains(&b'?') {
        return None;
    }
    let text = std::str::from_utf8(params).ok()?;
    // 매개변수가 하나도 없으면 빈 목록이고, 아니면 `;`로 나눈 개수가 매개변수 수다
    let parts: Vec<&str> = if text.is_empty() {
        Vec::new()
    } else {
        text.split(';').collect()
    };
    let num = |i: usize| csi_num(parts.get(i).copied().unwrap_or("")).min(u16::MAX as u32) as u16;
    match (inter, fin) {
        // 매개변수가 여섯보다 많으면 Ghostty처럼 잘못된 명령으로 버린다. 쪽 번호(둘째)는 무시한다
        (b'*', b'y') if parts.len() <= 6 => Some(Signal::XtChecksumReport(crate::xt_checksum::Request {
            id: num(0),
            top: num(2),
            left: num(3),
            bottom: num(4),
            right: num(5),
        })),
        // 하나보다 많으면 버리고, 없으면 0이다. 모르는 윗 비트는 버린다
        (b'#', b'y') if parts.len() <= 1 => Some(Signal::XtChecksumExtension((num(0) & 0x1f) as u8)),
        (b'!', b'p') if parts.is_empty() => Some(Signal::SoftReset),
        _ => None,
    }
}

/// 끝난 CSI(`ESC [ 매개변수 끝바이트`)에서 뽑는 신호: 제목 보고(21 t), 창 크기 요청(8 ; 행 ; 열 t), 키보드 잠금(2 h·l)
fn csi_signal(fin: u8, params: &[u8]) -> Option<Signal> {
    // 매개변수 바이트는 숫자, `;`, `?`만 모았다. 맨 앞의 `?`(DEC 비공개)만 아래에서 따로 읽고, 다른 곳의 `?`는 모양이 아니다
    if let Some(private) = params.strip_prefix(b"?") {
        let text = std::str::from_utf8(private).ok()?;
        if text.contains('?') {
            return None;
        }
        return match fin {
            // 색 구성표 질문: `CSI ? 996 n`만(다른 매개변수가 붙으면 다른 질문이다)
            b'n' if text == "996" => Some(Signal::ColorSchemeQuery),
            // 보이는지 묻기(Ghostty 1.4 device_status.zig): 답은 모드 2033 알림과 같은 `CSI ? 999 ; 1|2 n`(#675)
            b'n' if text == "998" => Some(Signal::VisibilityStatusQuery),
            // DEC 모드 2031(색 구성표 변경 알림). 여러 모드 중 하나여도 읽는다
            b'h' | b'l' if text.split(';').any(|p| p == "2031") => Some(Signal::ColorSchemeUpdates(fin == b'h')),
            _ => None,
        };
    }
    if params.contains(&b'?') {
        return None;
    }
    let text = std::str::from_utf8(params).ok()?;
    let parts: Vec<&str> = text.split(';').collect();
    match fin {
        b't' if text == "21" => Some(Signal::TitleReport),
        b't' if parts[0] == "8" && parts.len() <= 3 => {
            let dim = |i: usize| csi_num(parts.get(i).copied().unwrap_or("")).min(u16::MAX as u32) as u16;
            Some(Signal::WindowResize {
                rows: dim(1),
                cols: dim(2),
            })
        }
        b'h' | b'l' if parts.iter().any(|p| csi_num(p) == 2 && !p.is_empty()) => Some(Signal::Kam(fin == b'h')),
        _ => None,
    }
}

fn parse_osc(s: &str) -> Option<Signal> {
    // 번호만 있는 문자열(`OSC 7 ST`)은 `;`가 없어 명령이 아니다(Ghostty `osc.zig`: 번호 뒤에 `;`가 와야 본문을 모은다). 빈 OSC 7은 `7;`뿐이다(#764)
    let (code, rest) = s.split_once(';')?;
    match code {
        "9" => match osc9(rest) {
            Osc9::Notify => {
                let body = clean(rest, MAX_BODY);
                (!body.is_empty()).then(|| {
                    Signal::Notify(Notification {
                        title: String::new(),
                        body,
                    })
                })
            }
            Osc9::Progress(state, percent) => Some(Signal::Progress { state, percent }),
            // 9;9;경로는 OSC 7과 같은 report_pwd다(Ghostty osc9.zig `'9'` 가지)
            Osc9::Cwd(url) => parse_osc(&format!("7;{url}")),
            // 프롬프트 표시(9;12)는 `finish`가 133;A와 같은 신호 둘로 바꾼다
            Osc9::Prompt | Osc9::Ignore => None,
        },
        "777" => {
            // 777;notify;제목;본문
            let rest = rest.strip_prefix("notify;")?;
            let (title, body) = rest.split_once(';').unwrap_or((rest, ""));
            let (title, body) = (clean(title, MAX_TITLE), clean(body, MAX_BODY));
            (!title.is_empty() || !body.is_empty()).then_some(Signal::Notify(Notification { title, body }))
        }
        "133" | "633" => {
            let mut parts = rest.split(';');
            let kind = parts.next()?;
            Some(match kind {
                // N(#329)은 A와 같고, I는 B와 같다(Ghostty `new_command`·`end_prompt_start_input_terminate_eol`)
                "A" => Signal::Prompt(PromptMark::PromptStart),
                "N" if code == "133" => Signal::Prompt(PromptMark::PromptStart),
                // P도 Ghostty처럼 프롬프트 시작이다. 다만 k=c·s·r(이어지는·두 번째·오른쪽 프롬프트)은 새 프롬프트 줄로 세지 않는다
                "P" if code == "133" && parts.clone().all(|o| o.strip_prefix("k=").is_none_or(|k| k == "i")) => {
                    Signal::Prompt(PromptMark::PromptStart)
                }
                "B" => Signal::Prompt(PromptMark::CommandStart),
                "I" if code == "133" => Signal::Prompt(PromptMark::CommandStart),
                "C" => Signal::Prompt(PromptMark::CommandExecuted),
                // 종료 코드는 `osc133::parse`가 읽은 하나만 쓴다(#822: Ghostty `parseInt`라 공백을 받지 않는다)
                "D" => Signal::Prompt(PromptMark::CommandFinished(
                    crate::osc133::parse(rest).and_then(|sp| sp.exit_code),
                )),
                // VS Code: 633;E;명령(;는 \x3b, \는 \\로 적는다)
                "E" if code == "633" => {
                    let cmd = rest.split_once(';').map(|x| x.1).unwrap_or("");
                    let cmd = cmd
                        .split(';')
                        .next()
                        .unwrap_or("")
                        .replace("\\x3b", ";")
                        .replace("\\\\", "\\");
                    Signal::CommandLine(clean(&cmd, MAX_BODY))
                }
                _ => return None,
            })
        }
        "7" => {
            // 7; 빈 값은 알려진 작업 폴더를 지운다(Ghostty stream_handler.zig reportPwd)
            if rest.is_empty() {
                return Some(Signal::CwdReset);
            }
            // 7;file://호스트/경로 (경로는 퍼센트 인코딩), 7;kitty-shell-cwd://호스트/경로 (경로는 인코딩하지 않은 그대로)
            let (rest, raw) = match rest.strip_prefix("file://") {
                Some(r) => (r, false),
                None => (rest.strip_prefix("kitty-shell-cwd://")?, true),
            };
            let slash = rest.find('/')?;
            let host = clean(&rest[..slash], 255);
            let decoded = if raw {
                rest[slash..].to_string()
            } else {
                percent_decode(&rest[slash..])?
            };
            (decoded.starts_with('/') && !decoded.contains('\0')).then(|| Signal::Cwd {
                host,
                path: PathBuf::from(decoded),
            })
        }
        "22" => mouse_shape_signal(rest),
        // 5522: 상한(MAX_OSC)은 모으기 전에 이미 지났다. 해석은 세션이 쥔 거래 상태와 함께 한다(#325)
        "5522" => Some(Signal::KittyClipboard {
            packet: rest.to_string(),
            bel: false,
        }),
        "21" => crate::kitty_color::parse(rest)
            .filter(|r| !r.is_empty())
            .map(|requests| Signal::KittyColor { requests, bel: false }),
        _ => None,
    }
}

/// OSC 9의 갈래(Ghostty v1.4.0 `osc9.zig` `parse`)
#[derive(Debug, PartialEq, Eq)]
enum Osc9<'a> {
    /// ConEmu 명령이 아니거나 형식이 맞지 않다: iTerm2 알림(본문은 `9;` 뒤 전체)
    Notify,
    /// ConEmu 명령이지만 앱이 쓰지 않고 버린다(sleep·message box·tab title·wait input·guimacro·run process·environment·xterm emulation·comment)
    Ignore,
    /// 9;12: 프롬프트 시작(OSC 133;A와 같다)
    Prompt,
    /// 9;4;상태;값
    Progress(u8, Option<u8>),
    /// 9;9;경로(report_pwd)
    Cwd(&'a str),
}

/// `9;` 뒤의 글을 Ghostty `osc9.zig`의 분기대로 가른다. 바이트 비교만 하므로 글자 경계는 ASCII `;` 다음에서만 자른다
fn osc9(data: &str) -> Osc9<'_> {
    let b = data.as_bytes();
    // 이름 번호 뒤에 `;`가 와야 그 명령이다. 모자라거나 다르면 알림이다
    let semi = |i: usize| b.get(i) == Some(&b';');
    match b.first() {
        Some(b'1') => match b.get(1) {
            Some(b';') => Osc9::Ignore, // 9;1 sleep
            Some(b'0') => {
                // 9;10 xterm emulation: `10`, 또는 `10;0`~`10;3`(뒤는 보지 않는다)
                if b.len() == 2 {
                    return Osc9::Ignore;
                }
                if b.len() < 4 || !semi(2) {
                    return Osc9::Notify;
                }
                if matches!(b[3], b'0'..=b'3') {
                    Osc9::Ignore
                } else {
                    Osc9::Notify
                }
            }
            Some(b'1') if b.len() >= 3 && semi(2) => Osc9::Ignore, // 9;11 comment
            Some(b'2') => Osc9::Prompt,                            // 9;12 뒤는 보지 않는다
            _ => Osc9::Notify,
        },
        // 9;2 message box, 9;3 tab title, 9;6 guimacro, 9;7 run process, 9;8 environment variable
        Some(b'2' | b'3' | b'6' | b'7' | b'8') if semi(1) => Osc9::Ignore,
        Some(b'4') if semi(1) && b.len() >= 3 => {
            let state = match b[2] {
                b'0' => 0,
                b'1' => 1,
                b'2' => 2,
                b'3' => 3,
                b'4' => 4,
                _ => return Osc9::Notify,
            };
            // 값은 set·error·pause만 읽는다. 값 자리가 없으면 set은 0, 나머지는 없음이다(`4;1`은 0%).
            // `;`가 있는데 읽을 수 없으면(빈 글, 숫자 아님, usize 넘침, `4_2`) 없음이다
            let mut percent = (state == 1).then_some(0);
            if matches!(state, 1 | 2 | 4) && b.len() >= 4 && semi(3) {
                let v = &data[4..];
                percent = (!v.is_empty() && v.bytes().all(|c| c.is_ascii_digit()))
                    .then(|| v.parse::<u64>().ok())
                    .flatten()
                    .map(|n| n.min(100) as u8);
            }
            Osc9::Progress(state, if matches!(state, 1 | 2 | 4) { percent } else { None })
        }
        // 9;5 wait input: 첫 글자가 5면 뒤는 보지 않는다
        Some(b'5') => Osc9::Ignore,
        Some(b'9') if semi(1) => Osc9::Cwd(&data[2..]),
        _ => Osc9::Notify,
    }
}

/// 제어 문자를 지우고 앞뒤 공백을 자른 뒤 길이(글자 수)를 자른다
pub(crate) fn clean(s: &str, max: usize) -> String {
    let filtered: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    truncate(filtered.trim(), max)
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod apc_tests {
    use super::*;

    /// #76: APC _G를 조각난 바이트에서도 뽑고, 다른 APC와 화면 지우기를 가린다
    #[test]
    fn enquiry_byte_is_a_signal_only_outside_strings() {
        let mut sc = Scanner::default();
        assert_eq!(sc.feed(b"a\x05b"), vec![Signal::Enquiry]);
        // CSI 매개변수 사이의 C0는 VT 엔진도 실행한다
        assert_eq!(sc.feed(b"\x1b[1\x05m"), vec![Signal::Enquiry]);
        // OSC·APC 문자열 안의 0x05는 글자다
        assert_eq!(sc.feed(b"\x1b]2;a\x05b\x07"), vec![]);
        assert_eq!(
            sc.feed(b"\x1b_Gi=1;a\x05b\x1b\\"),
            vec![Signal::Graphics(b"i=1;a\x05b".to_vec())]
        );
    }

    #[test]
    fn kam_set_and_reset_are_plain_ansi_modes_only() {
        let mut sc = Scanner::default();
        assert_eq!(sc.feed(b"\x1b[2h"), vec![Signal::Kam(true)]);
        assert_eq!(sc.feed(b"\x1b[2l"), vec![Signal::Kam(false)]);
        assert_eq!(sc.feed(b"\x1b[4;2h"), vec![Signal::Kam(true)], "여러 모드 중 하나");
        assert_eq!(sc.feed(b"\x1b[?2h\x1b[4h\x1b[12h\x1b[22h\x1b[2;4t"), vec![]);
        // 조각으로 와도 이어 읽는다
        assert_eq!(sc.feed(b"\x1b[").len() + sc.feed(b"2").len(), 0);
        assert_eq!(sc.feed(b"h"), vec![Signal::Kam(true)]);
    }

    #[test]
    fn color_scheme_query_and_mode_2031_are_parsed() {
        let mut sc = Scanner::default();
        assert_eq!(sc.feed(b"\x1b[?996n"), vec![Signal::ColorSchemeQuery]);
        // 보이는지 묻기(Ghostty 1.4 `CSI ? 998 n`, #675). 다른 매개변수가 붙으면 다른 질문이다
        assert_eq!(sc.feed(b"\x1b[?998n"), vec![Signal::VisibilityStatusQuery]);
        assert!(sc.feed(b"\x1b[?998;1n\x1b[998n\x1b[?9980n").is_empty());
        assert_eq!(sc.feed(b"\x1b[?2031h"), vec![Signal::ColorSchemeUpdates(true)]);
        assert_eq!(sc.feed(b"\x1b[?2031l"), vec![Signal::ColorSchemeUpdates(false)]);
        assert_eq!(
            sc.feed(b"\x1b[?25;2031h"),
            vec![Signal::ColorSchemeUpdates(true)],
            "여러 모드 중 하나"
        );
        // 비슷하지만 다른 것은 신호가 아니다
        assert_eq!(
            sc.feed(b"\x1b[996n\x1b[?2030h\x1b[?996;1n\x1b[?20310h\x1b[2031h\x1b[5n"),
            vec![]
        );
        // 조각으로 와도 이어 읽는다
        assert_eq!(sc.feed(b"\x1b[?99").len() + sc.feed(b"6").len(), 0);
        assert_eq!(sc.feed(b"n"), vec![Signal::ColorSchemeQuery]);
    }

    #[test]
    fn window_resize_request_is_parsed() {
        let r = |s: &[u8]| Scanner::default().feed(s);
        let w = |rows, cols| vec![Signal::WindowResize { rows, cols }];
        assert_eq!(r(b"\x1b[8;30;100t"), w(30, 100));
        assert_eq!(r(b"\x1b[8;0;100t"), w(0, 100));
        assert_eq!(r(b"\x1b[8;30t"), w(30, 0));
        assert_eq!(r(b"\x1b[8;;100t"), w(0, 100));
        assert_eq!(r(b"\x1b[8t"), w(0, 0));
        assert_eq!(r(b"\x1b[8;99999;70000t"), w(u16::MAX, u16::MAX), "넘치면 최댓값");
        // 다른 CSI t(보고·제목 스택)와 모양이 다른 것은 신호가 아니다
        assert_eq!(
            r(b"\x1b[14t\x1b[18t\x1b[22;0t\x1b[88;3;4t\x1b[?8;3;4t\x1b[8;a;4t"),
            vec![]
        );
        // 매개변수가 너무 길어 잘렸으면 믿지 않는다
        assert_eq!(r(b"\x1b[8;1;2;3;4;5;6;7;8;9;100t"), vec![]);
    }

    fn notes(input: &[u8]) -> Vec<Signal> {
        Scanner::default().feed(input)
    }

    #[test]
    fn osc22_sets_the_mouse_shape_with_bel_or_st() {
        use crate::mouse_shape::MouseShape as M;
        assert_eq!(
            notes(b"\x1b]22;crosshair\x07"),
            vec![Signal::MouseShape(Some(M::Crosshair))]
        );
        assert_eq!(
            notes(b"\x1b]22;pointer\x1b\\"),
            vec![Signal::MouseShape(Some(M::Pointer))]
        );
        // xterm 별칭
        assert_eq!(
            notes(b"\x1b]22;left_ptr\x07"),
            vec![Signal::MouseShape(Some(M::Default))]
        );
        // 읽기 조각 경계에서 잘려 와도 한 번만
        let mut s = Scanner::default();
        let mut got = Vec::new();
        for ch in b"\x1b]22;text\x1b\\".chunks(2) {
            got.extend(s.feed(ch));
        }
        assert_eq!(got, vec![Signal::MouseShape(Some(M::Text))]);
    }

    #[test]
    fn osc22_empty_name_resets_and_unknown_or_hostile_names_are_ignored() {
        assert_eq!(notes(b"\x1b]22;\x07"), vec![Signal::MouseShape(None)]);
        assert_eq!(notes(b"\x1b]22;banana\x07"), vec![]);
        assert_eq!(notes(b"\x1b]22;Pointer\x07"), vec![]);
        // 이름 안의 C0는 엔진처럼 버린다(#598): 엔진이 `pointer`로 읽는 글을 우리도 같게 읽는다
        assert_eq!(
            notes(b"\x1b]22;poi\x01nter\x07"),
            vec![Signal::MouseShape(Some(crate::mouse_shape::MouseShape::Pointer))]
        );
        // 너무 긴 이름
        let mut long = b"\x1b]22;".to_vec();
        long.extend(std::iter::repeat_n(b'p', 500));
        long.push(0x07);
        assert_eq!(notes(&long), vec![]);
        // 뒤따르는 정상 OSC는 받는다
        long.extend_from_slice(b"\x1b]22;wait\x07");
        assert_eq!(
            notes(&long),
            vec![Signal::MouseShape(Some(crate::mouse_shape::MouseShape::Wait))]
        );
    }

    #[test]
    fn graphics_apc_and_erase_signals() {
        let mut s = Scanner::default();
        let bytes = b"ab\x1b_Ga=q,i=1;AAAA\x1b\\cd\x1b_Xother\x1b\\\x1b[2J\x1b[3J\x1b[0J\x1bc";
        let mut got = Vec::new();
        for ch in bytes.chunks(3) {
            got.extend(s.feed(ch));
        }
        assert_eq!(
            got,
            vec![
                Signal::Graphics(b"a=q,i=1;AAAA".to_vec()),
                Signal::Erase {
                    visible: true,
                    scrollback: false
                },
                Signal::Erase {
                    visible: false,
                    scrollback: true
                },
                Signal::Erase {
                    visible: true,
                    scrollback: true
                },
            ]
        );
        // 끝난 자리는 ESC \ 바로 뒤
        let at = Scanner::default().feed_at(b"xx\x1b_Gq\x1b\\yy");
        assert_eq!(at[0].0, 8, "x x ESC _ G q ESC \\ 다음 자리");
    }

    /// #779: APC `25a1;`는 글리프 신호가 된다. 조각난 바이트, CAN·SUB·8비트 ST 끝맺음, 다른 식별자
    #[test]
    fn glyph_apc_becomes_a_signal() {
        let mut s = Scanner::default();
        let mut got = Vec::new();
        for ch in b"ab\x1b_25a1;q;cp=e0a0\x1b\\cd\x1b_25a1;s\x18\x1b_25a1;s\x1a\x1b_25a1;s\x9c".chunks(2) {
            got.extend(s.feed(ch));
        }
        assert_eq!(
            got,
            vec![
                Signal::Glyph(b"q;cp=e0a0".to_vec()),
                Signal::Glyph(b"s".to_vec()),
                Signal::Glyph(b"s".to_vec()),
                Signal::Glyph(b"s".to_vec()),
            ]
        );
        // `;` 없는 25a1, 다른 식별자, kitty 그래픽은 글리프가 아니다
        let mut s = Scanner::default();
        assert_eq!(
            s.feed(b"\x1b_25a1\x1b\\\x1b_25a2;s\x1b\\\x1b_25a;s\x1b\\\x1b_Gq\x1b\\"),
            vec![Signal::Graphics(b"q".to_vec())]
        );
        // ESC 뒤가 `\`가 아니어도 모인 APC는 끝난 것으로 내보낸다(kitty 그래픽과 같다, #595)
        let mut s = Scanner::default();
        assert_eq!(s.feed(b"\x1b_25a1;s\x1b[m"), vec![Signal::Glyph(b"s".to_vec())]);
        // tmux 패스스루 안쪽은 신호를 내지 않는다(#603)
        let mut s = Scanner::default();
        assert!(s.feed(b"\x1bPtmux;\x1b\x1b_25a1;s\x1b\x1b\\\x1b\\").is_empty());
    }

    /// #779: 글리프 APC는 식별자 뒤 1MiB까지만 모은다(Ghostty `apc.zig` 409줄). 정확히 1MiB는 내고, 1바이트 넘으면 버리고, 버퍼도 늘리지 않는다
    #[test]
    fn glyph_apc_over_one_mebibyte_is_dropped() {
        let max = crate::glyph::MAX_REQUEST;
        let mut s = Scanner::default();
        s.feed(b"\x1b_25a1;");
        s.feed(&vec![b'A'; max]);
        let at_limit = s.feed(b"\x1b\\");
        assert_eq!(at_limit.len(), 1);
        assert!(matches!(&at_limit[0], Signal::Glyph(d) if d.len() == max));
        let mut s = Scanner::default();
        s.feed(b"\x1b_25a1;");
        s.feed(&vec![b'A'; max + 1]);
        assert!(s.apc.capacity() == 0, "상한을 넘으면 버퍼를 비운다");
        assert!(s.feed(b"\x1b\\").is_empty());
        // 그 뒤의 요청은 다시 읽는다
        assert_eq!(s.feed(b"\x1b_25a1;s\x1b\\"), vec![Signal::Glyph(b"s".to_vec())]);
    }

    #[test]
    fn oversized_apc_is_dropped_without_growing() {
        let mut s = Scanner::default();
        s.feed(b"\x1b_G");
        let chunk = vec![b'A'; 1024 * 1024];
        for _ in 0..9 {
            assert!(s.feed(&chunk).is_empty());
        }
        assert!(s.feed(b"\x1b\\").is_empty(), "상한을 넘은 APC는 버린다");
        assert!(s.apc.capacity() <= MAX_APC * 2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notes(input: &[u8]) -> Vec<Signal> {
        Scanner::default().feed(input)
    }

    fn rq(id: u16, top: u16, left: u16, bottom: u16, right: u16) -> Signal {
        Signal::XtChecksumReport(crate::xt_checksum::Request {
            id,
            top,
            left,
            bottom,
            right,
        })
    }

    #[test]
    fn decrqcra_is_parsed_with_its_intermediate_byte() {
        // 요청 번호, 쪽(무시), 위·왼쪽·아래·오른쪽
        assert_eq!(notes(b"\x1b[1;1;1;1;1;3*y"), vec![rq(1, 1, 1, 1, 3)]);
        assert_eq!(notes(b"\x1b[65535;9;2;3;4;5*y"), vec![rq(65535, 2, 3, 4, 5)]);
        // 생략한 것은 0
        assert_eq!(notes(b"\x1b[*y"), vec![rq(0, 0, 0, 0, 0)]);
        assert_eq!(notes(b"\x1b[7*y"), vec![rq(7, 0, 0, 0, 0)]);
        assert_eq!(notes(b"\x1b[7;;;2;;4*y"), vec![rq(7, 0, 2, 0, 4)]);
        assert_eq!(
            notes(b"\x1b[99999;1;1;1;1;1*y"),
            vec![rq(u16::MAX, 1, 1, 1, 1)],
            "넘치면 최댓값"
        );
        // 조각으로 와도 이어 읽는다
        let mut sc = Scanner::default();
        assert!(sc.feed(b"\x1b[3;1;1;2;").is_empty() && sc.feed(b"1;3").is_empty() && sc.feed(b"*").is_empty());
        assert_eq!(sc.feed(b"y"), vec![rq(3, 1, 2, 1, 3)]);
        // 신호가 끝난 자리는 마지막 바이트 바로 뒤다
        let mut sc = Scanner::default();
        assert_eq!(sc.feed_at(b"ab\x1b[1*yc"), vec![(7, rq(1, 0, 0, 0, 0))]);
    }

    #[test]
    fn decrqcra_with_a_wrong_shape_is_no_signal() {
        let none: Vec<Signal> = vec![];
        // 매개변수가 여섯보다 많다
        assert_eq!(notes(b"\x1b[1;1;1;1;1;3;4*y"), none);
        // 비공개 표시, 부 매개변수, 다른 표지
        assert_eq!(notes(b"\x1b[?1;1;1;1;1;3*y"), none);
        assert_eq!(notes(b"\x1b[1;1:2;1;1;1;3*y"), none);
        assert_eq!(notes(b"\x1b[<1*y\x1b[=1*y\x1b[>1*y"), none);
        // 중간 바이트가 둘이거나 매개변수 뒤가 아니라 앞에 왔거나, 다른 중간 바이트
        assert_eq!(notes(b"\x1b[1**y\x1b[1*#y\x1b[*1y\x1b[1 y\x1b[1$y\x1b[1+y"), none);
        // 끝바이트가 다르다
        assert_eq!(notes(b"\x1b[1*z\x1b[1*x\x1b[1*Y"), none);
        // 너무 길어 잘렸으면 믿지 않는다
        let long = format!("\x1b[{}*y", "1;".repeat(30));
        assert_eq!(notes(long.as_bytes()), none);
    }

    #[test]
    fn xtchecksum_is_parsed_and_keeps_only_five_bits() {
        let x = |b| vec![Signal::XtChecksumExtension(b)];
        assert_eq!(notes(b"\x1b[#y"), x(0));
        assert_eq!(notes(b"\x1b[0#y"), x(0));
        assert_eq!(notes(b"\x1b[1#y"), x(1));
        assert_eq!(notes(b"\x1b[23#y"), x(23));
        assert_eq!(notes(b"\x1b[33#y"), x(1), "윗 비트는 버린다");
        assert_eq!(notes(b"\x1b[255#y"), x(31));
        assert_eq!(
            notes(b"\x1b[1;2#y\x1b[?1#y\x1b[1##y\x1b[1#z\x1b[#1y"),
            vec![],
            "다른 모양은 무시"
        );
    }

    #[test]
    fn decstr_is_a_soft_reset_signal() {
        assert_eq!(notes(b"\x1b[!p"), vec![Signal::SoftReset]);
        assert_eq!(notes(b"\x1b[0!p\x1b[?!p\x1b[!!p\x1b[!q"), vec![]);
    }

    #[test]
    fn an_intermediate_byte_keeps_other_csi_from_being_read_as_signals() {
        // 중간 바이트가 붙은 것은 21 t·8 t·2 h·? 996 n·? 2031 h와 다른 명령이다
        let none: Vec<Signal> = vec![];
        assert_eq!(
            notes(b"\x1b[21*t\x1b[21#t\x1b[8;30;100*t\x1b[2*h\x1b[2#l\x1b[?996*n\x1b[?2031#h\x1b[?2031$h"),
            none
        );
        // 그리고 체크섬 신호 곁의 다른 신호는 그대로 읽는다
        assert_eq!(
            notes(b"\x1b[21t\x1b[1;1;1;1;1;1*y\x1b[8;30;100t\x1b[2h\x1b[?996n\x1b[?2031h\x1b[1#y"),
            vec![
                Signal::TitleReport,
                rq(1, 1, 1, 1, 1),
                Signal::WindowResize { rows: 30, cols: 100 },
                Signal::Kam(true),
                Signal::ColorSchemeQuery,
                Signal::ColorSchemeUpdates(true),
                Signal::XtChecksumExtension(1),
            ]
        );
        // 한 CSI의 중간 바이트 상태가 다음 CSI로 넘어가지 않는다
        assert_eq!(notes(b"\x1b[1*\x1b[21t"), vec![Signal::TitleReport]);
        assert_eq!(notes(b"\x1b[1*x\x1b[1;2;3;4;5;6y"), none);
    }

    fn n(title: &str, body: &str) -> Signal {
        Signal::Notify(Notification {
            title: title.into(),
            body: body.into(),
        })
    }

    #[test]
    fn osc9_777_and_99_become_notifications() {
        assert_eq!(notes(b"\x1b]9;Build finished\x07"), vec![n("", "Build finished")]);
        assert_eq!(
            notes("\x1b]777;notify;make;빌드가 끝났습니다\x1b\\".as_bytes()),
            vec![n("make", "빌드가 끝났습니다")]
        );
        // d=0은 아직 끝이 아니다(kitty 명세 52~63줄): 쌓아 두고 알리지 않는다. d를 생략하거나 d=1이면 알린다(#561)
        assert!(notes(b"\x1b]99;i=1:d=0;Build done\x1b\\").is_empty());
        assert_eq!(notes(b"\x1b]99;i=1;Build done\x1b\\"), vec![n("Build done", "")]);
        assert_eq!(notes(b"\x1b]99;;Build done\x1b\\"), vec![n("Build done", "")]);
        assert_eq!(notes(b"\x1b]99;i=1:p=body;all green\x07"), vec![n("", "all green")]);
    }

    #[test]
    fn osc7501_makes_a_query_or_a_checked_report_signal() {
        use crate::program_status::State;
        assert_eq!(
            notes(b"\x1b]7501;?\x1b\\"),
            vec![Signal::ProgramStatusQuery { bel: false }]
        );
        assert_eq!(
            notes(b"\x1b]7501;?\x07"),
            vec![Signal::ProgramStatusQuery { bel: true }]
        );
        let got = notes(b"\x1b]7501;state=working:progress=40\x07");
        assert!(matches!(&got[..], [Signal::ProgramStatus(r)] if r.state == State::Working && r.progress == Some(40)));
        // 틀린 보고, 번호만 있는 문자열, 모르는 번호는 조용하다
        for bad in [
            &b"\x1b]7501;state=sleeping\x07"[..],
            b"\x1b]7501\x07",
            b"\x1b]7502;state=idle\x07",
        ] {
            assert!(notes(bad).is_empty(), "{bad:?}");
        }
        // 조각으로 와도 하나로 모인다
        let mut s = Scanner::default();
        assert!(s.feed(b"\x1b]7501;state=do").is_empty());
        assert_eq!(s.feed(b"ne\x1b\\").len(), 1);
        // tmux 패스스루 안쪽은 조용하다(ADR-053)
        assert!(notes(b"\x1bPtmux;\x1b\x1b]7501;?\x07\x1b\\").is_empty());
    }

    #[test]
    fn osc21337_makes_the_same_signal_as_7501_with_either_terminator() {
        for tail in [&b"\x07"[..], b"\x1b\\"] {
            let mut input = b"\x1b]21337;status=Working;indicator=#ffa500".to_vec();
            input.extend_from_slice(tail);
            let got = notes(&input);
            assert!(
                matches!(got.as_slice(), [Signal::ProgramStatus(r)] if r.state == crate::program_status::State::Working),
                "{got:?}"
            );
        }
        let got = notes(b"\x1b]21337;status=Waiting;detail=ok\x07");
        assert!(
            matches!(got.as_slice(), [Signal::ProgramStatus(r)] if r.state == crate::program_status::State::Blocked)
        );
        assert!(notes(b"\x1b]21337;indicator=red\x07").is_empty());
        // C0는 스캐너가 본문에 넣지 않고(엔진과 같다), DEL·C1은 해석기가 버린다
        assert!(notes(b"\x1b]21337;status=a\x7fb\x07").is_empty());
        assert!(notes("\x1b]21337;status=a\u{85}b\x07".as_bytes()).is_empty());
        assert!(notes(b"\x1b]21337\x07").is_empty());
        assert!(notes(b"\x1b]213370;status=Working\x07").is_empty());
    }

    #[test]
    fn split_across_reads_is_reassembled() {
        let mut s = Scanner::default();
        assert!(s.feed(b"out\x1b]77").is_empty());
        assert!(s.feed(b"7;notify;ti").is_empty());
        assert_eq!(s.feed(b"tle;body\x1b").len(), 0);
        assert_eq!(s.feed(b"\\more"), vec![n("title", "body")]);
    }

    #[test]
    fn progress_is_not_a_notification() {
        assert_eq!(
            notes(b"\x1b]9;4;1;40\x07"),
            vec![Signal::Progress {
                state: 1,
                percent: Some(40)
            }]
        );
        assert_eq!(
            notes(b"\x1b]9;4;0;\x07"),
            vec![Signal::Progress {
                state: 0,
                percent: None
            }]
        );
        assert!(
            notes(b"\x1b]9;9;\"C:\\\\\"\x07").is_empty(),
            "다른 ConEmu 명령은 버린다"
        );
    }

    #[test]
    fn shell_integration_marks_and_their_positions() {
        let input = b"out\x1b]133;C\x07hello\n\x1b]133;D;2\x1b\\\x1b]133;A\x07$ \x1b]633;E;git commit -m a\\x3bb\x07";
        // 133의 옵션을 읽은 신호(Semantic)는 따로 시험한다(#329)
        let got: Vec<_> = Scanner::default()
            .feed_at(input)
            .into_iter()
            .filter(|(_, s)| !matches!(s, Signal::Semantic(_)))
            .collect();
        assert_eq!(got[0], (11, Signal::Prompt(PromptMark::CommandExecuted)));
        assert_eq!(&input[11..17], b"hello\n", "C가 끝난 자리부터 출력이다");
        assert_eq!(got[1].1, Signal::Prompt(PromptMark::CommandFinished(Some(2))));
        assert_eq!(got[2].1, Signal::Prompt(PromptMark::PromptStart));
        assert_eq!(got[3].1, Signal::CommandLine("git commit -m a;b".into()));
        let d = Scanner::default().feed(b"\x1b]133;D\x07");
        assert_eq!(d.last(), Some(&Signal::Prompt(PromptMark::CommandFinished(None))));
    }

    #[test]
    fn osc133_emits_the_parsed_command_before_the_legacy_mark() {
        use crate::osc133::{Action, PromptKind, Redraw};
        let got = Scanner::default().feed(b"\x1b]133;A;aid=3;k=s;redraw=last\x07");
        assert_eq!(got.len(), 2);
        let Signal::Semantic(sp) = &got[0] else {
            panic!("Semantic이 먼저다: {:?}", got[0]);
        };
        assert_eq!(sp.action, Action::FreshLineNewPrompt);
        assert_eq!(sp.aid.as_deref(), Some("3"));
        assert_eq!(sp.kind, Some(PromptKind::Secondary));
        assert_eq!(sp.redraw, Some(Redraw::Last));
        assert_eq!(got[1], Signal::Prompt(PromptMark::PromptStart));
        // L·P는 탭 수준의 표지(프롬프트 목록·명령 알림)에는 영향이 없다
        for osc in [&b"\x1b]133;L\x07"[..], b"\x1b]133;P;k=r\x07"] {
            let got = Scanner::default().feed(osc);
            assert_eq!(got.len(), 1);
            assert!(matches!(got[0], Signal::Semantic(_)));
        }
        // N은 A처럼, I는 B처럼 탭 수준 표지에 알린다
        assert_eq!(
            Scanner::default().feed(b"\x1b]133;N;aid=x\x07")[1],
            Signal::Prompt(PromptMark::PromptStart)
        );
        assert_eq!(
            Scanner::default().feed(b"\x1b]133;I\x07")[1],
            Signal::Prompt(PromptMark::CommandStart)
        );
        // 잘못된 OSC는 아무것도 내지 않는다. 633은 예전 그대로 A~D만 읽는다
        assert!(Scanner::default().feed(b"\x1b]133;Ax\x07").is_empty());
        assert!(Scanner::default().feed(b"\x1b]133;L;x\x07").is_empty());
        assert_eq!(
            Scanner::default().feed(b"\x1b]633;A\x07"),
            vec![Signal::Prompt(PromptMark::PromptStart)]
        );
        assert!(Scanner::default().feed(b"\x1b]633;N\x07").is_empty());
    }

    #[test]
    fn osc133_p_primary_is_a_prompt_start_like_ghostty() {
        // Ghostty stream_terminal.zig: A·N·P 모두 prompt_start. zsh 보강(#851)이 내는 133;P;k=i가 셸 통합으로 잡혀야 한다
        let start = Signal::Prompt(PromptMark::PromptStart);
        assert!(Scanner::default().feed(b"\x1b]133;P;k=i\x07").contains(&start));
        assert!(Scanner::default().feed(b"\x1b]133;P\x07").contains(&start));
        // 이어지는·두 번째·오른쪽 프롬프트는 새 프롬프트 줄로 세지 않는다
        for k in ["c", "s", "r"] {
            let seq = format!("\x1b]133;P;k={k}\x07");
            assert!(!Scanner::default().feed(seq.as_bytes()).contains(&start), "k={k}");
        }
        // 633은 예전 그대로 P를 읽지 않는다
        assert!(!Scanner::default().feed(b"\x1b]633;P\x07").contains(&start));
    }

    #[test]
    fn osc133_split_across_reads_and_with_st_end() {
        let mut s = Scanner::default();
        assert!(s.feed(b"\x1b]133;A;k").is_empty());
        let got = s.feed(b"=c\x1b\\");
        assert!(matches!(&got[0], Signal::Semantic(sp) if sp.kind == Some(crate::osc133::PromptKind::Continuation)));
    }

    #[test]
    fn osc7_empty_value_resets_cwd() {
        // Ghostty report_pwd.zig: `7;`는 빈 값이고 지우는 신호다. `OSC 7 ST`(세미콜론 없음)는 명령이 아니라 아무것도 하지 않는다(#764)
        assert_eq!(notes(b"\x1b]7;\x07"), vec![Signal::CwdReset]);
        assert_eq!(notes(b"\x1b]7;\x1b\\"), vec![Signal::CwdReset]);
        assert!(notes(b"\x1b]7\x07").is_empty());
        assert!(notes(b"\x1b]7\x1b\\").is_empty());
        // 7로 시작하는 다른 번호는 아니다
        assert!(notes(b"\x1b]70\x07").is_empty());
    }

    #[test]
    fn osc7_kitty_shell_cwd_uses_raw_path() {
        // kitty-shell-cwd는 인코딩하지 않은 경로를 보낸다: %는 그대로, 공백도 그대로
        assert_eq!(
            notes(b"\x1b]7;kitty-shell-cwd://mac.local/Users/me/My Dir/100%\x07"),
            vec![Signal::Cwd {
                host: "mac.local".into(),
                path: PathBuf::from("/Users/me/My Dir/100%")
            }]
        );
        assert_eq!(
            notes(b"\x1b]7;kitty-shell-cwd://h/a%20b\x1b\\"),
            vec![Signal::Cwd {
                host: "h".into(),
                path: PathBuf::from("/a%20b")
            }]
        );
        // 같은 검증: 경로가 없거나 NUL이 들면 버린다. 글자로 온 NUL(0x00)은 엔진이 OSC 안에서 버리므로(#598) 퍼센트 인코딩한 `%00`으로 시험한다
        assert!(notes(b"\x1b]7;kitty-shell-cwd://host\x07").is_empty());
        assert!(notes(b"\x1b]7;file://h/a%00b\x07").is_empty());
        assert_eq!(
            notes(b"\x1b]7;kitty-shell-cwd://h/a\0b\x07"),
            vec![Signal::Cwd {
                host: "h".into(),
                path: PathBuf::from("/ab")
            }]
        );
        // file 스킴의 퍼센트 해독은 그대로다
        assert_eq!(
            notes(b"\x1b]7;file://h/a%20b\x07"),
            vec![Signal::Cwd {
                host: "h".into(),
                path: PathBuf::from("/a b")
            }]
        );
        assert!(notes(b"\x1b]7;file://h/a%00b\x07").is_empty());
        assert!(notes(b"\x1b]7;kitty-cwd://h/a\x07").is_empty());
    }

    #[test]
    fn osc7_reports_decoded_cwd() {
        assert_eq!(
            notes(b"\x1b]7;file://mac.local/Users/me/My%20Dir\x07"),
            vec![Signal::Cwd {
                host: "mac.local".into(),
                path: PathBuf::from("/Users/me/My Dir")
            }]
        );
        assert!(notes(b"\x1b]7;http://x/y\x07").is_empty());
    }

    #[test]
    fn control_chars_are_stripped_and_length_capped() {
        let evil = format!("\x1b]777;notify;t\x1b[31m;{}\x07", "가".repeat(2000));
        // 제목 안의 ESC는 OSC를 끝낸다(#595, 엔진과 같다): 거기까지가 알림이고, 뒤의 `[31m;가가…`는 글자일 뿐 알림 글이 아니다
        let out = notes(evil.as_bytes());
        assert_eq!(out.len(), 1);
        let Signal::Notify(note) = &out[0] else { panic!() };
        assert_eq!((note.title.as_str(), note.body.as_str()), ("t", ""));
        let long = format!("\x1b]9;{}\x07", "a".repeat(5000));
        let out = notes(long.as_bytes());
        let Signal::Notify(note) = &out[0] else { panic!() };
        assert_eq!(note.body.chars().count(), MAX_BODY);
        assert!(note.body.ends_with('…'));
        let tabbed = notes(b"\x1b]9;a\tb\x07");
        // 탭(0x09)도 OSC 안의 C0라 엔진처럼 버린다(#598)
        assert_eq!(tabbed, vec![n("", "ab")]);
    }

    #[test]
    fn oversized_osc_is_dropped_without_growing_memory() {
        let mut s = Scanner::default();
        let mut huge = b"\x1b]9;".to_vec();
        huge.extend(std::iter::repeat_n(b'x', MAX_OSC + 10));
        huge.push(0x07);
        assert!(s.feed(&huge).is_empty());
        assert!(s.buf.capacity() <= MAX_OSC * 2);
        // 뒤따르는 정상 OSC는 받는다
        assert_eq!(s.feed(b"\x1b]9;ok\x07"), vec![n("", "ok")]);
    }

    #[test]
    fn osc21_becomes_a_kitty_color_signal_remembering_the_terminator() {
        use crate::kitty_color::{Key, Request, Special};
        let mut s = Scanner::default();
        assert_eq!(
            s.feed(b"\x1b]21;foreground=?\x1b\\"),
            vec![Signal::KittyColor {
                requests: vec![Request::Query(Key::Special(Special::Foreground))],
                bel: false
            }]
        );
        assert_eq!(
            s.feed(b"\x1b]21;background=#102030\x07"),
            vec![Signal::KittyColor {
                requests: vec![Request::Set(Key::Special(Special::Background), (0x10, 0x20, 0x30))],
                bel: true
            }]
        );
        // 알아볼 항목이 하나도 없으면 신호도 없다
        assert!(s.feed(b"\x1b]21;nope=?\x07").is_empty());
    }
}

#[cfg(test)]
#[path = "notify_dcs_tests.rs"]
mod dcs_tests;

#[cfg(test)]
#[path = "kitty_notify_tests.rs"]
mod kitty_notify_tests;

#[cfg(test)]
mod fast_csi_tests {
    use super::*;

    type Signals = Vec<(usize, Signal)>;

    /// 같은 바이트를 거르기가 켜진 스캐너(제품 길)와 꺼진 스캐너(옛 길: 모든 CSI가 전부 dispatch)에 먹여 나온 (자리, 신호) 목록을 돌려준다.
    /// 조각 크기마다 따로 먹여 조각 경계에서도 같은지 본다
    fn both(input: &[u8], chunk: usize) -> (Signals, Signals) {
        let run = |slow: bool| {
            let mut s = Scanner {
                no_fast_csi: slow,
                ..Scanner::default()
            };
            let mut out = Vec::new();
            let mut base = 0;
            for c in input.chunks(chunk) {
                out.extend(s.feed_at(c).into_iter().map(|(at, sig)| (base + at, sig)));
                base += c.len();
            }
            out
        };
        (run(false), run(true))
    }

    /// #938: 끝 바이트(0x40~0x7E 전부) x 앞 표시(`?` `>` `=` `<` `:`과 없음) x 중간 바이트 x 대표 매개변수가 옛 길과 같은 신호를 낸다
    #[test]
    fn fast_csi_path_matches_full_dispatch() {
        let prefixes = ["", "?", ">", "=", "<", ":"];
        let inters = ["", "$", "*", "#", "!", "$$", " "];
        let params = [
            "",
            "0",
            "1",
            "2",
            "3",
            "22",
            "4",
            "6",
            "8",
            "8;24;80",
            "8;24",
            "8;1;2;3",
            "21",
            "16",
            "19",
            "14",
            "18",
            "996",
            "998",
            "2031",
            "2033",
            "2048",
            "5522",
            "1;2031",
            "2031;1",
            "2;3",
            "1049",
            "1;2",
            "63",
            "63;5",
            "15",
            "25",
            "26",
            "55",
            "56",
            "62",
            "75",
            "85",
            "1;2;3;4;5;6;7",
            "1;2;3;4;5;6;7;8;9;10;11;12;13;14;15;16;17;18;19;20;21;22;23;24;25;26;27;28;29;30;31;32",
            ";",
            ";;",
            "2;",
            ";2",
            "?",
            "1?2",
        ];
        let mut cases = 0usize;
        let mut with_signal = 0usize;
        for fin in 0x40u8..=0x7e {
            for prefix in prefixes {
                for inter in inters {
                    for p in params {
                        // 앞 표시 + 매개변수 + 중간 바이트 + 끝, 그리고 중간 바이트를 매개변수 앞에 둔 꼴
                        for body in [format!("{prefix}{p}{inter}"), format!("{prefix}{inter}{p}")] {
                            let mut input = b"\x1b[".to_vec();
                            input.extend_from_slice(body.as_bytes());
                            input.push(fin);
                            input.extend_from_slice(b"ok");
                            let (fast, old) = both(&input, input.len());
                            assert_eq!(fast, old, "입력 {:?}", String::from_utf8_lossy(&input));
                            cases += 1;
                            with_signal += usize::from(!old.is_empty());
                        }
                    }
                }
            }
        }
        // 시험이 속 빈 것이 아님을 보인다: 신호가 나오는 경우가 충분히 많다
        assert!(cases > 50_000, "{cases}");
        assert!(with_signal > 300, "{with_signal}");
    }

    /// 거르는 바이트 목록이 실제로 신호가 나는 끝 바이트를 다 담는다: 신호가 나는 끝 바이트는 전부 목록 안이다
    #[test]
    fn every_final_that_makes_a_signal_is_watched() {
        let mut seen = Vec::new();
        for fin in 0x40u8..=0x7e {
            for prefix in ["", "?", ">"] {
                for inter in ["", "$", "*", "#", "!"] {
                    for p in [
                        "", "1", "2", "8;24;80", "21", "16", "19", "996", "998", "2031", "2033", "2048", "5522", "15",
                        "63;1",
                    ] {
                        let mut input = b"\x1b[".to_vec();
                        input.extend_from_slice(format!("{prefix}{p}{inter}").as_bytes());
                        input.push(fin);
                        let mut s = Scanner {
                            no_fast_csi: true,
                            ..Scanner::default()
                        };
                        if !s.feed(&input).is_empty() && !seen.contains(&fin) {
                            seen.push(fin);
                        }
                    }
                }
            }
        }
        seen.sort_unstable();
        let mut want = CSI_WATCHED_FINALS.to_vec();
        want.sort_unstable();
        assert_eq!(String::from_utf8(seen).unwrap(), String::from_utf8(want).unwrap());
    }

    /// 실제 출력 꼴: 색 출력, vim 시작과 끝, 알림·질의가 섞인 것. 조각을 1·7·64·전체로 나눠도 옛 길과 같다
    #[test]
    fn real_output_shapes_match_the_old_path() {
        let mut color = Vec::new();
        for i in 0..200u32 {
            color.extend_from_slice(
                format!(
                    "\x1b[01;34mdir{i}\x1b[0m \x1b[38;5;{}mf\x1b[0m \x1b[38;2;{};{};{}mt\x1b[0m \x1b[1;3;4;31ms\x1b[39;49m\x1b[K\r\n",
                    i % 256,
                    i % 256,
                    (i * 7) % 256,
                    (i * 13) % 256
                )
                .as_bytes(),
            );
        }
        let vim = b"\x1b[?1049h\x1b[22;0;0t\x1b[>4;2m\x1b[?1h\x1b=\x1b[?2004h\x1b[1;24r\x1b[?12h\x1b[?25h\x1b[2 q\x1b[H\x1b[2J\x1b[3J\x1b[6n\x1b[c\x1b[>c\x1b[>0q\x1b[?u\x1b[?996n\x1b[?2031h\x1b[?2033h\x1b[?2048h\x1b[?5522h\x1b[?2031$p\x1b[?2048;2031s\x1b[?2031;5522r\x1b[8;30;100t\x1b[21t\x1b[16t\x1b[19t\x1b[2h\x1b[2l\x1b[1;5;3h\x1b[?25l\x1b[2*y\x1b[1;1;1;2;3;4*y\x1b[7#y\x1b[!p\x1b[?1049l\x1b[23;0;0t".to_vec();
        let mixed = [
            &color[..400],
            b"\x1b]9;build done\x07",
            &vim[..],
            b"\x1b]777;notify;title;body\x1b\\",
            b"\x1b]7501;state=idle\x07\x1b[?63;1n\x1bP+q544e\x1b\\\x1b_Ga=q,i=1;\x1b\\",
            b"\x1b]133;A\x07\x1b[0m\x1b[?2004h",
            &color[400..],
            b"\x1b[1;2\x1b[3m\x1b[2;3\x18\x1b[21t\x1b[?2031\x9b0m\x1b[8;3;4t\xe2\x80\x1b[2J\x1b[3J",
        ]
        .concat();
        for input in [&color[..], &vim[..], &mixed[..]] {
            let mut signals = 0;
            for chunk in [1, 7, 64, input.len()] {
                let (fast, old) = both(input, chunk);
                assert_eq!(fast, old, "조각 {chunk}");
                signals = old.len();
            }
            if input.len() == mixed.len() {
                assert!(signals > 20, "섞은 입력에서 신호가 나와야 한다: {signals}");
            }
        }
    }
}
