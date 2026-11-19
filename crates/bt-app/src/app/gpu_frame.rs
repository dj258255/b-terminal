//! GPU 길 그리기(ADR-036. macOS 기본, `BT_GPU=0`이면 CPU 길, ADR-039).
//!
//! CPU가 "아래층"(창 바탕, 제목 막대, 탭 막대, 칸 바탕, 여백)과 "위층"(kitty 그림, 흐림, 검색 막대, 크기 표시, 스크롤바,
//! 진행 막대, 칸 나눔 선, 링크 띠, 팔레트, 알림 띠, 자물쇠 표시, 제목 입력, 확인 막대)을 지금 `Canvas`로 한 장씩 그리고,
//! GPU가 아래층 → 칸마다 격자 → 위층 순서로 창 surface에 합친다. 층은 격자 밖 모양이 바뀐 프레임(통째)에만 다시 그려 올리고,
//! 바뀐 줄만 바뀐 프레임은 층을 그대로 둔 채 격자만 다시 그린다. 통째 프레임에서도 GPU 쪽이 앞서 올린 층과 견줘 달라진
//! 사각형만 올린다(ADR-037).
//!
//! 위층을 그리는 호출은 CPU 길(`redraw`)과 같은 순서의 같은 함수들이다. 그쪽에 덧그림이 더해지면 여기도 맞춘다

use super::gpu_pace::{self, FramePlan, LayerInputs, ViewLayer};
use super::*;

/// 명령 팔레트: 친 글, 보일 줄들(이름, 단축키), 고른 줄
type PaletteState = (String, Vec<(String, String, String)>, Option<usize>);

