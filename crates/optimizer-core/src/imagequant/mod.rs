//! Palette quantization ported from libimagequant 2.4.1, the last
//! BSD-licensed release of pngquant's library (the BSD-2-Clause and Poskanzer
//! notices are in third_party/libimagequant/COPYRIGHT). It weights colours by
//! a noise map, repeats median cut with feedback, refines the palette by
//! Voronoi iteration and dithers only flat areas.

mod contrast;
mod histogram;
mod mediancut;
mod nearest;
mod pixel;

use histogram::Histogram;
use mediancut::{Colormap, PaletteEntry, mediancut};
use nearest::NearestMap;
use pixel::{FPixel, MAX_DIFF, SRGB_GAMMA, gamma_lut, to_f, to_rgb};

#[derive(Clone, Copy, Debug)]
pub(crate) struct QuantizeOptions {
    pub max_colors: usize,
    /// Results below this quality are rejected.
    pub min_quality: u8,
    /// The palette stops growing once this quality is reached.
    pub target_quality: u8,
    /// 1 (slowest, best) to 10 (fastest).
    pub speed: u8,
    /// Floyd–Steinberg strength, 0.0–1.0, applied through the dither map.
    pub dithering: f32,
}

pub(crate) struct Quantized {
    /// RGBA entries; translucent entries come first so `tRNS` stays short.
    pub palette: Vec<[u8; 4]>,
    pub indices: Vec<u8>,
    /// pngquant's 0–100 quality of the palette before dithering.
    pub quality: u8,
}

struct SpeedSettings {
    voronoi_iterations: u32,
    voronoi_iteration_limit: f64,
    feedback_loop_trials: i32,
    max_histogram_entries: usize,
    use_dither_map: bool,
    use_contrast_maps: bool,
}

impl SpeedSettings {
    fn new(speed: u8) -> Self {
        let speed = u32::from(speed.clamp(1, 10));
        let mut iterations = 8_u32.saturating_sub(speed);
        iterations += iterations * iterations / 2;
        let use_dither_map = speed <= 5;
        Self {
            voronoi_iterations: iterations,
            voronoi_iteration_limit: 1.0 / f64::from(1_u32 << (23 - speed)),
            feedback_loop_trials: (56 - 9 * speed as i32).max(0),
            max_histogram_entries: (1 << 17) + (1 << 18) * (10 - speed as usize),
            use_dither_map,
            use_contrast_maps: speed <= 7 || use_dither_map,
        }
    }
}

struct Image<'a> {
    rgba: &'a [u8],
    width: usize,
    height: usize,
    lut: [f32; 256],
}

impl Image<'_> {
    /// Rows are converted on demand; a cached float copy of a 24 MP image
    /// would not fit the Worker's memory budget.
    fn row(&self, y: usize, out: &mut [FPixel]) {
        let start = y * self.width * 4;
        for (px, rgba) in out
            .iter_mut()
            .zip(self.rgba[start..start + self.width * 4].as_chunks::<4>().0)
        {
            *px = to_f(&self.lut, [rgba[0], rgba[1], rgba[2], rgba[3]]);
        }
    }
}

/// Returns `None` when the best palette is below `min_quality`.
pub(crate) fn quantize(
    rgba: &[u8],
    width: u32,
    height: u32,
    options: &QuantizeOptions,
) -> Option<Quantized> {
    let settings = SpeedSettings::new(options.speed);
    let image = Image {
        rgba,
        width: width as usize,
        height: height as usize,
        lut: gamma_lut(SRGB_GAMMA),
    };
    let max_colors = options.max_colors.clamp(2, 256);
    let target_mse = quality_to_mse(options.target_quality);
    let max_mse = quality_to_mse(options.min_quality);

    let maps = settings
        .use_contrast_maps
        .then(|| contrast::contrast_maps(image.width, image.height, |y, out| image.row(y, out)))
        .flatten();
    let mut hist = histogram::build(
        rgba,
        width,
        height,
        maps.as_ref().map(|maps| maps.noise.as_slice()),
        settings.max_histogram_entries,
        &image.lut,
    );
    let (mut map, palette_error) =
        quantize_histogram(&mut hist, &settings, max_colors, target_mse, max_mse)?;
    sort_palette(&mut map);

    let mut indices = vec![0_u8; image.width * image.height];
    let edges = maps.map(|maps| maps.edges);
    let palette = remap(
        &image,
        &mut map,
        &mut indices,
        edges,
        palette_error,
        &settings,
        options.dithering,
    );
    Some(Quantized {
        palette,
        indices,
        quality: mse_to_quality(palette_error),
    })
}

