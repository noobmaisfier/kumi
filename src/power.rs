//! Full-screen framed Power Menu built with `gpui-component`.
//!
//! Displays system context (user@host, live clock, uptime, battery),
//! 5 Lucide action cards (`Verrouiller`, `Déconnexion`, `Veille`,
//! `Redémarrer`, `Éteindre`), and an explicit confirmation banner for
//! destructive actions.

use gpui::prelude::*;
use gpui::{
    div, px, AnimationExt, ClickEvent, Context, Entity, FocusHandle, FontWeight, KeyDownEvent,
    Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{ActiveTheme as _, Sizable as _};
use kumizo_kit::prelude::*;

use crate::{
    kit::{
        clamped_panel_size, ensure_components_init, hsla, panel_bg, raised_bg, surface_bg,
        svg_icon, sync_gpui_theme, sync_if_needed, SVG_ALERT, SVG_CLOSE, SVG_LOCK, SVG_LOGOUT,
        SVG_MOON, SVG_POWER, SVG_ROTATE_CCW,
    },
    theme,
    ui::{self, ids},
};

#[derive(Clone, Copy)]
enum Action {
    Lock,
    Logout,
    Suspend,
    Reboot,
    Shutdown,
}

struct Entry {
    action: Action,
    icon: &'static [u8],
    label: &'static str,
    key: &'static str,
    confirm: bool,
    destructive: bool,
}

const ENTRIES: [Entry; 5] = [
    Entry {
        action: Action::Lock,
        icon: SVG_LOCK,
        label: "Lock",
        key: "l",
        confirm: false,
        destructive: false,
    },
    Entry {
        action: Action::Logout,
        icon: SVG_LOGOUT,
        label: "Log out",
        key: "o",
        confirm: true,
        destructive: true,
    },
    Entry {
        action: Action::Suspend,
        icon: SVG_MOON,
        label: "Suspend",
        key: "s",
        confirm: false,
        destructive: false,
    },
    Entry {
        action: Action::Reboot,
        icon: SVG_ROTATE_CCW,
        label: "Reboot",
        key: "r",
        confirm: true,
        destructive: true,
    },
    Entry {
        action: Action::Shutdown,
        icon: SVG_POWER,
        label: "Power off",
        key: "p",
        confirm: true,
        destructive: true,
    },
];

fn read_user_host() -> String {
    let user = std::env::var("USER")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "session".to_string());
    let host = std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "linux".to_string());
    format!("{user}@{host}")
}

fn read_uptime() -> String {
    let Ok(raw) = std::fs::read_to_string("/proc/uptime") else {
        return "—".to_string();
    };
    let secs = raw
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
        .max(0.0) as u64;
    let mins = secs / 60;
    let hours = mins / 60;
    let days = hours / 24;
    if days > 0 {
        format!("{}d {}h", days, hours % 24)
    } else if hours > 0 {
        format!("{}h {:02}m", hours, mins % 60)
    } else {
        format!("{} min", mins.max(1))
    }
}

fn logout() {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        ui::run("hyprctl", &["dispatch", "exit"]);
    } else if std::env::var_os("SWAYSOCK").is_some() {
        ui::run("swaymsg", &["exit"]);
    } else if let Ok(session) = std::env::var("XDG_SESSION_ID") {
        ui::run("loginctl", &["terminate-session", &session]);
    }
}

fn perform(action: Action) {
    match action {
        Action::Lock => ui::run("kumi-lockscreen", &[]),
        Action::Logout => logout(),
        Action::Suspend => ui::run("systemctl", &["suspend"]),
        Action::Reboot => ui::run("systemctl", &["reboot"]),
        Action::Shutdown => ui::run("systemctl", &["poweroff"]),
    }
}

pub struct PowerMenu {
    clock: Entity<Clock>,
    battery: Entity<Battery>,
    user_host: String,
    uptime_label: String,
    selected: usize,
    armed: Option<usize>,
    last_theme: &'static str,
    focus: FocusHandle,
}

impl PowerMenu {
    pub fn new(cx: &mut Context<Self>) -> Self {
        ensure_components_init(cx);
        sync_gpui_theme(cx);

        let clock = cx.service::<Clock>();
        let battery = cx.service::<Battery>();
        cx.observe(&clock, |this, _, cx| {
            this.uptime_label = read_uptime();
            cx.notify();
        })
        .detach();
        cx.observe(&battery, |_, _, cx| cx.notify()).detach();

        Self {
            clock,
            battery,
            user_host: read_user_host(),
            uptime_label: read_uptime(),
            selected: 0,
            armed: None,
            last_theme: theme::current().name,
            focus: cx.focus_handle(),
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        self.selected = 0;
        self.armed = None;
        cx.hide_window(ids::POWER);
        cx.notify();
    }

    fn execute_index(&mut self, i: usize, cx: &mut Context<Self>) {
        let action = ENTRIES[i].action;
        self.close(cx);
        perform(action);
    }

    fn activate(&mut self, i: usize, cx: &mut Context<Self>) {
        let entry = &ENTRIES[i];
        self.selected = i;
        if entry.confirm && self.armed != Some(i) {
            self.armed = Some(i);
            cx.notify();
            return;
        }
        self.execute_index(i, cx);
    }

    fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let n = ENTRIES.len();
        match ui::key(ev) {
            "escape" => {
                if self.armed.is_some() {
                    self.armed = None;
                    cx.notify();
                } else {
                    self.close(cx);
                }
            }
            "left" => {
                self.selected = (self.selected + n - 1) % n;
                self.armed = None;
                cx.notify();
            }
            "right" | "tab" => {
                self.selected = (self.selected + 1) % n;
                self.armed = None;
                cx.notify();
            }
            "enter" => self.activate(self.selected, cx),
            other => {
                if let Some(i) = ENTRIES.iter().position(|e| e.key == other) {
                    self.activate(i, cx);
                }
            }
        }
    }
}

