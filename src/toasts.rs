//! Toasts: ephemeral popups in the top-right corner (window opened/closed by the bar).

use gpui::prelude::*;
use gpui::{div, px, AnimationExt, Context, Entity, Window};
use kumizo_kit::prelude::*;

use crate::{notifs, ui};

pub struct NotificationToasts {
    notifs: Entity<Notifications>,
}

impl NotificationToasts {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            notifs: cx.service::<Notifications>(),
        }
    }
}

impl Render for NotificationToasts {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let toasts = self.notifs.read(cx);
        let items = toasts.toasts();
        if items.is_empty() {
            return div();
        }
        let mut col = div()
            .size_full()
            .flex()
            .flex_col()
            .items_end()
            .gap_2()
            .pt(px(6.));
        for n in items.iter() {
            col = col.child(
                notifs::card("toast", n, self.notifs.clone(), false)
                    .with_animation("toast-in", ui::anim_toast(), |el, delta| {
                        el.opacity(delta).mr(px(12.0 * (1.0 - delta)))
                    }),
            );
        }
        col
    }
}
