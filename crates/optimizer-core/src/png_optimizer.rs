use std::io::{Cursor, Read, Write};

use crc32fast::Hasher;
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use image::{DynamicImage, GenericImageView, RgbImage, RgbaImage};
use quantette::{ImageBuf, PaletteSize, Pipeline, QuantizeMethod, dither::FloydSteinberg};

use crate::{
    check_cancelled,
    metrics::{butteraugli_score, ssimulacra2_score},
    types::*,
};

struct PngWinner {
    bytes: Vec<u8>,
    colors: u16,
    dithered: bool,
    ssimulacra2: f64,
    butteraugli: f64,
}

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
) -> Result<OptimizationResult, OptimizeError> {
    let bit_depth = input.get(24).copied().unwrap_or(8);
    let force_lossless = options.method == CompressionMethod::Lossless
        || options.profile == CompressionProfile::Lossless
        || bit_depth == 16
        || analysis.has_alpha
        || has_color_management_chunks(input);
    if force_lossless {
        let mut warnings = Vec::new();
        if options.profile != CompressionProfile::Lossless {
            if bit_depth == 16 {
                warnings.push(
                    "PNG 16-bit zoptymalizowano bezstratnie; nie zredukowano go do 8 bitów.".into(),
                );
            }
            if analysis.has_alpha {
                warnings
                    .push("PNG z kanałem alpha używa w v1 bezpiecznej ścieżki lossless.".into());
            }
            if has_color_management_chunks(input) {
                warnings.push(
                    "Zachowano osadzony profil i informacje o kolorze przez ścieżkę lossless."
                        .into(),
                );
            }
        }
        if options.method == CompressionMethod::Palette {
            return Ok(passthrough(
                input,
                width,
                height,
                analysis,
                0,
                vec!["Metoda paletowa nie obsługuje bez utraty danych PNG z alpha, 16-bit lub osadzonym kolorem; zachowano oryginał.".into()],
            ));
        }
        return optimize_lossless(
            input,
            width,
            height,
            analysis,
            warnings,
            1,
            progress,
            cancellation,
            options.limits,
            None,
        );
    }

    let rules = options
        .profile
        .rules(options.search_effort)
        .expect("lossy profile has rules");
    let rgb = RgbImage::from_fn(width, height, |x, y| {
        let pixel = reference.get_pixel(x, y);
        image::Rgb([pixel[0], pixel[1], pixel[2]])
    });
    let quantette_image =
        ImageBuf::try_from(rgb).map_err(|error| OptimizeError::Encode(error.to_string()))?;
    let palette_sizes = palette_schedule(analysis.estimated_colors);
    let mut candidates_tested = 0_u16;
    let mut winner: Option<PngWinner> = None;

    let candidate_budget = match options.method {
        // Smart evaluates only the closest palette candidates. The full budget
        // remains available to the explicit palette and comparison modes.
        CompressionMethod::Auto if should_try_palette(&analysis) => rules.candidate_budget.min(3),
        CompressionMethod::Auto => rules.candidate_budget.min(2),
        _ => rules.candidate_budget,
    };
    progress.report(ProgressEvent {
        stage: ProgressStage::Searching,
        candidate: Some(0),
        total: Some(candidate_budget),
    });
    'search: for colors in palette_sizes {
        for dithered in [false, true] {
            if candidates_tested >= candidate_budget {
                break 'search;
            }
            check_cancelled(cancellation)?;
            candidates_tested += 1;
            progress.report(ProgressEvent {
                stage: ProgressStage::Searching,
                candidate: Some(candidates_tested),
                total: Some(candidate_budget),
            });
            let palette_size = PaletteSize::try_from(colors)
                .map_err(|error| OptimizeError::Encode(error.to_string()))?;
            let pipeline = Pipeline::new()
                .palette_size(palette_size)
                .quantize_method(QuantizeMethod::kmeans());
            let pipeline = if dithered {
                pipeline.ditherer(FloydSteinberg::new())
            } else {
                pipeline.ditherer(None::<FloydSteinberg>)
            };
            let indexed = pipeline
                .input_image(quantette_image.as_ref())
                .output_srgb8_indexed_image();
            let bytes = encode_indexed_png(width, height, indexed.palette(), indexed.indices())?;
            let decoded = decode_png(&bytes)?;
            progress.report(ProgressEvent {
                stage: ProgressStage::Measuring,
                candidate: Some(candidates_tested),
                total: Some(candidate_budget),
            });
            let score = ssimulacra2_score(&reference, &decoded)?;
            if score < rules.ssimulacra2 {
                continue;
            }
            let butteraugli = butteraugli_score(&reference, &decoded)?;
            if butteraugli > rules.butteraugli {
                continue;
            }
            let optimized = smallest_oxipng(&bytes, options.limits)?;
            let optimized_decoded = decode_png(&optimized)?;
            let final_ssim = ssimulacra2_score(&reference, &optimized_decoded)?;
            let final_butteraugli = butteraugli_score(&reference, &optimized_decoded)?;
            if final_ssim < rules.ssimulacra2 || final_butteraugli > rules.butteraugli {
                continue;
            }
            let candidate = PngWinner {
                bytes: optimized,
                colors,
                dithered,
                ssimulacra2: final_ssim,
                butteraugli: final_butteraugli,
            };
            if winner
                .as_ref()
                .is_none_or(|best| candidate.bytes.len() < best.bytes.len())
            {
                winner = Some(candidate);
            }
        }
    }
    progress.report(ProgressEvent {
        stage: ProgressStage::Finalizing,
        candidate: None,
        total: None,
    });
    let Some(winner) = winner else {
        if options.method == CompressionMethod::Palette {
            return Ok(passthrough(
                input,
                width,
                height,
                analysis,
                candidates_tested,
                vec!["Żaden wariant palety nie przeszedł wybranego progu jakości; zachowano oryginał.".into()],
            ));
        }
        return optimize_lossless(
            input,
            width,
            height,
            analysis,
            vec!["Żaden wariant palety nie przeszedł obu progów; użyto bezstratnej optymalizacji PNG.".into()],
            candidates_tested,
            progress,
            cancellation,
            options.limits,
            None,
        );
    };
    let lossless = if matches!(
        options.method,
        CompressionMethod::Auto | CompressionMethod::Search
    ) {
        Some(smallest_oxipng(input, options.limits)?)
    } else {
        None
    };
    if lossless
        .as_ref()
        .is_some_and(|candidate| candidate.len() < winner.bytes.len())
    {
        return optimize_lossless(
            input,
            width,
            height,
            analysis,
            vec!["Bezstratny wariant PNG był mniejszy od wariantów palety.".into()],
            candidates_tested.saturating_add(1),
            progress,
            cancellation,
            options.limits,
            lossless,
        );
    }
    if winner.bytes.len() >= input.len() {
        return Ok(passthrough(
            input,
            width,
            height,
            analysis,
            candidates_tested,
            vec!["Oryginał jest mniejszy od wszystkich poprawnych kandydatów.".into()],
        ));
    }
    let optimized_size = winner.bytes.len();
    let saved_bytes = input.len() - optimized_size;
    Ok(OptimizationResult {
        output: winner.bytes,
        report: OptimizationReport {
            format: ImageFormat::Png,
            output_format: ImageFormat::Png,
            width,
            height,
            original_size: input.len(),
            optimized_size,
            saved_bytes,
            saved_percent: saved_bytes as f32 / input.len() as f32 * 100.0,
            metrics: QualityMetrics {
                ssimulacra2: Some(winner.ssimulacra2),
                butteraugli: Some(winner.butteraugli),
            },
            strategy: SelectedStrategy {
                encoder: "quantette + oxipng".into(),
                quality: None,
                chroma_subsampling: None,
                progressive: None,
                palette_colors: Some(winner.colors),
                dithering: Some(
                    if winner.dithered {
                        "floyd-steinberg"
                    } else {
                        "none"
                    }
                    .into(),
                ),
                lossless: false,
            },
            candidates_tested,
            processing_time_ms: 0.0,
            already_optimized: false,
            profile_set_version: PROFILE_SET_VERSION,
            warnings: Vec::new(),
            analysis,
        },
    })
}

