//! 닫은 것 되살리기(#164, Ghostty `undo-timeout`·`undo`·`redo`). 창 그리기와 상관없는 순수 자료 구조라 여기서 테스트한다.
//!
//! 닫은 것을 바로 버리지 않고 `undo-timeout` 동안 들고 있다가, 되살리면 꺼내 주고 기한이 지나면 버린다.
//! 기한은 일마다 따로다(새 일이 앞선 일의 기한을 미루지 않는다, Ghostty와 같다)

use std::time::{Duration, Instant};

/// 기한이 있는 되살리기 목록. 나중에 넣은 것이 위(먼저 되살아난다)
#[derive(Debug)]
pub struct UndoStack<T> {
    items: Vec<(Instant, T)>,
}

impl<T> Default for UndoStack<T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<T> UndoStack<T> {
    /// `timeout` 동안 들고 있는다. 0이면 들지 않고 돌려준다(되살리기 끔)
    pub fn push(&mut self, now: Instant, timeout: Duration, item: T) -> Result<(), T> {
        if timeout.is_zero() {
            return Err(item);
        }
        // 아주 큰 기한(Ghostty는 수년도 받는다)은 더하다 넘치지 않게 자른다
        let deadline = now
            .checked_add(timeout)
            .unwrap_or_else(|| now + Duration::from_secs(100 * 365 * 86_400));
        self.items.push((deadline, item));
        Ok(())
    }

    /// 기한이 지난 것을 목록에서 빼 돌려준다(버리는 것은 부른 쪽)
    pub fn expire(&mut self, now: Instant) -> Vec<T> {
        let (dead, alive): (Vec<_>, Vec<_>) = std::mem::take(&mut self.items)
            .into_iter()
            .partition(|(d, _)| *d <= now);
        self.items = alive;
        dead.into_iter().map(|(_, t)| t).collect()
    }

    /// 기한이 남은 것 중 가장 나중에 넣은 것을 꺼낸다
    pub fn pop(&mut self, now: Instant) -> Option<T> {
        let i = self.items.iter().rposition(|(d, _)| *d > now)?;
        Some(self.items.remove(i).1)
    }

    /// 기한이 남은 것이 있는가
    pub fn has_alive(&self, now: Instant) -> bool {
        self.items.iter().any(|(d, _)| *d > now)
    }

    /// 가장 이른 기한(깨어날 시각)
    pub fn next_deadline(&self) -> Option<Instant> {
        self.items.iter().map(|(d, _)| *d).min()
    }

    /// 조건에 맞는 것을 빼 돌려준다
    pub fn remove_where(&mut self, f: impl Fn(&T) -> bool) -> Vec<T> {
        let (hit, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.items).into_iter().partition(|(_, t)| f(t));
        self.items = keep;
        hit.into_iter().map(|(_, t)| t).collect()
    }

    /// 들고 있는 것을 모두 돌려준다
    pub fn clear(&mut self) -> Vec<T> {
        std::mem::take(&mut self.items).into_iter().map(|(_, t)| t).collect()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.items.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: Duration = Duration::from_secs(1);

    #[test]
    fn pop_returns_newest_first() {
        let t0 = Instant::now();
        let mut s = UndoStack::default();
        s.push(t0, 5 * S, "a").unwrap();
        s.push(t0 + S, 5 * S, "b").unwrap();
        assert_eq!(s.pop(t0 + 2 * S), Some("b"));
        assert_eq!(s.pop(t0 + 2 * S), Some("a"));
        assert_eq!(s.pop(t0 + 2 * S), None);
    }

    #[test]
    fn expired_items_are_not_restored() {
        let t0 = Instant::now();
        let mut s = UndoStack::default();
        s.push(t0, 5 * S, "old").unwrap();
        s.push(t0 + 4 * S, 5 * S, "new").unwrap();
        // old는 5초에 끝나고 new는 9초까지 산다
        assert!(s.has_alive(t0 + 6 * S));
        assert_eq!(s.pop(t0 + 6 * S), Some("new"));
        assert_eq!(s.pop(t0 + 6 * S), None, "old는 기한이 지나 꺼낼 수 없다");
    }

    #[test]
    fn each_item_has_its_own_deadline() {
        let t0 = Instant::now();
        let mut s = UndoStack::default();
        s.push(t0, 5 * S, "a").unwrap();
        s.push(t0 + 3 * S, 5 * S, "b").unwrap();
        // b를 넣어도 a의 기한(5초)은 밀리지 않는다
        let dropped = s.expire(t0 + 5 * S);
        assert_eq!(dropped, vec!["a"]);
        assert_eq!(s.len(), 1);
        assert_eq!(s.next_deadline(), Some(t0 + 8 * S));
    }

    #[test]
    fn expire_hands_back_dropped_items_in_order() {
        let t0 = Instant::now();
        let mut s = UndoStack::default();
        s.push(t0, S, 1).unwrap();
        s.push(t0, 2 * S, 2).unwrap();
        s.push(t0, 10 * S, 3).unwrap();
        assert_eq!(s.expire(t0 + 3 * S), vec![1, 2]);
        assert_eq!(s.expire(t0 + 3 * S), Vec::<i32>::new(), "두 번 불러도 같다");
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn zero_timeout_keeps_nothing() {
        let t0 = Instant::now();
        let mut s = UndoStack::default();
        assert_eq!(s.push(t0, Duration::ZERO, "x"), Err("x"));
        assert_eq!(s.len(), 0);
        assert!(!s.has_alive(t0));
        assert_eq!(s.next_deadline(), None);
    }

    #[test]
    fn deadline_is_exclusive() {
        let t0 = Instant::now();
        let mut s = UndoStack::default();
        s.push(t0, 5 * S, "a").unwrap();
        assert!(s.has_alive(t0 + 5 * S - Duration::from_millis(1)));
        assert!(!s.has_alive(t0 + 5 * S), "기한 시각 자체는 이미 지난 것");
    }

    #[test]
    fn remove_where_and_clear() {
        let t0 = Instant::now();
        let mut s = UndoStack::default();
        s.push(t0, 5 * S, 1).unwrap();
        s.push(t0, 5 * S, 2).unwrap();
        s.push(t0, 5 * S, 3).unwrap();
        assert_eq!(s.remove_where(|x| *x == 2), vec![2]);
        assert_eq!(s.pop(t0), Some(3));
        assert_eq!(s.clear(), vec![1]);
        s.push(t0, 5 * S, 9).unwrap();
        assert_eq!(s.clear(), vec![9]);
        assert_eq!(s.len(), 0);
    }
}