/// libimagequant's quality scale, fudged to resemble libjpeg's.
fn quality_to_mse(quality: u8) -> f64 {
    match quality {
        0 => f64::from(MAX_DIFF),
        100.. => 0.0,
        _ => {
            let quality = f64::from(quality);
            let extra_low_quality_fudge = (0.016 / (0.001 + quality) - 0.001).max(0.0);
            extra_low_quality_fudge + 2.5 / (210.0 + quality).powf(1.2) * (100.1 - quality) / 100.0
        }
    }
}

fn mse_to_quality(mse: f64) -> u8 {
    (1..=100)
        .rev()
        .find(|&quality| mse <= quality_to_mse(quality) + 0.000_001)
        .unwrap_or(0)
}

fn quantize_histogram(
    hist: &mut Histogram,
    settings: &SpeedSettings,
    max_colors: usize,
    target_mse: f64,
    max_mse: f64,
) -> Option<(Colormap, f64)> {
    let few_input_colors = hist.items.len() <= max_colors;
    // Palette-to-palette conversion rarely looks good, so an image that already
    // fits must improve much more to be degraded.
    if few_input_colors && target_mse == 0.0 {
        let entries = hist
            .items
            .iter()
            .map(|item| PaletteEntry {
                color: item.color,
                popularity: item.perceptual_weight,
            })
            .collect();
        return Some((
            Colormap {
                entries,
                subset: None,
            },
            0.0,
        ));
    }
    let max_mse = max_mse * if few_input_colors { 0.33 } else { 1.0 };
    let (mut map, mut palette_error) =
        find_best_palette(hist, settings, max_colors, target_mse, max_mse);

    let mut iterations = settings.voronoi_iterations;
    if iterations == 0 && palette_error < 0.0 && max_mse < f64::from(MAX_DIFF) {
        // Otherwise the error is never measured and the limit cannot apply.
        iterations = 1;
    }
    if iterations > 0 {
        for item in &mut hist.items {
            if usize::from(item.likely_index) >= map.entries.len() {
                item.likely_index = 0;
            }
        }
        let mut previous_error = f64::from(MAX_DIFF);
        let mut i = 0;
        while i < iterations {
            palette_error = voronoi_iteration(hist, &mut map, false);
            if (previous_error - palette_error).abs() < settings.voronoi_iteration_limit {
                break;
            }
            if palette_error > max_mse * 1.5 {
                // Probably hopeless.
                if palette_error > max_mse * 3.0 {
                    break;
                }
                i += 1;
            }
            previous_error = palette_error;
            i += 1;
        }
    }
    (palette_error <= max_mse).then_some((map, palette_error))
}