fn should_try_palette(analysis: &ImageAnalysis) -> bool {
    analysis.estimated_colors <= 4_096
        && !matches!(analysis.kind, ContentKind::Photo)
        && analysis.noise < 0.08
}

#[allow(clippy::too_many_arguments)]
fn optimize_lossless(
    input: &[u8],
    width: u32,
    height: u32,
    analysis: ImageAnalysis,
    mut warnings: Vec<String>,
    candidates_tested: u16,
    progress: &dyn ProgressSink,
    cancellation: &dyn CancellationToken,
    limits: ResourceLimits,
    precomputed: Option<Vec<u8>>,
) -> Result<OptimizationResult, OptimizeError> {
    #[cfg(target_arch = "wasm32")]
    warnings.push(
        "Wersja przeglądarkowa ponownie kompresuje dane PNG bezstratnie w czystym Ruście.".into(),
    );
    check_cancelled(cancellation)?;
    progress.report(ProgressEvent {
        stage: ProgressStage::Finalizing,
        candidate: None,
        total: None,
    });
    let optimized = match precomputed {
        Some(bytes) => bytes,
        None => smallest_oxipng(input, limits)?,
    };
    if optimized.len() >= input.len() {
        warnings.push("PNG był już zoptymalizowany.".into());
        return Ok(passthrough(
            input,
            width,
            height,
            analysis,
            candidates_tested,
            warnings,
        ));
    }
    let before = image::load_from_memory_with_format(input, image::ImageFormat::Png)
        .map_err(|error| OptimizeError::Decode(error.to_string()))?;
    let after = image::load_from_memory_with_format(&optimized, image::ImageFormat::Png)
        .map_err(|error| OptimizeError::Decode(error.to_string()))?;
    let pixels_equal = if input.get(24) == Some(&16) {
        before.to_rgba16() == after.to_rgba16()
    } else {
        before.to_rgba8() == after.to_rgba8()
    };
    if before.dimensions() != after.dimensions() || !pixels_equal {
        return Err(OptimizeError::Encode(
            "lossless PNG verification failed; decoded pixels changed".into(),
        ));
    }
    let optimized_size = optimized.len();
    let saved_bytes = input.len() - optimized_size;
    Ok(OptimizationResult {
        output: optimized,
        report: OptimizationReport {
            format: ImageFormat::Png,
            output_format: ImageFormat::Png,
            width,
            height,
            original_size: input.len(),
            optimized_size,
            saved_bytes,
            saved_percent: saved_bytes as f32 / input.len() as f32 * 100.0,
            metrics: QualityMetrics {
                ssimulacra2: None,
                butteraugli: None,
            },
            strategy: SelectedStrategy {
                encoder: lossless_encoder_name().into(),
                quality: None,
                chroma_subsampling: None,
                progressive: None,
                palette_colors: None,
                dithering: None,
                lossless: true,
            },
            candidates_tested,
            processing_time_ms: 0.0,
            already_optimized: false,
            profile_set_version: PROFILE_SET_VERSION,
            warnings,
            analysis,
        },
    })
}

