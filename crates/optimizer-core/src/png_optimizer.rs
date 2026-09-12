use std::io::Cursor;

use image::{DynamicImage, GenericImageView, RgbImage, RgbaImage};
use quantette::{ImageBuf, PaletteSize, Pipeline, QuantizeMethod, dither::FloydSteinberg};

use crate::{
    check_cancelled,
    metrics::{butteraugli_score, ssimulacra2_score},
    types::*,
};

struct PngCandidate {
    bytes: Vec<u8>,
    decoded: RgbaImage,
    colors: u16,
    dithered: bool,
    ssimulacra2: f64,
}

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
    let force_lossless = options.profile == CompressionProfile::Lossless
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
        return optimize_lossless(
            input,
            width,
            height,
            analysis,
            warnings,
            progress,
            cancellation,
            options.limits,
        );
    }

    let rules = options.profile.rules().expect("lossy profile has rules");
    let rgb = RgbImage::from_fn(width, height, |x, y| {
        let pixel = reference.get_pixel(x, y);
        image::Rgb([pixel[0], pixel[1], pixel[2]])
    });
    let quantette_image =
        ImageBuf::try_from(rgb).map_err(|error| OptimizeError::Encode(error.to_string()))?;
    let palette_sizes = palette_schedule(analysis.estimated_colors);
    let mut candidates = Vec::new();
    let mut candidates_tested = 0_u16;

    progress.report(ProgressEvent {
        stage: ProgressStage::Searching,
        candidate: Some(0),
        total: Some(rules.candidate_budget),
    });
    'search: for colors in palette_sizes {
        for dithered in [false, true] {
            if candidates_tested >= rules.candidate_budget {
                break 'search;
            }
            check_cancelled(cancellation)?;
            candidates_tested += 1;
            progress.report(ProgressEvent {
                stage: ProgressStage::Searching,
                candidate: Some(candidates_tested),
                total: Some(rules.candidate_budget),
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
                total: Some(rules.candidate_budget),
            });
            let score = ssimulacra2_score(&reference, &decoded)?;
            if score >= rules.ssimulacra2 {
                candidates.push(PngCandidate {
                    bytes,
                    decoded,
                    colors,
                    dithered,
                    ssimulacra2: score,
                });
            }
        }
    }
    candidates.sort_by_key(|candidate| candidate.bytes.len());
    candidates.truncate(3);

    progress.report(ProgressEvent {
        stage: ProgressStage::Finalizing,
        candidate: None,
        total: None,
    });
    let mut finalists = Vec::new();
    for mut candidate in candidates {
        check_cancelled(cancellation)?;
        let butteraugli = butteraugli_score(&reference, &candidate.decoded)?;
        if butteraugli > rules.butteraugli {
            continue;
        }
        candidate.bytes = smallest_oxipng(&candidate.bytes, options.limits)?;
        candidate.decoded = decode_png(&candidate.bytes)?;
        let final_ssim = ssimulacra2_score(&reference, &candidate.decoded)?;
        let final_butteraugli = butteraugli_score(&reference, &candidate.decoded)?;
        if final_ssim >= rules.ssimulacra2 && final_butteraugli <= rules.butteraugli {
            candidate.ssimulacra2 = final_ssim;
            finalists.push((candidate, final_butteraugli));
        }
    }
    finalists.sort_by_key(|(candidate, _)| candidate.bytes.len());
    let Some((winner, butteraugli)) = finalists.into_iter().next() else {
        return optimize_lossless(
            input,
            width,
            height,
            analysis,
            vec!["Żaden wariant palety nie przeszedł obu progów; użyto OxiPNG lossless.".into()],
            progress,
            cancellation,
            options.limits,
        );
    };
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
            width,
            height,
            original_size: input.len(),
            optimized_size,
            saved_bytes,
            saved_percent: saved_bytes as f32 / input.len() as f32 * 100.0,
            metrics: QualityMetrics {
                ssimulacra2: Some(winner.ssimulacra2),
                butteraugli: Some(butteraugli),
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
) -> Result<OptimizationResult, OptimizeError> {
    #[cfg(target_arch = "wasm32")]
    warnings.push(
        "Wersja przeglądarkowa zachowuje ten PNG bez zmian; finalizacja OxiPNG jest dostępna w CLI."
            .into(),
    );
    check_cancelled(cancellation)?;
    progress.report(ProgressEvent {
        stage: ProgressStage::Finalizing,
        candidate: None,
        total: Some(2),
    });
    let optimized = smallest_oxipng(input, limits)?;
    if optimized.len() >= input.len() {
        warnings.push("PNG był już zoptymalizowany.".into());
        return Ok(passthrough(input, width, height, analysis, 2, warnings));
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
                encoder: "oxipng".into(),
                quality: None,
                chroma_subsampling: None,
                progressive: None,
                palette_colors: None,
                dithering: None,
                lossless: true,
            },
            candidates_tested: 2,
            processing_time_ms: 0.0,
            already_optimized: false,
            profile_set_version: PROFILE_SET_VERSION,
            warnings,
            analysis,
        },
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn smallest_oxipng(input: &[u8], limits: ResourceLimits) -> Result<Vec<u8>, OptimizeError> {
    let mut variants = vec![input.to_vec()];
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
    Ok(input.to_vec())
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
