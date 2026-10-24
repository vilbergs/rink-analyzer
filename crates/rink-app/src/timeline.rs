//! The timeline strip: events on top, the selected entity's keyframes below,
//! and a playhead you can scrub.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    canvas, div, point, prelude::*, px, quad, rgb, size, Bounds, Context, Entity, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels, Point, Window,
};
use rink_core::{EventKind, Play};

use crate::paint::paint_text;
use crate::state::{snap, PlayState};

const PAD_PX: f32 = 16.0;
const STRIP_HEIGHT_PX: f32 = 72.0;
const BG: u32 = 0x23272e;
const TRACK: u32 = 0x343a43;
const TICK: u32 = 0x5b6370;
const LABEL: u32 = 0x9aa3b0;
const KEYFRAME: u32 = 0xe8ecf1;
const KEYFRAME_PUCK: u32 = 0x111111;
const PLAYHEAD: u32 = 0xff4d4d;
const DECISION: u32 = 0xffb300;
const EVENT: u32 = 0x5fb3ff;
const FEEDBACK: u32 = 0x3ccf7a;

pub struct TimelineView {
    state: Entity<PlayState>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    scrubbing: bool,
}

#[derive(Clone, Copy)]
struct TimeAxis {
    left: f32,
    width: f32,
    duration_ms: u32,
}

impl TimeAxis {
    fn new(bounds: Bounds<Pixels>, duration_ms: u32) -> Self {
        TimeAxis {
            left: f32::from(bounds.origin.x) + PAD_PX,
            width: (f32::from(bounds.size.width) - 2.0 * PAD_PX).max(1.0),
            duration_ms: duration_ms.max(1),
        }
    }

    fn x(&self, t_ms: u32) -> Pixels {
        px(self.left + t_ms as f32 / self.duration_ms as f32 * self.width)
    }

    fn t(&self, x: Pixels) -> u32 {
        let u = ((f32::from(x) - self.left) / self.width).clamp(0.0, 1.0);
        (u * self.duration_ms as f32).round() as u32
    }
}

impl TimelineView {
    pub fn new(state: Entity<PlayState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        TimelineView { state, bounds: Rc::new(Cell::new(None)), scrubbing: false }
    }

    fn scrub_to(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(bounds) = self.bounds.get() else { return };
        self.state.update(cx, |s, cx| {
            let axis = TimeAxis::new(bounds, s.play.duration_ms);
            s.playing = false;
            s.set_playhead(snap(axis.t(x)));
            cx.notify();
        });
    }

    fn mouse_down(&mut self, ev: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.scrubbing = true;
        self.scrub_to(ev.position.x, cx);
    }

    fn mouse_move(&mut self, ev: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if !self.scrubbing || ev.pressed_button != Some(MouseButton::Left) {
            self.scrubbing = false;
            return;
        }
        self.scrub_to(ev.position.x, cx);
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.scrubbing = false;
    }
}

fn fill_rect(window: &mut Window, origin: Point<Pixels>, w: f32, h: f32, hex: u32) {
    window.paint_quad(quad(
        Bounds { origin, size: size(px(w), px(h)) },
        px(0.),
        rgb(hex),
        px(0.),
        rgb(hex),
        Default::default(),
    ));
}

fn diamond(window: &mut Window, c: Point<Pixels>, r: f32, hex: u32) {
    let mut b = PathBuilder::fill();
    b.add_polygon(
        &[point(c.x, c.y - px(r)), point(c.x + px(r), c.y), point(c.x, c.y + px(r)), point(c.x - px(r), c.y)],
        true,
    );
    if let Ok(path) = b.build() {
        window.paint_path(path, rgb(hex));
    }
}

fn paint_strip(window: &mut Window, cx: &mut gpui::App, bounds: Bounds<Pixels>, play: &Play, state: StripState) {
    let axis = TimeAxis::new(bounds, play.duration_ms);
    let top = f32::from(bounds.origin.y);
    let events_y = top + 14.0;
    let keys_y = top + 40.0;
    let ticks_y = top + 60.0;

    fill_rect(window, bounds.origin, f32::from(bounds.size.width), f32::from(bounds.size.height), BG);
    fill_rect(window, point(axis.x(0), px(keys_y - 1.0)), axis.width, 2.0, TRACK);

    // One tick per second, labelled.
    let mut s = 0;
    while s * 1000 <= play.duration_ms {
        let x = axis.x(s * 1000);
        fill_rect(window, point(x, px(keys_y - 6.0)), 1.0, 12.0, TICK);
        paint_text(window, cx, point(x, px(ticks_y)), &format!("{s}s"), px(10.0), LABEL);
        s += 1;
    }

    // Feedback: a bar from where the better route splits off to the moment
    // it's about, with a dot on that moment.
    let feedback_y = top + 27.0;
    for s in &state.suggestions {
        let x0 = axis.x(s.branch_from_ms);
        let x1 = axis.x(s.t_ms);
        fill_rect(window, point(x0, px(feedback_y - 1.5)), f32::from(x1 - x0), 3.0, FEEDBACK);
        diamond(window, point(x1, px(feedback_y)), 5.0, FEEDBACK);
    }

    for ev in &play.events {
        let c = point(axis.x(ev.t_ms), px(events_y));
        let color = if ev.kind == EventKind::DecisionPoint { DECISION } else { EVENT };
        diamond(window, c, 5.0, color);
    }

    if let Some(id) = &state.selected {
        for k in play.track(id) {
            let c = point(axis.x(k.t_ms), px(keys_y));
            diamond(window, c, 6.0, KEYFRAME);
            if k.has_puck {
                diamond(window, c, 3.0, KEYFRAME_PUCK);
            }
        }
    }

    let x = axis.x(state.playhead_ms);
    fill_rect(window, point(x - px(1.0), bounds.origin.y), 2.0, f32::from(bounds.size.height), PLAYHEAD);
}

struct StripState {
    playhead_ms: u32,
    selected: Option<rink_core::EntityId>,
    suggestions: Vec<rink_core::Suggestion>,
}

fn fmt_s(ms: u32) -> String {
    format!("{:.1}", ms as f32 / 1000.0)
}

impl Render for TimelineView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let s = self.state.read(cx);
        let play = s.play.clone();
        let strip = StripState {
            playhead_ms: s.playhead_ms,
            selected: s.selected.clone(),
            suggestions: s.suggestions().to_vec(),
        };
        let header = format!(
            "{}  {} / {} s   {}×   {}",
            if s.playing { "▶" } else { "❚❚" },
            fmt_s(s.playhead_ms),
            fmt_s(s.play.duration_ms),
            s.speed,
            s.selected.as_ref().map(|id| format!("keyframes: {id}")).unwrap_or_else(|| "no selection".into()),
        );
        let cell = self.bounds.clone();

        div()
            .flex()
            .flex_col()
            .w_full()
            .bg(rgb(BG))
            .text_color(rgb(LABEL))
            .text_sm()
            .child(div().px_4().py_1().child(header))
            .child(
                div()
                    .w_full()
                    .h(px(STRIP_HEIGHT_PX))
                    .child(
                        canvas(
                            move |bounds, _, _| {
                                cell.set(Some(bounds));
                                bounds
                            },
                            move |_, bounds, window, cx| paint_strip(window, cx, bounds, &play, strip),
                        )
                        .size_full(),
                    )
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
                    .on_mouse_move(cx.listener(Self::mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up)),
            )
    }
}