/// Repeats median cut with histogram weights adjusted by the previous
/// palette's error and keeps the best palette, preferring fewer colours once
/// the target is met.
fn find_best_palette(
    hist: &mut Histogram,
    settings: &SpeedSettings,
    mut max_colors: usize,
    target_mse: f64,
    max_mse: f64,
) -> (Colormap, f64) {
    // Never aim below what 8-bit output can represent.
    let target_mse = max_mse.min(target_mse.max((1.0 / 1024.0_f64).powi(2)));
    let mut trials = settings.feedback_loop_trials;
    let mut best: Option<Colormap> = None;
    let mut least_error = f64::from(MAX_DIFF);
    let mut target_mse_overshoot = if trials > 0 { 1.05 } else { 1.0 };
    loop {
        let mut map = mediancut(
            hist,
            max_colors,
            target_mse * target_mse_overshoot,
            (90.0 / 65536.0_f64).max(target_mse).max(least_error) * 1.2,
        );
        if trials <= 0 {
            return (map, -1.0);
        }
        // The first pass at a non-zero target keeps the median cut weights.
        let first_run_of_target_mse = best.is_none() && target_mse > 0.0;
        let total_error = voronoi_iteration(hist, &mut map, !first_run_of_target_mse);
        if best.is_none()
            || total_error < least_error
            || (total_error <= target_mse && map.entries.len() < max_colors)
        {
            if total_error < target_mse && total_error > 0.0 {
                // Voronoi iteration beats what median cut aims for, so median
                // cut may aim lower.
                target_mse_overshoot = (target_mse_overshoot * 1.25).min(target_mse / total_error);
            }
            least_error = total_error;
            // Keep a reduced colour count, with one spare colour of room.
            max_colors = max_colors.min(map.entries.len() + 1);
            best = Some(map);
            trials -= 1;
        } else {
            for item in &mut hist.items {
                item.adjusted_weight = (item.perceptual_weight + item.adjusted_weight) / 2.0;
            }
            target_mse_overshoot = 1.0;
            trials -= 6;
            // A much worse palette is unlikely to improve.
            if total_error > least_error * 4.0 {
                trials -= 3;
            }
        }
        if trials <= 0 {
            break;
        }
    }
    (
        best.expect("the first trial always keeps its palette"),
        least_error,
    )
}

/// Moves each palette colour to the weighted mean of the histogram colours
/// it represents and returns the mean error.
fn voronoi_iteration(hist: &mut Histogram, map: &mut Colormap, adjust_weights: bool) -> f64 {
    let nearest = NearestMap::new(map);
    let mut sums = vec![[0.0_f64; 5]; map.entries.len()];
    let mut total_diff = 0.0_f64;
    for item in &mut hist.items {
        let (index, diff) = nearest.search(item.color, usize::from(item.likely_index));
        item.likely_index = index as u8;
        total_diff += f64::from(diff) * f64::from(item.perceptual_weight);
        accumulate(&mut sums[index], item.color, item.perceptual_weight);
        if adjust_weights {
            // Give poorly matched colours more weight in the next median cut.
            item.adjusted_weight =
                (item.perceptual_weight + item.adjusted_weight) * (1.0 + diff).sqrt();
        }
    }
    finish_voronoi(map, &sums);
    total_diff / hist.total_perceptual_weight
}

fn accumulate(sum: &mut [f64; 5], color: FPixel, weight: f32) {
    let weight = f64::from(weight);
    sum[0] += f64::from(color.a) * weight;
    sum[1] += f64::from(color.r) * weight;
    sum[2] += f64::from(color.g) * weight;
    sum[3] += f64::from(color.b) * weight;
    sum[4] += weight;
}

fn finish_voronoi(map: &mut Colormap, sums: &[[f64; 5]]) {
    for (index, (entry, sum)) in map.entries.iter_mut().zip(sums).enumerate() {
        let mut total = sum[4];
        if total != 0.0 {
            entry.color = FPixel::new(
                (sum[0] / total) as f32,
                (sum[1] / total) as f32,
                (sum[2] / total) as f32,
                (sum[3] / total) as f32,
            );
        } else {
            total = index as f64 / 1024.0;
        }
        entry.popularity = total as f32;
    }
}

/// Puts translucent entries first to shorten `tRNS` and orders each group by
/// popularity, which compresses slightly better.
fn sort_palette(map: &mut Colormap) {
    let by_popularity =
        |left: &PaletteEntry, right: &PaletteEntry| right.popularity.total_cmp(&left.popularity);
    let (mut translucent, mut opaque): (Vec<_>, Vec<_>) = map
        .entries
        .iter()
        .partition(|entry| entry.color.a < 255.0 / 256.0);
    translucent.sort_by(by_popularity);
    opaque.sort_by(by_popularity);
    map.entries = translucent.into_iter().chain(opaque).collect();
    if map.entries.len() > 16 {
        map.entries.swap(7, 1);
        map.entries.swap(8, 2);
        map.entries.swap(9, 3);
    }
}

