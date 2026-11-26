//! Enriched Raycast-style Application Launcher built with `gpui-component`.
//!
//! Features a real `Input` search bar, app descriptions (`comment`),
//! category tags, persisted favorites & recents, `.desktop` rescan,
//! and quick shell command execution via `> command`.

use std::collections::HashSet;
use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{
    div, px, AnimationExt, App, ClickEvent, Context, Entity, FocusHandle, FontWeight,
    KeyDownEvent, ScrollHandle, Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::tag::{Tag, TagVariant};
use gpui_component::{ActiveTheme as _, Sizable as _};
use kumizo_kit::prelude::*;
use serde::{Deserialize, Serialize};

use crate::{
    kit::{
        clamped_panel_size, ensure_components_init, hsla, panel_bg, surface_bg, svg_icon,
        sync_gpui_theme, sync_if_needed, SVG_APPS, SVG_CLOSE, SVG_PIN, SVG_ROTATE_CCW, SVG_SEARCH,
        SVG_TERMINAL,
    },
    theme,
    ui::{self, ids},
};

const MAX_RECENTS: usize = 6;

#[derive(Clone, Default, Serialize, Deserialize)]
struct LauncherPrefs {
    #[serde(default)]
    favorites: Vec<String>,
    #[serde(default)]
    recent: Vec<String>,
}

fn prefs_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("kumi/launcher_prefs.json"))
}

