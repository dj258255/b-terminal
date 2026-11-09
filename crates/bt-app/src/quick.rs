//! 퀵 터미널의 자리·크기·화면·움직임 계산(#110). 창과 상관없는 순수 함수라 여기서 테스트한다.
//! 좌표는 논리 점(point), 원점은 주 화면 왼쪽 위다(macOS CG 좌표와 같다)

use bt_core::config::{QuickDim, QuickPosition, QuickTerminal};

/// 화면이나 창의 사각형(논리 점)
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// 창이 너무 작아지지 않게(점)
const MIN_SIDE: f64 = 120.0;

/// 설정과 화면으로 퀵 창이 놓일 자리
pub fn frame(q: &QuickTerminal, screen: Rect) -> Rect {
    let dim = |d: QuickDim, full: f64| {
        let v = match d {
            QuickDim::Percent(p) => full * p / 100.0,
            QuickDim::Px(px) => px,
        };
        v.clamp(MIN_SIDE.min(full), full)
    };
    let mid = |start: f64, full: f64, len: f64| start + (full - len) / 2.0;
    match q.position {
        QuickPosition::Top | QuickPosition::Bottom => {
            let h = dim(q.size, screen.h);
            let w = q.size2.map_or(screen.w, |d| dim(d, screen.w));
            let y = if q.position == QuickPosition::Top {
                screen.y
            } else {
                screen.y + screen.h - h
            };
            Rect {
                x: mid(screen.x, screen.w, w),
                y,
                w,
                h,
            }
        }
        QuickPosition::Left | QuickPosition::Right => {
            let w = dim(q.size, screen.w);
            let h = q.size2.map_or(screen.h, |d| dim(d, screen.h));
            let x = if q.position == QuickPosition::Left {
                screen.x
            } else {
                screen.x + screen.w - w
            };
            Rect {
                x,
                y: mid(screen.y, screen.h, h),
                w,
                h,
            }
        }
        QuickPosition::Center => {
            let w = dim(q.size, screen.w);
            let h = dim(q.size2.unwrap_or(q.size), screen.h);
            Rect {
                x: mid(screen.x, screen.w, w),
                y: mid(screen.y, screen.h, h),
                w,
                h,
            }
        }
    }
}

/// 움직임이 시작(보일 때)·끝(숨을 때)나는 자리: 붙은 가장자리 바깥. 가운데는 움직이지 않는다
pub fn hidden_origin(pos: QuickPosition, frame: Rect, screen: Rect) -> (f64, f64) {
    match pos {
        QuickPosition::Top => (frame.x, screen.y - frame.h),
        QuickPosition::Bottom => (frame.x, screen.y + screen.h),
        QuickPosition::Left => (screen.x - frame.w, frame.y),
        QuickPosition::Right => (screen.x + screen.w, frame.y),
        QuickPosition::Center => (frame.x, frame.y),
    }
}

/// `t`(0~1) 때의 자리. 끝에서 느려진다(ease-out)
pub fn slide(from: (f64, f64), to: (f64, f64), t: f64) -> (f64, f64) {
    let t = t.clamp(0.0, 1.0);
    let e = 1.0 - (1.0 - t).powi(3);
    (from.0 + (to.0 - from.0) * e, from.1 + (to.1 - from.1) * e)
}

/// 이 점이 든 화면(`quick-terminal-screen = mouse`, macOS에서만 마우스 자리를 안다)
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn screen_at(p: (f64, f64), screens: &[Rect]) -> Option<usize> {
    screens
        .iter()
        .position(|r| p.0 >= r.x && p.0 < r.x + r.w && p.1 >= r.y && p.1 < r.y + r.h)
}

/// macOS가 권하는 화면(`NSScreen.mainScreen`: 앞에 있는 창의 화면)의 사각형. `primary_h`는 주 화면 높이(점)
#[cfg(target_os = "macos")]
pub fn main_screen(primary_h: f64) -> Option<Rect> {
    let mtm = objc2::MainThreadMarker::new()?;
    let f = objc2_app_kit::NSScreen::mainScreen(mtm)?.frame();
    // Cocoa는 주 화면 왼쪽 아래가 원점이고 위가 양수다
    Some(Rect {
        x: f.origin.x,
        y: primary_h - (f.origin.y + f.size.height),
        w: f.size.width,
        h: f.size.height,
    })
}

/// 이 화면(winit의 사각형)에서 메뉴 막대와 Dock을 뺀 영역. 찾지 못하면 없다
#[cfg(target_os = "macos")]
pub fn visible_area(screen: Rect, primary_h: f64) -> Option<Rect> {
    let mtm = objc2::MainThreadMarker::new()?;
    // Cocoa는 주 화면 왼쪽 아래가 원점이고 위가 양수다
    let flip = |x: f64, y: f64, w: f64, h: f64| Rect {
        x,
        y: primary_h - (y + h),
        w,
        h,
    };
    objc2_app_kit::NSScreen::screens(mtm)
        .iter()
        .find(|s| {
            let f = s.frame();
            let f = flip(f.origin.x, f.origin.y, f.size.width, f.size.height);
            (f.x - screen.x).abs() < 2.0 && (f.y - screen.y).abs() < 2.0
        })
        .map(|s| {
            let v = s.visibleFrame();
            flip(v.origin.x, v.origin.y, v.size.width, v.size.height)
        })
}

