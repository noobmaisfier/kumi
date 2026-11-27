//! Persistent Markdown note store shared across Kumi (`kumi-notes`, `kumi-bar`, `kumi-control`).
//!
//! Each note lives as an individual `.md` file under `~/.local/share/kumi/notes/`
//! with YAML frontmatter (`title`, `tags`, `pinned`, `created_at`, `updated_at`),
//! so notes remain directly editable in external tools like Neovim or Obsidian.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::{DateTime, Local, NaiveDateTime, TimeZone};
use gpui::{App, AppContext, Context, Entity, Global};

const AUTOSAVE_DEBOUNCE: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteItem {
    /// Stable filename stem (without `.md`), used as unique ID.
    pub id: String,
    pub path: PathBuf,
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
    pub pinned: bool,
    pub created_at: String,
    pub updated_at: String,
    pub updated_unix: i64,
    pub mtime: Option<SystemTime>,
}

impl NoteItem {
    pub fn display_title(&self) -> &str {
        let trimmed = self.title.trim();
        if trimmed.is_empty() {
            "Untitled note"
        } else {
            trimmed
        }
    }

    pub fn snippet(&self) -> String {
        for line in self.body.lines() {
            let trimmed = line.trim().trim_start_matches('#').trim();
            if !trimmed.is_empty() {
                return truncate_chars(trimmed, 90);
            }
        }
        "Empty note".to_string()
    }

    pub fn word_count(&self) -> usize {
        self.body.split_whitespace().count()
    }

    pub fn format_updated_short(&self) -> String {
        let Some(dt) = parse_timestamp(&self.updated_at).or_else(|| {
            Local
                .timestamp_opt(self.updated_unix, 0)
                .single()
                .map(|d| d.naive_local())
        }) else {
            return self.updated_at.clone();
        };

        let today = Local::now().date_naive();
        let diff = (today - dt.date()).num_days();
        match diff {
            0 => dt.format("%H:%M").to_string(),
            1 => "Yesterday".to_string(),
            2..=6 => dt.format("%a").to_string(),
            _ => dt.format("%b %d").to_string(),
        }
    }

    pub fn to_markdown_file(&self) -> String {
        let mut out = String::from("---\n");
        out.push_str(&format!("title: {}\n", yaml_quote(&self.title)));
        if self.tags.is_empty() {
            out.push_str("tags: []\n");
        } else {
            let items: Vec<String> = self.tags.iter().map(|t| yaml_quote(t)).collect();
            out.push_str(&format!("tags: [{}]\n", items.join(", ")));
        }
        out.push_str(&format!("pinned: {}\n", self.pinned));
        out.push_str(&format!("created_at: {}\n", self.created_at));
        out.push_str(&format!("updated_at: {}\n", self.updated_at));
        out.push_str("---\n");
        if !self.body.is_empty() {
            out.push('\n');
            out.push_str(&self.body);
            if !self.body.ends_with('\n') {
                out.push('\n');
            }
        }
        out
    }
}

pub fn notes_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("kumi").join("notes"))
}

fn now_timestamp() -> (String, i64) {
    let now = Local::now();
    (now.format("%Y-%m-%dT%H:%M:%S").to_string(), now.timestamp())
}

fn system_time_to_unix(t: SystemTime) -> i64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn system_time_to_iso(t: SystemTime) -> String {
    let dt: DateTime<Local> = t.into();
    dt.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn parse_timestamp(s: &str) -> Option<NaiveDateTime> {
    let trimmed = s.trim();
    NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S"))
        .or_else(|_| {
            DateTime::parse_from_rfc3339(trimmed).map(|d| d.with_timezone(&Local).naive_local())
        })
        .ok()
}

fn timestamp_to_unix(s: &str) -> Option<i64> {
    let naive = parse_timestamp(s)?;
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|dt| dt.timestamp())
}

fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut iter = s.chars();
    let collected: String = iter.by_ref().take(max_chars).collect();
    if iter.next().is_some() {
        format!("{collected}…")
    } else {
        collected
    }
}

fn yaml_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn yaml_unquote(raw: &str) -> String {
    let s = raw.trim();
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        let inner = &s[1..s.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => out.push('\\'),
                }
            } else {
                out.push(ch);
            }
        }
        return out;
    }
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        return s[1..s.len() - 1].replace("''", "'");
    }
    s.to_string()
}

