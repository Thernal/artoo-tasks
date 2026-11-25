//! The board's loading without a jump (round 41, `design/explore/stability/tasks.html`), on the kit's three pieces:
//! the freshness slot in the bar (`Updated 14:35`, `Refreshing…`, `1 moved ↑`, `Can't reach · from 14:35 ↻`), the
//! hairline while a read takes long, and still tints where the board's issues will be. The store says what is
//! the case (`Store::slot`, `Store::reading_shown`); this keeps what moves between frames: the slot's cross-fade,
//! the hairline's clock and when the first read began.

use artoo_plugin_sdk::gpui::{self, AnyElement, App, Div, IntoElement, canvas, div, prelude::*};
use artoo_plugin_sdk::kit::fresh::{
    self, HairlineClock, SkeletonPhase, Slot, SlotAction, SlotFade,
};
use artoo_plugin_sdk::kit::{motion, px};
use std::cell::{Cell, RefCell};
use std::time::Instant;

/// What the board's slot, hairline and skeleton keep between frames.
#[derive(Default)]
pub struct Freshness {
    fade: RefCell<Option<SlotFade>>,
    hair: Cell<HairlineClock>,
    /// The read with nothing to show under it: when it began, when it answered.
    first: Cell<Option<(Instant, Option<Instant>)>>,
    /// Whether a read was on its way when last drawn.
    reading: Cell<bool>,
}

impl Freshness {
    /// What the store says as the page draws: a read on its way or not, and whether there is nothing to show under
    /// it. The hairline and the skeleton start and end as it changes.
    pub fn track(&self, reading: bool, nothing: bool, at: Instant) {
        if reading == self.reading.replace(reading) {
            return;
        }
        let mut hair = self.hair.get();
        if reading {
            hair.begin(at);
            let running = matches!(self.first.get(), Some((_, None)));
            if !(nothing && running) {
                self.first.set(nothing.then_some((at, None)));
            }
        } else {
            hair.end(at);
            if let Some((started, None)) = self.first.get() {
                self.first.set(Some((started, Some(at))));
            }
        }
        self.hair.set(hair);
    }

    /// A read as if begun at `at`, for a snapshot: the hairline and the tints already shown.
    pub fn force(&self, reading: bool, nothing: bool, at: Instant) {
        self.hair.set(HairlineClock::default());
        self.first.set(None);
        self.reading.set(false);
        self.track(reading, nothing, at);
    }

    /// How much of the skeleton and of the board show now.
    pub fn phase(&self, at: Instant) -> SkeletonPhase {
        match self.first.get() {
            Some((started, ready)) => fresh::skeleton_phase(started, ready, at),
            None => SkeletonPhase {
                skeleton: 0.,
                content: 1.,
            },
        }
    }

    /// Whether the skeleton's timing still moves by itself: waiting to show, held, or fading.
    pub fn skeleton_moves(&self, at: Instant) -> bool {
        match self.first.get() {
            Some((started, None)) => at < started + motion::SKELETON_AFTER,
            Some((_, Some(_))) => self.phase(at).content < 1.,
            None => false,
        }
    }

    /// The slot saying `slot`, cross-faded from what it said before; a click hands its action to `on_action`.
    pub fn slot(
        &self,
        slot: Slot,
        on_action: impl Fn(SlotAction, &mut App) + 'static,
    ) -> AnyElement {
        let at = Instant::now();
        let mut kept = self.fade.borrow_mut();
        let fade = kept.get_or_insert_with(|| SlotFade::new(slot.clone()));
        fade.set(slot, at);
        fresh::slot("tasks-fresh", fade, move |action, _, cx| {
            on_action(action, cx)
        })
    }

    /// The hairline along the board's top edge while a read takes long.
    pub fn hairline(&self, cx: &App) -> AnyElement {
        let at = Instant::now();
        let clock = self.hair.get();
        let opacity = clock.opacity(at);
        div()
            .absolute()
            .left_0()
            .right_0()
            .top_0()
            .h(px(fresh::HAIRLINE_HEIGHT))
            .child(fresh::hairline("tasks-hairline", opacity, cx))
            .when(clock.active(at) && opacity < 1., |s| s.child(frames()))
            .into_any_element()
    }
}

/// Asks for the next frame while something timed is on its way.
pub fn frames() -> impl IntoElement {
    canvas(
        |_, window, _| window.request_animation_frame(),
        |_, _, _, _| {},
    )
    .absolute()
    .size_0()
}

