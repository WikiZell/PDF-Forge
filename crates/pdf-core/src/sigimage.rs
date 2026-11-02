//! Signature image preparation: decode PNG/JPEG, remove a white-ish background, crop to content.

use std::fmt;
use std::io::Cursor;

use image::{DynamicImage, ImageFormat, RgbaImage};

/// Reject images larger than this many pixels before decoding (decompression-bomb guard).
pub const MAX_PIXELS: u64 = 40_000_000;
/// Imported images are downscaled so their longest side is at most this.
pub const MAX_DIMENSION: u32 = 1600;

#[derive(Debug, PartialEq, Eq)]
pub enum ImageError {
    Decode(String),
    TooLarge,
    Empty,
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "The image could not be read: {e}"),
            Self::TooLarge => write!(f, "The image is too large."),
            Self::Empty => write!(f, "No signature content was found. Try raising the threshold."),
        }
    }
}
impl std::error::Error for ImageError {}

/// Decode PNG or JPEG bytes to RGBA, downscaling very large images.
pub fn decode(bytes: &[u8]) -> Result<RgbaImage, ImageError> {
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| ImageError::Decode(e.to_string()))?;
    match reader.format() {
        Some(ImageFormat::Png) | Some(ImageFormat::Jpeg) => {}
        _ => return Err(ImageError::Decode("only PNG and JPEG are supported".into())),
    }
    let (w, h) = reader.into_dimensions().map_err(|e| ImageError::Decode(e.to_string()))?;
    if (w as u64) * (h as u64) > MAX_PIXELS {
        return Err(ImageError::TooLarge);
    }
    let img = image::load_from_memory(bytes).map_err(|e| ImageError::Decode(e.to_string()))?;
    let img = if w.max(h) > MAX_DIMENSION {
        img.resize(MAX_DIMENSION, MAX_DIMENSION, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    Ok(img.to_rgba8())
}

/// Make pixels brighter than `threshold` (0-255 luminance) transparent, with a soft edge, then crop
/// to the remaining content plus a small margin. Existing transparency is preserved.
pub fn remove_background(img: &RgbaImage, threshold: u8) -> Result<RgbaImage, ImageError> {
    let t = threshold as f32;
    let feather = 24.0_f32.min(t.max(1.0));
    let mut out = img.clone();
    let (mut minx, mut miny, mut maxx, mut maxy) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for (x, y, p) in out.enumerate_pixels_mut() {
        let lum = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
        let keep = ((t - lum) / feather).clamp(0.0, 1.0);
        let a = (p[3] as f32 * keep).round() as u8;
        p[3] = a;
        if a > 8 {
            minx = minx.min(x);
            miny = miny.min(y);
            maxx = maxx.max(x);
            maxy = maxy.max(y);
        }
    }
    if minx == u32::MAX {
        return Err(ImageError::Empty);
    }
    let pad = 2;
    let x0 = minx.saturating_sub(pad);
    let y0 = miny.saturating_sub(pad);
    let x1 = (maxx + pad).min(out.width() - 1);
    let y1 = (maxy + pad).min(out.height() - 1);
    Ok(image::imageops::crop_imm(&out, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image())
}

/// Crop transparent margins only (for already-transparent PNGs such as typed signatures).
pub fn crop_to_content(img: &RgbaImage) -> Result<RgbaImage, ImageError> {
    let (mut minx, mut miny, mut maxx, mut maxy) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > 8 {
            minx = minx.min(x);
            miny = miny.min(y);
            maxx = maxx.max(x);
            maxy = maxy.max(y);
        }
    }
    if minx == u32::MAX {
        return Err(ImageError::Empty);
    }
    Ok(image::imageops::crop_imm(img, minx, miny, maxx - minx + 1, maxy - miny + 1).to_image())
}

pub fn encode_png(img: &RgbaImage) -> Vec<u8> {
    let mut v = Vec::new();
    DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut Cursor::new(&mut v), ImageFormat::Png)
        .expect("PNG encoding to memory cannot fail");
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn page_with_ink() -> RgbaImage {
        // 100x60 white page, dark 20x10 "signature" at (30,20)
        let mut im = RgbaImage::from_pixel(100, 60, Rgba([255, 255, 255, 255]));
        for y in 20..30 {
            for x in 30..50 {
                im.put_pixel(x, y, Rgba([10, 10, 80, 255]));
            }
        }
        im
    }

    #[test]
    fn background_becomes_transparent_and_image_is_cropped() {
        let out = remove_background(&page_with_ink(), 200).unwrap();
        assert!(out.width() <= 24 && out.height() <= 14, "{}x{}", out.width(), out.height());
        assert!(out.width() >= 20 && out.height() >= 10);
        // corner is background => transparent; centre is ink => opaque
        assert_eq!(out.get_pixel(0, 0)[3], 0);
        assert_eq!(out.get_pixel(out.width() / 2, out.height() / 2)[3], 255);
    }

    #[test]
    fn threshold_controls_what_is_kept() {
        let mut im = page_with_ink();
        for x in 60..70 { for y in 20..30 { im.put_pixel(x, y, Rgba([180, 180, 180, 255])); } } // light grey smudge
        let strict = remove_background(&im, 100).unwrap(); // only dark ink survives
        let loose = remove_background(&im, 230).unwrap(); // grey survives too
        assert!(loose.width() > strict.width() + 10, "{} vs {}", loose.width(), strict.width());
    }

    #[test]
    fn all_white_is_reported_as_empty() {
        let im = RgbaImage::from_pixel(10, 10, Rgba([255, 255, 255, 255]));
        assert_eq!(remove_background(&im, 200), Err(ImageError::Empty));
    }

    #[test]
    fn existing_transparency_is_preserved() {
        let mut im = RgbaImage::from_pixel(10, 10, Rgba([0, 0, 0, 0]));
        im.put_pixel(5, 5, Rgba([0, 0, 0, 255]));
        let out = remove_background(&im, 200).unwrap();
        assert_eq!(out.pixels().filter(|p| p[3] == 255).count(), 1);
    }

    #[test]
    fn png_roundtrip_and_decode_guards() {
        let png = encode_png(&page_with_ink());
        let back = decode(&png).unwrap();
        assert_eq!(back.dimensions(), (100, 60));
        assert!(matches!(decode(b"GIF89a....."), Err(ImageError::Decode(_))));
        assert!(matches!(decode(b""), Err(ImageError::Decode(_))));
        assert!(matches!(decode(&png[..20]), Err(ImageError::Decode(_))));
    }

    #[test]
    fn large_images_are_downscaled() {
        let big = RgbaImage::from_pixel(3200, 800, Rgba([0, 0, 0, 255]));
        let back = decode(&encode_png(&big)).unwrap();
        assert_eq!(back.width(), MAX_DIMENSION);
        assert_eq!(back.height(), 400);
    }
}
