// Ported from libimagequant 2.4.1; see third_party/libimagequant/COPYRIGHT.

use super::pixel::FPixel;

/// Above this size the maps would cost too much memory in a browser Worker.
const MAX_MAP_BYTES: usize = 1 << 26;

pub(super) struct ContrastMaps {
    /// 255 = flat, 0 = high-frequency noise; straight edges are not noise.
    pub noise: Vec<u8>,
    /// The noise map with every edge included.
    pub edges: Vec<u8>,
}

/// `row` fills the buffer with one image row in the quantizer's colour space.
pub(super) fn contrast_maps(
    width: usize,
    height: usize,
    mut row: impl FnMut(usize, &mut [FPixel]),
) -> Option<ContrastMaps> {
    if width < 4 || height < 4 || 3 * width * height > MAX_MAP_BYTES {
        return None;
    }
    let mut noise = vec![0_u8; width * height];
    let mut edges = vec![0_u8; width * height];
    let mut tmp = vec![0_u8; width * height];

    let mut prev_row = vec![FPixel::default(); width];
    let mut curr_row = vec![FPixel::default(); width];
    let mut next_row = vec![FPixel::default(); width];
    row(0, &mut next_row);
    curr_row.copy_from_slice(&next_row);
    for j in 0..height {
        std::mem::swap(&mut prev_row, &mut curr_row);
        std::mem::swap(&mut curr_row, &mut next_row);
        if j == 0 {
            prev_row.copy_from_slice(&curr_row);
        }
        row((height - 1).min(j + 1), &mut next_row);

        let mut curr = curr_row[0];
        let mut next = curr;
        for i in 0..width {
            let prev = curr;
            curr = next;
            next = curr_row[(width - 1).min(i + 1)];

            // Contrast between horizontal and vertical neighbours.
            let horizontal = second_difference(prev, curr, next);
            let vertical = second_difference(prev_row[i], curr, next_row[i]);
            let edge = horizontal.max(vertical);
            let mut z = edge - (horizontal - vertical).abs() * 0.5;
            z = 1.0 - z.max(horizontal.min(vertical));
            // Noise is amplified.
            z *= z;
            z *= z;
            noise[j * width + i] = to_byte(z * 256.0);
            edges[j * width + i] = to_byte((1.0 - edge) * 256.0);
        }
    }

    // Shrinking and expanding the noise map removes thin edges from it.
    max3(&noise, &mut tmp, width, height);
    max3(&tmp, &mut noise, width, height);
    blur(&mut noise, &mut tmp, width, height, 3);
    max3(&noise, &mut tmp, width, height);
    min3(&tmp, &mut noise, width, height);
    min3(&noise, &mut tmp, width, height);
    min3(&tmp, &mut noise, width, height);

    min3(&edges, &mut tmp, width, height);
    max3(&tmp, &mut edges, width, height);
    for (edge, noise) in edges.iter_mut().zip(&noise) {
        *edge = (*edge).min(*noise);
    }
    Some(ContrastMaps { noise, edges })
}

fn second_difference(prev: FPixel, curr: FPixel, next: FPixel) -> f32 {
    let a = (prev.a + next.a - curr.a * 2.0).abs();
    let r = (prev.r + next.r - curr.r * 2.0).abs();
    let g = (prev.g + next.g - curr.g * 2.0).abs();
    let b = (prev.b + next.b - curr.b * 2.0).abs();
    a.max(r).max(g.max(b))
}

fn to_byte(value: f32) -> u8 {
    if value < 256.0 { value as u8 } else { 255 }
}

fn neighbourhood(
    src: &[u8],
    dst: &mut [u8],
    width: usize,
    height: usize,
    pick: impl Fn(u8, u8) -> u8,
) {
    let mut out = 0;
    for j in 0..height {
        let row = &src[j * width..(j + 1) * width];
        let prev_row = &src[j.saturating_sub(1) * width..][..width];
        let next_row = &src[(height - 1).min(j + 1) * width..][..width];
        let mut curr = row[0];
        let mut next = row[0];
        for i in 0..width - 1 {
            let prev = curr;
            curr = next;
            next = row[i + 1];
            dst[out] = pick(curr, pick(pick(prev, next), pick(next_row[i], prev_row[i])));
            out += 1;
        }
        dst[out] = pick(
            pick(curr, next),
            pick(next_row[width - 1], prev_row[width - 1]),
        );
        out += 1;
    }
}

fn max3(src: &[u8], dst: &mut [u8], width: usize, height: usize) {
    neighbourhood(src, dst, width, height, u8::max);
}

fn min3(src: &[u8], dst: &mut [u8], width: usize, height: usize) {
    neighbourhood(src, dst, width, height, u8::min);
}

/// Box blur of radius `size`, in place, using `tmp` for the transposed pass.
fn blur(image: &mut [u8], tmp: &mut [u8], width: usize, height: usize, size: usize) {
    if width < 2 * size + 1 || height < 2 * size + 1 {
        return;
    }
    transposing_blur(image, tmp, width, height, size);
    transposing_blur(tmp, image, height, width, size);
}

/// Blurs rows horizontally and writes the result transposed.
fn transposing_blur(src: &[u8], dst: &mut [u8], width: usize, height: usize, size: usize) {
    let divisor = (size * 2) as u32;
    for j in 0..height {
        let row = &src[j * width..(j + 1) * width];
        let mut sum = u32::from(row[0]) * size as u32;
        for value in &row[..size] {
            sum += u32::from(*value);
        }
        for i in 0..size {
            sum -= u32::from(row[0]);
            sum += u32::from(row[i + size]);
            dst[i * height + j] = (sum / divisor) as u8;
        }
        for i in size..width - size {
            sum -= u32::from(row[i - size]);
            sum += u32::from(row[i + size]);
            dst[i * height + j] = (sum / divisor) as u8;
        }
        for i in width - size..width {
            sum -= u32::from(row[i - size]);
            sum += u32::from(row[width - 1]);
            dst[i * height + j] = (sum / divisor) as u8;
        }
    }
}
