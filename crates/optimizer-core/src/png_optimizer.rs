use std::io::{Cursor, Read, Write};

use crc32fast::Hasher;
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use image::{GenericImageView, RgbImage, RgbaImage};
use quantette::{ImageBuf, PaletteSize, Pipeline, QuantizeMethod};

use crate::{check_cancelled, types::*};

const PALETTE_COLORS: u16 = 256;
const MAX_PALETTE_MEAN_DELTA: f32 = 2.5;

#[allow(clippy::too_many_arguments)]
pub(crate) fn optimize_png(
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
    let bit_depth = input.get(24).copied().unwrap_or(8);
    let sensitive_chunks = has_sensitive_png_chunks(input);
    if bit_depth == 16 || analysis.has_alpha || sensitive_chunks || !is_palette_candidate(&analysis)
    {
        let warnings = lossless_warnings(bit_depth, analysis.has_alpha, sensitive_chunks);
        return optimize_lossless(
            input,
            width,
            height,
            analysis,
            warnings,
            progress,
            cancellation,
            options.limits,
            observer,
        );
    }

    check_cancelled(cancellation)?;
    progress.report(ProgressEvent {
        stage: ProgressStage::Compressing,
    });
    let rgb = RgbImage::from_fn(width, height, |x, y| {
        let pixel = reference.get_pixel(x, y);
        image::Rgb([pixel[0], pixel[1], pixel[2]])
    });
    let quantette_image =
        ImageBuf::try_from(rgb).map_err(|error| OptimizeError::Encode(error.to_string()))?;
    let palette_size = PaletteSize::try_from(PALETTE_COLORS)
        .map_err(|error| OptimizeError::Encode(error.to_string()))?;

    observer.begin(OptimizationOperation::PaletteBuild);
    let indexed = Pipeline::new()
        .palette_size(palette_size)
        .quantize_method(QuantizeMethod::kmeans())
        .input_image(quantette_image.as_ref())
        .output_srgb8_indexed_image();
    observer.end(OptimizationOperation::PaletteBuild);

    // This is a single, cheap guard on the already-selected indexed image.
    // It does not decode another file and never triggers a second encoder run.
    if palette_mean_delta(&reference, indexed.palette(), indexed.indices()) > MAX_PALETTE_MEAN_DELTA
    {
        return optimize_lossless(
            input,
            width,
            height,
            analysis,
            vec![
                "Paleta 256 kolorów zbyt mocno zmieniałaby wygląd; zachowano bezstratny PNG."
                    .into(),
            ],
            progress,
            cancellation,
            options.limits,
            observer,
        );
    }

    observer.begin(OptimizationOperation::PngEncode);
    let output = encode_indexed_png(width, height, indexed.palette(), indexed.indices())?;
    observer.end(OptimizationOperation::PngEncode);
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
            format: ImageFormat::Png,
            output_format: ImageFormat::Png,
            width,
            height,
            original_size: input.len(),
            optimized_size,
            saved_bytes,
            saved_percent: saved_bytes as f32 / input.len() as f32 * 100.0,
            strategy: SelectedStrategy {
                encoder: "quantette".into(),
                quality: None,
                chroma_subsampling: None,
                progressive: None,
                palette_colors: Some(PALETTE_COLORS),
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

fn is_palette_candidate(analysis: &ImageAnalysis) -> bool {
    !matches!(analysis.kind, ContentKind::Photo)
        && analysis.noise < 0.08
        && analysis.estimated_colors <= 65_536
}

fn lossless_warnings(bit_depth: u8, has_alpha: bool, sensitive_chunks: bool) -> Vec<String> {
    let mut warnings = Vec::new();
    if bit_depth == 16 {
        warnings.push("PNG 16-bit pozostawiono w 16 bitach i skompresowano bezstratnie.".into());
    }
    if has_alpha {
        warnings.push("Przezroczystość zachowano w bezstratnym przebiegu PNG.".into());
    }
    if sensitive_chunks {
        warnings.push("Profil koloru i metadane PNG zachowano bezstratnie.".into());
    }
    if warnings.is_empty() {
        warnings.push("Ten PNG otrzymał jeden bezstratny przebieg.".into());
    }
    warnings
}

#[allow(clippy::too_many_arguments)]
fn optimize_lossless(
    input: &[u8],
    width: u32,
    height: u32,
    analysis: ImageAnalysis,
    mut warnings: Vec<String>,
    progress: &dyn ProgressSink,
    cancellation: &dyn CancellationToken,
    limits: ResourceLimits,
    observer: &dyn OptimizationObserver,
) -> Result<OptimizationResult, OptimizeError> {
    check_cancelled(cancellation)?;
    progress.report(ProgressEvent {
        stage: ProgressStage::Compressing,
    });
    observer.begin(OptimizationOperation::LosslessRecompress);
    let output = recompress_png_idat(input, limits)?;
    observer.end(OptimizationOperation::LosslessRecompress);
    progress.report(ProgressEvent {
        stage: ProgressStage::Finalizing,
    });
    if output.len() >= input.len() {
        warnings.push("Brak oszczędności w tym przebiegu.".into());
        return Ok(passthrough(input, width, height, analysis, warnings));
    }

    observer.begin(OptimizationOperation::LosslessVerification);
    let before = image::load_from_memory_with_format(input, image::ImageFormat::Png)
        .map_err(|error| OptimizeError::Decode(error.to_string()))?;
    let after = image::load_from_memory_with_format(&output, image::ImageFormat::Png)
        .map_err(|error| OptimizeError::Decode(error.to_string()))?;
    let pixels_equal = if input.get(24) == Some(&16) {
        before.to_rgba16() == after.to_rgba16()
    } else {
        before.to_rgba8() == after.to_rgba8()
    };
    observer.end(OptimizationOperation::LosslessVerification);
    if before.dimensions() != after.dimensions() || !pixels_equal {
        return Err(OptimizeError::Encode(
            "lossless PNG verification failed; decoded pixels changed".into(),
        ));
    }

    let optimized_size = output.len();
    let saved_bytes = input.len() - optimized_size;
    Ok(OptimizationResult {
        output,
        report: OptimizationReport {
            format: ImageFormat::Png,
            output_format: ImageFormat::Png,
            width,
            height,
            original_size: input.len(),
            optimized_size,
            saved_bytes,
            saved_percent: saved_bytes as f32 / input.len() as f32 * 100.0,
            strategy: SelectedStrategy {
                encoder: "pure-rust PNG zlib".into(),
                quality: None,
                chroma_subsampling: None,
                progressive: None,
                palette_colors: None,
                lossless: true,
            },
            processing_time_ms: 0.0,
            already_optimized: false,
            optimizer_version: OPTIMIZER_VERSION,
            warnings,
            analysis,
        },
    })
}

/// Re-encodes only the PNG's zlib stream. Non-IDAT chunks are kept byte for
/// byte, including ICC, alpha and 16-bit metadata. Level six is materially
/// faster than maximum compression and is appropriate for a one-shot UI path.
fn recompress_png_idat(input: &[u8], limits: ResourceLimits) -> Result<Vec<u8>, OptimizeError> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if !input.starts_with(PNG_SIGNATURE) {
        return Err(OptimizeError::Decode("invalid PNG signature".into()));
    }

    let mut idat = Vec::new();
    let mut cursor = PNG_SIGNATURE.len();
    while cursor < input.len() {
        let (chunk_end, chunk_type, data) = png_chunk(input, cursor)?;
        if chunk_type == b"IDAT" {
            idat.extend_from_slice(data);
        }
        cursor = chunk_end;
    }
    if idat.is_empty() {
        return Err(OptimizeError::Decode("PNG has no IDAT data".into()));
    }

    let raw = inflate_png_data(&idat, limits)?;
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&raw)
        .map_err(|error| OptimizeError::Encode(error.to_string()))?;
    let compressed = encoder
        .finish()
        .map_err(|error| OptimizeError::Encode(error.to_string()))?;

    let mut output = Vec::with_capacity(input.len());
    output.extend_from_slice(PNG_SIGNATURE);
    cursor = PNG_SIGNATURE.len();
    let mut wrote_idat = false;
    while cursor < input.len() {
        let (chunk_end, chunk_type, _data) = png_chunk(input, cursor)?;
        if chunk_type == b"IDAT" {
            if !wrote_idat {
                append_png_chunk(&mut output, b"IDAT", &compressed);
                wrote_idat = true;
            }
        } else {
            output.extend_from_slice(&input[cursor..chunk_end]);
        }
        cursor = chunk_end;
    }
    Ok(output)
}