/// `redraw`가 모아 둔, 이번 프레임을 그리는 데 필요한 것
pub(super) struct GpuFrame<'a> {
    /// 입력 → 화면 지연(#417): 이 프레임에 출력이 든 칸의 값
    pub latency: super::input_latency::Samples,
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
    /// 칸마다 끌기 손잡이를 어떻게 보이는가(`drag-handle`)
    pub handles: &'a [crate::pane_drag::Look],
    /// 끌어다 놓을 자리 미리 보기: 놓일 칸의 바깥 사각형과 쪽
    pub drop_preview: Option<(Rect, crate::split::Direction)>,
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
    /// `redraw`의 맨 앞에서 부른다. 참이면 이번 그리기는 하지 않는다(ADR-038).
    /// 가려진 창은 아무것도 그리지 않고 앞 프레임 기억을 지워, 보이게 되면 층을 통째로 다시 그리게 한다.
    /// 그림판 얻기가 막힐 것 같으면 아무것도 쓰지 않고 `pace_wake`에 다시 그릴 때를 남긴다(`about_to_wait`가 깨운다).
    /// 미룬 프레임은 상태를 건드리지 않았으므로 깨어나 그릴 때 그때의 최신 내용이 올라간다
    pub(super) fn gpu_frame_gate(&mut self) -> bool {
        let now = Instant::now();
        let occluded = self.w.occluded;
        let Some(gfx) = self.w.gfx.as_mut() else { return false };
        let Some(gpu) = gfx.gpu.as_mut() else { return false };
        let millihertz = gfx.window.current_monitor().and_then(|m| m.refresh_rate_millihertz());
        gpu.set_refresh_interval(bt_gpu::pacing::interval_from_millihertz(millihertz));
        let plan = gpu_pace::plan_frame(occluded, gpu.pace_check(now));
        let waiting = self.w.pace_wake.take().is_some();
        match plan {
            FramePlan::Draw => false,
            FramePlan::SkipOccluded => {
                self.w.last_frame = None;
                self.w.layer_key = None;
                self.w.pace_stats.occluded += 1;
                true
            }
            FramePlan::Defer(wait) => {
                self.w.pace_wake = Some(now + wait);
                if !waiting {
                    self.w.pace_stats.deferred += 1;
                }
                true
            }
        }
    }

    /// 미뤄 둔 프레임을 다시 그릴 때가 된 창을 깨운다
    pub(super) fn fire_pace_wakes(&mut self, now: Instant) {
        for w in std::iter::once(&mut self.w).chain(self.others.iter_mut()) {
            if w.pace_wake.is_some_and(|t| t <= now) {
                w.pace_wake = None;
                if let Some(g) = &w.gfx {
                    g.window.request_redraw();
                }
            }
        }
    }

    pub(super) fn redraw_gpu(&mut self, f: GpuFrame<'_>) {
        let started = Instant::now();
        let theme = self.config.theme.clone();
        let scale = self.scale;
        let Some(gfx) = self.w.gfx.as_mut() else { return };
        let Some(mut fonts) = self.w.fonts.parts_mut() else {
            return;
        };
        let Some(gpu) = gfx.gpu.as_mut() else { return };
        let size = gfx.window.inner_size();
        let (sw, sh) = (size.width, size.height);
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
            handles,
            drop_preview,
            dividers,
            divider_color,
            dim_alpha,
            dim_fill,
            split,
            single_tab,
            top_inset,
            title,
            proxy,
            latency,
        } = f;

        // 투명 창·배경 그림(#359): CPU 길(`redraw`)과 같은 값. 투명으로 열지 않은 창은 설정이 나중에 바뀌어도 불투명으로 그린다
        // (surface의 알파 모드는 열 때 정해진다)
        let opacity = if gpu.window_transparent() {
            crate::toggles::effective_opacity(self.config.background_opacity, self.w.opaque)
        } else {
            1.0
        };
        let win_bg = views
            .iter()
            .find(|v| v.focused && v.screen.modes.reverse_colors)
            .map_or(theme.background, |v| v.screen.background);

        // 층 입력이 앞 프레임과 같으면(진행 막대 값이 그대로 등) 덧그림이 켜져 있어도 층은 그대로다(ADR-038).
        // 키를 헤아릴 수 없는 덧그림(알림 띠·검색 막대 등)이 있으면 키가 없고 늘 통째로 그린다
        let key = gpu_pace::layer_key(&LayerInputs {
            opacity,
            win_bg,
            chrome_bg: views.iter().find(|v| v.focused).map(|v| v.screen.chrome_background),
            theme: &theme,
            top_inset,
            scale,
            single_tab,
            views: &views
                .iter()
                .zip(bars)
                .map(|(v, bar)| ViewLayer {
                    bar_shown: bar.0,
                    bar_hover: bar.1,
                    history: v.screen.history,
                    display_offset: v.screen.display_offset,
                    progress: v.progress,
                    bell_border: v.bell_border,
                })
                .collect::<Vec<_>>(),
            toast: toast.is_some(),
            secure_badge,
            hover_hint: hover.as_ref().is_some_and(|(_, h)| link_previews.shows(h.explicit)),
            search: search.is_some(),
            palette: palette.is_some(),
            title_prompt: title_prompt.is_some(),
            confirm: confirm.is_some(),
            resize_overlay: !resize_overlays.is_empty(),
            images: views.iter().any(|v| !v.screen.images.is_empty()),
        });
        let structural = self.w.last_frame.as_ref().is_none_or(|l| l.chrome != chrome) || self.w.frame.len() != n;
        let reuse = !structural && chrome.overlay && key.is_some() && key == self.w.layer_key;
        let full = structural || (chrome.overlay && !reuse);
        if full {
            self.w.layer_key = key;
        } else if reuse {
            self.w.pace_stats.compare_skipped += 1;
        }

        let mut repainted_rows = 0;
        // 칸마다 바뀐 것이 있는가(셰이더가 칸마다 도니 칸마다 알아야 한다, #441). 통째 프레임은 모든 칸이 바뀐 것이다
        let mut pane_dirty = vec![full; views.len()];
        // 셰이더가 돌면 칸 안 화면(격자와 kitty 그림)만 셰이더가 덮고 나머지 덧그림은 셰이더 뒤에 얹는다(Ghostty는 탭 막대·팔레트·
        // 칸 사이 틈·스크롤 막대 등이 렌더러 밖의 OS 화면이다). 그래서 그것들을 위층이 아닌 맨 위층에 그린다
        let split_layers = chrome.shader_layers;
        if full {
            if self.w.frame.len() != n {
                self.w.frame = vec![0; n];
            }
            // 아래층: 격자 밑에 깔리는 것. 투명 창이면 곱한 알파, 배경 그림이 있으면 그 층을 깐다(CPU 길과 같은 캔버스 설정)
            super::backdrop::refresh(
                &mut self.w.backdrop,
                self.bg_image.as_deref(),
                &self.config,
                (sw, sh),
                win_bg,
                opacity,
            );
            let mut lower = Canvas::new(&mut self.w.frame, sw, sh);
            if let Some((_, layer)) = &self.w.backdrop {
                lower.set_backdrop(win_bg, layer);
            }
            if opacity < 1.0 {
                lower.set_see_through(win_bg, (opacity * 255.0).round() as u8);
            }
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
                    fonts.chrome(),
                    self.w.title_fonts.as_mut(),
                    &theme,
                    labels,
                    top_inset as i32,
                );
            }
            for (v, pad) in views.iter().zip(pads) {
                let screen = &v.screen;
                let vm = v.metrics;
                lower.fill_rect(v.outer.x, v.outer.y, v.outer.w, v.outer.h, screen.background);
                if let Some(p) = pad {
                    let grid_rect = Rect::new(
                        v.inner.x,
                        v.inner.y,
                        screen.cols as u32 * vm.width,
                        screen.rows as u32 * vm.height,
                    );
                    crate::padding::paint(
                        &mut lower,
                        v.outer,
                        grid_rect,
                        vm.width,
                        vm.height,
                        p,
                        screen.background,
                        None,
                    );
                }
                // kitty 그림 가운데 칸 바탕 아래 층(z < i32::MIN / 2): 격자 칸 바탕 아래라 이 층에 깐다. 기본 바탕 칸은 GPU가 그리지 않아 그림이 비친다(#547)
                let images = super::kitty_images::PaneImages::new(
                    &screen.images,
                    (v.inner.x, v.inner.y),
                    (vm.width, vm.height),
                    (screen.cols, screen.rows),
                );
                images.paint(&mut lower, bt_core::kitty_geom::Layer::BelowBackground);
            }

            // 위층: 투명 바탕(0)에 그린다. 캔버스의 섞기가 곱한 알파로 쌓아 준다.
            // 위층에 그리는 것은 덧그림(`chrome.overlay`)과, 화면을 나눴을 때의 흐린 칸·경계선뿐이다.
            // 둘 다 없으면 그리지 않고 버퍼를 놓는다. 빈 조각을 받은 렌더러도 위층 텍스처와 사본을 놓는다(#387).
            // debug 빌드는 그래도 그려 보고 정말 비었는지 본다(판단에 빠진 그리기를 잡는다).
            // 셰이더가 돌면(`split_layers`) 위층에는 kitty 그림만 그리고 나머지는 맨 위층(`top`)에 그린다
            let uses_upper = chrome.overlay || split;
            let uses_images = views.iter().any(|v| !v.screen.images.is_empty());
            let plan = layer_plan(split_layers, uses_upper, uses_images, cfg!(debug_assertions));
            let (upper_used, top_used) = (plan.keep_upper, plan.keep_top);
            if plan.alloc_upper {
                // 지난 프레임에 놓았으면 새로 잡는다(새로 잡은 것은 이미 0)
                if self.w.upper.len() == n {
                    self.w.upper.fill(0);
                } else {
                    self.w.upper = vec![0; n];
                }
            }
            if plan.alloc_top {
                if self.w.top.len() == n {
                    self.w.top.fill(0);
                } else {
                    self.w.top = vec![0; n];
                }
            } else {
                self.w.top = Vec::new();
            }
            // 가운데 층: 칸 바탕과 글자 사이(kitty z가 -2^30 이상 0 미만인 그림, #547). 없으면 비워 두어 GPU가 층을 놓게 한다
            let uses_mid = views.iter().any(|v| {
                v.screen
                    .images
                    .iter()
                    .any(|im| bt_core::kitty_geom::layer_of_z(im.z) == bt_core::kitty_geom::Layer::BelowText)
            });
            if uses_mid {
                if self.w.mid.len() == n {
                    self.w.mid.fill(0);
                } else {
                    self.w.mid = vec![0; n];
                }
                let mut mid = Canvas::new(&mut self.w.mid, sw, sh);
                for v in views {
                    super::kitty_images::PaneImages::new(
                        &v.screen.images,
                        (v.inner.x, v.inner.y),
                        (v.metrics.width, v.metrics.height),
                        (v.screen.cols, v.screen.rows),
                    )
                    .paint(&mut mid, bt_core::kitty_geom::Layer::BelowText);
                }
            } else {
                self.w.mid = Vec::new();
            }
            if plan.draw {
                let mut upper = Canvas::new(&mut self.w.upper, sw, sh);
                let mut top_canvas = split_layers.then(|| Canvas::new(&mut self.w.top, sw, sh));
                // 칸 안 화면이 아닌 덧그림이 가는 캔버스: 셰이더가 돌면 맨 위층, 아니면 위층
                macro_rules! ui {
                    () => {
                        match top_canvas.as_mut() {
                            Some(t) => t,
                            None => &mut upper,
                        }
                    };
                }
                for ((v, (_, _, _)), bar) in views.iter().zip(&drawn).zip(bars) {
                    let screen = &v.screen;
                    let vm = v.metrics;
                    // kitty 그림 가운데 글자 위 층(z >= 0). 칸 바탕 아래·글자 아래 층은 아래층·가운데 층에 있다(#547)
                    super::kitty_images::PaneImages::new(
                        &screen.images,
                        (v.inner.x, v.inner.y),
                        (vm.width, vm.height),
                        (screen.cols, screen.rows),
                    )
                    .paint(&mut upper, bt_core::kitty_geom::Layer::AboveText);
                    if split && !v.focused && dim_alpha > 0 {
                        ui!().blend_rect(v.outer.x, v.outer.y, v.outer.w, v.outer.h, dim_fill, dim_alpha);
                    }
                    if let Some((_, query, status, missing)) = search.as_ref().filter(|s| s.0 == v.id) {
                        tabbar::draw_search_bar(
                            ui!(),
                            fonts.chrome(),
                            &theme,
                            v.outer,
                            &format!("{query}{preedit}"),
                            status,
                            *missing,
                        );
                    }
                    if resize_overlays.contains(&v.id) {
                        tabbar::draw_resize_overlay(
                            ui!(),
                            fonts.chrome(),
                            &theme,
                            v.outer,
                            self.config.resize_overlay_position,
                            &format!("{} × {}", screen.cols, screen.rows),
                        );
                    }
                    if bar.0 {
                        let width = (bt_render::scrollbar::BAR_WIDTH as f64 * scale).round() as u32;
                        bt_render::scrollbar::draw(
                            ui!(),
                            v.outer.right() - width as i32 - scale.round().max(1.0) as i32,
                            v.inner.y,
                            screen.rows as u32 * vm.height,
                            width,
                            screen.rows,
                            screen.history,
                            screen.display_offset,
                            theme.foreground,
                            if bar.1 { 170 } else { 110 },
                        );
                    }
                    if let (true, Some(p)) = (single_tab, v.progress) {
                        tabbar::draw_progress(ui!(), &theme, v.outer.x, v.outer.y, v.outer.w, p, vm.stroke * 3);
                    }
                }
                for d in dividers {
                    ui!().fill_rect(d.x, d.y, d.w, d.h, divider_color);
                }
                for (v, look) in views.iter().zip(handles) {
                    crate::pane_drag::draw_handle(ui!(), v.outer, *look, theme.foreground, scale);
                }
                if let Some((rect, dir)) = drop_preview {
                    crate::pane_drag::draw_drop(ui!(), rect, dir, theme.cursor);
                }
                for v in views.iter().filter(|v| v.bell_border) {
                    crate::bell_border::draw(ui!(), v.outer, scale);
                }
                if let Some((_, hit)) = hover.as_ref().filter(|(_, h)| link_previews.shows(h.explicit)) {
                    tabbar::draw_link_hint(ui!(), fonts.chrome(), &theme, &bt_core::proto::printable(&hit.url));
                }
                if let Some((q, rows, sel)) = palette {
                    tabbar::draw_palette(ui!(), fonts.chrome(), &theme, &format!("{q}{preedit}"), rows, *sel);
                }
                if let Some((text, _)) = toast {
                    tabbar::draw_toast(ui!(), fonts.chrome(), &theme, text);
                }
                if secure_badge {
                    tabbar::draw_secure_badge(ui!(), fonts.chrome(), &theme, badge_top);
                }
                if let Some((label, text)) = title_prompt {
                    tabbar::draw_prompt(
                        ui!(),
                        fonts.chrome(),
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
                    tabbar::draw_confirm(ui!(), fonts.chrome(), &theme, message, &hint);
                }
            }
            if !upper_used {
                debug_assert!(
                    self.w.upper.iter().all(|&p| p == 0),
                    "위층을 쓰지 않는다고 봤는데 그린 것이 있다"
                );
                self.w.upper = Vec::new();
            }
            if split_layers && !top_used {
                debug_assert!(
                    self.w.top.iter().all(|&p| p == 0),
                    "맨 위층을 쓰지 않는다고 봤는데 그린 것이 있다"
                );
                self.w.top = Vec::new();
            }
            repainted_rows = views.iter().map(|v| v.screen.rows).sum();
        } else {
            // 바뀐 줄만 바뀐 프레임: 층은 그대로, 몇 줄이 바뀌었는지만 센다(GPU는 격자를 늘 통째로 그린다)
            for (i, (v, (cells, cursor, pre))) in views.iter().zip(&drawn).enumerate() {
                let prev = self
                    .w
                    .last_frame
                    .as_ref()
                    .and_then(|l| l.panes.iter().find(|p| p.0 == v.id));
                let rows = dirty_rows(v.screen.cols, v.screen.rows, cells, *cursor, pre, prev)
                    .iter()
                    .filter(|d| **d)
                    .count();
                pane_dirty[i] = rows > 0;
                repainted_rows += rows;
            }
        }
        if let Some(v) = views.iter().find(|v| v.focused) {
            // IME 후보 창을 커서 옆에 띄운다
            let vm = v.metrics;
            let px = v.inner.x + (v.screen.cursor.col as u32 * vm.width) as i32;
            let py = v.inner.y + (v.screen.cursor.row as u32 * vm.height) as i32;
            gfx.window
                .set_ime_cursor_area(PhysicalPosition::new(px, py), PhysicalSize::new(vm.width, vm.height));
        }
        let draw_ms = started.elapsed().as_secs_f64() * 1000.0;

        // 바뀐 것이 없으면 창에 올리지 않는다(CPU 길과 같다). 사용자 셰이더가 움직이는 중이면 시간이 흐르므로 늘 올린다(#376)
        let force_present = std::mem::take(&mut self.w.force_present);
        let shaders = gpu.has_custom_shaders();
        let animates =
            super::shader_inputs::shader_animates(shaders, self.config.custom_shader_animation, self.w.focused);
        let skip = !full && repainted_rows == 0 && !force_present && !animates;
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
            // 격자 칸 바탕의 알파도 CPU 길과 같게(#359)
            if opacity < 1.0 {
                gpu.set_see_through(win_bg, (opacity * 255.0).round() as u8);
            } else {
                gpu.clear_see_through();
            }
            gpu.set_cells_alpha(bt_render::cells_alpha(opacity, self.config.background_opacity_cells));
            gpu.set_backdrop(self.w.backdrop.as_ref().map(|_| win_bg));
            // 칸마다 그 칸의 글자 크기의 글꼴 묶음으로 그린다(0번이 창의 기준 크기, #422)
            let panes: Vec<bt_gpu::PaneDraw> = views
                .iter()
                .zip(&grid_views)
                .map(|(v, gv)| bt_gpu::PaneDraw {
                    font: fonts.index_of(v.font_size),
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
            // 셰이더 뒤에 얹는 층(#441): 셰이더가 돌 때만 쓴다. 쓰지 않는데 통째 프레임이면 빈 조각으로 놓는다
            let top = full.then_some(&self.w.top[..]);
            // 칸 바탕과 글자 사이 층(#547): 그림이 없으면 빈 조각으로 놓는다
            let mid = full.then_some(&self.w.mid[..]);
            if shaders {
                // 칸마다 그 칸의 영역·시간·커서·초점으로 셰이더를 돌린다(#441)
                let alive: Vec<u64> = self.panes.iter().map(|p| p.id).collect();
                let regions = shader_regions(
                    &mut self.w.shader,
                    ShaderCtx {
                        theme: &theme,
                        setting: self.config.custom_shader_animation,
                        alive: &alive,
                        window_focused: self.w.focused,
                    },
                    views,
                    &drawn,
                    &pane_dirty,
                );
                gpu.set_shader_regions(&regions);
            }
            outcome = Some(gpu.present_layers(&mut fonts.all(), (sw, sh), layers, top, mid, &panes));
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
        if matches!(outcome, bt_gpu::PresentOutcome::Skipped(_)) {
            // 올리지 못했으니 층이 올라갔는지 믿을 수 없다. 다음 프레임에 층을 통째로 다시 그려 넘긴다
            // (가려진 창에서는 층을 올리기 전에 돌아오므로 올라가지 않았다, ADR-038)
            self.w.last_frame = None;
            self.w.layer_key = None;
            if matches!(outcome, bt_gpu::PresentOutcome::Skipped("창이 가려졌다")) {
                self.w.pace_stats.occluded += 1;
            }
        }
        let stats = self.w.pace_stats;
        let t = gpu.timings();
        log::debug!(
            "다시 그리기: {} {}줄 그리기 {:.2}ms, 올리기까지 {:.2}ms (GPU: 층 올리기 {:.2}({}바이트) 준비 {:.2} 제출·올리기 {:.2} = 얻기 {:.2} 인코딩 {:.2} 제출 {:.2} 올리기 {:.2}, , {:?}, 누적: 미룬 프레임 {} 가려짐 건너뜀 {} 층 견주기 건너뜀 {})",
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
            outcome,
            stats.deferred,
            stats.occluded,
            stats.compare_skipped
        );
        if matches!(outcome, bt_gpu::PresentOutcome::Presented) {
            gfx.first.presented("GPU");
            self.latency_presented(&latency, "GPU");
        }
    }
}