/// Rounds the palette to bytes and stores the rounded colours back so that
/// remapping and dithering measure what the PNG will contain.
fn round_palette(map: &mut Colormap, lut: &[f32; 256]) -> Vec<[u8; 4]> {
    map.entries
        .iter_mut()
        .map(|entry| {
            let px = to_rgb(SRGB_GAMMA, entry.color);
            entry.color = to_f(lut, px);
            px
        })
        .collect()
}

fn remap(
    image: &Image,
    map: &mut Colormap,
    indices: &mut [u8],
    edges: Option<Vec<u8>>,
    palette_error: f64,
    settings: &SpeedSettings,
    dithering: f32,
) -> Vec<[u8; 4]> {
    if dithering <= 0.0 {
        let palette = round_palette(map, &image.lut);
        remap_plain(image, map, indices);
        return palette;
    }
    let mut remapping_error = palette_error;
    let mut dither_map = None;
    if settings.use_dither_map
        && let Some(mut edges) = edges
    {
        // The plain remap finds areas that need dithering; it is also the
        // last Voronoi step, so the palette is rounded only afterwards.
        remapping_error = remap_plain(image, map, indices);
        update_dither_map(indices, &mut edges, image.width, image.height);
        dither_map = Some(edges);
    }
    let palette = round_palette(map, &image.lut);
    let remapped = dither_map.is_some();
    remap_floyd(
        image,
        map,
        indices,
        dither_map.as_deref(),
        (remapping_error * 2.4).max(16.0 / 256.0) as f32,
        remapped,
        dithering,
    );
    palette
}

/// Nearest-colour remap without dithering; also refines the palette by one
/// Voronoi step over the actual pixels.
fn remap_plain(image: &Image, map: &mut Colormap, indices: &mut [u8]) -> f64 {
    let nearest = NearestMap::new(map);
    let mut sums = vec![[0.0_f64; 5]; map.entries.len()];
    let mut row = vec![FPixel::default(); image.width];
    let mut total_error = 0.0_f64;
    for y in 0..image.height {
        image.row(y, &mut row);
        let mut last_match = 0;
        for (x, px) in row.iter().enumerate() {
            let (index, diff) = nearest.search(*px, last_match);
            last_match = index;
            indices[y * image.width + x] = index as u8;
            total_error += f64::from(diff);
            accumulate(&mut sums[index], *px, 1.0);
        }
    }
    finish_voronoi(map, &sums);
    total_error / (image.width * image.height) as f64
}

/// Scales dithering down at edges and inside runs of different colours: only
/// large flat areas mapped to one colour need it. Dithering on edges makes
/// them jagged, and noisy areas are dithered naturally.
fn update_dither_map(indices: &[u8], edges: &mut [u8], width: usize, height: usize) {
    for row in 0..height {
        let line = &indices[row * width..(row + 1) * width];
        let mut last_pixel = line[0];
        let mut last_col = 0;
        for (col, &px) in line.iter().enumerate().skip(1) {
            if px != last_pixel || col == width - 1 {
                let mut neighbour_count = 2.5 + (col - last_col) as f32;
                for i in last_col..col {
                    if row > 0 && indices[(row - 1) * width + i] == last_pixel {
                        neighbour_count += 1.0;
                    }
                    if row < height - 1 && indices[(row + 1) * width + i] == last_pixel {
                        neighbour_count += 1.0;
                    }
                }
                while last_col <= col {
                    let e = f32::from(edges[row * width + last_col]) / 255.0
                        * (1.0 - 2.5 / neighbour_count);
                    edges[row * width + last_col] = (e * 255.0) as u8;
                    last_col += 1;
                }
                last_pixel = px;
            }
        }
    }
}

/// Deterministic noise so that the same input gives the same output.
struct Lcg(u32);

impl Lcg {
    const MAX: f64 = 2_147_483_647.0;

    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        f64::from(self.0 & 0x7fff_ffff)
    }
}

