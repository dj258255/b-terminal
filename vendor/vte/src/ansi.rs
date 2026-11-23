// SPDX-License-Identifier: Apache-2.0
//
// This module was originally part of the `alacritty_terminal` crate, which is
// licensed under the Apache License, Version 2.0 and is part of the Alacritty
// project (https://github.com/alacritty/alacritty).

//! ANSI Terminal Stream Parsing.

extern crate alloc;

use alloc::borrow::ToOwned;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::convert::TryFrom;
use core::fmt::{self, Display, Formatter, Write};
#[cfg(feature = "std")]
use core::ops::Mul;
use core::ops::{Add, Sub};
use core::str::FromStr;
use core::time::Duration;
use core::{iter, mem, str};
#[cfg(feature = "std")]
use std::time::Instant;

use bitflags::bitflags;
#[doc(inline)]
pub use cursor_icon;
use cursor_icon::CursorIcon;
use log::debug;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{Params, ParamsIter};

/// Maximum time before a synchronized update is aborted.
const SYNC_UPDATE_TIMEOUT: Duration = Duration::from_millis(150);

/// Maximum number of bytes read in one synchronized update (2MiB).
const SYNC_BUFFER_SIZE: usize = 0x20_0000;

/// Number of bytes in the BSU/ESU CSI sequences.
const SYNC_ESCAPE_LEN: usize = 8;

/// BSU CSI sequence for beginning or extending synchronized updates.
const BSU_CSI: [u8; SYNC_ESCAPE_LEN] = *b"\x1b[?2026h";

/// ESU CSI sequence for terminating synchronized updates.
const ESU_CSI: [u8; SYNC_ESCAPE_LEN] = *b"\x1b[?2026l";

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Hyperlink {
    /// Identifier for the given hyperlink.
    pub id: Option<String>,
    /// Resource identifier of the hyperlink.
    pub uri: String,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// Implementation of [W3C's luminance algorithm].
    ///
    /// [W3C's luminance algorithm]: https://www.w3.org/TR/WCAG20/#relativeluminancedef
    #[cfg(feature = "std")]
    pub fn luminance(self) -> f64 {
        let channel_luminance = |channel| {
            let channel = channel as f64 / 255.;
            if channel <= 0.03928 {
                channel / 12.92
            } else {
                f64::powf((channel + 0.055) / 1.055, 2.4)
            }
        };

        let r_luminance = channel_luminance(self.r);
        let g_luminance = channel_luminance(self.g);
        let b_luminance = channel_luminance(self.b);

        0.2126 * r_luminance + 0.7152 * g_luminance + 0.0722 * b_luminance
    }

    /// Implementation of [W3C's contrast algorithm].
    ///
    /// [W3C's contrast algorithm]: https://www.w3.org/TR/WCAG20/#contrast-ratiodef
    #[cfg(feature = "std")]
    pub fn contrast(self, other: Rgb) -> f64 {
        let self_luminance = self.luminance();
        let other_luminance = other.luminance();

        let (darker, lighter) = if self_luminance > other_luminance {
            (other_luminance, self_luminance)
        } else {
            (self_luminance, other_luminance)
        };

        (lighter + 0.05) / (darker + 0.05)
    }
}

// A multiply function for Rgb, as the default dim is just *2/3.
#[cfg(feature = "std")]
impl Mul<f32> for Rgb {
    type Output = Rgb;

    fn mul(self, rhs: f32) -> Rgb {
        let result = Rgb {
            r: (f32::from(self.r) * rhs).clamp(0.0, 255.0) as u8,
            g: (f32::from(self.g) * rhs).clamp(0.0, 255.0) as u8,
            b: (f32::from(self.b) * rhs).clamp(0.0, 255.0) as u8,
        };

        log::trace!("Scaling RGB by {} from {:?} to {:?}", rhs, self, result);
        result
    }
}

impl Add<Rgb> for Rgb {
    type Output = Rgb;

    fn add(self, rhs: Rgb) -> Rgb {
        Rgb {
            r: self.r.saturating_add(rhs.r),
            g: self.g.saturating_add(rhs.g),
            b: self.b.saturating_add(rhs.b),
        }
    }
}

impl Sub<Rgb> for Rgb {
    type Output = Rgb;

    fn sub(self, rhs: Rgb) -> Rgb {
        Rgb {
            r: self.r.saturating_sub(rhs.r),
            g: self.g.saturating_sub(rhs.g),
            b: self.b.saturating_sub(rhs.b),
        }
    }
}

impl Display for Rgb {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

impl FromStr for Rgb {
    type Err = ();

    fn from_str(s: &str) -> Result<Rgb, ()> {
        let chars = if s.starts_with("0x") && s.len() == 8 {
            &s[2..]
        } else if s.starts_with('#') && s.len() == 7 {
            &s[1..]
        } else {
            return Err(());
        };

        match u32::from_str_radix(chars, 16) {
            Ok(mut color) => {
                let b = (color & 0xFF) as u8;
                color >>= 8;
                let g = (color & 0xFF) as u8;
                color >>= 8;
                let r = color as u8;
                Ok(Rgb { r, g, b })
            },
            Err(_) => Err(()),
        }
    }
}

/// X11 색 이름 표(`rgb.txt`). 출처·라이선스는 `vendor/README.md`. b-terminal(#765)
const X11_RGB: &str = include_str!("x11_rgb.txt");

/// 색 이름(대소문자 무시, 안쪽 공백은 그대로)으로 X11 표에서 찾는다. 줄 모양은 `RRR GGG BBB<탭>이름`(열 고정).
fn x11_color(name: &[u8]) -> Option<Rgb> {
    let name = str::from_utf8(name).ok()?;
    for line in X11_RGB.lines() {
        let line = line.trim_end_matches('\r');
        let entry = match line.get(12..) {
            Some(entry) => entry.trim_matches([' ', '\t']),
            None => continue,
        };
        if !entry.eq_ignore_ascii_case(name) {
            continue;
        }
        let channel = |range: core::ops::Range<usize>| line.get(range)?.trim().parse::<u8>().ok();
        return Some(Rgb { r: channel(0..3)?, g: channel(4..7)?, b: channel(8..11)? });
    }
    None
}

/// 16진수 한 채널(1~4자리, 숫자만)을 0~255로 줄여 맞춘다(Ghostty `RGB.fromHex`). `+`·`-`·`_`는 받지 않는다.
fn scale_hex_channel(input: &[u8]) -> Option<u8> {
    if input.is_empty() || input.len() > 4 || !input.iter().all(u8::is_ascii_hexdigit) {
        return None;
    }
    let value = u32::from_str_radix(str::from_utf8(input).ok()?, 16).ok()?;
    let max = u32::pow(16, input.len() as u32) - 1;
    Some((255 * value / max) as u8)
}

/// `0.0..=1.0` 십진 분수(Ghostty `fraction.parse`). 부호 하나, 숫자와 소수점 하나, 숫자 한 자리 이상, 지수 없음.
fn parse_fraction(input: &[u8]) -> Option<f64> {
    let (negative, digits) = match input.first() {
        Some(b'+') => (false, &input[1..]),
        Some(b'-') => (true, &input[1..]),
        _ => (false, input),
    };
    let (int_part, frac_part) = match digits.iter().position(|&b| b == b'.') {
        Some(dot) => (&digits[..dot], Some(&digits[dot + 1..])),
        None => (digits, None),
    };
    if !int_part.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut count = int_part.len();
    let mut int_value = 0f64;
    for &d in int_part {
        int_value = int_value * 10.0 + f64::from(d - b'0');
    }
    let (mut frac, mut scale) = (0u64, 1u64);
    if let Some(frac_part) = frac_part {
        for &d in frac_part {
            if !d.is_ascii_digit() {
                return None;
            }
            // 15자리까지만 모아 f64로 정확히 나눈다(Ghostty와 같게)
            if scale < 1_000_000_000_000_000 {
                frac = frac * 10 + u64::from(d - b'0');
                scale *= 10;
            }
            count += 1;
        }
    }
    if count == 0 {
        return None;
    }
    let magnitude = int_value + frac as f64 / scale as f64;
    let value = if negative { -magnitude } else { magnitude };
    (0.0..=1.0).contains(&value).then_some(value)
}

/// 색 문자열을 Ghostty `RGB.parse`(color.zig 761줄)처럼 읽는다. b-terminal(#765)
///
/// 앞뒤 공백·탭은 무시한다. 받는 꼴: `#rgb`·`#rrggbb`·`#rrrgggbbb`·`#rrrrggggbbbb`, X11 색 이름(대소문자 무시),
/// 이름이 아니면 `rgb`·`rrggbb`(`#` 없음), `rgb:r/g/b`(채널마다 1~4자리), `rgbi:r/g/b`(0.0~1.0).
///
/// `#` 꼴만은 Ghostty가 채널을 0~255로 줄여 맞추는 것(`#1af`가 `11aaff`)과 달리 xterm처럼 낮은 자리를 0으로 채운다
/// (`#1af`가 `10a0f0`). 전부터 xterm과 같던 곳이라 그대로 둔다. 자릿수는 3의 배수 3~12만 받는다.
fn xparse_color(color: &[u8]) -> Option<Rgb> {
    let is_blank = |b: &u8| matches!(b, b' ' | b'\t');
    let start = color.iter().position(|b| !is_blank(b))?;
    let end = color.iter().rposition(|b| !is_blank(b))? + 1;
    let input = &color[start..end];
    if input[0] == b'#' {
        let digits = &input[1..];
        if !matches!(digits.len(), 3 | 6 | 9 | 12) || !digits.iter().all(u8::is_ascii_hexdigit) {
            return None;
        }
        return parse_legacy_color(digits);
    }

    if let Some(rgb) = x11_color(input) {
        return Some(rgb);
    }

    if matches!(input.len(), 3 | 6) {
        let n = input.len() / 3;
        return Some(Rgb {
            r: scale_hex_channel(&input[..n])?,
            g: scale_hex_channel(&input[n..2 * n])?,
            b: scale_hex_channel(&input[2 * n..])?,
        });
    }

    if let Some(rest) = input.strip_prefix(b"rgb:") {
        return parse_rgb_color(rest, scale_hex_channel);
    }
    if let Some(rest) = input.strip_prefix(b"rgbi:") {
        return parse_rgb_color(rest, |part| {
            Some((parse_fraction(part)? * 255.0) as u8)
        });
    }
    None
}

/// `r/g/b` 세 채널을 `channel`로 읽는다.
fn parse_rgb_color(color: &[u8], channel: impl Fn(&[u8]) -> Option<u8>) -> Option<Rgb> {
    let mut parts = color.split(|&b| b == b'/');
    let (r, g, b) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    Some(Rgb { r: channel(r)?, g: channel(g)?, b: channel(b)? })
}

/// `#r(rrr)g(ggg)b(bbb)`의 자릿수 부분(16진수만, 3의 배수)을 xterm처럼 읽는다: 낮은 자리를 버리거나 0으로 채운다.
fn parse_legacy_color(color: &[u8]) -> Option<Rgb> {
    let item_len = color.len() / 3;

    let color_from_slice = |slice: &[u8]| {
        let col = usize::from_str_radix(str::from_utf8(slice).ok()?, 16).ok()? << 4;
        Some((col >> (4 * slice.len().saturating_sub(1))) as u8)
    };

    Some(Rgb {
        r: color_from_slice(&color[0..item_len])?,
        g: color_from_slice(&color[item_len..item_len * 2])?,
        b: color_from_slice(&color[item_len * 2..])?,
    })
}

/// OSC 4·104의 색 번호(숫자만, 0~511). 256~259는 Ghostty의 특수 색 자리, 그 위는 `None`(Ghostty `u9` 바깥). b-terminal(#765)
fn parse_color_index(input: &[u8]) -> Option<u16> {
    if input.is_empty() || !input.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let mut value = 0u16;
    for &d in input {
        value = value.checked_mul(10)?.checked_add(u16::from(d - b'0'))?;
        if value > 511 {
            return None;
        }
    }
    Some(value)
}

/// 색 OSC(4·10~12·104·110~112)의 인자 조각. Ghostty `tokenizeScalar(';')`처럼 빈 조각을 건너뛴다.
/// 16번째 칸에 `;`가 글자로 들어올 수 있어 칸마다 `;`로 다시 가른다. b-terminal(#765)
fn color_tokens<'a>(params: &'a [&'a [u8]]) -> impl Iterator<Item = &'a [u8]> {
    params[1..].iter().flat_map(|p| p.split(|&b| b == b';')).filter(|t| !t.is_empty())
}

/// Ghostty 고정 캡처의 크기(`osc.zig` `Parser.MAX_BUF`). OSC 0·2·8의 `N;` 뒤 글(끝 널 한 바이트 포함)이 이 크기에
/// 닿으면 명령 전체가 없다: 2047바이트까지 받고 2048부터 버린다. b-terminal(#765)
const OSC_MAX_BUF: usize = 2048;

/// `params[0]` 뒤 인자를 `;`로 이었을 때의 바이트 수
fn osc_payload_len(params: &[&[u8]]) -> usize {
    let rest = &params[1..];
    rest.iter().map(|p| p.len()).sum::<usize>() + rest.len().saturating_sub(1)
}

/// OSC 8 매개변수(`key=value:key=value`)에서 `id`를 Ghostty `hyperlink.zig`처럼 읽는다. 값이 빈 `id=`는 없음이고,
/// `=` 없는 조각을 만나면 거기서 멈춘다. b-terminal(#765)
fn osc8_id(link_params: &[u8]) -> Option<String> {
    // Ghostty는 `:`와 끝 `;`를 널로 바꾼 뒤 널 사이를 한 조각씩 읽는다. 첫 조각은 둘째 널부터 찾는다(앞이 빈 조각이면 합쳐진다).
    let mut kvs: Vec<u8> = link_params.iter().map(|&b| if b == b':' { 0 } else { b }).collect();
    kvs.push(0);
    let mut id = None;
    let mut start = 0;
    while start < kvs.len() {
        let Some(end) = kvs[start + 1..].iter().position(|&b| b == 0).map(|i| start + 1 + i) else {
            break;
        };
        let kv = &kvs[start..=end];
        let Some(eq) = kv.iter().position(|&b| b == b'=') else {
            break;
        };
        if &kv[..eq] == b"id" {
            let value = &kv[eq + 1..kv.len() - 1];
            if !value.is_empty() {
                id = str::from_utf8(value).ok().map(str::to_owned);
            }
        }
        start = end + 1;
    }
    id
}

/// Internal state for VTE processor.
#[derive(Debug, Default)]
struct ProcessorState<T: Timeout> {
    /// Last processed character for repetition.
    preceding_char: Option<char>,

    /// State for synchronized terminal updates.
    sync_state: SyncState<T>,
}

#[derive(Debug)]
struct SyncState<T: Timeout> {
    /// Handler for synchronized updates.
    timeout: T,

    /// Bytes read during the synchronized update.
    buffer: Vec<u8>,
}

impl<T: Timeout> Default for SyncState<T> {
    fn default() -> Self {
        Self { buffer: Vec::with_capacity(SYNC_BUFFER_SIZE), timeout: Default::default() }
    }
}

/// The processor wraps a `crate::Parser` to ultimately call methods on a
/// Handler.
#[cfg(feature = "std")]
#[derive(Default)]
pub struct Processor<T: Timeout = StdSyncHandler> {
    state: ProcessorState<T>,
    parser: crate::Parser,
}

/// The processor wraps a `crate::Parser` to ultimately call methods on a
/// Handler.
#[cfg(not(feature = "std"))]
#[derive(Default)]
pub struct Processor<T: Timeout> {
    state: ProcessorState<T>,
    parser: crate::Parser,
}

impl<T: Timeout> Processor<T> {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Synchronized update timeout.
    pub fn sync_timeout(&self) -> &T {
        &self.state.sync_state.timeout
    }

    /// Process a new byte from the PTY.
    #[inline]
    pub fn advance<H>(&mut self, handler: &mut H, bytes: &[u8])
    where
        H: Handler,
    {
        let mut processed = 0;
        while processed != bytes.len() {
            if self.state.sync_state.timeout.pending_timeout() {
                processed += self.advance_sync(handler, &bytes[processed..]);
            } else {
                let mut performer = Performer::new(&mut self.state, handler);
                processed +=
                    self.parser.advance_until_terminated(&mut performer, &bytes[processed..]);
            }
        }
    }

    /// End a synchronized update.
    pub fn stop_sync<H>(&mut self, handler: &mut H)
    where
        H: Handler,
    {
        self.stop_sync_internal(handler, None);
    }

    /// End a synchronized update.
    ///
    /// The `bsu_offset` parameter should be passed if the sync buffer contains
    /// a new BSU escape that is not part of the current synchronized
    /// update.
    fn stop_sync_internal<H>(&mut self, handler: &mut H, bsu_offset: Option<usize>)
    where
        H: Handler,
    {
        // Process all synchronized bytes.
        //
        // NOTE: We do not use `advance_until_terminated` here since BSU sequences are
        // processed automatically during the synchronized update.
        let buffer = mem::take(&mut self.state.sync_state.buffer);
        let offset = bsu_offset.unwrap_or(buffer.len());
        let mut performer = Performer::new(&mut self.state, handler);
        self.parser.advance(&mut performer, &buffer[..offset]);
        self.state.sync_state.buffer = buffer;

        match bsu_offset {
            // Just clear processed bytes if there is a new BSU.
            //
            // NOTE: We do not need to re-process for a new ESU since the `advance_sync`
            // function checks for BSUs in reverse.
            Some(bsu_offset) => {
                let new_len = self.state.sync_state.buffer.len() - bsu_offset;
                self.state.sync_state.buffer.copy_within(bsu_offset.., 0);
                self.state.sync_state.buffer.truncate(new_len);
            },
            // Report mode and clear state if no new BSU is present.
            None => {
                handler.unset_private_mode(NamedPrivateMode::SyncUpdate.into());
                self.state.sync_state.timeout.clear_timeout();
                self.state.sync_state.buffer.clear();
            },
        }
    }

    /// Number of bytes in the synchronization buffer.
    #[inline]
    pub fn sync_bytes_count(&self) -> usize {
        self.state.sync_state.buffer.len()
    }

    /// Process a new byte during a synchronized update.
    ///
    /// Returns the number of bytes processed.
    #[cold]
    fn advance_sync<H>(&mut self, handler: &mut H, bytes: &[u8]) -> usize
    where
        H: Handler,
    {
        // Advance sync parser or stop sync if we'd exceed the maximum buffer size.
        if self.state.sync_state.buffer.len() + bytes.len() >= SYNC_BUFFER_SIZE - 1 {
            // Terminate the synchronized update.
            self.stop_sync_internal(handler, None);

            // Just parse the bytes normally.
            let mut performer = Performer::new(&mut self.state, handler);
            self.parser.advance_until_terminated(&mut performer, bytes)
        } else {
            self.state.sync_state.buffer.extend(bytes);
            self.advance_sync_csi(handler, bytes.len());
            bytes.len()
        }
    }

    /// Handle BSU/ESU CSI sequences during synchronized update.
    fn advance_sync_csi<H>(&mut self, handler: &mut H, new_bytes: usize)
    where
        H: Handler,
    {
        // Get constraints within which a new escape character might be relevant.
        let buffer_len = self.state.sync_state.buffer.len();
        let start_offset = (buffer_len - new_bytes).saturating_sub(SYNC_ESCAPE_LEN - 1);
        let end_offset = buffer_len.saturating_sub(SYNC_ESCAPE_LEN - 1);
        let search_buffer = &self.state.sync_state.buffer[start_offset..end_offset];

        // Search for termination/extension escapes in the added bytes.
        //
        // NOTE: It is technically legal to specify multiple private modes in the same
        // escape, but we only allow EXACTLY `\e[?2026h`/`\e[?2026l` to keep the parser
        // more simple.
        let mut bsu_offset = None;
        for index in memchr::memchr_iter(0x1B, search_buffer).rev() {
            let offset = start_offset + index;
            let escape = &self.state.sync_state.buffer[offset..offset + SYNC_ESCAPE_LEN];

            if escape == BSU_CSI {
                self.state.sync_state.timeout.set_timeout(SYNC_UPDATE_TIMEOUT);
                bsu_offset = Some(offset);
            } else if escape == ESU_CSI {
                self.stop_sync_internal(handler, bsu_offset);
                break;
            }
        }
    }
}

/// Helper type that implements `crate::Perform`.
///
/// Processor creates a Performer when running advance and passes the Performer
/// to `crate::Parser`.
struct Performer<'a, H: Handler, T: Timeout> {
    state: &'a mut ProcessorState<T>,
    handler: &'a mut H,

    /// Whether the parser should be prematurely terminated.
    terminated: bool,
}

impl<'a, H: Handler + 'a, T: Timeout> Performer<'a, H, T> {
    /// Create a performer.
    #[inline]
    pub fn new<'b>(state: &'b mut ProcessorState<T>, handler: &'b mut H) -> Performer<'b, H, T> {
        Performer { state, handler, terminated: Default::default() }
    }
}