/// A tint of `width` standing for a line of text `height` tall, centred in a box of the line's height `line`.
fn tint(width: f32, height: f32, line: f32) -> Div {
    div()
        .h(px(line))
        .flex_none()
        .flex()
        .items_center()
        .child(fresh::tint(px(width).into(), height))
}

/// A chip's or a button's box as a tint.
fn pill(width: f32, height: f32) -> Div {
    fresh::tint(px(width).into(), height).rounded(px(5.))
}

/// The sprint's head as tints: its dates and days left, the progress and its count, the goal.
pub fn skeleton_sprint_head() -> Div {
    div()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(4.))
        .px(px(16.))
        .py(px(8.))
        .border_b_1()
        .border_color(artoo_plugin_sdk::kit::theme::border())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(20.))
                .child(tint(120., 8., 18.))
                .child(tint(58., 8., 18.))
                .child(div().flex_1())
                .child(tint(140., 4., 18.)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(tint(32., 8., 18.))
                .child(tint(288., 8., 18.)),
        )
}

/// A list row as tints in the real row's boxes: the assignee's face, the summary, the state's chip and the `⋯`; the
/// type's mark, the key, and the epic's chip at the end.
fn skeleton_row(summary: f32, epic: f32) -> Div {
    div()
        .px(px(16.))
        .py(px(8.))
        .flex()
        .flex_col()
        .gap(px(4.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(fresh::tint(px(20.).into(), 20.).rounded_full())
                .child(tint(summary, 8., 22.))
                .child(div().flex_1())
                .child(pill(54., 22.))
                .child(pill(22., 22.)),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .pl(px(30.))
                .child(pill(12., 12.))
                .child(tint(48., 8., 18.))
                .child(div().flex_1())
                .child(pill(epic, 16.)),
        )
}

/// The list's skeleton: the group's band, then rows of the widths a board's issues have.
pub fn skeleton_list() -> Div {
    const ROWS: [(f32, f32); 10] = [
        (150., 110.),
        (234., 110.),
        (132., 110.),
        (252., 110.),
        (252., 110.),
        (138., 110.),
        (210., 116.),
        (168., 110.),
        (186., 110.),
        (204., 110.),
    ];
    let mut list = div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .overflow_hidden()
        .child(
            div()
                .flex_none()
                .h(px(28.))
                .flex()
                .items_center()
                .gap(px(8.))
                .px(px(36.))
                .bg(artoo_plugin_sdk::kit::theme::panel())
                .child(tint(30., 8., 18.))
                .child(div().flex_1())
                .child(pill(22., 12.).rounded_full()),
        );
    for (summary, epic) in ROWS {
        list = list.child(skeleton_row(summary, epic));
    }
    list
}

/// The preview's skeleton: the key and the state, the summary, the details, the description and its buttons.
pub fn skeleton_preview() -> Div {
    let field = |w: f32| {
        div()
            .flex()
            .items_center()
            .gap(px(40.))
            .child(tint(40., 8., 24.))
            .child(tint(w, 8., 24.))
    };
    div()
        .size_full()
        .flex()
        .flex_col()
        .gap(px(14.))
        .px(px(24.))
        .py(px(22.))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .child(pill(14., 14.))
                .child(tint(60., 8., 18.))
                .child(div().flex_1())
                .child(tint(30., 8., 18.)),
        )
        .child(tint(214., 12., 26.))
        .child(
            div()
                .flex()
                .flex_col()
                .pb(px(14.))
                .border_b_1()
                .border_color(artoo_plugin_sdk::kit::theme::border())
                .child(field(130.))
                .child(field(64.))
                .child(field(24.))
                .child(field(170.)),
        )
        .child(tint(286., 8., 18.))
        .child(
            div()
                .flex()
                .gap(px(8.))
                .child(pill(66., 28.))
                .child(pill(124., 28.)),
        )
}

/// Tints over the board's place, as much as `phase` shows them; `moving` asks for frames while their timing moves.
pub fn veil(skeleton: Div, phase: SkeletonPhase, moving: bool) -> Div {
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .opacity(phase.skeleton)
        .child(skeleton)
        .when(moving, |d| d.child(frames()))
}

/// A message in the board's place: what is the case, why, and the way on.
pub fn message(title: String, why: String, action: Option<AnyElement>) -> Div {
    use artoo_plugin_sdk::kit::theme;
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .px(px(24.))
        .text_size(px(13.))
        .child(
            div()
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(theme::text())
                .child(title),
        )
        .child(
            div()
                .max_w(px(420.))
                .text_center()
                .text_size(px(12.))
                .text_color(theme::muted())
                .child(why),
        )
        .children(action)
}
