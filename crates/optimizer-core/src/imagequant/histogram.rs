// Ported from libimagequant 2.4.1; see third_party/libimagequant/COPYRIGHT.

use super::pixel::{FPixel, to_f};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct HistItem {
    pub color: FPixel,
    /// Perceptual weight changed by the feedback loop to steer median cut.
    pub adjusted_weight: f32,
    /// Pixel count weighted by the importance map.
    pub perceptual_weight: f32,
    pub color_weight: f32,
    pub sort_value: u32,
    pub likely_index: u8,
}

pub(super) struct Histogram {
    pub items: Vec<HistItem>,
    pub total_perceptual_weight: f64,
}

/// Counts colours, weighting each pixel by `0.5 + importance`. When there are
/// more than `max_entries` colours, low bits are dropped until they fit.
/// Bucket order mirrors libimagequant so median cut sees the same sequence.
pub(super) fn build(
    rgba: &[u8],
    width: u32,
    height: u32,
    importance: Option<&[u8]>,
    max_entries: usize,
    lut: &[f32; 256],
) -> Histogram {
    let surface = width as usize * height as usize;
    let mut ignorebits = 0_u32;
    loop {
        if let Some(buckets) = count_colors(rgba, surface, importance, max_entries, ignorebits) {
            return into_histogram(buckets, surface, lut);
        }
        ignorebits += 1;
    }
}

fn count_colors(
    rgba: &[u8],
    surface: usize,
    importance: Option<&[u8]>,
    max_entries: usize,
    ignorebits: u32,
) -> Option<Vec<Vec<(u32, f32)>>> {
    let estimated =
        max_entries.min(surface / (ignorebits as usize + if surface > 512 * 512 { 5 } else { 4 }));
    let hash_size = if estimated < 66_000 {
        6673
    } else if estimated < 200_000 {
        12_011
    } else {
        24_019
    };
    let channel_mask = 255_u32 >> ignorebits << ignorebits;
    let channel_high_mask = (255_u32 >> ignorebits) ^ 0xff;
    let posterize_mask = channel_mask << 24 | channel_mask << 16 | channel_mask << 8 | channel_mask;
    let posterize_high_mask = channel_high_mask << 24
        | channel_high_mask << 16
        | channel_high_mask << 8
        | channel_high_mask;

    let mut buckets = vec![Vec::<(u32, f32)>::new(); hash_size];
    let mut colors = 0_usize;
    for (index, px) in rgba.as_chunks::<4>().0.iter().enumerate() {
        let boost = importance.map_or(1.0, |map| (0.5 + f64::from(map[index]) / 255.0) as f32);
        let (key, hash) = if px[3] == 0 {
            // Transparent pixels with any RGB are one colour.
            (0, 0)
        } else {
            let raw = u32::from_le_bytes([px[0], px[1], px[2], px[3]]);
            let key = (raw & posterize_mask) | ((raw & posterize_high_mask) >> (8 - ignorebits));
            (key, key as usize % hash_size)
        };
        let bucket = &mut buckets[hash];
        if let Some(entry) = bucket.iter_mut().find(|entry| entry.0 == key) {
            entry.1 += boost;
            continue;
        }
        colors += 1;
        if colors > max_entries {
            return None;
        }
        bucket.push((key, boost));
    }
    Some(buckets)
}

fn into_histogram(buckets: Vec<Vec<(u32, f32)>>, surface: usize, lut: &[f32; 256]) -> Histogram {
    // One colour may carry at most a tenth of the surface so it cannot
    // dominate every other colour.
    let max_perceptual_weight = 0.1_f32 * surface as f32;
    let mut total = 0.0_f64;
    let items = buckets
        .into_iter()
        .flatten()
        .map(|(key, weight)| {
            let weight = weight.min(max_perceptual_weight);
            total += f64::from(weight);
            HistItem {
                color: to_f(lut, key.to_le_bytes()),
                adjusted_weight: weight,
                perceptual_weight: weight,
                ..HistItem::default()
            }
        })
        .collect();
    Histogram {
        items,
        total_perceptual_weight: total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imagequant::pixel::{SRGB_GAMMA, gamma_lut};

    #[test]
    fn transparent_pixels_share_one_entry_and_weights_are_capped() {
        let rgba = [
            [10, 20, 30, 0],
            [200, 100, 0, 0],
            [1, 2, 3, 255],
            [1, 2, 3, 255],
        ]
        .concat();
        let histogram = build(&rgba, 2, 2, None, 1 << 17, &gamma_lut(SRGB_GAMMA));
        assert_eq!(histogram.items.len(), 2);
        assert!(
            histogram
                .items
                .iter()
                .all(|item| item.perceptual_weight <= 0.4)
        );
    }

    #[test]
    fn too_many_colours_drop_low_bits() {
        let rgba = (0..=255_u8)
            .flat_map(|v| [v, 0, 0, 255])
            .collect::<Vec<_>>();
        let histogram = build(&rgba, 256, 1, None, 100, &gamma_lut(SRGB_GAMMA));
        assert!(!histogram.items.is_empty() && histogram.items.len() <= 100);
    }
}
