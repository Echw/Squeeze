// Ported from libimagequant 2.4.1; see third_party/libimagequant/COPYRIGHT.

pub(super) const MAX_DIFF: f32 = 1e20;
const INTERNAL_GAMMA: f64 = 0.5499;
/// libimagequant's default for sRGB input.
pub(super) const SRGB_GAMMA: f64 = 0.45455;

/// Premultiplied colour in libimagequant's internal gamma.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct FPixel {
    pub a: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

impl FPixel {
    pub(super) const fn new(a: f32, r: f32, g: f32, b: f32) -> Self {
        Self { a, r, g, b }
    }

    pub(super) fn channel(&self, index: usize) -> f32 {
        match index {
            0 => self.a,
            1 => self.r,
            2 => self.g,
            _ => self.b,
        }
    }
}

pub(super) fn gamma_lut(gamma: f64) -> [f32; 256] {
    std::array::from_fn(|i| (i as f64 / 255.0).powf(INTERNAL_GAMMA / gamma) as f32)
}

pub(super) fn to_f(lut: &[f32; 256], px: [u8; 4]) -> FPixel {
    let a = f32::from(px[3]) / 255.0;
    FPixel {
        a,
        r: lut[px[0] as usize] * a,
        g: lut[px[1] as usize] * a,
        b: lut[px[2] as usize] * a,
    }
}

pub(super) fn to_rgb(gamma: f64, px: FPixel) -> [u8; 4] {
    if px.a < 1.0 / 256.0 {
        return [0, 0, 0, 0];
    }
    let exponent = (gamma / INTERNAL_GAMMA) as f32;
    // 256 because values are in 1..255.9999 and rounded down.
    let channel = |value: f32| {
        let scaled = (value / px.a).powf(exponent) * 256.0;
        if scaled >= 255.0 { 255 } else { scaled as u8 }
    };
    let alpha = px.a * 256.0;
    [
        channel(px.r),
        channel(px.g),
        channel(px.b),
        if alpha >= 255.0 { 255 } else { alpha as u8 },
    ]
}

/// Sum of squared channel differences after blending on black and on white.
/// Premultiplied alpha reduces both blends to this short formula.
#[inline(always)]
pub(super) fn color_difference(px: FPixel, py: FPixel) -> f32 {
    let alphas = py.a - px.a;
    let channel = |x: f32, y: f32| {
        let black = x - y;
        let white = black + alphas;
        black * black + white * white
    };
    channel(px.r, py.r) + channel(px.g, py.g) + channel(px.b, py.b)
}

/// Least possible difference over any background.
pub(super) fn min_color_difference(px: FPixel, py: FPixel) -> f32 {
    let alphas = f64::from(py.a - px.a);
    let channel = |x: f32, y: f32| {
        let black = f64::from(x) - f64::from(y);
        let white = black + alphas;
        (black * black).min(white * white) * 2.0
    };
    (channel(px.r, py.r) + channel(px.g, py.g) + channel(px.b, py.b)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_round_trip_keeps_bytes() {
        let lut = gamma_lut(SRGB_GAMMA);
        for px in [
            [0, 0, 0, 255],
            [12, 200, 99, 255],
            [255, 255, 255, 255],
            [80, 40, 20, 128],
        ] {
            let back = to_rgb(SRGB_GAMMA, to_f(&lut, px));
            for channel in 0..4 {
                assert!(
                    back[channel].abs_diff(px[channel]) <= 1,
                    "{px:?} -> {back:?}"
                );
            }
        }
    }

    #[test]
    fn difference_counts_alpha_on_both_backgrounds() {
        let lut = gamma_lut(SRGB_GAMMA);
        let opaque = to_f(&lut, [255, 255, 255, 255]);
        let transparent = to_f(&lut, [255, 255, 255, 0]);
        assert!(color_difference(opaque, transparent) > 0.9);
        assert_eq!(color_difference(opaque, opaque), 0.0);
    }
}
