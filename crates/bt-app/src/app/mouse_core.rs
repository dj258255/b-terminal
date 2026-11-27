//! 마우스 처리의 공유 부분(#662): 사람의 마우스(`App::window_event`)와 스크립트 마우스(`send mouse ...`)가 함께 지나는 길.
//!
//! 이 파일의 함수는 사람의 상태(`Win::cursor_pos`, `selecting`, `last_click`, `scroll_accum`, `link_click`, 칸의 `mouse_down` 따위)와
//! 사람의 수식 키(`App::mods`)를 읽거나 쓰지 않는다. 위치와 수식 키는 `PtrIn`으로, 바뀌는 상태는 `PtrState`로만 받는다.
//! 사람 쪽은 흩어진 상태를 모아 넘기고 돌아오면 바뀐 것만 돌려놓는다(`App::human_ptr`·`store_human_ptr`). 스크립트 쪽은 칸마다 둔
//! `ScriptMouse`의 상태를 넘긴다. 시험(`script_mouse::tests::shared_mouse_code_never_touches_the_human_state`)이 이 파일에 사람 상태 이름이
//! 없음을 지킨다.
//!
//! `PtrIn::script`가 참이면 클립보드를 읽는 동작(붙여넣기)과 모달 메뉴는 하지 않는다. 클립보드를 읽는 길은 다른 앱이 이 터미널을 거쳐
//! 클립보드를 읽어 가는 길이고, kitty 붙여넣기 이벤트(모드 5522)는 프로그램에 클립보드를 읽을 일회용 암호를 준다. 메뉴는 스크립트가 시간 초과로
//! 막힌다. 복사(`right-click-action = copy`, `copy-or-paste`의 복사 쪽, `copy-on-select`)는 한다.

use winit::dpi::PhysicalPosition;
use winit::event::MouseButton;

use super::{App, Instant};
use crate::mouse;
use crate::script_mouse::{Click, PtrMods, PtrState, next_click};

/// 마우스 사건의 입력: 위치(창 실제 픽셀)와 수식 키, 그리고 스크립트가 보낸 것인가
#[derive(Debug, Clone, Copy)]
pub(super) struct PtrIn {
    pub pos: PhysicalPosition<f64>,
    pub mods: PtrMods,
    pub script: bool,
}

impl PtrIn {
    /// 링크를 짚는 수식 키: macOS는 Cmd, 그 밖에는 Ctrl(Ghostty와 같다)
    fn link_mod(&self) -> bool {
        if cfg!(target_os = "macos") {
            self.mods.sup
        } else {
            self.mods.ctrl
        }
    }

    /// 사각 고르기 키인가(#433). macOS는 Option, 그 밖에는 Ctrl(또는 Super)+Alt
    fn rectangle_select(&self) -> bool {
        mouse::is_rectangle_select(cfg!(target_os = "macos"), self.mods.alt, self.mods.ctrl, self.mods.sup)
    }
}

/// winit의 단추를 마우스 보고의 단추로. 모르는 단추는 `None`
pub(super) fn report_button(button: MouseButton) -> Option<mouse::Button> {
    match button {
        MouseButton::Left => Some(mouse::Button::Left),
        MouseButton::Middle => Some(mouse::Button::Middle),
        MouseButton::Right => Some(mouse::Button::Right),
        MouseButton::Back => Some(mouse::Button::Back),
        MouseButton::Forward => Some(mouse::Button::Forward),
        _ => None,
    }
}

impl App {
    /// 칸 idx의 프로그램이 마우스 보고를 켰으면 그 모드. `shift`는 이 사건의 Shift다(mouse-shift-capture가 정한다)
    pub(super) fn mouse_reporting_with(&self, idx: usize, shift: bool) -> Option<bt_term::Modes> {
        let pane = self.panes.get(idx)?;
        // 읽기 전용 칸은 프로그램에 마우스를 보내지 않는다. 마우스는 고르기·스크롤에 쓴다(#209, ADR-018)
        if pane.session.readonly() {
            return None;
        }
        let m = pane.session.modes();
        // mouse-reporting을 끄면 보내지 않는다. Shift를 누르면 mouse-shift-capture가 true·always일 때만
        // 프로그램이 받고, 아니면 고르기를 한다(xterm 관례)
        Self::mouse_mode_on(m).then_some(m).filter(|_| {
            mouse::reports_to_program(
                crate::toggles::mouse_reporting_on(self.config.mouse_reporting, pane.mouse_reporting_over),
                self.config.mouse_shift_capture,
                pane.session.mouse_shift_capture(),
                shift,
            )
        })
    }