#[cfg(feature = "std")]
#[derive(Default)]
pub struct StdSyncHandler {
    timeout: Option<Instant>,
}

#[cfg(feature = "std")]
impl StdSyncHandler {
    /// Synchronized update expiration time.
    #[inline]
    pub fn sync_timeout(&self) -> Option<Instant> {
        self.timeout
    }
}

#[cfg(feature = "std")]
impl Timeout for StdSyncHandler {
    #[inline]
    fn set_timeout(&mut self, duration: Duration) {
        self.timeout = Some(Instant::now() + duration);
    }

    #[inline]
    fn clear_timeout(&mut self) {
        self.timeout = None;
    }

    #[inline]
    fn pending_timeout(&self) -> bool {
        self.timeout.is_some()
    }
}

/// Interface for creating timeouts and checking their expiry.
///
/// This is internally used by the [`Processor`] to handle synchronized
/// updates.
pub trait Timeout: Default {
    /// Sets the timeout for the next synchronized update.
    ///
    /// The `duration` parameter specifies the duration of the timeout. Once the
    /// specified duration has elapsed, the synchronized update rotuine can be
    /// performed.
    fn set_timeout(&mut self, duration: Duration);
    /// Clear the current timeout.
    fn clear_timeout(&mut self);
    /// Returns whether a timeout is currently active and has not yet expired.
    fn pending_timeout(&self) -> bool;
}

/// Type that handles actions from the parser.
///
/// XXX Should probably not provide default impls for everything, but it makes
/// writing specific handler impls for tests far easier.
pub trait Handler {
    /// OSC to set window title.
    fn set_title(&mut self, _: Option<String>) {}

    /// Set the cursor style.
    fn set_cursor_style(&mut self, _: Option<CursorStyle>) {}

    /// Set the cursor shape.
    fn set_cursor_shape(&mut self, _shape: CursorShape) {}

    /// A character to be displayed.
    fn input(&mut self, _c: char) {}

    /// Set cursor to position.
    fn goto(&mut self, _line: i32, _col: usize) {}

    /// Set cursor to specific row.
    fn goto_line(&mut self, _line: i32) {}

    /// Set cursor to specific column.
    fn goto_col(&mut self, _col: usize) {}

    /// Insert blank characters in current line starting from cursor.
    fn insert_blank(&mut self, _: usize) {}

    /// Move cursor up `rows`.
    fn move_up(&mut self, _: usize) {}

    /// Move cursor down `rows`.
    fn move_down(&mut self, _: usize) {}

    /// Identify the terminal (should write back to the pty stream).
    fn identify_terminal(&mut self, _intermediate: Option<char>) {}

    /// Report device status.
    fn device_status(&mut self, _: usize) {}

    /// b-terminal(#327): DECXCPR(`CSI ? 6 n`). 답은 `ESC [ ? 행 ; 열 ; 1 R`(쪽 번호는 늘 1)
    fn report_cursor_position_extended(&mut self) {}

    /// b-terminal(#327): XTSHIFTESCAPE(`CSI > Ps s`). Shift 마우스를 보고에 넣을지 프로그램이 정한다
    fn set_mouse_shift_capture(&mut self, _capture: bool) {}

    /// b-terminal(#327): ED 22(`CSI 22 J`). 보이는 화면을 스크롤백으로 밀고 지운다
    fn clear_screen_scroll_complete(&mut self) {}

    /// Move cursor forward `cols`.
    fn move_forward(&mut self, _col: usize) {}

    /// Move cursor backward `cols`.
    fn move_backward(&mut self, _col: usize) {}

    /// Move cursor down `rows` and set to column 1.
    fn move_down_and_cr(&mut self, _row: usize) {}

    /// Move cursor up `rows` and set to column 1.
    fn move_up_and_cr(&mut self, _row: usize) {}

    /// Put `count` tabs.
    fn put_tab(&mut self, _count: u16) {}

    /// Backspace `count` characters.
    fn backspace(&mut self) {}

    /// Carriage return.
    fn carriage_return(&mut self) {}

    /// Linefeed.
    fn linefeed(&mut self) {}

    /// Ring the bell.
    ///
    /// Hopefully this is never implemented.
    fn bell(&mut self) {}

    /// Substitute char under cursor.
    fn substitute(&mut self) {}

    /// Newline.
    fn newline(&mut self) {}

    /// Set current position as a tabstop.
    fn set_horizontal_tabstop(&mut self) {}

    /// Scroll up `rows` rows.
    fn scroll_up(&mut self, _: usize) {}

    /// Scroll down `rows` rows.
    fn scroll_down(&mut self, _: usize) {}

    /// Insert `count` blank lines.
    fn insert_blank_lines(&mut self, _: usize) {}

    /// Delete `count` lines.
    fn delete_lines(&mut self, _: usize) {}

    /// Erase `count` chars in current line following cursor.
    ///
    /// Erase means resetting to the default state (default colors, no content,
    /// no mode flags).
    fn erase_chars(&mut self, _: usize) {}

    /// Delete `count` chars.
    ///
    /// Deleting a character is like the delete key on the keyboard - everything
    /// to the right of the deleted things is shifted left.
    fn delete_chars(&mut self, _: usize) {}

