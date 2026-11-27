//! 출력 처리량 벤치(실험 E28, #923). PTY를 열지 않고 같은 바이트를 메모리에서 읽기 스레드와 같은 순서로 먹여 단계별 시간을 잰다.
//!
//! 쓰는 법: `cargo run --release -p bt-term --example throughput -- [runs] [chunk 바이트…]`
//! (기본: 5번, 조각 65536). 동작 코드는 바꾸지 않고, 시험 묶음에서 돌지 않는다(예제는 빌드만 된다).
//!
//! 단계(세션 읽기 스레드 `bt-pty-read`와 같은 호출을 공개 API로 맞춘 것):
//! 1. scan: `Scanner::feed_at`만
//! 2. parse: `Processor::advance` + `Term`만(스크롤백 5만 줄, 124x40)
//! 3. both: 스캐너 + 엔진 락 + `advance` + `take_scroll_log`(읽기 한 번마다 `Mutex` 한 번)
//! 4. 조각 크기는 인자로 바꾼다
//!
//! 5. trig·trig-hit(#928): both에 트리거 검사(`Triggers::scan`, 읽기 한 번마다)를 더한 것. trig는 아무 줄에도 맞지 않는 규칙,
//!    trig-hit는 `seq` 줄 대부분에 맞는 규칙(`9 => attention`)이다. `E928_RULE`로 규칙을 바꿀 수 있다(예 `error => notify`)
//!
//! 환경 변수 `E28_STAGE`(scan·parse·both·trig·trig-hit)와 `E28_INPUT`(seq-crlf·seq-lf·seq-bsd·sgr)로 하나만 돌릴 수 있다

use std::hint::black_box;
use std::time::Instant;

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::Processor;
use bt_term::notify::Scanner;
use parking_lot::Mutex;

#[derive(Clone, Copy)]
struct Null;
impl EventListener for Null {}

struct Dims;
impl Dimensions for Dims {
    fn total_lines(&self) -> usize {
        40
    }
    fn screen_lines(&self) -> usize {
        40
    }
    fn columns(&self) -> usize {
        124
    }
}

fn new_term() -> Term<Null> {
    let config = Config {
        scrolling_history: 50_000,
        kitty_keyboard: true,
        ..Config::default()
    };
    Term::new(config, &Dims, Null)
}

/// `seq 1 3000000`을 PTY에 통과시킨 바이트: 줄마다 `\r\n`(onlcr)
fn seq_bytes(crlf: bool) -> Vec<u8> {
    let mut v = Vec::with_capacity(24_000_000);
    for n in 1..=3_000_000u32 {
        v.extend_from_slice(n.to_string().as_bytes());
        if crlf {
            v.push(b'\r');
        }
        v.push(b'\n');
    }
    v
}

/// 이 기계의 `seq 1 3000000`이 실제로 낸 바이트에 PTY처럼 `\r`을 줄마다 붙인 것. macOS(BSD) `seq`는 100만부터 `%g` 꼴
/// (`1e+06`, `1.00001e+06`)로 찍어 정수만 찍는 쪽보다 크다. `seq`가 없으면 빈 목록이다(PTY는 열지 않고 파이프로만 받는다)
fn seq_bsd_bytes() -> Vec<u8> {
    let Ok(out) = std::process::Command::new("seq").args(["1", "3000000"]).output() else {
        return Vec::new();
    };
    let mut v = Vec::with_capacity(out.stdout.len() + 3_100_000);
    for &b in &out.stdout {
        if b == b'\n' {
            v.push(b'\r');
        }
        v.push(b);
    }
    v
}

/// `ls --color`와 `git diff --color` 꼴: 줄마다 SGR 색을 여러 번 켜고 끈다(기본 16색, 256색, 트루컬러, 굵게)
fn sgr_bytes(target: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(target + 512);
    let mut i = 0u32;
    while v.len() < target {
        let f = i % 8;
        v.extend_from_slice(
            format!(
                "\x1b[01;34mdir{i:05}\x1b[0m  \x1b[0;32mrun{i:05}.sh\x1b[0m  \x1b[38;5;{}mfile{i:05}.txt\x1b[0m  \
                 \x1b[38;2;{};{};{}mtruecolor\x1b[0m \x1b[1;3;4;{}mstyled\x1b[0m \x1b[7m{f}\x1b[27m 한글 출력 줄\r\n",
                16 + i % 200,
                i % 256,
                (i * 7) % 256,
                (i * 13) % 256,
                31 + f % 7
            )
            .as_bytes(),
        );
        i += 1;
    }
    v
}

fn run_scan(data: &[u8], chunk: usize) -> usize {
    let mut s = Scanner::default();
    let mut n = 0;
    for c in data.chunks(chunk) {
        n += s.feed_at(black_box(c)).len();
    }
    n
}

fn run_parse(data: &[u8], chunk: usize) -> usize {
    let mut term = new_term();
    let mut p: Processor = Processor::new();
    for c in data.chunks(chunk) {
        p.advance(&mut term, black_box(c));
    }
    term.grid().history_size()
}

fn run_both(data: &[u8], chunk: usize) -> usize {
    let inner: Mutex<(Term<Null>, Processor)> = Mutex::new((new_term(), Processor::new()));
    let mut s = Scanner::default();
    for c in data.chunks(chunk) {
        let signals = s.feed_at(black_box(c));
        let mut g = inner.lock();
        let (term, p) = &mut *g;
        let mut pos = 0;
        for (end, _sig) in signals {
            let end = end.min(c.len());
            p.advance(term, &c[pos..end]);
            pos = end;
        }
        p.advance(term, &c[pos..]);
        let _ = term.take_scroll_log();
    }
    let g = inner.lock();
    g.0.grid().history_size()
}

