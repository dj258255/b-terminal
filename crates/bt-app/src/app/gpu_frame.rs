//! GPU 길 그리기(`BT_GPU=1`, ADR-036).
//!
//! CPU가 "아래층"(창 바탕, 제목 막대, 탭 막대, 칸 바탕, 여백)과 "위층"(kitty 그림, 흐림, 검색 막대, 크기 표시, 스크롤바,
//! 진행 막대, 칸 나눔 선, 링크 띠, 팔레트, 알림 띠, 자물쇠 표시, 제목 입력, 확인 막대)을 지금 `Canvas`로 한 장씩 그리고,
//! GPU가 아래층 → 칸마다 격자 → 위층 순서로 창 surface에 합친다. 층은 격자 밖 모양이 바뀐 프레임(통째)에만 다시 그려 올리고,
//! 바뀐 줄만 바뀐 프레임은 층을 그대로 둔 채 격자만 다시 그린다. 통째 프레임에서도 GPU 쪽이 앞서 올린 층과 견줘 달라진
//! 사각형만 올린다(ADR-037).
//!
//! 위층을 그리는 호출은 CPU 길(`redraw`)과 같은 순서의 같은 함수들이다. 그쪽에 덧그림이 더해지면 여기도 맞춘다

use super::*;

/// 명령 팔레트: 친 글, 보일 줄들(이름, 단축키), 고른 줄
type PaletteState = (String, Vec<(String, String)>, usize);

/// `redraw`가 모아 둔, 이번 프레임을 그리는 데 필요한 것
pub(super) struct GpuFrame<'a> {
    pub views: &'a [PaneView],
    pub drawn: Vec<(Vec<DrawCell>, Option<DrawCursor>, Option<String>)>,
    pub pads: &'a [Option<crate::padding::PadPaint>],
    pub bars: &'a [(bool, bool)],
    pub labels: &'a [tabbar::TabLabel],
    pub chrome: Chrome,
    pub toast: &'a Option<(String, Instant)>,
    pub secure_badge: bool,
    pub badge_top: u32,
    pub confirm: &'a Option<(String, &'static str, bool)>,
    pub title_prompt: &'a Option<(&'static str, String)>,
    pub hover: &'a Option<(u64, bt_term::links::LinkHit)>,
    pub link_previews: bt_core::config::LinkPreviews,
    pub palette: &'a Option<PaletteState>,
    pub search: &'a Option<(u64, String, String, bool)>,
    pub preedit: &'a str,
    pub resize_overlays: &'a [u64],
    pub dividers: &'a [Rect],
    pub divider_color: bt_core::Rgb,
    pub dim_alpha: u8,
    pub dim_fill: bt_core::Rgb,
    pub split: bool,
    pub single_tab: bool,
    pub top_inset: u32,
    pub title: &'a str,
    pub proxy: Option<std::path::PathBuf>,
}