    /// 마우스 사건 하나를 칸 idx의 프로그램에 보고한다(보고를 켜지 않았으면 눌린 단추만 기억한다)
    pub(super) fn report_mouse(
        &mut self,
        idx: usize,
        button: mouse::Button,
        action: mouse::Action,
        input: &PtrIn,
        st: &mut PtrState,
    ) {
        // 눌린 단추를 기억한다(보고하지 않을 때도: 놓음을 놓치면 눌린 채로 남는다)
        if let Some(bit) = mouse::button_bit(button) {
            match action {
                mouse::Action::Press => st.down |= bit,
                mouse::Action::Release => st.down &= !bit,
                mouse::Action::Motion => {}
            }
        }
        let Some(m) = self.mouse_reporting_with(idx, input.mods.shift) else {
            return;
        };
        let Some(event) = mouse::EventMode::from_modes(&m) else {
            return;
        };
        let Some(rect) = self.pane_rect(self.panes[idx].id) else {
            return;
        };
        let Some(cm) = self.metrics_at(self.panes[idx].font_size) else {
            return;
        };
        let (cw, ch) = (cm.width as f64, cm.height as f64);
        let font = self.panes[idx].font_size;
        let inner = self
            .bar_and_inner(self.group_of(self.panes[idx].id).unwrap_or(self.w.active), rect, font)
            .1;
        let (grid_cols, grid_rows) = super::pane_cells(&self.panes[idx].session);
        // 좌표는 칸(창) 왼쪽 위에서 잰 실제 픽셀이다(배율을 곱한 픽셀. Ghostty도 표면 픽셀을 그대로 쓴다).
        // 1016은 거기서 격자 원점(여백을 뺀 자리)까지 뺀 값을 보낸다
        let geometry = mouse::Geometry {
            screen_w: rect.w as f64,
            screen_h: rect.h as f64,
            cell_w: cw,
            cell_h: ch,
            pad_left: (inner.x - rect.x) as f64,
            pad_top: (inner.y - rect.y) as f64,
            cols: grid_cols,
            rows: grid_rows,
        };
        let opts = mouse::Options {
            event,
            format: m.mouse_format,
            geometry,
            any_button_pressed: st.down != 0,
        };
        let pos = (input.pos.x - rect.x as f64, input.pos.y - rect.y as f64);
        let mods = mouse::Mods {
            shift: input.mods.shift,
            alt: input.mods.alt,
            ctrl: input.mods.ctrl,
        };
        let bytes = mouse::encode(button, action, mods, pos, &opts, &mut st.last_cell);
        if !bytes.is_empty() {
            self.panes[idx].session.write(&bytes);
        }
    }

    /// 마우스가 움직였다(칸 idx가 받는다). 고르는 중이면 고르기를 늘리고, 아니면 프로그램이 마우스 보고를 켰을 때 움직임을 보고한다.
    /// `over`는 마우스가 그 칸 위에 있는가(칸 밖 움직임은 보고하지 않는다)
    pub(super) fn pointer_moved(&mut self, idx: usize, input: &PtrIn, st: &mut PtrState, over: bool) {
        if st.selecting {
            if self.selection_screen_changed(idx, st) {
                return;
            }
            // 끌 때마다 포인터가 칸의 위·아래 밖인지 본다(Ghostty `SelectionGesture.drag`, 373줄). 밖이면 틱마다 한 줄씩 스크롤하며
            // 고르기를 늘린다(`pointer_autoscroll_tick`). 칸 안으로 돌아오면 멈춘다
            let dir = self.autoscroll_dir_at(idx, input.pos);
            st.autoscroll_next = mouse::autoscroll_schedule(dir, st.autoscroll_next, Instant::now());
            st.autoscroll = dir;
            self.drag_selection_to(idx, input, st);
        } else if self.mouse_reporting_with(idx, input.mods.shift).is_some() {
            // 움직임 보고는 고른 칸 위에서만. 단추를 누른 채면 그 단추로 보낸다(1002는 이것만 받는다).
            // 어떤 움직임을 보낼지는 이벤트 모드가 정한다(mouse::encode)
            if over {
                let held = mouse::held_button(st.down);
                self.report_mouse(idx, held, mouse::Action::Motion, input, st);
            }
        }
    }

