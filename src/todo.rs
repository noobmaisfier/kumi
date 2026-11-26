//! Modern 3-pane Linear/Notion-style hybrid Todo & Kanban workspace (`1100x720`).
//!
//! Built with `gpui-component` (`Sidebar`, `Input`, `Textarea`, `Button`,
//! `Progress`, `Tag`, `Badge`, `ScrollableElement`) and synchronized with
//! Kumi's active theme palette.

use std::sync::Once;

use gpui::prelude::*;
use gpui::{
    div, px, rgb, AnimationExt, App, Context, Entity, FocusHandle, FontWeight, Hsla, KeyDownEvent,
    Window,
};
use gpui_component::badge::Badge;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_component::progress::Progress;
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::sidebar::{
    Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem,
};
use gpui_component::tag::{Tag, TagVariant};
use gpui_component::{ActiveTheme as _, Icon, Sizable as _, Theme as GpuiTheme, ThemeMode};
use kumizo_kit::prelude::*;

use crate::{
    pomodoro::{self, Pomodoro},
    theme,
    todo_store::{
        self, format_due_label, in_days_str, is_valid_date, today_str, DueUrgency, TaskItem,
        TaskPriority, TaskStatus, TodoStore,
    },
    ui::{self, ids},
};

static INIT_COMPONENTS: Once = Once::new();

// Embedded Lucide SVG icons (Kumizo does not mount an external AssetSource,
// so embedding raw SVG bytes guarantees crisp rendering in every component).
const SVG_CHECK: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.75" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5"/></svg>"#;
const SVG_PLUS: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14"/><path d="M12 5v14"/></svg>"#;
const SVG_SEARCH: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/></svg>"#;
const SVG_CLOSE: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>"#;
const SVG_TRASH: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>"#;
const SVG_ARROW_LEFT: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m12 19-7-7 7-7"/><path d="M19 12H5"/></svg>"#;
const SVG_ARROW_RIGHT: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12h14"/><path d="m12 5 7 7-7 7"/></svg>"#;
const SVG_SPARKLES: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M9.937 15.5A2 2 0 0 0 8.5 14.063l-6.135-1.582a.5.5 0 0 1 0-.962L8.5 9.936A2 2 0 0 0 9.937 8.5l1.582-6.135a.5.5 0 0 1 .963 0L14.063 8.5A2 2 0 0 0 15.5 9.937l6.135 1.581a.5.5 0 0 1 0 .964L15.5 14.063a2 2 0 0 0-1.437 1.437l-1.582 6.135a.5.5 0 0 1-.963 0z"/></svg>"#;
const SVG_LIST: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 6h13"/><path d="M8 12h13"/><path d="M8 18h13"/><path d="M3 6h.01"/><path d="M3 12h.01"/><path d="M3 18h.01"/></svg>"#;
const SVG_KANBAN: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect width="18" height="18" x="3" y="3" rx="2"/><path d="M8 7v7"/><path d="M12 7v4"/><path d="M16 7v9"/></svg>"#;
const SVG_CALENDAR: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 2v4"/><path d="M16 2v4"/><rect width="18" height="18" x="3" y="4" rx="2"/><path d="M3 10h18"/></svg>"#;
const SVG_FLAME: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8.5 14.5A2.5 2.5 0 0 0 11 12c0-1.38-.5-2-1-3-1.072-2.143-.224-4.054 2-6 .5 2.5 2 4.9 4 6.5 2 1.6 3 3.5 3 5.5a7 7 0 1 1-14 0c0-1.153.433-2.294 1-3a2.5 2.5 0 0 0 2.5 2.5z"/></svg>"#;
const SVG_CLOCK: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/></svg>"#;
const SVG_CHECK_CIRCLE: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/></svg>"#;
const SVG_TAG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M12.586 2.586A2 2 0 0 0 11.172 2H4a2 2 0 0 0-2 2v7.172a2 2 0 0 0 .586 1.414l8.704 8.704a2.426 2.426 0 0 0 3.42 0l6.58-6.58a2.426 2.426 0 0 0 0-3.42z"/><circle cx="7.5" cy="7.5" r=".5" fill="currentColor"/></svg>"#;
const SVG_PLAY: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="6 3 20 12 6 21 6 3"/></svg>"#;
const SVG_PAUSE: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="14" y="4" width="4" height="16" rx="1"/><rect x="6" y="4" width="4" height="16" rx="1"/></svg>"#;
const SVG_SORT: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m3 16 4 4 4-4"/><path d="M7 20V4"/><path d="m21 8-4-4-4 4"/><path d="M17 4v16"/></svg>"#;

fn svg_icon(bytes: &'static [u8]) -> Icon {
    Icon::default().data(bytes)
}

fn hsla(hex: u32) -> Hsla {
    rgb(hex).into()
}

/// Synchronizes `gpui-component`'s global `Theme` with Kumi's active palette.
/// Must only be called on init or when Kumi's theme name actually changes.
fn sync_gpui_theme(cx: &mut App) {
    let target_mode = if theme::is_light() {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    };
    if GpuiTheme::global(cx).mode != target_mode {
        GpuiTheme::change(target_mode, None, cx);
    }
    let pal = theme::current();
    let bg = hsla(pal.bg);
    let surface = hsla(pal.surface);
    let raised = hsla(pal.raised);
    let border = hsla(pal.border);
    let text = hsla(pal.text);
    let muted = hsla(pal.muted);
    let accent = hsla(pal.accent);
    let accent2 = hsla(pal.accent2);
    let on_accent = hsla(pal.on_accent);
    let ok = hsla(pal.ok);
    let warn = hsla(pal.warn);
    let danger = hsla(pal.danger);

    GpuiTheme::update(cx, |t| {
        t.radius = px(8.);
        t.radius_lg = px(12.);
        t.font_size = px(14.);

        let c = &mut t.colors;
        c.background = surface;
        c.foreground = text;
        c.border = border;
        c.input = border;
        c.ring = accent;
        c.caret = accent;
        c.selection = accent.opacity(0.25);

        c.muted = raised;
        c.muted_foreground = muted;
        c.accent = raised;
        c.accent_foreground = text;
        c.popover = surface;
        c.popover_foreground = text;
        c.group_box = raised;
        c.group_box_foreground = text;

        c.primary = accent;
        c.primary_hover = accent.opacity(0.88);
        c.primary_active = accent.opacity(0.76);
        c.primary_foreground = on_accent;

        c.secondary = raised;
        c.secondary_hover = border;
        c.secondary_active = border;
        c.secondary_foreground = text;

        c.button = raised;
        c.button_hover = border;
        c.button_active = border;
        c.button_foreground = text;
        c.button_primary = accent;
        c.button_primary_hover = accent.opacity(0.88);
        c.button_primary_active = accent.opacity(0.76);
        c.button_primary_foreground = on_accent;
        c.button_secondary = raised;
        c.button_secondary_hover = border;
        c.button_secondary_active = border;
        c.button_secondary_foreground = text;

        c.success = ok;
        c.success_hover = ok.opacity(0.88);
        c.success_active = ok.opacity(0.76);
        c.success_foreground = on_accent;

        c.warning = warn;
        c.warning_hover = warn.opacity(0.88);
        c.warning_active = warn.opacity(0.76);
        c.warning_foreground = on_accent;

        c.danger = danger;
        c.danger_hover = danger.opacity(0.88);
        c.danger_active = danger.opacity(0.76);
        c.danger_foreground = on_accent;

        c.info = accent2;
        c.info_hover = accent2.opacity(0.88);
        c.info_active = accent2.opacity(0.76);
        c.info_foreground = on_accent;

        c.sidebar = bg;
        c.sidebar_foreground = text;
        c.sidebar_border = border;
        c.sidebar_accent = raised;
        c.sidebar_accent_foreground = accent;
        c.sidebar_primary = accent;
        c.sidebar_primary_foreground = on_accent;

        c.list = surface;
        c.list_hover = raised;
        c.list_active = raised;
        c.list_active_border = accent;

        c.progress_bar = accent;
        c.scrollbar = bg.opacity(0.0);
        c.scrollbar_thumb = border;
        c.scrollbar_thumb_hover = muted;
    });
}

