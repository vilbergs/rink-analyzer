//! The rink view: draws the play at the playhead, turns drags into keyframes
//! and right-clicks into skating orders, and zooms with a pinch or the
//! scroll wheel.

use std::cell::Cell;
use std::rc::Rc;

use futures::StreamExt;
use gpui::{
    canvas, div, prelude::*, rgb, Context, Entity, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ScrollDelta, ScrollWheelEvent, Task, Window,
};
use rink_core::{EntityId, Vec2};

use crate::paint::{
    paint_entities, paint_ghosts, paint_orders, paint_rink, paint_suggestion_zones, puck_draw_pos, EntityPaint,
};
use crate::pinch::{self, Pinch};
use crate::state::PlayState;
use crate::theme::{self, short_name};
use crate::transform::{Camera3d, RinkTransform, ViewZoom};

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

struct OrbitDrag {
    start: Vec2,
    camera: Camera3d,
    moved: bool,
}

pub struct RinkCanvas {
    state: Entity<PlayState>,
    /// Written during prepaint, read by mouse handlers.
    transform: Rc<Cell<Option<RinkTransform>>>,
    drag: Option<Drag>,
    view: ViewZoom,
    view_3d: ViewZoom,
    camera: Camera3d,
    orbit: Option<OrbitDrag>,
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
        RinkCanvas {
            state,
            transform: Rc::new(Cell::new(None)),
            drag: None,
            view: ViewZoom::default(),
            view_3d: ViewZoom::default(),
            camera: Camera3d::default(),
            orbit: None,
            _pinch: pinch,
        }
    }

    pub fn reset_zoom(&mut self, cx: &mut Context<Self>) {
        if self.state.read(cx).view_3d {
            self.view_3d = ViewZoom::default();
            self.camera = Camera3d::default();
        } else {
            self.view = ViewZoom::default();
        }
        self.drag = None;
        self.orbit = None;
        cx.notify();
    }

    pub fn cancel_gesture(&mut self, cx: &mut Context<Self>) {
        self.drag = None;
        self.orbit = None;
        self.transform.set(None);
        cx.notify();
    }

    fn zoom_at(&mut self, at: gpui::Point<gpui::Pixels>, factor: f32, cx: &mut Context<Self>) {
        let Some(t) = self.transform.get() else { return };
        if self.state.read(cx).view_3d {
            self.view_3d = t.zoomed(self.view_3d, at, factor);
        } else {
            self.view = t.zoomed(self.view, at, factor);
        }
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
                let delta = Vec2::new(f32::from(d.x), f32::from(d.y));
                if self.state.read(cx).view_3d {
                    self.view_3d = t.panned(self.view_3d, delta);
                } else {
                    self.view = t.panned(self.view, delta);
                }
                cx.notify();
            }
        }
    }

    /// Right-click (or Ctrl-click): the selected player skates there.
    /// Shift adds the spot to the order they're on. Works while playing.
    fn order(&mut self, ev: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.transform.get() else { return };
        let Some(at) = t.to_rink(ev.position) else { return };
        self.drag = None;
        self.orbit = None;
        self.state.update(cx, |s, cx| {
            s.order_to(at, ev.modifiers.shift);
            cx.notify();
        });
    }

    fn mouse_down(&mut self, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if ev.modifiers.control {
            return self.order(ev, window, cx);
        }
        let Some(t) = self.transform.get() else { return };
        let state = self.state.read(cx);
        let hit = if state.view_3d {
            crate::paint3d::hit_test(&t, &state.play, &state.snapshot(), ev.position)
        } else {
            t.to_rink(ev.position).and_then(|at| state.hit_test(at))
        };
        if state.view_3d && hit.is_none() {
            self.start_orbit(ev, cx);
            return;
        }
        let Some(at) = t.to_rink(ev.position) else { return };
        self.orbit = None;
        self.state.update(cx, |s, cx| {
            s.stop_review_loop();
            s.playing = false;
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
        if let Some(orbit) = &mut self.orbit {
            if !matches!(ev.pressed_button, Some(MouseButton::Left | MouseButton::Middle)) {
                self.orbit = None;
                return;
            }
            let delta = Vec2::new(f32::from(ev.position.x), f32::from(ev.position.y)) - orbit.start;
            if delta.length() > 3.0 || orbit.moved {
                orbit.moved = true;
                self.camera = orbit.camera.orbited(delta);
                cx.notify();
            }
            return;
        }
        if ev.pressed_button != Some(MouseButton::Left) {
            self.drag = None;
            return;
        }
        let (Some(drag), Some(t)) = (&mut self.drag, self.transform.get()) else { return };
        let Some(at) = t.to_rink(ev.position) else { return };
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

    fn mouse_up(&mut self, ev: &MouseUpEvent, _: &mut Window, cx: &mut Context<Self>) {
        if self.orbit.take().is_some_and(|o| !o.moved) && ev.button == MouseButton::Left {
            self.state.update(cx, |s, cx| {
                s.stop_review_loop();
                s.playing = false;
                s.selected = None;
                cx.notify();
            });
        }
        self.drag = None;
    }

    fn start_orbit(&mut self, ev: &MouseDownEvent, cx: &mut Context<Self>) {
        if !self.state.read(cx).view_3d {
            return;
        }
        self.drag = None;
        self.orbit = Some(OrbitDrag {
            start: Vec2::new(f32::from(ev.position.x), f32::from(ev.position.y)),
            camera: self.camera,
            moved: false,
        });
    }

    /// Convert a screen direction to a unit direction on the ice, in either view.
    pub fn screen_direction(&self, direction: Vec2, cx: &gpui::App) -> Vec2 {
        let s = self.state.read(cx);
        let Some(t) = self.transform.get() else {
            return Vec2::new(direction.x, -direction.y) * if s.flipped { -1.0 } else { 1.0 };
        };
        let snapshot = s.snapshot();
        let pos = s
            .selected
            .as_ref()
            .and_then(|id| {
                if s.play.entity(id).is_some_and(|e| e.is_puck()) {
                    Some(snapshot.puck.pos)
                } else {
                    snapshot.entity(id).map(|e| e.pos)
                }
            })
            .unwrap_or(Vec2::ZERO);
        let at = t.to_screen(pos);
        let next = gpui::point(at.x + gpui::px(direction.x * 10.0), at.y + gpui::px(direction.y * 10.0));
        t.to_rink(next).map(|p| (p - pos).normalize_or_zero()).unwrap_or(Vec2::ZERO)
    }
}

impl Render for RinkCanvas {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.state.read(cx);
        let play = state.play.clone();
        let snapshot = state.snapshot();
        let selected = state.selected.clone();
        let playhead_ms = state.playhead_ms;
        let active: Vec<_> = if state.show_ghosts {
            state
                .active_suggestions()
                .filter(|(s, _)| state.focused_suggestion.as_ref().is_none_or(|id| id == &s.id))
                .map(|(s, g)| (s.clone(), g))
                .collect()
        } else {
            Vec::new()
        };
        let flipped = state.flipped;
        let is_3d = state.view_3d;
        let view = if is_3d { self.view_3d } else { self.view };
        let camera = self.camera;
        let cell = self.transform.clone();

        div()
            .id("rink-canvas")
            .debug_selector(|| "rink-canvas".into())
            .size_full()
            .overflow_hidden()
            .bg(rgb(theme::APP_BG))
            .child(
                canvas(
                    {
                        let rink = play.rink.clone();
                        move |bounds, _, _| {
                            let t = if is_3d {
                                RinkTransform::fit_3d(bounds, &rink, flipped, view, camera)
                            } else {
                                RinkTransform::fit(bounds, &rink, flipped, view)
                            };
                            cell.set(Some(t));
                            t
                        }
                    },
                    move |_, t, window, cx| {
                        paint_rink(window, &t, &play.rink);
                        paint_suggestion_zones(window, cx, &t, &play, &active);
                        paint_orders(window, &t, &play, playhead_ms, selected.as_ref());
                        if is_3d {
                            paint_ghosts(window, cx, &t, &play, &snapshot, &active);
                            crate::paint3d::paint_scene(window, cx, &t, &play, &snapshot, selected.as_ref(), &active);
                        } else {
                            paint_entities(
                                window,
                                cx,
                                &t,
                                EntityPaint { play: &play, snapshot: &snapshot, selected: selected.as_ref() },
                            );
                            paint_ghosts(window, cx, &t, &play, &snapshot, &active);
                        }
                    },
                )
                .size_full(),
            )
            .on_mouse_down(MouseButton::Left, cx.listener(Self::mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::order))
            .on_mouse_down(MouseButton::Middle, cx.listener(|this, ev, _, cx| this.start_orbit(ev, cx)))
            .on_mouse_move(cx.listener(Self::mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::mouse_up))
            .on_mouse_up(MouseButton::Middle, cx.listener(Self::mouse_up))
            .on_scroll_wheel(cx.listener(Self::scroll))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px, Modifiers, TestAppContext, VisualTestContext};
    use rink_core::{
        templates::{build, Template},
        RinkSpec,
    };

    fn setup(cx: &mut TestAppContext) -> (Entity<RinkCanvas>, Entity<PlayState>, &mut VisualTestContext) {
        let state = cx.new(|_| {
            let mut s = PlayState::new(build(Template::Faceoff, RinkSpec::iihf()), None);
            s.view_3d = true;
            s
        });
        let (canvas, cx) = cx.add_window_view(|window, cx| RinkCanvas::new(state.clone(), window, cx));
        cx.run_until_parked();
        (canvas, state, cx)
    }

    #[gpui::test]
    fn orbit_and_zoom_do_not_edit_or_pause_the_play(cx: &mut TestAppContext) {
        let (canvas, state, cx) = setup(cx);
        let original = cx.update(|_, cx| state.read(cx).play.clone());
        cx.update(|_, cx| {
            state.update(cx, |s, cx| {
                s.playing = true;
                cx.notify();
            })
        });
        let start = cx.update(|_, cx| canvas.read(cx).transform.get().unwrap().to_screen(Vec2::new(20.0, 10.0)));
        let end = point(start.x + px(60.0), start.y + px(30.0));
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        let camera = cx.update(|_, cx| canvas.read(cx).camera);
        assert_ne!(camera, Camera3d::default());
        cx.update(|_, cx| canvas.update(cx, |c, cx| c.zoom_at(end, 2.0, cx)));
        let zoom = cx.update(|_, cx| canvas.read(cx).view_3d);
        assert_eq!(zoom.zoom, 2.0);
        // Each view remembers its own zoom; resetting 3D leaves the 2D view alone.
        cx.update(|_, cx| {
            state.update(cx, |s, cx| {
                s.view_3d = false;
                cx.notify();
            })
        });
        cx.run_until_parked();
        cx.update(|_, cx| canvas.update(cx, |c, cx| c.zoom_at(end, 3.0, cx)));
        let top_down_zoom = cx.update(|_, cx| canvas.read(cx).view);
        cx.update(|_, cx| {
            state.update(cx, |s, cx| {
                s.view_3d = true;
                cx.notify();
            })
        });
        cx.update(|_, cx| canvas.update(cx, |c, cx| c.reset_zoom(cx)));
        cx.update(|_, cx| {
            let c = canvas.read(cx);
            assert_eq!(c.camera, Camera3d::default());
            assert_eq!(c.view_3d, ViewZoom::default());
            assert_eq!(c.view, top_down_zoom);
            let s = state.read(cx);
            assert_eq!(s.play, original);
            assert!(s.playing && !s.dirty && !s.can_undo());
        });
    }

    #[gpui::test]
    fn raised_player_drag_and_orders_land_on_the_ice(cx: &mut TestAppContext) {
        let (canvas, state, cx) = setup(cx);
        let (me, original, pos) = cx.update(|_, cx| {
            let s = state.read(cx);
            let me = s.play.me().unwrap().id.clone();
            (me.clone(), s.play.clone(), s.snapshot().entity(&me).unwrap().pos)
        });
        let t = cx.update(|_, cx| canvas.read(cx).transform.get().unwrap());
        let start = t.to_screen_at(pos, 1.8);
        let end = point(start.x + px(25.0), start.y - px(10.0));
        let expected = pos + t.to_rink(end).unwrap() - t.to_rink(start).unwrap();
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::none());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
        cx.update(|_, cx| {
            state.update(cx, |s, cx| {
                assert_eq!(s.selected, Some(me.clone()));
                assert!(s.snapshot().entity(&me).unwrap().pos.distance(expected) < 0.001);
                assert!(s.undo());
                assert_eq!(s.play, original);
                cx.notify();
            })
        });
        let destination = Vec2::new(10.0, 5.0);
        cx.simulate_mouse_down(t.to_screen(destination), MouseButton::Right, Modifiers::none());
        cx.update(|_, cx| {
            let s = state.read(cx);
            assert!(s.play.orders(&me)[0].to[0].distance(destination) < 0.001);
        });
    }

    #[gpui::test]
    fn nudges_follow_screen_axes_after_orbiting_and_flipping(cx: &mut TestAppContext) {
        let (canvas, state, cx) = setup(cx);
        for flipped in [false, true] {
            cx.update(|_, cx| {
                state.update(cx, |s, cx| {
                    s.flipped = flipped;
                    cx.notify();
                })
            });
            cx.update(|_, cx| {
                canvas.update(cx, |c, cx| {
                    c.camera = c.camera.orbited(Vec2::new(200.0, -40.0));
                    cx.notify();
                })
            });
            cx.run_until_parked();
            cx.update(|_, cx| {
                let c = canvas.read(cx);
                let s = state.read(cx);
                let pos = s.snapshot().entity(s.selected.as_ref().unwrap()).unwrap().pos;
                let t = c.transform.get().unwrap();
                for dir in [Vec2::new(1.0, 0.0), Vec2::new(0.0, -1.0)] {
                    let delta = c.screen_direction(dir, cx) * 0.5;
                    assert!((delta.length() - 0.5).abs() < 0.001);
                    let screen = (t.to_screen_vec(pos + delta) - t.to_screen_vec(pos)).normalize();
                    assert!(screen.distance(dir) < 0.001, "{screen} vs {dir}");
                }
            });
        }
    }
}
