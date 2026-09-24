use std::{
    collections::HashMap,
    io::{Cursor, Read, Write},
};

use crc32fast::Hasher;
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use image::{GenericImageView, RgbImage, RgbaImage};
use quantette::{ImageBuf, PaletteSize, Pipeline, QuantizeMethod, dither::FloydSteinberg};

use crate::{check_cancelled, types::*};

const PALETTE_COLORS: u16 = 256;
const MAX_PALETTE_MEAN_DELTA: f32 = 2.5;
// A transparent canvas lets a sizeable graphic remain inexpensive in memory:
// the palette training is bounded by distinct RGBA values, not pixel count.
// Six megapixels covers common high-resolution logos and gradients without
// attempting unbounded photo-sized alpha quantization in the browser.
const MAX_ALPHA_PALETTE_PIXELS: u32 = 6_000_000;
const MAX_ALPHA_PALETTE_SAMPLES: usize = 100_000;
const MIN_TRANSPARENT_PIXEL_RATIO: f32 = 0.25;
const MAX_ALPHA_COMPOSITE_MEAN_DELTA: f32 = 1.25;
const ALPHA_PALETTE_KMEANS_ROUNDS: usize = 16;
// A very small perceptual curve gives the palette more precision where a
// translucent colour is most visible over a bright surface. The same palette
// remains checked against the actual black-and-white compositing error below.
const ALPHA_PALETTE_COLOR_GAMMA: f64 = 1.15;
const SQRT_3: f64 = 1.732_050_807_568_877_2;
// Maps and dense interface captures can have far more distinct colours than
// ordinary illustrations while still quantizing cleanly. The palette guard
// below decides their quality; this cap only keeps the one-pass encoder
// bounded on genuinely high-colour inputs.
const MAX_PALETTE_ESTIMATED_COLORS: u32 = 250_000;

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
    let srgb = source_srgb_intent(input);
    if bit_depth == 16 || sensitive_chunks {
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

    if analysis.has_alpha {
        if let Some(indexed) = exact_rgba_palette(&reference) {
            return optimize_exact_alpha_palette(
                input,
                width,
                height,
                analysis,
                indexed,
                srgb,
                progress,
                cancellation,
                observer,
            );
        }
        if is_alpha_palette_candidate(&reference) {
            check_cancelled(cancellation)?;
            progress.report(ProgressEvent {
                stage: ProgressStage::Compressing,
            });
            observer.begin(OptimizationOperation::PaletteBuild);
            let indexed = alpha_aware_palette(&reference, alpha_palette_size(&analysis));
            observer.end(OptimizationOperation::PaletteBuild);
            if let Some(indexed) = indexed {
                return optimize_alpha_palette(
                    input,
                    width,
                    height,
                    analysis,
                    indexed,
                    srgb,
                    progress,
                    cancellation,
                    observer,
                );
            }
        }
        return optimize_lossless(
            input,
            width,
            height,
            analysis,
            lossless_warnings(bit_depth, true, false),
            progress,
            cancellation,
            options.limits,
            observer,
        );
    }

    if !is_palette_candidate(&analysis) {
        let warnings = lossless_warnings(bit_depth, false, false);
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
        // Quantette enables Floyd-Steinberg by default. The basic path needs
        // one fast, deterministic palette pass without texture-like noise.
        .ditherer(None::<FloydSteinberg>)
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
    let output = encode_indexed_png(width, height, indexed.palette(), indexed.indices(), srgb)?;
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
                palette_colors: Some(indexed.palette().len() as u16),
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
        && analysis.estimated_colors <= MAX_PALETTE_ESTIMATED_COLORS
}

struct ExactRgbaPalette {
    colors: Vec<[u8; 4]>,
    indices: Vec<u8>,
}

#[derive(Clone, Copy)]
struct AlphaSample {
    color: [u8; 4],
    count: u32,
}

#[derive(Clone)]
struct PcaNode {
    start: usize,
    end: usize,
    mean: [f64; 4],
    axis: [f64; 4],
    variance: f64,
}

struct AlphaPalette {
    colors: Vec<[u8; 4]>,
    indices: Vec<u8>,
}

/// Finds a palette that is bit-for-bit lossless after PNG decoding. This is
/// intentionally limited to 256 distinct RGBA tuples, which is the PNG
/// indexed-colour limit. Complex alpha stays on the conservative lossless
/// path instead of approximating semi-transparent edges.
fn exact_rgba_palette(reference: &RgbaImage) -> Option<ExactRgbaPalette> {
    let mut lookup = HashMap::<[u8; 4], u8>::new();
    let mut colors = Vec::new();
    let mut indices = Vec::with_capacity(reference.as_raw().len() / 4);

    for pixel in reference.pixels() {
        let color = pixel.0;
        let index = if let Some(&index) = lookup.get(&color) {
            index
        } else {
            let index = u8::try_from(colors.len()).ok()?;
            colors.push(color);
            lookup.insert(color, index);
            index
        };
        indices.push(index);
    }

    Some(ExactRgbaPalette { colors, indices })
}

/// A palette path reserved for images with a sizeable fully-transparent canvas
/// and an alpha-bearing foreground. It measures RGBA candidates in a space
/// where alpha is measured against black and white backgrounds, with a small
/// perceptual curve on RGB. This keeps soft edges from being treated as
/// ordinary four-channel colours while preserving more detail over white.
fn is_alpha_palette_candidate(reference: &RgbaImage) -> bool {
    let pixels = reference.width().saturating_mul(reference.height());
    if pixels == 0 || pixels > MAX_ALPHA_PALETTE_PIXELS {
        return false;
    }
    let mut transparent = 0_u32;
    for pixel in reference.pixels() {
        if pixel[3] == 0 {
            transparent += 1;
        }
    }
    transparent as f32 / pixels as f32 >= MIN_TRANSPARENT_PIXEL_RATIO
}

/// A flat logo with a very small colour distribution needs fewer entries than
/// a gradient or a soft illustration. This is an analysis-only decision made
/// before the one palette build; it never creates alternative encodings.
fn alpha_palette_size(analysis: &ImageAnalysis) -> usize {
    if matches!(analysis.kind, ContentKind::Graphic)
        && analysis.entropy <= 1.0
        && analysis.flat_area_ratio >= 0.98
    {
        20
    } else {
        PALETTE_COLORS as usize
    }
}

fn alpha_aware_palette(reference: &RgbaImage, palette_size: usize) -> Option<AlphaPalette> {
    let mut histogram = std::collections::BTreeMap::<[u8; 4], u32>::new();
    for pixel in reference.pixels() {
        let color = normalized_alpha_color(pixel.0);
        *histogram.entry(color).or_default() += 1;
    }
    if histogram.len() <= PALETTE_COLORS as usize || histogram.len() > MAX_ALPHA_PALETTE_SAMPLES {
        return None;
    }
    let samples = histogram
        .into_iter()
        .map(|(color, count)| AlphaSample { color, count })
        .collect::<Vec<_>>();
    let mut centers = alpha_pca_seed(&samples, palette_size);
    if centers.len() < 2 {
        return None;
    }

    for _ in 0..ALPHA_PALETTE_KMEANS_ROUNDS {
        let mut sums = vec![[0.0_f64; 4]; centers.len()];
        let mut counts = vec![0_u64; centers.len()];
        for sample in &samples {
            let point = alpha_dual_point(sample.color);
            let index = nearest_alpha_color(point, &centers);
            for (channel, value) in point.iter().enumerate() {
                sums[index][channel] += value * f64::from(sample.count);
            }
            counts[index] += u64::from(sample.count);
        }
        for index in 0..centers.len() {
            if counts[index] == 0 {
                continue;
            }
            let mean = sums[index].map(|value| value / counts[index] as f64);
            // PNG palette entries are bytes.  Quantizing after every round
            // keeps the training metric identical to the emitted image.
            centers[index] = alpha_dual_point(alpha_dual_color(mean));
        }
    }

    let colors = centers
        .into_iter()
        .map(alpha_dual_color)
        .collect::<Vec<_>>();
    let palette_points = colors
        .iter()
        .copied()
        .map(alpha_dual_point)
        .collect::<Vec<_>>();
    let mut color_indices = std::collections::BTreeMap::<[u8; 4], u8>::new();
    for sample in &samples {
        color_indices.insert(
            sample.color,
            nearest_alpha_color(alpha_dual_point(sample.color), &palette_points) as u8,
        );
    }
    let indices = reference
        .pixels()
        .map(|pixel| color_indices[&normalized_alpha_color(pixel.0)])
        .collect::<Vec<_>>();

    if alpha_composite_mean_delta(reference, &colors, &indices) > MAX_ALPHA_COMPOSITE_MEAN_DELTA {
        return None;
    }
    Some(AlphaPalette { colors, indices })
}

fn normalized_alpha_color(mut color: [u8; 4]) -> [u8; 4] {
    if color[3] == 0 {
        color[..3].fill(0);
    }
    color
}

fn alpha_pca_seed(samples: &[AlphaSample], colors: usize) -> Vec<[f64; 4]> {
    let mut order = (0..samples.len()).collect::<Vec<_>>();
    let mut nodes = vec![alpha_pca_node(samples, &order, 0, order.len())];
    while nodes.len() < colors {
        let Some((selected, _)) = nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.end.saturating_sub(node.start) > 1)
            .max_by(|(_, left), (_, right)| left.variance.total_cmp(&right.variance))
        else {
            break;
        };
        let node = nodes[selected].clone();
        if node.variance < 1e-6 {
            break;
        }
        let split = alpha_pca_partition(samples, &mut order, &node);
        if split == node.start || split == node.end {
            nodes[selected].variance = 0.0;
            continue;
        }
        nodes[selected] = alpha_pca_node(samples, &order, node.start, split);
        nodes.push(alpha_pca_node(samples, &order, split, node.end));
    }
    nodes
        .into_iter()
        .map(|node| alpha_dual_point(alpha_premultiplied_color(node.mean)))
        .collect()
}

