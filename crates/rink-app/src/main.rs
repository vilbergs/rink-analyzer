//! Rink Replay: recreate hockey plays as keyframed animations on a to-scale
//! rink.
//!
//! Usage: `rink-replay [PLAY.rink.json]` or `rink-replay --template NAME`
//! (faceoff, dzone, breakout). With no arguments it opens the breakout example.

mod inspector;
mod paint;
mod rink_canvas;
mod state;
mod timeline;
mod transform;
mod workspace;

use std::path::PathBuf;

use anyhow::{bail, Context as _, Result};
use gpui::{prelude::*, px, size, App, Application, Bounds, Focusable, TitlebarOptions, WindowBounds, WindowOptions};
use rink_core::templates::{build, Template};
use rink_core::{io, RinkSpec};

use crate::state::PlayState;
use crate::workspace::Workspace;

fn initial_state() -> Result<PlayState> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [] => Ok(PlayState::new(build(Template::Breakout, RinkSpec::iihf()), None)),
        [flag, name] if flag == "--template" => {
            let template = match name.as_str() {
                "faceoff" => Template::Faceoff,
                "dzone" => Template::DZoneSet,
                "breakout" => Template::Breakout,
                other => bail!("unknown template {other:?}; use faceoff, dzone or breakout"),
            };
            Ok(PlayState::new(build(template, RinkSpec::iihf()), None))
        }
        [path] => {
            let path = PathBuf::from(path);
            let play = io::load(&path).with_context(|| format!("opening {}", path.display()))?;
            Ok(PlayState::new(play, Some(path)))
        }
        _ => bail!("usage: rink-replay [PLAY.rink.json | --template faceoff|dzone|breakout]"),
    }
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let state = initial_state()?;
    Application::new().run(move |cx: &mut App| {
        workspace::bind_keys(cx);
        let bounds = Bounds::centered(None, size(px(1440.), px(860.)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions { title: Some("Rink Replay".into()), ..Default::default() }),
                    focus: true,
                    ..Default::default()
                },
                |_, cx| cx.new(|cx| Workspace::new(state, cx)),
            )
            .expect("failed to open the main window");
        window
            .update(cx, |ws, window, cx| {
                window.focus(&ws.focus_handle(cx));
            })
            .ok();
        cx.on_window_closed(|cx| cx.quit()).detach();
        cx.activate(true);
    });
    Ok(())
}
