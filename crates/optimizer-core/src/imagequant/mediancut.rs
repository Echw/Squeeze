// Ported from libimagequant 2.4.1; see third_party/libimagequant/COPYRIGHT.
// The median cut follows Heckbert, "Color Image Quantization for Frame
// Buffer Display", SIGGRAPH 1982.

use super::histogram::{HistItem, Histogram};
use super::pixel::{FPixel, color_difference};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PaletteEntry {
    pub color: FPixel,
    pub popularity: f32,
}

#[derive(Clone, Debug, Default)]
pub(super) struct Colormap {
    pub entries: Vec<PaletteEntry>,
    /// A smaller palette remembered during median cut; it seeds nearest search.
    pub subset: Option<Vec<PaletteEntry>>,
}

#[derive(Clone, Copy, Default)]
struct Bin {
    color: FPixel,
    variance: FPixel,
    sum: f64,
    total_error: f64,
    max_error: f64,
    start: usize,
    colors: usize,
}

const CENTER: FPixel = FPixel::new(0.5, 0.5, 0.5, 0.5);

pub(super) fn mediancut(
    hist: &mut Histogram,
    max_colors: usize,
    target_mse: f64,
    max_mse: f64,
) -> Colormap {
    let total_perceptual_weight = hist.total_perceptual_weight;
    let items = &mut hist.items[..];
    let mut bins = Vec::with_capacity(max_colors);
    let mut first = Bin {
        start: 0,
        colors: items.len(),
        total_error: -1.0,
        ..Bin::default()
    };
    first.color = average_pixels(items, CENTER);
    first.variance = bin_variance(items, &first);
    first.max_error = bin_max_error(items, &first);
    first.sum = items
        .iter()
        .map(|item| f64::from(item.adjusted_weight))
        .sum();
    bins.push(first);

    let mut subset = None;
    let subset_size = (max_colors as f32).powf(0.7).ceil() as usize;

    while bins.len() < max_colors {
        if bins.len() == subset_size {
            subset = Some(entries_from_bins(&bins, items));
        }
        // Split bins over the quality limit first (an odd green pixel gets a
        // colour), then raise the limit so smooth gradients can get colours.
        let current_max_mse = max_mse + (bins.len() as f64 / max_colors as f64) * 16.0 * max_mse;
        let Some(selected) = best_splittable_bin(&bins, current_max_mse) else {
            break;
        };
        let Bin { start, colors, .. } = bins[selected];

        let half_variance = prepare_sort(&bins[selected], items);
        let break_at = (sort_half_variance(&mut items[start..start + colors], half_variance) + 1)
            .min(colors - 1);

        let total_sum = bins[selected].sum;
        let lower_sum: f64 = items[start..start + break_at]
            .iter()
            .map(|item| f64::from(item.adjusted_weight))
            .sum();
        let previous_center = bins[selected].color;
        bins[selected] = new_bin(items, start, break_at, lower_sum, previous_center);
        let upper = new_bin(
            items,
            start + break_at,
            colors - break_at,
            total_sum - lower_sum,
            previous_center,
        );
        bins.push(upper);

        if total_error_below_target(target_mse * total_perceptual_weight, &mut bins, items) {
            break;
        }
    }

    let entries = entries_from_bins(&bins, items);
    // Raise the weight of colours far from their final palette colour; the
    // feedback loop uses it when it repeats median cut.
    for (index, bin) in bins.iter().enumerate() {
        for item in &mut items[bin.start..bin.start + bin.colors] {
            item.adjusted_weight *= (1.0
                + f64::from(color_difference(entries[index].color, item.color)) / 4.0)
                .sqrt() as f32;
            item.likely_index = index as u8;
        }
    }
    Colormap { entries, subset }
}

fn new_bin(items: &[HistItem], start: usize, colors: usize, sum: f64, center: FPixel) -> Bin {
    let mut bin = Bin {
        start,
        colors,
        sum,
        total_error: -1.0,
        ..Bin::default()
    };
    bin.color = average_pixels(&items[start..start + colors], center);
    bin.variance = bin_variance(items, &bin);
    bin.max_error = bin_max_error(items, &bin);
    bin
}

fn entries_from_bins(bins: &[Bin], items: &[HistItem]) -> Vec<PaletteEntry> {
    bins.iter()
        .map(|bin| PaletteEntry {
            color: bin.color,
            popularity: items[bin.start..bin.start + bin.colors]
                .iter()
                .map(|item| item.perceptual_weight)
                .sum(),
        })
        .collect()
}

