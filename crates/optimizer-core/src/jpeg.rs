use image::RgbaImage;
use mozjpeg_rs::{Encoder, Preset, Subsampling};

use crate::{check_cancelled, jpeg_metadata, types::*};

#[allow(clippy::too_many_arguments)]
pub(crate) fn optimize_jpeg(
    input: &[u8],
    reference: RgbaImage,
    width: u32,
    height: u32,
    analysis: ImageAnalysis,
    options: OptimizeOptions,
    progress: &dyn ProgressSink,
    cancellation: &dyn CancellationToken,
    observer: &dyn OptimizationObserver,
) -> Result<OptimizationResult, OptimizeError> {
    check_cancelled(cancellation)?;
    let metadata = jpeg_metadata::extract(input)?;
    if metadata.components == 4 {
        // The decoder converts CMYK without its profile, so the colours would
        // change; this fallback path returns the original instead.
        return Ok(passthrough(
            input,
            width,
            height,
            analysis,
            vec!["JPEG w CMYK pozostawiono bez zmian, aby nie zmienić kolorów.".into()],
        ));
    }
    let (quality, subsampling) = jpeg_parameters(&analysis);
    let rgb = rgba_to_rgb(&reference);

    progress.report(ProgressEvent {
        stage: ProgressStage::Compressing,
    });
    observer.begin(OptimizationOperation::JpegEncode);
    let output = encode(
        &rgb,
        width,
        height,
        quality,
        subsampling,
        &metadata,
        options.limits,
    )?;
    observer.end(OptimizationOperation::JpegEncode);
    progress.report(ProgressEvent {
        stage: ProgressStage::Finalizing,
    });

    if output.len() >= input.len() {
        return Ok(passthrough(
            input,
            width,
            height,
            analysis,
            vec!["Brak oszczędności w tym przebiegu.".into()],
        ));
    }

    let optimized_size = output.len();
    let saved_bytes = input.len() - optimized_size;
    Ok(OptimizationResult {
        output,
        report: OptimizationReport {
            format: ImageFormat::Jpeg,
            output_format: ImageFormat::Jpeg,
            width,
            height,
            original_size: input.len(),
            optimized_size,
            saved_bytes,
            saved_percent: saved_bytes as f32 / input.len() as f32 * 100.0,
            strategy: SelectedStrategy {
                encoder: "mozjpeg-rs".into(),
                quality: Some(quality),
                chroma_subsampling: Some(subsampling_name(subsampling).into()),
                progressive: Some(true),
                palette_colors: None,
                lossless: false,
            },
            processing_time_ms: 0.0,
            already_optimized: false,
            optimizer_version: OPTIMIZER_VERSION,
            warnings: Vec::new(),
            analysis,
        },
    })
}

/// This is a parameter selection, not a candidate search. Photos can safely
/// use chroma subsampling; UI text and screenshots keep full chroma detail.
fn jpeg_parameters(analysis: &ImageAnalysis) -> (u8, Subsampling) {
    match analysis.kind {
        ContentKind::Photo => (88, Subsampling::S420),
        ContentKind::Graphic | ContentKind::Screenshot => (90, Subsampling::S444),
        ContentKind::Mixed => (88, Subsampling::S422),
    }
}

fn encode(
    rgb: &[u8],
    width: u32,
    height: u32,
    quality: u8,
    subsampling: Subsampling,
    metadata: &jpeg_metadata::JpegMetadata,
    limits: ResourceLimits,
) -> Result<Vec<u8>, OptimizeError> {
    let mut encoder = Encoder::new(Preset::ProgressiveSmallest)
        .quality(quality)
        .progressive(true)
        .subsampling(subsampling)
        .optimize_huffman(true)
        .limits(
            mozjpeg_rs::Limits::default()
                .max_pixel_count(limits.max_pixels)
                .max_alloc_bytes(limits.max_working_bytes as usize)
                .max_icc_profile_bytes(4 * 1024 * 1024),
        );
    // The output is YCbCr, so only an RGB profile still describes it. CMYK and
    // grey profiles would make viewers misinterpret the colours.
    if let Some(icc) = metadata
        .icc
        .as_ref()
        .filter(|icc| icc.get(16..20) == Some(b"RGB "))
    {
        encoder = encoder.icc_profile(icc.clone());
    }
    encoder
        .encode_rgb(rgb, width, height)
        .map_err(|error| OptimizeError::Encode(error.to_string()))
}

fn rgba_to_rgb(image: &RgbaImage) -> Vec<u8> {
    image
        .pixels()
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect()
}

fn subsampling_name(value: Subsampling) -> &'static str {
    match value {
        Subsampling::S444 => "4:4:4",
        Subsampling::S422 => "4:2:2",
        Subsampling::S420 => "4:2:0",
        Subsampling::S440 => "4:4:0",
        Subsampling::Gray => "gray",
    }
}

fn passthrough(
    input: &[u8],
    width: u32,
    height: u32,
    analysis: ImageAnalysis,
    warnings: Vec<String>,
) -> OptimizationResult {
    OptimizationResult {
        output: input.to_vec(),
        report: OptimizationReport {
            format: ImageFormat::Jpeg,
            output_format: ImageFormat::Jpeg,
            width,
            height,
            original_size: input.len(),
            optimized_size: input.len(),
            saved_bytes: 0,
            saved_percent: 0.0,
            strategy: SelectedStrategy {
                encoder: "already-optimized".into(),
                quality: None,
                chroma_subsampling: None,
                progressive: None,
                palette_colors: None,
                lossless: true,
            },
            processing_time_ms: 0.0,
            already_optimized: true,
            optimizer_version: OPTIMIZER_VERSION,
            warnings,
            analysis,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chooses_full_chroma_for_text_and_subsampling_for_photos() {
        let mut analysis = ImageAnalysis {
            kind: ContentKind::Photo,
            entropy: 0.0,
            estimated_colors: 0,
            edge_density: 0.0,
            noise: 0.0,
            flat_area_ratio: 0.0,
            has_alpha: false,
        };
        assert_eq!(jpeg_parameters(&analysis), (88, Subsampling::S420));
        analysis.kind = ContentKind::Screenshot;
        assert_eq!(jpeg_parameters(&analysis), (90, Subsampling::S444));
    }
}