fn alpha_pca_node(samples: &[AlphaSample], order: &[usize], start: usize, end: usize) -> PcaNode {
    let mut sum = [0.0_f64; 4];
    let mut products = [[0.0_f64; 4]; 4];
    let mut count = 0.0_f64;
    for &index in &order[start..end] {
        let sample = samples[index];
        let point = alpha_premultiplied_point(sample.color);
        let weight = f64::from(sample.count);
        count += weight;
        for row in 0..4 {
            sum[row] += point[row] * weight;
            for column in 0..4 {
                products[row][column] += point[row] * point[column] * weight;
            }
        }
    }
    let mean = sum.map(|value| value / count.max(1.0));
    let mut covariance = [[0.0_f64; 4]; 4];
    for row in 0..4 {
        for column in 0..4 {
            covariance[row][column] =
                products[row][column] - sum[row] * sum[column] / count.max(1.0);
        }
    }
    let mut axis = [0.5_f64; 4];
    let mut variance = 0.0_f64;
    for _ in 0..12 {
        let next = std::array::from_fn(|row| {
            (0..4)
                .map(|column| covariance[row][column] * axis[column])
                .sum::<f64>()
        });
        let length = next.iter().map(|value| value * value).sum::<f64>().sqrt();
        if length <= f64::EPSILON {
            variance = 0.0;
            break;
        }
        axis = next.map(|value| value / length);
        variance = length;
    }
    PcaNode {
        start,
        end,
        mean,
        axis,
        variance,
    }
}