impl App {
    pub(super) fn redraw_gpu(&mut self, f: GpuFrame<'_>) {
        let started = Instant::now();
        let theme = self.config.theme.clone();
        let scale = self.scale;
        let Some(gfx) = self.w.gfx.as_mut() else { return };
        let Some(fonts) = self.fonts.as_mut() else { return };
        let Some(gpu) = gfx.gpu.as_mut() else { return };
        let size = gfx.window.inner_size();
        let (sw, sh) = (size.width, size.height);
        let m = fonts.metrics();
        let n = sw as usize * sh as usize;
        let GpuFrame {
            views,
            drawn,
            pads,
            bars,
            labels,
            chrome,
            toast,
            secure_badge,
            badge_top,
            confirm,
            title_prompt,
            hover,
            link_previews,
            palette,
            search,
            preedit,
            resize_overlays,
            dividers,
            divider_color,
            dim_alpha,
            dim_fill,
            split,
            single_tab,
            top_inset,
            title,
            proxy,
        } = f;

        let full = chrome.overlay
            || self.w.last_frame.as_ref().is_none_or(|l| l.chrome != chrome)
            || self.w.frame.len() != n
            || self.w.upper.len() != n;

        let mut repainted_rows = 0;
        if full {
            if self.w.frame.len() != n {
                self.w.frame = vec![0; n];
            }
            if self.w.upper.len() != n {
                self.w.upper = vec![0; n];
            }
            // 아래층: 격자 밑에 깔리는 것. 불투명이다(투명 창·배경 그림은 CPU 길로 돌아가는 설정이라 여기 오지 않는다.
            // 열려 있는 창의 설정이 나중에 바뀌어도 이 창은 불투명으로 그린다)
            let mut lower = Canvas::new(&mut self.w.frame, sw, sh);
            let bg = views
                .iter()
                .find(|v| v.focused)
                .map(|v| v.screen.background)
                .unwrap_or(theme.background);
            lower.clear(bg);
            if let Some(fv) = views.iter().find(|v| v.focused) {
                if let Some((x, y, w, h, c)) = titlebar_fill(
                    top_inset,
                    sw,
                    fv.screen.modes.reverse_colors,
                    fv.screen.chrome_background,
                ) {
                    lower.fill_rect(x, y, w, h, c);
                }
            }
            if !labels.is_empty() {
                tabbar::draw_with_title_font(
                    &mut lower,
                    fonts,
                    self.title_fonts.as_mut(),
                    &theme,
                    labels,
                    top_inset as i32,
                );
            }
            for (v, pad) in views.iter().zip(pads) {
                let screen = &v.screen;
                lower.fill_rect(v.outer.x, v.outer.y, v.outer.w, v.outer.h, screen.background);
                if let Some(p) = pad {
                    let grid_rect = Rect::new(
                        v.inner.x,
                        v.inner.y,
                        screen.cols as u32 * m.width,
                        screen.rows as u32 * m.height,
                    );
                    crate::padding::paint(
                        &mut lower,
                        v.outer,
                        grid_rect,
                        m.width,
                        m.height,
                        p,
                        screen.background,
                        None,
                    );
                }
            }

            // 위층: 투명 바탕(0)에 그린다. 캔버스의 섞기가 곱한 알파로 쌓아 준다
            self.w.upper.fill(0);
            let mut upper = Canvas::new(&mut self.w.upper, sw, sh);
            for ((v, (_, _, _)), bar) in views.iter().zip(&drawn).zip(bars) {
                let screen = &v.screen;
                // kitty 그림: 놓인 칸에, 칸 수에 맞춰 늘리거나 원래 크기로. 격자 밖은 자른다
                let grid_clip = (
                    v.inner.x,
                    v.inner.y,
                    v.inner.x + (screen.cols as u32 * m.width) as i32,
                    v.inner.y + (screen.rows as u32 * m.height) as i32,
                );
                for im in &screen.images {
                    let (cw, chh) = (im.cols * m.width, im.rows * m.height);
                    let (w, h) = if im.scaled { (cw, chh) } else { (im.width, im.height) };
                    let clip = (
                        grid_clip.0,
                        grid_clip.1,
                        grid_clip.2.min(v.inner.x + (im.col as u32 * m.width + cw) as i32),
                        grid_clip.3,
                    );
                    upper.draw_rgba(
                        v.inner.x + (im.col as u32 * m.width) as i32,
                        v.inner.y + im.row * m.height as i32,
                        w,
                        h,
                        &im.rgba,
                        im.width,
                        im.height,
                        clip,
                    );
                }
                if split && !v.focused && dim_alpha > 0 {
                    upper.blend_rect(v.outer.x, v.outer.y, v.outer.w, v.outer.h, dim_fill, dim_alpha);
                }
                if let Some((_, query, status, missing)) = search.as_ref().filter(|s| s.0 == v.id) {
                    tabbar::draw_search_bar(
                        &mut upper,
                        fonts,
                        &theme,
                        v.outer,
                        &format!("{query}{preedit}"),
                        status,
                        *missing,
                    );
                }
                if resize_overlays.contains(&v.id) {
                    tabbar::draw_resize_overlay(
                        &mut upper,
                        fonts,
                        &theme,
                        v.outer,
                        self.config.resize_overlay_position,
                        &format!("{} × {}", screen.cols, screen.rows),
                    );
                }
                if bar.0 {
                    let width = (bt_render::scrollbar::BAR_WIDTH as f64 * scale).round() as u32;
                    bt_render::scrollbar::draw(
                        &mut upper,
                        v.outer.right() - width as i32 - scale.round().max(1.0) as i32,
                        v.inner.y,
                        screen.rows as u32 * m.height,
                        width,
                        screen.rows,
                        screen.history,
                        screen.display_offset,
                        theme.foreground,
                        if bar.1 { 170 } else { 110 },
                    );
                }
                if let (true, Some(p)) = (single_tab, v.progress) {
                    tabbar::draw_progress(&mut upper, &theme, v.outer.x, v.outer.y, v.outer.w, p, m.stroke * 3);
                }
            }
            for d in dividers {
                upper.fill_rect(d.x, d.y, d.w, d.h, divider_color);
            }
            if let Some((_, hit)) = hover.as_ref().filter(|(_, h)| link_previews.shows(h.explicit)) {
                tabbar::draw_link_hint(&mut upper, fonts, &theme, &bt_core::proto::printable(&hit.url));
            }
            if let Some((q, rows, sel)) = palette {
                tabbar::draw_palette(&mut upper, fonts, &theme, &format!("{q}{preedit}"), rows, *sel);
            }
            if let Some((text, _)) = toast {
                tabbar::draw_toast(&mut upper, fonts, &theme, text);
            }
            if secure_badge {
                tabbar::draw_secure_badge(&mut upper, fonts, &theme, badge_top);
            }
            if let Some((label, text)) = title_prompt {
                tabbar::draw_prompt(
                    &mut upper,
                    fonts,
                    &theme,
                    label,
                    &format!("{text}{preedit}"),
                    "↩ 정하기    esc 취소    비우고 ↩면 프로그램이 정한 제목으로",
                );
            }
            if let Some((message, yes, from_program)) = confirm {
                let hint = if *from_program {
                    format!("y {yes}    n·esc 거절")
                } else {
                    format!("↩ {yes}    esc 취소")
                };
                tabbar::draw_confirm(&mut upper, fonts, &theme, message, &hint);
            }
            repainted_rows = views.iter().map(|v| v.screen.rows).sum();
        } else {
            // 바뀐 줄만 바뀐 프레임: 층은 그대로, 몇 줄이 바뀌었는지만 센다(GPU는 격자를 늘 통째로 그린다)
            for (v, (cells, cursor, pre)) in views.iter().zip(&drawn) {
                let prev = self
                    .w
                    .last_frame
                    .as_ref()
                    .and_then(|l| l.panes.iter().find(|p| p.0 == v.id));
                repainted_rows += dirty_rows(v.screen.cols, v.screen.rows, cells, *cursor, pre, prev)
                    .iter()
                    .filter(|d| **d)
                    .count();
            }
        }
        if let Some(v) = views.iter().find(|v| v.focused) {
            // IME 후보 창을 커서 옆에 띄운다
            let px = v.inner.x + (v.screen.cursor.col as u32 * m.width) as i32;
            let py = v.inner.y + (v.screen.cursor.row as u32 * m.height) as i32;
            gfx.window
                .set_ime_cursor_area(PhysicalPosition::new(px, py), PhysicalSize::new(m.width, m.height));
        }
        let draw_ms = started.elapsed().as_secs_f64() * 1000.0;

        // 바뀐 것이 없으면 창에 올리지 않는다(CPU 길과 같다)
        let force_present = std::mem::take(&mut self.w.force_present);
        let skip = !full && repainted_rows == 0 && !force_present;
        let mut outcome = None;
        if !skip {
            let grid_views: Vec<GridView> = views
                .iter()
                .zip(&drawn)
                .map(|(v, (cells, cursor, pre))| GridView {
                    cols: v.screen.cols,
                    rows: v.screen.rows,
                    cells,
                    cursor: *cursor,
                    selection_bg: theme.selection_background,
                    selection_fg: theme.selection_foreground,
                    preedit: pre.as_deref(),
                    min_contrast: self.config.minimum_contrast,
                })
                .collect();
            let panes: Vec<bt_gpu::PaneDraw> = views
                .iter()
                .zip(&grid_views)
                .map(|(v, gv)| bt_gpu::PaneDraw {
                    x: v.inner.x,
                    y: v.inner.y,
                    view: gv,
                    clear: v.screen.background,
                })
                .collect();
            let layers = bt_gpu::LayerUpdate {
                lower: full.then_some(&self.w.frame[..]),
                upper: full.then_some(&self.w.upper[..]),
            };
            outcome = Some(gpu.present(fonts, (sw, sh), layers, &panes));
        }
        let end = Instant::now();
        self.w.last_frame = Some(FrameMemo {
            chrome,
            panes: views
                .iter()
                .zip(drawn)
                .map(|(v, (c, cur, pre))| (v.id, c, cur, pre))
                .collect(),
        });
        gfx.window.set_title(title);
        if self.w.proxy_icon != proxy {
            #[cfg(target_os = "macos")]
            winit::platform::macos::set_represented_path(&gfx.window, proxy.as_deref());
            self.w.proxy_icon = proxy;
        }
        let Some(outcome) = outcome else { return };
        let t = gpu.timings();
        log::debug!(
            "다시 그리기: {} {}줄 그리기 {:.2}ms, 올리기까지 {:.2}ms (GPU: 층 올리기 {:.2}({}바이트) 준비 {:.2} 제출·올리기 {:.2} = 얻기 {:.2} 인코딩 {:.2} 제출 {:.2} 올리기 {:.2}, {:?})",
            if full { "통째" } else { "바뀐 줄" },
            repainted_rows,
            draw_ms,
            (end - started).as_secs_f64() * 1000.0,
            t.upload_ms,
            t.upload_bytes,
            t.prepare_ms,
            t.gpu_ms,
            t.acquire_ms,
            t.encode_ms,
            t.submit_ms,
            t.present_ms,
            outcome
        );
    }
}