    /// Move backward `count` tabs.
    fn move_backward_tabs(&mut self, _count: u16) {}

    /// Move forward `count` tabs.
    fn move_forward_tabs(&mut self, _count: u16) {}

    /// Save current cursor position.
    fn save_cursor_position(&mut self) {}

    /// b-terminal(#311): `CSI s`(매개변수 없음). 좌우 여백 모드(69)가 켜져 있으면 여백을 되돌리고(DECSLRM), 아니면 커서를 저장한다(SCOSC)
    fn save_cursor_or_reset_margins(&mut self) {
        self.save_cursor_position();
    }

    /// b-terminal(#311): DECSLRM(`CSI Pl ; Pr s`). 1부터 세고 0은 기본값(왼쪽 1, 오른쪽 끝)이다
    fn set_left_right_margins(&mut self, _left: usize, _right: usize) {}

    /// Restore cursor position.
    fn restore_cursor_position(&mut self) {}

    /// Clear current line.
    fn clear_line(&mut self, _mode: LineClearMode) {}

    /// Clear screen.
    fn clear_screen(&mut self, _mode: ClearMode) {}

    /// b-terminal(#294): DECSED(`CSI ? Ps J`). Clear screen, keeping protected cells.
    fn selective_clear_screen(&mut self, _mode: ClearMode) {}

    /// b-terminal(#294): DECSEL(`CSI ? Ps K`). Clear line, keeping protected cells.
    fn selective_clear_line(&mut self, _mode: LineClearMode) {}

    /// b-terminal(#294): DECSCA(`CSI Ps " q`), SPA(`ESC V`) and EPA(`ESC W`).
    fn set_protected_mode(&mut self, _mode: ProtectedMode) {}

    /// Clear tab stops.
    fn clear_tabs(&mut self, _mode: TabulationClearMode) {}

    /// Set tab stops at every `interval`.
    fn set_tabs(&mut self, _interval: u16) {}

    /// Reset terminal state.
    fn reset_state(&mut self) {}

    /// Reverse Index.
    ///
    /// Move the active position to the same horizontal position on the
    /// preceding line. If the active position is at the top margin, a scroll
    /// down is performed.
    fn reverse_index(&mut self) {}

    /// DECIC: insert `count` blank columns at the cursor column in every row of the scroll region.
    // b-terminal(#295)
    fn insert_columns(&mut self, _count: usize) {}

    /// DECDC: delete `count` columns at the cursor column in every row of the scroll region.
    // b-terminal(#295)
    fn delete_columns(&mut self, _count: usize) {}

    /// DECBI: move the cursor left, or insert a column at the left edge.
    // b-terminal(#295)
    fn back_index(&mut self) {}

    /// DECFI: move the cursor right, or delete the left column at the right edge.
    // b-terminal(#295)
    fn forward_index(&mut self) {}

    /// SS2/SS3: use the given charset for exactly the next printed character.
    // b-terminal(#295)
    fn set_single_shift(&mut self, _: CharsetIndex) {}

    /// Set a terminal attribute.
    fn terminal_attribute(&mut self, _attr: Attr) {}

    /// Set mode.
    fn set_mode(&mut self, _mode: Mode) {}

    /// Unset mode.
    fn unset_mode(&mut self, _mode: Mode) {}

    /// DECRPM - report mode.
    fn report_mode(&mut self, _mode: Mode) {}

    /// Set private mode.
    fn set_private_mode(&mut self, _mode: PrivateMode) {}

    /// Unset private mode.
    fn unset_private_mode(&mut self, _mode: PrivateMode) {}

    /// DECRPM - report private mode.
    fn report_private_mode(&mut self, _mode: PrivateMode) {}

    /// XTSAVE (`CSI ? Pm s`) - save a private mode's state. b-terminal(#313)
    fn save_private_mode(&mut self, _mode: PrivateMode) {}

    /// XTRESTORE (`CSI ? Pm r`) - restore a private mode saved by XTSAVE. b-terminal(#313)
    fn restore_private_mode(&mut self, _mode: PrivateMode) {}

    /// DECSTBM - Set the terminal scrolling region.
    fn set_scrolling_region(&mut self, _top: usize, _bottom: Option<usize>) {}

    /// DECKPAM - Set keypad to applications mode (ESCape instead of digits).
    fn set_keypad_application_mode(&mut self) {}

    /// DECKPNM - Set keypad to numeric mode (digits instead of ESCape seq).
    fn unset_keypad_application_mode(&mut self) {}

    /// Set one of the graphic character sets, G0 to G3, as the active charset.
    ///
    /// 'Invoke' one of G0 to G3 in the GL area. Also referred to as shift in,
    /// shift out and locking shift depending on the set being activated.
    fn set_active_charset(&mut self, _: CharsetIndex) {}

    /// Assign a graphic character set to G0, G1, G2 or G3.
    ///
    /// 'Designate' a graphic character set as one of G0 to G3, so that it can
    /// later be 'invoked' by `set_active_charset`.
    fn configure_charset(&mut self, _: CharsetIndex, _: StandardCharset) {}

    /// Set an indexed color value.
    fn set_color(&mut self, _: usize, _: Rgb) {}

    /// Respond to a color query escape sequence.
    fn dynamic_color_sequence(&mut self, _: String, _: usize, _: &str) {}

    /// Reset an indexed color to original value.
    fn reset_color(&mut self, _: usize) {}

    /// Store data into clipboard.
    fn clipboard_store(&mut self, _: u8, _: &[u8]) {}

    /// Load data from clipboard.
    fn clipboard_load(&mut self, _: u8, _: &str) {}

    /// Run the decaln routine.
    fn decaln(&mut self) {}

    /// Push a title onto the stack.
    fn push_title(&mut self) {}

    /// Pop the last title from the stack.
    fn pop_title(&mut self) {}

    /// Report text area size in pixels.
    fn text_area_size_pixels(&mut self) {}

    /// Report text area size in characters.
    fn text_area_size_chars(&mut self) {}

    /// Set hyperlink.
    fn set_hyperlink(&mut self, _: Option<Hyperlink>) {}

    /// Set mouse cursor icon.
    fn set_mouse_cursor_icon(&mut self, _: CursorIcon) {}

    /// Report current keyboard mode.
    fn report_keyboard_mode(&mut self) {}

    /// Push keyboard mode into the keyboard mode stack.
    fn push_keyboard_mode(&mut self, _mode: KeyboardModes) {}

    /// Pop the given amount of keyboard modes from the
    /// keyboard mode stack.
    fn pop_keyboard_modes(&mut self, _to_pop: u16) {}

    /// Set the [`keyboard mode`] using the given [`behavior`].
    ///
    /// [`keyboard mode`]: crate::ansi::KeyboardModes
    /// [`behavior`]: crate::ansi::KeyboardModesApplyBehavior
    fn set_keyboard_mode(&mut self, _mode: KeyboardModes, _behavior: KeyboardModesApplyBehavior) {}

    /// Set XTerm's [`ModifyOtherKeys`] option.
    fn set_modify_other_keys(&mut self, _mode: ModifyOtherKeys) {}

    /// Report XTerm's [`ModifyOtherKeys`] state.
    ///
    /// The output is of form `CSI > 4 ; mode m`.
    fn report_modify_other_keys(&mut self) {}

    // Set SCP control.
    fn set_scp(&mut self, _char_path: ScpCharPath, _update_mode: ScpUpdateMode) {}
}

bitflags! {
    /// A set of [`kitty keyboard protocol'] modes.
    ///
    /// [`kitty keyboard protocol']: https://sw.kovidgoyal.net/kitty/keyboard-protocol
    #[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct KeyboardModes : u8 {
        /// No keyboard protocol mode is set.
        const NO_MODE                 = 0b0000_0000;
        /// Report `Esc`, `alt` + `key`, `ctrl` + `key`, `ctrl` + `alt` + `key`, `shift`
        /// + `alt` + `key` keys using `CSI u` sequence instead of raw ones.
        const DISAMBIGUATE_ESC_CODES  = 0b0000_0001;
        /// Report key presses, release, and repetition alongside the escape. Key events
        /// that result in text are reported as plain UTF-8, unless the
        /// [`Self::REPORT_ALL_KEYS_AS_ESC`] is enabled.
        const REPORT_EVENT_TYPES      = 0b0000_0010;
        /// Additionally report shifted key an dbase layout key.
        const REPORT_ALTERNATE_KEYS   = 0b0000_0100;
        /// Report every key as an escape sequence.
        const REPORT_ALL_KEYS_AS_ESC  = 0b0000_1000;
        /// Report the text generated by the key event.
        const REPORT_ASSOCIATED_TEXT  = 0b0001_0000;
    }
}

/// XTMODKEYS modifyOtherKeys state.
///
/// This only applies to keys corresponding to ascii characters.
///
/// For the details on how to implement the mode handling correctly, consult
/// [`XTerm's implementation`] and the [`output`] of XTerm's provided [`perl
/// script`]. Some libraries and implementations also use the [`fixterms`]
/// definition of the `CSI u`.
///
/// The end escape sequence has a `CSI char; modifiers u` form while the
/// original `CSI 27 ; modifier ; char ~`. The clients should prefer the `CSI
/// u`, since it has more adoption.
///
/// [`XTerm's implementation`]: https://invisible-island.net/xterm/modified-keys.html
/// [`perl script`]: https://github.com/ThomasDickey/xterm-snapshots/blob/master/vttests/modify-keys.pl
/// [`output`]: https://github.com/alacritty/vte/blob/master/doc/modifyOtherKeys-example.txt
/// [`fixterms`]: http://www.leonerd.org.uk/hacks/fixterms/
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModifyOtherKeys {
    /// Reset the state.
    Reset,
    /// Enables this feature except for keys with well-known behavior, e.g.,
    /// Tab, Backspace and some special control character cases which are
    /// built into the X11 library (e.g., Control-Space to make a NUL, or
    /// Control-3 to make an Escape character).
    ///
    /// Escape sequences shouldn't be emitted under the following circumstances:
    /// - When the key is in range of `[64;127]` and the modifier is either
    ///   Control or Shift
    /// - When the key combination is a known control combination alias
    ///
    /// For more details, consult the [`example`] for the suggested translation.
    ///
    /// [`example`]: https://github.com/alacritty/vte/blob/master/doc/modifyOtherKeys-example.txt
    EnableExceptWellDefined,
    /// Enables this feature for all keys including the exceptions of
    /// [`Self::EnableExceptWellDefined`].  XTerm still ignores the special
    /// cases built into the X11 library. Any shifted (modified) ordinary
    /// key send an escape sequence. The Alt- and Meta- modifiers cause
    /// XTerm to send escape sequences.
    ///
    /// For more details, consult the [`example`] for the suggested translation.
    ///
    /// [`example`]: https://github.com/alacritty/vte/blob/master/doc/modifyOtherKeys-example.txt
    EnableAll,
}

/// Describes how the new [`KeyboardModes`] should be applied.
#[repr(u8)]
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardModesApplyBehavior {
    /// Replace the active flags with the new ones.
    #[default]
    Replace,
    /// Merge the given flags with currently active ones.
    Union,
    /// Remove the given flags from the active ones.
    Difference,
}

/// Terminal cursor configuration.
#[derive(Default, Debug, Eq, PartialEq, Copy, Clone, Hash)]
pub struct CursorStyle {
    pub shape: CursorShape,
    pub blinking: bool,
}

/// Terminal cursor shape.
#[derive(Debug, Default, Eq, PartialEq, Copy, Clone, Hash)]
pub enum CursorShape {
    /// Cursor is a block like `▒`.
    #[default]
    Block,

    /// Cursor is an underscore like `_`.
    Underline,

    /// Cursor is a vertical bar `⎸`.
    Beam,

    /// Cursor is a box like `☐`.
    HollowBlock,

    /// Invisible cursor.
    Hidden,
}

/// Wrapper for the ANSI modes.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Mode {
    /// Known ANSI mode.
    Named(NamedMode),
    /// Unidentified publc mode.
    Unknown(u16),
}

impl Mode {
    fn new(mode: u16) -> Self {
        match mode {
            2 => Self::Named(NamedMode::KeyboardAction), // b-terminal(#302)
            4 => Self::Named(NamedMode::Insert),
            12 => Self::Named(NamedMode::SendReceive), // b-terminal(#302)
            20 => Self::Named(NamedMode::LineFeedNewLine),
            _ => Self::Unknown(mode),
        }
    }

    /// Get the raw value of the mode.
    pub fn raw(self) -> u16 {
        match self {
            Self::Named(named) => named as u16,
            Self::Unknown(mode) => mode,
        }
    }
}

impl From<NamedMode> for Mode {
    fn from(value: NamedMode) -> Self {
        Self::Named(value)
    }
}

/// ANSI modes.
#[repr(u16)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NamedMode {
    /// KAM Keyboard Action Mode. b-terminal(#302): 상태만 두고 DECRQM이 답한다
    KeyboardAction = 2,
    /// IRM Insert Mode.
    Insert = 4,
    /// SRM Send/Receive Mode (기본 켬). b-terminal(#302)
    SendReceive = 12,
    LineFeedNewLine = 20,
}

