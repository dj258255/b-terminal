//! 제어 소켓 `watch`의 이벤트 모음(#925). 앱 스레드가 `Hub::publish`로 이벤트를 던지면, 구독자마다 묶음 상한이 있는 큐에 한 줄씩 쌓고
//! 소켓 스레드가 그것을 써 보낸다. 앱 스레드는 어떤 경우에도 기다리지 않는다: 큐가 가득 찬 느린 구독자는 끊고, 구독자 수에도 상한이 있다.
//! 이벤트에는 화면 글·명령 글·출력·알림 본문을 싣지 않는다(`bt_core::proto::Event`).

use std::collections::HashMap;
use std::io::Write;
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bt_core::proto::{Event, event_line, export_text};

/// 동시에 받는 구독자 수의 상한
pub const MAX_SUBSCRIBERS: usize = 8;
/// 구독자 하나의 큐 길이(줄). 이만큼 밀리면 그 구독자를 끊는다
pub const QUEUE_LINES: usize = 256;
/// 조용할 때 연결이 살아 있는지 확인하려고 `ping`을 보내는 간격
pub const HEARTBEAT: Duration = Duration::from_secs(15);

struct Sub {
    id: u64,
    tx: SyncSender<Arc<str>>,
}

#[derive(Default)]
struct Inner {
    subs: Vec<Sub>,
    next_id: u64,
    /// 칸마다 마지막으로 내보낸 상태 이벤트(같은 것을 되풀이해 큐를 채우지 않는다)
    last_status: HashMap<u64, Event>,
}

/// 이벤트 모음. 앱과 소켓 스레드가 복제해 쥔다
#[derive(Clone, Default)]
pub struct Hub {
    inner: Arc<Mutex<Inner>>,
}

/// 구독 하나. 떨어뜨리면 구독자 목록에서 빠진다
pub struct Subscription {
    id: u64,
    rx: Receiver<Arc<str>>,
    hub: Hub,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Ok(mut g) = self.hub.inner.lock() {
            g.subs.retain(|s| s.id != self.id);
        }
    }
}

impl Hub {
    /// 구독을 연다. 구독자가 이미 `MAX_SUBSCRIBERS`명이면 거절한다
    pub fn subscribe(&self) -> Result<Subscription, &'static str> {
        let mut g = self.inner.lock().map_err(|_| "이벤트 모음을 쓸 수 없습니다")?;
        if g.subs.len() >= MAX_SUBSCRIBERS {
            return Err("이벤트를 받는 연결이 너무 많습니다");
        }
        let (tx, rx) = sync_channel(QUEUE_LINES);
        g.next_id += 1;
        let id = g.next_id;
        g.subs.push(Sub { id, tx });
        Ok(Subscription {
            id,
            rx,
            hub: self.clone(),
        })
    }

    #[cfg(test)]
    pub fn subscribers(&self) -> usize {
        self.inner.lock().map(|g| g.subs.len()).unwrap_or(0)
    }

    /// 이벤트를 모든 구독자에게 던진다. 기다리지 않는다: 큐가 가득 찼거나 끊긴 구독자는 목록에서 뺀다(그 연결은 곧 닫힌다).
    /// 바로 앞과 같은 상태 이벤트는 건너뛴다
    pub fn publish(&self, e: Event) {
        let Ok(mut g) = self.inner.lock() else { return };
        match &e {
            Event::Status { tab, .. } => {
                if g.last_status.get(tab) == Some(&e) {
                    return;
                }
                g.last_status.insert(*tab, e.clone());
            }
            Event::TabClosed { tab } => {
                g.last_status.remove(tab);
            }
            _ => {}
        }
        if g.subs.is_empty() {
            return;
        }
        let line: Arc<str> = event_line(&e).into();
        g.subs.retain(|s| match s.tx.try_send(line.clone()) {
            Ok(()) => true,
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => false,
        });
    }
}

impl Subscription {
    /// 이벤트를 `w`에 한 줄씩 쓴다. 쓰기에 실패하거나(느린 손님은 소켓의 쓰기 기한이 끊는다) 큐에서 밀려 끊기면 돌아온다.
    /// 조용한 동안은 `heartbeat`마다 `ping`을 써서 닫힌 연결을 알아챈다
    pub fn pump(&self, w: &mut impl Write, heartbeat: Duration) {
        loop {
            let line: Arc<str> = match self.rx.recv_timeout(heartbeat) {
                Ok(l) => l,
                Err(RecvTimeoutError::Timeout) => event_line(&Event::Ping).into(),
                Err(RecvTimeoutError::Disconnected) => return,
            };
            if w.write_all(line.as_bytes()).is_err() || w.write_all(b"\n").is_err() || w.flush().is_err() {
                return;
            }
        }
    }

    #[cfg(test)]
    fn try_next(&self) -> Option<Arc<str>> {
        self.rx.try_recv().ok()
    }
}