fn ensure_components_init(cx: &mut App) {
    INIT_COMPONENTS.call_once(|| {
        gpui_component::init(cx);
    });
}

#[derive(Clone, PartialEq, Eq)]
enum NavFilter {
    Focus,
    All,
    Kanban,
    Today,
    HighPriority,
    InProgress,
    Done,
    Tag(String),
}

impl NavFilter {
    fn title(&self) -> String {
        match self {
            NavFilter::Focus => "Vue Focus · Prochaines tâches".to_string(),
            NavFilter::All => "Toutes les tâches".to_string(),
            NavFilter::Kanban => "Tableau Kanban".to_string(),
            NavFilter::Today => "Aujourd'hui & en retard".to_string(),
            NavFilter::HighPriority => "Haute priorité".to_string(),
            NavFilter::InProgress => "En cours".to_string(),
            NavFilter::Done => "Tâches terminées".to_string(),
            NavFilter::Tag(t) => format!("#{t}"),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortBy {
    Priority,
    DueDate,
    Order,
}

impl SortBy {
    fn label(self) -> &'static str {
        match self {
            SortBy::Priority => "Tri: Priorité",
            SortBy::DueDate => "Tri: Échéance",
            SortBy::Order => "Tri: Ajout",
        }
    }

    fn next(self) -> Self {
        match self {
            SortBy::Priority => SortBy::DueDate,
            SortBy::DueDate => SortBy::Order,
            SortBy::Order => SortBy::Priority,
        }
    }
}

struct DetailInputs {
    task_id: u64,
    due_invalid: bool,
    title: Entity<InputState>,
    description: Entity<TextareaState>,
    due_date: Entity<InputState>,
    tag: Entity<InputState>,
    subtask: Entity<InputState>,
}

struct CreateModalState {
    status: TaskStatus,
    priority: TaskPriority,
    due_date: Option<String>,
    due_invalid: bool,
    need_focus: bool,
    title: Entity<InputState>,
    description: Entity<TextareaState>,
    due_input: Entity<InputState>,
    tags_input: Entity<InputState>,
}

pub struct TodoWidget {
    store: Entity<TodoStore>,
    pomodoro: Entity<Pomodoro>,
    focus_task_id: Option<u64>,
    nav: NavFilter,
    sort_by: SortBy,
    selected_id: Option<u64>,
    search_query: String,
    last_theme: &'static str,

    search_input: Option<Entity<InputState>>,
    detail_inputs: Option<DetailInputs>,
    create_modal: Option<CreateModalState>,
    focus: FocusHandle,
}

impl TodoWidget {
    pub fn new(cx: &mut Context<Self>) -> Self {
        ensure_components_init(cx);
        sync_gpui_theme(cx);
        let store = todo_store::handle(cx);
        let pomodoro = pomodoro::handle(cx);
        cx.observe(&store, |_, _, cx| cx.notify()).detach();
        cx.observe(&pomodoro, |_, _, cx| cx.notify()).detach();

        Self {
            store,
            pomodoro,
            focus_task_id: None,
            nav: NavFilter::Focus,
            sort_by: SortBy::Priority,
            selected_id: None,
            search_query: String::new(),
            last_theme: theme::current().name,
            search_input: None,
            detail_inputs: None,
            create_modal: None,
            focus: cx.focus_handle(),
        }
    }

    fn ensure_search_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(search) = &self.search_input {
            return search.clone();
        }
        let search =
            cx.new(|cx| InputState::new(window, cx).placeholder("Rechercher… (Ctrl+F)"));
        cx.subscribe_in(&search, window, |this, state, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.search_query = state.read(cx).value().to_string();
                cx.notify();
            }
        })
        .detach();
        self.search_input = Some(search.clone());
        search
    }

    fn open_create_modal(
        &mut self,
        default_status: Option<TaskStatus>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let status = default_status.unwrap_or(match self.nav {
            NavFilter::InProgress => TaskStatus::InProgress,
            NavFilter::Done => TaskStatus::Done,
            _ => TaskStatus::Todo,
        });
        let priority = match self.nav {
            NavFilter::HighPriority => TaskPriority::High,
            _ => TaskPriority::Normal,
        };
        let due_date = match self.nav {
            NavFilter::Today => Some(today_str()),
            _ => None,
        };
        let initial_tags = match &self.nav {
            NavFilter::Tag(t) => t.clone(),
            _ => String::new(),
        };

        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Titre de la tâche…"));
        cx.subscribe_in(&title, window, |this, _, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                this.submit_create_modal(window, cx);
            }
        })
        .detach();

        let description = cx.new(|cx| {
            TextareaState::new(window, cx)
                .rows(3)
                .placeholder("Notes, contexte ou étapes (optionnel)…")
        });

        let due_str = due_date.clone().unwrap_or_default();
        let due_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("AAAA-MM-JJ");
            if !due_str.is_empty() {
                st.set_value(due_str, window, cx);
            }
            st
        });
        cx.subscribe_in(&due_input, window, |this, st, ev: &InputEvent, window, cx| {
            match ev {
                InputEvent::Change => {
                    let raw = st.read(cx).value().to_string();
                    let trimmed = raw.trim();
                    if let Some(modal) = &mut this.create_modal {
                        if trimmed.is_empty() {
                            modal.due_date = None;
                            modal.due_invalid = false;
                        } else if is_valid_date(trimmed) {
                            modal.due_date = Some(trimmed.to_string());
                            modal.due_invalid = false;
                        } else {
                            modal.due_invalid = true;
                        }
                        cx.notify();
                    }
                }
                InputEvent::PressEnter { .. } => {
                    this.submit_create_modal(window, cx);
                }
                _ => {}
            }
        })
        .detach();

        let tags_input = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("Tags séparés par virgule (ex: dev, urgent)");
            if !initial_tags.is_empty() {
                st.set_value(initial_tags, window, cx);
            }
            st
        });
        cx.subscribe_in(&tags_input, window, |this, _, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                this.submit_create_modal(window, cx);
            }
        })
        .detach();

        self.create_modal = Some(CreateModalState {
            status,
            priority,
            due_date,
            due_invalid: false,
            need_focus: true,
            title,
            description,
            due_input,
            tags_input,
        });
        cx.notify();
    }

    fn close_create_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.create_modal = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn submit_create_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(modal) = &self.create_modal else {
            return;
        };
        let title = modal.title.read(cx).value().to_string();
        if title.trim().is_empty() {
            modal.title.update(cx, |st, cx| st.focus(window, cx));
            return;
        }
        let description = modal.description.read(cx).value().to_string();
        let tags_raw = modal.tags_input.read(cx).value().to_string();
        let tags: Vec<String> = tags_raw
            .split([',', ' '])
            .map(|s| s.trim().trim_start_matches('#').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let status = modal.status;
        let priority = modal.priority;
        let due_date = modal.due_date.clone();

        self.store.update(cx, |s, cx| {
            s.add(title, description, priority, status, due_date, tags, cx)
        });
        self.create_modal = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn ensure_detail_inputs(
        &mut self,
        task: &TaskItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(existing) = &self.detail_inputs {
            if existing.task_id == task.id {
                return;
            }
        }
        let task_id = task.id;
        let initial_title = task.title.clone();
        let initial_desc = task.description.clone();
        let initial_due = task.due_date.clone().unwrap_or_default();

        let title = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("Titre de la tâche");
            st.set_value(initial_title, window, cx);
            st
        });
        cx.subscribe_in(&title, window, move |this, st, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                let val = st.read(cx).value().to_string();
                this.store
                    .update(cx, |s, cx| s.set_title(task_id, val, cx));
            }
        })
        .detach();

        let description = cx.new(|cx| {
            let mut st = TextareaState::new(window, cx)
                .rows(4)
                .placeholder("Ajouter des notes, liens ou contexte…");
            st.set_value(initial_desc, window, cx);
            st
        });
        cx.subscribe_in(
            &description,
            window,
            move |this, st, ev: &InputEvent, _, cx| {
                if matches!(ev, InputEvent::Change) {
                    let val = st.read(cx).value().to_string();
                    this.store
                        .update(cx, |s, cx| s.set_description(task_id, val, cx));
                }
            },
        )
        .detach();

        let due_date = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("AAAA-MM-JJ");
            st.set_value(initial_due, window, cx);
            st
        });
        cx.subscribe_in(&due_date, window, move |this, st, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change | InputEvent::PressEnter { .. }) {
                let raw = st.read(cx).value().to_string();
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    if let Some(det) = &mut this.detail_inputs {
                        det.due_invalid = false;
                    }
                    this.store
                        .update(cx, |s, cx| s.set_due_date(task_id, None, cx));
                } else if is_valid_date(trimmed) {
                    if let Some(det) = &mut this.detail_inputs {
                        det.due_invalid = false;
                    }
                    this.store.update(cx, |s, cx| {
                        s.set_due_date(task_id, Some(trimmed.to_string()), cx)
                    });
                } else if let Some(det) = &mut this.detail_inputs {
                    det.due_invalid = true;
                    cx.notify();
                }
            }
        })
        .detach();

        let tag = cx.new(|cx| InputState::new(window, cx).placeholder("Nouveau tag…"));
        cx.subscribe_in(&tag, window, move |this, st, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                let val = st.read(cx).value().to_string();
                if !val.trim().is_empty() {
                    this.store.update(cx, |s, cx| s.add_tag(task_id, val, cx));
                    st.update(cx, |s, cx| s.set_value("", window, cx));
                }
            }
        })
        .detach();

        let subtask =
            cx.new(|cx| InputState::new(window, cx).placeholder("Ajouter une étape…"));
        cx.subscribe_in(
            &subtask,
            window,
            move |this, st, ev: &InputEvent, window, cx| {
                if matches!(ev, InputEvent::PressEnter { .. }) {
                    let val = st.read(cx).value().to_string();
                    if !val.trim().is_empty() {
                        this.store
                            .update(cx, |s, cx| s.add_subtask(task_id, val, cx));
                        st.update(cx, |s, cx| s.set_value("", window, cx));
                    }
                }
            },
        )
        .detach();

        self.detail_inputs = Some(DetailInputs {
            task_id,
            due_invalid: false,
            title,
            description,
            due_date,
            tag,
            subtask,
        });
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = ui::key(ev);
        let mods = &ev.keystroke.modifiers;

        if key == "escape" {
            if self.create_modal.is_some() {
                self.close_create_modal(window, cx);
            } else if self.selected_id.is_some() {
                self.selected_id = None;
                self.detail_inputs = None;
                window.focus(&self.focus, cx);
                cx.notify();
            } else {
                cx.hide_window(ids::TODO);
            }
            return;
        }

        if mods.control && (key == "f" || key == "k") {
            if self.create_modal.is_none() {
                if let Some(search) = &self.search_input {
                    search.update(cx, |st, cx| st.focus(window, cx));
                }
            }
            return;
        }

        if mods.control && key == "n" {
            self.open_create_modal(None, window, cx);
        }
    }

    fn filtered_tasks(&self, cx: &Context<Self>) -> Vec<TaskItem> {
        let query = self.search_query.trim().to_lowercase();
        let mut items: Vec<TaskItem> = self
            .store
            .read(cx)
            .tasks()
            .iter()
            .filter(|t| {
                let matches_nav = match &self.nav {
                    NavFilter::Focus | NavFilter::Kanban => true,
                    NavFilter::All => true,
                    NavFilter::Today => t.is_due_today_or_overdue(),
                    NavFilter::HighPriority => {
                        t.status != TaskStatus::Done && t.priority == TaskPriority::High
                    }
                    NavFilter::InProgress => t.status == TaskStatus::InProgress,
                    NavFilter::Done => t.status == TaskStatus::Done,
                    NavFilter::Tag(tag) => {
                        t.status != TaskStatus::Done
                            && t.tags.iter().any(|tg| tg.eq_ignore_ascii_case(tag))
                    }
                };
                if !matches_nav {
                    return false;
                }
                if query.is_empty() {
                    return true;
                }
                t.title.to_lowercase().contains(&query)
                    || t.description.to_lowercase().contains(&query)
                    || t.tags.iter().any(|tg| tg.to_lowercase().contains(&query))
            })
            .cloned()
            .collect();

        match self.sort_by {
            SortBy::Priority => items.sort_by_key(|t| (t.priority, t.order)),
            SortBy::DueDate => items.sort_by(|a, b| {
                let da = a.due_date.as_deref().unwrap_or("9999-99-99");
                let db = b.due_date.as_deref().unwrap_or("9999-99-99");
                da.cmp(db)
                    .then_with(|| a.priority.cmp(&b.priority))
                    .then_with(|| a.order.cmp(&b.order))
            }),
            SortBy::Order => items.sort_by_key(|t| t.order),
        }
        items
    }

    fn priority_variant(p: TaskPriority) -> TagVariant {
        match p {
            TaskPriority::High => TagVariant::Danger,
            TaskPriority::Normal => TagVariant::Info,
            TaskPriority::Low => TagVariant::Secondary,
        }
    }

    fn priority_color(p: TaskPriority, cx: &App) -> Hsla {
        match p {
            TaskPriority::High => cx.theme().danger,
            TaskPriority::Normal => cx.theme().primary,
            TaskPriority::Low => cx.theme().muted_foreground,
        }
    }

    fn task_checkbox(
        &self,
        id: impl Into<gpui::ElementId>,
        checked: bool,
        priority: TaskPriority,
        cx: &App,
        on_toggle: impl Fn(&mut App) + 'static,
    ) -> impl IntoElement {
        let border_col = if checked {
            cx.theme().primary
        } else {
            Self::priority_color(priority, cx)
        };
        let bg_col = if checked {
            cx.theme().primary
        } else {
            cx.theme().background
        };
        let check_fg = cx.theme().primary_foreground;

        div()
            .id(id)
            .w(px(18.))
            .h(px(18.))
            .flex_shrink_0()
            .rounded(px(5.))
            .border_1()
            .border_color(border_col)
            .bg(bg_col)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|s| s.opacity(0.8))
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                on_toggle(cx);
            })
            .children(checked.then(|| svg_icon(SVG_CHECK).xsmall().text_color(check_fg)))
    }

    fn due_tag(date_str: &str) -> Tag {
        let (label, urgency) = format_due_label(date_str);
        let variant = match urgency {
            DueUrgency::Overdue => TagVariant::Danger,
            DueUrgency::Today => TagVariant::Warning,
            DueUrgency::Upcoming => TagVariant::Secondary,
        };
        Tag::new().with_variant(variant).xsmall().child(label)
    }

    fn start_focus_on(&mut self, task_id: u64, cx: &mut Context<Self>) {
        self.focus_task_id = Some(task_id);
        self.selected_id = None;
        self.detail_inputs = None;
        self.nav = NavFilter::Focus;
        self.store.update(cx, |s, cx| {
            if let Some(t) = s.get(task_id) {
                if t.status == TaskStatus::Todo {
                    s.set_status(task_id, TaskStatus::InProgress, cx);
                }
            }
        });
        let running = self.pomodoro.read(cx).running;
        if !running {
            self.pomodoro.update(cx, |p, cx| p.toggle(cx));
        }
        cx.notify();
    }

    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let active = store.active_count();
        let total = store.total_count();
        let done = store.done_count();
        let today = store.today_count();
        let high = store.high_count();
        let in_prog = store.in_progress_count();
        let tags = store.all_tags();
        let pct = if total == 0 {
            0.0
        } else {
            (done as f32 / total as f32) * 100.0
        };

        let pomo = self.pomodoro.read(cx);
        let pomo_time = pomo.mmss();
        let pomo_running = pomo.running;
        let pomo_label = pomo.phase.label();
        let pomo_entity = self.pomodoro.clone();

        let count_pill = |n: usize| {
            move |_: &mut Window, cx: &mut App| {
                div()
                    .px_1p5()
                    .py_0p5()
                    .rounded_full()
                    .bg(cx.theme().muted)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(n.to_string())
            }
        };

        let views_menu = SidebarMenu::new()
            .child(
                SidebarMenuItem::new("Focus")
                    .icon(svg_icon(SVG_SPARKLES))
                    .active(self.nav == NavFilter::Focus)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.nav = NavFilter::Focus;
                        cx.notify();
                    })),
            )
            .child(
                SidebarMenuItem::new("Liste Todo")
                    .icon(svg_icon(SVG_LIST))
                    .active(self.nav == NavFilter::All)
                    .suffix(count_pill(active))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.nav = NavFilter::All;
                        cx.notify();
                    })),
            )
            .child(
                SidebarMenuItem::new("Tableau Kanban")
                    .icon(svg_icon(SVG_KANBAN))
                    .active(self.nav == NavFilter::Kanban)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.nav = NavFilter::Kanban;
                        cx.notify();
                    })),
            );

        let filters_menu = SidebarMenu::new()
            .child(
                SidebarMenuItem::new("Aujourd'hui")
                    .icon(svg_icon(SVG_CALENDAR))
                    .active(self.nav == NavFilter::Today)
                    .suffix(count_pill(today))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.nav = NavFilter::Today;
                        cx.notify();
                    })),
            )
            .child(
                SidebarMenuItem::new("Haute priorité")
                    .icon(svg_icon(SVG_FLAME))
                    .active(self.nav == NavFilter::HighPriority)
                    .suffix(count_pill(high))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.nav = NavFilter::HighPriority;
                        cx.notify();
                    })),
            )
            .child(
                SidebarMenuItem::new("En cours")
                    .icon(svg_icon(SVG_CLOCK))
                    .active(self.nav == NavFilter::InProgress)
                    .suffix(count_pill(in_prog))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.nav = NavFilter::InProgress;
                        cx.notify();
                    })),
            )
            .child(
                SidebarMenuItem::new("Terminées")
                    .icon(svg_icon(SVG_CHECK_CIRCLE))
                    .active(self.nav == NavFilter::Done)
                    .suffix(count_pill(done))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.nav = NavFilter::Done;
                        cx.notify();
                    })),
            );

        let mut sidebar = Sidebar::new("todo-sidebar")
            .w(px(236.))
            .collapsible(false)
            .header(
                SidebarHeader::new().child(
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .px_1()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(svg_icon(SVG_CHECK_CIRCLE).text_color(cx.theme().primary))
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(cx.theme().sidebar_foreground)
                                        .child("Kumi Tasks"),
                                ),
                        )
                        .child(Badge::new().count(active)),
                ),
            )
            .child(SidebarGroup::new("Vues").child(views_menu))
            .child(SidebarGroup::new("Filtres").child(filters_menu));

        if !tags.is_empty() {
            let mut tags_menu = SidebarMenu::new();
            for (tag_name, count) in tags {
                let active_tag = self.nav == NavFilter::Tag(tag_name.clone());
                let tag_clone = tag_name.clone();
                tags_menu = tags_menu.child(
                    SidebarMenuItem::new(format!("#{tag_name}"))
                        .icon(svg_icon(SVG_TAG))
                        .active(active_tag)
                        .suffix(count_pill(count))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.nav = NavFilter::Tag(tag_clone.clone());
                            cx.notify();
                        })),
                );
            }
            sidebar = sidebar.child(SidebarGroup::new("Tags").child(tags_menu));
        }

        sidebar.footer(
            SidebarFooter::new().child(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .p_2p5()
                            .rounded(px(8.))
                            .bg(cx.theme().muted)
                            .border_1()
                            .border_color(cx.theme().border)
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .text_xs()
                                    .child(
                                        div()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(cx.theme().foreground)
                                            .child("Progression"),
                                    )
                                    .child(
                                        div()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!("{done}/{total}")),
                                    ),
                            )
                            .child(Progress::new("sidebar-global-prog").value(pct)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .p_2p5()
                            .rounded(px(8.))
                            .bg(cx.theme().muted)
                            .border_1()
                            .border_color(if pomo_running {
                                cx.theme().primary
                            } else {
                                cx.theme().border
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!("Pomodoro · {pomo_label}")),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(FontWeight::BOLD)
                                            .font_features(ui::tabular())
                                            .text_color(cx.theme().foreground)
                                            .child(pomo_time),
                                    ),
                            )
                            .child(
                                Button::new("sidebar-pomo-toggle")
                                    .small()
                                    .primary()
                                    .icon(svg_icon(if pomo_running {
                                        SVG_PAUSE
                                    } else {
                                        SVG_PLAY
                                    }))
                                    .on_click(move |_, _, cx| {
                                        pomo_entity.update(cx, |p, cx| p.toggle(cx));
                                    }),
                            ),
                    ),
            ),
        )
    }

    fn render_task_row(&mut self, item: &TaskItem, cx: &mut Context<Self>) -> impl IntoElement {
        let id = item.id;
        let selected = self.selected_id == Some(id);
        let done = item.status == TaskStatus::Done;
        let in_progress = item.status == TaskStatus::InProgress;
        let (sub_done, sub_total) = item.subtask_progress();
        let store = self.store.clone();
        let store_prio = self.store.clone();

        div()
            .id(("task-row", id as usize))
            .w_full()
            .flex()
            .flex_row()
            .items_center()
            .gap_3()
            .px_3p5()
            .py_2p5()
            .rounded(px(10.))
            .cursor_pointer()
            .bg(if selected {
                cx.theme().list_active
            } else {
                cx.theme().background
            })
            .border_1()
            .border_color(if selected {
                cx.theme().primary
            } else if in_progress {
                cx.theme().warning.opacity(0.6)
            } else {
                cx.theme().border
            })
            .hover(|s| s.bg(cx.theme().list_hover))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected_id = Some(id);
                cx.notify();
            }))
            .child(self.task_checkbox(
                ("row-cb", id as usize),
                done,
                item.priority,
                cx,
                move |cx| {
                    store.update(cx, |s, cx| s.toggle_done(id, cx));
                },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(if done {
                                        cx.theme().muted_foreground
                                    } else {
                                        cx.theme().foreground
                                    })
                                    .when(done, |d| d.line_through())
                                    .truncate()
                                    .child(item.title.clone()),
                            ),
                    )
                    .when(
                        sub_total > 0 || !item.tags.is_empty() || !item.description.is_empty(),
                        |d| {
                            d.child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .when(sub_total > 0, |row| {
                                        row.child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(format!("✓ {sub_done}/{sub_total}")),
                                        )
                                    })
                                    .children(item.tags.iter().take(3).map(|t| {
                                        Tag::new()
                                            .with_variant(TagVariant::Secondary)
                                            .xsmall()
                                            .child(format!("#{t}"))
                                    })),
                            )
                        },
                    ),
            )
            .children(in_progress.then(|| {
                Tag::new()
                    .with_variant(TagVariant::Warning)
                    .xsmall()
                    .child("En cours")
            }))
            .children(item.due_date.as_deref().map(Self::due_tag))
            .child(
                div()
                    .id(("row-prio", id as usize))
                    .cursor_pointer()
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        store_prio.update(cx, |s, cx| s.cycle_priority(id, cx));
                    })
                    .child(
                        Tag::new()
                            .with_variant(Self::priority_variant(item.priority))
                            .xsmall()
                            .child(item.priority.label()),
                    ),
            )
    }

    fn render_focus_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.search_query.trim().to_lowercase();
        let focus_items: Vec<TaskItem> = self
            .store
            .read(cx)
            .focus_tasks()
            .into_iter()
            .filter(|t| {
                query.is_empty()
                    || t.title.to_lowercase().contains(&query)
                    || t.tags.iter().any(|tg| tg.to_lowercase().contains(&query))
            })
            .cloned()
            .collect();

        let focused_task = self
            .focus_task_id
            .and_then(|id| self.store.read(cx).get(id).cloned())
            .or_else(|| focus_items.first().cloned());

        let pomo = self.pomodoro.read(cx);
        let pomo_time = pomo.mmss();
        let pomo_phase = pomo.phase.label();
        let pomo_running = pomo.running;
        let pomo_prog = pomo.progress() * 100.0;
        let pomo_toggle = self.pomodoro.clone();
        let pomo_reset = self.pomodoro.clone();
        let pomo_skip = self.pomodoro.clone();

        let hero = div()
            .w_full()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .rounded(px(12.))
            .bg(cx.theme().muted)
            .border_1()
            .border_color(if pomo_running {
                cx.theme().primary
            } else {
                cx.theme().border
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap_4()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        Tag::new()
                                            .with_variant(if pomo_running {
                                                TagVariant::Primary
                                            } else {
                                                TagVariant::Secondary
                                            })
                                            .xsmall()
                                            .child(format!("Focus · {pomo_phase}")),
                                    )
                                    .when_some(
                                        focused_task.as_ref().and_then(|t| t.due_date.as_deref()),
                                        |d, date| d.child(Self::due_tag(date)),
                                    ),
                            )
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(cx.theme().foreground)
                                    .truncate()
                                    .child(
                                        focused_task
                                            .as_ref()
                                            .map(|t| t.title.clone())
                                            .unwrap_or_else(|| {
                                                "Aucune tâche active — créez-en une avec + Nouvelle tâche"
                                                    .to_string()
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .text_2xl()
                                    .font_weight(FontWeight::BOLD)
                                    .font_features(ui::tabular())
                                    .text_color(cx.theme().foreground)
                                    .mr_2()
                                    .child(pomo_time),
                            )
                            .child(
                                Button::new("focus-pomo-play")
                                    .small()
                                    .primary()
                                    .icon(svg_icon(if pomo_running {
                                        SVG_PAUSE
                                    } else {
                                        SVG_PLAY
                                    }))
                                    .label(if pomo_running { "Pause" } else { "Démarrer" })
                                    .on_click(move |_, _, cx| {
                                        pomo_toggle.update(cx, |p, cx| p.toggle(cx));
                                    }),
                            )
                            .child(
                                Button::new("focus-pomo-skip")
                                    .small()
                                    .ghost()
                                    .label("Passer")
                                    .on_click(move |_, _, cx| {
                                        pomo_skip.update(cx, |p, cx| p.skip(cx));
                                    }),
                            )
                            .child(
                                Button::new("focus-pomo-reset")
                                    .small()
                                    .ghost()
                                    .label("Reset")
                                    .on_click(move |_, _, cx| {
                                        pomo_reset.update(cx, |p, cx| p.reset(cx));
                                    }),
                            ),
                    ),
            )
            .child(Progress::new("focus-pomo-progress").value(pomo_prog));

        let mut next_list = div().flex().flex_col().gap_2().child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .pt_1()
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(cx.theme().muted_foreground)
                        .child("PROCHAINES TÂCHES PRIORITAIRES (3–5)"),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!(
                            "{} active(s) au total",
                            self.store.read(cx).active_count()
                        )),
                ),
        );

        if focus_items.is_empty() {
            next_list = next_list.child(
                div()
                    .py_12()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Tout est à jour !")
                    .child(
                        Button::new("focus-empty-new")
                            .small()
                            .primary()
                            .icon(svg_icon(SVG_PLUS))
                            .label("Nouvelle tâche")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_create_modal(None, window, cx);
                            })),
                    ),
            );
        } else {
            for item in focus_items {
                let id = item.id;
                let row = self.render_task_row(&item, cx);
                next_list = next_list.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(div().flex_1().min_w_0().child(row))
                        .child(
                            Button::new(("focus-start", id as usize))
                                .small()
                                .secondary()
                                .icon(svg_icon(SVG_PLAY))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.start_focus_on(id, cx);
                                })),
                        ),
                );
            }
        }

        div().flex().flex_col().gap_4().child(hero).child(next_list)
    }

    fn render_list_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let items = self.filtered_tasks(cx);
        let only_done = self.nav == NavFilter::Done;

        let active: Vec<TaskItem> = items
            .iter()
            .filter(|t| t.status != TaskStatus::Done)
            .cloned()
            .collect();
        let done: Vec<TaskItem> = items
            .iter()
            .filter(|t| t.status == TaskStatus::Done)
            .cloned()
            .collect();

        let mut col = div().flex().flex_col().gap_2();

        if items.is_empty() {
            return col.child(
                div()
                    .py_12()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Aucune tâche dans cette vue.")
                    .child(
                        Button::new("list-empty-new")
                            .small()
                            .primary()
                            .icon(svg_icon(SVG_PLUS))
                            .label("Nouvelle tâche")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_create_modal(None, window, cx);
                            })),
                    ),
            );
        }

        if !only_done {
            for item in active {
                col = col.child(self.render_task_row(&item, cx));
            }
        }

        if !done.is_empty() && (only_done || self.nav == NavFilter::All) {
            let store_clear = self.store.clone();
            col = col.child(
                div()
                    .pt_3()
                    .pb_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("TERMINÉES ({})", done.len())),
                    )
                    .child(
                        Button::new("list-clear-done")
                            .xsmall()
                            .ghost()
                            .label("Tout effacer")
                            .on_click(move |_, _, cx| {
                                store_clear.update(cx, |s, cx| s.clear_done(cx));
                            }),
                    ),
            );
            for item in done {
                col = col.child(self.render_task_row(&item, cx));
            }
        }

        col
    }

    fn render_kanban_view(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let items = self.filtered_tasks(cx);
        let columns = [TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done];
        let mut board = div().flex().flex_row().gap_3().w_full().items_start();

        for col_status in columns {
            let col_items: Vec<TaskItem> = items
                .iter()
                .filter(|t| t.status == col_status)
                .cloned()
                .collect();

            let mut col = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_2()
                .p_3()
                .rounded(px(12.))
                .bg(cx.theme().muted)
                .border_1()
                .border_color(cx.theme().border)
                .min_h(px(520.))
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .pb_1()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_2()
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(cx.theme().foreground)
                                        .child(col_status.label()),
                                )
                                .child(
                                    Tag::new()
                                        .with_variant(TagVariant::Secondary)
                                        .xsmall()
                                        .child(col_items.len().to_string()),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .gap_1()
                                .when(
                                    col_status == TaskStatus::Done && !col_items.is_empty(),
                                    |d| {
                                        let s_clear = self.store.clone();
                                        d.child(
                                            Button::new("kb-clear-done")
                                                .xsmall()
                                                .ghost()
                                                .label("Vider")
                                                .on_click(move |_, _, cx| {
                                                    s_clear.update(cx, |s, cx| s.clear_done(cx));
                                                }),
                                        )
                                    },
                                )
                                .child(
                                    Button::new(("kb-add-col", col_status as usize))
                                        .xsmall()
                                        .ghost()
                                        .icon(svg_icon(SVG_PLUS))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            this.open_create_modal(Some(col_status), window, cx);
                                        })),
                                ),
                        ),
                );

            if col_items.is_empty() {
                col = col.child(
                    div()
                        .py_10()
                        .flex()
                        .justify_center()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("Aucune carte"),
                );
            }

            for item in col_items {
                let id = item.id;
                let selected = self.selected_id == Some(id);
                let (sub_done, sub_total) = item.subtask_progress();
                let s_left = self.store.clone();
                let s_right = self.store.clone();
                let s_prio = self.store.clone();

                let mut card = div()
                    .id(("kb-card", id as usize))
                    .w_full()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .p_3()
                    .rounded(px(9.))
                    .cursor_pointer()
                    .bg(if selected {
                        cx.theme().list_active
                    } else {
                        cx.theme().background
                    })
                    .border_1()
                    .border_color(if selected {
                        cx.theme().primary
                    } else {
                        cx.theme().border
                    })
                    .hover(|s| s.border_color(cx.theme().primary.opacity(0.6)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_id = Some(id);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(if col_status == TaskStatus::Done {
                                cx.theme().muted_foreground
                            } else {
                                cx.theme().foreground
                            })
                            .when(col_status == TaskStatus::Done, |d| d.line_through())
                            .line_clamp(2)
                            .child(item.title.clone()),
                    );

                if sub_total > 0 || !item.tags.is_empty() || item.due_date.is_some() {
                    card = card.child(
                        div()
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .items_center()
                            .gap_1p5()
                            .children(item.due_date.as_deref().map(Self::due_tag))
                            .when(sub_total > 0, |d| {
                                d.child(
                                    Tag::new()
                                        .with_variant(TagVariant::Secondary)
                                        .xsmall()
                                        .child(format!("✓ {sub_done}/{sub_total}")),
                                )
                            })
                            .children(item.tags.iter().take(2).map(|t| {
                                Tag::new()
                                    .with_variant(TagVariant::Secondary)
                                    .xsmall()
                                    .child(format!("#{t}"))
                            })),
                    );
                }

                let mut nav_btns = div().flex().flex_row().items_center().gap_1();
                if col_status != TaskStatus::Todo {
                    nav_btns = nav_btns.child(
                        Button::new(("kb-left", id as usize))
                            .xsmall()
                            .ghost()
                            .icon(svg_icon(SVG_ARROW_LEFT))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                s_left.update(cx, |s, cx| s.move_status(id, -1, cx));
                            }),
                    );
                }
                if col_status != TaskStatus::Done {
                    nav_btns = nav_btns.child(
                        Button::new(("kb-right", id as usize))
                            .xsmall()
                            .ghost()
                            .icon(svg_icon(SVG_ARROW_RIGHT))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                s_right.update(cx, |s, cx| s.move_status(id, 1, cx));
                            }),
                    );
                }

                card = card.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_between()
                        .pt_0p5()
                        .child(
                            div()
                                .id(("kb-prio", id as usize))
                                .cursor_pointer()
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    s_prio.update(cx, |s, cx| s.cycle_priority(id, cx));
                                })
                                .child(
                                    Tag::new()
                                        .with_variant(Self::priority_variant(item.priority))
                                        .xsmall()
                                        .child(item.priority.label()),
                                ),
                        )
                        .child(nav_btns),
                );

                col = col.child(card);
            }

            board = board.child(col);
        }

        board
    }

    fn render_create_modal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let modal = self.create_modal.as_mut()?;
        if modal.need_focus {
            modal.need_focus = false;
            modal.title.update(cx, |st, cx| st.focus(window, cx));
        }

        let title_state = modal.title.clone();
        let desc_state = modal.description.clone();
        let due_input_state = modal.due_input.clone();
        let tags_state = modal.tags_input.clone();
        let cur_status = modal.status;
        let cur_prio = modal.priority;
        let cur_due = modal.due_date.clone();
        let cur_due_invalid = modal.due_invalid;

        let mut status_bar = div().flex().flex_row().items_center().gap_1p5();
        for status in [TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done] {
            let active = cur_status == status;
            let mut btn = Button::new(("mod-status", status as usize))
                .small()
                .label(status.label())
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(m) = &mut this.create_modal {
                        m.status = status;
                        cx.notify();
                    }
                }));
            btn = if active {
                btn.primary()
            } else {
                btn.secondary()
            };
            status_bar = status_bar.child(btn);
        }

        let mut prio_bar = div().flex().flex_row().items_center().gap_1p5();
        for prio in [TaskPriority::High, TaskPriority::Normal, TaskPriority::Low] {
            let active = cur_prio == prio;
            let mut btn = Button::new(("mod-prio", prio as usize))
                .small()
                .label(prio.label())
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(m) = &mut this.create_modal {
                        m.priority = prio;
                        cx.notify();
                    }
                }));
            btn = if active {
                btn.primary()
            } else {
                btn.secondary()
            };
            prio_bar = prio_bar.child(btn);
        }

        let due_presets = [
            ("Aucune", None),
            ("Aujourd'hui", Some(today_str())),
            ("Demain", Some(in_days_str(1))),
            ("+3 jours", Some(in_days_str(3))),
        ];
        let mut due_row = div().flex().flex_row().items_center().gap_1p5();
        for (ix, (label, val)) in due_presets.into_iter().enumerate() {
            let active = cur_due == val;
            let due_in = due_input_state.clone();
            let mut btn = Button::new(("mod-due", ix))
                .xsmall()
                .label(label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(m) = &mut this.create_modal {
                        m.due_date = val.clone();
                        m.due_invalid = false;
                        due_in.update(cx, |st, cx| {
                            st.set_value(val.clone().unwrap_or_default(), window, cx)
                        });
                        cx.notify();
                    }
                }));
            btn = if active { btn.primary() } else { btn.ghost() };
            due_row = due_row.child(btn);
        }

        let card = div()
            .occlude()
            .w(px(520.))
            .flex()
            .flex_col()
            .rounded(px(14.))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .border_1()
            .border_color(cx.theme().border)
            .shadow_lg()
            .child(
                div()
                    .px_5()
                    .py_3p5()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(svg_icon(SVG_PLUS).text_color(cx.theme().primary))
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(cx.theme().foreground)
                                    .child("Nouvelle tâche"),
                            ),
                    )
                    .child(
                        Button::new("mod-close")
                            .xsmall()
                            .ghost()
                            .icon(svg_icon(SVG_CLOSE))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.close_create_modal(window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .p_5()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().muted_foreground)
                                    .child("TITRE"),
                            )
                            .child(Input::new(&title_state)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap_4()
                            .child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(cx.theme().muted_foreground)
                                            .child("COLONNE / STATUT"),
                                    )
                                    .child(status_bar),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_1p5()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(cx.theme().muted_foreground)
                                            .child("PRIORITÉ"),
                                    )
                                    .child(prio_bar),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(cx.theme().muted_foreground)
                                            .child("ÉCHÉANCE"),
                                    )
                                    .children(if cur_due_invalid {
                                        Some(
                                            Tag::new()
                                                .with_variant(TagVariant::Danger)
                                                .xsmall()
                                                .child("Format AAAA-MM-JJ")
                                                .into_any_element(),
                                        )
                                    } else {
                                        cur_due
                                            .as_deref()
                                            .map(|d| Self::due_tag(d).into_any_element())
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(due_row)
                                    .child(
                                        div()
                                            .w(px(130.))
                                            .rounded(px(8.))
                                            .when(cur_due_invalid, |d| {
                                                d.border_1().border_color(cx.theme().danger)
                                            })
                                            .child(Input::new(&due_input_state).small()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().muted_foreground)
                                    .child("TAGS"),
                            )
                            .child(Input::new(&tags_state).small()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().muted_foreground)
                                    .child("NOTES"),
                            )
                            .child(Textarea::new(&desc_state).h(px(88.))),
                    ),
            )
            .child(
                div()
                    .px_5()
                    .py_3p5()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Entrée pour valider · Échap pour annuler"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("mod-cancel")
                                    .small()
                                    .ghost()
                                    .label("Annuler")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.close_create_modal(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("mod-submit")
                                    .small()
                                    .primary()
                                    .icon(svg_icon(SVG_PLUS))
                                    .label("Créer la tâche")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.submit_create_modal(window, cx);
                                    })),
                            ),
                    ),
            )
            .with_animation("todo-modal-in", ui::anim_pop(), |el, d| el.opacity(d));

        Some(
            div()
                .id("todo-create-modal-overlay")
                .absolute()
                .inset_0()
                .bg(cx.theme().background.opacity(0.65))
                .flex()
                .items_center()
                .justify_center()
                .on_click(cx.listener(|this, _, window, cx| {
                    cx.stop_propagation();
                    this.close_create_modal(window, cx);
                }))
                .child(card),
        )
    }

    fn render_detail_modal(
        &mut self,
        task: TaskItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.ensure_detail_inputs(&task, window, cx);
        let inputs = self.detail_inputs.as_ref().unwrap();
        let title_state = inputs.title.clone();
        let desc_state = inputs.description.clone();
        let due_state = inputs.due_date.clone();
        let tag_state = inputs.tag.clone();
        let sub_state = inputs.subtask.clone();
        let due_invalid = inputs.due_invalid;

        let id = task.id;
        let (sub_done, sub_total) = task.subtask_progress();
        let sub_pct = if sub_total == 0 {
            0.0
        } else {
            (sub_done as f32 / sub_total as f32) * 100.0
        };

        let mut status_bar = div().flex().flex_row().items_center().gap_1();
        for status in [TaskStatus::Todo, TaskStatus::InProgress, TaskStatus::Done] {
            let active = task.status == status;
            let store = self.store.clone();
            let mut btn = Button::new(("det-status", status as usize))
                .small()
                .label(status.label())
                .on_click(move |_, _, cx| {
                    store.update(cx, |s, cx| s.set_status(id, status, cx));
                });
            btn = if active { btn.primary() } else { btn.ghost() };
            status_bar = status_bar.child(btn);
        }

        let mut prio_bar = div().flex().flex_row().items_center().gap_1p5();
        for prio in [TaskPriority::High, TaskPriority::Normal, TaskPriority::Low] {
            let active = task.priority == prio;
            let store = self.store.clone();
            let mut btn = Button::new(("det-prio", prio as usize))
                .small()
                .label(prio.label())
                .on_click(move |_, _, cx| {
                    store.update(cx, |s, cx| s.set_priority(id, prio, cx));
                });
            btn = if active {
                btn.primary()
            } else {
                btn.secondary()
            };
            prio_bar = prio_bar.child(btn);
        }

        let due_presets = [
            ("Auj.", Some(today_str())),
            ("Dem.", Some(in_days_str(1))),
            ("+3j", Some(in_days_str(3))),
            ("Aucune", None),
        ];
        let mut due_btns = div().flex().flex_row().items_center().gap_1();
        for (ix, (label, val)) in due_presets.into_iter().enumerate() {
            let due_input = due_state.clone();
            let active = task.due_date == val;
            let mut btn = Button::new(("det-due-preset", ix))
                .xsmall()
                .label(label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    let next = val.clone();
                    if let Some(det) = &mut this.detail_inputs {
                        det.due_invalid = false;
                    }
                    this.store
                        .update(cx, |s, cx| s.set_due_date(id, next.clone(), cx));
                    due_input.update(cx, |st, cx| {
                        st.set_value(next.unwrap_or_default(), window, cx)
                    });
                }));
            btn = if active { btn.primary() } else { btn.ghost() };
            due_btns = due_btns.child(btn);
        }

        let mut tags_row = div().flex().flex_row().flex_wrap().items_center().gap_1p5();
        for (ix, tag) in task.tags.iter().enumerate() {
            let store = self.store.clone();
            let tag_name = tag.clone();
            tags_row = tags_row.child(
                Button::new(("det-tag-rm", ix))
                    .xsmall()
                    .secondary()
                    .icon(svg_icon(SVG_CLOSE))
                    .label(format!("#{tag}"))
                    .on_click(move |_, _, cx| {
                        store.update(cx, |s, cx| s.remove_tag(id, &tag_name, cx));
                    }),
            );
        }

        let mut sub_list = div().flex().flex_col().gap_1p5();
        for sub in &task.subtasks {
            let sub_id = sub.id;
            let s_tog = self.store.clone();
            let s_del = self.store.clone();
            sub_list = sub_list.child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_2p5()
                    .py_1p5()
                    .rounded(px(7.))
                    .bg(cx.theme().muted)
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(self.task_checkbox(
                        ("det-sub-cb", sub_id as usize),
                        sub.done,
                        TaskPriority::Normal,
                        cx,
                        move |cx| {
                            s_tog.update(cx, |s, cx| s.toggle_subtask(id, sub_id, cx));
                        },
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .text_color(if sub.done {
                                cx.theme().muted_foreground
                            } else {
                                cx.theme().foreground
                            })
                            .when(sub.done, |d| d.line_through())
                            .truncate()
                            .child(sub.title.clone()),
                    )
                    .child(
                        Button::new(("det-sub-del", sub_id as usize))
                            .xsmall()
                            .ghost()
                            .icon(svg_icon(SVG_CLOSE))
                            .on_click(move |_, _, cx| {
                                s_del.update(cx, |s, cx| s.remove_subtask(id, sub_id, cx));
                            }),
                    ),
            );
        }

        let store_del = self.store.clone();

        let card = div()
            .occlude()
            .w(px(660.))
            .h(px(640.))
            .flex()
            .flex_col()
            .rounded(px(14.))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .border_1()
            .border_color(cx.theme().border)
            .shadow_lg()
            .child(
                div()
                    .px_5()
                    .py_3p5()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(status_bar)
                    .child(
                        Button::new("det-close")
                            .xsmall()
                            .ghost()
                            .icon(svg_icon(SVG_CLOSE))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.selected_id = None;
                                this.detail_inputs = None;
                                window.focus(&this.focus, cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .p_5()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().muted_foreground)
                                    .child("TITRE"),
                            )
                            .child(Input::new(&title_state)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().muted_foreground)
                                    .child("PRIORITÉ"),
                            )
                            .child(prio_bar),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(cx.theme().muted_foreground)
                                            .child("ÉCHÉANCE"),
                                    )
                                    .children(if due_invalid {
                                        Some(
                                            Tag::new()
                                                .with_variant(TagVariant::Danger)
                                                .xsmall()
                                                .child("Format AAAA-MM-JJ")
                                                .into_any_element(),
                                        )
                                    } else {
                                        task.due_date
                                            .as_deref()
                                            .map(|d| Self::due_tag(d).into_any_element())
                                    }),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(due_btns)
                                    .child(
                                        div()
                                            .w(px(140.))
                                            .rounded(px(8.))
                                            .when(due_invalid, |d| {
                                                d.border_1().border_color(cx.theme().danger)
                                            })
                                            .child(Input::new(&due_state).small()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().muted_foreground)
                                    .child("TAGS"),
                            )
                            .when(!task.tags.is_empty(), |d| d.child(tags_row))
                            .child(Input::new(&tag_state).small()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(cx.theme().muted_foreground)
                                            .child(format!("SOUS-TÂCHES ({sub_done}/{sub_total})")),
                                    ),
                            )
                            .when(sub_total > 0, |d| {
                                d.child(Progress::new("det-sub-prog").value(sub_pct))
                            })
                            .when(!task.subtasks.is_empty(), |d| d.child(sub_list))
                            .child(Input::new(&sub_state).small()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1p5()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().muted_foreground)
                                    .child("NOTES"),
                            )
                            .child(Textarea::new(&desc_state).h(px(104.))),
                    ),
            )
            .child(
                div()
                    .px_5()
                    .py_3p5()
                    .border_t_1()
                    .border_color(cx.theme().border)
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
                                Button::new("det-focus-btn")
                                    .small()
                                    .primary()
                                    .icon(svg_icon(SVG_PLAY))
                                    .label("Lancer Focus")
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.start_focus_on(id, cx);
                                    })),
                            )
                            .child(
                                Button::new("det-delete-btn")
                                    .small()
                                    .danger()
                                    .icon(svg_icon(SVG_TRASH))
                                    .label("Supprimer")
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        store_del.update(cx, |s, cx| s.remove(id, cx));
                                        this.selected_id = None;
                                        this.detail_inputs = None;
                                        window.focus(&this.focus, cx);
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(
                        Button::new("det-done-close")
                            .small()
                            .secondary()
                            .label("Fermer")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.selected_id = None;
                                this.detail_inputs = None;
                                window.focus(&this.focus, cx);
                                cx.notify();
                            })),
                    ),
            )
            .with_animation("todo-detail-modal-in", ui::anim_pop(), |el, d| {
                el.opacity(d)
            });

        div()
            .id("todo-detail-modal-overlay")
            .absolute()
            .inset_0()
            .bg(cx.theme().background.opacity(0.65))
            .flex()
            .items_center()
            .justify_center()
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.selected_id = None;
                this.detail_inputs = None;
                window.focus(&this.focus, cx);
                cx.notify();
            }))
            .child(card)
    }
}