/// Wrapper for the private DEC modes.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PrivateMode {
    /// Known private mode.
    Named(NamedPrivateMode),
    /// Unknown private mode.
    Unknown(u16),
}

impl PrivateMode {
    fn new(mode: u16) -> Self {
        match mode {
            1 => Self::Named(NamedPrivateMode::CursorKeys),
            3 => Self::Named(NamedPrivateMode::ColumnMode),
            4 => Self::Named(NamedPrivateMode::SlowScroll), // b-terminal(#302)
            5 => Self::Named(NamedPrivateMode::ReverseColors), // b-terminal(#302)
            6 => Self::Named(NamedPrivateMode::Origin),
            7 => Self::Named(NamedPrivateMode::LineWrap),
            8 => Self::Named(NamedPrivateMode::Autorepeat), // b-terminal(#302)
            9 => Self::Named(NamedPrivateMode::X10Mouse), // b-terminal(#300)
            12 => Self::Named(NamedPrivateMode::BlinkingCursor),
            // b-terminal(#296): 옛 대체 화면 47·1047, 커서 저장 1048, DECNKM 66
            47 => Self::Named(NamedPrivateMode::SwapScreenLegacy),
            66 => Self::Named(NamedPrivateMode::KeypadApplication),
            25 => Self::Named(NamedPrivateMode::ShowCursor),
            45 => Self::Named(NamedPrivateMode::ReverseWrap), // b-terminal(#302)
            1000 => Self::Named(NamedPrivateMode::ReportMouseClicks),
            1002 => Self::Named(NamedPrivateMode::ReportCellMouseMotion),
            1003 => Self::Named(NamedPrivateMode::ReportAllMouseMotion),
            1004 => Self::Named(NamedPrivateMode::ReportFocusInOut),
            1005 => Self::Named(NamedPrivateMode::Utf8Mouse),
            1006 => Self::Named(NamedPrivateMode::SgrMouse),
            1007 => Self::Named(NamedPrivateMode::AlternateScroll),
            1015 => Self::Named(NamedPrivateMode::UrxvtMouse), // b-terminal(#300)
            1016 => Self::Named(NamedPrivateMode::SgrPixelsMouse), // b-terminal(#300)
            1042 => Self::Named(NamedPrivateMode::UrgencyHints),
            1047 => Self::Named(NamedPrivateMode::SwapScreenAndClear),
            1048 => Self::Named(NamedPrivateMode::SaveRestoreCursor),
            1045 => Self::Named(NamedPrivateMode::ReverseWrapExtended), // b-terminal(#302)
            1049 => Self::Named(NamedPrivateMode::SwapScreenAndSetRestoreCursor),
            2004 => Self::Named(NamedPrivateMode::BracketedPaste),
            2026 => Self::Named(NamedPrivateMode::SyncUpdate),
            _ => Self::Unknown(mode),
        }
    }

    /// Get the raw value of the mode.
    pub fn raw(self) -> u16 {
        match self {
            Self::Named(named) => named as u16,
            Self::Unknown(mode) => mode,
        }
    }
}

impl From<NamedPrivateMode> for PrivateMode {
    fn from(value: NamedPrivateMode) -> Self {
        Self::Named(value)
    }
}

/// Private DEC modes.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum NamedPrivateMode {
    CursorKeys = 1,
    /// Select 80 or 132 columns per page (DECCOLM).
    ///
    /// CSI ? 3 h -> set 132 column font.
    /// CSI ? 3 l -> reset 80 column font.
    ///
    /// Additionally,
    ///
    /// * set margins to default positions
    /// * erases all data in page memory
    /// * resets DECLRMM to unavailable
    /// * clears data from the status line (if set to host-writable)
    ColumnMode = 3,
    /// DECSCLM. b-terminal(#302): 저장만 한다(Ghostty도 부드러운 스크롤을 하지 않는다)
    SlowScroll = 4,
    /// DECSCNM 화면 반전. b-terminal(#302)
    ReverseColors = 5,
    Origin = 6,
    LineWrap = 7,
    /// DECARM. b-terminal(#302): 저장만 한다(xterm·Ghostty는 자동 반복을 끄지 않는다)
    Autorepeat = 8,
    /// X10 mouse reporting: presses only, no modifiers. b-terminal(#300)
    X10Mouse = 9,
    BlinkingCursor = 12,
    /// 역방향 줄바꿈(xterm). b-terminal(#302)
    ReverseWrap = 45,
    /// 확장 역방향 줄바꿈(XTREVWRAP2). b-terminal(#302)
    ReverseWrapExtended = 1045,
    ShowCursor = 25,
    ReportMouseClicks = 1000,
    ReportCellMouseMotion = 1002,
    ReportAllMouseMotion = 1003,
    ReportFocusInOut = 1004,
    Utf8Mouse = 1005,
    SgrMouse = 1006,
    AlternateScroll = 1007,
    /// urxvt mouse format. b-terminal(#300)
    UrxvtMouse = 1015,
    /// SGR mouse format with pixel coordinates. b-terminal(#300)
    SgrPixelsMouse = 1016,
    UrgencyHints = 1042,
    /// b-terminal(#296): 옛 대체 화면(47). 화면만 바꾼다.
    SwapScreenLegacy = 47,
    /// b-terminal(#296): DECNKM(66). `ESC =`·`ESC >`와 같은 키패드 모드.
    KeypadApplication = 66,
    /// b-terminal(#296): 1047. 나갈 때 대체 화면을 지운다.
    SwapScreenAndClear = 1047,
    /// b-terminal(#296): 1048. 커서만 저장·복원한다.
    SaveRestoreCursor = 1048,
    SwapScreenAndSetRestoreCursor = 1049,
    BracketedPaste = 2004,
    /// The mode is handled automatically by [`Processor`].
    SyncUpdate = 2026,
}

/// Mode for clearing line.
///
/// Relative to cursor.
#[derive(Debug)]
pub enum LineClearMode {
    /// Clear right of cursor.
    Right,
    /// Clear left of cursor.
    Left,
    /// Clear entire line.
    All,
}

/// b-terminal(#294): Character protection mode (Ghostty `ansi.ProtectedMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtectedMode {
    /// New cells are not protected (DECSCA 0 or 2, EPA).
    Off,
    /// ISO protection (SPA): erase operations keep protected cells.
    Iso,
    /// DEC protection (DECSCA 1): only selective erases keep protected cells.
    Dec,
}

/// Mode for clearing terminal.
///
/// Relative to cursor.
#[derive(Debug)]
pub enum ClearMode {
    /// Clear below cursor.
    Below,
    /// Clear above cursor.
    Above,
    /// Clear entire terminal.
    All,
    /// Clear 'saved' lines (scrollback).
    Saved,
}

/// Mode for clearing tab stops.
#[derive(Debug)]
pub enum TabulationClearMode {
    /// Clear stop under cursor.
    Current,
    /// Clear all stops.
    All,
}

/// Standard colors.
///
/// The order here matters since the enum should be castable to a `usize` for
/// indexing a color list.
#[derive(Debug, Copy, Clone, Eq, PartialEq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum NamedColor {
    /// Black.
    Black = 0,
    /// Red.
    Red,
    /// Green.
    Green,
    /// Yellow.
    Yellow,
    /// Blue.
    Blue,
    /// Magenta.
    Magenta,
    /// Cyan.
    Cyan,
    /// White.
    White,
    /// Bright black.
    BrightBlack,
    /// Bright red.
    BrightRed,
    /// Bright green.
    BrightGreen,
    /// Bright yellow.
    BrightYellow,
    /// Bright blue.
    BrightBlue,
    /// Bright magenta.
    BrightMagenta,
    /// Bright cyan.
    BrightCyan,
    /// Bright white.
    BrightWhite,
    /// The foreground color.
    Foreground = 256,
    /// The background color.
    Background,
    /// Color for the cursor itself.
    Cursor,
    /// Dim black.
    DimBlack,
    /// Dim red.
    DimRed,
    /// Dim green.
    DimGreen,
    /// Dim yellow.
    DimYellow,
    /// Dim blue.
    DimBlue,
    /// Dim magenta.
    DimMagenta,
    /// Dim cyan.
    DimCyan,
    /// Dim white.
    DimWhite,
    /// The bright foreground color.
    BrightForeground,
    /// Dim foreground.
    DimForeground,
}

impl NamedColor {
    #[must_use]
    pub fn to_bright(self) -> Self {
        match self {
            NamedColor::Foreground => NamedColor::BrightForeground,
            NamedColor::Black => NamedColor::BrightBlack,
            NamedColor::Red => NamedColor::BrightRed,
            NamedColor::Green => NamedColor::BrightGreen,
            NamedColor::Yellow => NamedColor::BrightYellow,
            NamedColor::Blue => NamedColor::BrightBlue,
            NamedColor::Magenta => NamedColor::BrightMagenta,
            NamedColor::Cyan => NamedColor::BrightCyan,
            NamedColor::White => NamedColor::BrightWhite,
            NamedColor::DimForeground => NamedColor::Foreground,
            NamedColor::DimBlack => NamedColor::Black,
            NamedColor::DimRed => NamedColor::Red,
            NamedColor::DimGreen => NamedColor::Green,
            NamedColor::DimYellow => NamedColor::Yellow,
            NamedColor::DimBlue => NamedColor::Blue,
            NamedColor::DimMagenta => NamedColor::Magenta,
            NamedColor::DimCyan => NamedColor::Cyan,
            NamedColor::DimWhite => NamedColor::White,
            val => val,
        }
    }

    #[must_use]
    pub fn to_dim(self) -> Self {
        match self {
            NamedColor::Black => NamedColor::DimBlack,
            NamedColor::Red => NamedColor::DimRed,
            NamedColor::Green => NamedColor::DimGreen,
            NamedColor::Yellow => NamedColor::DimYellow,
            NamedColor::Blue => NamedColor::DimBlue,
            NamedColor::Magenta => NamedColor::DimMagenta,
            NamedColor::Cyan => NamedColor::DimCyan,
            NamedColor::White => NamedColor::DimWhite,
            NamedColor::Foreground => NamedColor::DimForeground,
            NamedColor::BrightBlack => NamedColor::Black,
            NamedColor::BrightRed => NamedColor::Red,
            NamedColor::BrightGreen => NamedColor::Green,
            NamedColor::BrightYellow => NamedColor::Yellow,
            NamedColor::BrightBlue => NamedColor::Blue,
            NamedColor::BrightMagenta => NamedColor::Magenta,
            NamedColor::BrightCyan => NamedColor::Cyan,
            NamedColor::BrightWhite => NamedColor::White,
            NamedColor::BrightForeground => NamedColor::Foreground,
            val => val,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Color {
    Named(NamedColor),
    Spec(Rgb),
    Indexed(u8),
}

/// Terminal character attributes.
#[derive(Debug, Eq, PartialEq)]
pub enum Attr {
    /// Clear all special abilities.
    Reset,
    /// Bold text.
    Bold,
    /// Dim or secondary color.
    Dim,
    /// Italic text.
    Italic,
    /// Underline text.
    Underline,
    /// Underlined twice.
    DoubleUnderline,
    /// Undercurled text.
    Undercurl,
    /// Dotted underlined text.
    DottedUnderline,
    /// Dashed underlined text.
    DashedUnderline,
    /// Blink cursor slowly.
    BlinkSlow,
    /// Blink cursor fast.
    BlinkFast,
    /// Invert colors.
    Reverse,
    /// Do not display characters.
    Hidden,
    /// Strikeout text.
    Strike,
    /// Cancel bold.
    CancelBold,
    /// Cancel bold and dim.
    CancelBoldDim,
    /// Cancel italic.
    CancelItalic,
    /// Cancel all underlines.
    CancelUnderline,
    /// Cancel blink.
    CancelBlink,
    /// Cancel inversion.
    CancelReverse,
    /// Cancel text hiding.
    CancelHidden,
    /// Cancel strikeout.
    CancelStrike,
    /// Set indexed foreground color.
    Foreground(Color),
    /// Set indexed background color.
    Background(Color),
    /// Underline color.
    UnderlineColor(Option<Color>),
    /// Overlined text. b-terminal(#277)
    Overline,
    /// Cancel overline. b-terminal(#277)
    CancelOverline,
}

/// Identifiers which can be assigned to a graphic character set.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CharsetIndex {
    /// Default set, is designated as ASCII at startup.
    #[default]
    G0,
    G1,
    G2,
    G3,
}

/// Standard or common character sets which can be designated as G0-G3.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum StandardCharset {
    #[default]
    Ascii,
    SpecialCharacterAndLineDrawing,
    // b-terminal(#295): ESC ( A 영국 문자 집합. '#'만 '£'로 바뀐다
    British,
}

impl StandardCharset {
    /// Switch/Map character to the active charset. Ascii is the common case and
    /// for that we want to do as little as possible.
    #[inline]
    pub fn map(self, c: char) -> char {
        match self {
            StandardCharset::Ascii => c,
            StandardCharset::SpecialCharacterAndLineDrawing => match c {
                '_' => ' ',
                '`' => '◆',
                'a' => '▒',
                'b' => '\u{2409}', // Symbol for horizontal tabulation
                'c' => '\u{240c}', // Symbol for form feed
                'd' => '\u{240d}', // Symbol for carriage return
                'e' => '\u{240a}', // Symbol for line feed
                'f' => '°',
                'g' => '±',
                'h' => '\u{2424}', // Symbol for newline
                'i' => '\u{240b}', // Symbol for vertical tabulation
                'j' => '┘',
                'k' => '┐',
                'l' => '┌',
                'm' => '└',
                'n' => '┼',
                'o' => '⎺',
                'p' => '⎻',
                'q' => '─',
                'r' => '⎼',
                's' => '⎽',
                't' => '├',
                'u' => '┤',
                'v' => '┴',
                'w' => '┬',
                'x' => '│',
                'y' => '≤',
                'z' => '≥',
                '{' => 'π',
                '|' => '≠',
                '}' => '£',
                '~' => '·',
                _ => c,
            },
            // b-terminal(#295)
            StandardCharset::British => match c {
                '#' => '£',
                _ => c,
            },
        }
    }
}

/// SCP control's first parameter which determines character path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScpCharPath {
    /// SCP's first parameter value of 0. Behavior is implementation defined.
    Default,
    /// SCP's first parameter value of 1 which sets character path to
    /// LEFT-TO-RIGHT.
    LTR,
    /// SCP's first parameter value of 2 which sets character path to
    /// RIGHT-TO-LEFT.
    RTL,
}