fn parse_inline_list(raw: &str) -> Vec<String> {
    let s = raw.trim();
    let inner = s.strip_prefix('[').and_then(|v| v.strip_suffix(']')).unwrap_or(s);
    if inner.trim().is_empty() {
        return Vec::new();
    }
    let mut items = Vec::new();
    let mut cur = String::new();
    let mut in_quotes: Option<char> = None;
    let mut escaped = false;
    for ch in inner.chars() {
        if escaped {
            cur.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' && in_quotes == Some('"') {
            cur.push(ch);
            escaped = true;
            continue;
        }
        if let Some(q) = in_quotes {
            cur.push(ch);
            if ch == q {
                in_quotes = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                in_quotes = Some(ch);
                cur.push(ch);
            }
            ',' => {
                let clean = normalize_tag(&yaml_unquote(&cur));
                if !clean.is_empty() && !items.iter().any(|t: &String| t.eq_ignore_ascii_case(&clean))
                {
                    items.push(clean);
                }
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    let clean = normalize_tag(&yaml_unquote(&cur));
    if !clean.is_empty() && !items.iter().any(|t: &String| t.eq_ignore_ascii_case(&clean)) {
        items.push(clean);
    }
    items
}

pub fn normalize_tag(raw: &str) -> String {
    raw.trim().trim_start_matches('#').trim().to_string()
}

fn slugify(raw: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
        if slug.len() >= 48 {
            break;
        }
    }
    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "note".to_string()
    } else {
        trimmed
    }
}

fn split_frontmatter(raw: &str) -> (Option<&str>, &str) {
    let normalized = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let rest = if let Some(r) = normalized.strip_prefix("---\n") {
        r
    } else if let Some(r) = normalized.strip_prefix("---\r\n") {
        r
    } else {
        return (None, normalized);
    };

    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" || trimmed == "..." {
            let fm = &rest[..offset];
            let body_raw = &rest[offset + line.len()..];
            let body = body_raw
                .strip_prefix("\r\n")
                .or_else(|| body_raw.strip_prefix('\n'))
                .unwrap_or(body_raw);
            return (Some(fm), body);
        }
        offset += line.len();
    }

    (None, normalized)
}

fn infer_title_from_body(body: &str, id: &str) -> String {
    for line in body.lines() {
        let trimmed = line.trim();
        if let Some(heading) = trimmed.strip_prefix('#') {
            let h = heading.trim_start_matches('#').trim();
            if !h.is_empty() {
                return h.to_string();
            }
        }
    }
    id.replace(['-', '_'], " ")
}

