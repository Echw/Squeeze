use butteraugli::{ButteraugliParams, Img, butteraugli};
use fast_ssim2::{
    CompareContext, LinearRgbImage, Ssimulacra2Reference, compute_ssimulacra2, srgb_u8_to_linear,
};
use image::RgbaImage;
use rgb::RGB8;

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
    compute_ssimulacra2(linear_rgb(reference), linear_rgb(candidate))
        .map_err(|error| OptimizeError::Metric(error.to_string()))
}

/// A tile whose SSIMULACRA2 reference is computed once and compared with
/// several candidates; it halves the cost of each comparison.
pub(crate) struct TileReference {
    reference: Ssimulacra2Reference,
    context: CompareContext,
}

impl TileReference {
    pub(crate) fn new(tile: &RgbaImage) -> Result<Self, OptimizeError> {
        let reference = Ssimulacra2Reference::new(linear_rgb(tile))
            .map_err(|error| OptimizeError::Metric(error.to_string()))?;
        let context = reference.compare_context();
        Ok(Self { reference, context })
    }

    pub(crate) fn score(&mut self, candidate: &RgbaImage) -> Result<f64, OptimizeError> {
        self.reference
            .compare_with(&mut self.context, linear_rgb(candidate))
            .map_err(|error| OptimizeError::Metric(error.to_string()))
    }
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
    .map(|result| result.score)
    .map_err(|error| OptimizeError::Metric(error.to_string()))
}

fn ensure_dimensions(a: &RgbaImage, b: &RgbaImage) -> Result<(), OptimizeError> {
    if a.dimensions() != b.dimensions() {
        return Err(OptimizeError::Metric("candidate dimensions changed".into()));
    }
    Ok(())
}

fn linear_rgb(image: &RgbaImage) -> LinearRgbImage {
    let pixels = image
        .pixels()
        .map(|pixel| {
            let [r, g, b, _] = pixel.0;
            [
                srgb_u8_to_linear(r),
                srgb_u8_to_linear(g),
                srgb_u8_to_linear(b),
            ]
        })
        .collect();
    LinearRgbImage::new(pixels, image.width() as usize, image.height() as usize)
}

fn rgb8(image: &RgbaImage) -> Vec<RGB8> {
    image
        .pixels()
        .map(|pixel| RGB8::new(pixel[0], pixel[1], pixel[2]))
        .collect()
}
