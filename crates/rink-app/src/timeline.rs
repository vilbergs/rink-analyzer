//! The timeline: a transport bar (play, time, speed) over a strip with a
//! ruler, the play's events, position feedback, and the selected entity's
//! keyframes, plus a playhead you can scrub. Hovering a marker names it;
//! dragging a keyframe or event marker retimes it.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    canvas, div, point, prelude::*, px, quad, rgb, size, Bounds, Context, CursorStyle, Entity, FontWeight, Hsla,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, PathBuilder, Pixels, Point, SharedString, TextRun,
    Window,
};
use rink_core::{EntityId, EventKind, Play, Suggestion};

use crate::paint::paint_text;
use crate::state::{snap, PlayState};
use crate::theme::{self, button, seconds, short_name};
use crate::workspace::{CycleSpeed, TogglePlay};

const PAD_PX: f32 = 16.0;
const GUTTER_PX: f32 = 96.0;
const RULER_PX: f32 = 22.0;
const LANE_PX: f32 = 22.0;
const LANES: usize = 3;
const STRIP_HEIGHT_PX: f32 = RULER_PX + LANE_PX * LANES as f32 + 8.0;

const LANE_BG: u32 = 0x1a1e24;
const TICK: u32 = 0x3a404b;
const KEYFRAME: u32 = 0xe8ecf1;
const POSSESSION: u32 = 0x8f99a8;
/// How close (in px) the pointer must be to a marker to hover or grab it.
const MARKER_HIT_PX: f32 = 7.0;

pub struct TimelineView {
    state: Entity<PlayState>,
    bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    drag: Option<Drag>,
    /// Pointer position while it's over the strip, for hover labels.
    hover: Option<Point<Pixels>>,
}

enum Drag {
    Scrub,
    /// Moving the selection's keyframe that is now at `t_ms`.
    Keyframe {
        id: EntityId,
        t_ms: u32,
        recorded: bool,
    },
    /// Moving the event now at `index`.
    Event {
        index: usize,
        recorded: bool,
    },
}

#[derive(Clone, PartialEq)]
enum Target {
    Event(usize),
    Suggestion(String),
    Keyframe(u32),
}

/// Something drawn on a lane that can be hovered or grabbed.
struct Marker {
    lane: usize,
    t_ms: u32,
    label: String,
    target: Target,
}

fn markers(play: &Play, selected: Option<&EntityId>, suggestions: &[Suggestion]) -> Vec<Marker> {
    let mut out: Vec<Marker> = play
        .events
        .iter()
        .enumerate()
        .map(|(i, ev)| {
            let who: Vec<String> = ev.actors.iter().map(|a| short_name(play, a)).collect();
            let mut label = format!("{} · {}", seconds(ev.t_ms), ev.kind.label());
            if !who.is_empty() {
                label.push_str(&format!(" · {}", who.join(" → ")));
            }
            Marker { lane: 0, t_ms: ev.t_ms, label, target: Target::Event(i) }
        })
        .collect();
    out.extend(suggestions.iter().map(|s| Marker {
        lane: 1,
        t_ms: s.t_ms,
        label: format!("{} · {}", seconds(s.t_ms), s.principle),
        target: Target::Suggestion(s.id.clone()),
    }));
    if let Some(id) = selected {
        out.extend(play.track(id).iter().map(|k| Marker {
            lane: 2,
            t_ms: k.t_ms,
            label: format!("{} · keyframe of {}", seconds(k.t_ms), short_name(play, id)),
            target: Target::Keyframe(k.t_ms),
        }));
    }
    out
}

