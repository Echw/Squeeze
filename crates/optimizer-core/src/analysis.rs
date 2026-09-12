use image::RgbaImage;

use crate::types::{ContentKind, ImageAnalysis};

pub(crate) fn analyze(image: &RgbaImage) -> ImageAnalysis {
    let (width, height) = image.dimensions();
    let stride = ((u64::from(width) * u64::from(height)) / 250_000).max(1) as usize;
    let mut histogram = [0_u32; 256];
    let mut colors = std::collections::HashSet::with_capacity(4096);
    let mut sampled = 0_u32;
    let mut alpha = false;
    let mut edges = 0_u32;
    let mut noisy = 0_u32;
    let mut flat = 0_u32;

    for (index, pixel) in image.pixels().enumerate().step_by(stride) {
        let [r, g, b, a] = pixel.0;
        alpha |= a != 255;
        let luma = ((u16::from(r) * 54 + u16::from(g) * 183 + u16::from(b) * 19) >> 8) as u8;
        histogram[luma as usize] += 1;
        if colors.len() < 65_536 {
            colors.insert((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b));
        }
        sampled += 1;

        let x = (index as u32) % width;
        let y = (index as u32) / width;
        if x > 0 && y > 0 && y < height {
            let left = image.get_pixel(x - 1, y).0;
            let up = image.get_pixel(x, y - 1).0;
            let diff = channel_difference(pixel.0, left).max(channel_difference(pixel.0, up));
            edges += u32::from(diff > 42);
            noisy += u32::from((12..=42).contains(&diff));
            flat += u32::from(diff < 5);
        }
    }

    let entropy = entropy(&histogram, sampled);
    let divisor = sampled.max(1) as f32;
    let edge_density = edges as f32 / divisor;
    let noise = noisy as f32 / divisor;
    let flat_area_ratio = flat as f32 / divisor;
    let estimated_colors = ((colors.len() * stride).min(16_777_216)) as u32;
    let kind = if flat_area_ratio > 0.72 && edge_density > 0.04 {
        ContentKind::Screenshot
    } else if estimated_colors < 2_000 && flat_area_ratio > 0.55 {
        ContentKind::Graphic
    } else if entropy > 6.8 && noise > 0.18 {
        ContentKind::Photo
    } else {
        ContentKind::Mixed
    };

    ImageAnalysis {
        kind,
        entropy,
        estimated_colors,
        edge_density,
        noise,
        flat_area_ratio,
        has_alpha: alpha,
    }
}

fn channel_difference(a: [u8; 4], b: [u8; 4]) -> u8 {
    let sum = u16::from(a[0].abs_diff(b[0]))
        + u16::from(a[1].abs_diff(b[1]))
        + u16::from(a[2].abs_diff(b[2]));
    (sum / 3) as u8
}

fn entropy(histogram: &[u32; 256], total: u32) -> f32 {
    if total == 0 {
        return 0.0;
    }
    histogram
        .iter()
        .filter(|count| **count > 0)
        .map(|count| {
            let probability = *count as f32 / total as f32;
            -probability * probability.log2()
        })
        .sum()
}
