//! Centered Bento Control Center (`940x650`) built with `gpui-component`.
//!
//! Combines clean connectivity tiles with on-demand Wi-Fi / Bluetooth drawers,
//! continuous volume & brightness sliders, live system telemetry (Battery & RAM),
//! Pomodoro timer, MPRIS media player, top priority tasks from `TodoStore`,
//! quick shell/power actions, and Kumi theme switching.

use gpui::prelude::*;
use gpui::{
    div, px, AnimationExt, App, Context, Div, Entity, FocusHandle, FontWeight, KeyDownEvent,
    Window,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::progress::Progress;
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::slider::{Slider, SliderEvent, SliderState};
use gpui_component::switch::Switch;
use gpui_component::tag::{Tag, TagVariant};
use gpui_component::{ActiveTheme as _, Sizable as _};
use kumizo_kit::prelude::*;

use crate::{
    kit::{
        ensure_components_init, hsla, inset_row_bg, panel_bg, raised_bg, surface_bg, svg_icon,
        sync_gpui_theme, SVG_ARROW_RIGHT, SVG_BATTERY, SVG_BELL, SVG_BELL_OFF, SVG_BLUETOOTH,
        SVG_CHECK, SVG_CHECK_CIRCLE, SVG_CHEVRON_RIGHT, SVG_CLIPBOARD, SVG_CLOSE, SVG_CPU,
        SVG_LOCK, SVG_MOON, SVG_MUSIC, SVG_PALETTE, SVG_PAUSE, SVG_PLAY, SVG_POWER, SVG_ROTATE_CCW,
        SVG_SKIP_BACK, SVG_SKIP_FWD, SVG_SLIDERS, SVG_SUN, SVG_VOL_2, SVG_VOL_X, SVG_WIFI,
    },
    pomodoro::{self, Pomodoro},
    theme,
    todo_store::{self, TaskPriority, TodoStore},
    ui::{self, ids},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum SubView {
    None,
    Wifi,
    Bluetooth,
}

pub struct ControlCenter {
    clock: Entity<Clock>,
    audio: Entity<Audio>,
    brightness: Entity<Brightness>,
    bluetooth: Entity<Bluetooth>,
    network: Entity<Network>,
    battery: Entity<Battery>,
    memory: Entity<Memory>,
    mpris: Entity<Mpris>,
    notifs: Entity<Notifications>,
    pomodoro: Entity<Pomodoro>,
    todo: Entity<TodoStore>,

    sub_view: SubView,
    wifi_pw_for: Option<String>,
    wifi_pw_need_focus: bool,
    wifi_pw_input: Option<Entity<InputState>>,
    vol_slider: Option<Entity<SliderState>>,
    bri_slider: Option<Entity<SliderState>>,
    last_vol: u8,
    last_bri: u8,
    disk: (f64, f64, u8),
    last_theme: &'static str,
    focus: FocusHandle,
}

fn read_root_disk() -> (f64, f64, u8) {
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    let c_path = c"/";
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if rc != 0 || stat.f_blocks == 0 {
        return (0.0, 0.0, 0);
    }
    let frsize = if stat.f_frsize > 0 {
        stat.f_frsize as u64
    } else {
        stat.f_bsize as u64
    };
    let total = (stat.f_blocks as u64).saturating_mul(frsize);
    let used = (stat.f_blocks as u64)
        .saturating_sub(stat.f_bfree as u64)
        .saturating_mul(frsize);
    let avail = (stat.f_bavail as u64).saturating_mul(frsize);
    let usable = used.saturating_add(avail).max(1);
    let pct = ((used as f64 / usable as f64) * 100.0).round().clamp(0.0, 100.0) as u8;
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    (used as f64 / GIB, total as f64 / GIB, pct)
}

fn bento_card(cx: &App) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2p5()
        .p_3()
        .rounded(px(12.))
        .bg(surface_bg())
        .border_1()
        .border_color(cx.theme().border)
}

fn card_heading(icon: &'static [u8], title: &'static str, cx: &App) -> Div {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_2()
        .child(svg_icon(icon).small().text_color(cx.theme().primary))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(cx.theme().muted_foreground)
                .child(title.to_uppercase()),
        )
}

fn fmt_duration(d: std::time::Duration) -> String {
    let mins = d.as_secs() / 60;
    let h = mins / 60;
    let m = mins % 60;
    if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m")
    }
}

impl ControlCenter {
    pub fn new(cx: &mut Context<Self>) -> Self {
        ensure_components_init(cx);
        sync_gpui_theme(cx);

        let pomodoro = pomodoro::handle(cx);
        let todo = todo_store::handle(cx);
        let audio = cx.service::<Audio>();
        let brightness = cx.service::<Brightness>();
        let memory = cx.service::<Memory>();
        let last_vol = audio.read(cx).volume();
        let last_bri = brightness.read(cx).percent();

        cx.observe(&pomodoro, |_, _, cx| cx.notify()).detach();
        cx.observe(&todo, |_, _, cx| cx.notify()).detach();
        cx.observe(&memory, |this, _, cx| {
            this.disk = read_root_disk();
            cx.notify();
        })
        .detach();

        Self {
            clock: cx.service::<Clock>(),
            audio,
            brightness,
            bluetooth: cx.service::<Bluetooth>(),
            network: cx.service::<Network>(),
            battery: cx.service::<Battery>(),
            memory,
            mpris: cx.service::<Mpris>(),
            notifs: cx.service::<Notifications>(),
            pomodoro,
            todo,
            sub_view: SubView::None,
            wifi_pw_for: None,
            wifi_pw_need_focus: false,
            wifi_pw_input: None,
            vol_slider: None,
            bri_slider: None,
            last_vol,
            last_bri,
            disk: read_root_disk(),
            last_theme: theme::current().name,
            focus: cx.focus_handle(),
        }
    }