fn alpha_pca_partition(samples: &[AlphaSample], order: &mut [usize], node: &PcaNode) -> usize {
    let plane = alpha_dot(node.mean, node.axis);
    let mut split = node.start;
    for index in node.start..node.end {
        if alpha_dot(
            alpha_premultiplied_point(samples[order[index]].color),
            node.axis,
        ) <= plane
        {
            order.swap(index, split);
            split += 1;
        }
    }
    split
}

fn alpha_premultiplied_point(color: [u8; 4]) -> [f64; 4] {
    let alpha = f64::from(color[3]);
    [
        f64::from(color[0]) * alpha / 255.0,
        f64::from(color[1]) * alpha / 255.0,
        f64::from(color[2]) * alpha / 255.0,
        alpha,
    ]
}

fn alpha_premultiplied_color(point: [f64; 4]) -> [u8; 4] {
    let alpha = point[3].round().clamp(0.0, 255.0);
    if alpha == 0.0 {
        return [0, 0, 0, 0];
    }
    let channel = |value: f64| (value * 255.0 / alpha).round().clamp(0.0, 255.0) as u8;
    [
        channel(point[0]),
        channel(point[1]),
        channel(point[2]),
        alpha as u8,
    ]
}

fn alpha_dual_point(color: [u8; 4]) -> [f64; 4] {
    let alpha = f64::from(color[3]);
    let channel = |value: u8| {
        (f64::from(value) / 255.0).powf(ALPHA_PALETTE_COLOR_GAMMA) * alpha - alpha * 0.5
    };
    [
        channel(color[0]),
        channel(color[1]),
        channel(color[2]),
        alpha * SQRT_3 * 0.5,
    ]
}

