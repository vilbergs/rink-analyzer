use std::{
    fs::File,
    io::{BufWriter, Write},
};

use anyhow::{ensure, Context, Result};
use image::RgbImage;
use mp4::{AvcConfig, Mp4Config, Mp4Sample, Mp4Writer, TrackConfig};
use rusty_h264_common::{nal::split_annex_b, YuvPlanes};
use rusty_h264_encoder::{Encoder, EncoderConfig, Preset};

pub struct Video {
    encoder: Encoder,
    writer: Mp4Writer<BufWriter<File>>,
    dimensions: (u32, u32),
    fps: u32,
    count: u64,
}

impl Video {
    pub fn new(file: File, width: u32, height: u32, fps: u32) -> Result<Self> {
        ensure!(width % 2 == 0 && height % 2 == 0, "the frame dimensions must be even");
        let mut config = EncoderConfig::baseline(width as usize, height as usize);
        config.framerate = fps as f32;
        config.gop_size = fps;
        config.preset = Preset::Fast;
        config.bitrate = 2_000_000;
        config.bframes = 0;
        let writer = Mp4Writer::write_start(
            BufWriter::new(file),
            &Mp4Config {
                major_brand: "isom".parse()?,
                minor_version: 512,
                compatible_brands: vec!["isom".parse()?, "avc1".parse()?, "mp42".parse()?],
                timescale: fps,
            },
        )?;
        let encoder = Encoder::new(config).map_err(|error| anyhow::anyhow!("H.264 configuration: {error:?}"))?;
        Ok(Self { encoder, writer, dimensions: (width, height), fps, count: 0 })
    }

    pub fn frame(&mut self, image: &RgbImage, repetitions: u32) -> Result<()> {
        ensure!(self.dimensions == image.dimensions(), "image dimensions changed");
        let (width, height) = (image.width() as usize, image.height() as usize);
        let (y, u, v) = convert(image, width, height);
        let planes = YuvPlanes::new(width, height, &y, &u, &v, width, width / 2).context("invalid YUV planes")?;

        for _ in 0..repetitions {
            let bytes =
                self.encoder.encode_planes(&planes).map_err(|error| anyhow::anyhow!("encode frame: {error:?}"))?;
            let units = split_annex_b(&bytes);
            if self.count == 0 {
                let parameter = |kind| {
                    units
                        .iter()
                        .find(|nal| nal[0] & 31 == kind)
                        .map(|nal| nal.to_vec())
                        .context("encoder omitted an H.264 parameter set")
                };
                let mut track = TrackConfig::from(AvcConfig {
                    width: width.try_into()?,
                    height: height.try_into()?,
                    seq_param_set: parameter(7)?,
                    pic_param_set: parameter(8)?,
                });
                track.timescale = self.fps;
                self.writer.add_track(&track)?;
            }
            let is_sync = units.iter().any(|nal| nal[0] & 31 == 5);
            let mut sample = Vec::new();
            for nal in units.into_iter().filter(|nal| !matches!(nal[0] & 31, 7 | 8)) {
                sample.extend_from_slice(&u32::try_from(nal.len())?.to_be_bytes());
                sample.extend_from_slice(nal);
            }
            ensure!(!sample.is_empty(), "encoder omitted frame {}", self.count);
            self.writer.write_sample(
                1,
                &Mp4Sample { start_time: self.count, duration: 1, rendering_offset: 0, is_sync, bytes: sample.into() },
            )?;
            self.count += 1;
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<()> {
        ensure!(self.encoder.flush().is_empty(), "unexpected delayed H.264 frames");
        self.writer.write_end()?;
        self.writer.into_writer().flush()?;
        Ok(())
    }
}

fn convert(image: &RgbImage, width: usize, height: usize) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut y = Vec::with_capacity(width * height);
    let mut u = Vec::with_capacity(width * height / 4);
    let mut v = Vec::with_capacity(width * height / 4);
    for pixel in image.pixels() {
        let [r, g, b] = pixel.0.map(i32::from);
        y.push((((66 * r + 129 * g + 25 * b + 128) >> 8) + 16) as u8);
    }
    for row in (0..height as u32).step_by(2) {
        for col in (0..width as u32).step_by(2) {
            let mut rgb = [0; 3];
            for (x, y) in [(col, row), (col + 1, row), (col, row + 1), (col + 1, row + 1)] {
                for (sum, value) in rgb.iter_mut().zip(image.get_pixel(x, y).0) {
                    *sum += i32::from(value);
                }
            }
            let [r, g, b] = rgb.map(|value| (value + 2) / 4);
            u.push((((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128) as u8);
            v.push((((112 * r - 94 * g - 18 * b + 128) >> 8) + 128) as u8);
        }
    }
    (y, u, v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, io::Cursor, time::SystemTime};

    #[test]
    fn preview_track_and_container_report_the_same_duration() -> Result<()> {
        let unique = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!("rink-preview-{}-{unique}.mp4", std::process::id()));
        let file = File::create_new(&path)?;
        let mut video = Video::new(file, 32, 32, 15)?;
        video.frame(&RgbImage::new(32, 32), 15)?;
        video.finish()?;
        let bytes = fs::read(&path)?;
        fs::remove_file(path)?;
        let len = bytes.len() as u64;
        let movie = mp4::Mp4Reader::read_header(Cursor::new(bytes), len)?;
        let track = movie.tracks().get(&1).context("missing preview track")?;
        assert_eq!(track.sample_count(), 15);
        assert_eq!(track.duration().as_secs_f64(), 1.0);
        assert_eq!(movie.duration().as_secs_f64(), 1.0);
        Ok(())
    }
}
