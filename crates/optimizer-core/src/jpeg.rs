use image::{DynamicImage, RgbaImage, metadata::Orientation};
use mozjpeg_rs::{Encoder, Preset, Subsampling};

use crate::{
    check_cancelled, jpeg_metadata,
    metrics::{butteraugli_score, ssimulacra2_score},
    types::*,
};

struct Winner {
    bytes: Vec<u8>,
    quality: u8,
    subsampling: Subsampling,
    ssimulacra2: f64,
    butteraugli: Option<f64>,
    progressive: bool,
}

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
    if options.profile == CompressionProfile::Lossless {
        let warning = match options.metadata {
            MetadataPolicy::StripPrivate => {
                "JPEG Lossless pozostawiono bez zmian: bezpieczne usuwanie prywatnego EXIF przy zachowaniu orientacji wymaga transformacji współczynników."
            }
            MetadataPolicy::PreserveAll => {
                "JPEG Lossless zachowuje oryginalny strumień i komplet metadanych."
            }
        };
        return Ok(passthrough(
            input,
            width,
            height,
            analysis,
            vec![warning.into()],
            "jpeg-passthrough",
        ));
    }

    let rules = options
        .profile
        .rules(options.search_effort)
        .expect("lossy profile has rules");
    let metadata = jpeg_metadata::extract(input)?;
    let rgb = rgba_to_rgb(&reference);
    let subsampling_order = match analysis.kind {
        ContentKind::Photo => [Subsampling::S420, Subsampling::S422, Subsampling::S444],
        ContentKind::Graphic | ContentKind::Screenshot => {
            [Subsampling::S444, Subsampling::S422, Subsampling::S420]
        }
        ContentKind::Mixed => [Subsampling::S422, Subsampling::S444, Subsampling::S420],
    };
    let mut candidates_tested = 0_u16;
    let mut winner: Option<Winner> = None;
    let fast_auto =
        options.method == CompressionMethod::Auto && options.search_effort == SearchEffort::Auto;
    let candidate_budget = if fast_auto { 1 } else { rules.candidate_budget };

    progress.report(ProgressEvent {
        stage: ProgressStage::Searching,
        candidate: Some(0),
        total: Some(candidate_budget),
        variant: None,
    });
    for (configuration_index, subsampling) in subsampling_order.into_iter().enumerate() {
        let configuration_budget = if fast_auto {
            1
        } else {
            budget_for_configuration(candidate_budget, configuration_index as u16, 3)
        };
        for quality in quality_schedule(rules.start_quality, configuration_budget) {
            if candidates_tested >= candidate_budget {
                break;
            }
            check_cancelled(cancellation)?;
            candidates_tested += 1;
            progress.report(ProgressEvent {
                stage: ProgressStage::Searching,
                candidate: Some(candidates_tested),
                total: Some(candidate_budget),
                variant: Some(format!(
                    "JPEG Q{quality}, {}",
                    subsampling_name(subsampling)
                )),
            });
            observer.begin(OptimizationOperation::PngEncode);
            let bytes = encode(
                &rgb,
                width,
                height,
                quality,
                subsampling,
                false,
                &metadata,
                options.metadata,
                options.limits,
            )?;
            observer.end(OptimizationOperation::PngEncode);
            observer.begin(OptimizationOperation::CandidateDecode);
            let decoded = decode_candidate(&bytes)?;
            observer.end(OptimizationOperation::CandidateDecode);
            progress.report(ProgressEvent {
                stage: ProgressStage::Measuring,
                candidate: Some(candidates_tested),
                total: Some(rules.candidate_budget),
                variant: Some(format!(
                    "JPEG Q{quality}, {}",
                    subsampling_name(subsampling)
                )),
            });
            observer.begin(OptimizationOperation::Ssimulacra2);
            let score = ssimulacra2_score(&reference, &decoded)?;
            observer.end(OptimizationOperation::Ssimulacra2);
            if score < rules.ssimulacra2 {
                continue;
            }
            let butteraugli = if fast_auto {
                None
            } else {
                observer.begin(OptimizationOperation::Butteraugli);
                let value = butteraugli_score(&reference, &decoded)?;
                observer.end(OptimizationOperation::Butteraugli);
                if value > rules.butteraugli {
                    continue;
                }
                Some(value)
            };
            let mut candidate = Winner {
                bytes,
                quality,
                subsampling,
                ssimulacra2: score,
                butteraugli,
                progressive: false,
            };
            if !fast_auto {
                let progressive_bytes = encode(
                    &rgb,
                    width,
                    height,
                    quality,
                    subsampling,
                    true,
                    &metadata,
                    options.metadata,
                    options.limits,
                )?;
                if progressive_bytes.len() < candidate.bytes.len() {
                    let progressive_decoded = decode_candidate(&progressive_bytes)?;
                    let progressive_ssim = ssimulacra2_score(&reference, &progressive_decoded)?;
                    let progressive_butteraugli =
                        butteraugli_score(&reference, &progressive_decoded)?;
                    if progressive_ssim >= rules.ssimulacra2
                        && progressive_butteraugli <= rules.butteraugli
                    {
                        candidate.bytes = progressive_bytes;
                        candidate.ssimulacra2 = progressive_ssim;
                        candidate.butteraugli = Some(progressive_butteraugli);
                        candidate.progressive = true;
                    }
                }
            }
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
        return Ok(passthrough(
            input,
            width,
            height,
            analysis,
            vec!["Nie znaleziono mniejszego JPEG spełniającego oba progi jakości.".into()],
            "no-smaller-safe-candidate",
        ));
    };
    if winner.bytes.len() >= input.len() {
        return Ok(passthrough(
            input,
            width,
            height,
            analysis,
            vec!["Nie znaleziono mniejszego JPEG spełniającego oba progi jakości.".into()],
            "no-smaller-safe-candidate",
        ));
    }

    let optimized_size = winner.bytes.len();
    let saved_bytes = input.len() - optimized_size;
    Ok(OptimizationResult {
        output: winner.bytes,
        report: OptimizationReport {
            format: ImageFormat::Jpeg,
            output_format: ImageFormat::Jpeg,
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
                encoder: "mozjpeg-rs".into(),
                quality: Some(winner.quality),
                chroma_subsampling: Some(subsampling_name(winner.subsampling).into()),
                progressive: Some(winner.progressive),
                palette_colors: None,
                dithering: None,
                quality_guard: Some(
                    if fast_auto {
                        "ssimulacra2"
                    } else {
                        "ssimulacra2+butteraugli"
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

fn quality_schedule(start: u8, budget: u16) -> Vec<u8> {
    // Coarse samples establish the viable range. Detailed mode receives a larger
    // budget and therefore also evaluates the neighbouring values. We deliberately
    // retain every value: encoder results and perceptual metrics are not monotonic.
    let coarse = [0, 16, 32, 48, 64, 72];
    let mut values = coarse
        .into_iter()
        .map(|delta| start.saturating_sub(delta).max(20))
        .collect::<Vec<_>>();
    values.dedup();
    if values.len() >= budget as usize {
        values.truncate(budget as usize);
        return values;
    }
    for center in coarse.into_iter().skip(1) {
        let center = start.saturating_sub(center).max(20);
        for delta in [4, 8] {
            values.push(center.saturating_add(delta).min(start));
            values.push(center.saturating_sub(delta).max(20));
        }
    }
    values.dedup();
    values.truncate(budget as usize);
    values
}

fn budget_for_configuration(total: u16, index: u16, configurations: u16) -> u16 {
    total / configurations + u16::from(index < total % configurations)
}

#[allow(clippy::too_many_arguments)]
fn encode(
    rgb: &[u8],
    width: u32,
    height: u32,
    quality: u8,
    subsampling: Subsampling,
    progressive: bool,
    metadata: &jpeg_metadata::JpegMetadata,
    metadata_policy: MetadataPolicy,
    limits: ResourceLimits,
) -> Result<Vec<u8>, OptimizeError> {
    let preset = if progressive {
        Preset::ProgressiveSmallest
    } else {
        Preset::BaselineBalanced
    };
    let mut encoder = Encoder::new(preset)
        .quality(quality)
        .progressive(progressive)
        .subsampling(subsampling)
        .optimize_huffman(true)
        .limits(
            mozjpeg_rs::Limits::default()
                .max_pixel_count(limits.max_pixels)
                .max_alloc_bytes(limits.max_working_bytes as usize)
                .max_icc_profile_bytes(4 * 1024 * 1024),
        );
    if let Some(icc) = &metadata.icc {
        encoder = encoder.icc_profile(icc.clone());
    }
    if metadata_policy == MetadataPolicy::PreserveAll
        && let Some(exif) = &metadata.exif
    {
        let mut normalized = exif.clone();
        let _ = Orientation::remove_from_exif_chunk(&mut normalized);
        encoder = encoder.exif_data(normalized);
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

fn decode_candidate(bytes: &[u8]) -> Result<RgbaImage, OptimizeError> {
    image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
        .map(DynamicImage::into_rgba8)
        .map_err(|error| OptimizeError::Decode(error.to_string()))
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
    encoder: &str,
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
            metrics: QualityMetrics {
                ssimulacra2: None,
                butteraugli: None,
            },
            strategy: SelectedStrategy {
                encoder: encoder.into(),
                quality: None,
                chroma_subsampling: None,
                progressive: None,
                palette_colors: None,
                dithering: None,
                quality_guard: None,
                lossless: true,
            },
            candidates_tested: 0,
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
    fn candidate_budget_is_shared_across_configurations() {
        for total in [8, 12, 16] {
            let allocated: u16 = (0..3)
                .map(|index| budget_for_configuration(total, index, 3))
                .sum();
            assert_eq!(allocated, total);
        }
    }

    #[test]
    fn non_monotonic_samples_are_not_skipped() {
        let schedule = quality_schedule(92, 12);
        assert!(schedule.contains(&76));
        assert!(schedule.contains(&80));
        // Q76 can fail while Q80 passes; both remain explicit samples rather
        // than relying on monotonicity between encoder results.
        let samples = [(92, true), (80, true), (76, false), (84, true)];
        assert_eq!(samples.iter().filter(|(_, passes)| *passes).count(), 3);
    }
}
