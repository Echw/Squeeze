use oxipng::{
    BitDepth, ColorType, Deflater, Options as OxiOptions, RawImage, StripChunks, ZopfliOptions,
};
use rgb::RGBA8;
use std::{collections::HashMap, num::NonZeroU64};

use image::RgbaImage;

use crate::{
    check_cancelled,
    imagequant::{self, QuantizeOptions},
    types::*,
};

/// pngquant's quality scale. The target matches the colour budget TinyPNG
/// chooses on the public corpus; the minimum rejects palettes that would
/// visibly posterize a photograph.
const TARGET_QUALITY: u8 = 97;
const MIN_QUALITY: u8 = 40;
/// Dithering is applied through the quantizer's dither map, so only flat
/// areas mapped to one colour receive it.
const DITHERING: f32 = 1.0;
/// Quantizer speed (1 slowest–10 fastest) and OxiPNG effort shrink as images
/// grow. Speed 5 is the fastest setting that keeps the dither map, which
/// gradients and flat graphics need. Interfaces and graphics with few colours
/// quantize quickly at any size and keep the thorough setting.
const THOROUGH_MAX_PIXELS: u64 = 500_000;
const THOROUGH_MAX_COLORS: u32 = 65_536;
const BALANCED_MAX_PIXELS: u64 = 4_000_000;
const BRUTE_FILTER_MAX_PIXELS: u64 = 1_000_000;
/// Zopfli costs several times more than libdeflate and saves 2–5% on small
/// files (up to 20% on tiny icons), so its effort shrinks as images grow.
const ZOPFLI_THOROUGH_MAX_PIXELS: u64 = 64 * 1024;
const ZOPFLI_MAX_PIXELS: u64 = 300_000;
/// Chunks that affect how the pixels are displayed. They describe the colour
/// space of the RGB values, which a palette keeps, so they are copied as-is.
const DISPLAY_CHUNKS: [[u8; 4]; 7] = [
    *b"cICP", *b"iCCP", *b"sRGB", *b"gAMA", *b"cHRM", *b"pHYs", *b"eXIf",
];

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
    check_cancelled(cancellation)?;
    progress.report(ProgressEvent {
        stage: ProgressStage::Compressing,
    });
    let pixels = u64::from(width) * u64::from(height);
    let bit_depth = input.get(24).copied().unwrap_or(8);
    let chunks = display_chunks(input);

    let (palette, strategy) = if bit_depth == 16 {
        (
            None,
            Strategy::Lossless(Some(
                "PNG 16-bit pozostawiono w 16 bitach i skompresowano bezstratnie.",
            )),
        )
    } else {
        observer.begin(OptimizationOperation::PaletteBuild);
        let palette = select_palette(&reference, quantizer_speed(pixels, &analysis));
        observer.end(OptimizationOperation::PaletteBuild);
        match palette {
            Some((palette, strategy)) => (Some(palette), strategy),
            None => (
                None,
                Strategy::Lossless(Some(
                    "Paleta zbyt mocno zmieniałaby wygląd; zachowano bezstratny PNG.",
                )),
            ),
        }
    };
    check_cancelled(cancellation)?;

    let output = match palette {
        Some(palette) => {
            observer.begin(OptimizationOperation::PngEncode);
            let output = encode_indexed(
                width,
                height,
                palette,
                &chunks,
                oxipng_options(pixels, &analysis),
            )?;
            observer.end(OptimizationOperation::PngEncode);
            output
        }
        None => {
            observer.begin(OptimizationOperation::LosslessRecompress);
            let output =
                recompress_lossless(input, oxipng_options(pixels, &analysis), options.limits)?;
            observer.end(OptimizationOperation::LosslessRecompress);
            output
        }
    };
    progress.report(ProgressEvent {
        stage: ProgressStage::Finalizing,
    });

    if output.len() >= input.len() {
        let mut warnings = strategy.warnings();
        warnings.push("Brak oszczędności w tym przebiegu.".into());
        return Ok(passthrough(input, width, height, analysis, warnings));
    }
    if matches!(strategy, Strategy::Lossless(_)) {
        observer.begin(OptimizationOperation::LosslessVerification);
        verify_lossless(input, &output)?;
        observer.end(OptimizationOperation::LosslessVerification);
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
            strategy: strategy.selected(),
            processing_time_ms: 0.0,
            already_optimized: false,
            optimizer_version: OPTIMIZER_VERSION,
            warnings: strategy.warnings(),
            analysis,
        },
    })
}

