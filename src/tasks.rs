//! Open window selector (Sway / Hyprland / Niri via the Compositor service).
//! Keeps the centered floating card grid while adding French copy, Lucide
//! fallback icons, 2D arrow navigation, type-to-filter, workspace filtering,
//! and quick window actions (focus, close, float toggle, move to workspace).

use std::collections::HashSet;

use gpui::prelude::*;
use gpui::{
    div, px, AnimationExt, ClickEvent, Context, Entity, FocusHandle, FontWeight, KeyDownEvent,
    ScrollHandle, Window,
};
use gpui_component::{ActiveTheme as _, Sizable as _};
use kumizo_kit::prelude::*;

use crate::{
    kit::{
        inset_row_bg, raised_bg, surface_bg, svg_icon, sync_if_needed, SVG_APPS, SVG_CLOSE,
        SVG_SEARCH,
    },
    theme,
    ui::{self, ids},
};

const CARD_W: f32 = 300.0;
const CARD_H: f32 = 112.0;
const GRID_GAP: f32 = 12.0;

#[derive(Clone)]
struct Row {
    id: i64,
    title: String,
    class: String,
    app_name: String,
    workspace_id: i32,
    workspace: String,
    focused: bool,
}

fn friendly_app_name(apps: &Apps, class: &str) -> String {
    if class.is_empty() {
        return "Window".to_string();
    }
    if let Some(app) = apps.apps().iter().find(|a| {
        a.id.eq_ignore_ascii_case(class)
            || a.id
                .rsplit('.')
                .next()
                .is_some_and(|tail| tail.eq_ignore_ascii_case(class))
    }) {
        return app.name.clone();
    }
    if let Some(tail) = class.rsplit('.').next().filter(|s| !s.is_empty() && class.contains('.')) {
        let mut chars = tail.chars();
        if let Some(first) = chars.next() {
            return format!("{}{}", first.to_uppercase(), chars.as_str());
        }
    }
    class.to_string()
}

fn ws_pill(
    id: &'static str,
    label: String,
    active: bool,
    primary: gpui::Hsla,
    primary_fg: gpui::Hsla,
    muted_fg: gpui::Hsla,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .px_2p5()
        .py(px(3.))
        .rounded_full()
        .cursor_pointer()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .bg(if active { primary } else { surface_bg() })
        .text_color(if active { primary_fg } else { muted_fg })
        .hover(|s| s.opacity(0.9))
        .child(label)
}

pub struct TaskSwitcher {
    compositor: Entity<Compositor>,
    apps: Entity<Apps>,
    selected: usize,
    cols: usize,
    query: String,
    workspace_only: bool,
    need_reset: bool,
    last_focused_id: Option<i64>,
    scroll: ScrollHandle,
    last_theme: &'static str,
    focus: FocusHandle,
}