impl Render for PowerMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        sync_if_needed(&mut self.last_theme, cx);

        if !self.focus.is_focused(window) {
            window.focus(&self.focus, cx);
        }

        let clk = self.clock.read(cx);
        let time_hm = clk.time_hm();
        let bat_pct = self.battery.read(cx).percent();
        let bat_present = Battery::read_sysfs().is_some();

        let meta = if bat_present {
            format!("{time_hm} · {bat_pct}%")
        } else {
            time_hm
        };

        let header = div()
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
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(cx.theme().foreground)
                            .child(self.user_host.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("· Uptime {}", self.uptime_label)),
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
                            .text_xs()
                            .font_features(ui::tabular())
                            .text_color(cx.theme().muted_foreground)
                            .child(meta),
                    )
                    .child(
                        Button::new("pw-close")
                            .xsmall()
                            .ghost()
                            .icon(svg_icon(SVG_CLOSE))
                            .on_click(cx.listener(|this, _, _, cx| this.close(cx))),
                    ),
            );

        let mut cards_row = div().flex().flex_row().gap_2p5().w_full();
        for (i, e) in ENTRIES.iter().enumerate() {
            let selected = i == self.selected;
            let armed = self.armed == Some(i);
            let tone = if armed || e.destructive {
                cx.theme().danger
            } else {
                cx.theme().primary
            };

            let hover_bg = if armed {
                cx.theme().danger.opacity(0.22)
            } else {
                raised_bg().opacity(0.8)
            };

            cards_row = cards_row.child(
                div()
                    .id(("pw-card", i))
                    .flex_1()
                    .min_w_0()
                    .h(px(110.))
                    .px_2()
                    .py_3()
                    .rounded(px(11.))
                    .cursor_pointer()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .bg(if armed {
                        cx.theme().danger.opacity(0.18)
                    } else if selected {
                        raised_bg()
                    } else {
                        surface_bg().opacity(0.65)
                    })
                    .border_1()
                    .border_color(if armed {
                        cx.theme().danger
                    } else if selected {
                        tone.opacity(0.8)
                    } else {
                        hsla(0).opacity(0.0)
                    })
                    .hover(move |s| s.bg(hover_bg))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.activate(i, cx);
                    }))
                    .child(svg_icon(e.icon).large().text_color(if selected || armed {
                        tone
                    } else if e.destructive {
                        cx.theme().danger.opacity(0.85)
                    } else {
                        cx.theme().foreground
                    }))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().foreground)
                                    .child(e.label),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(e.key.to_uppercase()),
                            ),
                    ),
            );
        }

        let confirm_banner = self.armed.map(|idx| {
            let entry = &ENTRIES[idx];
            div()
                .w_full()
                .px_3p5()
                .py_2p5()
                .rounded(px(10.))
                .bg(cx.theme().danger.opacity(0.14))
                .border_1()
                .border_color(cx.theme().danger.opacity(0.55))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(svg_icon(SVG_ALERT).small().text_color(cx.theme().danger))
                        .child(
                            div()
                                .text_xs()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(cx.theme().foreground)
                                .child(format!(
                                    "Confirm \"{}\"? (Enter or [{}])",
                                    entry.label,
                                    entry.key.to_uppercase()
                                )),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .child(
                            Button::new("pw-arm-cancel")
                                .xsmall()
                                .ghost()
                                .label("Cancel")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.armed = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("pw-arm-ok")
                                .xsmall()
                                .danger()
                                .label(entry.label)
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.execute_index(idx, cx);
                                })),
                        ),
                )
        });

        let footer = div()
            .px_5()
            .py_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child("←/→ select · Enter confirm · L O S R P shortcuts")
            .child("Esc close");

        let (panel_w, _) = clamped_panel_size(window, 620., 360., 500., 240.);

        let panel = div()
            .occlude()
            .w(panel_w)
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
            .child(
                div()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(cards_row)
                    .children(confirm_banner),
            )
            .child(footer)
            .with_animation("pw-open", ui::anim_pop(), |el, delta| el.opacity(delta));

        ui::backdrop("pw-bg", ids::POWER, ui::DIM_MODAL)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| this.on_key(ev, cx)))
            .flex()
            .items_center()
            .justify_center()
            .child(panel)
    }
}
