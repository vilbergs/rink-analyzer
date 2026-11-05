//! Rink Replay in the browser: a loaded play or lesson, sampled, drawn and
//! graded by the same Rust code as the desktop app. JS gets plain objects
//! (serde's JSON shapes) and a [`Scene`](rink_core::scene::Scene) display
//! list to paint on a canvas.

use std::collections::BTreeMap;

use rink_core::review::active_suggestions;
use rink_core::scene::{self, Frame, Mark};
use rink_core::transform::{RinkTransform, ViewZoom, Viewport};
use rink_core::{io, sample, EntityId, Play, Review, Vec2};
use rink_lesson::{check, grade, Answer, Lesson, Plays, Which};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Plain JS objects and arrays rather than `Map`s, and `null` for `None`.
fn to_js<T: Serialize>(value: &T) -> JsValue {
    value.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).expect("model types serialize")
}

/// How a canvas shows a play: its size in CSS pixels, the view, and what to
/// draw on top of the play.
#[derive(Deserialize)]
struct ViewOpts {
    width: f32,
    height: f32,
    #[serde(default = "one")]
    zoom: f32,
    #[serde(default)]
    pan: [f32; 2],
    #[serde(default)]
    flipped: bool,
    /// The rink's length runs down the canvas, home's own net at the bottom
    /// unless `flipped`.
    #[serde(default)]
    portrait: bool,
    /// The rink x-range, `[x0, x1]` in meters, that fills the canvas at zoom 1,
    /// as [`PlayHandle::focus`] gives it. Kept 8 px from the canvas edges
    /// rather than 24.
    #[serde(default)]
    focus: Option<[f32; 2]>,
    #[serde(default)]
    selected: Option<String>,
    /// Entity ids left out of the drawing.
    #[serde(default)]
    hidden: Vec<String>,
    #[serde(default)]
    marks: Vec<Mark>,
    /// A `.review.json` file's text; its suggestions on the ice are drawn as ghosts.
    #[serde(default)]
    review: Option<String>,
}

/// Canvas pixels kept free around a focused view, small enough for a phone.
const FOCUS_MARGIN_PX: f32 = 8.0;
/// Meters of ice shown beyond the user's player and the puck.
const FOCUS_PAD_M: f32 = 6.0;

fn one() -> f32 {
    1.0
}

impl ViewOpts {
    fn parse(opts: JsValue) -> Result<ViewOpts, JsError> {
        Ok(serde_wasm_bindgen::from_value(opts)?)
    }

    fn transform(&self, play: &Play) -> RinkTransform {
        let view = ViewZoom { zoom: self.zoom, pan: Vec2::from(self.pan) };
        let mut viewport = Viewport {
            flipped: self.flipped,
            portrait: self.portrait,
            crop: self.focus,
            ..Viewport::new(Vec2::ZERO, Vec2::new(self.width, self.height))
        };
        if self.focus.is_some() {
            viewport.margin_px = FOCUS_MARGIN_PX;
        }
        RinkTransform::fit(&play.rink, &viewport, view)
    }
}

/// The x-range where `play` happens for the user: their player and the puck
/// over the whole play, padded.
fn focus(play: &Play) -> [f32; 2] {
    let ids: Vec<EntityId> = [play.me(), play.puck()].into_iter().flatten().map(|e| e.id.clone()).collect();
    scene::x_extent(play, &ids, FOCUS_PAD_M)
}

fn draw(play: &Play, t_ms: u32, opts: JsValue) -> Result<JsValue, JsError> {
    let opts = ViewOpts::parse(opts)?;
    let ghosts = match &opts.review {
        Some(text) => {
            let review: Review = serde_json::from_str(text)?;
            active_suggestions(&review.suggestions, play, t_ms).map(|(s, g)| (s.clone(), g)).collect()
        }
        None => Vec::new(),
    };
    let snapshot = sample(play, t_ms);
    let selected = opts.selected.clone().map(EntityId);
    let hidden: Vec<EntityId> = opts.hidden.iter().cloned().map(EntityId).collect();
    let frame = Frame {
        play,
        snapshot: &snapshot,
        playhead_ms: t_ms,
        selected: selected.as_ref(),
        ghosts: &ghosts,
        hidden: &hidden,
        marks: &opts.marks,
    };
    Ok(to_js(&scene::build(&frame, &opts.transform(play))))
}

fn hit(play: &Play, t_ms: u32, x_px: f32, y_px: f32, opts: JsValue) -> Result<Option<String>, JsError> {
    let at = ViewOpts::parse(opts)?.transform(play).to_rink(Vec2::new(x_px, y_px));
    Ok(scene::hit_test(play, &sample(play, t_ms), at).map(|id| id.0))
}

fn rink_point(play: &Play, x_px: f32, y_px: f32, opts: JsValue) -> Result<JsValue, JsError> {
    let at = ViewOpts::parse(opts)?.transform(play).to_rink(Vec2::new(x_px, y_px));
    Ok(to_js(&at))
}

#[wasm_bindgen]
pub struct PlayHandle {
    play: Play,
}

#[wasm_bindgen]
impl PlayHandle {
    /// Parse a `.rink.json` file's text, migrating older schema versions.
    pub fn from_json(text: &str) -> Result<PlayHandle, JsError> {
        Ok(PlayHandle { play: io::from_json(text)? })
    }

    pub fn to_json(&self) -> String {
        io::to_json(&self.play)
    }

    pub fn duration_ms(&self) -> u32 {
        self.play.duration_ms
    }

    pub fn title(&self) -> String {
        self.play.title.clone()
    }

    /// Every player and the puck at `t_ms`.
    pub fn snapshot(&self, t_ms: u32) -> JsValue {
        to_js(&sample(&self.play, t_ms))
    }

