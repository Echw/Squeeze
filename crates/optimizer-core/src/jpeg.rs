use image::{DynamicImage, RgbaImage, metadata::Orientation};
use mozjpeg_rs::{Encoder, Preset, Subsampling};

use crate::{
    check_cancelled, jpeg_metadata,
    metrics::{butteraugli_score, ssimulacra2_score},
    types::*,
};

struct Candidate {
    bytes: Vec<u8>,
    decoded: RgbaImage,
    quality: u8,
    subsampling: Subsampling,
    ssimulacra2: f64,
    progressive: bool,
}

pub(crate) fn optimize_jpeg(
    input: &[u8],
    reference: RgbaImage,
    width: u32,
    height: u32,
    analysis: ImageAnalysis,
    options: OptimizeOptions,
    progress: &dyn ProgressSink,
    cancellation: &dyn CancellationToken,
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

    let rules = options.profile.rules().expect("lossy profile has rules");
    let metadata = jpeg_metadata::extract(input)?;
    let rgb = rgba_to_rgb(&reference);
    let subsampling_order = match analysis.kind {
        ContentKind::Photo => [Subsampling::S420, Subsampling::S422, Subsampling::S444],
        ContentKind::Graphic | ContentKind::Screenshot => {
            [Subsampling::S444, Subsampling::S422, Subsampling::S420]
        }
        ContentKind::Mixed => [Subsampling::S422, Subsampling::S444, Subsampling::S420],
    };
    let qualities = quality_schedule(rules.start_quality);
    let mut candidates_tested = 0_u16;
    let mut passing = Vec::new();

    progress.report(ProgressEvent {
        stage: ProgressStage::Searching,
        candidate: Some(0),
        total: Some(rules.candidate_budget),
    });
    for (configuration_index, subsampling) in subsampling_order.into_iter().enumerate() {
        let configuration_budget =
            budget_for_configuration(rules.candidate_budget, configuration_index as u16, 3);
        for quality in qualities
            .iter()
            .copied()
            .take(configuration_budget as usize)
        {
            if candidates_tested >= rules.candidate_budget {
                break;
            }
            check_cancelled(cancellation)?;
            candidates_tested += 1;
            progress.report(ProgressEvent {
                stage: ProgressStage::Searching,
                candidate: Some(candidates_tested),
                total: Some(rules.candidate_budget),
            });
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
            let decoded = decode_candidate(&bytes)?;
            progress.report(ProgressEvent {
                stage: ProgressStage::Measuring,
                candidate: Some(candidates_tested),
                total: Some(rules.candidate_budget),
            });
            let score = ssimulacra2_score(&reference, &decoded)?;
            if score >= rules.ssimulacra2 {
                passing.push(Candidate {
                    bytes,
                    decoded,
                    quality,
                    subsampling,
                    ssimulacra2: score,
                    progressive: false,
                });
            }
        }
    }

    if passing.is_empty() {
        return Ok(passthrough(
            input,
            width,
            height,
            analysis,
            vec!["Żaden kandydat JPEG nie przeszedł progu SSIMULACRA2.".into()],
            "already-optimized",
        ));
    }

    passing.sort_by_key(|candidate| candidate.bytes.len());
    passing.truncate(3);
    progress.report(ProgressEvent {
        stage: ProgressStage::Finalizing,
        candidate: None,
        total: None,
    });

    let mut finalists = Vec::new();
    for mut candidate in passing {
        check_cancelled(cancellation)?;
        let butteraugli = butteraugli_score(&reference, &candidate.decoded)?;
        if butteraugli > rules.butteraugli {
            continue;
        }
        let progressive_bytes = encode(
            &rgb,
            width,
            height,
            candidate.quality,
            candidate.subsampling,
            true,
            &metadata,
            options.metadata,
            options.limits,
        )?;
        if progressive_bytes.len() < candidate.bytes.len() {
            let progressive_decoded = decode_candidate(&progressive_bytes)?;
            let progressive_ssim = ssimulacra2_score(&reference, &progressive_decoded)?;
            let progressive_butteraugli = butteraugli_score(&reference, &progressive_decoded)?;
            if progressive_ssim >= rules.ssimulacra2 && progressive_butteraugli <= rules.butteraugli
            {
                candidate.bytes = progressive_bytes;
                candidate.decoded = progressive_decoded;
                candidate.ssimulacra2 = progressive_ssim;
                candidate.progressive = true;
                finalists.push((candidate, progressive_butteraugli));
                continue;
            }
        }
        finalists.push((candidate, butteraugli));
    }
    finalists.sort_by_key(|(candidate, _)| candidate.bytes.len());
    let Some((winner, butteraugli)) = finalists.into_iter().next() else {
        return Ok(passthrough(
            input,
            width,
            height,
            analysis,
            vec!["Finaliści JPEG nie przeszli guardraila Butteraugli.".into()],
            "already-optimized",
        ));
    };
    if winner.bytes.len() >= input.len() {
        return Ok(passthrough(
            input,
            width,
            height,
            analysis,
            vec!["Oryginał jest mniejszy od wszystkich poprawnych kandydatów.".into()],
            "already-optimized",
        ));
    }

    let optimized_size = winner.bytes.len();
    let saved_bytes = input.len() - optimized_size;
    Ok(OptimizationResult {
        output: winner.bytes,
        report: OptimizationReport {
            format: ImageFormat::Jpeg,
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
                encoder: "mozjpeg-rs".into(),
                quality: Some(winner.quality),
                chroma_subsampling: Some(subsampling_name(winner.subsampling).into()),
                progressive: Some(winner.progressive),
                palette_colors: None,
                dithering: None,
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

fn quality_schedule(start: u8) -> Vec<u8> {
    let mut values = vec![start];
    for delta in [12, 24, 18, 15, 9, 6, 3, 1] {
        values.push(start.saturating_sub(delta).max(20));
    }
    values.dedup();
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
    if metadata_policy == MetadataPolicy::PreserveAll {
        if let Some(exif) = &metadata.exif {
            let mut normalized = exif.clone();
            let _ = Orientation::remove_from_exif_chunk(&mut normalized);
            encoder = encoder.exif_data(normalized);
        }
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
        let schedule = quality_schedule(92);
        assert_eq!(&schedule[..4], &[92, 80, 68, 74]);
        // Q68 can fail while Q74 passes; both remain explicit samples rather
        // than relying on monotonicity between encoder results.
        let samples = [(92, true), (80, true), (68, false), (74, true)];
        assert_eq!(samples.iter().filter(|(_, passes)| *passes).count(), 3);
    }
}
