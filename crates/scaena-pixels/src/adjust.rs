//! An image's light and color adjusted (PLAN 2.117), as a presentation app's Adjust Image
//! adjusts a picture: exposure, contrast, saturation, and warmth. The same bytes on every
//! target: `f32` arithmetic, which is IEEE's everywhere, over tables made by `libm`.
//!
//! Exposure, warmth, and saturation work in linear light, as a camera's would: exposure scales
//! the light, warmth scales red and blue apart as a white balance does, and saturation moves
//! each color toward or away from its luminance. Contrast works on the sRGB values, about their
//! middle, as the eye judges lights and darks. Alpha is left as it is: a picture is straight
//! (unpremultiplied) sRGB RGBA8, before and after.

use std::sync::OnceLock;

/// How a picture is adjusted, each in thousandths from -1000 to 1000, where 0 leaves it as it
/// is. Whole numbers, so an adjustment names its pixels exactly ([`Adjust::name`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Adjust {
    /// 1000 is two stops brighter, -1000 two darker.
    pub exposure: i16,
    /// -1000 is a flat gray, 1000 twice the contrast.
    pub contrast: i16,
    /// -1000 is black and white, 1000 twice the color.
    pub saturation: i16,
    /// 1000 is warmer, toward orange; -1000 cooler, toward blue.
    pub warmth: i16,
}

/// What follows an image's own asset id to name its adjusted pixels: `sha256:…+adjust(…)`.
const MARK: &str = "+adjust(";

/// Linear light's steps in the table that encodes it as sRGB: enough that each of sRGB's 256
/// values has a step of its own.
const STEPS: usize = 16384;

impl Adjust {
    /// From fractions of the most each takes, -1 to 1, as a deck writes them: in thousandths,
    /// clamped, a value that is not a number taken as 0.
    pub fn from_fractions(exposure: f64, contrast: f64, saturation: f64, warmth: f64) -> Adjust {
        let whole = |v: f64| if v.is_finite() { (v.clamp(-1.0, 1.0) * 1000.0).round() as i16 } else { 0 };
        Adjust {
            exposure: whole(exposure),
            contrast: whole(contrast),
            saturation: whole(saturation),
            warmth: whole(warmth),
        }
    }

    /// Whether it leaves a picture as it is.
    pub fn is_none(&self) -> bool {
        *self == Adjust::default()
    }

    /// `a` to `b`, `p` of the way, in whole thousandths.
    pub fn lerp(a: Adjust, b: Adjust, p: f32) -> Adjust {
        let mix = |x: i16, y: i16| (f32::from(x) + (f32::from(y) - f32::from(x)) * p).round() as i16;
        Adjust {
            exposure: mix(a.exposure, b.exposure),
            contrast: mix(a.contrast, b.contrast),
            saturation: mix(a.saturation, b.saturation),
            warmth: mix(a.warmth, b.warmth),
        }
    }

    /// The asset id of image `id`'s pixels adjusted so, as a display list names them:
    /// `id` itself where it leaves them as they are.
    pub fn name(&self, id: &str) -> String {
        if self.is_none() {
            return id.to_string();
        }
        let Adjust { exposure, contrast, saturation, warmth } = self;
        format!("{id}{MARK}{exposure},{contrast},{saturation},{warmth})")
    }

    /// An asset id read back: the image's own id and how its pixels are adjusted, or `None`
    /// for an id that names no adjustment.
    pub fn named(asset: &str) -> Option<(&str, Adjust)> {
        let (id, rest) = asset.split_once(MARK)?;
        let mut values = rest.strip_suffix(')')?.split(',').map(|v| v.parse::<i16>().ok().filter(|v| v.abs() <= 1000));
        let mut next = || values.next().flatten();
        let adjust = Adjust { exposure: next()?, contrast: next()?, saturation: next()?, warmth: next()? };
        values.next().is_none().then_some((id, adjust))
    }