#[cfg(target_arch = "wasm32")]
fn lossless_encoder_name() -> &'static str {
    "pure-rust PNG zlib"
}

#[cfg(not(target_arch = "wasm32"))]
fn lossless_encoder_name() -> &'static str {
    "oxipng"
}

#[cfg(not(target_arch = "wasm32"))]
fn smallest_oxipng(input: &[u8], limits: ResourceLimits) -> Result<Vec<u8>, OptimizeError> {
    let mut variants = vec![input.to_vec()];
    if let Ok(bytes) = recompress_png_idat(input, limits) {
        variants.push(bytes);
    }
    for preset in [2, 4] {
        let mut options = oxipng::Options::from_preset(preset);
        options.optimize_alpha = false;
        options.scale_16 = false;
        options.max_decompressed_size = Some(limits.max_working_bytes as usize);
        if let Ok(bytes) = oxipng::optimize_from_memory(input, &options) {
            variants.push(bytes);
        }
    }
    variants
        .into_iter()
        .min_by_key(Vec::len)
        .ok_or_else(|| OptimizeError::Encode("OxiPNG produced no output".into()))
}

#[cfg(target_arch = "wasm32")]
fn smallest_oxipng(input: &[u8], _limits: ResourceLimits) -> Result<Vec<u8>, OptimizeError> {
    recompress_png_idat(input, _limits)
}

