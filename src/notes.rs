//! 3-pane Markdown Note-Taking workspace (`kumi-notes`).
//!
//! Layout inspired by Bear / Apple Notes:
//!   1. Sidebar (`All Notes`, `Pinned`, dynamic `#tags`)
//!   2. Searchable notes list (`Input` search + pinned-first sorting)
//!   3. Editor & live Markdown preview (`Edit` / `Split` / `Preview`)
//!
//! Built with `gpui-component` (`Sidebar`, `Input`, `Textarea`, `TextView`,
//! `Button`, `Tag`, `Badge`, `ScrollableElement`) and synchronized with Kumi's
//! active theme palette.

use std::time::Duration;

use gpui::prelude::*;
use gpui::{
    div, px, relative, AnimationExt, App, Context, Entity, FocusHandle, FontWeight, KeyDownEvent,
    Window, WindowId,
};
use gpui_component::badge::Badge;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputEvent, InputState, Textarea, TextareaState};
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::sidebar::{
    Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem,
};
use gpui_component::tag::{Tag, TagVariant};
use gpui_component::text::TextView;
use gpui_component::{ActiveTheme as _, Sizable as _};
use kumizo_kit::prelude::*;

use crate::{
    kit::{
        ensure_components_init, hsla, inset_row_bg, panel_bg, raised_bg, surface_bg, svg_icon,
        sync_if_needed, SVG_CHECK, SVG_CLOSE, SVG_COLUMNS, SVG_COPY, SVG_EDIT, SVG_EYE,
        SVG_FILE_TEXT, SVG_PIN, SVG_PLUS, SVG_SEARCH, SVG_TAG, SVG_TRASH,
    },
    notes_store::{self, NoteItem, NotesStore},
    theme,
    ui::{self, ids},
};

#[derive(Clone, PartialEq, Eq)]
enum NoteFilter {
    All,
    Pinned,
    Tag(String),
}