fn alpha_dual_color(point: [f64; 4]) -> [u8; 4] {
    let alpha = (point[3] * 2.0 / SQRT_3).round().clamp(0.0, 255.0);
    if alpha == 0.0 {
        return [0, 0, 0, 0];
    }
    let channel = |value: f64| {
        (((value + alpha * 0.5) / alpha)
            .clamp(0.0, 1.0)
            .powf(1.0 / ALPHA_PALETTE_COLOR_GAMMA)
            * 255.0)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    [
        channel(point[0]),
        channel(point[1]),
        channel(point[2]),
        alpha as u8,
    ]
}

fn nearest_alpha_color(point: [f64; 4], palette: &[[f64; 4]]) -> usize {
    let mut winner = 0;
    let mut best = f64::INFINITY;
    for (index, candidate) in palette.iter().enumerate() {
        let d0 = point[0] - candidate[0];
        let d1 = point[1] - candidate[1];
        let d2 = point[2] - candidate[2];
        let d3 = point[3] - candidate[3];
        let distance = d0 * d0 + d1 * d1 + d2 * d2 + d3 * d3;
        if distance < best {
            best = distance;
            winner = index;
        }
    }
    winner
}

fn alpha_dot(left: [f64; 4], right: [f64; 4]) -> f64 {
    (0..4).map(|index| left[index] * right[index]).sum()
}

fn alpha_composite_mean_delta(reference: &RgbaImage, palette: &[[u8; 4]], indices: &[u8]) -> f32 {
    let total = reference
        .pixels()
        .zip(indices)
        .flat_map(|(pixel, &index)| {
            let candidate = palette[index as usize];
            [0_u8, 255].into_iter().flat_map(move |background| {
                (0..3).map(move |channel| {
                    let original = (f32::from(pixel[channel]) * f32::from(pixel[3])
                        + f32::from(background) * f32::from(255 - pixel[3]))
                        / 255.0;
                    let compressed = (f32::from(candidate[channel]) * f32::from(candidate[3])
                        + f32::from(background) * f32::from(255 - candidate[3]))
                        / 255.0;
                    (original - compressed).abs()
                })
            })
        })
        .sum::<f32>();
    total / (reference.width().saturating_mul(reference.height()).max(1) * 6) as f32
}

#[allow(clippy::too_many_arguments)]
fn optimize_exact_alpha_palette(
    input: &[u8],
    width: u32,
    height: u32,
    analysis: ImageAnalysis,
    indexed: ExactRgbaPalette,
    srgb: Option<png::SrgbRenderingIntent>,
    progress: &dyn ProgressSink,
    cancellation: &dyn CancellationToken,
    observer: &dyn OptimizationObserver,
) -> Result<OptimizationResult, OptimizeError> {
    check_cancelled(cancellation)?;
    progress.report(ProgressEvent {
        stage: ProgressStage::Compressing,
    });
    observer.begin(OptimizationOperation::PngEncode);
    let output = encode_indexed_rgba_png(width, height, &indexed.colors, &indexed.indices, srgb)?;
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
                encoder: "exact RGBA indexed PNG".into(),
                quality: None,
                chroma_subsampling: None,
                progressive: None,
                palette_colors: Some(indexed.colors.len() as u16),
                lossless: true,
            },
            processing_time_ms: 0.0,
            already_optimized: false,
            optimizer_version: OPTIMIZER_VERSION,
            warnings: vec!["Przezroczystość i kolory zachowano dokładnie w palecie PNG.".into()],
            analysis,
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn optimize_alpha_palette(
    input: &[u8],
    width: u32,
    height: u32,
    analysis: ImageAnalysis,
    indexed: AlphaPalette,
    srgb: Option<png::SrgbRenderingIntent>,
    progress: &dyn ProgressSink,
    cancellation: &dyn CancellationToken,
    observer: &dyn OptimizationObserver,
) -> Result<OptimizationResult, OptimizeError> {
    check_cancelled(cancellation)?;
    observer.begin(OptimizationOperation::PngEncode);
    let output = encode_indexed_rgba_png(width, height, &indexed.colors, &indexed.indices, srgb)?;
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
                encoder: "alpha-aware palette".into(),
                quality: None,
                chroma_subsampling: None,
                progressive: None,
                palette_colors: Some(indexed.colors.len() as u16),
                lossless: false,
            },
            processing_time_ms: 0.0,
            already_optimized: false,
            optimizer_version: OPTIMIZER_VERSION,
            warnings: vec![
                "Przezroczystość zachowano w palecie dopasowanej do jasnego i ciemnego tła.".into(),
            ],
            analysis,
        },
    })
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
    srgb: Option<png::SrgbRenderingIntent>,
) -> Result<Vec<u8>, OptimizeError> {
    let (palette, indices) = sort_palette_by_luma(palette, indices);
    let depth = indexed_bit_depth(palette.len());
    let packed_indices = (depth != png::BitDepth::Eight)
        .then(|| pack_indices(&indices, depth, width as usize, height as usize));
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut output), width, height);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(depth);
        if let Some(intent) = srgb {
            encoder.set_source_srgb(intent);
        }
        let bytes = palette
            .iter()
            .flat_map(|color| [color.red, color.green, color.blue])
            .collect::<Vec<_>>();
        encoder.set_palette(bytes);
        encoder
            .write_header()
            .map_err(|error| OptimizeError::Encode(error.to_string()))?
            .write_image_data(packed_indices.as_deref().unwrap_or(&indices))
            .map_err(|error| OptimizeError::Encode(error.to_string()))?;
    }
    Ok(output)
}