/// SCP control's second parameter which determines update mode/direction
/// between components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScpUpdateMode {
    /// SCP's second parameter value of 0 (the default). Implementation
    /// dependant update.
    ImplementationDependant,
    /// SCP's second parameter value of 1.
    ///
    /// Reflect data component changes in the presentation component.
    DataToPresentation,
    /// SCP's second parameter value of 2.
    ///
    /// Reflect presentation component changes in the data component.
    PresentationToData,
}

impl<'a, H, T> crate::Perform for Performer<'a, H, T>
where
    H: Handler + 'a,
    T: Timeout,
{
    #[inline]
    fn print(&mut self, c: char) {
        self.handler.input(c);
        self.state.preceding_char = Some(c);
    }

    #[inline]
    fn execute(&mut self, byte: u8) {
        match byte {
            C0::HT => self.handler.put_tab(1),
            C0::BS => self.handler.backspace(),
            C0::CR => self.handler.carriage_return(),
            // b-terminal(#526): LF·VT·FF는 `newline`(LNM이 켜졌으면 CR도 함). IND(`ESC D`)는 `linefeed` 그대로(Ghostty `linefeed`·`index`)
            C0::LF | C0::VT | C0::FF => self.handler.newline(),
            C0::BEL => self.handler.bell(),
            C0::SUB => self.handler.substitute(),
            C0::SI => self.handler.set_active_charset(CharsetIndex::G0),
            C0::SO => self.handler.set_active_charset(CharsetIndex::G1),
            _ => debug!("[unhandled] execute byte={:02x}", byte),
        }
    }

    #[inline]
    fn hook(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char) {
        debug!(
            "[unhandled hook] params={:?}, ints: {:?}, ignore: {:?}, action: {:?}",
            params, intermediates, ignore, action
        );
    }

    #[inline]
    fn put(&mut self, byte: u8) {
        debug!("[unhandled put] byte={:?}", byte);
    }

    #[inline]
    fn unhook(&mut self) {
        debug!("[unhandled unhook]");
    }

    #[inline]
    fn osc_dispatch(&mut self, params: &[&[u8]], bell_terminated: bool) {
        let terminator = if bell_terminated { "\x07" } else { "\x1b\\" };

        fn unhandled(params: &[&[u8]]) {
            let mut buf = String::new();
            for items in params {
                buf.push('[');
                for item in *items {
                    let _ = write!(buf, "{:?}", *item as char);
                }
                buf.push_str("],");
            }
            debug!("[unhandled osc_dispatch]: [{}] at line {}", &buf, line!());
        }

        if params.is_empty() || params[0].is_empty() {
            return;
        }

        match params[0] {
            // Set window title.
            b"0" | b"2" => {
                // Ghostty: 깨진 UTF-8이 섞이면 요청 전체를 무시하고(`stream.zig` 2855줄), 앞뒤 공백은 그대로 둔다.
                // 2047바이트를 넘으면 명령이 없다(고정 캡처). 16번째 칸 뒤도 `;` 포함 전부 제목이다. b-terminal(#765)
                if params.len() >= 2 && osc_payload_len(params) < OSC_MAX_BUF {
                    let mut title = String::new();
                    for (i, piece) in params[1..].iter().enumerate() {
                        let Ok(piece) = str::from_utf8(piece) else {
                            return unhandled(params);
                        };
                        if i > 0 {
                            title.push(';');
                        }
                        title.push_str(piece);
                    }
                    self.handler.set_title(Some(title));
                    return;
                }
                unhandled(params);
            },

            // Set color index.
            b"4" => {
                // Ghostty `parseGetSetAnsiColor`: 쌍이 안 맞거나 번호·색이 틀린 곳에서 멈추고 그 앞까지 쓴다.
                // 빈 조각은 건너뛴다. b-terminal(#765)
                let mut tokens = color_tokens(params);
                while let (Some(index), Some(spec)) = (tokens.next(), tokens.next()) {
                    let Some(index) = parse_color_index(index) else {
                        unhandled(params);
                        break;
                    };
                    // 256~259는 Ghostty의 특수 색(굵게·밑줄 등). 우리에겐 그 자리가 없어 받아만 두고 건너뛴다.
                    let palette = u8::try_from(index).ok();
                    if index > 259 {
                        unhandled(params);
                        break;
                    }

                    if spec == b"?" {
                        if let Some(index) = palette {
                            let prefix = alloc::format!("4;{index}");
                            self.handler.dynamic_color_sequence(prefix, index as usize, terminator);
                        }
                    } else if let Some(c) = xparse_color(spec) {
                        if let Some(index) = palette {
                            self.handler.set_color(index as usize, c);
                        }
                    } else {
                        unhandled(params);
                        break;
                    }
                }
            },

            // Hyperlink.
            b"8" if params.len() > 2 => {
                if osc_payload_len(params) >= OSC_MAX_BUF {
                    return unhandled(params);
                }

                // NOTE: The escape sequence is of form 'OSC 8 ; params ; URI ST', where
                // URI is URL-encoded. However `;` is a special character and might be
                // passed as is, thus we need to rebuild the URI.
                let mut uri = params[2].to_vec();
                for param in params[3..].iter() {
                    uri.push(b';');
                    uri.extend_from_slice(param);
                }
                let Ok(uri) = String::from_utf8(uri) else {
                    return unhandled(params);
                };

                // Ghostty `hyperlink.zig`(b-terminal #765): 값이 빈 `id=`는 id 없음이다. 주소가 비면 링크를 끝내지만
                // id가 있으면 무효 요청이라 열린 링크가 이어진다(`8;id=foo;`).
                let id = osc8_id(params[1]);
                if uri.is_empty() {
                    if id.is_some() {
                        return unhandled(params);
                    }
                    self.handler.set_hyperlink(None);
                    return;
                }

                self.handler.set_hyperlink(Some(Hyperlink { id, uri }));
            },

            // Get/set Foreground, Background, Cursor colors.
            b"10" | b"11" | b"12" => {
                // Ghostty `parseGetSetDynamicColor`: 색이 틀린 곳에서 멈추고 앞까지 쓴다. 빈 조각은 건너뛴다.
                // b-terminal(#765)
                let mut dynamic_code: usize = match params[0] {
                    b"10" => 10,
                    b"11" => 11,
                    _ => 12,
                };
                for param in color_tokens(params) {
                    // 10 is the first dynamic color, also the foreground.
                    let offset = dynamic_code - 10;
                    let index = NamedColor::Foreground as usize + offset;

                    // End of setting dynamic colors.
                    if index > NamedColor::Cursor as usize {
                        unhandled(params);
                        break;
                    }

                    if param == b"?" {
                        self.handler.dynamic_color_sequence(
                            dynamic_code.to_string(),
                            index,
                            terminator,
                        );
                    } else if let Some(color) = xparse_color(param) {
                        self.handler.set_color(index, color);
                    } else {
                        unhandled(params);
                        break;
                    }
                    dynamic_code += 1;
                }
            },

            // Set mouse cursor shape.
            b"22" if params.len() == 2 => {
                let shape = String::from_utf8_lossy(params[1]);
                match CursorIcon::from_str(&shape) {
                    Ok(cursor_icon) => self.handler.set_mouse_cursor_icon(cursor_icon),
                    Err(_) => debug!("[osc 22] unrecognized cursor icon shape: {shape:?}"),
                }
            },

            // Set cursor style.
            b"50" => {
                if params.len() >= 2
                    && params[1].len() >= 13
                    && params[1][0..12] == *b"CursorShape="
                {
                    let shape = match params[1][12] as char {
                        '0' => CursorShape::Block,
                        '1' => CursorShape::Beam,
                        '2' => CursorShape::Underline,
                        _ => return unhandled(params),
                    };
                    self.handler.set_cursor_shape(shape);
                    return;
                }
                unhandled(params);
            },

            // Set clipboard.
            b"52" => {
                if params.len() < 3 {
                    return unhandled(params);
                }

                let clipboard = params[1].first().unwrap_or(&b'c');
                match params[2] {
                    b"?" => self.handler.clipboard_load(*clipboard, terminator),
                    base64 => self.handler.clipboard_store(*clipboard, base64),
                }
            },

            // Reset color index.
            b"104" => {
                // Ghostty `parseResetAnsiColor`(b-terminal #765): 빈 조각과 틀린 번호는 건너뛰고, 쓸 만한 번호가
                // 하나도 없으면(인자가 없을 때 포함) 팔레트 전체를 되돌린다. 256~259(특수 색)는 우리에겐 자리가
                // 없지만 번호로는 센다.
                let mut valid = 0usize;
                for param in color_tokens(params) {
                    match parse_color_index(param) {
                        Some(index) if index < 256 => {
                            self.handler.reset_color(index as usize);
                            valid += 1;
                        },
                        Some(256..=259) => valid += 1,
                        _ => unhandled(params),
                    }
                }
                if valid == 0 {
                    for i in 0..256 {
                        self.handler.reset_color(i);
                    }
                }
            },

            // Reset foreground/background/text cursor color. Ghostty는 인자가 있으면(`110;junk`) 되돌리지 않는다.
            // 끝에 `;`만 붙은 것(`110;`)은 인자가 아니다. b-terminal(#765)
            b"110" | b"111" | b"112" => {
                if color_tokens(params).next().is_some() {
                    return unhandled(params);
                }
                let color = match params[0] {
                    b"110" => NamedColor::Foreground,
                    b"111" => NamedColor::Background,
                    _ => NamedColor::Cursor,
                };
                self.handler.reset_color(color as usize);
            },

            _ => unhandled(params),
        }
    }

    #[allow(clippy::cognitive_complexity)]
    #[inline]
    fn csi_dispatch(
        &mut self,
        params: &Params,
        intermediates: &[u8],
        has_ignored_intermediates: bool,
        action: char,
    ) {
        macro_rules! unhandled {
            () => {{
                debug!(
                    "[Unhandled CSI] action={:?}, params={:?}, intermediates={:?}",
                    action, params, intermediates
                );
            }};
        }

        if has_ignored_intermediates || intermediates.len() > 2 {
            unhandled!();
            return;
        }

        let mut params_iter = params.iter();
        let handler = &mut self.handler;

        let mut next_param_or = |default: u16| match params_iter.next() {
            Some(&[param, ..]) if param != 0 => param,
            _ => default,
        };

        match (action, intermediates) {
            ('@', []) => handler.insert_blank(next_param_or(1) as usize),
            ('A', []) => handler.move_up(next_param_or(1) as usize),
            ('B', []) | ('e', []) => handler.move_down(next_param_or(1) as usize),
            ('b', []) => {
                if let Some(c) = self.state.preceding_char {
                    for _ in 0..next_param_or(1) {
                        handler.input(c);
                    }
                } else {
                    debug!("tried to repeat with no preceding char");
                }
            },
            ('C', []) | ('a', []) => handler.move_forward(next_param_or(1) as usize),
            ('c', intermediates) if next_param_or(0) == 0 => {
                handler.identify_terminal(intermediates.first().map(|&i| i as char))
            },
            ('D', []) => handler.move_backward(next_param_or(1) as usize),
            ('d', []) => handler.goto_line(next_param_or(1) as i32 - 1),
            ('E', []) => handler.move_down_and_cr(next_param_or(1) as usize),
            ('F', []) => handler.move_up_and_cr(next_param_or(1) as usize),
            ('G', []) | ('`', []) => handler.goto_col(next_param_or(1) as usize - 1),
            ('W', [b'?']) if next_param_or(0) == 5 => handler.set_tabs(8),
            // b-terminal(#295): CTC. 0은 칸에 탭 정지, 2는 그 칸 정지 지움, 5는 모두 지움(Ghostty stream.zig 'W')
            ('W', []) if params.len() <= 1 => match next_param_or(0) {
                0 => handler.set_horizontal_tabstop(),
                2 => handler.clear_tabs(TabulationClearMode::Current),
                5 => handler.clear_tabs(TabulationClearMode::All),
                _ => unhandled!(),
            },
            ('g', []) => {
                let mode = match next_param_or(0) {
                    0 => TabulationClearMode::Current,
                    3 => TabulationClearMode::All,
                    _ => {
                        unhandled!();
                        return;
                    },
                };

                handler.clear_tabs(mode);
            },
            ('H', []) | ('f', []) => {
                let y = next_param_or(1) as i32;
                let x = next_param_or(1) as usize;
                handler.goto(y - 1, x - 1);
            },
            ('h', []) => {
                for param in params_iter.map(|param| param[0]) {
                    handler.set_mode(Mode::new(param))
                }
            },
            ('h', [b'?']) => {
                for param in params_iter.map(|param| param[0]) {
                    // Handle sync updates opaquely.
                    if param == NamedPrivateMode::SyncUpdate as u16 {
                        self.state.sync_state.timeout.set_timeout(SYNC_UPDATE_TIMEOUT);
                        self.terminated = true;
                    }

                    handler.set_private_mode(PrivateMode::new(param))
                }
            },
            ('I', []) => handler.move_forward_tabs(next_param_or(1)),
            // b-terminal(#327): ED 22(Ghostty stream.zig 1924줄 `scroll_complete`)
            ('J', []) if params.len() == 1 && params.iter().next() == Some(&[22][..]) => {
                handler.clear_screen_scroll_complete();
            },
            ('J', []) => {
                let mode = match next_param_or(0) {
                    0 => ClearMode::Below,
                    1 => ClearMode::Above,
                    2 => ClearMode::All,
                    3 => ClearMode::Saved,
                    _ => {
                        unhandled!();
                        return;
                    },
                };

                handler.clear_screen(mode);
            },
            ('K', []) => {
                let mode = match next_param_or(0) {
                    0 => LineClearMode::Right,
                    1 => LineClearMode::Left,
                    2 => LineClearMode::All,
                    _ => {
                        unhandled!();
                        return;
                    },
                };

                handler.clear_line(mode);
            },
            // b-terminal(#295): HPB는 CUB, VPB는 CUU와 같다(Ghostty stream.zig 'j'·'k')
            ('j', []) => handler.move_backward(next_param_or(1) as usize),
            ('k', []) => handler.move_up(next_param_or(1) as usize),
            // b-terminal(#339): DECSED 22. Ghostty stream.zig 1896~1925줄은 `?`가 붙어도 `scroll_complete`로 보내고,
            // `scrollClear`는 보호 칸을 가리지 않으므로 ED 22와 같다
            ('J', [b'?']) if params.len() == 1 && params.iter().next() == Some(&[22][..]) => {
                handler.clear_screen_scroll_complete();
            },
            // b-terminal(#294): DECSED(`CSI ? Ps J`). Ghostty stream.zig: 0..=3 like ED.
            ('J', [b'?']) => {
                let mode = match next_param_or(0) {
                    0 => ClearMode::Below,
                    1 => ClearMode::Above,
                    2 => ClearMode::All,
                    3 => ClearMode::Saved,
                    _ => {
                        unhandled!();
                        return;
                    },
                };

                handler.selective_clear_screen(mode);
            },
            // b-terminal(#294): DECSEL(`CSI ? Ps K`).
            ('K', [b'?']) => {
                let mode = match next_param_or(0) {
                    0 => LineClearMode::Right,
                    1 => LineClearMode::Left,
                    2 => LineClearMode::All,
                    _ => {
                        unhandled!();
                        return;
                    },
                };

                handler.selective_clear_line(mode);
            },
            ('k', [b' ']) => {
                // SCP control.
                let char_path = match next_param_or(0) {
                    0 => ScpCharPath::Default,
                    1 => ScpCharPath::LTR,
                    2 => ScpCharPath::RTL,
                    _ => {
                        unhandled!();
                        return;
                    },
                };

                let update_mode = match next_param_or(0) {
                    0 => ScpUpdateMode::ImplementationDependant,
                    1 => ScpUpdateMode::DataToPresentation,
                    2 => ScpUpdateMode::PresentationToData,
                    _ => {
                        unhandled!();
                        return;
                    },
                };

                handler.set_scp(char_path, update_mode);
            },
            ('L', []) => handler.insert_blank_lines(next_param_or(1) as usize),
            // b-terminal(#295): DECIC·DECDC. 매개변수가 둘 이상이면 Ghostty처럼 버린다
            ('}', [b'\'']) if params.len() <= 1 => handler.insert_columns(next_param_or(1) as usize),
            ('~', [b'\'']) if params.len() <= 1 => handler.delete_columns(next_param_or(1) as usize),
            ('l', []) => {
                for param in params_iter.map(|param| param[0]) {
                    handler.unset_mode(Mode::new(param))
                }
            },
            ('l', [b'?']) => {
                for param in params_iter.map(|param| param[0]) {
                    handler.unset_private_mode(PrivateMode::new(param))
                }
            },
            ('M', []) => handler.delete_lines(next_param_or(1) as usize),
            ('m', []) => {
                if params.is_empty() {
                    handler.terminal_attribute(Attr::Reset);
                } else {
                    attrs_from_sgr_parameters(*handler, &mut params_iter);
                }
            },
            // b-terminal(#312): Ghostty stream.zig 2323~2360줄. 매개변수가 없거나 0·1·2면 끄고, 4는 두 번째 값으로 정한다.
            // 매개변수가 셋 이상이거나 그 밖의 첫 값이면 버린다
            ('m', [b'>']) => {
                let first = params.iter().next().map(|p| p[0]);
                let second = params.iter().nth(1).map(|p| p[0]);
                let mode = match (params.len() <= 2, first) {
                    (true, None | Some(0..=2)) => ModifyOtherKeys::Reset,
                    (true, Some(4)) => match second {
                        Some(1) => ModifyOtherKeys::EnableExceptWellDefined,
                        Some(2) => ModifyOtherKeys::EnableAll,
                        _ => ModifyOtherKeys::Reset,
                    },
                    _ => return unhandled!(),
                };
                handler.set_modify_other_keys(mode);
            },
            ('m', [b'?']) => {
                if params_iter.next() == Some(&[4]) {
                    handler.report_modify_other_keys();
                } else {
                    unhandled!()
                }
            },
            ('n', []) => handler.device_status(next_param_or(0) as usize),
            // b-terminal(#327): DECXCPR. 매개변수가 정확히 하나이고 6일 때만(Ghostty stream.zig 2391·2405줄, device_status.zig 110줄)
            ('n', [b'?']) if params.len() == 1 && next_param_or(0) == 6 => handler.report_cursor_position_extended(),
            // b-terminal(#312): `CSI > Ps n`은 modifyOtherKeys를 되돌린다(Ghostty stream.zig 2362~2368줄: 켜짐 없는 기본으로)
            ('n', [b'>']) => handler.set_modify_other_keys(ModifyOtherKeys::Reset),
            ('P', []) => handler.delete_chars(next_param_or(1) as usize),
            ('p', [b'$']) => {
                let mode = next_param_or(0);
                handler.report_mode(Mode::new(mode));
            },
            ('p', [b'?', b'$']) => {
                let mode = next_param_or(0);
                handler.report_private_mode(PrivateMode::new(mode));
            },
            // b-terminal(#294): DECSCA(`CSI Ps " q`). Ghostty stream.zig: 0 and 2 off, 1 DEC protected.
            ('q', [b'"']) => {
                let mode = match next_param_or(0) {
                    0 | 2 => ProtectedMode::Off,
                    1 => ProtectedMode::Dec,
                    _ => {
                        unhandled!();
                        return;
                    },
                };

                handler.set_protected_mode(mode);
            },
            ('q', [b' ']) => {
                // DECSCUSR (CSI Ps SP q) -- Set Cursor Style.
                let cursor_style_id = next_param_or(0);
                let shape = match cursor_style_id {
                    0 => None,
                    1 | 2 => Some(CursorShape::Block),
                    3 | 4 => Some(CursorShape::Underline),
                    5 | 6 => Some(CursorShape::Beam),
                    _ => {
                        unhandled!();
                        return;
                    },
                };
                let cursor_style =
                    shape.map(|shape| CursorStyle { shape, blinking: cursor_style_id % 2 == 1 });

                handler.set_cursor_style(cursor_style);
            },
            ('r', []) => {
                let top = next_param_or(1) as usize;
                let bottom =
                    params_iter.next().map(|param| param[0] as usize).filter(|&param| param != 0);

                handler.set_scrolling_region(top, bottom);
            },
            // b-terminal(#313): XTRESTORE. Ghostty stream.zig는 `?`가 붙은 `r`·`s`의 매개변수마다 모드 하나씩 처리한다
            ('r', [b'?']) => {
                for param in params_iter.map(|param| param[0]) {
                    handler.restore_private_mode(PrivateMode::new(param))
                }
            },
            ('S', []) => handler.scroll_up(next_param_or(1) as usize),
            // b-terminal(#311): 매개변수가 없거나 0 하나면 DECSLRM(69 켬)이나 SCOSC(69 끔)을 Term이 가른다. vte는 `CSI s`와
            // `CSI 0 s`를 구분하지 못한다(둘 다 `[0]`). 값이 있으면 DECSLRM(69가 꺼져 있으면 Term이 버린다)
            ('s', []) => {
                let left = next_param_or(0) as usize;
                if params.len() <= 1 && left == 0 {
                    handler.save_cursor_or_reset_margins();
                } else if params.len() <= 2 {
                    let right = next_param_or(0) as usize;
                    handler.set_left_right_margins(left, right);
                } else {
                    unhandled!();
                }
            },
            // b-terminal(#327): XTSHIFTESCAPE. 매개변수가 없거나 0이면 거짓, 1이면 참, 그 밖은 버린다(Ghostty stream.zig 2632줄)
            ('s', [b'>']) => {
                if params.len() > 1 {
                    unhandled!();
                    return;
                }
                match next_param_or(0) {
                    0 => handler.set_mouse_shift_capture(false),
                    1 => handler.set_mouse_shift_capture(true),
                    _ => unhandled!(),
                }
            },
            // b-terminal(#313): XTSAVE
            ('s', [b'?']) => {
                for param in params_iter.map(|param| param[0]) {
                    handler.save_private_mode(PrivateMode::new(param))
                }
            },
            ('T', []) => handler.scroll_down(next_param_or(1) as usize),
            ('t', []) => match next_param_or(1) as usize {
                14 => handler.text_area_size_pixels(),
                18 => handler.text_area_size_chars(),
                22 => handler.push_title(),
                23 => handler.pop_title(),
                _ => unhandled!(),
            },
            ('u', [b'?']) => handler.report_keyboard_mode(),
            ('u', [b'=']) => {
                let mode = KeyboardModes::from_bits_truncate(next_param_or(0) as u8);
                let behavior = match next_param_or(1) {
                    3 => KeyboardModesApplyBehavior::Difference,
                    2 => KeyboardModesApplyBehavior::Union,
                    // Default is replace.
                    _ => KeyboardModesApplyBehavior::Replace,
                };
                handler.set_keyboard_mode(mode, behavior);
            },
            ('u', [b'>']) => {
                let mode = KeyboardModes::from_bits_truncate(next_param_or(0) as u8);
                handler.push_keyboard_mode(mode);
            },
            ('u', [b'<']) => {
                // The default is 1.
                handler.pop_keyboard_modes(next_param_or(1));
            },
            ('u', []) => handler.restore_cursor_position(),
            ('X', []) => handler.erase_chars(next_param_or(1) as usize),
            ('Z', []) => handler.move_backward_tabs(next_param_or(1)),
            _ => unhandled!(),
        }
    }

    #[inline]
    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        macro_rules! unhandled {
            () => {{
                debug!(
                    "[unhandled] esc_dispatch ints={:?}, byte={:?} ({:02x})",
                    intermediates, byte as char, byte
                );
            }};
        }

        macro_rules! configure_charset {
            ($charset:path, $intermediates:expr) => {{
                let index: CharsetIndex = match $intermediates {
                    [b'('] => CharsetIndex::G0,
                    [b')'] => CharsetIndex::G1,
                    [b'*'] => CharsetIndex::G2,
                    [b'+'] => CharsetIndex::G3,
                    _ => {
                        unhandled!();
                        return;
                    },
                };
                self.handler.configure_charset(index, $charset)
            }};
        }

        match (byte, intermediates) {
            (b'B', intermediates) => configure_charset!(StandardCharset::Ascii, intermediates),
            // b-terminal(#295): 영국 문자 집합, DECBI, DECFI, SS2·SS3, LS2·LS3, LS1R~LS3R
            (b'A', intermediates) => configure_charset!(StandardCharset::British, intermediates),
            (b'6', []) => self.handler.back_index(),
            (b'9', []) => self.handler.forward_index(),
            (b'N', []) => self.handler.set_single_shift(CharsetIndex::G2),
            (b'O', []) => self.handler.set_single_shift(CharsetIndex::G3),
            (b'n', []) => self.handler.set_active_charset(CharsetIndex::G2),
            (b'o', []) => self.handler.set_active_charset(CharsetIndex::G3),
            // GR로 부르는 집합은 Ghostty도 아직 글자에 쓰지 않는다(Terminal.zig 2279줄 TODO). 받기만 한다
            (b'~', []) | (b'}', []) | (b'|', []) => (),
            (b'D', []) => self.handler.linefeed(),
            (b'E', []) => {
                self.handler.linefeed();
                self.handler.carriage_return();
            },
            (b'H', []) => self.handler.set_horizontal_tabstop(),
            (b'M', []) => self.handler.reverse_index(),
            // b-terminal(#294): SPA (start of guarded area) and EPA (end of guarded area).
            (b'V', []) => self.handler.set_protected_mode(ProtectedMode::Iso),
            (b'W', []) => self.handler.set_protected_mode(ProtectedMode::Off),
            (b'Z', []) => self.handler.identify_terminal(None),
            (b'c', []) => self.handler.reset_state(),
            (b'0', intermediates) => {
                configure_charset!(StandardCharset::SpecialCharacterAndLineDrawing, intermediates)
            },
            (b'7', []) => self.handler.save_cursor_position(),
            (b'8', [b'#']) => self.handler.decaln(),
            (b'8', []) => self.handler.restore_cursor_position(),
            (b'=', []) => self.handler.set_keypad_application_mode(),
            (b'>', []) => self.handler.unset_keypad_application_mode(),
            // String terminator, do nothing (parser handles as string terminator).
            (b'\\', []) => (),
            _ => unhandled!(),
        }
    }

    #[inline]
    fn terminated(&self) -> bool {
        self.terminated
    }
}

