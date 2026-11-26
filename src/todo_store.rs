//! Persistent task store shared across Kumi (`kumi-todo` and `kumi-bar`).
//!
//! Stores tasks in `~/.local/share/kumi/tasks.json` with backward-compatible
//! serde defaults for subtasks, tags, notes, and due dates.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{Duration, Local, NaiveDate};
use gpui::{App, AppContext, Context, Entity, Global};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Debug)]
pub enum TaskStatus {
    Todo,
    InProgress,
    Done,
}

impl TaskStatus {
    pub fn label(self) -> &'static str {
        match self {
            TaskStatus::Todo => "To Do",
            TaskStatus::InProgress => "In Progress",
            TaskStatus::Done => "Done",
        }
    }

    pub fn step(self, delta: isize) -> Self {
        let idx = match self {
            TaskStatus::Todo => 0isize,
            TaskStatus::InProgress => 1,
            TaskStatus::Done => 2,
        };
        match (idx + delta).clamp(0, 2) {
            0 => TaskStatus::Todo,
            1 => TaskStatus::InProgress,
            _ => TaskStatus::Done,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Debug)]
pub enum TaskPriority {
    High,
    Normal,
    Low,
}

impl TaskPriority {
    pub fn label(self) -> &'static str {
        match self {
            TaskPriority::High => "High",
            TaskPriority::Normal => "Normal",
            TaskPriority::Low => "Low",
        }
    }

    pub fn next(self) -> Self {
        match self {
            TaskPriority::Normal => TaskPriority::High,
            TaskPriority::High => TaskPriority::Low,
            TaskPriority::Low => TaskPriority::Normal,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SubTask {
    pub id: u64,
    pub title: String,
    pub done: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TaskItem {
    pub id: u64,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    #[serde(default)]
    pub due_date: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub subtasks: Vec<SubTask>,
    #[serde(default)]
    pub created_at: String,
    pub order: usize,
}

impl TaskItem {
    pub fn subtask_progress(&self) -> (usize, usize) {
        let total = self.subtasks.len();
        let done = self.subtasks.iter().filter(|s| s.done).count();
        (done, total)
    }

    pub fn is_due_today_or_overdue(&self) -> bool {
        if self.status == TaskStatus::Done {
            return false;
        }
        let Some(ref d) = self.due_date else {
            return false;
        };
        let Ok(parsed) = NaiveDate::parse_from_str(d, "%Y-%m-%d") else {
            return false;
        };
        parsed <= Local::now().date_naive()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DueUrgency {
    Overdue,
    Today,
    Upcoming,
}

pub fn today_str() -> String {
    Local::now().date_naive().format("%Y-%m-%d").to_string()
}

pub fn in_days_str(days: i64) -> String {
    (Local::now().date_naive() + Duration::days(days))
        .format("%Y-%m-%d")
        .to_string()
}

pub fn is_valid_date(s: &str) -> bool {
    NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").is_ok()
}

pub fn format_due_label(date_str: &str) -> (String, DueUrgency) {
    let today = Local::now().date_naive();
    let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") else {
        return (date_str.to_string(), DueUrgency::Upcoming);
    };
    let diff = (date - today).num_days();
    match diff {
        d if d < 0 => (format!("Overdue ({}d)", -d), DueUrgency::Overdue),
        0 => ("Today".to_string(), DueUrgency::Today),
        1 => ("Tomorrow".to_string(), DueUrgency::Upcoming),
        2..=6 => (format!("In {}d", diff), DueUrgency::Upcoming),
        _ => (date.format("%b %d").to_string(), DueUrgency::Upcoming),
    }
}

#[derive(Serialize, Deserialize, Default)]
struct PersistedTasks {
    next_id: u64,
    tasks: Vec<TaskItem>,
}

fn storage_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("kumi").join("tasks.json"))
}

pub struct TodoStore {
    next_id: u64,
    tasks: Vec<TaskItem>,
}

impl TodoStore {
    fn load() -> Self {
        let loaded = storage_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| serde_json::from_str::<PersistedTasks>(&s).ok());
        match loaded {
            Some(data) => {
                let mut max_id = data.tasks.iter().map(|t| t.id).max().unwrap_or(0);
                for t in &data.tasks {
                    for s in &t.subtasks {
                        max_id = max_id.max(s.id);
                    }
                }
                Self {
                    next_id: data.next_id.max(max_id.saturating_add(1)),
                    tasks: data.tasks,
                }
            }
            None => Self {
                next_id: 1,
                tasks: Vec::new(),
            },
        }
    }

    fn save(&self) {
        if let Some(path) = storage_path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let payload = PersistedTasks {
                next_id: self.next_id,
                tasks: self.tasks.clone(),
            };
            if let Ok(json) = serde_json::to_string_pretty(&payload) {
                let _ = std::fs::write(path, json);
            }
        }
    }

    fn alloc_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn tasks(&self) -> &[TaskItem] {
        &self.tasks
    }

    pub fn get(&self, id: u64) -> Option<&TaskItem> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn total_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn active_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| t.status != TaskStatus::Done)
            .count()
    }

    pub fn done_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Done)
            .count()
    }

    pub fn in_progress_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| t.status == TaskStatus::InProgress)
            .count()
    }

    pub fn high_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| t.status != TaskStatus::Done && t.priority == TaskPriority::High)
            .count()
    }

    pub fn today_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|t| t.is_due_today_or_overdue())
            .count()
    }

    pub fn all_tags(&self) -> Vec<(String, usize)> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for t in &self.tasks {
            if t.status != TaskStatus::Done {
                for tag in &t.tags {
                    *counts.entry(tag.clone()).or_insert(0) += 1;
                }
            }
        }
        counts.into_iter().collect()
    }

    /// Up to 5 next unfinished tasks: `InProgress` first, then due today/overdue,
    /// then priority (`High` -> `Normal` -> `Low`), then `order`.
    pub fn focus_tasks(&self) -> Vec<&TaskItem> {
        let mut items: Vec<&TaskItem> = self
            .tasks
            .iter()
            .filter(|t| t.status != TaskStatus::Done)
            .collect();
        items.sort_by_key(|t| {
            let status_rank = match t.status {
                TaskStatus::InProgress => 0u8,
                TaskStatus::Todo => 1,
                TaskStatus::Done => 2,
            };
            let due_rank = if t.is_due_today_or_overdue() { 0u8 } else { 1 };
            (status_rank, due_rank, t.priority, t.order)
        });
        items.truncate(5);
        items
    }

    pub fn add(
        &mut self,
        title: String,
        description: String,
        priority: TaskPriority,
        status: TaskStatus,
        due_date: Option<String>,
        tags: Vec<String>,
        cx: &mut Context<Self>,
    ) -> Option<u64> {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return None;
        }
        let order = self.tasks.iter().map(|t| t.order).max().unwrap_or(0) + 1;
        let id = self.alloc_id();
        let clean_tags: Vec<String> = tags
            .into_iter()
            .map(|s| s.trim().trim_start_matches('#').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        self.tasks.push(TaskItem {
            id,
            title: trimmed.to_string(),
            description: description.trim().to_string(),
            status,
            priority,
            due_date,
            tags: clean_tags,
            subtasks: Vec::new(),
            created_at: today_str(),
            order,
        });
        self.save();
        cx.notify();
        Some(id)
    }

    pub fn set_title(&mut self, id: u64, title: String, cx: &mut Context<Self>) {
        let trimmed = title.trim();
        if trimmed.is_empty() {
            return;
        }
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            if task.title != trimmed {
                task.title = trimmed.to_string();
                self.save();
                cx.notify();
            }
        }
    }

    pub fn set_description(&mut self, id: u64, description: String, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            if task.description != description {
                task.description = description;
                self.save();
                cx.notify();
            }
        }
    }

    pub fn set_status(&mut self, id: u64, status: TaskStatus, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            if task.status != status {
                task.status = status;
                self.save();
                cx.notify();
            }
        }
    }

    pub fn toggle_done(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.status = if task.status == TaskStatus::Done {
                TaskStatus::Todo
            } else {
                TaskStatus::Done
            };
            self.save();
            cx.notify();
        }
    }

    pub fn move_status(&mut self, id: u64, delta: isize, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            let next = task.status.step(delta);
            if next != task.status {
                task.status = next;
                self.save();
                cx.notify();
            }
        }
    }

    pub fn set_priority(&mut self, id: u64, priority: TaskPriority, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            if task.priority != priority {
                task.priority = priority;
                self.save();
                cx.notify();
            }
        }
    }

    pub fn cycle_priority(&mut self, id: u64, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.priority = task.priority.next();
            self.save();
            cx.notify();
        }
    }

    pub fn set_due_date(&mut self, id: u64, due_date: Option<String>, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            if task.due_date != due_date {
                task.due_date = due_date;
                self.save();
                cx.notify();
            }
        }
    }

    pub fn add_tag(&mut self, id: u64, tag: String, cx: &mut Context<Self>) {
        let clean = tag.trim().trim_start_matches('#').to_string();
        if clean.is_empty() {
            return;
        }
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            if !task.tags.iter().any(|t| t.eq_ignore_ascii_case(&clean)) {
                task.tags.push(clean);
                self.save();
                cx.notify();
            }
        }
    }

    pub fn remove_tag(&mut self, id: u64, tag: &str, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            let len = task.tags.len();
            task.tags.retain(|t| t != tag);
            if task.tags.len() != len {
                self.save();
                cx.notify();
            }
        }
    }

    pub fn add_subtask(&mut self, id: u64, title: String, cx: &mut Context<Self>) {
        let clean = title.trim();
        if clean.is_empty() {
            return;
        }
        let sub_id = self.alloc_id();
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            task.subtasks.push(SubTask {
                id: sub_id,
                title: clean.to_string(),
                done: false,
            });
            self.save();
            cx.notify();
        }
    }

    pub fn toggle_subtask(&mut self, id: u64, sub_id: u64, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            if let Some(sub) = task.subtasks.iter_mut().find(|s| s.id == sub_id) {
                sub.done = !sub.done;
                self.save();
                cx.notify();
            }
        }
    }

    pub fn remove_subtask(&mut self, id: u64, sub_id: u64, cx: &mut Context<Self>) {
        if let Some(task) = self.tasks.iter_mut().find(|t| t.id == id) {
            let len = task.subtasks.len();
            task.subtasks.retain(|s| s.id != sub_id);
            if task.subtasks.len() != len {
                self.save();
                cx.notify();
            }
        }
    }

    pub fn remove(&mut self, id: u64, cx: &mut Context<Self>) {
        let len = self.tasks.len();
        self.tasks.retain(|t| t.id != id);
        if self.tasks.len() != len {
            self.save();
            cx.notify();
        }
    }

    pub fn clear_done(&mut self, cx: &mut Context<Self>) {
        let len = self.tasks.len();
        self.tasks.retain(|t| t.status != TaskStatus::Done);
        if self.tasks.len() != len {
            self.save();
            cx.notify();
        }
    }
}

struct TodoGlobal(Entity<TodoStore>);
impl Global for TodoGlobal {}

pub fn handle(cx: &mut App) -> Entity<TodoStore> {
    if !cx.has_global::<TodoGlobal>() {
        let e = cx.new(|_| TodoStore::load());
        cx.set_global(TodoGlobal(e));
    }
    cx.global::<TodoGlobal>().0.clone()
}