/// 마우스 자리(점, 주 화면 왼쪽 위가 원점)
#[cfg(target_os = "macos")]
pub fn mouse_location(primary_h: f64) -> (f64, f64) {
    let p = objc2_app_kit::NSEvent::mouseLocation();
    (p.x, primary_h - p.y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bt_core::config::QuickScreen;

    const SCREEN: Rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1000.0,
        h: 800.0,
    };

    fn q(position: QuickPosition, size: QuickDim, size2: Option<QuickDim>) -> QuickTerminal {
        QuickTerminal {
            position,
            size,
            size2,
            screen: QuickScreen::Main,
            autohide: true,
            animation: 0.2,
        }
    }

    #[test]
    fn frames_stick_to_the_edge_like_ghostty() {
        use QuickDim::{Percent, Px};
        use QuickPosition::*;
        let r = |x, y, w, h| Rect { x, y, w, h };
        assert_eq!(frame(&q(Top, Percent(40.0), None), SCREEN), r(0.0, 0.0, 1000.0, 320.0));
        assert_eq!(frame(&q(Bottom, Px(200.0), None), SCREEN), r(0.0, 600.0, 1000.0, 200.0));
        // 다른 축을 주면 가운데에
        assert_eq!(
            frame(&q(Top, Percent(50.0), Some(Px(600.0))), SCREEN),
            r(200.0, 0.0, 600.0, 400.0)
        );
        assert_eq!(frame(&q(Left, Percent(30.0), None), SCREEN), r(0.0, 0.0, 300.0, 800.0));
        assert_eq!(
            frame(&q(Right, Px(250.0), Some(Percent(50.0))), SCREEN),
            r(750.0, 200.0, 250.0, 400.0)
        );
        // 가운데: 다른 축이 없으면 같은 값
        assert_eq!(
            frame(&q(Center, Percent(50.0), None), SCREEN),
            r(250.0, 200.0, 500.0, 400.0)
        );
        // 화면보다 크면 화면까지, 너무 작으면 MIN_SIDE까지
        assert_eq!(frame(&q(Top, Px(5000.0), None), SCREEN).h, 800.0);
        assert_eq!(frame(&q(Top, Px(10.0), None), SCREEN).h, MIN_SIDE);
        // 다른 화면(왼쪽 위가 0이 아님)에서도 그 화면 안에
        let second = r(1000.0, -200.0, 1600.0, 900.0);
        assert_eq!(
            frame(&q(Bottom, Percent(20.0), None), second),
            r(1000.0, 520.0, 1600.0, 180.0)
        );
    }

    #[test]
    fn slides_in_from_outside_the_edge() {
        use QuickPosition::*;
        let f = frame(&q(Top, QuickDim::Percent(40.0), None), SCREEN);
        assert_eq!(hidden_origin(Top, f, SCREEN), (0.0, -320.0));
        let f = frame(&q(Bottom, QuickDim::Percent(25.0), None), SCREEN);
        assert_eq!(hidden_origin(Bottom, f, SCREEN), (0.0, 800.0));
        let f = frame(&q(Left, QuickDim::Px(300.0), None), SCREEN);
        assert_eq!(hidden_origin(Left, f, SCREEN), (-300.0, 0.0));
        let f = frame(&q(Right, QuickDim::Px(300.0), None), SCREEN);
        assert_eq!(hidden_origin(Right, f, SCREEN), (1000.0, 0.0));
        let f = frame(&q(Center, QuickDim::Percent(50.0), None), SCREEN);
        assert_eq!(hidden_origin(Center, f, SCREEN), (f.x, f.y), "가운데는 제자리");
        // 움직임: 처음과 끝, 중간은 절반보다 많이 와 있다(끝에서 느려짐), 범위 밖은 끝에 붙음
        let (from, to) = ((0.0, -320.0), (0.0, 0.0));
        assert_eq!(slide(from, to, 0.0), from);
        assert_eq!(slide(from, to, 1.0), to);
        assert!(slide(from, to, 0.5).1 > -160.0);
        assert_eq!(slide(from, to, 7.0), to);
        assert_eq!(slide(from, to, -1.0), from);
    }

    #[test]
    fn finds_the_screen_under_a_point() {
        let screens = [
            SCREEN,
            Rect {
                x: 1000.0,
                y: -200.0,
                w: 1600.0,
                h: 900.0,
            },
        ];
        assert_eq!(screen_at((10.0, 10.0), &screens), Some(0));
        assert_eq!(screen_at((1500.0, -100.0), &screens), Some(1));
        assert_eq!(screen_at((999.9, 799.9), &screens), Some(0), "오른쪽 아래 끝 바로 안");
        assert_eq!(screen_at((1000.0, 0.0), &screens), Some(1), "경계는 오른쪽 화면");
        assert_eq!(screen_at((-5.0, 0.0), &screens), None);
    }
}