impl TaskSwitcher {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            compositor: cx.service::<Compositor>(),
            apps: cx.service::<Apps>(),
            selected: 0,
            cols: 3,
            query: String::new(),
            workspace_only: false,
            need_reset: true,
            last_focused_id: None,
            scroll: ScrollHandle::new(),
            last_theme: theme::current().name,
            focus: cx.focus_handle(),
        }
    }

    fn all_rows(&self, cx: &Context<Self>) -> (Vec<Row>, i32, String, bool) {
        let comp = self.compositor.read(cx);
        let apps = self.apps.read(cx);
        let active_ws_id = comp.active_workspace_id();
        let active_ws_label = comp
            .active_workspace()
            .map(|w| w.name.clone())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| active_ws_id.to_string());

        let mut rows: Vec<Row> = comp
            .windows()
            .iter()
            .map(|w| {
                let app_name = friendly_app_name(apps, &w.class);
                Row {
                    id: w.id,
                    title: w.title.clone(),
                    class: w.class.clone(),
                    app_name,
                    workspace_id: w.workspace_id,
                    workspace: w.workspace_name.clone(),
                    focused: w.is_focused,
                }
            })
            .collect();

        let workspaces: HashSet<i32> = rows.iter().map(|r| r.workspace_id).collect();
        let multi_ws = workspaces.len() > 1;

        // Current workspace windows first while keeping compositor order inside each group.
        rows.sort_by_key(|r| if r.workspace_id == active_ws_id { 0 } else { 1 });

        (rows, active_ws_id, active_ws_label, multi_ws)
    }

    fn filtered_rows(&self, all: &[Row], active_ws_id: i32) -> Vec<Row> {
        let q = self.query.trim().to_lowercase();
        all.iter()
            .filter(|r| !self.workspace_only || r.workspace_id == active_ws_id)
            .filter(|r| {
                q.is_empty()
                    || r.title.to_lowercase().contains(&q)
                    || r.app_name.to_lowercase().contains(&q)
                    || r.class.to_lowercase().contains(&q)
                    || r.workspace.to_lowercase().contains(&q)
            })
            .cloned()
            .collect()
    }

    fn visible_rows(&self, cx: &Context<Self>) -> Vec<Row> {
        let (all, active_ws_id, _, _) = self.all_rows(cx);
        self.filtered_rows(&all, active_ws_id)
    }

    fn select_default(&mut self, rows: &[Row]) {
        self.selected = rows.iter().position(|r| !r.focused).unwrap_or(0);
        self.scroll.scroll_to_item(self.selected / self.cols.max(1));
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.selected = 0;
        self.query.clear();
        self.workspace_only = false;
        self.need_reset = true;
        self.scroll.scroll_to_item(0);
        cx.hide_window(ids::TASKS);
        cx.notify();
    }

    fn activate(&mut self, i: usize, cx: &mut Context<Self>) {
        let rows = self.visible_rows(cx);
        if let Some(r) = rows.get(i) {
            let id = r.id;
            self.compositor.update(cx, |c, _| c.focus_window(id));
        }
        self.close(cx);
    }

    fn close_window_by_id(&mut self, id: i64, cx: &mut Context<Self>) {
        self.compositor.update(cx, |c, _| c.close_window_id(id));
        cx.notify();
    }

    fn close_selected(&mut self, cx: &mut Context<Self>) {
        let rows = self.visible_rows(cx);
        if let Some(r) = rows.get(self.selected) {
            let id = r.id;
            self.compositor.update(cx, |c, _| c.close_window_id(id));
            if self.selected > 0 && self.selected + 1 >= rows.len() {
                self.selected -= 1;
            }
            cx.notify();
        }
    }

    fn toggle_floating_selected(&mut self, cx: &mut Context<Self>) {
        let rows = self.visible_rows(cx);
        if let Some(r) = rows.get(self.selected) {
            let id = r.id;
            self.compositor
                .update(cx, |c, _| { let _ = c.toggle_floating(Some(id)); });
            cx.notify();
        }
    }

    fn move_selected_here(&mut self, cx: &mut Context<Self>) {
        let rows = self.visible_rows(cx);
        let Some(r) = rows.get(self.selected) else {
            return;
        };
        let id = r.id;
        self.compositor.update(cx, |c, _| {
            if let Some(ws) = c.active_workspace() {
                let addr = ws.address.clone();
                let _ = c.move_window_to_workspace(Some(id), &addr, false);
            }
        });
        cx.notify();
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let n = self.visible_rows(cx).len();
        if n > 0 {
            self.selected = (self.selected as isize + delta).rem_euclid(n as isize) as usize;
            self.scroll.scroll_to_item(self.selected / self.cols.max(1));
            cx.notify();
        }
    }

    fn step_row(&mut self, row_delta: isize, cx: &mut Context<Self>) {
        let n = self.visible_rows(cx).len();
        if n == 0 {
            return;
        }
        let cols = self.cols.max(1) as isize;
        let next = self.selected as isize + row_delta * cols;
        if next >= 0 && (next as usize) < n {
            self.selected = next as usize;
        } else if row_delta > 0 {
            self.selected = n - 1;
        } else {
            self.selected = 0;
        }
        self.scroll.scroll_to_item(self.selected / self.cols.max(1));
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let m = &ev.keystroke.modifiers;
        let key = ui::key(ev);

        if m.control {
            match key {
                "w" => self.close_selected(cx),
                "f" => self.toggle_floating_selected(cx),
                "m" => self.move_selected_here(cx),
                "l" => {
                    self.query.clear();
                    self.selected = 0;
                    cx.notify();
                }
                _ => {}
            }
            return;
        }

        if m.alt {
            if let Some(d) = key.chars().next().and_then(|c| c.to_digit(10)) {
                if (1..=9).contains(&d) {
                    self.activate((d - 1) as usize, cx);
                }
            }
            return;
        }

        match key {
            "escape" => {
                if !self.query.is_empty() {
                    self.query.clear();
                    self.selected = 0;
                    self.scroll.scroll_to_item(0);
                    cx.notify();
                } else {
                    self.close(cx);
                }
            }
            "enter" => self.activate(self.selected, cx),
            "delete" => self.close_selected(cx),
            "backspace" => {
                if !self.query.is_empty() {
                    self.query.pop();
                    self.selected = 0;
                    self.scroll.scroll_to_item(0);
                    cx.notify();
                }
            }
            "left" => self.step(-1, cx),
            "right" => self.step(1, cx),
            "up" => self.step_row(-1, cx),
            "down" => self.step_row(1, cx),
            "tab" => self.step(if m.shift { -1 } else { 1 }, cx),
            _ => {
                if !m.platform {
                    if let Some(ch) = ev.keystroke.key_char.as_ref() {
                        if !ch.chars().any(char::is_control) && self.query.len() < 48 {
                            self.query.push_str(ch);
                            self.selected = 0;
                            self.scroll.scroll_to_item(0);
                            cx.notify();
                        }
                    }
                }
            }
        }
    }
}