fn encode_indexed_rgba_png(
    width: u32,
    height: u32,
    palette: &[[u8; 4]],
    indices: &[u8],
    srgb: Option<png::SrgbRenderingIntent>,
) -> Result<Vec<u8>, OptimizeError> {
    let (palette, indices) = sort_rgba_palette_by_luma(palette, indices);
    let depth = indexed_bit_depth(palette.len());
    let packed_indices = pack_indices(&indices, depth, width as usize, height as usize);
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(Cursor::new(&mut output), width, height);
        encoder.set_color(png::ColorType::Indexed);
        encoder.set_depth(depth);
        if let Some(intent) = srgb {
            encoder.set_source_srgb(intent);
        }
        encoder.set_palette(
            palette
                .iter()
                .flat_map(|color| color[..3].iter().copied())
                .collect::<Vec<_>>(),
        );
        if let Some(last_transparent) = palette.iter().rposition(|color| color[3] != 255) {
            encoder.set_trns(
                palette[..=last_transparent]
                    .iter()
                    .map(|color| color[3])
                    .collect::<Vec<_>>(),
            );
        }
        encoder
            .write_header()
            .map_err(|error| OptimizeError::Encode(error.to_string()))?
            .write_image_data(&packed_indices)
            .map_err(|error| OptimizeError::Encode(error.to_string()))?;
    }
    Ok(output)
}