/// The marker under `at`, if any: same lane, nearest in time.
fn hit(markers: &[Marker], bounds: Bounds<Pixels>, axis: TimeAxis, at: Point<Pixels>) -> Option<&Marker> {
    let top = f32::from(bounds.origin.y);
    let y = f32::from(at.y);
    markers
        .iter()
        .filter(|m| (lane_center(top, m.lane) - y).abs() <= LANE_PX / 2.0)
        .map(|m| ((f32::from(axis.x(m.t_ms)) - f32::from(at.x)).abs(), m))
        .filter(|(d, _)| *d <= MARKER_HIT_PX)
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, m)| m)
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
        TimelineView { state, bounds: Rc::new(Cell::new(None)), drag: None, hover: None }
    }

    /// The marker under the pointer, from the last painted bounds.
    fn marker_at(&self, at: Point<Pixels>, cx: &Context<Self>) -> Option<(Target, String)> {
        let bounds = self.bounds.get()?;
        let s = self.state.read(cx);
        let axis = TimeAxis::new(bounds, s.play.duration_ms);
        let list = markers(&s.play, s.selected.as_ref(), s.suggestions());
        hit(&list, bounds, axis, at).map(|m| (m.target.clone(), m.label.clone()))
    }

    fn time_at(&self, x: Pixels, cx: &Context<Self>) -> Option<u32> {
        let bounds = self.bounds.get()?;
        Some(TimeAxis::new(bounds, self.state.read(cx).play.duration_ms).t(x))
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
        let target = self.marker_at(ev.position, cx).map(|(t, _)| t);
        self.drag = Some(match target {
            Some(Target::Keyframe(t_ms)) => {
                let Some(id) = self.state.read(cx).selected.clone() else { return };
                self.state.update(cx, |s, cx| {
                    s.playing = false;
                    s.set_playhead(t_ms);
                    cx.notify();
                });
                Drag::Keyframe { id, t_ms, recorded: false }
            }
            Some(Target::Event(index)) => {
                self.state.update(cx, |s, cx| {
                    s.jump_to_event(index);
                    cx.notify();
                });
                Drag::Event { index, recorded: false }
            }
            Some(Target::Suggestion(id)) => {
                self.state.update(cx, |s, cx| {
                    s.focus_suggestion(&id);
                    cx.notify();
                });
                return;
            }
            None => {
                self.scrub_to(ev.position.x, cx);
                Drag::Scrub
            }
        });
    }

    fn mouse_move(&mut self, ev: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        let inside = self.bounds.get().is_some_and(|b| b.contains(&ev.position));
        let hover = inside.then_some(ev.position);
        if hover != self.hover {
            self.hover = hover;
            cx.notify();
        }
        if ev.pressed_button != Some(MouseButton::Left) {
            self.drag = None;
            return;
        }
        let Some(to) = self.time_at(ev.position.x, cx) else { return };
        match &mut self.drag {
            None => {}
            Some(Drag::Scrub) => self.scrub_to(ev.position.x, cx),
            Some(Drag::Keyframe { id, t_ms, recorded }) => {
                let first = !std::mem::replace(recorded, true);
                *t_ms = self.state.update(cx, |s, cx| {
                    if first {
                        s.checkpoint(format!("retime of {}'s keyframe", short_name(&s.play, id)));
                    }
                    cx.notify();
                    s.retime_keyframe(id, *t_ms, to)
                });
            }
            Some(Drag::Event { index, recorded }) => {
                let first = !std::mem::replace(recorded, true);
                *index = self.state.update(cx, |s, cx| {
                    if first {
                        let label = s.play.events.get(*index).map_or("event", |e| e.kind.label()).to_lowercase();
                        s.checkpoint(format!("retime of a {label}"));
                    }
                    cx.notify();
                    s.retime_event(*index, to)
                });
            }
        }
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.drag = None;
    }
}

fn fill_rect(window: &mut Window, origin: Point<Pixels>, w: f32, h: f32, hex: u32) {
    rounded_rect(window, origin, w, h, 0.0, hex);
}