pub fn parse_markdown_note(
    path: PathBuf,
    raw: &str,
    mtime: Option<SystemTime>,
) -> Option<NoteItem> {
    let id = path.file_stem()?.to_str()?.to_string();
    let (fm_opt, body_raw) = split_frontmatter(raw);
    let body = body_raw.trim_end_matches(['\r', '\n']).to_string();

    let mut title: Option<String> = None;
    let mut tags: Vec<String> = Vec::new();
    let mut pinned = false;
    let mut created_at: Option<String> = None;
    let mut updated_at: Option<String> = None;

    if let Some(fm) = fm_opt {
        let mut reading_tags_list = false;
        for line in fm.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if reading_tags_list {
                if let Some(item) = trimmed.strip_prefix('-') {
                    let clean = normalize_tag(&yaml_unquote(item));
                    if !clean.is_empty()
                        && !tags.iter().any(|t| t.eq_ignore_ascii_case(&clean))
                    {
                        tags.push(clean);
                    }
                    continue;
                } else {
                    reading_tags_list = false;
                }
            }
            if let Some((k, v)) = trimmed.split_once(':') {
                let key = k.trim();
                let val = v.trim();
                match key {
                    "title" => title = Some(yaml_unquote(val)),
                    "pinned" => pinned = val.eq_ignore_ascii_case("true"),
                    "created_at" => {
                        let u = yaml_unquote(val);
                        if !u.is_empty() {
                            created_at = Some(u);
                        }
                    }
                    "updated_at" => {
                        let u = yaml_unquote(val);
                        if !u.is_empty() {
                            updated_at = Some(u);
                        }
                    }
                    "tags" => {
                        if val.is_empty() {
                            reading_tags_list = true;
                        } else {
                            for tag in parse_inline_list(val) {
                                if !tags.iter().any(|t| t.eq_ignore_ascii_case(&tag)) {
                                    tags.push(tag);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let fallback_time = mtime
        .map(system_time_to_iso)
        .unwrap_or_else(|| now_timestamp().0);
    let created = created_at.unwrap_or_else(|| fallback_time.clone());
    let updated = updated_at.unwrap_or_else(|| created.clone());
    let updated_unix = mtime
        .map(system_time_to_unix)
        .or_else(|| timestamp_to_unix(&updated))
        .unwrap_or(0);

    let resolved_title = title.unwrap_or_else(|| infer_title_from_body(&body, &id));

    Some(NoteItem {
        id,
        path,
        title: resolved_title,
        body,
        tags,
        pinned,
        created_at: created,
        updated_at: updated,
        updated_unix,
        mtime,
    })
}

fn load_notes_from_dir(dir: &Path) -> Vec<NoteItem> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut notes = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let mtime = entry.metadata().ok().and_then(|m| m.modified().ok());
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Some(note) = parse_markdown_note(path, &raw, mtime) {
                notes.push(note);
            }
        }
    }
    sort_notes(&mut notes);
    notes
}

fn sort_notes(notes: &mut [NoteItem]) {
    notes.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| b.updated_unix.cmp(&a.updated_unix))
            .then_with(|| a.id.cmp(&b.id))
    });
}

fn write_note_to_disk(note: &NoteItem) -> Option<SystemTime> {
    if let Some(parent) = note.path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let content = note.to_markdown_file();
    if std::fs::write(&note.path, content).is_ok() {
        std::fs::metadata(&note.path)
            .ok()
            .and_then(|m| m.modified().ok())
    } else {
        None
    }
}

pub struct NotesStore {
    notes: Vec<NoteItem>,
    dirty_ids: HashSet<String>,
    save_gens: HashMap<String, u64>,
    disk_revision: u64,
    reload_in_flight: bool,
}

impl NotesStore {
    fn load() -> Self {
        let notes = notes_dir()
            .map(|dir| load_notes_from_dir(&dir))
            .unwrap_or_default();
        Self {
            notes,
            dirty_ids: HashSet::new(),
            save_gens: HashMap::new(),
            disk_revision: 0,
            reload_in_flight: false,
        }
    }

    pub fn notes(&self) -> &[NoteItem] {
        &self.notes
    }

    pub fn get(&self, id: &str) -> Option<&NoteItem> {
        self.notes.iter().find(|n| n.id == id)
    }

    pub fn total_count(&self) -> usize {
        self.notes.len()
    }

    pub fn pinned_count(&self) -> usize {
        self.notes.iter().filter(|n| n.pinned).count()
    }

    pub fn is_dirty(&self, id: &str) -> bool {
        self.dirty_ids.contains(id)
    }

    pub fn disk_revision(&self) -> u64 {
        self.disk_revision
    }

    pub fn all_tags(&self) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for n in &self.notes {
            for tag in &n.tags {
                *counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        counts.into_iter().collect()
    }

    fn unique_id(&self, base_title: &str, dir: &Path) -> String {
        let base = if base_title.trim().is_empty() {
            format!("note-{}", Local::now().format("%Y%m%d-%H%M%S"))
        } else {
            slugify(base_title)
        };
        let mut candidate = base.clone();
        let mut n = 2usize;
        while self.notes.iter().any(|item| item.id == candidate)
            || dir.join(format!("{candidate}.md")).exists()
        {
            candidate = format!("{base}-{n}");
            n += 1;
        }
        candidate
    }

    pub fn create(&mut self, title: String, body: String, cx: &mut Context<Self>) -> Option<String> {
        let dir = notes_dir()?;
        let id = self.unique_id(&title, &dir);
        let path = dir.join(format!("{id}.md"));
        let (now_iso, now_unix) = now_timestamp();
        let mut note = NoteItem {
            id: id.clone(),
            path,
            title,
            body,
            tags: Vec::new(),
            pinned: false,
            created_at: now_iso.clone(),
            updated_at: now_iso,
            updated_unix: now_unix,
            mtime: None,
        };
        note.mtime = write_note_to_disk(&note);
        self.notes.push(note);
        sort_notes(&mut self.notes);
        cx.notify();
        Some(id)
    }

    fn schedule_debounced_save(&mut self, id: String, cx: &mut Context<Self>) {
        self.dirty_ids.insert(id.clone());
        let next_gen = self.save_gens.get(&id).copied().unwrap_or(0).wrapping_add(1);
        self.save_gens.insert(id.clone(), next_gen);

        cx.spawn(async move |this, cx| {
            async_io::Timer::after(AUTOSAVE_DEBOUNCE).await;
            let _ = this.update(cx, |store, cx| {
                if store.save_gens.get(&id).copied() == Some(next_gen)
                    && store.dirty_ids.contains(&id)
                {
                    store.flush_note(&id, cx);
                }
            });
        })
        .detach();
    }

    pub fn flush_note(&mut self, id: &str, cx: &mut Context<Self>) {
        if !self.dirty_ids.remove(id) {
            return;
        }
        if let Some(note) = self.notes.iter_mut().find(|n| n.id == id) {
            note.mtime = write_note_to_disk(note);
        }
        cx.notify();
    }

    pub fn flush_all(&mut self, cx: &mut Context<Self>) {
        if self.dirty_ids.is_empty() {
            return;
        }
        let dirty: Vec<String> = self.dirty_ids.drain().collect();
        for id in dirty {
            if let Some(note) = self.notes.iter_mut().find(|n| n.id == id) {
                note.mtime = write_note_to_disk(note);
            }
        }
        cx.notify();
    }

    pub fn set_title(&mut self, id: &str, title: String, cx: &mut Context<Self>) {
        let Some(note) = self.notes.iter_mut().find(|n| n.id == id) else {
            return;
        };
        if note.title == title {
            return;
        }
        let (now_iso, now_unix) = now_timestamp();
        note.title = title;
        note.updated_at = now_iso;
        note.updated_unix = now_unix;
        sort_notes(&mut self.notes);
        self.schedule_debounced_save(id.to_string(), cx);
        cx.notify();
    }

    pub fn set_body(&mut self, id: &str, body: String, cx: &mut Context<Self>) {
        let Some(note) = self.notes.iter_mut().find(|n| n.id == id) else {
            return;
        };
        if note.body == body {
            return;
        }
        let (now_iso, now_unix) = now_timestamp();
        note.body = body;
        note.updated_at = now_iso;
        note.updated_unix = now_unix;
        sort_notes(&mut self.notes);
        self.schedule_debounced_save(id.to_string(), cx);
        cx.notify();
    }

    pub fn toggle_pin(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(note) = self.notes.iter_mut().find(|n| n.id == id) else {
            return;
        };
        note.pinned = !note.pinned;
        let (now_iso, now_unix) = now_timestamp();
        note.updated_at = now_iso;
        note.updated_unix = now_unix;
        note.mtime = write_note_to_disk(note);
        self.dirty_ids.remove(id);
        sort_notes(&mut self.notes);
        cx.notify();
    }

    pub fn add_tag(&mut self, id: &str, tag: String, cx: &mut Context<Self>) {
        let clean = normalize_tag(&tag);
        if clean.is_empty() {
            return;
        }
        let Some(note) = self.notes.iter_mut().find(|n| n.id == id) else {
            return;
        };
        if note.tags.iter().any(|t| t.eq_ignore_ascii_case(&clean)) {
            return;
        }
        note.tags.push(clean);
        let (now_iso, now_unix) = now_timestamp();
        note.updated_at = now_iso;
        note.updated_unix = now_unix;
        note.mtime = write_note_to_disk(note);
        self.dirty_ids.remove(id);
        cx.notify();
    }

    pub fn remove_tag(&mut self, id: &str, tag: &str, cx: &mut Context<Self>) {
        let Some(note) = self.notes.iter_mut().find(|n| n.id == id) else {
            return;
        };
        let len = note.tags.len();
        note.tags.retain(|t| t != tag);
        if note.tags.len() != len {
            let (now_iso, now_unix) = now_timestamp();
            note.updated_at = now_iso;
            note.updated_unix = now_unix;
            note.mtime = write_note_to_disk(note);
            self.dirty_ids.remove(id);
            cx.notify();
        }
    }

    pub fn delete(&mut self, id: &str, cx: &mut Context<Self>) {
        self.dirty_ids.remove(id);
        self.save_gens.remove(id);
        if let Some(pos) = self.notes.iter().position(|n| n.id == id) {
            let removed = self.notes.remove(pos);
            let _ = std::fs::remove_file(&removed.path);
            cx.notify();
        }
    }

    /// Flushes dirty edits and reloads external changes from `~/.local/share/kumi/notes/`
    /// off the UI thread.
    pub fn reload_async(&mut self, cx: &mut Context<Self>) {
        self.flush_all(cx);
        if self.reload_in_flight {
            return;
        }
        let Some(dir) = notes_dir() else {
            return;
        };
        self.reload_in_flight = true;

        cx.spawn(async move |this, cx| {
            let disk_notes = cx
                .background_executor()
                .spawn(async move { load_notes_from_dir(&dir) })
                .await;

            let _ = this.update(cx, |store, cx| {
                store.reload_in_flight = false;

                // Keep any note dirtied while the async scan was running.
                let mut merged: Vec<NoteItem> = Vec::with_capacity(disk_notes.len());
                for disk in disk_notes {
                    if store.dirty_ids.contains(&disk.id) {
                        if let Some(in_mem) = store.notes.iter().find(|n| n.id == disk.id) {
                            merged.push(in_mem.clone());
                            continue;
                        }
                    }
                    merged.push(disk);
                }
                for dirty_id in &store.dirty_ids {
                    if !merged.iter().any(|n| &n.id == dirty_id) {
                        if let Some(in_mem) = store.notes.iter().find(|n| &n.id == dirty_id) {
                            merged.push(in_mem.clone());
                        }
                    }
                }
                sort_notes(&mut merged);

                let changed = merged.len() != store.notes.len()
                    || merged.iter().zip(store.notes.iter()).any(|(a, b)| {
                        a.id != b.id
                            || a.title != b.title
                            || a.body != b.body
                            || a.tags != b.tags
                            || a.pinned != b.pinned
                    });

                if changed {
                    store.notes = merged;
                    store.disk_revision = store.disk_revision.wrapping_add(1);
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

struct NotesGlobal(Entity<NotesStore>);
impl Global for NotesGlobal {}

pub fn handle(cx: &mut App) -> Entity<NotesStore> {
    if !cx.has_global::<NotesGlobal>() {
        let e = cx.new(|_| NotesStore::load());
        cx.set_global(NotesGlobal(e));
    }
    cx.global::<NotesGlobal>().0.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_roundtrip_preserves_metadata_and_body() {
        let note = NoteItem {
            id: "meeting-notes".into(),
            path: PathBuf::from("/tmp/meeting-notes.md"),
            title: "Design \"Kumi\" Notes: v1".into(),
            body: "# Heading\n\n- [ ] Task one\n- [x] Task two".into(),
            tags: vec!["work".into(), "rust".into()],
            pinned: true,
            created_at: "2026-11-27T00:10:00".into(),
            updated_at: "2026-11-27T00:25:00".into(),
            updated_unix: 1_795_739_100,
            mtime: None,
        };

        let raw = note.to_markdown_file();
        let parsed = parse_markdown_note(note.path.clone(), &raw, None).expect("parsed note");

        assert_eq!(parsed.id, "meeting-notes");
        assert_eq!(parsed.title, note.title);
        assert_eq!(parsed.body, note.body);
        assert_eq!(parsed.tags, note.tags);
        assert!(parsed.pinned);
        assert_eq!(parsed.created_at, note.created_at);
        assert_eq!(parsed.updated_at, note.updated_at);
    }

    #[test]
    fn external_plain_markdown_infers_title_and_yaml_list_tags() {
        let raw = "\
---
tags:
  - nvim
  - notes
pinned: false
---
# Quick Thought

Remember to check Wayland layer-shell focus.
";
        let parsed = parse_markdown_note(PathBuf::from("/tmp/quick.md"), raw, None).unwrap();
        assert_eq!(parsed.title, "Quick Thought");
        assert_eq!(parsed.tags, vec!["nvim", "notes"]);
        assert!(!parsed.pinned);
        assert!(parsed.body.contains("Wayland layer-shell focus"));
    }

    #[test]
    fn plain_file_without_frontmatter_uses_heading_or_filename() {
        let raw = "Just a plain scratchpad line.\nSecond line.";
        let parsed = parse_markdown_note(PathBuf::from("/tmp/daily_ideas.md"), raw, None).unwrap();
        assert_eq!(parsed.title, "daily ideas");
        assert_eq!(parsed.body, raw);
        assert!(parsed.tags.is_empty());
    }

    #[test]
    fn slugify_handles_unicode_and_symbols() {
        assert_eq!(slugify("Notes d'archi: v2!"), "notes-d-archi-v2");
        assert_eq!(slugify("   !!!   "), "note");
    }
}