impl Render for TodoWidget {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ensure_components_init(cx);
        let active_theme_name = theme::current().name;
        if self.last_theme != active_theme_name {
            self.last_theme = active_theme_name;
            sync_gpui_theme(cx);
        }

        let search_state = self.ensure_search_input(window, cx);

        if let Some(sel) = self.selected_id {
            if self.store.read(cx).get(sel).is_none() {
                self.selected_id = None;
                self.detail_inputs = None;
            }
        }

        let selected_task = self
            .selected_id
            .and_then(|id| self.store.read(cx).get(id).cloned());

        let sidebar = self.render_sidebar(cx);

        let top_toolbar = div()
            .px_5()
            .py_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::BOLD)
                    .text_color(cx.theme().foreground)
                    .truncate()
                    .child(self.nav.title()),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div().w(px(200.)).child(
                            Input::new(&search_state)
                                .small()
                                .prefix(svg_icon(SVG_SEARCH).small()),
                        ),
                    )
                    .child(
                        Button::new("toolbar-sort")
                            .small()
                            .secondary()
                            .icon(svg_icon(SVG_SORT))
                            .label(self.sort_by.label())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sort_by = this.sort_by.next();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("toolbar-new-task")
                            .small()
                            .primary()
                            .icon(svg_icon(SVG_PLUS))
                            .label("Nouvelle tâche")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_create_modal(None, window, cx);
                            })),
                    )
                    .child(
                        Button::new("toolbar-close")
                            .small()
                            .ghost()
                            .icon(svg_icon(SVG_CLOSE))
                            .on_click(|_, _, cx| cx.hide_window(ids::TODO)),
                    ),
            );

        let body_content = match self.nav {
            NavFilter::Focus => self.render_focus_view(cx).into_any_element(),
            NavFilter::Kanban => self.render_kanban_view(cx).into_any_element(),
            _ => self.render_list_view(cx).into_any_element(),
        };

        let center_pane = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(top_toolbar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .p_5()
                    .child(body_content),
            );

        let modal_el = self.render_create_modal(window, cx);

        let mut app_shell = div()
            .occlude()
            .relative()
            .w(px(1100.))
            .h(px(720.))
            .flex()
            .flex_row()
            .overflow_hidden()
            .rounded(px(14.))
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .border_1()
            .border_color(cx.theme().border)
            .shadow_lg()
            .child(sidebar)
            .child(center_pane);

        if let Some(modal) = modal_el {
            app_shell = app_shell.child(modal);
        } else if let Some(task) = selected_task {
            let detail_modal = self.render_detail_modal(task, window, cx);
            app_shell = app_shell.child(detail_modal);
        }

        let app_shell = app_shell.with_animation("todo-open", ui::anim_pop(), |el, delta| {
            el.opacity(delta)
        });

        ui::backdrop("todo-bg", ids::TODO, 0.52)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                this.on_key(ev, window, cx);
            }))
            .flex()
            .items_center()
            .justify_center()
            .child(app_shell)
    }
}