fn rounded_rect(window: &mut Window, origin: Point<Pixels>, w: f32, h: f32, radius: f32, hex: u32) {
    window.paint_quad(quad(
        Bounds { origin, size: size(px(w), px(h)) },
        px(radius),
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

/// A hover label: `text` in a dark pill centered over `x`, kept inside
/// `bounds`, in the ruler row.
fn paint_label(window: &mut Window, cx: &mut gpui::App, bounds: Bounds<Pixels>, x: Pixels, text: &str) {
    let font_size = px(11.0);
    let run = TextRun {
        len: text.len(),
        font: window.text_style().font(),
        color: Hsla::from(rgb(theme::TEXT)),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window.text_system().shape_line(SharedString::from(text.to_string()), font_size, &[run], None);
    let (pad, h) = (8.0, 18.0);
    let w = f32::from(line.width) + 2.0 * pad;
    let left = f32::from(bounds.origin.x) + 4.0;
    let right = f32::from(bounds.origin.x + bounds.size.width) - 4.0 - w;
    let x0 = (f32::from(x) - w / 2.0).clamp(left, right.max(left));
    let y0 = f32::from(bounds.origin.y) + 2.0;
    window.paint_quad(quad(
        Bounds { origin: point(px(x0), px(y0)), size: size(px(w), px(h)) },
        px(4.0),
        rgb(theme::RAISED),
        px(1.0),
        rgb(theme::BORDER_STRONG),
        Default::default(),
    ));
    line.paint(point(px(x0 + pad), px(y0 + (h - 13.2) / 2.0)), px(13.2), window, cx).ok();
}

fn lane_center(top: f32, lane: usize) -> f32 {
    top + RULER_PX + LANE_PX * lane as f32 + LANE_PX / 2.0
}

fn paint_strip(window: &mut Window, cx: &mut gpui::App, bounds: Bounds<Pixels>, play: &Play, state: StripState) {
    let axis = TimeAxis::new(bounds, play.duration_ms);
    let top = f32::from(bounds.origin.y);
    let width = f32::from(bounds.size.width);

    for lane in 0..LANES {
        let y = lane_center(top, lane) - LANE_PX / 2.0 + 2.0;
        rounded_rect(window, point(bounds.origin.x + px(4.0), px(y)), width - 8.0, LANE_PX - 4.0, 4.0, LANE_BG);
    }

    // Ruler: a tick every half second, labelled every second.
    let mut t = 0;
    while t <= play.duration_ms {
        let x = axis.x(t);
        let major = t % 1000 == 0;
        let h = if major { 6.0 } else { 3.0 };
        fill_rect(window, point(x, px(top + RULER_PX - h - 2.0)), 1.0, h, TICK);
        if major {
            paint_text(window, cx, point(x, px(top + 8.0)), &format!("{}s", t / 1000), px(10.0), theme::TEXT_FAINT);
        }
        t += 500;
    }

    // Lane 0: events.
    for ev in &play.events {
        let color = if ev.kind == EventKind::DecisionPoint { theme::WARN } else { theme::ACCENT };
        diamond(window, point(axis.x(ev.t_ms), px(lane_center(top, 0))), 5.0, color);
    }

    // Lane 1: feedback, a bar from where the better route splits off to the
    // moment it's about.
    let y = lane_center(top, 1);
    for s in &state.suggestions {
        let x0 = axis.x(s.branch_from_ms);
        let x1 = axis.x(s.t_ms);
        rounded_rect(window, point(x0, px(y - 2.0)), f32::from(x1 - x0), 4.0, 2.0, theme::FEEDBACK);
        diamond(window, point(x1, px(y)), 5.0, theme::FEEDBACK);
    }

    // Lane 2: the selection's keyframes, over a bar while they have the puck.
    if let Some(id) = &state.selected {
        let y = lane_center(top, 2);
        for p in rink_core::sample::possessions(play).iter().filter(|p| &p.entity == id) {
            let x0 = axis.x(p.start_ms);
            let x1 = axis.x(p.end_ms.unwrap_or(play.duration_ms));
            rounded_rect(window, point(x0, px(y - 2.0)), f32::from(x1 - x0), 4.0, 2.0, POSSESSION);
        }
        for k in play.track(id) {
            diamond(window, point(axis.x(k.t_ms), px(y)), 5.0, KEYFRAME);
        }
    }

    // Playhead: a line with a handle in the ruler.
    let x = axis.x(state.playhead_ms);
    fill_rect(window, point(x - px(1.0), px(top + 4.0)), 2.0, STRIP_HEIGHT_PX - 4.0, theme::ACCENT);
    rounded_rect(window, point(x - px(6.0), px(top + 2.0)), 12.0, 12.0, 3.0, theme::ACCENT);

    if let Some((t_ms, label)) = &state.label {
        paint_label(window, cx, bounds, axis.x(*t_ms), label);
    }
}

struct StripState {
    playhead_ms: u32,
    selected: Option<rink_core::EntityId>,
    suggestions: Vec<rink_core::Suggestion>,
    /// A hover label: the marker's time and text.
    label: Option<(u32, String)>,
}

fn legend(label: &'static str, hex: u32) -> gpui::Div {
    div().flex().items_center().gap_1p5().child(div().size(px(7.)).rounded_sm().bg(rgb(hex))).child(label)
}

fn lane_label(text: impl Into<SharedString>) -> gpui::Div {
    div().h(px(LANE_PX)).flex().items_center().px_4().truncate().child(text.into())
}

impl Render for TimelineView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let hovered = self.hover.and_then(|at| self.marker_at(at, cx));
        let s = self.state.read(cx);
        let play = s.play.clone();
        let label = match &self.drag {
            Some(Drag::Keyframe { t_ms, .. }) => Some((*t_ms, seconds(*t_ms))),
            Some(Drag::Event { index, .. }) => play.events.get(*index).map(|e| (e.t_ms, seconds(e.t_ms))),
            Some(Drag::Scrub) => None,
            None => hovered.as_ref().map(|(target, label)| {
                let t = match target {
                    Target::Event(i) => play.events.get(*i).map_or(0, |e| e.t_ms),
                    Target::Suggestion(id) => s.suggestions().iter().find(|x| &x.id == id).map_or(0, |x| x.t_ms),
                    Target::Keyframe(t) => *t,
                };
                (t, label.clone())
            }),
        };
        let grabbable = matches!(self.drag, Some(Drag::Keyframe { .. } | Drag::Event { .. }))
            || matches!(hovered, Some((Target::Keyframe(_) | Target::Event(_), _)));
        let strip = StripState {
            playhead_ms: s.playhead_ms,
            selected: s.selected.clone(),
            suggestions: s.suggestions().to_vec(),
            label,
        };
        let selection = s.selected.as_ref().map(|id| short_name(&play, id)).unwrap_or_else(|| "Keyframes".into());
        let cell = self.bounds.clone();

        let transport = div()
            .flex()
            .items_center()
            .justify_between()
            .h(px(44.))
            .px_4()
            .border_b_1()
            .border_color(rgb(theme::BORDER))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .id("play")
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(px(30.))
                            .rounded_full()
                            .bg(rgb(theme::ACCENT))
                            .text_color(rgb(0xffffff))
                            .text_sm()
                            .cursor_pointer()
                            .hover(|st| st.bg(rgb(0x6a9fff)))
                            .child(if s.playing { "❚❚" } else { "▶" })
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(TogglePlay), cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap_1()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme::TEXT))
                                    .child(SharedString::from(seconds(s.playhead_ms))),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(theme::TEXT_FAINT))
                                    .child(SharedString::from(format!("/ {}", seconds(play.duration_ms)))),
                            ),
                    )
                    .child(
                        button("speed", format!("{}×", s.speed), false)
                            .on_click(|_, window, cx| window.dispatch_action(Box::new(CycleSpeed), cx)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_4()
                    .text_xs()
                    .text_color(rgb(theme::TEXT_MUTED))
                    .child(legend("Event", theme::ACCENT))
                    .child(legend("Decision point", theme::WARN))
                    .child(legend("Feedback", theme::FEEDBACK))
                    .child(legend("Has the puck", POSSESSION)),
            );

        let labels = div()
            .flex()
            .flex_col()
            .flex_none()
            .w(px(GUTTER_PX))
            .pt(px(RULER_PX))
            .text_xs()
            .text_color(rgb(theme::TEXT_MUTED))
            .border_r_1()
            .border_color(rgb(theme::BORDER))
            .child(lane_label("Events"))
            .child(lane_label("Feedback"))
            .child(lane_label(selection));

        div()
            .flex()
            .flex_col()
            .w_full()
            .bg(rgb(theme::PANEL))
            .border_t_1()
            .border_color(rgb(theme::BORDER))
            .child(transport)
            .child(
                div().flex().h(px(STRIP_HEIGHT_PX)).child(labels).child(
                    div()
                        .id("strip")
                        .flex_1()
                        .h_full()
                        .cursor(if grabbable { CursorStyle::ResizeLeftRight } else { CursorStyle::PointingHand })
                        .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                            if !hovered && this.hover.take().is_some() {
                                cx.notify();
                            }
                        }))
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
                ),
            )
    }
}