#[inline]
fn attrs_from_sgr_parameters<H: Handler>(handler: &mut H, params: &mut ParamsIter<'_>) {
    while let Some(param) = params.next() {
        let attr = match param {
            [0] => Some(Attr::Reset),
            [1] => Some(Attr::Bold),
            [2] => Some(Attr::Dim),
            [3] => Some(Attr::Italic),
            [4, 0] => Some(Attr::CancelUnderline),
            [4, 2] => Some(Attr::DoubleUnderline),
            [4, 3] => Some(Attr::Undercurl),
            [4, 4] => Some(Attr::DottedUnderline),
            [4, 5] => Some(Attr::DashedUnderline),
            // b-terminal(#761): 하위 매개변수가 하나면(모르는 모양 포함) 홑밑줄, 둘 이상이면 알 수 없는 속성(Ghostty sgr.zig)
            [4] | [4, _] => Some(Attr::Underline),
            [4, ..] => None,
            [5] => Some(Attr::BlinkSlow),
            [6] => Some(Attr::BlinkFast),
            [7] => Some(Attr::Reverse),
            [8] => Some(Attr::Hidden),
            [9] => Some(Attr::Strike),
            [21] => Some(Attr::CancelBold),
            [22] => Some(Attr::CancelBoldDim),
            [23] => Some(Attr::CancelItalic),
            [24] => Some(Attr::CancelUnderline),
            [25] => Some(Attr::CancelBlink),
            [27] => Some(Attr::CancelReverse),
            [28] => Some(Attr::CancelHidden),
            [29] => Some(Attr::CancelStrike),
            [30] => Some(Attr::Foreground(Color::Named(NamedColor::Black))),
            [31] => Some(Attr::Foreground(Color::Named(NamedColor::Red))),
            [32] => Some(Attr::Foreground(Color::Named(NamedColor::Green))),
            [33] => Some(Attr::Foreground(Color::Named(NamedColor::Yellow))),
            [34] => Some(Attr::Foreground(Color::Named(NamedColor::Blue))),
            [35] => Some(Attr::Foreground(Color::Named(NamedColor::Magenta))),
            [36] => Some(Attr::Foreground(Color::Named(NamedColor::Cyan))),
            [37] => Some(Attr::Foreground(Color::Named(NamedColor::White))),
            [38] => {
                let mut iter = params.map(|param| param[0]);
                parse_sgr_color(&mut iter).map(Attr::Foreground)
            },
            [38, params @ ..] => handle_colon_rgb(params).map(Attr::Foreground),
            [39] => Some(Attr::Foreground(Color::Named(NamedColor::Foreground))),
            [40] => Some(Attr::Background(Color::Named(NamedColor::Black))),
            [41] => Some(Attr::Background(Color::Named(NamedColor::Red))),
            [42] => Some(Attr::Background(Color::Named(NamedColor::Green))),
            [43] => Some(Attr::Background(Color::Named(NamedColor::Yellow))),
            [44] => Some(Attr::Background(Color::Named(NamedColor::Blue))),
            [45] => Some(Attr::Background(Color::Named(NamedColor::Magenta))),
            [46] => Some(Attr::Background(Color::Named(NamedColor::Cyan))),
            [47] => Some(Attr::Background(Color::Named(NamedColor::White))),
            [48] => {
                let mut iter = params.map(|param| param[0]);
                parse_sgr_color(&mut iter).map(Attr::Background)
            },
            [48, params @ ..] => handle_colon_rgb(params).map(Attr::Background),
            [49] => Some(Attr::Background(Color::Named(NamedColor::Background))),
            [53] => Some(Attr::Overline),       // b-terminal(#277)
            [55] => Some(Attr::CancelOverline), // b-terminal(#277)
            [58] => {
                let mut iter = params.map(|param| param[0]);
                parse_sgr_color(&mut iter).map(|color| Attr::UnderlineColor(Some(color)))
            },
            [58, params @ ..] => {
                handle_colon_rgb(params).map(|color| Attr::UnderlineColor(Some(color)))
            },
            [59] => Some(Attr::UnderlineColor(None)),
            [90] => Some(Attr::Foreground(Color::Named(NamedColor::BrightBlack))),
            [91] => Some(Attr::Foreground(Color::Named(NamedColor::BrightRed))),
            [92] => Some(Attr::Foreground(Color::Named(NamedColor::BrightGreen))),
            [93] => Some(Attr::Foreground(Color::Named(NamedColor::BrightYellow))),
            [94] => Some(Attr::Foreground(Color::Named(NamedColor::BrightBlue))),
            [95] => Some(Attr::Foreground(Color::Named(NamedColor::BrightMagenta))),
            [96] => Some(Attr::Foreground(Color::Named(NamedColor::BrightCyan))),
            [97] => Some(Attr::Foreground(Color::Named(NamedColor::BrightWhite))),
            [100] => Some(Attr::Background(Color::Named(NamedColor::BrightBlack))),
            [101] => Some(Attr::Background(Color::Named(NamedColor::BrightRed))),
            [102] => Some(Attr::Background(Color::Named(NamedColor::BrightGreen))),
            [103] => Some(Attr::Background(Color::Named(NamedColor::BrightYellow))),
            [104] => Some(Attr::Background(Color::Named(NamedColor::BrightBlue))),
            [105] => Some(Attr::Background(Color::Named(NamedColor::BrightMagenta))),
            [106] => Some(Attr::Background(Color::Named(NamedColor::BrightCyan))),
            [107] => Some(Attr::Background(Color::Named(NamedColor::BrightWhite))),
            _ => None,
        };

        match attr {
            Some(attr) => handler.terminal_attribute(attr),
            None => continue,
        }
    }
}