fn load_prefs() -> LauncherPrefs {
    let Some(path) = prefs_path() else {
        return LauncherPrefs::default();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_prefs(prefs: &LauncherPrefs) {
    let Some(path) = prefs_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(prefs) {
        let _ = std::fs::write(path, json);
    }
}

#[derive(Clone)]
enum EntryBadge {
    None,
    Favorite,
    Recent,
}

#[derive(Clone)]
enum LaunchTarget {
    App {
        app: AppInfo,
        badge: EntryBadge,
    },
    ShellCommand(String),
}

pub struct AppLauncher {
    apps: Entity<Apps>,
    query: String,
    search_input: Option<Entity<InputState>>,
    need_focus: bool,
    selected: usize,
    prefs: LauncherPrefs,
    visible_child_indices: Vec<usize>,
    scroll: ScrollHandle,
    last_theme: &'static str,
    focus: FocusHandle,
}

impl AppLauncher {
    pub fn new(cx: &mut Context<Self>) -> Self {
        ensure_components_init(cx);
        sync_gpui_theme(cx);

        Self {
            apps: cx.service::<Apps>(),
            query: String::new(),
            search_input: None,
            need_focus: true,
            selected: 0,
            prefs: load_prefs(),
            visible_child_indices: Vec::new(),
            scroll: ScrollHandle::new(),
            last_theme: theme::current().name,
            focus: cx.focus_handle(),
        }
    }

    fn ensure_search_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(input) = &self.search_input {
            return input.clone();
        }
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Rechercher une application ou « > commande »…")
        });
        cx.subscribe_in(&input, window, |this, state, ev: &InputEvent, window, cx| {
            match ev {
                InputEvent::Change => {
                    this.query = state.read(cx).value().to_string();
                    this.selected = 0;
                    this.scroll.scroll_to_item(0);
                    cx.notify();
                }
                InputEvent::PressEnter { .. } => {
                    this.activate(this.selected, window, cx);
                }
                _ => {}
            }
        })
        .detach();
        self.search_input = Some(input.clone());
        input
    }

    fn entries(&self, cx: &Context<Self>) -> Vec<LaunchTarget> {
        let trimmed = self.query.trim();
        if let Some(rest) = trimmed.strip_prefix('>') {
            return vec![LaunchTarget::ShellCommand(rest.trim().to_string())];
        }

        let fav_set: HashSet<&str> = self.prefs.favorites.iter().map(String::as_str).collect();
        let rec_set: HashSet<&str> = self.prefs.recent.iter().map(String::as_str).collect();

        if trimmed.is_empty() {
            let all = self.apps.read(cx).apps();
            let mut seen = HashSet::new();
            let mut out = Vec::new();

            for id in &self.prefs.favorites {
                if let Some(app) = all.iter().find(|a| &a.id == id) {
                    if seen.insert(app.id.clone()) {
                        out.push(LaunchTarget::App {
                            app: app.clone(),
                            badge: EntryBadge::Favorite,
                        });
                    }
                }
            }
            for id in &self.prefs.recent {
                if let Some(app) = all.iter().find(|a| &a.id == id) {
                    if seen.insert(app.id.clone()) {
                        out.push(LaunchTarget::App {
                            app: app.clone(),
                            badge: EntryBadge::Recent,
                        });
                    }
                }
            }
            for app in self.apps.read(cx).search("") {
                if seen.insert(app.id.clone()) {
                    out.push(LaunchTarget::App {
                        app,
                        badge: EntryBadge::None,
                    });
                }
            }
            return out;
        }

        self.apps
            .read(cx)
            .search(trimmed)
            .into_iter()
            .map(|app| {
                let badge = if fav_set.contains(app.id.as_str()) {
                    EntryBadge::Favorite
                } else if rec_set.contains(app.id.as_str()) {
                    EntryBadge::Recent
                } else {
                    EntryBadge::None
                };
                LaunchTarget::App { app, badge }
            })
            .collect()
    }

    fn record_launch(&mut self, app_id: &str) {
        self.prefs.recent.retain(|id| id != app_id);
        self.prefs.recent.insert(0, app_id.to_string());
        self.prefs.recent.truncate(MAX_RECENTS);
        save_prefs(&self.prefs);
    }

    fn toggle_favorite(&mut self, app_id: String, cx: &mut Context<Self>) {
        if let Some(pos) = self.prefs.favorites.iter().position(|id| id == &app_id) {
            self.prefs.favorites.remove(pos);
        } else {
            self.prefs.favorites.push(app_id);
        }
        save_prefs(&self.prefs);
        cx.notify();
    }

    fn clear_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.clear();
        self.selected = 0;
        self.scroll.scroll_to_item(0);
        if let Some(input) = &self.search_input {
            input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        cx.notify();
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.clear();
        self.selected = 0;
        self.need_focus = true;
        self.scroll.scroll_to_item(0);
        if let Some(input) = &self.search_input {
            input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        cx.hide_window(ids::LAUNCHER);
        cx.notify();
    }

    fn activate(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.entries(cx);
        let Some(target) = items.get(i) else {
            return;
        };
        match target {
            LaunchTarget::App { app, .. } => {
                let id = app.id.clone();
                self.apps.update(cx, |a, _| a.launch(app));
                self.record_launch(&id);
                self.close(window, cx);
            }
            LaunchTarget::ShellCommand(cmd) => {
                if !cmd.is_empty() {
                    ui::run("sh", &["-c", cmd]);
                    self.close(window, cx);
                }
            }
        }
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.visible_child_indices.len();
        if count > 0 {
            self.selected = (self.selected as isize + delta).rem_euclid(count as isize) as usize;
            if let Some(&child_idx) = self.visible_child_indices.get(self.selected) {
                self.scroll.scroll_to_item(child_idx);
            }
            cx.notify();
        }
    }

    fn capture_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = ui::key(ev);
        let mods = &ev.keystroke.modifiers;

        match key {
            "escape" => {
                cx.stop_propagation();
                if !self.query.is_empty() {
                    self.clear_query(window, cx);
                } else {
                    self.close(window, cx);
                }
            }
            "up" => {
                cx.stop_propagation();
                self.step(-1, cx);
            }
            "down" | "tab" => {
                cx.stop_propagation();
                self.step(1, cx);
            }
            "r" if mods.control => {
                cx.stop_propagation();
                self.apps.update(cx, |a, _| a.reload());
                self.selected = 0;
                self.scroll.scroll_to_item(0);
                cx.notify();
            }
            "p" if mods.alt || mods.control => {
                cx.stop_propagation();
                if let Some(LaunchTarget::App { app, .. }) = self.entries(cx).get(self.selected) {
                    self.toggle_favorite(app.id.clone(), cx);
                }
            }
            "f" if mods.control => {
                cx.stop_propagation();
                if let Some(input) = &self.search_input {
                    input.update(cx, |s, cx| s.focus(window, cx));
                }
            }
            _ => {}
        }
    }
}

