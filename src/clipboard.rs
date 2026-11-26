//! Two-pane Clipboard Manager built with `gpui-component`.
//!
//! Features a real `Input` search bar, category filters (`Tout`, `Épinglés`,
//! `Liens`, `Code`), smart type & `#hex` color detection, persisted pins,
//! full multi-line preview pane, direct `+ Tâche` creation in `TodoStore`,
//! and 2-click safety on clearing history.

use std::collections::HashSet;
use std::path::PathBuf;

use chrono::{DateTime, Local, Utc};
use gpui::prelude::*;
use gpui::{
    div, px, rgba, AnimationExt, ClickEvent, Context, Entity, FocusHandle, FontWeight, Hsla,
    KeyDownEvent, ScrollHandle, Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::tag::{Tag, TagVariant};
use gpui_component::{ActiveTheme as _, Sizable as _};
use kumizo_kit::prelude::*;

use crate::{
    kit::{
        clamped_panel_size, ensure_components_init, hsla, panel_bg, surface_bg, svg_icon,
        sync_gpui_theme, sync_if_needed, SVG_CHECK, SVG_CHECK_CIRCLE, SVG_CLIPBOARD, SVG_CLOSE,
        SVG_CODE, SVG_COPY, SVG_FILE_TEXT, SVG_LINK, SVG_PIN, SVG_SEARCH, SVG_TRASH,
    },
    theme,
    todo_store::{self, TaskPriority, TaskStatus, TodoStore},
    ui::{self, ids},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClipFilter {
    All,
    Pinned,
    Links,
    Code,
}

#[derive(Clone)]
enum ClipKind {
    Color(Hsla, String),
    Url,
    Code,
    Text,
}

impl ClipKind {
    fn label(&self) -> &'static str {
        match self {
            Self::Color(..) => "Couleur",
            Self::Url => "Lien URL",
            Self::Code => "Code",
            Self::Text => "Texte",
        }
    }

    fn tag_variant(&self) -> TagVariant {
        match self {
            Self::Color(..) => TagVariant::Primary,
            Self::Url => TagVariant::Info,
            Self::Code => TagVariant::Warning,
            Self::Text => TagVariant::Secondary,
        }
    }
}

#[derive(Clone)]
struct ClipEntry {
    id: Option<u64>,
    text: String,
    preview: String,
    lines: usize,
    chars: usize,
    timestamp_unix: Option<i64>,
    pinned: bool,
    kind: ClipKind,
}

fn pins_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("kumi/clipboard_pins.json"))
}

