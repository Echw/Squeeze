mod analysis;
mod jpeg;
mod jpeg_metadata;
mod metrics;
mod png_optimizer;
mod types;

pub use types::*;

#[cfg(not(target_arch = "wasm32"))]
use std::time::Instant;

use std::io::Cursor;

use image::{DynamicImage, GenericImageView, ImageDecoder, ImageReader};

pub fn optimize(
    input: &[u8],
    options: OptimizeOptions,
    progress: &dyn ProgressSink,
    cancellation: &dyn CancellationToken,
) -> Result<OptimizationResult, OptimizeError> {
    if options.output_format != OutputFormat::Preserve {
        return Err(OptimizeError::UnsupportedOutputFormat);
    }
    #[cfg(not(target_arch = "wasm32"))]
    let started = Instant::now();
    if input.len() > options.limits.max_input_bytes {
        return Err(OptimizeError::InputTooLarge {
            limit: options.limits.max_input_bytes,
        });
    }
    check_cancelled(cancellation)?;
    progress.report(ProgressEvent {
        stage: ProgressStage::Decoding,
        candidate: None,
        total: None,
    });

    let format = detect_format(input)?;
    if format == ImageFormat::Png && contains_apng_control_chunk(input) {
        return Err(OptimizeError::AnimatedPng);
    }
    let reader = ImageReader::new(Cursor::new(input))
        .with_guessed_format()
        .map_err(|error| OptimizeError::Decode(error.to_string()))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|error| OptimizeError::Decode(error.to_string()))?;
    let (encoded_width, encoded_height) = decoder.dimensions();
    let encoded_pixels = u64::from(encoded_width) * u64::from(encoded_height);
    if encoded_pixels > options.limits.max_pixels {
        return Err(OptimizeError::PixelLimit {
            pixels: encoded_pixels,
            limit: options.limits.max_pixels,
        });
    }
    let estimated_memory = encoded_pixels
        .saturating_mul(20)
        .saturating_add(input.len() as u64 * 2);
    if estimated_memory > options.limits.max_working_bytes {
        return Err(OptimizeError::MemoryLimit {
            estimated: estimated_memory,
            limit: options.limits.max_working_bytes,
        });
    }
    let orientation = decoder
        .orientation()
        .map_err(|error| OptimizeError::Decode(error.to_string()))?;
    let mut decoded = DynamicImage::from_decoder(decoder)
        .map_err(|error| OptimizeError::Decode(error.to_string()))?;
    if format == ImageFormat::Jpeg && options.profile != CompressionProfile::Lossless {
        decoded.apply_orientation(orientation);
    }
    let (width, height) = decoded.dimensions();
    let pixels = u64::from(width) * u64::from(height);
    if pixels > options.limits.max_pixels {
        return Err(OptimizeError::PixelLimit {
            pixels,
            limit: options.limits.max_pixels,
        });
    }
    let reference = decoded.to_rgba8();
    check_cancelled(cancellation)?;
    progress.report(ProgressEvent {
        stage: ProgressStage::Analyzing,
        candidate: None,
        total: None,
    });
    let analysis = analysis::analyze(&reference);

    let result = match format {
        ImageFormat::Jpeg => jpeg::optimize_jpeg(
            input,
            reference,
            width,
            height,
            analysis,
            options,
            progress,
            cancellation,
        )?,
        ImageFormat::Png => png_optimizer::optimize_png(
            input,
            reference,
            width,
            height,
            analysis,
            options,
            progress,
            cancellation,
        )?,
    };
    #[cfg(not(target_arch = "wasm32"))]
    let result = {
        let mut result = result;
        result.report.processing_time_ms = started.elapsed().as_secs_f64() * 1000.0;
        result
    };
    Ok(result)
}

pub(crate) fn check_cancelled(token: &dyn CancellationToken) -> Result<(), OptimizeError> {
    if token.is_cancelled() {
        Err(OptimizeError::Cancelled)
    } else {
        Ok(())
    }
}

fn detect_format(input: &[u8]) -> Result<ImageFormat, OptimizeError> {
    if input.starts_with(&[0xff, 0xd8, 0xff]) {
        Ok(ImageFormat::Jpeg)
    } else if input.starts_with(b"\x89PNG\r\n\x1a\n") {
        Ok(ImageFormat::Png)
    } else {
        Err(OptimizeError::UnsupportedFormat)
    }
}

fn contains_apng_control_chunk(input: &[u8]) -> bool {
    let mut cursor = 8_usize;
    while cursor + 12 <= input.len() {
        let length = u32::from_be_bytes([
            input[cursor],
            input[cursor + 1],
            input[cursor + 2],
            input[cursor + 3],
        ]) as usize;
        if cursor + 12 + length > input.len() {
            return false;
        }
        let chunk_type = &input[cursor + 4..cursor + 8];
        if chunk_type == b"acTL" {
            return true;
        }
        if chunk_type == b"IEND" {
            break;
        }
        cursor += 12 + length;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_signatures() {
        assert_eq!(
            detect_format(&[0xff, 0xd8, 0xff]).unwrap(),
            ImageFormat::Jpeg
        );
        assert_eq!(
            detect_format(b"\x89PNG\r\n\x1a\nrest").unwrap(),
            ImageFormat::Png
        );
    }

    #[test]
    fn rejects_unknown_signature() {
        assert!(matches!(
            detect_format(b"GIF89a"),
            Err(OptimizeError::UnsupportedFormat)
        ));
    }

    #[test]
    fn detects_apng_control_chunk() {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&8_u32.to_be_bytes());
        bytes.extend_from_slice(b"acTL");
        bytes.extend_from_slice(&[0; 8]);
        bytes.extend_from_slice(&[0; 4]);
        assert!(contains_apng_control_chunk(&bytes));
    }

    #[test]
    fn rejects_input_over_byte_limit_before_decoding() {
        let options = OptimizeOptions {
            limits: ResourceLimits {
                max_input_bytes: 2,
                ..ResourceLimits::default()
            },
            ..OptimizeOptions::default()
        };
        assert!(matches!(
            optimize(&[0xff, 0xd8, 0xff], options, &NoProgress, &NeverCancelled),
            Err(OptimizeError::InputTooLarge { limit: 2 })
        ));
    }

    #[test]
    fn rejects_an_output_codec_without_an_explicit_adapter() {
        let options = OptimizeOptions {
            output_format: OutputFormat::Webp,
            ..OptimizeOptions::default()
        };
        assert!(matches!(
            optimize(&[0xff, 0xd8, 0xff], options, &NoProgress, &NeverCancelled),
            Err(OptimizeError::UnsupportedOutputFormat)
        ));
    }
}
