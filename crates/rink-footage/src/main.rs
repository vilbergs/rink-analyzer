use std::{
    env,
    path::{Path, PathBuf},
};

use anyhow::{bail, ensure, Context, Result};
use rink_footage::{extract, Selection};

const USAGE: &str = "Usage: rink-footage <HEVC.mov|HEVC.mp4> <output-dir> <max-pixels> <timestamp-ms>... | --fps <fps>";

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = env::args_os().skip(1);
    let Some(input) = args.next() else {
        bail!("{USAGE}");
    };
    if input == "--help" || input == "-h" {
        println!("{USAGE}");
        return Ok(());
    }
    let output = PathBuf::from(args.next().context(USAGE)?);
    let max_pixels = number(args.next().context(USAGE)?)?;
    ensure!((1..=4096).contains(&max_pixels), "max-pixels must be between 1 and 4096");
    let first = args.next().context(USAGE)?;
    let selection = if first == "--fps" {
        let fps = number(args.next().context(USAGE)?)?;
        ensure!((1..=60).contains(&fps) && args.next().is_none(), "{USAGE}");
        Selection::FullClip(fps)
    } else {
        Selection::Times(std::iter::once(first).chain(args).map(number).collect::<Result<Vec<_>>>()?)
    };
    extract(Path::new(&input), &output, max_pixels as u32, selection, |frame| {
        println!("{} ms -> {} ms    {}", frame.requested_ms, frame.actual_ms, frame.path.display());
    })?;
    Ok(())
}

fn number(value: std::ffi::OsString) -> Result<u64> {
    value.to_str().context("expected an integer")?.parse().context("expected an unsigned integer")
}