fn png_chunk(input: &[u8], start: usize) -> Result<(usize, &[u8], &[u8]), OptimizeError> {
    let header_end = start
        .checked_add(8)
        .ok_or_else(|| OptimizeError::Decode("PNG chunk header overflow".into()))?;
    if header_end > input.len() {
        return Err(OptimizeError::Decode("truncated PNG chunk header".into()));
    }
    let length = u32::from_be_bytes(input[start..start + 4].try_into().unwrap()) as usize;
    let data_start = header_end;
    let data_end = data_start
        .checked_add(length)
        .ok_or_else(|| OptimizeError::Decode("PNG chunk data overflow".into()))?;
    let chunk_end = data_end
        .checked_add(4)
        .ok_or_else(|| OptimizeError::Decode("PNG chunk CRC overflow".into()))?;
    if chunk_end > input.len() {
        return Err(OptimizeError::Decode("truncated PNG chunk data".into()));
    }
    Ok((
        chunk_end,
        &input[start + 4..header_end],
        &input[data_start..data_end],
    ))
}

fn inflate_png_data(compressed: &[u8], limits: ResourceLimits) -> Result<Vec<u8>, OptimizeError> {
    let mut decoder = ZlibDecoder::new(compressed);
    let mut raw = Vec::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = decoder
            .read(&mut buffer)
            .map_err(|error| OptimizeError::Decode(error.to_string()))?;
        if read == 0 {
            break;
        }
        let estimated = raw.len().saturating_add(read) as u64;
        if estimated > limits.max_working_bytes {
            return Err(OptimizeError::MemoryLimit {
                estimated,
                limit: limits.max_working_bytes,
            });
        }
        raw.extend_from_slice(&buffer[..read]);
    }
    Ok(raw)
}