    /// `rgba`, straight sRGB RGBA8, adjusted in place.
    pub fn apply(&self, rgba: &mut [u8]) {
        if self.is_none() {
            return;
        }
        let (linear, encode) = tables();
        let fraction = |v: i16| f32::from(v) / 1000.0;
        // Two stops either way: 2^(2e).
        let gain = libm::exp2f(2.0 * fraction(self.exposure));
        let w = fraction(self.warmth);
        let [kr, kg, kb] = [gain * (1.0 + 0.25 * w), gain * (1.0 + 0.05 * w), gain * (1.0 - 0.25 * w)];
        let chroma = 1.0 + fraction(self.saturation);
        let slope = 1.0 + fraction(self.contrast);
        let contrast: [u8; 256] = std::array::from_fn(|v| {
            let c = (v as f32 / 255.0 - 0.5) * slope + 0.5;
            (c.clamp(0.0, 1.0) * 255.0).round() as u8
        });
        let last = (STEPS - 1) as f32;
        let out = |c: f32| contrast[usize::from(encode[(c.clamp(0.0, 1.0) * last).round() as usize])];
        for px in rgba.as_chunks_mut::<4>().0 {
            let [r, g, b] = [px[0], px[1], px[2]].map(|v| linear[usize::from(v)]);
            let [r, g, b] = [r * kr, g * kg, b * kb];
            let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let [r, g, b] = [r, g, b].map(|c| y + (c - y) * chroma);
            px[0] = out(r);
            px[1] = out(g);
            px[2] = out(b);
        }
    }
}