impl NoteFilter {
    fn matches(&self, note: &NoteItem) -> bool {
        match self {
            NoteFilter::All => true,
            NoteFilter::Pinned => note.pinned,
            NoteFilter::Tag(tag) => note.tags.iter().any(|t| t.eq_ignore_ascii_case(tag)),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Edit,
    Split,
    Preview,
}

impl ViewMode {
    fn next(self) -> Self {
        match self {
            ViewMode::Edit => ViewMode::Split,
            ViewMode::Split => ViewMode::Preview,
            ViewMode::Preview => ViewMode::Edit,
        }
    }
}

struct EditorInputs {
    note_id: String,
    disk_revision: u64,
    suppress_sync: bool,
    title: Entity<InputState>,
    body: Entity<TextareaState>,
    tag: Entity<InputState>,
}

pub struct NotesWidget {
    store: Entity<NotesStore>,
    clipboard: Entity<Clipboard>,
    filter: NoteFilter,
    view_mode: ViewMode,
    selected_id: Option<String>,
    search_query: String,
    confirm_delete_id: Option<String>,
    copied_gen: u64,
    copied_feedback: bool,
    last_theme: &'static str,
    last_window_id: Option<WindowId>,
    need_body_focus: bool,
    need_title_focus: bool,

    search_input: Option<Entity<InputState>>,
    editor: Option<EditorInputs>,
    focus: FocusHandle,
}

impl NotesWidget {
    pub fn new(cx: &mut Context<Self>) -> Self {
        ensure_components_init(cx);
        let mut last_theme = theme::current().name;
        sync_if_needed(&mut last_theme, cx);

        let store = notes_store::handle(cx);
        let clipboard = cx.service::<Clipboard>();
        let initial_id = store.read(cx).notes().first().map(|n| n.id.clone());

        cx.observe(&store, |_, _, cx| cx.notify()).detach();

        Self {
            store,
            clipboard,
            filter: NoteFilter::All,
            view_mode: ViewMode::Edit,
            selected_id: initial_id,
            search_query: String::new(),
            confirm_delete_id: None,
            copied_gen: 0,
            copied_feedback: false,
            last_theme,
            last_window_id: None,
            need_body_focus: false,
            need_title_focus: false,
            search_input: None,
            editor: None,
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
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search… (Ctrl+F)"));
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

    fn ensure_editor(
        &mut self,
        note: &NoteItem,
        disk_revision: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(ed) = &mut self.editor {
            if ed.note_id == note.id {
                if ed.disk_revision != disk_revision {
                    ed.disk_revision = disk_revision;
                    ed.suppress_sync = true;
                    let title_val = note.title.clone();
                    let body_val = note.body.clone();
                    let title_ent = ed.title.clone();
                    let body_ent = ed.body.clone();
                    title_ent.update(cx, |s, cx| s.set_value(title_val, window, cx));
                    body_ent.update(cx, |s, cx| s.set_value(body_val, window, cx));
                    if let Some(ed_mut) = &mut self.editor {
                        ed_mut.suppress_sync = false;
                    }
                }
                return;
            }
        }

        let note_id = note.id.clone();
        let initial_title = note.title.clone();
        let initial_body = note.body.clone();

        let title = cx.new(|cx| {
            let mut st = InputState::new(window, cx).placeholder("Note title…");
            st.set_value(initial_title, window, cx);
            st
        });
        let note_id_title = note_id.clone();
        cx.subscribe_in(&title, window, move |this, st, ev: &InputEvent, window, cx| {
            if this.editor.as_ref().is_some_and(|e| e.suppress_sync) {
                return;
            }
            match ev {
                InputEvent::Change => {
                    let val = st.read(cx).value().to_string();
                    this.store
                        .update(cx, |s, cx| s.set_title(&note_id_title, val, cx));
                }
                InputEvent::PressEnter { .. } => {
                    if let Some(ed) = &this.editor {
                        ed.body.update(cx, |b, cx| b.focus(window, cx));
                    }
                }
                _ => {}
            }
        })
        .detach();

        let body = cx.new(|cx| {
            let mut st = TextareaState::new(window, cx).placeholder("Write in Markdown…");
            st.set_value(initial_body, window, cx);
            st
        });
        let note_id_body = note_id.clone();
        cx.subscribe_in(&body, window, move |this, st, ev: &InputEvent, _, cx| {
            if this.editor.as_ref().is_some_and(|e| e.suppress_sync) {
                return;
            }
            if matches!(ev, InputEvent::Change) {
                let val = st.read(cx).value().to_string();
                this.store
                    .update(cx, |s, cx| s.set_body(&note_id_body, val, cx));
            }
        })
        .detach();

        let tag = cx.new(|cx| InputState::new(window, cx).placeholder("+ tag…"));
        let note_id_tag = note_id.clone();
        cx.subscribe_in(&tag, window, move |this, st, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                let val = st.read(cx).value().to_string();
                if !val.trim().is_empty() {
                    this.store
                        .update(cx, |s, cx| s.add_tag(&note_id_tag, val, cx));
                    st.update(cx, |s, cx| s.set_value("", window, cx));
                }
            }
        })
        .detach();

        self.editor = Some(EditorInputs {
            note_id,
            disk_revision,
            suppress_sync: false,
            title,
            body,
            tag,
        });
    }

    fn select_note(&mut self, id: String, cx: &mut Context<Self>) {
        if self.selected_id.as_deref() == Some(id.as_str()) {
            return;
        }
        self.store.update(cx, |s, cx| s.flush_all(cx));
        self.selected_id = Some(id);
        self.confirm_delete_id = None;
        self.editor = None;
        cx.notify();
    }

    fn create_note(&mut self, cx: &mut Context<Self>) {
        self.store.update(cx, |s, cx| s.flush_all(cx));
        let created = self
            .store
            .update(cx, |s, cx| s.create(String::new(), String::new(), cx));
        if let Some(id) = created {
            if let NoteFilter::Tag(tag) = &self.filter {
                let tag = tag.clone();
                self.store.update(cx, |s, cx| s.add_tag(&id, tag, cx));
            }
            self.selected_id = Some(id);
            self.confirm_delete_id = None;
            self.editor = None;
            if self.view_mode == ViewMode::Preview {
                self.view_mode = ViewMode::Edit;
            }
            self.need_title_focus = true;
            cx.notify();
        }
    }

    fn close_window(&mut self, cx: &mut Context<Self>) {
        self.confirm_delete_id = None;
        self.store.update(cx, |s, cx| s.flush_all(cx));
        cx.hide_window(ids::NOTES);
    }

    fn copy_note(&mut self, note: &NoteItem, cx: &mut Context<Self>) {
        let mut content = String::new();
        if !note.title.trim().is_empty() {
            content.push_str(&format!("# {}\n\n", note.title.trim()));
        }
        content.push_str(&note.body);
        self.clipboard.update(cx, |c, _| c.copy_text(&content));

        self.copied_gen = self.copied_gen.wrapping_add(1);
        self.copied_feedback = true;
        let current_gen = self.copied_gen;
        cx.notify();

        cx.spawn(async move |this, cx| {
            async_io::Timer::after(Duration::from_millis(1400)).await;
            let _ = this.update(cx, |view, cx| {
                if view.copied_gen == current_gen {
                    view.copied_feedback = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn filtered_notes(&self, cx: &Context<Self>) -> Vec<NoteItem> {
        let query = self.search_query.trim().to_lowercase();
        self.store
            .read(cx)
            .notes()
            .iter()
            .filter(|n| self.filter.matches(n))
            .filter(|n| {
                if query.is_empty() {
                    return true;
                }
                n.title.to_lowercase().contains(&query)
                    || n.body.to_lowercase().contains(&query)
                    || n.tags.iter().any(|t| t.to_lowercase().contains(&query))
            })
            .cloned()
            .collect()
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let key = ui::key(ev);
        let mods = &ev.keystroke.modifiers;

        if key == "escape" {
            if self.confirm_delete_id.is_some() {
                self.confirm_delete_id = None;
                cx.notify();
            } else {
                self.close_window(cx);
            }
            return;
        }

        if mods.control && (key == "f" || key == "k") {
            if let Some(search) = &self.search_input {
                search.update(cx, |st, cx| st.focus(window, cx));
            }
            return;
        }

        if mods.control && key == "n" {
            self.create_note(cx);
            return;
        }

        if mods.control && key == "p" {
            self.view_mode = self.view_mode.next();
            if self.view_mode != ViewMode::Preview {
                self.need_body_focus = true;
            }
            cx.notify();
            return;
        }

        if mods.control && key == "s" {
            self.store.update(cx, |s, cx| s.flush_all(cx));
        }
    }

    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let total = store.total_count();
        let pinned = store.pinned_count();
        let tags = store.all_tags();

        let count_pill = |n: usize| {
            move |_: &mut Window, cx: &mut App| {
                div()
                    .px_1p5()
                    .py_0p5()
                    .rounded_full()
                    .bg(surface_bg())
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(n.to_string())
            }
        };

        let views_menu = SidebarMenu::new()
            .child(
                SidebarMenuItem::new("All Notes")
                    .icon(svg_icon(SVG_FILE_TEXT))
                    .active(self.filter == NoteFilter::All)
                    .suffix(count_pill(total))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.filter = NoteFilter::All;
                        cx.notify();
                    })),
            )
            .child(
                SidebarMenuItem::new("Pinned")
                    .icon(svg_icon(SVG_PIN))
                    .active(self.filter == NoteFilter::Pinned)
                    .suffix(count_pill(pinned))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.filter = NoteFilter::Pinned;
                        cx.notify();
                    })),
            );

        let mut sidebar = Sidebar::new("notes-sidebar")
            .w(px(200.))
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
                                .child(svg_icon(SVG_FILE_TEXT).text_color(cx.theme().primary))
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(cx.theme().sidebar_foreground)
                                        .child("Kumi Notes"),
                                ),
                        )
                        .child(Badge::new().count(total)),
                ),
            )
            .child(SidebarGroup::new("Library").child(views_menu));

        if !tags.is_empty() {
            let mut tags_menu = SidebarMenu::new();
            for (tag_name, count) in tags {
                let active_tag = self.filter == NoteFilter::Tag(tag_name.clone());
                let tag_clone = tag_name.clone();
                tags_menu = tags_menu.child(
                    SidebarMenuItem::new(format!("#{tag_name}"))
                        .icon(svg_icon(SVG_TAG))
                        .active(active_tag)
                        .suffix(count_pill(count))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.filter = NoteFilter::Tag(tag_clone.clone());
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
                    .p_2p5()
                    .rounded(px(8.))
                    .bg(surface_bg())
                    .border_1()
                    .border_color(cx.theme().border)
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().foreground)
                            .child("Shortcuts"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Ctrl+N new · Ctrl+P mode"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("Ctrl+F search · Esc close"),
                    ),
            ),
        )
    }

    fn render_notes_list(
        &mut self,
        filtered: &[NoteItem],
        search_state: Entity<InputState>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let header = div()
            .p_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&search_state)
                        .small()
                        .prefix(svg_icon(SVG_SEARCH).small()),
                ),
            )
            .child(
                Button::new("notes-new-btn")
                    .small()
                    .primary()
                    .icon(svg_icon(SVG_PLUS))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.create_note(cx);
                    })),
            );

        let mut list = div()
            .flex_1()
            .min_h_0()
            .overflow_y_scrollbar()
            .p_2()
            .flex()
            .flex_col()
            .gap_1p5();

        if filtered.is_empty() {
            list = list.child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .text_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(if self.search_query.trim().is_empty() {
                                "No notes in this view."
                            } else {
                                "No matching notes."
                            }),
                    )
                    .child(
                        Button::new("notes-list-empty-new")
                            .xsmall()
                            .secondary()
                            .icon(svg_icon(SVG_PLUS))
                            .label("New note")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.create_note(cx);
                            })),
                    ),
            );
        } else {
            for (idx, note) in filtered.iter().enumerate() {
                let id = note.id.clone();
                let selected = self.selected_id.as_deref() == Some(id.as_str());
                let title = note.display_title().to_string();
                let snippet = note.snippet();
                let date_label = note.format_updated_short();

                list = list.child(
                    div()
                        .id(("note-list-item", idx))
                        .w_full()
                        .p_2p5()
                        .rounded(px(9.))
                        .cursor_pointer()
                        .bg(if selected {
                            raised_bg()
                        } else {
                            inset_row_bg()
                        })
                        .border_1()
                        .border_color(if selected {
                            cx.theme().primary
                        } else {
                            cx.theme().border
                        })
                        .hover(|s| s.bg(raised_bg().opacity(0.82)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.select_note(id.clone(), cx);
                        }))
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_center()
                                .justify_between()
                                .gap_1p5()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap_1p5()
                                        .when(note.pinned, |d| {
                                            d.child(
                                                svg_icon(SVG_PIN)
                                                    .xsmall()
                                                    .text_color(cx.theme().primary),
                                            )
                                        })
                                        .child(
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .text_sm()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(cx.theme().foreground)
                                                .truncate()
                                                .child(title),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .font_features(ui::tabular())
                                        .text_color(cx.theme().muted_foreground)
                                        .child(date_label),
                                ),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .truncate()
                                .child(snippet),
                        )
                        .when(!note.tags.is_empty(), |d| {
                            d.child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .flex_wrap()
                                    .gap_1()
                                    .children(note.tags.iter().take(2).map(|t| {
                                        Tag::new()
                                            .with_variant(TagVariant::Secondary)
                                            .xsmall()
                                            .child(format!("#{t}"))
                                    })),
                            )
                        }),
                );
            }
        }

        div()
            .w(px(265.))
            .h_full()
            .border_r_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_col()
            .child(header)
            .child(list)
    }

    fn render_markdown_preview(&self, body: &str, cx: &Context<Self>) -> impl IntoElement {
        if body.trim().is_empty() {
            return div()
                .flex_1()
                .min_h_0()
                .p_5()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("Nothing to preview yet — switch to Edit or Split to write in Markdown.")
                .into_any_element();
        }

        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .overflow_y_scrollbar()
            .p_5()
            .child(TextView::markdown("notes-md-preview", body.to_string()).selectable(true))
            .into_any_element()
    }

    fn render_editor_pane(
        &mut self,
        note: Option<NoteItem>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(note) = note else {
            return div()
                .flex_1()
                .min_w_0()
                .h_full()
                .flex()
                .flex_col()
                .child(
                    div()
                        .px_4()
                        .py_3()
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .flex()
                        .flex_row()
                        .items_center()
                        .justify_end()
                        .child(
                            Button::new("notes-empty-close")
                                .small()
                                .ghost()
                                .icon(svg_icon(SVG_CLOSE))
                                .on_click(cx.listener(|this, _, _, cx| this.close_window(cx))),
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap_3()
                        .p_6()
                        .child(svg_icon(SVG_FILE_TEXT).text_color(cx.theme().muted_foreground))
                        .child(
                            div()
                                .text_base()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(cx.theme().foreground)
                                .child("No note selected"),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("Create a Markdown note stored under ~/.local/share/kumi/notes/"),
                        )
                        .child(
                            Button::new("notes-empty-create")
                                .small()
                                .primary()
                                .icon(svg_icon(SVG_PLUS))
                                .label("New Note (Ctrl+N)")
                                .on_click(cx.listener(|this, _, _, cx| this.create_note(cx))),
                        ),
                )
                .into_any_element();
        };

        let disk_revision = self.store.read(cx).disk_revision();
        let is_dirty = self.store.read(cx).is_dirty(&note.id);
        self.ensure_editor(&note, disk_revision, window, cx);

        let (title_state, body_state, tag_state) = {
            let ed = self.editor.as_ref().expect("editor initialized");
            (ed.title.clone(), ed.body.clone(), ed.tag.clone())
        };

        if self.need_title_focus {
            self.need_title_focus = false;
            title_state.update(cx, |s, cx| s.focus(window, cx));
        } else if self.need_body_focus {
            self.need_body_focus = false;
            body_state.update(cx, |s, cx| s.focus(window, cx));
        }

        let note_id = note.id.clone();
        let note_for_copy = note.clone();
        let mode = self.view_mode;

        let mode_btn = |id: &'static str,
                        target: ViewMode,
                        icon_bytes: &'static [u8],
                        label: &'static str| {
            let active = mode == target;
            let b = Button::new(id)
                .xsmall()
                .icon(svg_icon(icon_bytes))
                .label(label);
            let b = if active { b.primary() } else { b.ghost() };
            b.on_click(cx.listener(move |this, _, _, cx| {
                this.view_mode = target;
                if target != ViewMode::Preview {
                    this.need_body_focus = true;
                }
                cx.notify();
            }))
        };

        let top_bar = div()
            .px_4()
            .py_2p5()
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&title_state).appearance(false).bordered(false)),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .child(
                        div()
                            .p_0p5()
                            .rounded(px(8.))
                            .bg(surface_bg())
                            .border_1()
                            .border_color(cx.theme().border)
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_0p5()
                            .child(mode_btn("note-mode-edit", ViewMode::Edit, SVG_EDIT, "Edit"))
                            .child(mode_btn(
                                "note-mode-split",
                                ViewMode::Split,
                                SVG_COLUMNS,
                                "Split",
                            ))
                            .child(mode_btn(
                                "note-mode-preview",
                                ViewMode::Preview,
                                SVG_EYE,
                                "Preview",
                            )),
                    )
                    .child({
                        let id_pin = note_id.clone();
                        let b = Button::new("note-pin-btn").small().icon(svg_icon(SVG_PIN));
                        let b = if note.pinned {
                            b.primary()
                        } else {
                            b.secondary()
                        };
                        b.on_click(cx.listener(move |this, _, _, cx| {
                            this.store.update(cx, |s, cx| s.toggle_pin(&id_pin, cx));
                        }))
                    })
                    .child(
                        Button::new("note-copy-btn")
                            .small()
                            .secondary()
                            .icon(svg_icon(if self.copied_feedback {
                                SVG_CHECK
                            } else {
                                SVG_COPY
                            }))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.copy_note(&note_for_copy, cx);
                            })),
                    )
                    .child({
                        let id_del = note_id.clone();
                        Button::new("note-del-btn")
                            .small()
                            .secondary()
                            .icon(svg_icon(SVG_TRASH))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.confirm_delete_id = Some(id_del.clone());
                                cx.notify();
                            }))
                    })
                    .child(
                        Button::new("note-close-btn")
                            .small()
                            .ghost()
                            .icon(svg_icon(SVG_CLOSE))
                            .on_click(cx.listener(|this, _, _, cx| this.close_window(cx))),
                    ),
            );

        let meta_bar = div()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .items_center()
                    .flex_wrap()
                    .gap_1p5()
                    .children(note.tags.iter().enumerate().map(|(i, tag)| {
                        let tag_name = tag.clone();
                        let id_tag = note_id.clone();
                        div()
                            .id(("note-tag-chip", i))
                            .cursor_pointer()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.store
                                    .update(cx, |s, cx| s.remove_tag(&id_tag, &tag_name, cx));
                            }))
                            .child(
                                Tag::new()
                                    .with_variant(TagVariant::Secondary)
                                    .xsmall()
                                    .child(format!("#{tag} ×")),
                            )
                    }))
                    .child(div().w(px(110.)).child(Input::new(&tag_state).xsmall())),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .text_xs()
                    .font_features(ui::tabular())
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{} words", note.word_count()))
                    .child("·")
                    .child(if is_dirty { "Saving…" } else { "Saved" })
                    .child("·")
                    .child(note.format_updated_short()),
            );

        let editor_area = match self.view_mode {
            ViewMode::Edit => div()
                .flex_1()
                .min_h_0()
                .p_3()
                .child(
                    Textarea::new(&body_state)
                        .h(relative(1.))
                        .appearance(false)
                        .bordered(false),
                )
                .into_any_element(),
            ViewMode::Preview => self.render_markdown_preview(&note.body, cx).into_any_element(),
            ViewMode::Split => div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_row()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .p_3()
                        .border_r_1()
                        .border_color(cx.theme().border)
                        .child(
                            Textarea::new(&body_state)
                                .h(relative(1.))
                                .appearance(false)
                                .bordered(false),
                        ),
                )
                .child(self.render_markdown_preview(&note.body, cx))
                .into_any_element(),
        };

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(top_bar)
            .child(meta_bar)
            .child(editor_area)
            .into_any_element()
    }

    fn render_delete_confirm(&mut self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let target_id = self.confirm_delete_id.as_ref()?.clone();
        let note_title = self
            .store
            .read(cx)
            .get(&target_id)
            .map(|n| n.display_title().to_string())
            .unwrap_or_else(|| "this note".to_string());

        let card = div()
            .occlude()
            .w(px(380.))
            .p_5()
            .rounded(px(12.))
            .bg(panel_bg())
            .border_1()
            .border_color(cx.theme().border)
            .shadow_lg()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .text_base()
                    .font_weight(FontWeight::BOLD)
                    .text_color(cx.theme().foreground)
                    .child("Delete note?"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!(
                        "\"{note_title}\" and its .md file will be permanently removed."
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("note-del-cancel")
                            .small()
                            .secondary()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.confirm_delete_id = None;
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("note-del-confirm")
                            .small()
                            .danger()
                            .icon(svg_icon(SVG_TRASH))
                            .label("Delete")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.store.update(cx, |s, cx| s.delete(&target_id, cx));
                                this.confirm_delete_id = None;
                                this.editor = None;
                                this.selected_id =
                                    this.store.read(cx).notes().first().map(|n| n.id.clone());
                                cx.notify();
                            })),
                    ),
            )
            .with_animation("note-del-confirm-in", ui::anim_pop(), |el, d| {
                el.opacity(d)
            });

        Some(
            div()
                .id("note-del-confirm-overlay")
                .occlude()
                .absolute()
                .inset_0()
                .bg(hsla(theme::current().bg).opacity(0.68))
                .flex()
                .items_center()
                .justify_center()
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.confirm_delete_id = None;
                    cx.notify();
                }))
                .child(card),
        )
    }
}