    /// 끌어 고르는 중 포인터 자리까지 고르기를 늘린다(움직임과 자동 스크롤 틱이 함께 쓴다)
    fn drag_selection_to(&mut self, idx: usize, input: &PtrIn, st: &mut PtrState) {
        // 끌 때마다 그 순간의 수식 키로 사각 여부를 정한다(Ghostty Surface.zig 4838줄)
        let rectangle = input.rectangle_select();
        let Some((c, r)) = self.cell_in(idx, input.pos) else {
            return;
        };
        match mouse::link_drag(st.link_word, (c, r)) {
            mouse::LinkDrag::Keep => return,
            mouse::LinkDrag::ToWord => {
                if let Some((lc, lr)) = st.link_word.take() {
                    self.panes[idx]
                        .session
                        .select_start_at(lc, lr, bt_term::SelectKind::Word, bt_term::CellSide::Left);
                }
            }
            mouse::LinkDrag::Normal => {}
        }
        self.panes[idx].session.select_set_block(rectangle);
        let side = self.cell_side_in(idx, input.pos);
        self.panes[idx].session.select_update_at(c, r, side);
        self.request_redraw();
    }

    /// 누른 뒤 대체 화면으로 들어가거나 나왔는가. 그러면 누른 자리는 다른 화면의 것이다(Ghostty는 누른 자리가 지금 화면의 것이 아니면
    /// 끌기를 하지 않고, 자동 스크롤 틱은 끌기를 끝낸다)
    fn selection_screen_changed(&self, idx: usize, st: &PtrState) -> bool {
        self.panes
            .get(idx)
            .is_some_and(|p| p.session.modes().alt_screen != st.press_alt)
    }

    /// 칸 idx 위에서 포인터(창 좌표) 자리가 부를 자동 스크롤 방향. 칸(여백 포함) 위 끝에서 1픽셀 안이거나 그 위면 위, 아래 끝에서 1픽셀 안이거나 그 아래면 아래
    fn autoscroll_dir_at(&self, idx: usize, pos: PhysicalPosition<f64>) -> mouse::Autoscroll {
        let Some(rect) = self.panes.get(idx).and_then(|p| self.pane_rect(p.id)) else {
            return mouse::Autoscroll::None;
        };
        mouse::autoscroll_dir(pos.y - rect.y as f64, rect.h as f64)
    }

    /// 자동 스크롤 틱(15ms마다, Ghostty `Surface.selectionScrollTick`·`SelectionGesture.autoscrollTick`): 틱마다 정확히 한 줄 스크롤하고
    /// 그 자리의 칸으로 고르기를 늘린다. 마지막 포인터 자리를 쓴다(포인터가 멈춰 있어도 계속된다).
    /// 고르는 중이 아니거나 방향이 없으면 멈춘다. 누른 뒤 화면이 바뀌었으면 끌기를 끝낸다. 마우스 보고 중이어도 방향이 남아 있으면 계속한다(Ghostty처럼)
    pub(super) fn pointer_autoscroll_tick(&mut self, idx: usize, input: &PtrIn, st: &mut PtrState) {
        if !st.selecting || st.autoscroll == mouse::Autoscroll::None {
            st.autoscroll = mouse::Autoscroll::None;
            st.autoscroll_next = None;
            return;
        }
        if self.selection_screen_changed(idx, st) {
            st.selecting = false;
            st.last_click = None;
            st.link_word = None;
            st.autoscroll = mouse::Autoscroll::None;
            st.autoscroll_next = None;
            return;
        }
        st.autoscroll_next = Some(mouse::autoscroll_after_tick(st.autoscroll_next, Instant::now()));
        self.panes[idx].session.scroll(mouse::autoscroll_lines(st.autoscroll));
        self.drag_selection_to(idx, input, st);
    }

    /// 트랙패드를 강하게 눌렀다(macOS force click, Ghostty `Surface.mousePressureCallback` 4614줄·`SelectionGesture.deepPress`).
    /// 왼쪽 단추를 누른 채일 때(고르는 중) 처음 누른 칸의 낱말을 고르고 끌기를 끝낸다: 누름 횟수 기록을 지우고 자동 스크롤을 멈추며
    /// 이어 움직여도 더는 끌어 고르지 않는다. 낱말 고르기는 두 번 누르기와 같은 함수를 쓴다
    pub(super) fn pointer_deep_press(&mut self, idx: usize, st: &mut PtrState) {
        if !st.selecting || self.selection_screen_changed(idx, st) {
            return;
        }
        let Some(click) = st.last_click else {
            return;
        };
        let session = &self.panes[idx].session;
        session.select_start(click.col, click.row, bt_term::SelectKind::Word);
        session.select_update(click.col, click.row);
        st.selecting = false;
        st.last_click = None;
        st.link_word = None;
        st.autoscroll = mouse::Autoscroll::None;
        st.autoscroll_next = None;
        self.request_redraw();
    }

