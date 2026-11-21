use std::{fs, path::Path};

use anyhow::{anyhow, Result};
use image::{codecs::jpeg::JpegEncoder, imageops, RgbImage};
use objc2::{rc::autoreleasepool, AnyThread};
use objc2_foundation::{NSArray, NSData, NSDictionary};
use objc2_vision::{VNDetectHumanRectanglesRequest, VNImageRequestHandler, VNRequest};

use super::association::Bounds;

pub fn detect(path: &Path, image: &RgbImage) -> Result<Vec<Bounds>> {
    let (width, height) = image.dimensions();
    let mut boxes = detect_jpeg(&fs::read(path)?, width, height)?;

    // Trim the ceiling and foreground before looking for small, distant skaters.
    // The horizontal overlap preserves full bodies near the center of the image.
    let top = height * 22 / 100;
    let crop_height = height * 51 / 100;
    let crop_width = width * 65 / 100;
    for left in [0, width - crop_width] {
        let crop = imageops::crop_imm(image, left, top, crop_width, crop_height).to_image();
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 90).encode_image(&crop)?;
        for mut candidate in detect_jpeg(&jpeg, crop_width, crop_height)? {
            let clipped_left = left > 0 && candidate.x < 2.0;
            let clipped_right = left + crop_width < width && candidate.x + candidate.w > crop_width as f32 - 2.0;
            if clipped_left || clipped_right {
                continue;
            }
            candidate.x += left as f32;
            candidate.y += top as f32;
            if !boxes.iter().any(|known| candidate.duplicates(*known)) {
                boxes.push(candidate);
            }
        }
    }
    Ok(boxes)
}

fn detect_jpeg(jpeg: &[u8], width: u32, height: u32) -> Result<Vec<Bounds>> {
    autoreleasepool(|_| {
        let data = NSData::with_bytes(jpeg);
        // The request and handler stay inside the pool and the option map is empty.
        let request = unsafe { VNDetectHumanRectanglesRequest::new() };
        unsafe { request.setUpperBodyOnly(false) };
        let requests = NSArray::<VNRequest>::from_retained_slice(&[request.clone().into_super().into_super()]);
        let handler =
            VNImageRequestHandler::initWithData_options(VNImageRequestHandler::alloc(), &data, &NSDictionary::new());
        handler.performRequests_error(&requests).map_err(|error| anyhow!("{error}"))?;

        let mut boxes = Vec::new();
        if let Some(results) = unsafe { request.results() } {
            for index in 0..results.count() {
                let observation = results.objectAtIndex(index);
                let rect = unsafe { observation.boundingBox() };
                let width = width as f32;
                let height = height as f32;
                let bounds = Bounds {
                    x: rect.origin.x as f32 * width,
                    y: (1.0 - (rect.origin.y + rect.size.height) as f32) * height,
                    w: rect.size.width as f32 * width,
                    h: rect.size.height as f32 * height,
                };
                if bounds.is_valid() {
                    boxes.push(bounds);
                }
            }
        }
        Ok(boxes)
    })
}