fn append_png_chunk(output: &mut Vec<u8>, chunk_type: &[u8; 4], data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(chunk_type);
    output.extend_from_slice(data);
    let mut hasher = Hasher::new();
    hasher.update(chunk_type);
    hasher.update(data);
    output.extend_from_slice(&hasher.finalize().to_be_bytes());
}

fn encode_indexed_png(
    width: u32,
    height: u32,
    palette: &[quantette::deps::palette::Srgb<u8>],
    indices: &[u8],
) -> Result<Vec<u8>, OptimizeError> {
    let (palette, indices) = sort_palette_by_luma(palette, indices);
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut output), width, height);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(png::BitDepth::Eight);
        let bytes = palette
            .iter()
            .flat_map(|color| [color.red, color.green, color.blue])
            .collect::<Vec<_>>();
        encoder.set_palette(bytes);
        encoder
            .write_header()
            .map_err(|error| OptimizeError::Encode(error.to_string()))?
            .write_image_data(&indices)
            .map_err(|error| OptimizeError::Encode(error.to_string()))?;
    }
    Ok(output)
}

fn palette_mean_delta(
    reference: &RgbaImage,
    palette: &[quantette::deps::palette::Srgb<u8>],
    indices: &[u8],
) -> f32 {
    let total = reference
        .pixels()
        .zip(indices)
        .map(|(pixel, index)| {
            let color = palette[*index as usize];
            u32::from(pixel[0].abs_diff(color.red))
                + u32::from(pixel[1].abs_diff(color.green))
                + u32::from(pixel[2].abs_diff(color.blue))
        })
        .sum::<u32>();
    total as f32 / (reference.width().saturating_mul(reference.height()).max(1) * 3) as f32
}

fn sort_palette_by_luma(
    palette: &[quantette::deps::palette::Srgb<u8>],
    indices: &[u8],
) -> (Vec<quantette::deps::palette::Srgb<u8>>, Vec<u8>) {
    let mut order = (0..palette.len()).collect::<Vec<_>>();
    order.sort_unstable_by_key(|&index| {
        let color = palette[index];
        std::cmp::Reverse(
            u32::from(color.red) * 299 + u32::from(color.green) * 587 + u32::from(color.blue) * 114,
        )
    });
    let mut remap = [0_u8; 256];
    for (new, &old) in order.iter().enumerate() {
        remap[old] = new as u8;
    }
    let sorted_palette = order.into_iter().map(|index| palette[index]).collect();
    let sorted_indices = indices.iter().map(|&index| remap[index as usize]).collect();
    (sorted_palette, sorted_indices)
}