/// 칸의 상태 이벤트. `state`가 `None`이면 보일 기록이 없다. 글은 소켓용으로 한 번 더 거른다
pub fn status_event(tab: u64, state: Option<(&'static str, Option<u8>)>, text: Option<&str>) -> Event {
    match state {
        Some((name, progress)) => Event::Status {
            tab,
            state: name.into(),
            progress,
            text: text.and_then(export_text),
        },
        None => Event::Status {
            tab,
            state: "none".into(),
            progress: None,
            text: None,
        },
    }
}

/// 명령 끝 이벤트. 명령 글과 출력은 받지 않는다(종료 코드와 걸린 시간만)
pub fn finished_event(tab: u64, exit_code: Option<i32>, duration: Duration) -> Event {
    Event::CommandFinished {
        tab,
        exit_code,
        duration_ms: duration.as_millis().min(u64::MAX as u128) as u64,
    }
}

/// 데스크톱 알림 이벤트. 제목만 싣는다
pub fn notification_event(tab: u64, title: &str) -> Event {
    Event::Notification {
        tab,
        title: export_text(title).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn st(tab: u64, p: u8) -> Event {
        status_event(tab, Some(("working", Some(p))), None)
    }

    #[test]
    fn subscriber_gets_one_json_line_per_event() {
        let hub = Hub::default();
        let sub = hub.subscribe().unwrap();
        hub.publish(Event::TabOpened { tab: 4 });
        hub.publish(finished_event(4, Some(3), Duration::from_millis(1500)));
        assert_eq!(&*sub.try_next().unwrap(), r#"{"event":"tab_opened","tab":4}"#);
        let l = sub.try_next().unwrap();
        assert!(
            l.contains(r#""exit_code":3"#) && l.contains(r#""duration_ms":1500"#),
            "{l}"
        );
        assert!(sub.try_next().is_none());
    }

    /// 구독자가 없으면 던져도 아무 일 없고, 앱이 기다리지 않는다
    #[test]
    fn publish_without_subscribers_is_a_no_op() {
        let hub = Hub::default();
        hub.publish(Event::TabOpened { tab: 1 });
        assert_eq!(hub.subscribers(), 0);
    }

    /// 읽지 않는 구독자는 큐가 차면 끊기고, 그동안 publish는 한 번도 막히지 않는다. 읽는 구독자는 영향이 없다
    #[test]
    fn slow_subscriber_is_dropped_and_publish_never_blocks() {
        let hub = Hub::default();
        let slow = hub.subscribe().unwrap();
        let fast = hub.subscribe().unwrap();
        let start = Instant::now();
        for i in 0..(QUEUE_LINES as u64 + 10) {
            hub.publish(Event::TabOpened { tab: i });
            while fast.try_next().is_some() {}
        }
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(hub.subscribers(), 1, "느린 구독자만 빠진다");
        // 끊긴 구독자의 pump는 큐를 다 쓴 뒤 돌아온다
        let mut sink = Vec::new();
        slow.pump(&mut sink, Duration::from_secs(5));
        assert_eq!(sink.iter().filter(|b| **b == b'\n').count(), QUEUE_LINES);
        hub.publish(Event::TabOpened { tab: 999 });
        assert!(fast.try_next().is_some());
    }

    #[test]
    fn subscriber_count_is_capped_and_freed_on_drop() {
        let hub = Hub::default();
        let subs: Vec<_> = (0..MAX_SUBSCRIBERS).map(|_| hub.subscribe().unwrap()).collect();
        assert!(hub.subscribe().is_err());
        drop(subs);
        assert_eq!(hub.subscribers(), 0);
        assert!(hub.subscribe().is_ok());
    }

    #[test]
    fn same_status_twice_is_sent_once_and_close_forgets_it() {
        let hub = Hub::default();
        let sub = hub.subscribe().unwrap();
        hub.publish(st(1, 10));
        hub.publish(st(1, 10));
        hub.publish(st(1, 20));
        hub.publish(st(2, 20));
        let mut n = 0;
        while sub.try_next().is_some() {
            n += 1;
        }
        assert_eq!(n, 3);
        hub.publish(Event::TabClosed { tab: 1 });
        hub.publish(st(1, 20));
        assert_eq!(sub.try_next().map(|l| l.contains("tab_closed")), Some(true));
        assert!(sub.try_next().is_some(), "닫힌 칸의 번호는 다시 쓰여도 새로 알린다");
    }

    /// 쓰기가 실패하는 손님(끊긴 연결)에서 pump는 곧 돌아온다
    #[test]
    fn pump_returns_when_the_writer_fails() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                Err(std::io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let hub = Hub::default();
        let sub = hub.subscribe().unwrap();
        hub.publish(Event::TabOpened { tab: 1 });
        sub.pump(&mut Broken, Duration::from_secs(5));
        // 조용해도 하트비트로 쓰기를 시도해 끊긴 연결을 알아챈다
        let quiet = hub.subscribe().unwrap();
        let start = Instant::now();
        quiet.pump(&mut Broken, Duration::from_millis(20));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    /// 보안: 이벤트에는 글이 정해진 자리(상태 제목·알림 제목)에만 있고, 거르고 자른 값이다. 명령 글·출력 자리는 없다
    #[test]
    fn events_carry_only_cleaned_short_text() {
        let long = "x".repeat(1000);
        let e = status_event(5, Some(("blocked", None)), Some(&format!("\x1b]0;evil\x07{long}")));
        let line = event_line(&e);
        assert!(!line.contains('\x1b') && !line.contains('\u{7}'));
        assert!(line.len() < 600, "{}", line.len());
        let n = notification_event(5, "빌드\u{202E} 끝\n줄");
        assert_eq!(
            event_line(&n),
            r#"{"event":"notification","tab":5,"title":"빌드 끝 줄"}"#
        );
        let f = event_line(&finished_event(5, None, Duration::from_secs(2)));
        assert_eq!(
            f,
            r#"{"event":"command_finished","tab":5,"exit_code":null,"duration_ms":2000}"#
        );
        assert_eq!(
            event_line(&status_event(5, None, Some("무시"))),
            r#"{"event":"status","tab":5,"state":"none"}"#
        );
    }
}