/// `shader_regions`가 칸과 상관없이 받는 값
struct ShaderCtx<'a> {
    theme: &'a bt_core::Theme,
    setting: bt_core::config::CustomShaderAnimation,
    /// 살아 있는 모든 칸의 id(다른 탭에 가려진 칸 포함): 이 밖의 칸 상태는 지운다
    alive: &'a [u64],
    window_focused: bool,
}

/// 사용자 셰이더를 돌릴 칸마다 이번 프레임의 영역과 유니폼 값(#376, 칸마다 #441). 셰이더는 칸마다 돈다: 영역은 칸의 바깥 사각형
/// (여백 포함, Ghostty의 surface 전체)이고, `iCurrentCursor`는 그 칸의 커서(칸 왼쪽 위에서 잰 픽셀), `iFocus`는 그 칸의 초점
/// (창에 초점이 있고 고른 칸), 시간은 그 칸의 것이다. 색은 테마에 칸이 바꾼 바탕색(OSC 11)을 얹는다. OSC 4·10·12로 바뀐 색은
/// VT 엔진이 화면 상태로 내보내지 않아 테마 값이다. 규칙은 `ShaderState::regions`(순수 함수, 시험)에 있다
fn shader_regions(
    state: &mut super::shader_inputs::ShaderState,
    ctx: ShaderCtx<'_>,
    views: &[PaneView],
    drawn: &[(Vec<DrawCell>, Option<DrawCursor>, Option<String>)],
    pane_dirty: &[bool],
) -> Vec<bt_gpu::ShaderRegion> {
    let ShaderCtx {
        theme,
        setting,
        alive,
        window_focused,
    } = ctx;
    use super::shader_inputs as si;
    let inputs: Vec<si::PaneShaderInput> = views
        .iter()
        .zip(drawn)
        .zip(pane_dirty)
        .map(|((v, (_, cursor, _)), dirty)| si::PaneShaderInput {
            id: v.id,
            outer: (v.outer.x, v.outer.y, v.outer.w, v.outer.h),
            inner: (v.inner.x, v.inner.y),
            // 커서 자리는 그 칸의 글자 크기의 칸 크기다(#422)
            metrics: si::CursorMetrics {
                cell_w: v.metrics.width,
                cell_h: v.metrics.height,
                cursor_h: v.metrics.cursor_height,
                beam: v.metrics.cursor_beam,
                stroke: v.metrics.stroke,
            },
            shape: v.screen.cursor.shape,
            visible: v.screen.cursor.visible,
            glyph: *cursor,
            focused: v.focused && window_focused,
            dirty: *dirty,
            background: v.screen.background,
        })
        .collect();
    state.regions(Instant::now(), theme, setting, alive, &inputs)
}