impl Render for TaskSwitcher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        sync_if_needed(&mut self.last_theme, cx);

        let just_opened = !self.focus.is_focused(window);
        if just_opened {
            window.focus(&self.focus, cx);
        }

        let vp = window.viewport_size();
        let max_grid_w = (f32::from(vp.width) - 64.0).clamp(CARD_W, 960.0);
        let cols = ((max_grid_w + GRID_GAP) / (CARD_W + GRID_GAP))
            .floor()
            .clamp(1.0, 3.0) as usize;
        self.cols = cols;
        let max_grid_h = (f32::from(vp.height) - 210.0).clamp(160.0, 560.0);

        let (all_rows, active_ws_id, active_ws_label, multi_ws) = self.all_rows(cx);
        if !multi_ws {
            self.workspace_only = false;
        }
        let total_count = all_rows.len();
        let active_ws_count = all_rows
            .iter()
            .filter(|r| r.workspace_id == active_ws_id)
            .count();
        let current_focused_id = all_rows.iter().find(|r| r.focused).map(|r| r.id);

        let rows = self.filtered_rows(&all_rows, active_ws_id);

        if self.need_reset || just_opened || current_focused_id != self.last_focused_id {
            self.need_reset = false;
            self.last_focused_id = current_focused_id;
            self.select_default(&rows);
        }

        if self.selected >= rows.len() {
            self.selected = rows.len().saturating_sub(1);
        }

        let mut grid = div()
            .id("tk-scroll")
            .max_h(px(max_grid_h))
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .flex()
            .flex_col()
            .items_center()
            .gap_3();

        if rows.is_empty() {
            grid = grid.child(
                div()
                    .py_8()
                    .px_10()
                    .rounded(px(ui::RADIUS_PANEL))
                    .bg(surface_bg())
                    .border_1()
                    .border_color(cx.theme().border.opacity(0.6))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(
                        svg_icon(SVG_APPS)
                            .size(px(30.))
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(if total_count == 0 {
                                "No open windows"
                            } else {
                                "No windows match the filter"
                            }),
                    ),
            );
        } else {
            let grid_w = cols as f32 * CARD_W + cols.saturating_sub(1) as f32 * GRID_GAP;
            grid = grid.w(px(grid_w));

            for (row_idx, chunk) in rows.chunks(cols).enumerate() {
                let mut row_el = div()
                    .id(("tk-row", row_idx))
                    .w_full()
                    .flex()
                    .flex_row()
                    .justify_center()
                    .gap_3();

                for (col_idx, r) in chunk.iter().enumerate() {
                    let i = row_idx * cols + col_idx;
                    let selected = i == self.selected;
                    let win_id = r.id;

                    let icon = match self.apps.read(cx).find_icon(&r.class) {
                        Some(img) => gpui::img(img).w(px(36.)).h(px(36.)).into_any_element(),
                        None => div()
                            .w(px(36.))
                            .h(px(36.))
                            .rounded(px(9.))
                            .bg(inset_row_bg())
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                svg_icon(SVG_APPS)
                                    .size(px(19.))
                                    .text_color(cx.theme().muted_foreground),
                            )
                            .into_any_element(),
                    };

                    let title = if r.title.is_empty() {
                        r.app_name.clone()
                    } else {
                        r.title.clone()
                    };

                    let subtitle = if r.workspace.is_empty() {
                        r.app_name.clone()
                    } else {
                        format!("{} · Workspace {}", r.app_name, r.workspace)
                    };

                    row_el = row_el.child(
                        div()
                            .id(("task", i))
                            .w(px(CARD_W))
                            .h(px(CARD_H))
                            .flex()
                            .flex_col()
                            .justify_between()
                            .p_3()
                            .rounded(px(ui::RADIUS_PANEL))
                            .cursor_pointer()
                            .bg(if selected { raised_bg() } else { surface_bg() })
                            .border_1()
                            .border_color(if selected {
                                cx.theme().primary
                            } else {
                                cx.theme().border.opacity(0.65)
                            })
                            .hover(|s| s.bg(raised_bg().opacity(0.85)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                this.activate(i, cx);
                            }))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_start()
                                    .gap_3()
                                    .child(icon)
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .text_sm()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(cx.theme().foreground)
                                            .line_clamp(2)
                                            .child(title),
                                    )
                                    .child(
                                        div()
                                            .id(("tk-close-wrap", i))
                                            .w(px(22.))
                                            .h(px(22.))
                                            .rounded(px(6.))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor_pointer()
                                            .text_color(if selected {
                                                cx.theme().muted_foreground
                                            } else {
                                                cx.theme().muted_foreground.opacity(0.55)
                                            })
                                            .hover(|s| {
                                                s.bg(cx.theme().danger.opacity(0.16))
                                                    .text_color(cx.theme().danger)
                                            })
                                            .on_click(cx.listener(
                                                move |this, _: &ClickEvent, _, cx| {
                                                    cx.stop_propagation();
                                                    this.close_window_by_id(win_id, cx);
                                                },
                                            ))
                                            .child(svg_icon(SVG_CLOSE).xsmall()),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(div().flex_1().min_w_0().truncate().child(subtitle))
                                    .children(r.focused.then(|| {
                                        div()
                                            .px_2()
                                            .py(px(1.))
                                            .rounded_full()
                                            .bg(cx.theme().primary)
                                            .text_color(cx.theme().primary_foreground)
                                            .font_weight(FontWeight::MEDIUM)
                                            .child("Active")
                                    })),
                            ),
                    );
                }

                grid = grid.child(row_el);
            }
        }

        let header = div()
            .flex()
            .flex_col()
            .items_center()
            .gap_2p5()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2p5()
                    .child(
                        div()
                            .text_size(px(22.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(cx.theme().foreground)
                            .child("Open windows"),
                    )
                    .when(total_count > 0, |d| {
                        d.child(
                            div()
                                .px_2()
                                .py(px(1.))
                                .rounded_full()
                                .bg(surface_bg())
                                .border_1()
                                .border_color(cx.theme().border.opacity(0.6))
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(total_count.to_string()),
                        )
                    }),
            )
            .when(multi_ws, |d| {
                let primary = cx.theme().primary;
                let primary_fg = cx.theme().primary_foreground;
                let muted_fg = cx.theme().muted_foreground;
                d.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .child(
                            ws_pill(
                                "tk-ws-all",
                                format!("All · {total_count}"),
                                !self.workspace_only,
                                primary,
                                primary_fg,
                                muted_fg,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.workspace_only = false;
                                this.selected = 0;
                                this.scroll.scroll_to_item(0);
                                cx.notify();
                            })),
                        )
                        .child(
                            ws_pill(
                                "tk-ws-cur",
                                format!("Workspace {active_ws_label} · {active_ws_count}"),
                                self.workspace_only,
                                primary,
                                primary_fg,
                                muted_fg,
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.workspace_only = true;
                                this.selected = 0;
                                this.scroll.scroll_to_item(0);
                                cx.notify();
                            })),
                        ),
                )
            })
            .when(!self.query.is_empty(), |d| {
                d.child(
                    div()
                        .px_3()
                        .py_1()
                        .rounded_full()
                        .bg(surface_bg())
                        .border_1()
                        .border_color(cx.theme().primary.opacity(0.65))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .child(
                            svg_icon(SVG_SEARCH)
                                .xsmall()
                                .text_color(cx.theme().primary),
                        )
                        .child(
                            div()
                                .text_color(cx.theme().foreground)
                                .font_weight(FontWeight::MEDIUM)
                                .child(self.query.clone()),
                        )
                        .child(
                            div()
                                .id("tk-clear-q")
                                .cursor_pointer()
                                .text_color(cx.theme().muted_foreground)
                                .hover(|s| s.text_color(cx.theme().foreground))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.query.clear();
                                    this.selected = 0;
                                    this.scroll.scroll_to_item(0);
                                    cx.notify();
                                }))
                                .child(svg_icon(SVG_CLOSE).xsmall()),
                        ),
                )
            });

        div()
            .id("tk-bg")
            .size_full()
            .bg(crate::theme::pal::with_alpha(crate::theme::pal::bg(), 0.72))
            .on_click(cx.listener(|this, _, _, cx| this.close(cx)))
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| this.on_key(ev, cx)))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_5()
            .child(
                div()
                    .occlude()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_5()
                    .child(header)
                    .child(
                        grid.with_animation("ts-open", ui::anim_modal(), |el, delta| {
                            el.opacity(delta)
                        }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground.opacity(0.85))
                            .child("Navigate · Enter focus · Del close · Ctrl+F float · Ctrl+M move here · Esc close"),
                    ),
            )
    }
}