    pub fn captions(&self) -> JsValue {
        to_js(&self.play.captions)
    }

    pub fn events(&self) -> JsValue {
        to_js(&self.play.events)
    }

    pub fn entities(&self) -> JsValue {
        to_js(&self.play.entities)
    }

    /// The frame at `t_ms` as a back-to-front list of shapes in canvas
    /// pixels. `opts` is the view: `{ width, height, zoom?, pan?: [x, y],
    /// flipped?, portrait?, focus?: [x0, x1], selected?, hidden?: [id],
    /// marks?: [mark], review? }`.
    pub fn scene(&self, t_ms: u32, opts: JsValue) -> Result<JsValue, JsError> {
        draw(&self.play, t_ms, opts)
    }

    /// The id of the entity under canvas pixel (`x_px`, `y_px`) at `t_ms`, for
    /// the same `opts` as [`PlayHandle::scene`].
    pub fn hit_test(&self, t_ms: u32, x_px: f32, y_px: f32, opts: JsValue) -> Result<Option<String>, JsError> {
        hit(&self.play, t_ms, x_px, y_px, opts)
    }

    /// Canvas pixel (`x_px`, `y_px`) in rink meters, as `[x, y]`.
    pub fn to_rink(&self, x_px: f32, y_px: f32, opts: JsValue) -> Result<JsValue, JsError> {
        rink_point(&self.play, x_px, y_px, opts)
    }

    /// `[x0, x1]`, the rink x-range the user's player and the puck cover over
    /// the play with 6 m to spare, for `opts.focus`.
    pub fn focus(&self) -> JsValue {
        to_js(&focus(&self.play))
    }
}

/// A lesson with its play and contrast variant. `which` arguments are
/// `"main"` or `"wrong"`.
#[wasm_bindgen]
pub struct LessonHandle {
    lesson: Lesson,
    plays: Plays,
}

#[derive(Serialize)]
struct MetaView<'a> {
    id: &'a str,
    module: &'a str,
    title: &'a str,
    sections: &'a BTreeMap<String, String>,
}

#[wasm_bindgen]
impl LessonHandle {
    /// A lesson's Markdown and steps file, its play and, when it has one, the
    /// wrong variant (all as file text).
    pub fn load(
        md: &str,
        steps_json: &str,
        play_json: &str,
        wrong_json: Option<String>,
    ) -> Result<LessonHandle, JsError> {
        let lesson = Lesson::parse(md, steps_json)?;
        let wrong = wrong_json.as_deref().map(io::from_json).transpose()?;
        Ok(LessonHandle { lesson, plays: Plays { main: io::from_json(play_json)?, wrong } })
    }

    /// `{ id, module, title, sections: { heading: text } }`.
    pub fn meta(&self) -> JsValue {
        let m = &self.lesson.meta;
        to_js(&MetaView { id: &m.id, module: &m.module, title: &m.title, sections: &self.lesson.sections })
    }

    /// The steps with every default filled in.
    pub fn steps(&self) -> JsValue {
        to_js(&self.lesson.resolved_steps(&self.plays))
    }

    pub fn duration_ms(&self, which: &str) -> Result<u32, JsError> {
        Ok(self.play(which)?.duration_ms)
    }

    pub fn captions(&self, which: &str) -> Result<JsValue, JsError> {
        Ok(to_js(&self.play(which)?.captions))
    }

    pub fn events(&self, which: &str) -> Result<JsValue, JsError> {
        Ok(to_js(&self.play(which)?.events))
    }

    /// See [`PlayHandle::scene`].
    pub fn scene(&self, which: &str, t_ms: u32, opts: JsValue) -> Result<JsValue, JsError> {
        draw(self.play(which)?, t_ms, opts)
    }

    pub fn hit_test(
        &self,
        which: &str,
        t_ms: u32,
        x_px: f32,
        y_px: f32,
        opts: JsValue,
    ) -> Result<Option<String>, JsError> {
        hit(self.play(which)?, t_ms, x_px, y_px, opts)
    }

    pub fn to_rink(&self, which: &str, x_px: f32, y_px: f32, opts: JsValue) -> Result<JsValue, JsError> {
        rink_point(self.play(which)?, x_px, y_px, opts)
    }

    /// See [`PlayHandle::focus`].
    pub fn focus(&self, which: &str) -> Result<JsValue, JsError> {
        Ok(to_js(&focus(self.play(which)?)))
    }

    /// Grade `{ kind: "choice", index }` or `{ kind: "spot", at: [x, y] }`
    /// for step `step_id`: `{ grade, feedback, watch, marks, facts }`.
    pub fn grade(&self, step_id: &str, answer: JsValue) -> Result<JsValue, JsError> {
        let answer: Answer = serde_wasm_bindgen::from_value(answer)?;
        Ok(to_js(&grade(&self.lesson, &self.plays, step_id, &answer)?))
    }

    /// The content issues, `[{ step, message }]`; empty for a sound lesson.
    pub fn check(&self) -> JsValue {
        to_js(&check(&self.lesson, &self.plays))
    }
}

impl LessonHandle {
    fn play(&self, which: &str) -> Result<&Play, JsError> {
        let which = match which {
            "main" => Which::Main,
            "wrong" => Which::Wrong,
            other => return Err(JsError::new(&format!("`{other}` is not \"main\" or \"wrong\""))),
        };
        self.plays.get(which).ok_or_else(|| JsError::new("this lesson has no wrong play"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dzone_lesson_focuses_on_the_defensive_zone() {
        let text = include_str!("../../../curriculum/plays/l1-dzone-strong-side-wall.rink.json");
        let [x0, x1] = focus(&io::from_json(text).unwrap());
        assert!(-30.0 <= x0 && x0 < x1 && x1 <= -5.0, "{x0}..{x1}");
    }
}