/// 통째 프레임에서 위층·맨 위층 버퍼를 어떻게 다룰지(#441). 셰이더가 돌면(`split_layers`) 위층에는 kitty 그림만,
/// 나머지 덧그림은 맨 위층에 그린다. `draw`이면 두 캔버스를 만들므로 그 버퍼가 화면 크기로 있어야 한다
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LayerPlan {
    /// 그리기 블록에 들어간다(캔버스를 만든다)
    draw: bool,
    /// 위층 버퍼를 화면 크기로 잡는다
    alloc_upper: bool,
    /// 맨 위층 버퍼를 화면 크기로 잡는다
    alloc_top: bool,
    /// 그린 뒤에도 위층을 남긴다(아니면 놓는다)
    keep_upper: bool,
    /// 그린 뒤에도 맨 위층을 남긴다
    keep_top: bool,
}

fn layer_plan(split_layers: bool, uses_upper: bool, uses_images: bool, debug: bool) -> LayerPlan {
    let keep_upper = if split_layers { uses_images } else { uses_upper };
    let keep_top = split_layers && uses_upper;
    let draw = keep_upper || keep_top || debug;
    LayerPlan {
        draw,
        alloc_upper: draw,
        alloc_top: split_layers && draw,
        keep_upper,
        keep_top,
    }
}