fn sort_rgba_palette_by_luma(palette: &[[u8; 4]], indices: &[u8]) -> (Vec<[u8; 4]>, Vec<u8>) {
    let mut order = (0..palette.len()).collect::<Vec<_>>();
    // Keep entries with alpha before opaque entries. This shortens tRNS while
    // luma ordering makes neighboring palette indexes more compressible.
    order.sort_unstable_by_key(|&index| {
        let color = palette[index];
        (
            u8::from(color[3] == 255),
            std::cmp::Reverse(
                u32::from(color[0]) * 299 + u32::from(color[1]) * 587 + u32::from(color[2]) * 114,
            ),
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

fn indexed_bit_depth(palette_size: usize) -> png::BitDepth {
    match palette_size {
        0..=2 => png::BitDepth::One,
        3..=4 => png::BitDepth::Two,
        5..=16 => png::BitDepth::Four,
        _ => png::BitDepth::Eight,
    }
}

fn pack_indices(indices: &[u8], depth: png::BitDepth, width: usize, height: usize) -> Vec<u8> {
    let bits = match depth {
        png::BitDepth::One => 1,
        png::BitDepth::Two => 2,
        png::BitDepth::Four => 4,
        png::BitDepth::Eight => return indices.to_vec(),
        _ => unreachable!("indexed PNG only uses 1, 2, 4, or 8 bits"),
    };
    let row_bytes = (width * bits).div_ceil(8);
    let mut packed = vec![0_u8; row_bytes * height];
    for (row, source) in indices.chunks_exact(width).enumerate() {
        for (column, index) in source.iter().enumerate() {
            let bit = column * bits;
            packed[row * row_bytes + bit / 8] |= *index << (8 - bits - (bit % 8));
        }
    }
    packed
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

fn source_srgb_intent(input: &[u8]) -> Option<png::SrgbRenderingIntent> {
    let mut cursor = 8_usize;
    while cursor + 12 <= input.len() {
        let (chunk_end, kind, data) = png_chunk(input, cursor).ok()?;
        if kind == b"sRGB" {
            return match data {
                [0] => Some(png::SrgbRenderingIntent::Perceptual),
                [1] => Some(png::SrgbRenderingIntent::RelativeColorimetric),
                [2] => Some(png::SrgbRenderingIntent::Saturation),
                [3] => Some(png::SrgbRenderingIntent::AbsoluteColorimetric),
                _ => None,
            };
        }
        if kind == b"IEND" {
            break;
        }
        cursor = chunk_end;
    }
    None
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
        if matches!(kind, b"iCCP" | b"gAMA" | b"cHRM" | b"cICP" | b"eXIf") {
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
    fn automatic_palette_allows_a_flat_high_colour_map() {
        let map = ImageAnalysis {
            kind: ContentKind::Mixed,
            entropy: 6.69,
            estimated_colors: 184_120,
            edge_density: 0.02,
            noise: 0.066,
            flat_area_ratio: 0.71,
            has_alpha: false,
        };
        assert!(is_palette_candidate(&map));
        assert!(!is_palette_candidate(&ImageAnalysis {
            estimated_colors: MAX_PALETTE_ESTIMATED_COLORS + 1,
            ..map
        }));
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
    fn exact_rgba_palette_reduces_simple_transparent_art_without_pixel_changes() {
        let reference = RgbaImage::from_fn(128, 96, |x, y| {
            if x < 32 {
                image::Rgba([0, 0, 0, 0])
            } else if (x + y) % 3 == 0 {
                image::Rgba([255, 196, 0, 128])
            } else {
                image::Rgba([24, 90, 180, 255])
            }
        });
        let input = encode_rgba_fast(&reference);
        let result = optimize_png(
            &input,
            reference.clone(),
            reference.width(),
            reference.height(),
            crate::analysis::analyze(&reference),
            OptimizeOptions::default(),
            &NoProgress,
            &NeverCancelled,
            &NoObserver,
        )
        .unwrap();

        assert_eq!(result.report.strategy.encoder, "exact RGBA indexed PNG");
        assert!(result.report.strategy.lossless);
        assert_eq!(result.report.strategy.palette_colors, Some(3));
        assert!(result.output.len() < input.len());
        assert_eq!(decode_png(&result.output).unwrap(), reference);
    }

    #[test]
    fn alpha_palette_preserves_a_transparent_canvas_and_limits_composite_error() {
        let reference = RgbaImage::from_fn(160, 120, |x, y| {
            if x < 48 {
                return image::Rgba([0, 0, 0, 0]);
            }
            let alpha = 72 + ((x * 11 + y * 7) % 170) as u8;
            let colors = [[218, 77, 88], [65, 130, 223], [82, 190, 116]];
            image::Rgba([
                colors[((x / 32 + y / 24) % 3) as usize][0],
                colors[((x / 32 + y / 24) % 3) as usize][1],
                colors[((x / 32 + y / 24) % 3) as usize][2],
                alpha,
            ])
        });
        assert!(is_alpha_palette_candidate(&reference));

        let indexed = alpha_aware_palette(&reference, PALETTE_COLORS as usize)
            .expect("palette should meet the alpha guard");
        assert!(indexed.colors.len() <= PALETTE_COLORS as usize);
        assert_eq!(
            indexed.indices.len(),
            reference.width() as usize * reference.height() as usize
        );
        assert!(
            alpha_composite_mean_delta(&reference, &indexed.colors, &indexed.indices)
                <= MAX_ALPHA_COMPOSITE_MEAN_DELTA
        );
        for (pixel, index) in reference.pixels().zip(&indexed.indices) {
            if pixel[3] == 0 {
                assert_eq!(indexed.colors[*index as usize][3], 0);
            }
        }
    }

    #[test]
    fn flat_alpha_graphics_use_the_small_palette_without_affecting_gradients() {
        let flat_logo = ImageAnalysis {
            kind: ContentKind::Graphic,
            entropy: 0.7,
            estimated_colors: 1_700,
            edge_density: 0.009,
            noise: 0.001,
            flat_area_ratio: 0.989,
            has_alpha: true,
        };
        let gradient = ImageAnalysis {
            kind: ContentKind::Mixed,
            entropy: 4.6,
            estimated_colors: 420_000,
            edge_density: 0.001,
            noise: 0.002,
            flat_area_ratio: 0.993,
            has_alpha: true,
        };

        assert_eq!(alpha_palette_size(&flat_logo), 20);
        assert_eq!(alpha_palette_size(&gradient), PALETTE_COLORS as usize);
    }

    #[test]
    fn alpha_perceptual_point_round_trips_palette_channels() {
        for color in [
            [0, 0, 0, 0],
            [18, 92, 181, 17],
            [66, 143, 27, 128],
            [241, 189, 34, 203],
            [255, 255, 255, 255],
        ] {
            assert_eq!(alpha_dual_color(alpha_dual_point(color)), color);
        }
    }

    #[test]
    fn palette_keeps_standard_srgb_intent() {
        let reference = RgbaImage::from_fn(128, 96, |x, y| {
            let shade = ((x / 16 + y / 16) % 3) as u8 * 80;
            image::Rgba([
                shade,
                shade.saturating_add(20),
                255_u8.saturating_sub(shade),
                255,
            ])
        });
        let input = with_srgb(encode_rgba_fast(&reference), 2);
        let analysis = crate::analysis::analyze(&reference);
        let result = optimize_png(
            &input,
            reference,
            128,
            96,
            analysis,
            OptimizeOptions::default(),
            &NoProgress,
            &NeverCancelled,
            &NoObserver,
        )
        .unwrap();

        assert!(result.output.len() < input.len());
        assert_eq!(
            source_srgb_intent(&result.output),
            Some(png::SrgbRenderingIntent::Saturation)
        );
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
    fn opaque_palette_packs_rows_without_changing_pixels() {
        let palette = vec![
            quantette::deps::palette::Srgb::new(20, 30, 40),
            quantette::deps::palette::Srgb::new(120, 130, 140),
            quantette::deps::palette::Srgb::new(220, 230, 240),
        ];
        let indices = (0..39).map(|pixel| (pixel % 3) as u8).collect::<Vec<_>>();
        let output = encode_indexed_png(13, 3, &palette, &indices, None).unwrap();

        assert_eq!(output[24], 2);
        let decoded = decode_png(&output).unwrap();
        for (pixel, &index) in decoded.pixels().zip(&indices) {
            let color = palette[index as usize];
            assert_eq!(pixel.0, [color.red, color.green, color.blue, 255]);
        }
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

    fn with_srgb(input: Vec<u8>, intent: u8) -> Vec<u8> {
        let ihdr_end = 8 + 12 + 13;
        let mut output = input[..ihdr_end].to_vec();
        append_png_chunk(&mut output, b"sRGB", &[intent]);
        output.extend_from_slice(&input[ihdr_end..]);
        output
    }

    fn decode_png(bytes: &[u8]) -> Result<RgbaImage, OptimizeError> {
        image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .map(image::DynamicImage::into_rgba8)
            .map_err(|error| OptimizeError::Decode(error.to_string()))
    }
}