fn run_trigger(data: &[u8], chunk: usize, rule: &str) -> usize {
    let spec = bt_core::trigger::TriggerSpec::parse(rule).expect("규칙");
    let (mut triggers, errors) = bt_term::triggers::Triggers::new(&[spec]);
    assert!(errors.is_empty());
    let inner: Mutex<(Term<Null>, Processor)> = Mutex::new((new_term(), Processor::new()));
    triggers.arm(&inner.lock().0);
    let mut s = Scanner::default();
    let mut effects = 0;
    for c in data.chunks(chunk) {
        let signals = s.feed_at(black_box(c));
        let mut g = inner.lock();
        let (term, p) = &mut *g;
        let mut pos = 0;
        for (end, _sig) in signals {
            let end = end.min(c.len());
            p.advance(term, &c[pos..end]);
            pos = end;
        }
        p.advance(term, &c[pos..]);
        let _ = term.take_scroll_log();
        effects += triggers.scan(term, Instant::now()).len();
    }
    black_box(effects)
}

fn run_trig(data: &[u8], chunk: usize) -> usize {
    let rule = std::env::var("E928_RULE").unwrap_or_else(|_| "zzqq-no-such-text => attention".into());
    run_trigger(data, chunk, &rule)
}

fn run_trig_hit(data: &[u8], chunk: usize) -> usize {
    let rule = std::env::var("E928_RULE").unwrap_or_else(|_| "9 => attention".into());
    run_trigger(data, chunk, &rule)
}

/// 5단계: `seq 1 3000000`을 **파이프**(PTY 아님)로 받아 읽기 스레드와 같은 64KiB 버퍼로 읽고 3단계와 같이 먹인다.
/// `E28_PIPE=1`일 때만 돈다. 읽는 시간(자식이 쓰는 시간 포함)과 엔진 시간을 나눠 돌려준다
fn run_pipe_both() -> (f64, f64, usize) {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut child = Command::new("seq")
        .args(["1", "3000000"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("seq를 띄운다");
    let mut out = child.stdout.take().expect("파이프");
    let inner: Mutex<(Term<Null>, Processor)> = Mutex::new((new_term(), Processor::new()));
    let mut s = Scanner::default();
    let mut buf = vec![0u8; 64 * 1024];
    let (mut read_s, mut engine_s, mut total) = (0.0, 0.0, 0usize);
    loop {
        let t = Instant::now();
        let n = out.read(&mut buf).expect("읽기");
        read_s += t.elapsed().as_secs_f64();
        if n == 0 {
            break;
        }
        total += n;
        let t = Instant::now();
        let c = &buf[..n];
        let signals = s.feed_at(c);
        let mut g = inner.lock();
        let (term, p) = &mut *g;
        let mut pos = 0;
        for (end, _sig) in signals {
            let end = end.min(c.len());
            p.advance(term, &c[pos..end]);
            pos = end;
        }
        p.advance(term, &c[pos..]);
        let _ = term.take_scroll_log();
        engine_s += t.elapsed().as_secs_f64();
    }
    let _ = child.wait();
    (read_s, engine_s, total)
}

fn measure(name: &str, data: &[u8], chunk: usize, runs: usize, f: fn(&[u8], usize) -> usize) {
    let mut secs = Vec::new();
    let mut check = 0;
    for _ in 0..runs {
        let t = Instant::now();
        check = black_box(f(data, chunk));
        secs.push(t.elapsed().as_secs_f64());
    }
    secs.sort_by(|a, b| a.total_cmp(b));
    let mb = data.len() as f64 / 1e6;
    let med = secs[secs.len() / 2];
    println!(
        "{name:<6} chunk {chunk:>7}  {:>8.1} MB/s (범위 {:.1}~{:.1})  {:>6.2} ms/MB  전체 {:.3}s (범위 {:.3}~{:.3})  [{check}]",
        mb / med,
        mb / secs[secs.len() - 1],
        mb / secs[0],
        med * 1000.0 / mb,
        med,
        secs[0],
        secs[secs.len() - 1],
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let runs: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(5).max(1);
    let mut chunks: Vec<usize> = args.filter_map(|a| a.parse().ok()).filter(|&c| c > 0).collect();
    if chunks.is_empty() {
        chunks.push(64 * 1024);
    }
    if std::env::var("E28_PIPE").is_ok() {
        for i in 0..runs {
            let t = Instant::now();
            let (read_s, engine_s, total) = run_pipe_both();
            let all = t.elapsed().as_secs_f64();
            println!(
                "pipe {i}: {total} 바이트, 전체 {all:.3}s ({:.1} MB/s), 읽기 대기 {read_s:.3}s, 스캐너+엔진 {engine_s:.3}s",
                total as f64 / 1e6 / all
            );
        }
        return;
    }
    let inputs = [
        ("seq-crlf", seq_bytes(true)),
        ("seq-lf", seq_bytes(false)),
        ("seq-bsd", seq_bsd_bytes()),
        ("sgr", sgr_bytes(24_000_000)),
    ];
    let only_stage = std::env::var("E28_STAGE").ok();
    let only_input = std::env::var("E28_INPUT").ok();
    for (label, data) in &inputs {
        if data.is_empty() || only_input.as_deref().is_some_and(|i| i != *label) {
            continue;
        }
        println!("== {label}: {} 바이트, {runs}번", data.len());
        for &chunk in &chunks {
            for (name, f) in [
                ("scan", run_scan as fn(&[u8], usize) -> usize),
                ("parse", run_parse),
                ("both", run_both),
                ("trig", run_trig),
                ("trig-hit", run_trig_hit),
            ] {
                if only_stage.as_deref().is_none_or(|s| s == name) {
                    measure(name, data, chunk, runs, f);
                }
            }
        }
    }
}
