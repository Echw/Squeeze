// Ported from libimagequant 2.4.1; see third_party/libimagequant/COPYRIGHT.

use super::mediancut::Colormap;
use super::pixel::{FPixel, MAX_DIFF, color_difference, min_color_difference};

struct Head {
    /// Colours closer than `radius` to the vantage point have their best
    /// match among the candidates.
    vantage_point: FPixel,
    radius: f32,
    candidate_colors: Vec<FPixel>,
    candidate_indices: Vec<u8>,
}

/// Vantage-point search over a palette of at most 256 colours.
pub(super) struct NearestMap {
    palette: Vec<FPixel>,
    nearest_other_color_distance: Vec<f32>,
    heads: Vec<Head>,
}

impl NearestMap {
    pub(super) fn new(map: &Colormap) -> Self {
        let palette = map
            .entries
            .iter()
            .map(|entry| entry.color)
            .collect::<Vec<_>>();
        let colors = palette.len();
        let subset = map.subset.as_ref().map_or_else(
            || palette[..colors.div_ceil(4)].to_vec(),
            |subset| subset.iter().map(|entry| entry.color).collect(),
        );
        let vantage_points = if colors > 16 {
            (colors / 3).min(subset.len())
        } else {
            0
        };
        let nearest_other_color_distance = (0..colors)
            .map(|i| {
                let mut second_best = MAX_DIFF;
                for (j, other) in palette.iter().enumerate() {
                    if i != j {
                        second_best = second_best.min(color_difference(palette[i], *other));
                    }
                }
                // Half of the squared distance.
                second_best / 4.0
            })
            .collect();

        let mut heads = Vec::with_capacity(vantage_points + 1);
        for (h, &vantage_point) in subset.iter().take(vantage_points).enumerate() {
            let candidates = 1 + colors / ((1 + vantage_points - h) / 2);
            heads.push(build_head(vantage_point, &palette, candidates));
        }
        // The vantage point radius holds only inside the palette's convex
        // hull, so the fallback head searches every colour.
        let mut fallback = build_head(FPixel::default(), &palette, colors);
        fallback.radius = MAX_DIFF;
        heads.push(fallback);

        Self {
            palette,
            nearest_other_color_distance,
            heads,
        }
    }

    /// Returns the palette index and squared difference of the closest colour.
    #[inline]
    pub(super) fn search(&self, px: FPixel, likely_index: usize) -> (usize, f32) {
        let guess_diff = color_difference(self.palette[likely_index], px);
        if guess_diff < self.nearest_other_color_distance[likely_index] {
            return (likely_index, guess_diff);
        }
        for head in &self.heads {
            if color_difference(px, head.vantage_point) <= head.radius {
                let mut best = 0;
                let mut best_diff = color_difference(px, head.candidate_colors[0]);
                for (index, candidate) in head.candidate_colors.iter().enumerate().skip(1) {
                    let diff = color_difference(px, *candidate);
                    if diff < best_diff {
                        best_diff = diff;
                        best = index;
                    }
                }
                return (usize::from(head.candidate_indices[best]), best_diff);
            }
        }
        unreachable!("the fallback head has an unlimited radius")
    }
}

/// libimagequant 2.4.1 also removed colours deep inside earlier heads from
/// later ones, which made the search miss the nearest colour for some
/// translucent pixels. Every head here considers the whole palette, so the
/// search is exact.
fn build_head(px: FPixel, palette: &[FPixel], candidates: usize) -> Head {
    let mut colors = palette
        .iter()
        .enumerate()
        .map(|(index, color)| (color_difference(px, *color), index))
        .collect::<Vec<_>>();
    colors.sort_by(|left, right| left.0.total_cmp(&right.0));
    let count = candidates.min(colors.len());
    let candidate_colors = colors[..count]
        .iter()
        .map(|(_, index)| palette[*index])
        .collect::<Vec<_>>();
    let candidate_indices = colors[..count]
        .iter()
        .map(|(_, index)| *index as u8)
        .collect();
    // Every colour within the radius is a candidate, so no better match lies
    // farther than half of the radius. Alpha requires a pessimistic radius.
    let radius = candidate_colors
        .last()
        .map_or(0.0, |last| min_color_difference(px, *last) / 4.0);
    Head {
        vantage_point: px,
        radius,
        candidate_colors,
        candidate_indices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imagequant::mediancut::PaletteEntry;

    #[test]
    fn matches_brute_force_search() {
        let entries = (0..64)
            .map(|i| {
                // Colours are premultiplied, so no channel exceeds alpha.
                let a = if i % 5 == 0 { 0.5 } else { 1.0 };
                PaletteEntry {
                    color: FPixel::new(
                        a,
                        (i % 4) as f32 / 4.0 * a,
                        (i / 4 % 4) as f32 / 4.0 * a,
                        (i / 16) as f32 / 4.0 * a,
                    ),
                    popularity: 1.0,
                }
            })
            .collect::<Vec<_>>();
        let map = Colormap {
            entries: entries.clone(),
            subset: None,
        };
        let nearest = NearestMap::new(&map);
        let mut state = 7_u32;
        for _ in 0..2_000 {
            let mut next = || {
                state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                (state >> 8) as f32 / (1 << 24) as f32
            };
            let a = next();
            let px = FPixel::new(a, next() * a, next() * a, next() * a);
            let (_, found) = nearest.search(px, 0);
            let best = entries
                .iter()
                .map(|entry| color_difference(px, entry.color))
                .fold(f32::MAX, f32::min);
            assert!(found <= best + 1e-6);
        }
    }
}