impl Render for NotesWidget {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        sync_if_needed(&mut self.last_theme, cx);

        let current_win_id = window.window_handle().window_id();
        if self.last_window_id != Some(current_win_id) {
            self.last_window_id = Some(current_win_id);
            let store = self.store.clone();
            window.defer(cx, move |_, cx| {
                store.update(cx, |s, cx| s.reload_async(cx));
            });
            window.focus(&self.focus, cx);
        }

        let search_state = self.ensure_search_input(window, cx);
        let filtered = self.filtered_notes(cx);

        if let Some(sel) = &self.selected_id {
            if self.store.read(cx).get(sel).is_none() {
                self.selected_id = filtered
                    .first()
                    .or_else(|| self.store.read(cx).notes().first())
                    .map(|n| n.id.clone());
                self.editor = None;
            }
        } else if let Some(first) = filtered.first() {
            self.selected_id = Some(first.id.clone());
        }

        let selected_note = self
            .selected_id
            .as_deref()
            .and_then(|id| self.store.read(cx).get(id).cloned());

        let sidebar = self.render_sidebar(cx);
        let list_pane = self.render_notes_list(&filtered, search_state, cx);
        let editor_pane = self.render_editor_pane(selected_note, window, cx);

        let (shell_w, shell_h) = shell_dimensions(window);