    /// 링크 수식 키(macOS ⌘, 그 밖에는 Ctrl) + 왼쪽 단추(#705, `mouse::link_step`). 참이면 삼킨다(평소 처리를 하지 않는다).
    /// 마우스 보고가 꺼져 있으면 누름은 고르기로 가고, 같은 칸에서 떼면 연다(연속 누르기의 첫 번째만). 보고 중이면 누를 때 열고 누름·뗌을 삼킨다.
    /// `clicks`는 이 단추 사건 앞의 마지막 누름 횟수(없으면 0)
    pub(super) fn link_click_step(
        &mut self,
        button: mouse::Button,
        pressed: bool,
        input: &PtrIn,
        link: &mut mouse::LinkPress,
        clicks: u8,
    ) -> bool {
        if button != mouse::Button::Left {
            return false;
        }
        let hit = if input.link_mod() {
            self.link_under(input.pos)
        } else {
            None
        };
        let place = hit.as_ref().and_then(|(id, _)| {
            let i = self.pane_index(*id)?;
            let (c, r) = self.cell_in(i, input.pos)?;
            Some((i, (*id, c, r)))
        });
        let reporting = place.is_some_and(|(i, _)| self.mouse_reporting_with(i, input.mods.shift).is_some());
        // 한 칸 안에서 끌어도 그 칸이 골라졌으면 끈 것이다: 링크를 열지 않는다(Ghostty `left_click_dragged`, 시험 1646)
        let dragged = !pressed && place.is_some_and(|(i, (_, c, r))| self.panes[i].session.selection_contains(c, r));
        let step = mouse::link_step(link, pressed, place.map(|(_, at)| at), reporting, clicks, dragged);
        if matches!(step, mouse::LinkStep::OpenPass | mouse::LinkStep::OpenSwallow) {
            if let Some((id, h)) = &hit {
                self.open_link(&h.url, h.explicit, *id);
            }
        }
        matches!(step, mouse::LinkStep::Swallow | mouse::LinkStep::OpenSwallow)
    }

