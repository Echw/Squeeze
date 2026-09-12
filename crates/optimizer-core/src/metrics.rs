use butteraugli::{ButteraugliParams, Img, butteraugli};
use image::RgbaImage;
use rgb::RGB8;
use ssimulacra2::{ColorPrimaries, Rgb, TransferCharacteristic, compute_frame_ssimulacra2};

use crate::types::OptimizeError;

pub(crate) fn ssimulacra2_score(
    reference: &RgbaImage,
    candidate: &RgbaImage,
) -> Result<f64, OptimizeError> {
    ensure_dimensions(reference, candidate)?;
    let (width, height) = reference.dimensions();
    if width < 8 || height < 8 {
        let mean_delta = reference
            .pixels()
            .zip(candidate.pixels())
            .map(|(a, b)| {
                (f64::from(a[0].abs_diff(b[0]))
                    + f64::from(a[1].abs_diff(b[1]))
                    + f64::from(a[2].abs_diff(b[2])))
                    / (3.0 * 255.0)
            })
            .sum::<f64>()
            / f64::from(width.saturating_mul(height).max(1));
        return Ok((100.0 - mean_delta * 100.0).clamp(0.0, 100.0));
    }
    let source = Rgb::new(
        normalized_rgb(reference),
        width as usize,
        height as usize,
        TransferCharacteristic::SRGB,
        ColorPrimaries::BT709,
    )
    .map_err(|error| OptimizeError::Metric(error.to_string()))?;
    let distorted = Rgb::new(
        normalized_rgb(candidate),
        width as usize,
        height as usize,
        TransferCharacteristic::SRGB,
        ColorPrimaries::BT709,
    )
    .map_err(|error| OptimizeError::Metric(error.to_string()))?;
    compute_frame_ssimulacra2(source, distorted)
        .map_err(|error| OptimizeError::Metric(error.to_string()))
}

pub(crate) fn butteraugli_score(
    reference: &RgbaImage,
    candidate: &RgbaImage,
) -> Result<f64, OptimizeError> {
    ensure_dimensions(reference, candidate)?;
    let (width, height) = reference.dimensions();
    if width < 8 || height < 8 {
        let max_delta = reference
            .pixels()
            .zip(candidate.pixels())
            .map(|(a, b)| {
                (u16::from(a[0].abs_diff(b[0]))
                    + u16::from(a[1].abs_diff(b[1]))
                    + u16::from(a[2].abs_diff(b[2]))) as f64
                    / (3.0 * 255.0)
            })
            .fold(0.0_f64, f64::max);
        return Ok(max_delta * 3.0);
    }
    let source_pixels = rgb8(reference);
    let candidate_pixels = rgb8(candidate);
    let source = Img::new(source_pixels, width as usize, height as usize);
    let distorted = Img::new(candidate_pixels, width as usize, height as usize);
    butteraugli(
        source.as_ref(),
        distorted.as_ref(),
        &ButteraugliParams::default(),
    )
    .map(|result| f64::from(result.score))
    .map_err(|error| OptimizeError::Metric(error.to_string()))
}

fn ensure_dimensions(a: &RgbaImage, b: &RgbaImage) -> Result<(), OptimizeError> {
    if a.dimensions() != b.dimensions() {
        return Err(OptimizeError::Metric("candidate dimensions changed".into()));
    }
    Ok(())
}

fn normalized_rgb(image: &RgbaImage) -> Vec<[f32; 3]> {
    image
        .pixels()
        .map(|pixel| {
            let [r, g, b, _] = pixel.0;
            [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
        })
        .collect()
}

fn rgb8(image: &RgbaImage) -> Vec<RGB8> {
    image
        .pixels()
        .map(|pixel| RGB8::new(pixel[0], pixel[1], pixel[2]))
        .collect()
}