fn section_label(text: &'static str, cx: &App) -> impl IntoElement {
    div()
        .px_2()
        .pt_1p5()
        .pb_0p5()
        .text_xs()
        .font_weight(FontWeight::BOLD)
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

impl Render for AppLauncher {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        sync_if_needed(&mut self.last_theme, cx);

        let search_input = self.ensure_search_input(window, cx);
        if self.need_focus || window.focused(cx).is_none() {
            self.need_focus = false;
            search_input.update(cx, |s, cx| s.focus(window, cx));
        }

        let entries = self.entries(cx);
        let total = entries.len();
        if self.selected >= total {
            self.selected = 0;
        }
        let is_shell_mode = self.query.trim().starts_with('>');
        let has_highlights = self.query.trim().is_empty()
            && entries.iter().any(|e| {
                matches!(
                    e,
                    LaunchTarget::App {
                        badge: EntryBadge::Favorite | EntryBadge::Recent,
                        ..
                    }
                )
            });

        let header = div()
            .px_4()
            .py_2p5()
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                svg_icon(if is_shell_mode {
                    SVG_TERMINAL
                } else {
                    SVG_SEARCH
                })
                .text_color(cx.theme().primary),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&search_input).appearance(false)),
            )
            .when(is_shell_mode, |d| {
                d.child(
                    Tag::new()
                        .with_variant(TagVariant::Primary)
                        .xsmall()
                        .child("Shell"),
                )
            })
            .when(!self.query.is_empty(), |d| {
                d.child(
                    Button::new("lc-clear")
                        .xsmall()
                        .ghost()
                        .icon(svg_icon(SVG_CLOSE))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.clear_query(window, cx);
                        })),
                )
            })
            .child(
                Button::new("lc-reload")
                    .xsmall()
                    .ghost()
                    .icon(svg_icon(SVG_ROTATE_CCW))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.apps.update(cx, |a, _| a.reload());
                        this.selected = 0;
                        this.scroll.scroll_to_item(0);
                        cx.notify();
                    })),
            );

        let mut list = div()
            .id("lc-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p_2()
            .flex()
            .flex_col()
            .gap_0p5();

        let mut child_indices = Vec::with_capacity(total);
        let mut child_cursor = 0usize;
        let mut shown_fav_header = false;
        let mut shown_apps_header = false;

        if entries.is_empty() {
            list = list.child(
                div()
                    .py_12()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1p5()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Aucune application trouvée")
                    .child(
                        div()
                            .text_xs()
                            .child("Préfixez avec « > » pour exécuter une commande shell"),
                    ),
            );
        } else {
            for (i, target) in entries.into_iter().enumerate() {
                let selected = i == self.selected;

                match target {
                    LaunchTarget::ShellCommand(cmd) => {
                        child_indices.push(child_cursor);
                        child_cursor += 1;

                        let empty_cmd = cmd.is_empty();
                        list = list.child(
                            div()
                                .id("lc-shell-row")
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .px_3()
                                .py_2p5()
                                .rounded(px(9.))
                                .cursor_pointer()
                                .bg(surface_bg())
                                .border_l_2()
                                .border_color(cx.theme().primary)
                                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                    this.activate(i, window, cx);
                                }))
                                .child(svg_icon(SVG_TERMINAL).text_color(cx.theme().primary))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(cx.theme().foreground)
                                                .truncate()
                                                .child(if empty_cmd {
                                                    "Mode commande shell".to_string()
                                                } else {
                                                    format!("Exécuter « {cmd} »")
                                                }),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(if empty_cmd {
                                                    "Tapez une commande après « > » puis Entrée"
                                                } else {
                                                    "Lancer via sh -c"
                                                }),
                                        ),
                                )
                                .child(
                                    Tag::new()
                                        .with_variant(TagVariant::Primary)
                                        .xsmall()
                                        .child("Entrée"),
                                ),
                        );
                    }
                    LaunchTarget::App { app, badge } => {
                        if has_highlights {
                            let is_highlight =
                                matches!(badge, EntryBadge::Favorite | EntryBadge::Recent);
                            if is_highlight && !shown_fav_header {
                                shown_fav_header = true;
                                list = list.child(section_label("FAVORIS & RÉCENTS", cx));
                                child_cursor += 1;
                            } else if !is_highlight && !shown_apps_header {
                                shown_apps_header = true;
                                list = list.child(section_label("APPLICATIONS", cx));
                                child_cursor += 1;
                            }
                        }

                        child_indices.push(child_cursor);
                        child_cursor += 1;

                        let is_fav = self.prefs.favorites.iter().any(|id| id == &app.id);
                        let fav_id = app.id.clone();

                        let icon_el = match app.icon.clone() {
                            Some(img) => {
                                gpui::img(img).w(px(28.)).h(px(28.)).into_any_element()
                            }
                            None => svg_icon(SVG_APPS)
                                .text_color(cx.theme().muted_foreground)
                                .into_any_element(),
                        };

                        list = list.child(
                            div()
                                .id(("lc-app", i))
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_3()
                                .px_3()
                                .py_2()
                                .rounded(px(9.))
                                .cursor_pointer()
                                .bg(if selected {
                                    surface_bg()
                                } else {
                                    hsla(0).opacity(0.0)
                                })
                                .border_l_2()
                                .border_color(if selected {
                                    cx.theme().primary
                                } else {
                                    hsla(0).opacity(0.0)
                                })
                                .hover(move |s| {
                                    s.bg(if selected {
                                        surface_bg()
                                    } else {
                                        surface_bg().opacity(0.5)
                                    })
                                })
                                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                    this.activate(i, window, cx);
                                }))
                                .child(
                                    div()
                                        .w(px(28.))
                                        .h(px(28.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .flex_shrink_0()
                                        .child(icon_el),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_sm()
                                                .font_weight(if selected {
                                                    FontWeight::SEMIBOLD
                                                } else {
                                                    FontWeight::MEDIUM
                                                })
                                                .text_color(cx.theme().foreground)
                                                .truncate()
                                                .child(app.name),
                                        )
                                        .when(!app.comment.trim().is_empty(), |d| {
                                            d.child(
                                                div()
                                                    .text_xs()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .truncate()
                                                    .child(app.comment),
                                            )
                                        }),
                                )
                                .when(is_fav && !selected, |d| {
                                    d.child(
                                        svg_icon(SVG_PIN)
                                            .xsmall()
                                            .text_color(cx.theme().primary.opacity(0.8)),
                                    )
                                })
                                .when(selected, |d| {
                                    d.child(
                                        div()
                                            .id(("lc-pin-wrap", i))
                                            .on_click(|_, _, cx| cx.stop_propagation())
                                            .child(
                                                Button::new(("lc-pin", i))
                                                    .xsmall()
                                                    .map(|b| {
                                                        if is_fav {
                                                            b.primary()
                                                        } else {
                                                            b.ghost()
                                                        }
                                                    })
                                                    .icon(svg_icon(SVG_PIN))
                                                    .on_click(cx.listener(move |this, _, _, cx| {
                                                        this.toggle_favorite(fav_id.clone(), cx);
                                                    })),
                                            ),
                                    )
                                }),
                        );
                    }
                }
            }
        }
        self.visible_child_indices = child_indices;

        let footer = div()
            .px_4()
            .py_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child("↑/↓ naviguer · Entrée ouvrir · > shell · Alt+P favori")
            .child("Échap fermer");

        let (panel_w, panel_h) = clamped_panel_size(window, 660., 480., 500., 340.);

        let panel = div()
            .occlude()
            .w(panel_w)
            .h(panel_h)
            .flex()
            .flex_col()
            .overflow_hidden()
            .rounded(px(14.))
            .bg(panel_bg())
            .text_color(cx.theme().foreground)
            .border_1()
            .border_color(cx.theme().border)
            .shadow_lg()
            .child(header)
            .child(list)
            .child(footer)
            .with_animation("lc-open", ui::anim_modal(), |el, delta| {
                el.opacity(delta).mt(px(6.0 * (1.0 - delta)))
            });

        ui::backdrop("lc-bg", ids::LAUNCHER, 0.58)
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                this.capture_key(ev, window, cx);
            }))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if ui::key(ev) == "enter" {
                    this.activate(this.selected, window, cx);
                }
            }))
            .flex()
            .justify_center()
            .items_start()
            .pt(px(120.))
            .child(panel)
    }
}