    /// 마우스 단추 하나(칸 idx가 받는다). 마우스 보고 중이면 프로그램에 보고하고, 아니면 오른쪽 단추 동작·메뉴·가운데 붙여넣기·왼쪽 고르기를 한다.
    /// 창 장식(제목 막대·탭 줄·손잡이·막대 띠)과 초점 누름 삼키기는 사람 쪽이 앞서 걸러 내므로 여기에 없다
    pub(super) fn pointer_button(
        &mut self,
        idx: usize,
        mb: mouse::Button,
        pressed: bool,
        input: &PtrIn,
        st: &mut PtrState,
    ) {
        // macOS Ctrl+왼쪽 클릭은 칸의 프로그램이 마우스 모드를 켜지 않았으면 메뉴다(Ghostty `menu(for:)`, #725). AppKit이 마우스
        // 사건보다 먼저 묻는 자리라 보고 판단보다 앞서고, 켰으면(Shift·읽기 전용·`mouse-reporting`과 상관없이) 보통 왼쪽 클릭이다.
        // 스크립트에는 메뉴를 띄우지 않는다(모달이라 스크립트가 막힌다)
        let program_mouse_mode = self
            .panes
            .get(idx)
            .is_some_and(|p| Self::mouse_mode_on(p.session.modes()));
        if mb == mouse::Button::Left
            && crate::context_menu::ctrl_click_opens_menu(
                cfg!(target_os = "macos"),
                input.mods.ctrl,
                program_mouse_mode,
            )
        {
            if pressed && !input.script {
                self.ctrl_click_menu();
            }
            return;
        }
        if self.mouse_reporting_with(idx, input.mods.shift).is_some() {
            let action = if pressed {
                mouse::Action::Press
            } else {
                mouse::Action::Release
            };
            self.report_mouse(idx, mb, action, input, st);
            return;
        }
        if mb == mouse::Button::Right {
            if pressed {
                self.right_click(input.script);
            }
            return;
        }
        if mb == mouse::Button::Middle {
            // 가운데 단추 붙여넣기는 클립보드를 읽는다: 스크립트에는 하지 않는다
            if pressed && !input.script && mouse::middle_click_pastes(self.config.middle_click_action) {
                let text = self.clipboard.as_mut().and_then(|cb| cb.get_text().ok());
                if let Some(text) = text.filter(|t| !t.is_empty()) {
                    self.paste(text);
                }
            }
            return;
        }
        if mb != mouse::Button::Left {
            return;
        }
        if pressed {
            let Some((c, r)) = self.cell_in(idx, input.pos) else {
                return;
            };
            let now = Instant::now();
            let interval = mouse::repeat_interval(self.config.click_repeat_interval, mouse::platform_repeat_interval());
            // 이어지는 누름인지는 처음 누른 자리에서 픽셀 거리가 칸 너비 이하인지로 본다(Ghostty `Surface.zig` 4129줄 `max_distance`)
            let cell_w = self
                .metrics_at(self.panes[idx].font_size)
                .map_or(0.0, |m| m.width as f64);
            let click = next_click(st.last_click, now, (c, r), (input.pos.x, input.pos.y), cell_w, interval);
            let clicks = click.count;
            st.last_click = Some(click);
            st.link_word = None;
            st.autoscroll = mouse::Autoscroll::None;
            st.autoscroll_next = None;
            st.press_alt = self.panes[idx].session.modes().alt_screen;
            // ⌘(macOS)·Ctrl+세 번 누르기는 줄 대신 그 명령의 출력 구간(#700, Ghostty `SelectionGesture`의 `.output`).
            // 출력이 아닌 칸(프롬프트·입력)이면 아무것도 새로 고르지 않는다(Ghostty도 앞서 고른 것을 그대로 둔다)
            if clicks == 3
                && mouse::triple_click_selects_output(cfg!(target_os = "macos"), input.mods.ctrl, input.mods.sup)
            {
                if self.panes[idx].session.select_output_at(c, r) {
                    st.selecting = true;
                    self.request_redraw();
                }
                return;
            }
            // 두 번 누르기가 화면 글의 URL 위면 낱말 대신 링크 전체(#701, Ghostty `Surface.zig` 4143~4156줄 `linkAtPin(pin, null)`).
            // Ghostty는 이때 수식 키를 보지 않고 `link` 규칙(URL)만 본다. OSC 8 링크는 보지 않는다
            if clicks == 2 {
                if let Some(((r0, c0), (r1, c1))) = self.double_click_link(idx, c, r) {
                    let session = &self.panes[idx].session;
                    session.select_start(c0, r0, bt_term::SelectKind::Simple);
                    session.select_update(c1, r1);
                    st.selecting = true;
                    st.link_word = Some((c, r));
                    self.request_redraw();
                    return;
                }
            }
            let kind = mouse::select_kind(clicks, input.rectangle_select());
            let side = self.cell_side_in(idx, input.pos);
            let session = &self.panes[idx].session;
            if clicks == 1 {
                session.select_clear();
            }
            session.select_start_at(c, r, kind, side);
            if clicks > 1 {
                session.select_update(c, r);
            }
            st.selecting = true;
            self.request_redraw();
        } else {
            st.selecting = false;
            st.autoscroll = mouse::Autoscroll::None;
            st.autoscroll_next = None;
            // 프롬프트 줄을 끌지 않고 한 번 누르면 그 자리로 커서를 옮긴다(cursor-click-to-move, #137). 수식 키는 보지 않고
            // 고른 글이 남았으면 옮기지 않는다(Ghostty maybePromptClick, #729)
            let released = self.cell_in(idx, input.pos);
            let single_same_cell = matches!(
                (st.last_click, released),
                (Some(Click { col: c, row: r, count: 1, .. }), Some((rc, rr))) if (c, r) == (rc, rr)
            );
            let selection_left = self.panes[idx].session.selection_text().is_some();
            if mouse::prompt_click_moves(single_same_cell, selection_left) && self.config.cursor_click_to_move {
                if let Some((col, row)) = released {
                    self.click_to_move(idx, col, row);
                }
            }
            // copy-on-select: 고르기를 마치면 값에 따라 선택 클립보드·시스템 클립보드로
            self.copy_on_select_now();
        }
    }

