//! Rink Replay in the browser: a loaded play, sampled and drawn by the same
//! core code as the desktop app. JS gets plain objects (serde's JSON shapes)
//! and a [`Scene`](rink_core::scene::Scene) display list to paint on a canvas.

use rink_core::review::active_suggestions;
use rink_core::scene::{self, Frame};
use rink_core::transform::{RinkTransform, ViewZoom};
use rink_core::{io, sample, EntityId, Play, Review, Vec2};
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct PlayHandle {
    play: Play,
}

/// Plain JS objects and arrays rather than `Map`s, and `null` for `None`.
fn to_js<T: Serialize>(value: &T) -> JsValue {
    value.serialize(&serde_wasm_bindgen::Serializer::json_compatible()).expect("model types serialize")
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

    /// The frame at `t_ms` for a `width` x `height` canvas, as a back-to-front
    /// list of shapes in canvas pixels. `review_json` is a `.review.json`
    /// file's text; its suggestions on the ice at `t_ms` are drawn as ghosts.
    #[allow(clippy::too_many_arguments)]
    pub fn scene(
        &self,
        t_ms: u32,
        width: f32,
        height: f32,
        zoom: f32,
        pan_x: f32,
        pan_y: f32,
        flipped: bool,
        selected: Option<String>,
        review_json: Option<String>,
    ) -> Result<JsValue, JsError> {
        let ghosts = match review_json {
            Some(text) => {
                let review: Review = serde_json::from_str(&text)?;
                active_suggestions(&review.suggestions, &self.play, t_ms).map(|(s, g)| (s.clone(), g)).collect()
            }
            None => Vec::new(),
        };
        let snapshot = sample(&self.play, t_ms);
        let selected = selected.map(EntityId);
        let frame = Frame {
            play: &self.play,
            snapshot: &snapshot,
            playhead_ms: t_ms,
            selected: selected.as_ref(),
            ghosts: &ghosts,
        };
        let t = self.transform(width, height, zoom, pan_x, pan_y, flipped);
        Ok(to_js(&scene::build(&frame, &t)))
    }

    /// The id of the entity under canvas pixel (`x_px`, `y_px`) at `t_ms`, for
    /// the same canvas and view as [`PlayHandle::scene`].
    #[allow(clippy::too_many_arguments)]
    pub fn hit_test(
        &self,
        t_ms: u32,
        x_px: f32,
        y_px: f32,
        width: f32,
        height: f32,
        zoom: f32,
        pan_x: f32,
        pan_y: f32,
        flipped: bool,
    ) -> Option<String> {
        let t = self.transform(width, height, zoom, pan_x, pan_y, flipped);
        let at = t.to_rink(Vec2::new(x_px, y_px));
        scene::hit_test(&self.play, &sample(&self.play, t_ms), at).map(|id| id.0)
    }
}

impl PlayHandle {
    fn transform(&self, width: f32, height: f32, zoom: f32, pan_x: f32, pan_y: f32, flipped: bool) -> RinkTransform {
        let view = ViewZoom { zoom, pan: Vec2::new(pan_x, pan_y) };
        RinkTransform::fit(Vec2::ZERO, Vec2::new(width, height), &self.play.rink, flipped, view)
    }
}