        let app_shell = div()
            .occlude()
            .relative()
            .w(px(shell_w))
            .h(px(shell_h))
            .flex()
            .flex_row()
            .overflow_hidden()
            .rounded(px(14.))
            .bg(panel_bg())
            .text_color(cx.theme().foreground)
            .border_1()
            .border_color(cx.theme().border)
            .shadow_lg()
            .child(sidebar)
            .child(list_pane)
            .child(editor_pane)
            .with_animation("notes-open", ui::anim_pop(), |el, delta| {
                el.opacity(delta)
            });

        let mut root = div()
            .id("notes-bg")
            .size_full()
            .bg(crate::theme::pal::with_alpha(crate::theme::pal::bg(), 0.52))
            .on_click(cx.listener(|this, _, _, cx| this.close_window(cx)))
            .relative()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                this.on_key(ev, window, cx);
            }))
            .flex()
            .items_center()
            .justify_center()
            .child(app_shell);

        if let Some(confirm) = self.render_delete_confirm(cx) {
            root = root.child(confirm);
        }

        root
    }
}

fn shell_dimensions(window: &Window) -> (f32, f32) {
    let vp = window.viewport_size();
    let w = (f32::from(vp.width) - 140.0).clamp(760.0, 1080.0);
    let h = (f32::from(vp.height) - 140.0).clamp(460.0, 660.0);
    (w, h)
}
