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
    butteraugli: Option<f64>,
    quality_guard: &'static str,
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
    observer: &dyn OptimizationObserver,
) -> Result<OptimizationResult, OptimizeError> {
    let bit_depth = input.get(24).copied().unwrap_or(8);
    let sensitive_chunks = has_sensitive_png_chunks(input);
    // Measurements on several PNG sizes showed palette/metric work needs far
    // more than the old 20 B/pixel admission estimate. Keep a 20% margin.
    let fast_working_bytes = u64::from(width)
        .saturating_mul(u64::from(height))
        .saturating_mul(192)
        .saturating_add(input.len() as u64 * 2);
    let force_lossless = options.method == CompressionMethod::Lossless
        || options.profile == CompressionProfile::Lossless
        || bit_depth == 16
        || analysis.has_alpha
        || sensitive_chunks
        || fast_working_bytes > options.limits.max_working_bytes;
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
            if sensitive_chunks {
                warnings.push(
                    "Zachowano osadzony profil i informacje o kolorze przez ścieżkę lossless."
                        .into(),
                );
            }
            if fast_working_bytes > options.limits.max_working_bytes {
                warnings.push("Paleta i pomiary przekroczyłyby limit pamięci; użyto bezpiecznej ścieżki lossless.".into());
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
            observer,
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
    let fast_auto = options.method == CompressionMethod::Auto
        && options.search_effort == SearchEffort::Auto
        && options.profile == CompressionProfile::MaximumCompression
        && !matches!(analysis.kind, ContentKind::Photo)
        && analysis.noise < 0.08;
    let manual_palette =
        options.method == CompressionMethod::Palette && options.palette_colors.is_some();
    let palette_sizes = if manual_palette {
        vec![
            options
                .palette_colors
                .expect("manual palette has a colour count"),
        ]
    } else if fast_auto {
        vec![256]
    } else {
        palette_schedule(analysis.estimated_colors)
    };
    let mut candidates_tested = 0_u16;
    let mut winner: Option<PngWinner> = None;

    let candidate_budget = if fast_auto || manual_palette {
        1
    } else {
        match options.method {
            // Smart evaluates only the closest palette candidates. The full budget
            // remains available to the explicit palette and comparison modes.
            CompressionMethod::Auto if should_try_palette(&analysis) => {
                rules.candidate_budget.min(3)
            }
            CompressionMethod::Auto => rules.candidate_budget.min(2),
            _ => rules.candidate_budget,
        }
    };
    progress.report(ProgressEvent {
        stage: ProgressStage::Searching,
        candidate: Some(0),
        total: Some(candidate_budget),
        variant: None,
    });
    'search: for colors in palette_sizes {
        for dithered in if manual_palette {
            vec![options.palette_dithering == PaletteDithering::FloydSteinberg]
        } else if fast_auto {
            vec![false]
        } else {
            vec![false, true]
        } {
            if candidates_tested >= candidate_budget {
                break 'search;
            }
            check_cancelled(cancellation)?;
            candidates_tested += 1;
            progress.report(ProgressEvent {
                stage: ProgressStage::Searching,
                candidate: Some(candidates_tested),
                total: Some(candidate_budget),
                variant: Some(palette_variant_name(colors, dithered)),
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
            observer.begin(if dithered {
                OptimizationOperation::Dithering
            } else {
                OptimizationOperation::PaletteBuild
            });
            let indexed = pipeline
                .input_image(quantette_image.as_ref())
                .output_srgb8_indexed_image();
            observer.end(if dithered {
                OptimizationOperation::Dithering
            } else {
                OptimizationOperation::PaletteBuild
            });
            observer.begin(OptimizationOperation::PngEncode);
            let bytes = encode_indexed_png(width, height, indexed.palette(), indexed.indices())?;
            observer.end(OptimizationOperation::PngEncode);
            observer.begin(OptimizationOperation::CandidateDecode);
            let decoded = decode_png(&bytes)?;
            observer.end(OptimizationOperation::CandidateDecode);
            progress.report(ProgressEvent {
                stage: ProgressStage::Measuring,
                candidate: Some(candidates_tested),
                total: Some(candidate_budget),
                variant: Some(palette_variant_name(colors, dithered)),
            });
            observer.begin(OptimizationOperation::Ssimulacra2);
            let score = ssimulacra2_score(&reference, &decoded)?;
            observer.end(OptimizationOperation::Ssimulacra2);
            if score < rules.png_palette_ssimulacra2 {
                continue;
            }
            let butteraugli = if fast_auto {
                None
            } else {
                observer.begin(OptimizationOperation::Butteraugli);
                let score = butteraugli_score(&reference, &decoded)?;
                observer.end(OptimizationOperation::Butteraugli);
                if score > rules.png_palette_butteraugli {
                    continue;
                }
                Some(score)
            };
            // Recompression cannot change decoded pixels. Preserve the metric
            // above rather than decoding and measuring the candidate again.
            observer.begin(OptimizationOperation::FinalRecompress);
            let optimized = smallest_oxipng(&bytes, options.limits)?;
            observer.end(OptimizationOperation::FinalRecompress);
            let candidate = PngWinner {
                bytes: optimized,
                colors,
                dithered,
                ssimulacra2: score,
                butteraugli,
                quality_guard: if fast_auto {
                    "ssimulacra2"
                } else {
                    "ssimulacra2+butteraugli"
                },
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
        variant: None,
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
            observer,
        );
    };
    let lossless = if !fast_auto
        && matches!(
            options.method,
            CompressionMethod::Auto | CompressionMethod::Search
        ) {
        observer.begin(OptimizationOperation::FinalRecompress);
        let candidate = smallest_oxipng(input, options.limits)?;
        observer.end(OptimizationOperation::FinalRecompress);
        Some(candidate)
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
            observer,
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
                butteraugli: winner.butteraugli,
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
                quality_guard: Some(winner.quality_guard.into()),
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
    observer: &dyn OptimizationObserver,
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
        variant: None,
    });
    let optimized = match precomputed {
        Some(bytes) => bytes,
        None => {
            observer.begin(OptimizationOperation::FinalRecompress);
            let bytes = smallest_oxipng(input, limits)?;
            observer.end(OptimizationOperation::FinalRecompress);
            bytes
        }
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
    observer.begin(OptimizationOperation::LosslessVerification);
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
    observer.end(OptimizationOperation::LosslessVerification);
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
                quality_guard: None,
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
        let mut writer = encoder
            .write_header()
            .map_err(|error| OptimizeError::Encode(error.to_string()))?;
        writer
            .write_image_data(&indices)
            .map_err(|error| OptimizeError::Encode(error.to_string()))?;
    }
    Ok(output)
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

fn palette_variant_name(colors: u16, dithered: bool) -> String {
    if dithered {
        format!("Paleta {colors} kolorów z ditheringiem")
    } else {
        format!("Paleta {colors} kolorów bez ditheringu")
    }
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
                quality_guard: None,
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
    fn strongest_profile_has_a_separate_png_palette_quality_envelope() {
        let rules = CompressionProfile::MaximumCompression
            .rules(SearchEffort::Auto)
            .unwrap();

        assert_eq!(rules.ssimulacra2, 93.0);
        assert_eq!(rules.butteraugli, 2.0);
        assert_eq!(rules.png_palette_ssimulacra2, 77.0);
        assert_eq!(rules.png_palette_butteraugli, 4.5);
    }

    #[test]
    fn strongest_profile_reduces_a_high_color_png_to_a_palette() {
        let width = 256;
        let height = 192;
        let reference = RgbaImage::from_fn(width, height, |x, y| {
            let field = ((x / 64 + y / 48) % 3) as u8;
            let noise = x
                .wrapping_mul(73_856_093)
                .wrapping_add(y.wrapping_mul(19_349_663))
                .rotate_left((x % 17) + 1);
            image::Rgba([
                72_u8
                    .saturating_add(field * 38)
                    .saturating_add((noise & 7) as u8),
                94_u8
                    .saturating_add(field * 28)
                    .saturating_add(((noise >> 3) & 7) as u8),
                54_u8
                    .saturating_add(field * 19)
                    .saturating_add(((noise >> 6) & 7) as u8),
                255,
            ])
        });
        let mut input = Vec::new();
        {
            let mut encoder = png::Encoder::new(Cursor::new(&mut input), width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_compression(png::Compression::Fast);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(reference.as_raw())
                .unwrap();
        }
        let analysis = crate::analysis::analyze(&reference);
        let options = OptimizeOptions {
            profile: CompressionProfile::MaximumCompression,
            ..OptimizeOptions::default()
        };

        let result = optimize_png(
            &input,
            reference,
            width,
            height,
            analysis,
            options,
            &NoProgress,
            &NeverCancelled,
            &NoObserver,
        )
        .unwrap();

        assert!(!result.report.strategy.lossless);
        assert_eq!(result.report.strategy.palette_colors, Some(256));
        assert_eq!(result.report.strategy.dithering.as_deref(), Some("none"));
        assert_eq!(
            result.report.strategy.quality_guard.as_deref(),
            Some("ssimulacra2")
        );
        assert_eq!(result.report.candidates_tested, 1);
        assert!(result.report.saved_percent > 40.0);
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
    fn sensitive_chunks_include_orientation_metadata() {
        let mut input = b"\x89PNG\r\n\x1a\n".to_vec();
        input.extend_from_slice(&0_u32.to_be_bytes());
        input.extend_from_slice(b"eXIf");
        input.extend_from_slice(&[0; 4]);
        assert!(has_sensitive_png_chunks(&input));
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