fn load_pins() -> Vec<String> {
    let Some(path) = pins_path() else {
        return Vec::new();
    };
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn save_pins(pins: &[String]) {
    let Some(path) = pins_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(pins) {
        let _ = std::fs::write(path, json);
    }
}

fn parse_hex_color(raw: &str) -> Option<(Hsla, String)> {
    let trimmed = raw.trim();
    let hex = trimmed.strip_prefix('#')?;
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let val = match hex.len() {
        3 => {
            let r = u32::from_str_radix(&hex[0..1], 16).ok()? * 17;
            let g = u32::from_str_radix(&hex[1..2], 16).ok()? * 17;
            let b = u32::from_str_radix(&hex[2..3], 16).ok()? * 17;
            (r << 24) | (g << 16) | (b << 8) | 0xff
        }
        6 => (u32::from_str_radix(hex, 16).ok()? << 8) | 0xff,
        8 => u32::from_str_radix(hex, 16).ok()?,
        _ => return None,
    };
    Some((rgba(val).into(), trimmed.to_lowercase()))
}

fn classify_text(text: &str) -> ClipKind {
    if let Some((col, norm)) = parse_hex_color(text) {
        return ClipKind::Color(col, norm);
    }
    let trimmed = text.trim();
    if !trimmed.contains('\n')
        && !trimmed.contains(' ')
        && (trimmed.starts_with("https://") || trimmed.starts_with("http://"))
    {
        return ClipKind::Url;
    }
    let looks_json = (trimmed.starts_with('{') && trimmed.ends_with('}'))
        || (trimmed.starts_with('[') && trimmed.ends_with(']'));
    let has_code_tokens = text.contains('\n')
        && ["{", "}", "fn ", "let ", "const ", "import ", "=>", "</", "def ", "return "]
            .iter()
            .any(|tok| text.contains(tok));
    if looks_json || has_code_tokens {
        return ClipKind::Code;
    }
    ClipKind::Text
}

fn single_line_preview(text: &str) -> String {
    let collapsed: String = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ↵ ");
    if collapsed.is_empty() {
        "(vide)".to_string()
    } else {
        collapsed
    }
}

fn format_relative_time(ts: Option<i64>) -> String {
    let Some(unix) = ts else {
        return "Épinglé".to_string();
    };
    let delta = (Utc::now().timestamp() - unix).max(0);
    if delta < 45 {
        "à l'instant".to_string()
    } else if delta < 3600 {
        format!("il y a {} min", delta / 60)
    } else if delta < 86_400 {
        format!("il y a {} h", delta / 3600)
    } else {
        format!("il y a {} j", delta / 86_400)
    }
}

fn format_exact_time(ts: Option<i64>) -> String {
    let Some(unix) = ts else {
        return "Conservé hors historique".to_string();
    };
    DateTime::<Utc>::from_timestamp(unix, 0)
        .map(|dt| dt.with_timezone(&Local).format("%d/%m %H:%M:%S").to_string())
        .unwrap_or_else(|| "—".to_string())
}

pub struct ClipboardPanel {
    clipboard: Entity<Clipboard>,
    todo: Entity<TodoStore>,
    query: String,
    search_input: Option<Entity<InputState>>,
    need_focus: bool,
    filter: ClipFilter,
    selected: usize,
    pins: Vec<String>,
    clear_armed: bool,
    added_task_text: Option<String>,
    scroll: ScrollHandle,
    last_theme: &'static str,
    focus: FocusHandle,
}

impl ClipboardPanel {
    pub fn new(cx: &mut Context<Self>) -> Self {
        ensure_components_init(cx);
        sync_gpui_theme(cx);

        let clipboard = cx.service::<Clipboard>();
        let todo = todo_store::handle(cx);
        cx.observe(&clipboard, |_, _, cx| cx.notify()).detach();

        Self {
            clipboard,
            todo,
            query: String::new(),
            search_input: None,
            need_focus: true,
            filter: ClipFilter::All,
            selected: 0,
            pins: load_pins(),
            clear_armed: false,
            added_task_text: None,
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
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Rechercher dans le presse-papiers…"));
        cx.subscribe_in(&input, window, |this, state, ev: &InputEvent, window, cx| {
            match ev {
                InputEvent::Change => {
                    this.query = state.read(cx).value().to_string();
                    this.selected = 0;
                    this.clear_armed = false;
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

    fn entries(&self, cx: &Context<Self>) -> Vec<ClipEntry> {
        let cb = self.clipboard.read(cx);
        let all_live = cb.items();
        let needle = self.query.trim().to_lowercase();
        let mut out = Vec::new();
        let mut pinned_seen = HashSet::new();

        for pin_text in &self.pins {
            pinned_seen.insert(pin_text.as_str());
            let live_match = all_live.iter().find(|it| &it.text == pin_text);
            let kind = classify_text(pin_text);
            let entry = match live_match {
                Some(it) => ClipEntry {
                    id: Some(it.id),
                    text: it.text.clone(),
                    preview: it.preview(),
                    lines: it.line_count(),
                    chars: it.char_count(),
                    timestamp_unix: Some(it.timestamp.unix_timestamp()),
                    pinned: true,
                    kind,
                },
                None => ClipEntry {
                    id: None,
                    text: pin_text.clone(),
                    preview: single_line_preview(pin_text),
                    lines: pin_text.lines().count().max(1),
                    chars: pin_text.chars().count(),
                    timestamp_unix: None,
                    pinned: true,
                    kind,
                },
            };
            if self.matches_filters(&entry, &needle) {
                out.push(entry);
            }
        }

        for it in cb.search(&self.query) {
            if pinned_seen.contains(it.text.as_str()) {
                continue;
            }
            let kind = classify_text(&it.text);
            let entry = ClipEntry {
                id: Some(it.id),
                text: it.text.clone(),
                preview: it.preview(),
                lines: it.line_count(),
                chars: it.char_count(),
                timestamp_unix: Some(it.timestamp.unix_timestamp()),
                pinned: false,
                kind,
            };
            if self.matches_filters(&entry, &needle) {
                out.push(entry);
            }
        }

        out
    }

    fn matches_filters(&self, entry: &ClipEntry, needle: &str) -> bool {
        let filter_ok = match self.filter {
            ClipFilter::All => true,
            ClipFilter::Pinned => entry.pinned,
            ClipFilter::Links => matches!(entry.kind, ClipKind::Url),
            ClipFilter::Code => matches!(entry.kind, ClipKind::Code | ClipKind::Color(..)),
        };
        if !filter_ok {
            return false;
        }
        if needle.is_empty() {
            return true;
        }
        entry.text.to_lowercase().contains(needle)
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.clear();
        self.selected = 0;
        self.clear_armed = false;
        self.added_task_text = None;
        self.need_focus = true;
        self.scroll.scroll_to_item(0);
        if let Some(input) = &self.search_input {
            input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        cx.hide_window(ids::CLIPBOARD);
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

    fn copy_entry(&mut self, entry: &ClipEntry, cx: &mut Context<Self>) {
        if let Some(id) = entry.id {
            self.clipboard.update(cx, |c, _| c.copy_item(id));
        } else {
            let text = entry.text.clone();
            self.clipboard.update(cx, |c, _| c.copy_text(&text));
        }
    }

    fn activate(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let list = self.entries(cx);
        if let Some(entry) = list.get(i) {
            self.copy_entry(entry, cx);
            self.close(window, cx);
        }
    }

    fn toggle_pin(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(pos) = self.pins.iter().position(|p| p == &text) {
            self.pins.remove(pos);
        } else {
            self.pins.insert(0, text);
        }
        save_pins(&self.pins);
        cx.notify();
    }

    fn remove_entry(&mut self, i: usize, cx: &mut Context<Self>) {
        let list = self.entries(cx);
        let Some(entry) = list.get(i).cloned() else {
            return;
        };
        if let Some(id) = entry.id {
            self.clipboard.update(cx, |c, _| c.remove_item(id));
        }
        if entry.pinned {
            self.pins.retain(|p| p != &entry.text);
            save_pins(&self.pins);
        }
        let n = self.entries(cx).len();
        self.selected = if n == 0 { 0 } else { self.selected.min(n - 1) };
        self.scroll.scroll_to_item(self.selected);
        cx.notify();
    }

    fn send_to_todo(&mut self, text: &str, cx: &mut Context<Self>) {
        let first_line = text
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("Extrait presse-papiers");
        let title: String = first_line.chars().take(90).collect();
        let description = if text.trim() != title.trim() {
            text.to_string()
        } else {
            String::new()
        };
        self.todo.update(cx, |s, cx| {
            s.add(
                title,
                description,
                TaskPriority::Normal,
                TaskStatus::Todo,
                None,
                vec!["clip".to_string()],
                cx,
            );
        });
        self.added_task_text = Some(text.to_string());
        cx.notify();
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let n = self.entries(cx).len();
        if n > 0 {
            self.selected = (self.selected as isize + delta).rem_euclid(n as isize) as usize;
            self.clear_armed = false;
            self.scroll.scroll_to_item(self.selected);
            cx.notify();
        }
    }

    fn capture_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = ui::key(ev);
        let mods = &ev.keystroke.modifiers;

        match key {
            "escape" => {
                cx.stop_propagation();
                if self.clear_armed {
                    self.clear_armed = false;
                    cx.notify();
                } else if !self.query.is_empty() {
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
            "p" if mods.alt || mods.control => {
                cx.stop_propagation();
                let list = self.entries(cx);
                if let Some(entry) = list.get(self.selected) {
                    self.toggle_pin(entry.text.clone(), cx);
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

impl Render for ClipboardPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        sync_if_needed(&mut self.last_theme, cx);

        let search_input = self.ensure_search_input(window, cx);
        if self.need_focus || window.focused(cx).is_none() {
            self.need_focus = false;
            search_input.update(cx, |s, cx| s.focus(window, cx));
        }

        let results = self.entries(cx);
        let total = results.len();
        if self.selected >= total {
            self.selected = 0;
        }
        let selected_entry = results.get(self.selected).cloned();
        let live_count = self.clipboard.read(cx).items().len();
        let pin_count = self.pins.len();

        let header = div()
            .px_4()
            .py_2p5()
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(svg_icon(SVG_SEARCH).text_color(cx.theme().primary))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&search_input).appearance(false)),
            )
            .when(!self.query.is_empty(), |d| {
                d.child(
                    Button::new("cb-clear-q")
                        .xsmall()
                        .ghost()
                        .icon(svg_icon(SVG_CLOSE))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.clear_query(window, cx);
                        })),
                )
            })
            .when(live_count > 0, |d| {
                d.child(
                    Button::new("cb-clear-all")
                        .xsmall()
                        .map(|b| {
                            if self.clear_armed {
                                b.danger()
                            } else {
                                b.ghost()
                            }
                        })
                        .icon(svg_icon(SVG_TRASH))
                        .label(if self.clear_armed {
                            "Confirmer ?"
                        } else {
                            "Vider"
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            if !this.clear_armed {
                                this.clear_armed = true;
                                cx.notify();
                                return;
                            }
                            this.clear_armed = false;
                            this.clipboard.update(cx, |c, cx| {
                                c.clear();
                                cx.notify();
                            });
                            this.selected = 0;
                            cx.notify();
                        })),
                )
            })
            .child(
                Button::new("cb-close")
                    .xsmall()
                    .ghost()
                    .icon(svg_icon(SVG_CLOSE))
                    .on_click(cx.listener(|this, _, window, cx| this.close(window, cx))),
            );

        let filters = [
            (ClipFilter::All, "Tout".to_string()),
            (ClipFilter::Pinned, format!("Épinglés ({pin_count})")),
            (ClipFilter::Links, "Liens".to_string()),
            (ClipFilter::Code, "Code & Couleurs".to_string()),
        ];

        let mut filter_bar = div()
            .px_4()
            .py_1p5()
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_row()
            .items_center()
            .justify_between();
        let mut filter_tabs = div().flex().flex_row().items_center().gap_1();
        for (idx, (flt, label)) in filters.into_iter().enumerate() {
            let active = self.filter == flt;
            filter_tabs = filter_tabs.child(
                Button::new(("cb-flt", idx))
                    .xsmall()
                    .map(|b| if active { b.secondary() } else { b.ghost() })
                    .label(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.filter = flt;
                        this.selected = 0;
                        this.clear_armed = false;
                        this.scroll.scroll_to_item(0);
                        cx.notify();
                    })),
            );
        }
        filter_bar = filter_bar.child(filter_tabs).child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{total}")),
        );

        let mut list = div()
            .id("cb-scroll")
            .w(px(330.))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(cx.theme().border)
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .p_2()
            .flex()
            .flex_col()
            .gap_0p5();

        if results.is_empty() {
            list = list.child(
                div()
                    .py_12()
                    .px_3()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1p5()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Aucun élément")
                    .child(
                        div()
                            .text_xs()
                            .child("Copiez du texte ou ajustez le filtre."),
                    ),
            );
        } else {
            for (i, entry) in results.into_iter().enumerate() {
                let selected = i == self.selected;
                let rel_time = format_relative_time(entry.timestamp_unix);
                let sub = if entry.lines > 1 {
                    format!("{rel_time} · {} l.", entry.lines)
                } else {
                    rel_time
                };

                let icon_el = match &entry.kind {
                    ClipKind::Color(col, _) => div()
                        .w(px(16.))
                        .h(px(16.))
                        .rounded(px(4.))
                        .bg(*col)
                        .border_1()
                        .border_color(cx.theme().border)
                        .flex_shrink_0()
                        .into_any_element(),
                    ClipKind::Url => svg_icon(SVG_LINK)
                        .small()
                        .text_color(cx.theme().info)
                        .into_any_element(),
                    ClipKind::Code => svg_icon(SVG_CODE)
                        .small()
                        .text_color(cx.theme().warning)
                        .into_any_element(),
                    ClipKind::Text => svg_icon(SVG_FILE_TEXT)
                        .small()
                        .text_color(cx.theme().muted_foreground)
                        .into_any_element(),
                };

                list = list.child(
                    div()
                        .id(("cb-row", i))
                        .w_full()
                        .px_3()
                        .py_2()
                        .rounded(px(9.))
                        .cursor_pointer()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2p5()
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
                            if this.selected == i {
                                this.activate(i, window, cx);
                            } else {
                                this.selected = i;
                                cx.notify();
                            }
                        }))
                        .child(
                            div()
                                .w(px(20.))
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
                                        .text_xs()
                                        .font_weight(if selected {
                                            FontWeight::SEMIBOLD
                                        } else {
                                            FontWeight::MEDIUM
                                        })
                                        .text_color(cx.theme().foreground)
                                        .truncate()
                                        .child(entry.preview),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .truncate()
                                        .child(sub),
                                ),
                        )
                        .when(entry.pinned, |d| {
                            d.child(
                                svg_icon(SVG_PIN)
                                    .xsmall()
                                    .text_color(cx.theme().primary.opacity(0.85)),
                            )
                        }),
                );
            }
        }

        let preview_pane = match selected_entry {
            None => div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(svg_icon(SVG_CLIPBOARD).large())
                .child("Sélectionnez une entrée"),
            Some(entry) => {
                let is_color = match &entry.kind {
                    ClipKind::Color(col, hex) => Some((*col, hex.clone())),
                    _ => None,
                };
                let entry_for_copy = entry.clone();
                let pin_text = entry.text.clone();
                let todo_text = entry.text.clone();
                let added_here = self.added_task_text.as_deref() == Some(entry.text.as_str());
                let sel_idx = self.selected;

                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                Tag::new()
                                    .with_variant(entry.kind.tag_variant())
                                    .xsmall()
                                    .child(entry.kind.label()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_features(ui::tabular())
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!(
                                        "{} · {} l. · {} car.",
                                        format_exact_time(entry.timestamp_unix),
                                        entry.lines,
                                        entry.chars
                                    )),
                            ),
                    )
                    .children(is_color.map(|(col, hex)| {
                        div()
                            .w_full()
                            .h(px(48.))
                            .rounded(px(9.))
                            .bg(col)
                            .border_1()
                            .border_color(cx.theme().border)
                            .px_3()
                            .flex()
                            .items_center()
                            .justify_end()
                            .child(
                                Tag::new()
                                    .with_variant(TagVariant::Secondary)
                                    .xsmall()
                                    .child(hex),
                            )
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .rounded(px(10.))
                            .bg(surface_bg().opacity(0.72))
                            .border_1()
                            .border_color(cx.theme().border)
                            .p_3p5()
                            .overflow_y_scrollbar()
                            .child(
                                div()
                                    .w_full()
                                    .text_sm()
                                    .text_color(cx.theme().foreground)
                                    .child(entry.text),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Button::new("cb-act-copy")
                                            .small()
                                            .primary()
                                            .icon(svg_icon(SVG_COPY))
                                            .label("Copier")
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.copy_entry(&entry_for_copy, cx);
                                                this.close(window, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("cb-act-pin")
                                            .small()
                                            .map(|b| {
                                                if entry.pinned {
                                                    b.primary()
                                                } else {
                                                    b.ghost()
                                                }
                                            })
                                            .icon(svg_icon(SVG_PIN))
                                            .label(if entry.pinned {
                                                "Épinglé"
                                            } else {
                                                "Épingler"
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.toggle_pin(pin_text.clone(), cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("cb-act-todo")
                                            .small()
                                            .ghost()
                                            .icon(svg_icon(if added_here {
                                                SVG_CHECK
                                            } else {
                                                SVG_CHECK_CIRCLE
                                            }))
                                            .label(if added_here {
                                                "Ajouté"
                                            } else {
                                                "+ Tâche"
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.send_to_todo(&todo_text, cx);
                                            })),
                                    ),
                            )
                            .child(
                                Button::new("cb-act-del")
                                    .small()
                                    .danger()
                                    .ghost()
                                    .icon(svg_icon(SVG_TRASH))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.remove_entry(sel_idx, cx);
                                    })),
                            ),
                    )
            }
        };

        let body = div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_row()
            .child(list)
            .child(preview_pane);

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
            .child("↑/↓ parcourir · Entrée copier · Alt+P épingler · Ctrl+F chercher")
            .child("Échap fermer");

        let (panel_w, panel_h) = clamped_panel_size(window, 840., 520., 620., 380.);

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
            .child(filter_bar)
            .child(body)
            .child(footer)
            .with_animation("cb-open", ui::anim_modal(), |el, delta| {
                el.opacity(delta).mt(px(6.0 * (1.0 - delta)))
            });

        ui::backdrop("cb-bg", ids::CLIPBOARD, 0.58)
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
            .pt(px(110.))
            .child(panel)
    }
}