#[cfg(test)]
mod layer_plan_tests {
    use super::*;

    /// #441: 캔버스를 만드는 프레임이면 그 버퍼가 있어야 한다(빈 버퍼로 `Canvas::new`를 부르면 패닉한다).
    /// 실제 창에서 셰이더를 건 채 화면을 나누면 위층 버퍼 없이 그리기 블록에 들어가 꺼졌다
    #[test]
    fn every_canvas_made_has_a_buffer() {
        for split_layers in [false, true] {
            for uses_upper in [false, true] {
                for uses_images in [false, true] {
                    for debug in [false, true] {
                        let p = layer_plan(split_layers, uses_upper, uses_images, debug);
                        let case = (split_layers, uses_upper, uses_images, debug);
                        if p.draw {
                            assert!(p.alloc_upper, "위층 캔버스를 만드는데 버퍼가 없다: {case:?}");
                            if split_layers {
                                assert!(p.alloc_top, "맨 위층 캔버스를 만드는데 버퍼가 없다: {case:?}");
                            }
                        }
                        assert!(!p.keep_upper || p.alloc_upper && p.draw, "{case:?}");
                        assert!(!p.keep_top || p.alloc_top && p.draw, "{case:?}");
                    }
                }
            }
        }
    }

    /// 셰이더가 없으면 지금까지와 같다: 덧그림이 있을 때만 위층, 맨 위층은 쓰지 않는다
    #[test]
    fn without_shaders_only_the_upper_layer_is_used() {
        let p = layer_plan(false, true, true, false);
        assert!(p.draw && p.alloc_upper && p.keep_upper && !p.alloc_top && !p.keep_top);
        let p = layer_plan(false, false, true, false);
        assert!(!p.draw && !p.keep_upper && !p.alloc_top);
    }
}
