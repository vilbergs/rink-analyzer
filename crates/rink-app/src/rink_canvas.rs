//! The rink view: draws the play at the playhead and turns drags into
//! keyframes.

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    canvas, div, prelude::*, rgb, Context, Entity, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Window,
};
use rink_core::{EntityId, Vec2};

use crate::paint::{paint_entities, paint_ghosts, paint_rink, paint_suggestion_zones, EntityPaint};
use crate::state::PlayState;
use crate::transform::RinkTransform;

const BACKGROUND: u32 = 0x1b1e23;

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
}

pub struct RinkCanvas {
    state: Entity<PlayState>,
    /// Written during prepaint, read by mouse handlers.
    transform: Rc<Cell<Option<RinkTransform>>>,
    drag: Option<Drag>,
}

impl RinkCanvas {
    pub fn new(state: Entity<PlayState>, cx: &mut Context<Self>) -> Self {
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        RinkCanvas { state, transform: Rc::new(Cell::new(None)), drag: None }
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
                    s.toggle_puck(&id);
                } else {
                    let snap = s.snapshot();
                    let center = if s.play.entity(&id).is_some_and(|e| e.is_puck()) {
                        snap.puck.pos
                    } else {
                        snap.entity(&id).map(|e| e.pos).unwrap_or(at)
                    };
                    let mode = if ev.modifiers.shift { DragMode::Rotate } else { DragMode::Move };
                    self.drag = Some(Drag { id, mode, grab_offset: at - center });
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
        let (Some(drag), Some(t)) = (&self.drag, self.transform.get()) else { return };
        let at = t.to_rink(ev.position);
        self.state.update(cx, |s, cx| {
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
        let cell = self.transform.clone();

        div()
            .size_full()
            .bg(rgb(BACKGROUND))
            .child(
                canvas(
                    {
                        let rink = play.rink.clone();
                        move |bounds, _, _| {
                            let t = RinkTransform::fit(bounds, &rink, flipped);
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
    }
}