fn dithered_pixel(dither_level: f32, max_dither_error: f32, error: FPixel, px: FPixel) -> FPixel {
    let sr = error.r * dither_level;
    let sg = error.g * dither_level;
    let sb = error.b * dither_level;
    let sa = error.a * dither_level;

    // A little overflow avoids undithered bands where all channels clamp.
    let mut ratio = 1.0_f32;
    let limit = |value: f32, step: f32, ratio: &mut f32| {
        if value + step > 1.03 {
            *ratio = ratio.min((1.03 - value) / step);
        } else if value + step < 0.0 {
            *ratio = ratio.min(value / -step);
        }
    };
    limit(px.r, sr, &mut ratio);
    limit(px.g, sg, &mut ratio);
    limit(px.b, sb, &mut ratio);
    let a = (px.a + sa).clamp(0.0, 1.0);

    // A huge propagated error would make stray pixels pop out.
    let dither_error = sr * sr + sg * sg + sb * sb + sa * sa;
    if dither_error > max_dither_error {
        ratio *= 0.8;
    } else if dither_error < 2.0 / 256.0 / 256.0 {
        // Invisible errors are not dithered, which keeps the file smaller.
        return px;
    }
    FPixel::new(a, px.r + sr * ratio, px.g + sg * ratio, px.b + sb * ratio)
}

/// Serpentine Floyd–Steinberg, scaled per pixel by the dither map.
fn remap_floyd(
    image: &Image,
    map: &Colormap,
    indices: &mut [u8],
    dither_map: Option<&[u8]>,
    max_dither_error: f32,
    output_is_remapped: bool,
    dithering: f32,
) {
    let (width, height) = (image.width, image.height);
    let palette = map
        .entries
        .iter()
        .map(|entry| entry.color)
        .collect::<Vec<_>>();
    let nearest = NearestMap::new(map);
    // Two extra cells avoid bounds checks at the row ends.
    let mut this_error = vec![FPixel::default(); width + 2];
    let mut next_error = vec![FPixel::default(); width + 2];
    let mut rng = Lcg(12_345);
    for cell in &mut this_error {
        let mut noise = || ((rng.next() - Lcg::MAX / 2.0) / Lcg::MAX / 255.0) as f32;
        *cell = FPixel::new(noise(), noise(), noise(), noise());
    }

    // The response is non-linear; without this curve any level below 0.8
    // would give almost no dithering.
    let mut base_level = 1.0 - (1.0 - dithering).powi(3);
    if dither_map.is_some() {
        base_level /= 255.0;
    }
    // Keeps small errors from accumulating.
    base_level *= 15.0 / 16.0;

    let mut row = vec![FPixel::default(); width];
    let mut left_to_right = true;
    let mut last_match = 0_usize;
    for y in 0..height {
        next_error.fill(FPixel::default());
        image.row(y, &mut row);
        let mut x = if left_to_right { 0 } else { width - 1 };
        loop {
            let offset = y * width + x;
            let mut level = base_level;
            if let Some(map) = dither_map {
                level *= f32::from(map[offset]);
            }
            let spx = dithered_pixel(level, max_dither_error, this_error[x + 1], row[x]);
            let guess = if output_is_remapped {
                usize::from(indices[offset])
            } else {
                last_match
            };
            let (index, _) = nearest.search(spx, guess);
            last_match = index;
            indices[offset] = index as u8;

            let xp = palette[index];
            let mut err = FPixel::new(spx.a - xp.a, spx.r - xp.r, spx.g - xp.g, spx.b - xp.b);
            if err.r * err.r + err.g * err.g + err.b * err.b + err.a * err.a > max_dither_error {
                level *= 0.75;
            }
            let color_importance = (3.0 + xp.a) / 4.0 * level;
            err.r *= color_importance;
            err.g *= color_importance;
            err.b *= color_importance;
            err.a *= level;

            let (ahead, behind) = if left_to_right {
                (x + 2, x)
            } else {
                (x, x + 2)
            };
            add_scaled(&mut this_error[ahead], err, 7.0 / 16.0);
            next_error[ahead] = scaled(err, 1.0 / 16.0);
            add_scaled(&mut next_error[x + 1], err, 5.0 / 16.0);
            add_scaled(&mut next_error[behind], err, 3.0 / 16.0);

            if left_to_right {
                x += 1;
                if x >= width {
                    break;
                }
            } else {
                if x == 0 {
                    break;
                }
                x -= 1;
            }
        }
        std::mem::swap(&mut this_error, &mut next_error);
        left_to_right = !left_to_right;
    }
}