    fn ensure_sliders(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (Entity<SliderState>, Entity<SliderState>) {
        let cur_vol = self.audio.read(cx).volume();
        let cur_bri = self.brightness.read(cx).percent();

        if self.vol_slider.is_none() {
            let vol = cx.new(|_| {
                SliderState::new()
                    .min(0.0)
                    .max(100.0)
                    .step(1.0)
                    .default_value(cur_vol as f32)
            });
            cx.subscribe(&vol, |this, _, ev: &SliderEvent, cx| {
                if let SliderEvent::Change(val) = ev {
                    let pct = val.start().round().clamp(0.0, 100.0) as u8;
                    this.last_vol = pct;
                    this.audio.update(cx, |a, _| a.set_volume(pct));
                }
            })
            .detach();
            self.vol_slider = Some(vol);
        } else if cur_vol != self.last_vol {
            self.last_vol = cur_vol;
            if let Some(vol) = &self.vol_slider {
                vol.update(cx, |s, cx| s.set_value(cur_vol as f32, window, cx));
            }
        }

        if self.bri_slider.is_none() {
            let bri = cx.new(|_| {
                SliderState::new()
                    .min(5.0)
                    .max(100.0)
                    .step(1.0)
                    .default_value((cur_bri.max(5)) as f32)
            });
            cx.subscribe(&bri, |this, _, ev: &SliderEvent, cx| {
                if let SliderEvent::Change(val) = ev {
                    let pct = val.start().round().clamp(5.0, 100.0) as u8;
                    this.last_bri = pct;
                    this.brightness.update(cx, |b, _| b.set_percent(pct));
                }
            })
            .detach();
            self.bri_slider = Some(bri);
        } else if cur_bri != self.last_bri {
            self.last_bri = cur_bri;
            if let Some(bri) = &self.bri_slider {
                bri.update(cx, |s, cx| s.set_value(cur_bri as f32, window, cx));
            }
        }

        (
            self.vol_slider.as_ref().unwrap().clone(),
            self.bri_slider.as_ref().unwrap().clone(),
        )
    }

    fn ensure_wifi_pw_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(st) = &self.wifi_pw_input {
            return st.clone();
        }
        let st = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Mot de passe Wi-Fi…")
        });
        cx.subscribe_in(&st, window, |this, _, ev: &InputEvent, window, cx| {
            if matches!(ev, InputEvent::PressEnter { .. }) {
                this.wifi_submit_pw(window, cx);
            }
        })
        .detach();
        self.wifi_pw_input = Some(st.clone());
        st
    }

    fn wifi_submit_pw(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(ssid) = self.wifi_pw_for.clone() {
            let pw = self
                .wifi_pw_input
                .as_ref()
                .map(|s| s.read(cx).value().to_string())
                .unwrap_or_default();
            if !pw.is_empty() {
                self.network.update(cx, |n, _| {
                    n.connect_with_password(&ssid, &pw);
                });
            }
            if let Some(input) = &self.wifi_pw_input {
                input.update(cx, |s, cx| s.set_value("", window, cx));
            }
            self.wifi_pw_for = None;
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    fn wifi_cancel_pw(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(input) = &self.wifi_pw_input {
            input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.wifi_pw_for = None;
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if ui::key(ev) == "escape" {
            if self.wifi_pw_for.is_some() {
                self.wifi_cancel_pw(window, cx);
            } else if self.sub_view != SubView::None {
                self.sub_view = SubView::None;
                window.focus(&self.focus, cx);
                cx.notify();
            } else {
                cx.hide_window(ids::CONTROL);
            }
        }
    }

    fn render_header(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let clk = self.clock.read(cx);
        let time = clk.time_hm();
        let date = clk.date_formatted();
        let active_tasks = self.todo.read(cx).active_count();
        let unread_notifs = self.notifs.read(cx).unread_count();

        div()
            .px_5()
            .py_3p5()
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
                    .gap_3()
                    .child(
                        div()
                            .w(px(34.))
                            .h(px(34.))
                            .rounded(px(9.))
                            .bg(cx.theme().primary.opacity(0.14))
                            .border_1()
                            .border_color(cx.theme().primary.opacity(0.35))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(svg_icon(SVG_SLIDERS).text_color(cx.theme().primary)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(cx.theme().foreground)
                                    .child("Centre de contrôle"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_features(ui::tabular())
                                    .text_color(cx.theme().muted_foreground)
                                    .child(format!("{date} · {time}")),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .child(
                        Button::new("cc-act-clip")
                            .small()
                            .secondary()
                            .icon(svg_icon(SVG_CLIPBOARD))
                            .label("Presse-papiers")
                            .on_click(|_, _, cx| ui::toggle_exclusive(ids::CLIPBOARD, cx)),
                    )
                    .child(
                        Button::new("cc-act-todo")
                            .small()
                            .secondary()
                            .icon(svg_icon(SVG_CHECK_CIRCLE))
                            .label(if active_tasks > 0 {
                                format!("Tâches ({active_tasks})")
                            } else {
                                "Tâches".to_string()
                            })
                            .on_click(|_, _, cx| ui::toggle_exclusive(ids::TODO, cx)),
                    )
                    .child(
                        Button::new("cc-act-notifs")
                            .small()
                            .secondary()
                            .icon(svg_icon(SVG_BELL))
                            .label(if unread_notifs > 0 {
                                format!("Notifs ({unread_notifs})")
                            } else {
                                "Notifs".to_string()
                            })
                            .on_click(|_, _, cx| ui::toggle_exclusive(ids::NOTIFS, cx)),
                    )
                    .child(div().w(px(1.)).h(px(20.)).mx_1().bg(cx.theme().border))
                    .child(
                        Button::new("cc-act-lock")
                            .small()
                            .ghost()
                            .icon(svg_icon(SVG_LOCK))
                            .on_click(|_, _, cx| {
                                cx.hide_window(ids::CONTROL);
                                ui::run("kumi-lockscreen", &[]);
                            }),
                    )
                    .child(
                        Button::new("cc-act-suspend")
                            .small()
                            .ghost()
                            .icon(svg_icon(SVG_MOON))
                            .on_click(|_, _, cx| {
                                cx.hide_window(ids::CONTROL);
                                ui::run("systemctl", &["suspend"]);
                            }),
                    )
                    .child(
                        Button::new("cc-act-power")
                            .small()
                            .ghost()
                            .icon(svg_icon(SVG_POWER))
                            .on_click(|_, _, cx| ui::toggle_exclusive(ids::POWER, cx)),
                    )
                    .child(
                        Button::new("cc-close")
                            .small()
                            .ghost()
                            .icon(svg_icon(SVG_CLOSE))
                            .on_click(|_, _, cx| cx.hide_window(ids::CONTROL)),
                    ),
            )
    }

    fn render_connectivity_card(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let net = self.network.read(cx);
        let (wifi_avail, radio, wifi_on, ssid, signal) = (
            net.has_radio(),
            net.wifi_enabled(),
            net.is_connected(),
            net.ssid().map(str::to_string),
            net.signal_percent(),
        );
        let bt = self.bluetooth.read(cx);
        let (bt_avail, bt_on, bt_count) = (bt.is_available(), bt.is_powered(), bt.connected_count());
        let bt_first_name = bt.connected_devices().next().map(|d| d.display_name().to_string());
        let dnd = self.notifs.read(cx).is_dnd();

        let wifi_sub = if !wifi_avail {
            "Indisponible".to_string()
        } else if !radio {
            "Désactivé".to_string()
        } else if wifi_on {
            let sig = signal.map(|s| format!(" · {s}%")).unwrap_or_default();
            format!("{}{sig}", ssid.unwrap_or_else(|| "Connecté".into()))
        } else {
            "Hors ligne · Voir réseaux".to_string()
        };

        let bt_sub = if !bt_avail {
            "Indisponible".to_string()
        } else if !bt_on {
            "Désactivé".to_string()
        } else if let Some(name) = bt_first_name {
            if bt_count > 1 {
                format!("{name} (+{})", bt_count - 1)
            } else {
                name
            }
        } else {
            "Activé · Voir appareils".to_string()
        };

        bento_card(cx)
            .child(card_heading(SVG_WIFI, "Connectivité", cx))
            .child(
                div()
                    .id("cc-row-wifi")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2p5()
                    .px_2p5()
                    .py_2()
                    .rounded(px(9.))
                    .bg(inset_row_bg())
                    .border_1()
                    .border_color(cx.theme().border)
                    .cursor_pointer()
                    .hover(|s| s.border_color(cx.theme().primary.opacity(0.6)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sub_view = SubView::Wifi;
                        this.network.update(cx, |n, _| n.request_scan());
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(30.))
                            .h(px(30.))
                            .rounded(px(8.))
                            .bg(if radio {
                                cx.theme().primary.opacity(0.16)
                            } else {
                                cx.theme().muted
                            })
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(svg_icon(SVG_WIFI).small().text_color(if radio {
                                cx.theme().primary
                            } else {
                                cx.theme().muted_foreground
                            })),
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
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().foreground)
                                    .child("Wi-Fi"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .truncate()
                                    .child(wifi_sub),
                            ),
                    )
                    .child(
                        div()
                            .id("cc-sw-wifi-wrap")
                            .on_click(|_, _, cx| cx.stop_propagation())
                            .child(
                                Switch::new("cc-sw-wifi")
                                    .small()
                                    .checked(radio)
                                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                        let target = *checked;
                                        this.network.update(cx, |n, cx| {
                                            if n.set_enabled(target).is_some() {
                                                cx.notify();
                                            }
                                        });
                                    })),
                            ),
                    )
                    .child(
                        svg_icon(SVG_CHEVRON_RIGHT)
                            .small()
                            .text_color(cx.theme().muted_foreground),
                    ),
            )
            .child(
                div()
                    .id("cc-row-bt")
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2p5()
                    .px_2p5()
                    .py_2()
                    .rounded(px(9.))
                    .bg(inset_row_bg())
                    .border_1()
                    .border_color(cx.theme().border)
                    .cursor_pointer()
                    .hover(|s| s.border_color(cx.theme().primary.opacity(0.6)))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.sub_view = SubView::Bluetooth;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(30.))
                            .h(px(30.))
                            .rounded(px(8.))
                            .bg(if bt_on {
                                cx.theme().primary.opacity(0.16)
                            } else {
                                cx.theme().muted
                            })
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(svg_icon(SVG_BLUETOOTH).small().text_color(if bt_on {
                                cx.theme().primary
                            } else {
                                cx.theme().muted_foreground
                            })),
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
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().foreground)
                                    .child("Bluetooth"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .truncate()
                                    .child(bt_sub),
                            ),
                    )
                    .child(
                        div()
                            .id("cc-sw-bt-wrap")
                            .on_click(|_, _, cx| cx.stop_propagation())
                            .child(
                                Switch::new("cc-sw-bt")
                                    .small()
                                    .checked(bt_on)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.bluetooth.update(cx, |b, _| b.toggle_power());
                                    })),
                            ),
                    )
                    .child(
                        svg_icon(SVG_CHEVRON_RIGHT)
                            .small()
                            .text_color(cx.theme().muted_foreground),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2p5()
                    .px_2p5()
                    .py_2()
                    .rounded(px(9.))
                    .bg(inset_row_bg())
                    .border_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .w(px(30.))
                            .h(px(30.))
                            .rounded(px(8.))
                            .bg(if dnd {
                                cx.theme().warning.opacity(0.18)
                            } else {
                                cx.theme().muted
                            })
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                svg_icon(if dnd { SVG_BELL_OFF } else { SVG_BELL })
                                    .small()
                                    .text_color(if dnd {
                                        cx.theme().warning
                                    } else {
                                        cx.theme().muted_foreground
                                    }),
                            ),
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
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(cx.theme().foreground)
                                    .child("Ne pas déranger"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(if dnd { "Silencieux" } else { "Notifications actives" }),
                            ),
                    )
                    .child(
                        Switch::new("cc-sw-dnd")
                            .small()
                            .checked(dnd)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notifs.update(cx, |n, _| n.toggle_dnd());
                            })),
                    ),
            )
    }

    fn render_sliders_card(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (vol_slider, bri_slider) = self.ensure_sliders(window, cx);
        let a = self.audio.read(cx);
        let (vol, muted, sink) = (a.volume(), a.is_muted(), a.sink_name().to_string());
        let bri = self.brightness.read(cx).percent();

        bento_card(cx)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(card_heading(SVG_VOL_2, "Audio & Écran", cx))
                    .when(!sink.is_empty(), |d| {
                        d.child(
                            div()
                                .max_w(px(140.))
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .truncate()
                                .child(sink),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2p5()
                    .child(
                        Button::new("cc-mute-btn")
                            .xsmall()
                            .ghost()
                            .icon(svg_icon(if muted || vol == 0 {
                                SVG_VOL_X
                            } else {
                                SVG_VOL_2
                            }))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.audio.update(cx, |a, _| a.toggle_mute());
                            })),
                    )
                    .child(div().flex_1().min_w_0().child(Slider::new(&vol_slider)))
                    .child(
                        div()
                            .w(px(40.))
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .font_features(ui::tabular())
                            .text_color(if muted {
                                cx.theme().muted_foreground
                            } else {
                                cx.theme().foreground
                            })
                            .child(if muted {
                                "Muet".to_string()
                            } else {
                                format!("{vol}%")
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2p5()
                    .child(
                        div()
                            .w(px(24.))
                            .flex()
                            .justify_center()
                            .child(svg_icon(SVG_SUN).small().text_color(cx.theme().info)),
                    )
                    .child(div().flex_1().min_w_0().child(Slider::new(&bri_slider)))
                    .child(
                        div()
                            .w(px(40.))
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .font_features(ui::tabular())
                            .text_color(cx.theme().foreground)
                            .child(format!("{bri}%")),
                    ),
            )
    }

    fn render_telemetry_card(&self, cx: &App) -> impl IntoElement {
        let bat = self.battery.read(cx);
        let bat_pct = bat.percent();
        let bat_state = bat.state();
        let watts = bat.energy_rate_watts();
        let bat_time = match bat_state {
            BatteryState::Charging => bat.time_to_full().map(|d| format!("Pleine dans {}", fmt_duration(d))),
            BatteryState::Discharging => bat.time_to_empty().map(|d| format!("Reste {}", fmt_duration(d))),
            BatteryState::FullyCharged => Some("Chargée".to_string()),
            _ => None,
        };
        let bat_detail = match (bat_time, watts) {
            (Some(t), Some(w)) if w > 0.1 => format!("{t} · {w:.1}W"),
            (Some(t), _) => t,
            (None, Some(w)) if w > 0.1 => format!("{} · {w:.1}W", bat_state.as_str()),
            _ => bat_state.as_str().to_string(),
        };
        let bat_color = if bat_state == BatteryState::Charging || bat_state == BatteryState::FullyCharged {
            cx.theme().success
        } else if bat_pct <= 15 {
            cx.theme().danger
        } else if bat_pct <= 30 {
            cx.theme().warning
        } else {
            cx.theme().primary
        };

        let mem = self.memory.read(cx);
        let mem_pct = mem.percent();
        let mem_used = mem.used_gib();
        let mem_total = mem.total_gib();
        let mem_color = if mem_pct >= 90 {
            cx.theme().danger
        } else if mem_pct >= 75 {
            cx.theme().warning
        } else {
            cx.theme().info
        };

        let (disk_used, disk_total, disk_pct) = self.disk;
        let disk_color = if disk_pct >= 90 {
            cx.theme().danger
        } else if disk_pct >= 75 {
            cx.theme().warning
        } else {
            cx.theme().primary
        };

        bento_card(cx)
            .child(card_heading(SVG_CPU, "Système", cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .text_xs()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .child(svg_icon(SVG_BATTERY).xsmall().text_color(bat_color))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(cx.theme().foreground)
                                            .child("Batterie"),
                                    ),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .font_features(ui::tabular())
                                    .text_color(bat_color)
                                    .child(format!("{bat_pct}%")),
                            ),
                    )
                    .child(
                        Progress::new("cc-bat-prog")
                            .value(bat_pct as f32)
                            .color(bat_color),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .truncate()
                            .child(bat_detail),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .justify_between()
                            .text_xs()
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .items_center()
                                    .gap_1p5()
                                    .child(svg_icon(SVG_CPU).xsmall().text_color(mem_color))
                                    .child(
                                        div()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(cx.theme().foreground)
                                            .child("Mémoire RAM"),
                                    ),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .font_features(ui::tabular())
                                    .text_color(cx.theme().foreground)
                                    .child(format!("{mem_used:.1} / {mem_total:.1} Go ({mem_pct}%)")),
                            ),
                    )
                    .child(
                        Progress::new("cc-ram-prog")
                            .value(mem_pct as f32)
                            .color(mem_color),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
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
                                    .child("Disque /"),
                            )
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .font_features(ui::tabular())
                                    .text_color(cx.theme().foreground)
                                    .child(format!("{disk_used:.0} / {disk_total:.0} Go ({disk_pct}%)")),
                            ),
                    )
                    .child(
                        Progress::new("cc-disk-prog")
                            .value(disk_pct as f32)
                            .color(disk_color),
                    ),
            )
    }

    fn render_pomodoro_card(&self, cx: &Context<Self>) -> impl IntoElement {
        let p = self.pomodoro.read(cx);
        let (phase, running, time, frac, done, sessions) = (
            p.phase,
            p.running,
            p.mmss(),
            p.progress(),
            p.cycle_done(),
            p.sessions,
        );
        let phase_color = match phase {
            pomodoro::Phase::Focus => cx.theme().primary,
            pomodoro::Phase::ShortBreak => cx.theme().success,
            pomodoro::Phase::LongBreak => cx.theme().info,
        };
        let (pm_toggle, pm_reset, pm_skip) = (
            self.pomodoro.clone(),
            self.pomodoro.clone(),
            self.pomodoro.clone(),
        );

        let mut dots = div().flex().flex_row().items_center().gap(px(5.));
        for i in 0..4u32 {
            dots = dots.child(
                div()
                    .size(px(8.))
                    .rounded_full()
                    .bg(if i < done {
                        phase_color
                    } else {
                        cx.theme().border
                    }),
            );
        }

        bento_card(cx)
            .child(
                div()
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
                            .child(Tag::new().with_variant(TagVariant::Primary).xsmall().child(phase.label()))
                            .child(dots),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("{sessions} session(s)")),
                    ),
            )
            .child(
                div()
                    .py_1()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_3xl()
                            .font_weight(FontWeight::BOLD)
                            .font_features(ui::tabular())
                            .text_color(cx.theme().foreground)
                            .child(time),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1p5()
                            .child(
                                Button::new("cc-pomo-toggle")
                                    .small()
                                    .primary()
                                    .icon(svg_icon(if running { SVG_PAUSE } else { SVG_PLAY }))
                                    .label(if running { "Pause" } else { "Focus" })
                                    .on_click(move |_, _, cx| {
                                        pm_toggle.update(cx, |p, cx| p.toggle(cx));
                                    }),
                            )
                            .child(
                                Button::new("cc-pomo-skip")
                                    .small()
                                    .secondary()
                                    .icon(svg_icon(SVG_SKIP_FWD))
                                    .on_click(move |_, _, cx| {
                                        pm_skip.update(cx, |p, cx| p.skip(cx));
                                    }),
                            )
                            .child(
                                Button::new("cc-pomo-reset")
                                    .small()
                                    .ghost()
                                    .icon(svg_icon(SVG_ROTATE_CCW))
                                    .on_click(move |_, _, cx| {
                                        pm_reset.update(cx, |p, cx| p.reset(cx));
                                    }),
                            ),
                    ),
            )
            .child(
                Progress::new("cc-pomo-prog")
                    .value(frac * 100.0)
                    .color(phase_color),
            )
    }

    fn render_media_card(&self, cx: &Context<Self>) -> impl IntoElement {
        let m = self.mpris.read(cx);
        let title = m.title().map(str::to_string);
        let artist = m.artist().map(str::to_string).unwrap_or_default();
        let player = m.player_name().map(str::to_string);
        let playing = matches!(m.status(), PlaybackStatus::Playing);
        let (p, pp, n) = (self.mpris.clone(), self.mpris.clone(), self.mpris.clone());

        bento_card(cx)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(card_heading(SVG_MUSIC, "Média", cx))
                    .when_some(player, |d, name| {
                        d.child(
                            Tag::new()
                                .with_variant(TagVariant::Secondary)
                                .xsmall()
                                .child(name),
                        )
                    }),
            )
            .child(match title {
                Some(t) => div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
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
                                    .child(t),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .truncate()
                                    .child(if artist.is_empty() {
                                        "Lecteur actif".to_string()
                                    } else {
                                        artist
                                    }),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(
                                Button::new("cc-mp-prev")
                                    .small()
                                    .ghost()
                                    .icon(svg_icon(SVG_SKIP_BACK))
                                    .on_click(move |_, _, cx| {
                                        p.update(cx, |m, _| m.previous());
                                    }),
                            )
                            .child(
                                Button::new("cc-mp-play")
                                    .small()
                                    .primary()
                                    .icon(svg_icon(if playing { SVG_PAUSE } else { SVG_PLAY }))
                                    .on_click(move |_, _, cx| {
                                        pp.update(cx, |m, _| m.play_pause());
                                    }),
                            )
                            .child(
                                Button::new("cc-mp-next")
                                    .small()
                                    .ghost()
                                    .icon(svg_icon(SVG_SKIP_FWD))
                                    .on_click(move |_, _, cx| {
                                        n.update(cx, |m, _| m.next());
                                    }),
                            ),
                    )
                    .into_any_element(),
                None => div()
                    .py_2()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Aucun média en cours de lecture")
                    .into_any_element(),
            })
    }

    fn render_tasks_card(&self, cx: &Context<Self>) -> impl IntoElement {
        let store_ref = self.todo.read(cx);
        let active_count = store_ref.active_count();
        let top_tasks: Vec<_> = store_ref.focus_tasks().into_iter().take(3).cloned().collect();
        let store = self.todo.clone();

        let mut list = div().flex().flex_col().gap_1p5();
        if top_tasks.is_empty() {
            list = list.child(
                div()
                    .py_4()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Aucune tâche en attente"),
            );
        } else {
            for item in top_tasks {
                let id = item.id;
                let s_tog = store.clone();
                let prio_col = match item.priority {
                    TaskPriority::High => cx.theme().danger,
                    TaskPriority::Normal => cx.theme().primary,
                    TaskPriority::Low => cx.theme().muted_foreground,
                };
                let prio_var = match item.priority {
                    TaskPriority::High => TagVariant::Danger,
                    TaskPriority::Normal => TagVariant::Info,
                    TaskPriority::Low => TagVariant::Secondary,
                };
                list = list.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_2p5()
                        .py_1p5()
                        .rounded(px(8.))
                        .bg(inset_row_bg())
                        .border_1()
                        .border_color(cx.theme().border)
                        .child(
                            div()
                                .id(("cc-todo-cb", id as usize))
                                .w(px(16.))
                                .h(px(16.))
                                .flex_shrink_0()
                                .rounded(px(4.))
                                .border_1()
                                .border_color(prio_col)
                                .cursor_pointer()
                                .hover(|s| s.bg(prio_col.opacity(0.18)))
                                .on_click(move |_, _, cx| {
                                    s_tog.update(cx, |s, cx| s.toggle_done(id, cx));
                                }),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_xs()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().foreground)
                                .truncate()
                                .child(item.title),
                        )
                        .child(
                            Tag::new()
                                .with_variant(prio_var)
                                .xsmall()
                                .child(item.priority.label()),
                        ),
                );
            }
        }

        bento_card(cx)
            .flex_1()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(card_heading(SVG_CHECK_CIRCLE, "Prochaines tâches", cx))
                    .child(
                        Button::new("cc-todo-open")
                            .xsmall()
                            .ghost()
                            .icon(svg_icon(SVG_ARROW_RIGHT))
                            .label(format!("{active_count}"))
                            .on_click(|_, _, cx| ui::toggle_exclusive(ids::TODO, cx)),
                    ),
            )
            .child(list)
    }

    fn render_themes_card(&self, cx: &Context<Self>) -> impl IntoElement {
        let current = theme::current_index();
        let mut rows = div().flex().flex_col().gap_1p5();

        for (i, th) in theme::THEMES.iter().enumerate() {
            let selected = i == current;
            rows = rows.child(
                div()
                    .id(("cc-theme-row", i))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2p5()
                    .px_2p5()
                    .py_1p5()
                    .rounded(px(8.))
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
                    .hover(|s| s.border_color(cx.theme().primary.opacity(0.6)))
                    .on_click(move |_, _, cx| {
                        if let Some(key) = theme::system_key(i) {
                            theme::apply_system(key);
                        } else {
                            theme::set(i, cx);
                        }
                    })
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_1()
                            .child(
                                div()
                                    .size(px(10.))
                                    .rounded_full()
                                    .border_1()
                                    .border_color(hsla(th.border))
                                    .bg(hsla(th.bg)),
                            )
                            .child(div().size(px(10.)).rounded_full().bg(hsla(th.accent)))
                            .child(div().size(px(10.)).rounded_full().bg(hsla(th.accent2))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_xs()
                            .font_weight(if selected {
                                FontWeight::SEMIBOLD
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(cx.theme().foreground)
                            .truncate()
                            .child(th.name),
                    )
                    .children(
                        selected.then(|| svg_icon(SVG_CHECK).xsmall().text_color(cx.theme().primary)),
                    ),
            );
        }

        bento_card(cx)
            .h_full()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(card_heading(SVG_PALETTE, "Thèmes", cx))
                    .child(
                        Tag::new()
                            .with_variant(TagVariant::Primary)
                            .xsmall()
                            .child(theme::current().name),
                    ),
            )
            .child(div().flex_1().min_h_0().overflow_y_scrollbar().child(rows))
    }

    fn render_wifi_modal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (enabled, networks) = {
            let net = self.network.read(cx);
            (net.wifi_enabled(), net.networks().to_vec())
        };
        let pw_input = self.ensure_wifi_pw_input(window, cx);
        if self.wifi_pw_need_focus {
            self.wifi_pw_need_focus = false;
            pw_input.update(cx, |st, cx| st.focus(window, cx));
        }

        let mut list = div().flex().flex_col().gap_1p5();
        if !enabled {
            list = list.child(
                div()
                    .py_8()
                    .flex()
                    .justify_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Le Wi-Fi est désactivé — activez l'interrupteur ci-dessus."),
            );
        } else if networks.is_empty() {
            list = list.child(
                div()
                    .py_8()
                    .flex()
                    .justify_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Aucun réseau détecté — lancez un scan."),
            );
        } else {
            for (i, n) in networks.into_iter().enumerate() {
                let (ssid, strength, secured, active) = (n.ssid, n.strength, n.secured, n.active);
                let pw_open = self.wifi_pw_for.as_deref() == Some(ssid.as_str());
                let click_ssid = ssid.clone();

                list = list.child(
                    div()
                        .id(("cc-modal-wifi", i))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .py_2()
                        .rounded(px(9.))
                        .cursor_pointer()
                        .bg(if active {
                            raised_bg()
                        } else {
                            surface_bg()
                        })
                        .border_1()
                        .border_color(if active {
                            cx.theme().primary
                        } else {
                            cx.theme().border
                        })
                        .hover(|s| s.border_color(cx.theme().primary.opacity(0.6)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if active {
                                this.network.update(cx, |net, _| {
                                    net.disconnect();
                                });
                            } else if secured {
                                let ok = this.network.update(cx, |net, _| net.connect(&click_ssid));
                                if !ok {
                                    if let Some(inp) = &this.wifi_pw_input {
                                        inp.update(cx, |s, cx| s.set_value("", window, cx));
                                    }
                                    this.wifi_pw_for = Some(click_ssid.clone());
                                    this.wifi_pw_need_focus = true;
                                    cx.notify();
                                }
                            } else {
                                this.network.update(cx, |net, _| {
                                    net.connect(&click_ssid);
                                });
                            }
                        }))
                        .child(svg_icon(SVG_WIFI).small().text_color(if active {
                            cx.theme().primary
                        } else {
                            cx.theme().muted_foreground
                        }))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().foreground)
                                .truncate()
                                .child(ssid),
                        )
                        .children(
                            secured.then(|| {
                                svg_icon(SVG_LOCK)
                                    .xsmall()
                                    .text_color(cx.theme().muted_foreground)
                            }),
                        )
                        .child(
                            div()
                                .text_xs()
                                .font_features(ui::tabular())
                                .text_color(cx.theme().muted_foreground)
                                .child(format!("{strength}%")),
                        )
                        .children(active.then(|| {
                            Tag::new()
                                .with_variant(TagVariant::Primary)
                                .xsmall()
                                .child("Connecté")
                        })),
                );

                if pw_open {
                    list = list.child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .p_2()
                            .rounded(px(9.))
                            .bg(surface_bg())
                            .border_1()
                            .border_color(cx.theme().primary)
                            .child(div().flex_1().min_w_0().child(Input::new(&pw_input).small()))
                            .child(
                                Button::new("cc-wifi-pw-cancel")
                                    .xsmall()
                                    .ghost()
                                    .label("Annuler")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.wifi_cancel_pw(window, cx);
                                    })),
                            )
                            .child(
                                Button::new("cc-wifi-pw-ok")
                                    .xsmall()
                                    .primary()
                                    .label("Connecter")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.wifi_submit_pw(window, cx);
                                    })),
                            ),
                    );
                }
            }
        }

        let card = div()
            .occlude()
            .w(px(520.))
            .h(px(480.))
            .flex()
            .flex_col()
            .rounded(px(14.))
            .bg(panel_bg())
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
                            .gap_2p5()
                            .child(svg_icon(SVG_WIFI).text_color(cx.theme().primary))
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::BOLD)
                                    .child("Réseaux Wi-Fi"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                Switch::new("cc-modal-wifi-sw")
                                    .small()
                                    .checked(enabled)
                                    .on_click(cx.listener(|this, checked: &bool, _, cx| {
                                        let target = *checked;
                                        this.network.update(cx, |n, cx| {
                                            if n.set_enabled(target).is_some() {
                                                cx.notify();
                                            }
                                        });
                                    })),
                            )
                            .child(
                                Button::new("cc-modal-wifi-scan")
                                    .xsmall()
                                    .secondary()
                                    .label("Scanner")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.network.update(cx, |n, _| n.request_scan());
                                    })),
                            )
                            .child(
                                Button::new("cc-modal-wifi-close")
                                    .xsmall()
                                    .ghost()
                                    .icon(svg_icon(SVG_CLOSE))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.sub_view = SubView::None;
                                        this.wifi_pw_for = None;
                                        window.focus(&this.focus, cx);
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(div().flex_1().min_h_0().overflow_y_scrollbar().p_4().child(list))
            .with_animation("cc-sub-wifi", ui::anim_pop(), |el, d| el.opacity(d));

        div()
            .id("cc-sub-overlay")
            .absolute()
            .inset_0()
            .bg(hsla(theme::current().bg).opacity(0.70))
            .flex()
            .items_center()
            .justify_center()
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.sub_view = SubView::None;
                this.wifi_pw_for = None;
                window.focus(&this.focus, cx);
                cx.notify();
            }))
            .child(card)
    }

    fn render_bt_modal(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let bt = self.bluetooth.read(cx);
        let (avail, powered, scanning, devices) = (
            bt.is_available(),
            bt.is_powered(),
            bt.is_discovering(),
            bt.devices().to_vec(),
        );

        let mut list = div().flex().flex_col().gap_1p5();
        if !avail {
            list = list.child(
                div()
                    .py_8()
                    .flex()
                    .justify_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Adaptateur Bluetooth indisponible."),
            );
        } else if !powered {
            list = list.child(
                div()
                    .py_8()
                    .flex()
                    .justify_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Bluetooth désactivé — activez l'interrupteur ci-dessus."),
            );
        } else if devices.is_empty() {
            list = list.child(
                div()
                    .py_8()
                    .flex()
                    .justify_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Aucun appareil — lancez une recherche."),
            );
        } else {
            for (i, d) in devices.into_iter().enumerate() {
                let connected = d.connected;
                let name = d.display_name().to_string();
                let path = d.path;
                list = list.child(
                    div()
                        .id(("cc-modal-bt", i))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_3()
                        .px_3()
                        .py_2()
                        .rounded(px(9.))
                        .cursor_pointer()
                        .bg(if connected {
                            raised_bg()
                        } else {
                            surface_bg()
                        })
                        .border_1()
                        .border_color(if connected {
                            cx.theme().primary
                        } else {
                            cx.theme().border
                        })
                        .hover(|s| s.border_color(cx.theme().primary.opacity(0.6)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let p = path.clone();
                            this.bluetooth.update(cx, |b, _| {
                                b.toggle_device_connection(p, connected);
                            });
                        }))
                        .child(svg_icon(SVG_BLUETOOTH).small().text_color(if connected {
                            cx.theme().primary
                        } else {
                            cx.theme().muted_foreground
                        }))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(cx.theme().foreground)
                                .truncate()
                                .child(name),
                        )
                        .child(
                            Tag::new()
                                .with_variant(if connected {
                                    TagVariant::Primary
                                } else {
                                    TagVariant::Secondary
                                })
                                .xsmall()
                                .child(if connected { "Connecté" } else { "Appairé" }),
                        ),
                );
            }
        }

        let card = div()
            .occlude()
            .w(px(520.))
            .h(px(480.))
            .flex()
            .flex_col()
            .rounded(px(14.))
            .bg(panel_bg())
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
                            .gap_2p5()
                            .child(svg_icon(SVG_BLUETOOTH).text_color(cx.theme().primary))
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::BOLD)
                                    .child("Appareils Bluetooth"),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .child(
                                Switch::new("cc-modal-bt-sw")
                                    .small()
                                    .checked(powered)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.bluetooth.update(cx, |b, _| b.toggle_power());
                                    })),
                            )
                            .child(
                                Button::new("cc-modal-bt-scan")
                                    .xsmall()
                                    .map(|b| if scanning { b.primary() } else { b.secondary() })
                                    .label(if scanning { "Recherche…" } else { "Rechercher" })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.bluetooth.update(cx, |b, _| b.toggle_scan());
                                    })),
                            )
                            .child(
                                Button::new("cc-modal-bt-close")
                                    .xsmall()
                                    .ghost()
                                    .icon(svg_icon(SVG_CLOSE))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.sub_view = SubView::None;
                                        window.focus(&this.focus, cx);
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .child(div().flex_1().min_h_0().overflow_y_scrollbar().p_4().child(list))
            .with_animation("cc-sub-bt", ui::anim_pop(), |el, d| el.opacity(d));

        div()
            .id("cc-sub-overlay")
            .absolute()
            .inset_0()
            .bg(hsla(theme::current().bg).opacity(0.70))
            .flex()
            .items_center()
            .justify_center()
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.sub_view = SubView::None;
                window.focus(&this.focus, cx);
                cx.notify();
            }))
            .child(card)
    }
}