enum Strategy {
    ExactPalette { colors: u16 },
    Quantized { colors: u16, quality: u8 },
    Lossless(Option<&'static str>),
}

impl Strategy {
    fn selected(&self) -> SelectedStrategy {
        let (encoder, quality, palette_colors, lossless) = match *self {
            Self::ExactPalette { colors } => ("exact palette + OxiPNG", None, Some(colors), true),
            Self::Quantized { colors, quality } => (
                "pngquant palette + OxiPNG",
                Some(quality),
                Some(colors),
                false,
            ),
            Self::Lossless(_) => ("OxiPNG lossless", None, None, true),
        };
        SelectedStrategy {
            encoder: encoder.into(),
            quality,
            chroma_subsampling: None,
            progressive: None,
            palette_colors,
            lossless,
        }
    }

    fn warnings(&self) -> Vec<String> {
        match self {
            Self::ExactPalette { .. } => {
                vec!["Kolory i przezroczystość zachowano dokładnie w palecie PNG.".into()]
            }
            Self::Quantized { .. } => Vec::new(),
            Self::Lossless(reason) => reason.map(String::from).into_iter().collect(),
        }
    }
}

struct Palette {
    colors: Vec<[u8; 4]>,
    indices: Vec<u8>,
}

fn quantizer_speed(pixels: u64, analysis: &ImageAnalysis) -> u8 {
    if analysis.estimated_colors <= THOROUGH_MAX_COLORS {
        return 3;
    }
    if pixels <= THOROUGH_MAX_PIXELS { 3 } else { 5 }
}

/// One quantized palette, as TinyPNG would produce. An image whose distinct
/// RGBA values the quantizer would all keep anyway is stored exactly instead.
/// `None` means even the best palette falls below the minimum quality.
fn select_palette(reference: &RgbaImage, speed: u8) -> Option<(Palette, Strategy)> {
    let exact = exact_palette(reference);
    let quantized = imagequant::quantize(
        reference.as_raw(),
        reference.width(),
        reference.height(),
        &QuantizeOptions {
            max_colors: 256,
            min_quality: MIN_QUALITY,
            target_quality: TARGET_QUALITY,
            speed,
            dithering: DITHERING,
        },
    );
    match (quantized, exact) {
        (Some(quantized), Some(exact)) if quantized.palette.len() >= exact.colors.len() => {
            Some(exact_strategy(exact))
        }
        (Some(quantized), _) => {
            let strategy = Strategy::Quantized {
                colors: quantized.palette.len() as u16,
                quality: quantized.quality,
            };
            Some((
                Palette {
                    colors: quantized.palette,
                    indices: quantized.indices,
                },
                strategy,
            ))
        }
        (None, exact) => exact.map(exact_strategy),
    }
}

fn exact_strategy(exact: Palette) -> (Palette, Strategy) {
    let colors = exact.colors.len() as u16;
    (exact, Strategy::ExactPalette { colors })
}

fn exact_palette(reference: &RgbaImage) -> Option<Palette> {
    let mut lookup = HashMap::<[u8; 4], u8>::new();
    let mut colors = Vec::new();
    let mut indices = Vec::with_capacity(reference.as_raw().len() / 4);
    for pixel in reference.pixels() {
        let mut color = pixel.0;
        // Fully transparent pixels look the same whatever their RGB.
        if color[3] == 0 {
            color = [0, 0, 0, 0];
        }
        let index = match lookup.get(&color) {
            Some(&index) => index,
            None => {
                let index = u8::try_from(colors.len()).ok()?;
                colors.push(color);
                lookup.insert(color, index);
                index
            }
        };
        indices.push(index);
    }
    Some(Palette { colors, indices })
}

/// OxiPNG chooses the bit depth, palette order, filters and deflate
/// parameters. The effort grows as the image gets smaller; brute-force filter
/// search also pays off on large interface captures with few colours.
fn oxipng_options(pixels: u64, analysis: &ImageAnalysis) -> OxiOptions {
    let few_colors = analysis.estimated_colors <= THOROUGH_MAX_COLORS;
    let preset =
        if pixels <= BRUTE_FILTER_MAX_PIXELS || (few_colors && pixels <= BALANCED_MAX_PIXELS * 2) {
            4
        } else if pixels <= BALANCED_MAX_PIXELS {
            2
        } else {
            1
        };
    let mut options = OxiOptions::from_preset(preset);
    if pixels <= ZOPFLI_MAX_PIXELS {
        let iterations = if pixels <= ZOPFLI_THOROUGH_MAX_PIXELS {
            15
        } else {
            3
        };
        options.deflater = Deflater::Zopfli(ZopfliOptions {
            iteration_count: NonZeroU64::new(iterations).expect("non-zero iterations"),
            ..ZopfliOptions::default()
        });
    }
    // Transparent pixels may change RGB; they look the same on any background.
    options.optimize_alpha = true;
    // Text and time chunks go; keeping sRGB lets OxiPNG replace an sRGB ICC
    // profile with the 1-byte chunk.
    options.strip = StripChunks::Keep(DISPLAY_CHUNKS.into_iter().collect());
    options
}

fn encode_indexed(
    width: u32,
    height: u32,
    palette: Palette,
    chunks: &[([u8; 4], Vec<u8>)],
    options: OxiOptions,
) -> Result<Vec<u8>, OptimizeError> {
    let colors = palette
        .colors
        .iter()
        .map(|&[r, g, b, a]| RGBA8::new(r, g, b, a))
        .collect();
    let mut raw = RawImage::new(
        width,
        height,
        ColorType::Indexed { palette: colors },
        BitDepth::Eight,
        palette.indices,
    )
    .map_err(|error| OptimizeError::Encode(error.to_string()))?;
    for (name, data) in chunks {
        raw.add_png_chunk(*name, data.clone());
    }
    raw.create_optimized_png(&options)
        .map_err(|error| OptimizeError::Encode(error.to_string()))
}

fn recompress_lossless(
    input: &[u8],
    mut options: OxiOptions,
    limits: ResourceLimits,
) -> Result<Vec<u8>, OptimizeError> {
    options.max_decompressed_size = Some(limits.max_working_bytes as usize);
    oxipng::optimize_from_memory(input, &options)
        .map_err(|error| OptimizeError::Encode(error.to_string()))
}

/// OxiPNG is lossless apart from the RGB of fully transparent pixels, which
/// `optimize_alpha` may change; everything else must decode identically.
fn verify_lossless(input: &[u8], output: &[u8]) -> Result<(), OptimizeError> {
    let decode = |bytes: &[u8]| {
        image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .map(|image| image.into_rgba16())
            .map_err(|error| OptimizeError::Decode(error.to_string()))
    };
    let before = decode(input)?;
    let after = decode(output)?;
    let same = before.dimensions() == after.dimensions()
        && before
            .pixels()
            .zip(after.pixels())
            .all(|(a, b)| a == b || (a[3] == 0 && b[3] == 0));
    if same {
        Ok(())
    } else {
        Err(OptimizeError::Encode(
            "lossless PNG verification failed; decoded pixels changed".into(),
        ))
    }
}

fn display_chunks(input: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let mut chunks = Vec::new();
    let mut cursor = 8_usize;
    while cursor + 12 <= input.len() {
        let length =
            u32::from_be_bytes(input[cursor..cursor + 4].try_into().expect("4 bytes")) as usize;
        let Some(end) = cursor
            .checked_add(12 + length)
            .filter(|&end| end <= input.len())
        else {
            break;
        };
        let name: [u8; 4] = input[cursor + 4..cursor + 8].try_into().expect("4 bytes");
        if &name == b"IEND" {
            break;
        }
        if DISPLAY_CHUNKS.contains(&name) {
            chunks.push((name, input[cursor + 8..cursor + 8 + length].to_vec()));
        }
        cursor = end;
    }
    chunks
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
    use std::{cell::RefCell, collections::BTreeSet, io::Cursor};

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

    fn run(
        input: &[u8],
        reference: &RgbaImage,
        observer: &dyn OptimizationObserver,
    ) -> OptimizationResult {
        optimize_png(
            input,
            reference.clone(),
            reference.width(),
            reference.height(),
            crate::analysis::analyze(reference),
            OptimizeOptions::default(),
            &NoProgress,
            &NeverCancelled,
            observer,
        )
        .unwrap()
    }

    fn gradient(width: u32, height: u32) -> RgbaImage {
        RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([(x * 3) as u8, (y * 2) as u8, ((x + y) % 256) as u8, 255])
        })
    }

    #[test]
    fn quantizes_a_many_colour_image_once() {
        let reference = gradient(96, 64);
        let input = encode_rgba(&reference, png::Compression::Fast);
        let observer = RecordingObserver::default();
        let result = run(&input, &reference, &observer);

        assert_eq!(result.report.strategy.encoder, "pngquant palette + OxiPNG");
        assert!(!result.report.strategy.lossless);
        assert!(result.output.len() < input.len());
        let operations = observer.operations.borrow();
        assert!(operations.contains(&OptimizationOperation::PaletteBuild));
        assert!(operations.contains(&OptimizationOperation::PngEncode));
        assert!(!operations.contains(&OptimizationOperation::LosslessRecompress));
        assert_eq!(decode_png(&result.output).dimensions(), (96, 64));
    }

    #[test]
    fn exact_palette_keeps_transparent_art_pixel_for_pixel() {
        let reference = RgbaImage::from_fn(128, 96, |x, y| {
            if x < 32 {
                image::Rgba([0, 0, 0, 0])
            } else if (x + y) % 3 == 0 {
                image::Rgba([255, 196, 0, 128])
            } else {
                image::Rgba([24, 90, 180, 255])
            }
        });
        let input = encode_rgba(&reference, png::Compression::Fast);
        let result = run(&input, &reference, &crate::NoObserver);

        assert_eq!(result.report.strategy.encoder, "exact palette + OxiPNG");
        assert!(result.report.strategy.lossless);
        assert_eq!(result.report.strategy.palette_colors, Some(3));
        assert_eq!(decode_png(&result.output), reference);
    }

    #[test]
    fn quantized_palette_keeps_fully_transparent_pixels_transparent() {
        let reference = RgbaImage::from_fn(120, 80, |x, y| {
            if x < 40 {
                image::Rgba([x as u8, 200, 10, 0])
            } else {
                image::Rgba([(x * 2) as u8, (y * 3) as u8, (x + y) as u8, (60 + x) as u8])
            }
        });
        let input = encode_rgba(&reference, png::Compression::Fast);
        let result = run(&input, &reference, &crate::NoObserver);
        let decoded = decode_png(&result.output);
        for (before, after) in reference.pixels().zip(decoded.pixels()) {
            if before[3] == 0 {
                assert_eq!(after[3], 0);
            }
        }
    }

    #[test]
    fn display_chunks_survive_quantization() {
        let reference = gradient(96, 64);
        let input = with_chunk(
            with_chunk(
                encode_rgba(&reference, png::Compression::Fast),
                b"gAMA",
                &45_455_u32.to_be_bytes(),
            ),
            b"pHYs",
            &[0, 0, 0x16, 0x25, 0, 0, 0x16, 0x25, 1],
        );
        let result = run(&input, &reference, &crate::NoObserver);
        let names = display_chunks(&result.output)
            .into_iter()
            .map(|(name, _)| name)
            .collect::<Vec<_>>();
        assert!(names.contains(b"gAMA"));
        assert!(names.contains(b"pHYs"));
    }

    #[test]
    fn sixteen_bit_png_stays_lossless() {
        let reference = gradient(64, 48);
        let wide = reference
            .pixels()
            .flat_map(|px| px.0.into_iter().flat_map(|v| [v, v ^ 0x55]))
            .collect::<Vec<_>>();
        let mut input = Vec::new();
        {
            let mut encoder = png::Encoder::new(Cursor::new(&mut input), 64, 48);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Sixteen);
            encoder.set_compression(png::Compression::Fast);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&wide)
                .unwrap();
        }
        let result = run(&input, &reference, &crate::NoObserver);
        assert!(result.report.strategy.lossless);
        let before = image::load_from_memory(&input).unwrap().into_rgba16();
        let after = image::load_from_memory(&result.output)
            .unwrap()
            .into_rgba16();
        assert_eq!(before, after);
    }

    fn encode_rgba(reference: &RgbaImage, compression: png::Compression) -> Vec<u8> {
        let mut input = Vec::new();
        let mut encoder = png::Encoder::new(
            Cursor::new(&mut input),
            reference.width(),
            reference.height(),
        );
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(compression);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(reference.as_raw())
            .unwrap();
        input
    }

    fn with_chunk(input: Vec<u8>, name: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let ihdr_end = 8 + 12 + 13;
        let mut output = input[..ihdr_end].to_vec();
        output.extend_from_slice(&(data.len() as u32).to_be_bytes());
        output.extend_from_slice(name);
        output.extend_from_slice(data);
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(name);
        hasher.update(data);
        output.extend_from_slice(&hasher.finalize().to_be_bytes());
        output.extend_from_slice(&input[ihdr_end..]);
        output
    }

    fn decode_png(bytes: &[u8]) -> RgbaImage {
        image::load_from_memory_with_format(bytes, image::ImageFormat::Png)
            .unwrap()
            .into_rgba8()
    }
}