/// Handle colon separated rgb color escape sequence.
#[inline]
fn handle_colon_rgb(params: &[u16]) -> Option<Color> {
    // b-terminal(#761): 직접 색은 `2:r:g:b`나 `2:색공간:r:g:b`만 받는다. 더 길면 알 수 없는 속성(Ghostty sgr.zig `parseDirectColor`)
    if params.first() == Some(&2) && params.len() > 5 {
        return None;
    }
    let rgb_start = if params.len() > 4 { 2 } else { 1 };
    let rgb_iter = params[rgb_start..].iter().copied();
    let mut iter = iter::once(params[0]).chain(rgb_iter);

    parse_sgr_color(&mut iter)
}

/// Parse a color specifier from list of attributes.
fn parse_sgr_color(params: &mut dyn Iterator<Item = u16>) -> Option<Color> {
    match params.next() {
        Some(2) => Some(Color::Spec(Rgb {
            r: u8::try_from(params.next()?).ok()?,
            g: u8::try_from(params.next()?).ok()?,
            b: u8::try_from(params.next()?).ok()?,
        })),
        Some(5) => Some(Color::Indexed(u8::try_from(params.next()?).ok()?)),
        _ => None,
    }
}

/// C0 set of 7-bit control characters (from ANSI X3.4-1977).
#[allow(non_snake_case)]
pub mod C0 {
    /// Null filler, terminal should ignore this character.
    pub const NUL: u8 = 0x00;
    /// Start of Header.
    pub const SOH: u8 = 0x01;
    /// Start of Text, implied end of header.
    pub const STX: u8 = 0x02;
    /// End of Text, causes some terminal to respond with ACK or NAK.
    pub const ETX: u8 = 0x03;
    /// End of Transmission.
    pub const EOT: u8 = 0x04;
    /// Enquiry, causes terminal to send ANSWER-BACK ID.
    pub const ENQ: u8 = 0x05;
    /// Acknowledge, usually sent by terminal in response to ETX.
    pub const ACK: u8 = 0x06;
    /// Bell, triggers the bell, buzzer, or beeper on the terminal.
    pub const BEL: u8 = 0x07;
    /// Backspace, can be used to define overstruck characters.
    pub const BS: u8 = 0x08;
    /// Horizontal Tabulation, move to next predetermined position.
    pub const HT: u8 = 0x09;
    /// Linefeed, move to same position on next line (see also NL).
    pub const LF: u8 = 0x0A;
    /// Vertical Tabulation, move to next predetermined line.
    pub const VT: u8 = 0x0B;
    /// Form Feed, move to next form or page.
    pub const FF: u8 = 0x0C;
    /// Carriage Return, move to first character of current line.
    pub const CR: u8 = 0x0D;
    /// Shift Out, switch to G1 (other half of character set).
    pub const SO: u8 = 0x0E;
    /// Shift In, switch to G0 (normal half of character set).
    pub const SI: u8 = 0x0F;
    /// Data Link Escape, interpret next control character specially.
    pub const DLE: u8 = 0x10;
    /// (DC1) Terminal is allowed to resume transmitting.
    pub const XON: u8 = 0x11;
    /// Device Control 2, causes ASR-33 to activate paper-tape reader.
    pub const DC2: u8 = 0x12;
    /// (DC2) Terminal must pause and refrain from transmitting.
    pub const XOFF: u8 = 0x13;
    /// Device Control 4, causes ASR-33 to deactivate paper-tape reader.
    pub const DC4: u8 = 0x14;
    /// Negative Acknowledge, used sometimes with ETX and ACK.
    pub const NAK: u8 = 0x15;
    /// Synchronous Idle, used to maintain timing in Sync communication.
    pub const SYN: u8 = 0x16;
    /// End of Transmission block.
    pub const ETB: u8 = 0x17;
    /// Cancel (makes VT100 abort current escape sequence if any).
    pub const CAN: u8 = 0x18;
    /// End of Medium.
    pub const EM: u8 = 0x19;
    /// Substitute (VT100 uses this to display parity errors).
    pub const SUB: u8 = 0x1A;
    /// Prefix to an escape sequence.
    pub const ESC: u8 = 0x1B;
    /// File Separator.
    pub const FS: u8 = 0x1C;
    /// Group Separator.
    pub const GS: u8 = 0x1D;
    /// Record Separator (sent by VT132 in block-transfer mode).
    pub const RS: u8 = 0x1E;
    /// Unit Separator.
    pub const US: u8 = 0x1F;
    /// Delete, should be ignored by terminal.
    pub const DEL: u8 = 0x7F;
}

