//! PTY 바이트에서 알림을 뽑는다. 채우는 것은 #8이다

/// 프로그램이 터미널에 보낸 알림
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

/// 조각난 바이트를 이어 가며 알림을 찾는다
#[derive(Debug, Default)]
pub struct Scanner {}

impl Scanner {
    pub fn feed(&mut self, _bytes: &[u8]) -> Vec<Notification> {
        Vec::new()
    }
}