    /// 두 번 누른 칸의 URL 범위((줄, 첫 칸), (줄, 끝 칸)). `link-url = false`면 없다(Ghostty는 `link` 규칙이 없으면 낱말 고르기)
    fn double_click_link(&self, idx: usize, c: usize, r: usize) -> Option<((usize, usize), (usize, usize))> {
        let opts = mouse::double_click_link_options(self.config.link_url)?;
        let hit = self.panes[idx].session.link_probe(c, r, opts).hit?;
        crate::context_menu::link_ends(&hit.spans)
    }

    /// 마우스 휠. `idx`는 휠이 향하는 칸(없으면 없다), `lines`·`xlines`는 이미 배율을 곱한 세로·가로 줄 수(양수가 위·왼쪽)다.
    /// `pixels`는 트랙패드가 보낸 세로 픽셀(배율을 곱한 값, 양수가 위, 눈금 휠이면 없다): 픽셀 스크롤(`pixel-scroll`, #902)이 켜져 있고 스크롤백을 굴리는 칸이면 줄로 바꾸지 않고 그대로 더한다.
    /// 마우스 보고 중이면 단추 4·5(세로)·6·7(가로)로 보고하고, 대체 화면이면 화살표로, 아니면 스크롤백을 굴린다. 한 번에 최대 10번 보낸다
    pub(super) fn pointer_wheel(
        &mut self,
        idx: Option<usize>,
        lines: f64,
        xlines: f64,
        pixels: Option<f64>,
        input: &PtrIn,
        st: &mut PtrState,
    ) {
        // 가로 휠(트랙패드 옆으로 밀기, 기울이는 휠): 마우스 보고 중일 때만 단추 6·7로 보낸다(#300).
        // 양수가 왼쪽이다(Ghostty `up_right` → `six`)
        let xpane = idx.filter(|&i| self.mouse_reporting_with(i, input.mods.shift).is_some());
        if let Some(xi) = xpane {
            st.accum_x += xlines;
            let xwhole = st.accum_x.trunc() as i32;
            if xwhole != 0 {
                st.accum_x -= xwhole as f64;
                let b = if xwhole > 0 {
                    mouse::Button::WheelLeft
                } else {
                    mouse::Button::WheelRight
                };
                for _ in 0..xwhole.unsigned_abs().min(10) {
                    self.report_mouse(xi, b, mouse::Action::Press, input, st);
                }
            }
        } else {
            st.accum_x = 0.0;
        }
        // 픽셀 스크롤(#902, kitty `pixel_scroll`): 트랙패드의 픽셀 값을 줄 단위로 모으지 않고 스크롤백에 바로 더한다.
        // 마우스 보고 중이거나 대체 화면이면 지금처럼 줄 단위로 간다
        if let (Some(px), Some(i)) = (pixels, idx) {
            let alt = self.panes[i].session.modes().alt_screen;
            let reporting = self.mouse_reporting_with(i, input.mods.shift).is_some();
            if mouse::pixel_scroll_applies(self.config.pixel_scroll, true, reporting, alt) {
                st.accum = 0.0;
                if self.panes[i].session.scroll_pixels(px) {
                    self.request_redraw();
                }
                return;
            }
        }
        st.accum += lines;
        let whole = st.accum.trunc() as i32;
        if whole == 0 {
            return;
        }
        st.accum -= whole as f64;
        let Some(idx) = idx else { return };
        let modes = self.panes[idx].session.modes();
        if self.mouse_reporting_with(idx, input.mods.shift).is_some() {
            let b = if whole > 0 {
                mouse::Button::WheelUp
            } else {
                mouse::Button::WheelDown
            };
            for _ in 0..whole.unsigned_abs().min(10) {
                self.report_mouse(idx, b, mouse::Action::Press, input, st);
            }
        } else if modes.alt_screen && modes.alternate_scroll && !self.panes[idx].session.readonly() {
            // 대체 화면(less·vim)에서는 휠을 위·아래 화살표로 보낸다
            let key = if whole > 0 {
                bt_term::NamedKey::Up
            } else {
                bt_term::NamedKey::Down
            };
            let one = bt_term::encode_named(key, bt_term::Mods::default(), modes.app_cursor);
            let bytes = one.repeat(whole.unsigned_abs().min(10) as usize);
            self.panes[idx].session.write(&bytes);
        } else {
            self.panes[idx].session.scroll(whole);
            self.request_redraw();
        }
    }
}