fn variance_diff(value: f64, good_enough: f64) -> f64 {
    let squared = value * value;
    if squared < good_enough * good_enough {
        squared * 0.25
    } else {
        squared
    }
}

/// Weighted per-channel variance, used to choose the split channel.
fn bin_variance(items: &[HistItem], bin: &Bin) -> FPixel {
    let mean = bin.color;
    let (mut a, mut r, mut g, mut b) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    for item in &items[bin.start..bin.start + bin.colors] {
        let px = item.color;
        let weight = f64::from(item.adjusted_weight);
        a += variance_diff(f64::from(mean.a - px.a), 2.0 / 256.0) * weight;
        r += variance_diff(f64::from(mean.r - px.r), 1.0 / 256.0) * weight;
        g += variance_diff(f64::from(mean.g - px.g), 1.0 / 256.0) * weight;
        b += variance_diff(f64::from(mean.b - px.b), 1.0 / 256.0) * weight;
    }
    FPixel::new(
        (a * (4.0 / 16.0)) as f32,
        (r * (7.0 / 16.0)) as f32,
        (g * (9.0 / 16.0)) as f32,
        (b * (5.0 / 16.0)) as f32,
    )
}

fn bin_max_error(items: &[HistItem], bin: &Bin) -> f64 {
    items[bin.start..bin.start + bin.colors]
        .iter()
        .map(|item| f64::from(color_difference(bin.color, item.color)))
        .fold(0.0, f64::max)
}

fn best_splittable_bin(bins: &[Bin], max_mse: f64) -> Option<usize> {
    let mut best = None;
    let mut max_sum = 0.0;
    for (index, bin) in bins.iter().enumerate() {
        if bin.colors < 2 {
            continue;
        }
        // Only the largest variance matters because the split uses it.
        let colour_variance = bin.variance.r.max(bin.variance.g).max(bin.variance.b);
        let mut sum = bin.sum * f64::from(bin.variance.a.max(colour_variance));
        if bin.max_error > max_mse {
            sum *= bin.max_error / max_mse;
        }
        if sum > max_sum {
            max_sum = sum;
            best = Some(index);
        }
    }
    best
}

/// Orders the bin by its highest-variance channel and returns half of its
/// total colour weight; the split balances that weight on both sides.
fn prepare_sort(bin: &Bin, items: &mut [HistItem]) -> f64 {
    let mut channels = [
        (1_usize, bin.variance.r),
        (2, bin.variance.g),
        (3, bin.variance.b),
        (0, bin.variance.a),
    ];
    channels.sort_by(|left, right| right.1.total_cmp(&left.1));
    let slice = &mut items[bin.start..bin.start + bin.colors];
    for item in slice.iter_mut() {
        let c = item.color;
        // Only the first channel matters; the rest keep repeated runs with
        // different weights deterministic.
        let primary = (f64::from(c.channel(channels[0].0)) * 65535.0) as u32;
        let secondary = ((f64::from(c.channel(channels[2].0))
            + f64::from(c.channel(channels[1].0)) / 2.0
            + f64::from(c.channel(channels[3].0)) / 4.0)
            * 65535.0) as u32;
        item.sort_value = (primary << 16) | secondary;
    }
    let median = median_color(slice);
    let mut total = 0.0;
    for item in slice.iter_mut() {
        item.color_weight = color_weight(median, item) as f32;
        total += f64::from(item.color_weight);
    }
    total / 2.0
}

fn median_color(items: &mut [HistItem]) -> FPixel {
    let median_start = (items.len() - 1) / 2;
    sort_range(items, median_start);
    if items.len() & 1 == 1 {
        return items[median_start].color;
    }
    // The second colour is not guaranteed to be sorted, which is close enough.
    average_pixels(&items[median_start..median_start + 2], CENTER)
}

fn color_weight(median: FPixel, item: &HistItem) -> f64 {
    let mut diff = color_difference(median, item.color);
    // A colour that is good enough should not drive further splits.
    if diff < 2.0 / 256.0 / 256.0 {
        diff /= 2.0;
    }
    f64::from(diff).sqrt() * ((1.0 + f64::from(item.adjusted_weight)).sqrt() - 1.0)
}