impl Render for ControlCenter {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        ensure_components_init(cx);
        let active_theme_name = theme::current().name;
        if self.last_theme != active_theme_name {
            self.last_theme = active_theme_name;
            sync_gpui_theme(cx);
        }

        if !self.focus.is_focused(window) && self.wifi_pw_for.is_none() {
            window.focus(&self.focus, cx);
        }

        let header = self.render_header(cx);
        let connectivity = self.render_connectivity_card(cx);
        let sliders = self.render_sliders_card(window, cx);
        let telemetry = self.render_telemetry_card(cx);
        let pomodoro = self.render_pomodoro_card(cx);
        let media = self.render_media_card(cx);
        let tasks = self.render_tasks_card(cx);
        let themes = self.render_themes_card(cx);

        let left_col = div()
            .w(px(312.))
            .h_full()
            .flex_shrink_0()
            .overflow_y_scrollbar()
            .flex()
            .flex_col()
            .gap_2p5()
            .child(connectivity)
            .child(sliders)
            .child(telemetry);

        let mid_col = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_y_scrollbar()
            .flex()
            .flex_col()
            .gap_2p5()
            .child(pomodoro)
            .child(media)
            .child(tasks);

        let right_col = div().w(px(250.)).h_full().flex_shrink_0().child(themes);

        let bento = div()
            .flex_1()
            .min_h_0()
            .p_4()
            .flex()
            .flex_row()
            .gap_3()
            .child(left_col)
            .child(mid_col)
            .child(right_col);

        let vp = window.viewport_size();
        let panel_w = px(960.).min((vp.width - px(48.)).max(px(720.)));
        let panel_h = px(690.).min((vp.height - px(48.)).max(px(520.)));

        let mut shell = div()
            .occlude()
            .relative()
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
            .child(bento);

        match self.sub_view {
            SubView::Wifi => {
                let modal = self.render_wifi_modal(window, cx);
                shell = shell.child(modal);
            }
            SubView::Bluetooth => {
                let modal = self.render_bt_modal(cx);
                shell = shell.child(modal);
            }
            SubView::None => {}
        }

        let shell = shell.with_animation("cc-open", ui::anim_pop(), |el, d| el.opacity(d));

        ui::backdrop("cc-bg", ids::CONTROL, 0.48)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                this.on_key(ev, window, cx);
            }))
            .flex()
            .items_center()
            .justify_center()
            .child(shell)
    }
}