// Tests for parsing escape sequences.
//
// Byte sequences used in these tests are recording of pty stdout.
#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    pub struct TestSyncHandler {
        is_sync: usize,
    }

    impl Timeout for TestSyncHandler {
        #[inline]
        fn set_timeout(&mut self, _: Duration) {
            self.is_sync += 1;
        }

        #[inline]
        fn clear_timeout(&mut self) {
            self.is_sync = 0;
        }

        #[inline]
        fn pending_timeout(&self) -> bool {
            self.is_sync != 0
        }
    }

    struct MockHandler {
        index: CharsetIndex,
        charset: StandardCharset,
        attr: Option<Attr>,
        identity_reported: bool,
        color: Option<Rgb>,
        reset_colors: Vec<usize>,
    }

    impl Handler for MockHandler {
        fn terminal_attribute(&mut self, attr: Attr) {
            self.attr = Some(attr);
        }

        fn configure_charset(&mut self, index: CharsetIndex, charset: StandardCharset) {
            self.index = index;
            self.charset = charset;
        }

        fn set_active_charset(&mut self, index: CharsetIndex) {
            self.index = index;
        }

        fn identify_terminal(&mut self, _intermediate: Option<char>) {
            self.identity_reported = true;
        }

        fn reset_state(&mut self) {
            *self = Self::default();
        }

        fn set_color(&mut self, _: usize, c: Rgb) {
            self.color = Some(c);
        }

        fn reset_color(&mut self, index: usize) {
            self.reset_colors.push(index)
        }
    }

    impl Default for MockHandler {
        fn default() -> MockHandler {
            MockHandler {
                index: CharsetIndex::G0,
                charset: StandardCharset::Ascii,
                attr: None,
                identity_reported: false,
                color: None,
                reset_colors: Vec::new(),
            }
        }
    }

    #[test]
    fn parse_control_attribute() {
        static BYTES: &[u8] = &[0x1B, b'[', b'1', b'm'];

        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        parser.advance(&mut handler, BYTES);

        assert_eq!(handler.attr, Some(Attr::Bold));
    }

    #[test]
    fn parse_terminal_identity_csi() {
        let bytes: &[u8] = &[0x1B, b'[', b'1', b'c'];

        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        parser.advance(&mut handler, bytes);

        assert!(!handler.identity_reported);
        handler.reset_state();

        let bytes: &[u8] = &[0x1B, b'[', b'c'];

        parser.advance(&mut handler, bytes);

        assert!(handler.identity_reported);
        handler.reset_state();

        let bytes: &[u8] = &[0x1B, b'[', b'0', b'c'];

        parser.advance(&mut handler, bytes);

        assert!(handler.identity_reported);
    }

    #[test]
    fn parse_terminal_identity_esc() {
        let bytes: &[u8] = &[0x1B, b'Z'];

        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        parser.advance(&mut handler, bytes);

        assert!(handler.identity_reported);
        handler.reset_state();

        let bytes: &[u8] = &[0x1B, b'#', b'Z'];

        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        parser.advance(&mut handler, bytes);

        assert!(!handler.identity_reported);
        handler.reset_state();
    }

    #[test]
    fn parse_truecolor_attr() {
        static BYTES: &[u8] = &[
            0x1B, b'[', b'3', b'8', b';', b'2', b';', b'1', b'2', b'8', b';', b'6', b'6', b';',
            b'2', b'5', b'5', b'm',
        ];

        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        parser.advance(&mut handler, BYTES);

        let spec = Rgb { r: 128, g: 66, b: 255 };

        assert_eq!(handler.attr, Some(Attr::Foreground(Color::Spec(spec))));
    }

    /// No exactly a test; useful for debugging.
    #[test]
    fn parse_zsh_startup() {
        static BYTES: &[u8] = &[
            0x1B, b'[', b'1', b'm', 0x1B, b'[', b'7', b'm', b'%', 0x1B, b'[', b'2', b'7', b'm',
            0x1B, b'[', b'1', b'm', 0x1B, b'[', b'0', b'm', b' ', b' ', b' ', b' ', b' ', b' ',
            b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ',
            b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ',
            b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ',
            b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ',
            b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ', b' ',
            b' ', b' ', b' ', b'\r', b' ', b'\r', b'\r', 0x1B, b'[', b'0', b'm', 0x1B, b'[', b'2',
            b'7', b'm', 0x1B, b'[', b'2', b'4', b'm', 0x1B, b'[', b'J', b'j', b'w', b'i', b'l',
            b'm', b'@', b'j', b'w', b'i', b'l', b'm', b'-', b'd', b'e', b's', b'k', b' ', 0x1B,
            b'[', b'0', b'1', b';', b'3', b'2', b'm', 0xE2, 0x9E, 0x9C, b' ', 0x1B, b'[', b'0',
            b'1', b';', b'3', b'2', b'm', b' ', 0x1B, b'[', b'3', b'6', b'm', b'~', b'/', b'c',
            b'o', b'd', b'e',
        ];

        let mut handler = MockHandler::default();
        let mut parser = Processor::<TestSyncHandler>::new();

        parser.advance(&mut handler, BYTES);
    }

    #[test]
    fn parse_designate_g0_as_line_drawing() {
        static BYTES: &[u8] = &[0x1B, b'(', b'0'];
        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        parser.advance(&mut handler, BYTES);

        assert_eq!(handler.index, CharsetIndex::G0);
        assert_eq!(handler.charset, StandardCharset::SpecialCharacterAndLineDrawing);
    }

    #[test]
    fn parse_designate_g1_as_line_drawing_and_invoke() {
        static BYTES: &[u8] = &[0x1B, b')', b'0', 0x0E];
        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        parser.advance(&mut handler, &BYTES[..3]);

        assert_eq!(handler.index, CharsetIndex::G1);
        assert_eq!(handler.charset, StandardCharset::SpecialCharacterAndLineDrawing);

        let mut handler = MockHandler::default();
        parser.advance(&mut handler, &[BYTES[3]]);

        assert_eq!(handler.index, CharsetIndex::G1);
    }

    #[test]
    fn parse_valid_rgb_colors() {
        assert_eq!(xparse_color(b"rgb:f/e/d"), Some(Rgb { r: 0xFF, g: 0xEE, b: 0xDD }));
        assert_eq!(xparse_color(b"rgb:11/aa/ff"), Some(Rgb { r: 0x11, g: 0xAA, b: 0xFF }));
        assert_eq!(xparse_color(b"rgb:f/ed1/cb23"), Some(Rgb { r: 0xFF, g: 0xEC, b: 0xCA }));
        assert_eq!(xparse_color(b"rgb:ffff/0/0"), Some(Rgb { r: 0xFF, g: 0x0, b: 0x0 }));
    }

    #[test]
    fn parse_valid_legacy_rgb_colors() {
        assert_eq!(xparse_color(b"#1af"), Some(Rgb { r: 0x10, g: 0xA0, b: 0xF0 }));
        assert_eq!(xparse_color(b"#11aaff"), Some(Rgb { r: 0x11, g: 0xAA, b: 0xFF }));
        assert_eq!(xparse_color(b"#110aa0ff0"), Some(Rgb { r: 0x11, g: 0xAA, b: 0xFF }));
        assert_eq!(xparse_color(b"#1100aa00ff00"), Some(Rgb { r: 0x11, g: 0xAA, b: 0xFF }));
    }

    #[test]
    fn parse_invalid_rgb_colors() {
        assert_eq!(xparse_color(b"rgb:0//"), None);
        assert_eq!(xparse_color(b"rgb://///"), None);
    }

    #[test]
    fn parse_invalid_legacy_rgb_colors() {
        assert_eq!(xparse_color(b"#"), None);
        assert_eq!(xparse_color(b"#f"), None);
    }

    #[test]
    fn parse_invalid_number() {
        assert_eq!(parse_color_index(b"1abc"), None);
    }

    #[test]
    fn parse_valid_number() {
        assert_eq!(parse_color_index(b"123"), Some(123));
    }

    #[test]
    fn parse_color_index_too_large() {
        assert_eq!(parse_color_index(b"512"), None);
    }

    #[test]
    fn parse_osc4_set_color() {
        let bytes: &[u8] = b"\x1b]4;0;#fff\x1b\\";

        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        parser.advance(&mut handler, bytes);

        assert_eq!(handler.color, Some(Rgb { r: 0xF0, g: 0xF0, b: 0xF0 }));
    }

    #[test]
    fn parse_osc104_reset_color() {
        let bytes: &[u8] = b"\x1b]104;1;\x1b\\";

        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        parser.advance(&mut handler, bytes);

        assert_eq!(handler.reset_colors, vec![1]);
    }

    #[test]
    fn parse_osc104_reset_all_colors() {
        let bytes: &[u8] = b"\x1b]104;\x1b\\";

        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        parser.advance(&mut handler, bytes);

        let expected: Vec<usize> = (0..256).collect();
        assert_eq!(handler.reset_colors, expected);
    }

    #[test]
    fn parse_osc104_reset_all_colors_no_semicolon() {
        let bytes: &[u8] = b"\x1b]104\x1b\\";

        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        parser.advance(&mut handler, bytes);

        let expected: Vec<usize> = (0..256).collect();
        assert_eq!(handler.reset_colors, expected);
    }

    #[test]
    fn partial_sync_updates() {
        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        assert_eq!(parser.state.sync_state.timeout.is_sync, 0);
        assert!(handler.attr.is_none());

        // Start synchronized update.

        parser.advance(&mut handler, b"\x1b[?20");
        assert_eq!(parser.state.sync_state.timeout.is_sync, 0);
        assert!(handler.attr.is_none());

        parser.advance(&mut handler, b"26h");
        assert_eq!(parser.state.sync_state.timeout.is_sync, 1);
        assert!(handler.attr.is_none());

        // Dispatch some data.

        parser.advance(&mut handler, b"random \x1b[31m stuff");
        assert_eq!(parser.state.sync_state.timeout.is_sync, 1);
        assert!(handler.attr.is_none());

        // Extend synchronized update.

        parser.advance(&mut handler, b"\x1b[?20");
        assert_eq!(parser.state.sync_state.timeout.is_sync, 1);
        assert!(handler.attr.is_none());

        parser.advance(&mut handler, b"26h");
        assert_eq!(parser.state.sync_state.timeout.is_sync, 2);
        assert!(handler.attr.is_none());

        // Terminate synchronized update.

        parser.advance(&mut handler, b"\x1b[?20");
        assert_eq!(parser.state.sync_state.timeout.is_sync, 2);
        assert!(handler.attr.is_none());

        parser.advance(&mut handler, b"26l");
        assert_eq!(parser.state.sync_state.timeout.is_sync, 0);
        assert!(handler.attr.is_some());
    }

    #[test]
    fn sync_bursts_buffer() {
        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        assert_eq!(parser.state.sync_state.timeout.is_sync, 0);
        assert!(handler.attr.is_none());

        // Repeat test twice to ensure internal state is reset properly.
        for _ in 0..2 {
            // Start synchronized update.
            parser.advance(&mut handler, b"\x1b[?2026h");
            assert_eq!(parser.state.sync_state.timeout.is_sync, 1);
            assert!(handler.attr.is_none());

            // Ensure sync works.
            parser.advance(&mut handler, b"\x1b[31m");
            assert_eq!(parser.state.sync_state.timeout.is_sync, 1);
            assert!(handler.attr.is_none());

            // Exceed sync buffer dimensions.
            parser.advance(&mut handler, "a".repeat(SYNC_BUFFER_SIZE).as_bytes());
            assert_eq!(parser.state.sync_state.timeout.is_sync, 0);
            assert!(handler.attr.take().is_some());

            // Ensure new events are dispatched directly.
            parser.advance(&mut handler, b"\x1b[31m");
            assert_eq!(parser.state.sync_state.timeout.is_sync, 0);
            assert!(handler.attr.take().is_some());
        }
    }

    #[test]
    fn mixed_sync_escape() {
        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        assert_eq!(parser.state.sync_state.timeout.is_sync, 0);
        assert!(handler.attr.is_none());

        // Start synchronized update with immediate SGR.
        parser.advance(&mut handler, b"\x1b[?2026h\x1b[31m");
        assert_eq!(parser.state.sync_state.timeout.is_sync, 1);
        assert!(handler.attr.is_none());

        // Terminate synchronized update and check for SGR.
        parser.advance(&mut handler, b"\x1b[?2026l");
        assert_eq!(parser.state.sync_state.timeout.is_sync, 0);
        assert!(handler.attr.is_some());
    }

    #[test]
    fn sync_bsu_with_esu() {
        let mut parser = Processor::<TestSyncHandler>::new();
        let mut handler = MockHandler::default();

        assert_eq!(parser.state.sync_state.timeout.is_sync, 0);
        assert!(handler.attr.is_none());

        // Start synchronized update with immediate SGR.
        parser.advance(&mut handler, b"\x1b[?2026h\x1b[1m");
        assert_eq!(parser.state.sync_state.timeout.is_sync, 1);
        assert!(handler.attr.is_none());

        // Terminate synchronized update, but immediately start a new one.
        parser.advance(&mut handler, b"\x1b[?2026l\x1b[?2026h\x1b[4m");
        assert_eq!(parser.state.sync_state.timeout.is_sync, 2);
        assert_eq!(handler.attr.take(), Some(Attr::Bold));

        // Terminate again, expecting one buffered SGR.
        parser.advance(&mut handler, b"\x1b[?2026l");
        assert_eq!(parser.state.sync_state.timeout.is_sync, 0);
        assert_eq!(handler.attr.take(), Some(Attr::Underline));
    }

    #[test]
    #[cfg(feature = "std")]
    fn contrast() {
        let rgb1 = Rgb { r: 0xFF, g: 0xFF, b: 0xFF };
        let rgb2 = Rgb { r: 0x00, g: 0x00, b: 0x00 };
        assert!((rgb1.contrast(rgb2) - 21.).abs() < f64::EPSILON);

        let rgb1 = Rgb { r: 0xFF, g: 0xFF, b: 0xFF };
        assert!((rgb1.contrast(rgb1) - 1.).abs() < f64::EPSILON);

        let rgb1 = Rgb { r: 0xFF, g: 0x00, b: 0xFF };
        let rgb2 = Rgb { r: 0x00, g: 0xFF, b: 0x00 };
        assert!((rgb1.contrast(rgb2) - 2.285_543_608_124_253_3).abs() < f64::EPSILON);

        let rgb1 = Rgb { r: 0x12, g: 0x34, b: 0x56 };
        let rgb2 = Rgb { r: 0xFE, g: 0xDC, b: 0xBA };
        assert!((rgb1.contrast(rgb2) - 9.786_558_997_257_74).abs() < f64::EPSILON);
    }
}
