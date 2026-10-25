//! The rink view: draws the play at the playhead, turns drags into keyframes,
//! and zooms with a pinch or the scroll wheel.

use std::cell::Cell;
use std::rc::Rc;

use futures::StreamExt;
use gpui::{
    canvas, div, prelude::*, rgb, Context, Entity, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ScrollDelta, ScrollWheelEvent, Task, Window,
};
use rink_core::{EntityId, Vec2};

use crate::paint::{paint_entities, paint_ghosts, paint_rink, paint_suggestion_zones, puck_draw_pos, EntityPaint};
use crate::pinch::{self, Pinch};
use crate::state::PlayState;
use crate::theme::{self, short_name};
use crate::transform::{RinkTransform, ViewZoom};

/// Zoom per line of a mouse wheel, and per pixel of Cmd/Ctrl-scroll.
const WHEEL_ZOOM_PER_LINE: f32 = 1.15;
const SCROLL_ZOOM_PER_PX: f32 = 1.01;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    Move,
    Rotate,
}

struct Drag {
    id: EntityId,
    mode: DragMode,
    /// Pointer offset from the entity's center at grab time, so it doesn't jump.
    grab_offset: Vec2,
    /// Undo is recorded on the first move, so a whole drag is one step.
    recorded: bool,
}

pub struct RinkCanvas {
    state: Entity<PlayState>,
    /// Written during prepaint, read by mouse handlers.
    transform: Rc<Cell<Option<RinkTransform>>>,
    drag: Option<Drag>,
    view: ViewZoom,
    _pinch: Option<Task<()>>,
}

impl RinkCanvas {
    pub fn new(state: Entity<PlayState>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        let pinch = pinch::listen(window).map(|mut steps| {
            cx.spawn(async move |this, cx| {
                while let Some(step) = steps.next().await {
                    if this.update(cx, |canvas, cx| canvas.pinch(step, cx)).is_err() {
                        break;
                    }
                }
            })
        });
        RinkCanvas { state, transform: Rc::new(Cell::new(None)), drag: None, view: ViewZoom::default(), _pinch: pinch }
    }

    pub fn reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.view = ViewZoom::default();
        cx.notify();
    }

    fn zoom_at(&mut self, at: gpui::Point<gpui::Pixels>, factor: f32, cx: &mut Context<Self>) {
        let Some(t) = self.transform.get() else { return };
        self.view = t.zoomed(self.view, at, factor);
        cx.notify();
    }

    fn pinch(&mut self, step: Pinch, cx: &mut Context<Self>) {
        self.zoom_at(step.position, step.factor, cx);
    }

    /// A mouse wheel or Cmd/Ctrl-scroll zooms; a two-finger scroll pans.
    fn scroll(&mut self, ev: &ScrollWheelEvent, _: &mut Window, cx: &mut Context<Self>) {
        match ev.delta {
            ScrollDelta::Lines(d) => self.zoom_at(ev.position, WHEEL_ZOOM_PER_LINE.powf(d.y), cx),
            ScrollDelta::Pixels(d) if ev.modifiers.platform || ev.modifiers.control => {
                self.zoom_at(ev.position, SCROLL_ZOOM_PER_PX.powf(f32::from(d.y)), cx)
            }
            ScrollDelta::Pixels(d) => {
                let Some(t) = self.transform.get() else { return };
                self.view = t.panned(self.view, Vec2::new(f32::from(d.x), f32::from(d.y)));
                cx.notify();
            }
        }
    }

    fn mouse_down(&mut self, ev: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.transform.get() else { return };
        let at = t.to_rink(ev.position);
        self.state.update(cx, |s, cx| {
            s.playing = false;
            let hit = s.hit_test(at);
            s.selected = hit.clone();
            if let Some(id) = hit {
                if ev.modifiers.alt {
                    s.give_puck(&id);
                } else {
                    let snap = s.snapshot();
                    let center = if s.play.entity(&id).is_some_and(|e| e.is_puck()) {
                        puck_draw_pos(&s.play, &snap)
                    } else {
                        snap.entity(&id).map(|e| e.pos).unwrap_or(at)
                    };
                    let mode = if ev.modifiers.shift { DragMode::Rotate } else { DragMode::Move };
                    self.drag = Some(Drag { id, mode, grab_offset: at - center, recorded: false });
                }
            }
            cx.notify();
        });
    }

    fn mouse_move(&mut self, ev: &MouseMoveEvent, _: &mut Window, cx: &mut Context<Self>) {
        if ev.pressed_button != Some(MouseButton::Left) {
            self.drag = None;
            return;
        }
        let (Some(drag), Some(t)) = (&mut self.drag, self.transform.get()) else { return };
        let at = t.to_rink(ev.position);
        let first = !std::mem::replace(&mut drag.recorded, true);
        self.state.update(cx, |s, cx| {
            if first {
                let who = short_name(&s.play, &drag.id);
                s.checkpoint(match drag.mode {
                    DragMode::Move => format!("move of {who}"),
                    DragMode::Rotate => format!("turn of {who}"),
                });
            }
            match drag.mode {
                DragMode::Move => s.move_entity(&drag.id, at - drag.grab_offset),
                DragMode::Rotate => s.rotate_entity(&drag.id, at),
            }
            cx.notify();
        });
    }

    fn mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.drag = None;
    }
}

impl Render for RinkCanvas {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let play = state.play.clone();
        let snapshot = state.snapshot();
        let selected = state.selected.clone();
        let active: Vec<_> = if state.show_ghosts {
            state.active_suggestions().map(|(s, g)| (s.clone(), g)).collect()
        } else {
            Vec::new()
        };
        let flipped = state.flipped;
        let view = self.view;
        let cell = self.transform.clone();

        div()
            .size_full()
            .bg(rgb(theme::APP_BG))
            .child(
                canvas(
                    {
                        let rink = play.rink.clone();
                        move |bounds, _, _| {
                            let t = RinkTransform::fit(bounds, &rink, flipped, view);
                            cell.set(Some(t));
                            t
                        }
                    },
                    move |_, t, window, cx| {
                        paint_rink(window, &t, &play.rink);
                        paint_suggestion_zones(window, cx, &t, &play, &active);
                        paint_entities(
                            window,
                            cx,
                            &t,
                            EntityPaint { play: &play, snapshot: &snapshot, selected: selected.as_ref() },
                        );
                        paint_ghosts(window, cx, &t, &play, &snapshot, &active);
                    },
                )
                .size_full(),
            )
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_scroll_wheel(cx.listener(Self::scroll))
    }
}