fn qsort_pivot(items: &[HistItem]) -> usize {
    let len = items.len();
    if len < 32 {
        return len / 2;
    }
    let (ai, bi, ci) = (8, len / 2, len - 1);
    let (a, b, c) = (
        items[ai].sort_value,
        items[bi].sort_value,
        items[ci].sort_value,
    );
    if a < b {
        if b < c {
            bi
        } else if a < c {
            ci
        } else {
            ai
        }
    } else if b > c {
        bi
    } else if a < c {
        ai
    } else {
        ci
    }
}

/// Partitions in descending `sort_value` order and returns the pivot index.
fn qsort_partition(items: &mut [HistItem]) -> usize {
    let len = items.len();
    if len >= 8 {
        let pivot = qsort_pivot(items);
        items.swap(0, pivot);
    }
    let pivot_value = items[0].sort_value;
    let (mut l, mut r) = (1, len);
    while l < r {
        if items[l].sort_value >= pivot_value {
            l += 1;
        } else {
            loop {
                r -= 1;
                if !(l < r && items[r].sort_value <= pivot_value) {
                    break;
                }
            }
            items.swap(l, r);
        }
    }
    l -= 1;
    items.swap(0, l);
    l
}

/// Quickselect: places the element at `sort_start` in its sorted position.
fn sort_range(mut items: &mut [HistItem], mut sort_start: usize) {
    loop {
        let l = qsort_partition(items);
        let r = l + 1;
        if l > 0 && sort_start < l {
            items = &mut items[..l];
        } else if r < items.len() && sort_start > r {
            items = &mut items[r..];
            sort_start -= r;
        } else {
            break;
        }
    }
}

/// Sorts just enough to put entries with half of the colour weight first and
/// returns the boundary.
fn sort_half_variance(mut items: &mut [HistItem], mut half_variance: f64) -> usize {
    let mut base = 0;
    loop {
        let l = qsort_partition(items);
        let r = l + 1;
        let mut left_sum = 0.0;
        for item in &items[..=l] {
            if left_sum >= half_variance {
                break;
            }
            left_sum += f64::from(item.color_weight);
        }
        if left_sum >= half_variance {
            if l > 0 {
                items = &mut items[..l];
                continue;
            }
            return base;
        }
        half_variance -= left_sum;
        if items.len() > r {
            items = &mut items[r..];
            base += r;
        } else {
            return base + items.len();
        }
    }
}

fn bin_error(bin: &Bin, items: &[HistItem]) -> f64 {
    items[bin.start..bin.start + bin.colors]
        .iter()
        .map(|item| {
            f64::from(color_difference(bin.color, item.color)) * f64::from(item.perceptual_weight)
        })
        .sum()
}

fn total_error_below_target(target: f64, bins: &mut [Bin], items: &[HistItem]) -> bool {
    let mut total = 0.0;
    for bin in bins.iter() {
        // Errors are calculated lazily.
        if bin.total_error >= 0.0 {
            total += bin.total_error;
        }
        if total > target {
            return false;
        }
    }
    for bin in bins.iter_mut() {
        if bin.total_error < 0.0 {
            bin.total_error = bin_error(bin, items);
            total += bin.total_error;
        }
        if total > target {
            return false;
        }
    }
    true
}

/// Weighted average that favours colours far from `center`, which prevents
/// desaturation and faded whites.
pub(super) fn average_pixels(items: &[HistItem], center: FPixel) -> FPixel {
    let (mut new_a, mut sum, mut max_a) = (0.0_f32, 0.0_f32, 0.0_f32);
    for item in items {
        new_a += item.color.a * item.adjusted_weight;
        sum += item.adjusted_weight;
        max_a = max_a.max(item.color.a);
    }
    if sum != 0.0 {
        new_a /= sum;
    }
    if new_a >= 1.0 && max_a >= 255.0 / 256.0 {
        new_a = 1.0;
    }

    let (mut r, mut g, mut b, mut a) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
    sum = 0.0;
    for item in items.iter().rev() {
        let mut px = item.color;
        let dr = center.r - px.r;
        let dg = center.g - px.g;
        let db = center.b - px.b;
        let weight = (1.0 + dr * dr + dg * dg + db * db) * item.adjusted_weight;
        sum += weight;
        if px.a != 0.0 {
            px.r /= px.a;
            px.g /= px.a;
            px.b /= px.a;
        }
        r += px.r * new_a * weight;
        g += px.g * new_a * weight;
        b += px.b * new_a * weight;
        a += new_a * weight;
    }
    if sum != 0.0 {
        a /= sum;
        r /= sum;
        g /= sum;
        b /= sum;
    }
    FPixel::new(a, r, g, b)
}
