use crate::mov;

use std::{
    collections::{BTreeMap, VecDeque},
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use anyhow::{bail, ensure, Context, Result};
use image::{codecs::jpeg::JpegEncoder, imageops, Rgb, RgbImage};
use mp4::Mp4Reader;
use rust_h265::{parse_hvcc, parse_nal, Decoder, Frame, PixelData};

/// Frame requests in presentation time, in milliseconds.
pub enum Selection {
    Times(Vec<u64>),
    FullClip(u64),
    /// First frame on the movie timeline, including its edit list.
    FirstFrame,
}

pub struct SavedFrame {
    pub path: PathBuf,
    pub requested_ms: u64,
    pub actual_ms: i128,
}

/// Extract upright JPEGs and report each saved frame. Returns the video duration in ms.
pub fn extract(
    input: &Path,
    output: &Path,
    max_pixels: u32,
    selection: Selection,
    mut on_saved: impl FnMut(SavedFrame),
) -> Result<u64> {
    ensure!((1..=4096).contains(&max_pixels), "max-pixels must be between 1 and 4096");
    let file = File::open(input).with_context(|| format!("open {}", input.display()))?;
    let size = file.metadata()?.len();
    let (source, config) = mov::VideoOnlyMov::open(file)?;
    let mut movie = Mp4Reader::read_header(BufReader::new(source), size)?;
    ensure!(!movie.is_fragmented(), "fragmented movies are unsupported");
    let track = movie.moov.traks.first().context("missing video track")?;
    let matrix = &track.tkhd.matrix;
    let rotation = rotation((matrix.a, matrix.b, matrix.c, matrix.d))?;
    let movie_scale = i128::from(movie.timescale());
    let id = track.tkhd.track_id;
    let timescale = i128::from(movie.tracks().get(&id).context("missing video track timing")?.timescale());
    ensure!(movie_scale != 0 && timescale != 0, "video has no timescale");

    let mut media_start = 0;
    let mut edit_start = 0;
    let mut edit_end = i128::MAX;
    if let Some(list) = track.edts.as_ref().and_then(|edit| edit.elst.as_ref()) {
        let empty_time = if list.version == 1 { u64::MAX } else { u64::from(u32::MAX) };
        let edit = match list.entries.as_slice() {
            [] => None,
            [media] => Some((0, media)),
            [empty, media] if empty.media_time == empty_time => Some((empty.segment_duration, media)),
            _ => bail!("unsupported movie edit list"),
        };
        if let Some((leading, media)) = edit {
            ensure!(
                media.media_time != empty_time && media.media_rate == 1 && media.media_rate_fraction == 0,
                "unsupported movie edit"
            );
            media_start = i128::from(media.media_time);
            edit_start = i128::from(leading) * timescale * 1000;
            edit_end = edit_start + i128::from(media.segment_duration) * timescale * 1000;
        }
    }
    let tick_scale = movie_scale * timescale;

    // Select by composition time before decoding in sample order. B-frames
    // can appear earlier on the timeline than the sample decoded before them.
    let count = movie.sample_count(id)?;
    let mut presentation = Vec::new();
    for index in 1..=count {
        let sample = movie.read_sample(id, index)?.context("missing video sample")?;
        let media_time = i128::from(sample.start_time) + i128::from(sample.rendering_offset);
        let start = (media_time - media_start) * movie_scale * 1000 + edit_start;
        let end = start + i128::from(sample.duration) * movie_scale * 1000;
        presentation.push((start, end, index));
    }
    presentation.sort_unstable();
    let &(first, _, _) = presentation.first().context("video track has no samples")?;
    let &(_, last, _) = presentation.last().context("video track has no samples")?;
    let end = last.min(edit_end);
    let initial = u64::try_from((first.max(edit_start) + tick_scale - 1) / tick_scale)?;
    let duration_ms = u64::try_from((end + tick_scale - 1) / tick_scale)?;
    let times = match selection {
        Selection::Times(mut times) => {
            times.sort_unstable();
            times.dedup();
            times
        }
        Selection::FirstFrame => vec![initial],
        Selection::FullClip(fps) => {
            ensure!((1..=60).contains(&fps), "fps must be between 1 and 60");
            (0..)
                .map(|index| initial + (index * 1000 + fps / 2) / fps)
                .take_while(|&time| i128::from(time) * tick_scale < end)
                .collect()
        }
    };
    ensure!(!times.is_empty(), "no frames selected");
    let mut selected = BTreeMap::<u32, Vec<(u64, i128)>>::new();
    for time in times {
        let tick = i128::from(time) * tick_scale;
        let next = presentation.partition_point(|&(start, _, _)| start <= tick);
        ensure!(next > 0 && edit_start <= tick && tick < edit_end, "no video frame at {time} ms");
        let (start, end, index) = presentation[next - 1];
        ensure!(next < presentation.len() || tick < end, "no video frame at {time} ms");
        selected.entry(index).or_default().push((time, start.div_euclid(tick_scale)));
    }

    let threads = std::thread::available_parallelism().map_or(1, |threads| threads.get().min(4));
    let mut decoder = Decoder::new().with_threads(threads);
    let length_size = load_parameter_sets(&mut decoder, &config)?;
    fs::create_dir_all(output)?;
    let basename = input.file_stem().context("missing input file name")?.to_string_lossy();
    let mut pending = VecDeque::new();
    for index in 1..=count {
        let sample = movie.read_sample(id, index)?.context("missing video sample")?;
        let units = parse_hvcc(&sample.bytes, length_size);
        ensure!(units.iter().any(|nal| nal.nal_unit_type.is_vcl()), "video sample {index} has no picture");
        pending.push_back(index);
        for unit in units {
            if let Some(frame) = decoder.decode_nal(&unit).with_context(|| format!("decode video sample {index}"))? {
                let index = pending.pop_front().context("unexpected decoded frame")?;
                if let Some(requests) = selected.remove(&index) {
                    save_frames(&frame, output, &basename, max_pixels, rotation, requests, &mut on_saved)?;
                }
            }
        }
        if selected.is_empty() {
            return Ok(duration_ms);
        }
    }
    while let Some(frame) = decoder.flush() {
        let index = pending.pop_front().context("unexpected decoded frame")?;
        if let Some(requests) = selected.remove(&index) {
            save_frames(&frame, output, &basename, max_pixels, rotation, requests, &mut on_saved)?;
        }
    }
    if let Some(error) = decoder.next_error() {
        return Err(error.into());
    }
    ensure!(selected.is_empty(), "decoder did not return every requested frame");
    Ok(duration_ms)
}

fn load_parameter_sets(decoder: &mut Decoder, data: &[u8]) -> Result<u8> {
    ensure!(data.len() >= 23 && data[0] == 1, "invalid HEVC configuration");
    let length_size = (data[21] & 3) + 1;
    let mut arrays = &data[23..];
    for _ in 0..data[22] {
        let (header, rest) = arrays.split_at_checked(3).context("truncated HEVC NAL array")?;
        arrays = rest;
        for _ in 0..u16::from_be_bytes([header[1], header[2]]) {
            let (size, rest) = arrays.split_at_checked(2).context("truncated HEVC NAL size")?;
            let size = usize::from(u16::from_be_bytes([size[0], size[1]]));
            let (unit, rest) = rest.split_at_checked(size).context("truncated HEVC NAL unit")?;
            decoder.decode_nal(&parse_nal(unit).context("invalid HEVC parameter set")?)?;
            arrays = rest;
        }
    }
    Ok(length_size)
}

fn rotation(matrix: (i32, i32, i32, i32)) -> Result<u16> {
    const ONE: i32 = 65536;
    const NEG_ONE: i32 = -ONE;
    match matrix {
        (ONE, 0, 0, ONE) => Ok(0),
        (0, ONE, NEG_ONE, 0) => Ok(90),
        (NEG_ONE, 0, 0, NEG_ONE) => Ok(180),
        (0, NEG_ONE, ONE, 0) => Ok(270),
        _ => bail!("unsupported video rotation matrix"),
    }
}

fn save_frames(
    frame: &Frame,
    output: &Path,
    basename: &str,
    max_pixels: u32,
    rotation: u16,
    requests: Vec<(u64, i128)>,
    on_saved: &mut impl FnMut(SavedFrame),
) -> Result<()> {
    let image = rgb(frame, max_pixels);
    let image = match rotation {
        90 => imageops::rotate90(&image),
        180 => imageops::rotate180(&image),
        270 => imageops::rotate270(&image),
        _ => image,
    };
    for (requested, actual) in requests {
        let path = output.join(format!("{basename}-{requested:06}.jpg"));
        let file = File::create_new(&path).with_context(|| format!("create {}", path.display()))?;
        let mut writer = BufWriter::new(file);
        JpegEncoder::new_with_quality(&mut writer, 90).encode_image(&image)?;
        writer.flush()?;
        on_saved(SavedFrame { path, requested_ms: requested, actual_ms: actual });
    }
    Ok(())
}

fn component(plane: &PixelData, index: usize, shift: u8) -> f64 {
    match plane {
        PixelData::U8(values) => f64::from(values[index]),
        PixelData::U16(values) => f64::from(values[index]) / f64::from(1u16 << shift),
    }
}

fn scaled(value: u32, input: u32, output: u32) -> u32 {
    (u64::from(value) * u64::from(output) / u64::from(input)) as u32
}

fn rgb(frame: &Frame, max_pixels: u32) -> RgbImage {
    let edge = frame.width.max(frame.height);
    let limit = max_pixels.min(edge);
    let width = scaled(frame.width, edge, limit).max(1);
    let height = scaled(frame.height, edge, limit).max(1);
    let subsample = if frame.chroma_format_idc == 1 { 2 } else { 1 };
    let chroma_width = frame.width.div_ceil(subsample);
    let shift = frame.bit_depth - 8;
    // Limited-range BT.709 is sufficient for position review JPEGs.
    let channel = |value: f64| value.round().clamp(0.0, 255.0) as u8;
    RgbImage::from_fn(width, height, |x, y| {
        let col = scaled(x, width, frame.width);
        let row = scaled(y, height, frame.height);
        let luma = (row * frame.width + col) as usize;
        let chroma = ((row / subsample) * chroma_width + col / subsample) as usize;
        let y = (component(&frame.y, luma, shift) - 16.0) * 255.0 / 219.0;
        let cb = (component(&frame.u, chroma, shift) - 128.0) * 255.0 / 224.0;
        let cr = (component(&frame.v, chroma, shift) - 128.0) * 255.0 / 224.0;
        Rgb([y + 1.5748 * cr, y - 0.1873 * cb - 0.4681 * cr, y + 1.8556 * cb].map(channel))
    })
}