fn has_sensitive_png_chunks(input: &[u8]) -> bool {
    let mut cursor = 8_usize;
    while cursor + 12 <= input.len() {
        let length = u32::from_be_bytes([
            input[cursor],
            input[cursor + 1],
            input[cursor + 2],
            input[cursor + 3],
        ]) as usize;
        if cursor + 12 + length > input.len() {
            break;
        }
        let kind = &input[cursor + 4..cursor + 8];
        if matches!(
            kind,
            b"iCCP" | b"gAMA" | b"cHRM" | b"sRGB" | b"cICP" | b"eXIf"
        ) {
            return true;
        }
        if kind == b"IEND" {
            break;
        }
        cursor += length + 12;
    }
    false
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
            format: ImageFormat::Png,
            output_format: ImageFormat::Png,
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
    use std::{cell::RefCell, collections::BTreeSet};

    #[derive(Default)]
    struct RecordingObserver {
        operations: RefCell<BTreeSet<OptimizationOperation>>,
    }

    impl OptimizationObserver for RecordingObserver {
        fn begin(&self, operation: OptimizationOperation) {
            self.operations.borrow_mut().insert(operation);
        }

        fn end(&self, _operation: OptimizationOperation) {}
    }

    #[test]
    fn automatic_png_uses_one_encode_without_runtime_quality_measurement() {
        let width = 96;
        let height = 64;
        let reference = RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([x as u8, y as u8, (x.wrapping_add(y)) as u8, 255])
        });
        let input = encode_rgba_fast(&reference);
        let observer = RecordingObserver::default();
        let _result = optimize_png(
            &input,
            reference.clone(),
            width,
            height,
            crate::analysis::analyze(&reference),
            OptimizeOptions::default(),
            &NoProgress,
            &NeverCancelled,
            &observer,
        )
        .unwrap();

        let operations = observer.operations.borrow();
        let palette_pass = operations.contains(&OptimizationOperation::PaletteBuild)
            && operations.contains(&OptimizationOperation::PngEncode);
        let lossless_pass = operations.contains(&OptimizationOperation::LosslessRecompress);
        assert!(palette_pass || lossless_pass);
        assert!(!(palette_pass && lossless_pass));
        assert!(
            !operations.contains(&OptimizationOperation::LosslessVerification) || lossless_pass
        );
    }

    #[test]
    fn transparent_png_uses_lossless_path_and_keeps_pixels() {
        let reference = RgbaImage::from_fn(64, 64, |x, y| {
            image::Rgba([x as u8, y as u8, 128, if x == y { 100 } else { 255 }])
        });
        let input = encode_rgba_fast(&reference);
        let result = optimize_png(
            &input,
            reference.clone(),
            64,
            64,
            crate::analysis::analyze(&reference),
            OptimizeOptions::default(),
            &NoProgress,
            &NeverCancelled,
            &NoObserver,
        )
        .unwrap();
        assert!(result.report.strategy.lossless);
        assert_eq!(decode_png(&result.output).unwrap(), reference);
    }

    #[test]
    fn palette_sort_keeps_each_index_bound_to_its_color() {
        let palette = vec![
            quantette::deps::palette::Srgb::new(10, 10, 10),
            quantette::deps::palette::Srgb::new(240, 240, 240),
            quantette::deps::palette::Srgb::new(120, 120, 120),
        ];
        let indices = vec![0, 1, 2, 1, 0];
        let (sorted, remapped) = sort_palette_by_luma(&palette, &indices);
        let restored = remapped
            .iter()
            .map(|&index| sorted[index as usize])
            .collect::<Vec<_>>();
        let expected = indices
            .iter()
            .map(|&index| palette[index as usize])
            .collect::<Vec<_>>();
        assert_eq!(restored, expected);
    }

    #[test]
    fn sensitive_chunks_include_orientation_metadata() {
        let mut input = b"\x89PNG\r\n\x1a\n".to_vec();
        input.extend_from_slice(&0_u32.to_be_bytes());
        input.extend_from_slice(b"eXIf");
        input.extend_from_slice(&[0; 4]);
        assert!(has_sensitive_png_chunks(&input));
    }

    fn encode_rgba_fast(reference: &RgbaImage) -> Vec<u8> {
        let mut input = Vec::new();
        let mut encoder = png::Encoder::new(
            Cursor::new(&mut input),
            reference.width(),
            reference.height(),
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Fast);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(reference.as_raw())
            .unwrap();
        input
    }

    fn decode_png(bytes: &[u8]) -> Result<RgbaImage, OptimizeError> {
        image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .map(image::DynamicImage::into_rgba8)
            .map_err(|error| OptimizeError::Decode(error.to_string()))
    }
}