fn scaled(px: FPixel, factor: f32) -> FPixel {
    FPixel::new(px.a * factor, px.r * factor, px.g * factor, px.b * factor)
}

fn add_scaled(target: &mut FPixel, px: FPixel, factor: f32) {
    target.a += px.a * factor;
    target.r += px.r * factor;
    target.g += px.g * factor;
    target.b += px.b * factor;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(width: u32, height: u32, alpha: bool) -> Vec<u8> {
        (0..height)
            .flat_map(|y| {
                (0..width).flat_map(move |x| {
                    [
                        (x * 255 / width) as u8,
                        (y * 255 / height) as u8,
                        ((x + y) * 127 / (width + height)) as u8,
                        if alpha { (x * 255 / width) as u8 } else { 255 },
                    ]
                })
            })
            .collect()
    }

    fn options(target_quality: u8, dithering: f32) -> QuantizeOptions {
        QuantizeOptions {
            max_colors: 256,
            min_quality: 0,
            target_quality,
            speed: 3,
            dithering,
        }
    }

    #[test]
    fn quality_scale_round_trips() {
        for quality in [0, 30, 70, 90, 97, 100] {
            assert_eq!(mse_to_quality(quality_to_mse(quality)), quality);
        }
    }

    #[test]
    fn few_colours_are_kept_exactly_at_full_quality() {
        let rgba = [
            [255, 0, 0, 255],
            [0, 0, 255, 128],
            [0, 0, 0, 0],
            [255, 0, 0, 255],
        ]
        .concat();
        let result = quantize(&rgba, 2, 2, &options(100, 1.0)).unwrap();
        assert_eq!(result.palette.len(), 3);
        for (px, &index) in rgba.as_chunks::<4>().0.iter().zip(&result.indices) {
            let colour = result.palette[usize::from(index)];
            assert_eq!(colour[3], px[3]);
            if px[3] > 0 {
                assert!(colour.iter().zip(px).all(|(a, b)| a.abs_diff(*b) <= 1));
            }
        }
        // Translucent entries come first.
        assert!(result.palette[0][3] < 255 && result.palette[1][3] < 255);
    }

    #[test]
    fn gradient_quantizes_within_palette_limits() {
        for alpha in [false, true] {
            let rgba = gradient(96, 64, alpha);
            for dithering in [0.0, 1.0] {
                let result = quantize(&rgba, 96, 64, &options(97, dithering)).unwrap();
                assert!(result.palette.len() <= 256);
                assert_eq!(result.indices.len(), 96 * 64);
                assert!(
                    result
                        .indices
                        .iter()
                        .all(|&index| usize::from(index) < result.palette.len())
                );
                // libimagequant 2.4.1 itself reaches 57 (opaque) and 75 (alpha).
                assert!(result.quality >= 55, "quality {}", result.quality);
            }
        }
    }

    #[test]
    fn rejects_palettes_below_minimum_quality() {
        let rgba = gradient(64, 64, false);
        let strict = QuantizeOptions {
            max_colors: 4,
            min_quality: 95,
            ..options(100, 0.0)
        };
        assert!(quantize(&rgba, 64, 64, &strict).is_none());
    }

    #[test]
    fn fully_transparent_pixels_stay_transparent() {
        let mut rgba = gradient(64, 64, true);
        for px in rgba.as_chunks_mut::<4>().0.iter_mut().step_by(3) {
            px.copy_from_slice(&[200, 10, 10, 0]);
        }
        let result = quantize(&rgba, 64, 64, &options(90, 1.0)).unwrap();
        for (px, &index) in rgba.as_chunks::<4>().0.iter().zip(&result.indices) {
            if px[3] == 0 {
                assert_eq!(result.palette[usize::from(index)][3], 0);
            }
        }
    }
}