/// Re-encodes only the PNG's zlib stream. It preserves every non-IDAT chunk
/// byte-for-byte, including ICC, alpha and 16-bit metadata, while offering a
/// pure-Rust lossless fallback that can run in `wasm32-unknown-unknown`.
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
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::best());
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
        let mut writer = encoder
            .write_header()
            .map_err(|error| OptimizeError::Encode(error.to_string()))?;
        writer
            .write_image_data(indices)
            .map_err(|error| OptimizeError::Encode(error.to_string()))?;
    }
    Ok(output)
}

fn decode_png(bytes: &[u8]) -> Result<RgbaImage, OptimizeError> {
    image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
        .map(DynamicImage::into_rgba8)
        .map_err(|error| OptimizeError::Decode(error.to_string()))
}

fn palette_schedule(estimated: u32) -> Vec<u16> {
    let start = estimated.clamp(2, 256) as u16;
    let mut values = vec![start, 256, 192, 128, 96, 64, 48, 32, 24, 16, 8, 4, 2];
    values.sort_unstable_by_key(|value| value.abs_diff(start));
    values.dedup();
    values
}

fn has_color_management_chunks(input: &[u8]) -> bool {
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
        if matches!(kind, b"iCCP" | b"gAMA" | b"cHRM" | b"sRGB" | b"cICP") {
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
    candidates_tested: u16,
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
            metrics: QualityMetrics {
                ssimulacra2: None,
                butteraugli: None,
            },
            strategy: SelectedStrategy {
                encoder: "already-optimized".into(),
                quality: None,
                chroma_subsampling: None,
                progressive: None,
                palette_colors: None,
                dithering: None,
                lossless: true,
            },
            candidates_tested,
            processing_time_ms: 0.0,
            already_optimized: true,
            profile_set_version: PROFILE_SET_VERSION,
            warnings,
            analysis,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_limits_palette_budget_for_high_colour_photographs() {
        let photo = ImageAnalysis {
            kind: ContentKind::Photo,
            entropy: 7.2,
            estimated_colors: 120_000,
            edge_density: 0.2,
            noise: 0.12,
            flat_area_ratio: 0.1,
            has_alpha: false,
        };
        let graphic = ImageAnalysis {
            kind: ContentKind::Graphic,
            entropy: 2.1,
            estimated_colors: 48,
            edge_density: 0.1,
            noise: 0.01,
            flat_area_ratio: 0.8,
            has_alpha: false,
        };

        assert!(!should_try_palette(&photo));
        assert!(should_try_palette(&graphic));
    }

    #[test]
    fn lossless_recompression_shrinks_a_fast_rgba_png_without_changing_pixels() {
        let width = 256;
        let height = 256;
        let pixels = (0..width * height)
            .flat_map(|index| {
                let value = if (index / width + index % width) % 2 == 0 {
                    24
                } else {
                    228
                };
                [value, 160, 80, if index % 11 == 0 { 180 } else { 255 }]
            })
            .collect::<Vec<_>>();
        let mut input = Vec::new();
        {
            let mut encoder = png::Encoder::new(Cursor::new(&mut input), width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(png::Compression::Fast);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&pixels)
                .unwrap();
        }

        let output = recompress_png_idat(&input, ResourceLimits::default()).unwrap();

        assert!(output.len() < input.len());
        assert_eq!(decode_png(&input).unwrap(), decode_png(&output).unwrap());
    }
}