/// sRGB's 256 values in linear light, and linear light's [`STEPS`] encoded as sRGB, made once.
fn tables() -> &'static ([f32; 256], Vec<u8>) {
    static TABLES: OnceLock<([f32; 256], Vec<u8>)> = OnceLock::new();
    TABLES.get_or_init(|| {
        let linear = std::array::from_fn(|v| {
            let c = v as f32 / 255.0;
            if c <= 0.04045 { c / 12.92 } else { libm::powf((c + 0.055) / 1.055, 2.4) }
        });
        let encode = (0..STEPS)
            .map(|i| {
                let l = i as f32 / (STEPS - 1) as f32;
                let c = if l <= 0.003_130_8 { l * 12.92 } else { 1.055 * libm::powf(l, 1.0 / 2.4) - 0.055 };
                (c.clamp(0.0, 1.0) * 255.0).round() as u8
            })
            .collect();
        (linear, encode)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture() -> Vec<u8> {
        // A ramp of grays, a red, a green, a blue, an orange, and a half-seen sky blue.
        let mut px: Vec<u8> = (0..=255u8).step_by(15).flat_map(|v| [v, v, v, 255]).collect();
        px.extend([200, 40, 30, 255, 40, 180, 60, 255, 30, 50, 210, 255, 240, 140, 20, 255, 90, 160, 230, 128]);
        px
    }

    fn adjusted(adjust: Adjust) -> Vec<u8> {
        let mut px = picture();
        adjust.apply(&mut px);
        px
    }

    #[test]
    fn none_leaves_the_picture_and_its_id_as_they_are() {
        assert_eq!(adjusted(Adjust::default()), picture());
        assert_eq!(Adjust::default().name("sha256:ab"), "sha256:ab");
        assert_eq!(Adjust::named("sha256:ab"), None);
    }

    #[test]
    fn an_adjustment_names_its_pixels_and_is_read_back() {
        let adjust = Adjust::from_fractions(0.25, -1.0, 0.0, 0.5);
        assert_eq!(adjust, Adjust { exposure: 250, contrast: -1000, saturation: 0, warmth: 500 });
        let name = adjust.name("sha256:ab");
        assert_eq!(name, "sha256:ab+adjust(250,-1000,0,500)");
        assert_eq!(Adjust::named(&name), Some(("sha256:ab", adjust)));
        for bad in [
            "sha256:ab+adjust(1,2,3)",
            "sha256:ab+adjust(1,2,3,4,5)",
            "sha256:ab+adjust(1,2,3,1001)",
            "x+adjust(a,b,c,d)",
        ] {
            assert_eq!(Adjust::named(bad), None, "{bad}");
        }
        assert_eq!(Adjust::from_fractions(2.0, f64::NAN, -3.0, 0.0004).name("i"), "i+adjust(1000,0,-1000,0)");
    }

    #[test]
    fn saturation_at_its_least_is_black_and_white() {
        let px = adjusted(Adjust::from_fractions(0.0, 0.0, -1.0, 0.0));
        for p in px.as_chunks::<4>().0 {
            assert!(p[0].abs_diff(p[1]) <= 1 && p[1].abs_diff(p[2]) <= 1, "{p:?}");
        }
        // Each keeps its lightness: the red is a darker gray than the orange.
        let gray = |i: usize| px[i * 4];
        let (red, orange) = (picture().len() / 4 - 5, picture().len() / 4 - 2);
        assert!(gray(red) < gray(orange), "{} {}", gray(red), gray(orange));
        // Alpha is as it was.
        assert_eq!(px[px.len() - 1], 128);
    }

    #[test]
    fn exposure_brightens_or_darkens_and_keeps_black_and_white() {
        let (bright, dark) = (
            adjusted(Adjust::from_fractions(0.5, 0.0, 0.0, 0.0)),
            adjusted(Adjust::from_fractions(-0.5, 0.0, 0.0, 0.0)),
        );
        let original = picture();
        for i in (0..original.len()).step_by(4).take(18) {
            let v = original[i];
            assert!(bright[i] >= v && dark[i] <= v, "{v}: {} {}", bright[i], dark[i]);
            if v > 10 && v < 200 {
                assert!(bright[i] > v && dark[i] < v, "{v}: {} {}", bright[i], dark[i]);
            }
        }
        assert_eq!((bright[0], dark[0]), (0, 0), "black stays black");
        // One stop up doubles the light: sRGB's 119 is 0.185 in linear light, and 0.369 is 164.
        let mut one = vec![119, 119, 119, 255];
        Adjust::from_fractions(0.5, 0.0, 0.0, 0.0).apply(&mut one);
        assert!(one[0].abs_diff(164) <= 1, "{one:?}");
    }

    #[test]
    fn contrast_spreads_the_lights_and_darks_or_flattens_them() {
        let flat = adjusted(Adjust::from_fractions(0.0, -1.0, 0.0, 0.0));
        assert!(flat.as_chunks::<4>().0.iter().all(|p| p[..3].iter().all(|&c| c.abs_diff(128) <= 1)), "{flat:?}");
        let more = adjusted(Adjust::from_fractions(0.0, 0.5, 0.0, 0.0));
        let original = picture();
        assert!(more[4 * 2] < original[4 * 2] && more[4 * 15] > original[4 * 15], "darks darker, lights lighter");
    }

    #[test]
    fn warmth_moves_white_toward_orange_or_blue() {
        let mut warm = vec![200, 200, 200, 255];
        Adjust::from_fractions(0.0, 0.0, 0.0, 1.0).apply(&mut warm);
        assert!(warm[0] > 200 && warm[2] < 200 && warm[0] > warm[1] && warm[1] > warm[2], "{warm:?}");
        let mut cool = vec![200, 200, 200, 255];
        Adjust::from_fractions(0.0, 0.0, 0.0, -1.0).apply(&mut cool);
        assert!(cool[2] > 200 && cool[0] < 200, "{cool:?}");
    }

    #[test]
    fn halfway_is_half_of_each() {
        let a = Adjust::from_fractions(0.0, -1.0, 0.5, 0.2);
        let b = Adjust::from_fractions(1.0, 0.0, -0.5, 0.2);
        assert_eq!(Adjust::lerp(a, b, 0.5), Adjust { exposure: 500, contrast: -500, saturation: 0, warmth: 200 });
        assert_eq!(Adjust::lerp(a, b, 0.0), a);
        assert_eq!(Adjust::lerp(a, b, 1.0), b);
    }
}
