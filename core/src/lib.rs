//! # paintify-core
//!
//! The heart of the MS Paintify pipeline. Takes any image, crunches it
//! through a series of destructive, nostalgia-maximizing transformations
//! to make it look like a Windows 95 MS Paint masterpiece.
//!
//! Pipeline: Downscale (Nearest-Neighbor) → Color Quantize → Upscale (Nearest-Neighbor)

use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use image::imageops::FilterType;

// ---------------------------------------------------------------------------
// The iconic MS Paint 16-color palette (Windows 95/98 default)
// ---------------------------------------------------------------------------
const MS_PAINT_PALETTE: [[u8; 3]; 16] = [
    [0, 0, 0],         // Black
    [255, 255, 255],   // White
    [128, 128, 128],   // Gray
    [192, 192, 192],   // Silver (light gray)
    [128, 0, 0],       // Maroon
    [255, 0, 0],       // Red
    [128, 128, 0],     // Olive
    [255, 255, 0],     // Yellow
    [0, 128, 0],       // Green
    [0, 255, 0],       // Lime
    [0, 128, 128],     // Teal
    [0, 255, 255],     // Aqua (Cyan)
    [0, 0, 128],       // Navy
    [0, 0, 255],       // Blue
    [128, 0, 128],     // Purple
    [255, 0, 255],     // Fuchsia (Magenta)
];

/// The extended MS Paint 28-color palette including custom color slots.
/// Use this via `PaintConfig::extended_palette(true)`.
const MS_PAINT_EXTENDED: [[u8; 3]; 28] = [
    [0, 0, 0],
    [255, 255, 255],
    [128, 128, 128],
    [192, 192, 192],
    [128, 0, 0],
    [255, 0, 0],
    [128, 128, 0],
    [255, 255, 0],
    [0, 128, 0],
    [0, 255, 0],
    [0, 128, 128],
    [0, 255, 255],
    [0, 0, 128],
    [0, 0, 255],
    [128, 0, 128],
    [255, 0, 255],
    // Extended colors (custom color slots in MS Paint)
    [128, 64, 0],      // Brown
    [255, 128, 64],    // Orange
    [0, 64, 0],        // Dark Green
    [128, 255, 128],   // Light Green
    [0, 64, 128],      // Dark Blue
    [128, 128, 255],   // Light Blue
    [128, 0, 64],      // Dark Purple
    [255, 128, 192],   // Pink
    [64, 64, 64],      // Dark Gray
    [255, 128, 128],   // Light Red
    [255, 255, 128],   // Light Yellow
    [128, 255, 255],   // Light Cyan
];

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Controls the degree of "crunch" applied to the image.
pub struct PaintConfig {
    /// How many times to divide resolution before quantizing.
    /// Higher = chunkier pixels. Default: 4.
    pub downscale_factor: u32,
    /// Use the extended 28-color palette instead of classic 16.
    pub extended_palette: bool,
    /// Apply Bayer ordered dithering (requires `dithering` feature).
    pub dithering: bool,
    /// Preserve original image dimensions (upscale back after processing).
    pub preserve_aspect: bool,
}

impl Default for PaintConfig {
    fn default() -> Self {
        Self {
            downscale_factor: 4,
            extended_palette: false,
            dithering: false,
            preserve_aspect: true,
        }
    }
}

impl PaintConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn chunky(mut self, factor: u32) -> Self {
        self.downscale_factor = factor;
        self
    }

    pub fn extended_palette(mut self, yes: bool) -> Self {
        self.extended_palette = yes;
        self
    }
}

// ---------------------------------------------------------------------------
// Color quantization
// ---------------------------------------------------------------------------

/// Find the closest palette color using Euclidean distance in RGB space.
fn closest_color(pixel: &Rgba<u8>, palette: &[[u8; 3]]) -> Rgba<u8> {
    let mut min_dist = u32::MAX;
    let mut best = palette[0]; // fallback

    for color in palette {
        let dr = pixel[0] as i32 - color[0] as i32;
        let dg = pixel[1] as i32 - color[1] as i32;
        let db = pixel[2] as i32 - color[2] as i32;
        // Squared Euclidean — no need for sqrt, monotonic
        let dist = (dr * dr + dg * dg + db * db) as u32;

        if dist < min_dist {
            min_dist = dist;
            best = *color;
        }
    }

    Rgba([best[0], best[1], best[2], pixel[3]])
}

/// Quantize every pixel to the nearest palette entry.
fn quantize_colors(buf: &mut RgbaImage, palette: &[[u8; 3]]) {
    for pixel in buf.pixels_mut() {
        *pixel = closest_color(pixel, palette);
    }
}

// ---------------------------------------------------------------------------
// Ordered dithering (Bayer 4x4 matrix)
// ---------------------------------------------------------------------------

#[cfg(feature = "dithering")]
mod dither {
    /// Bayer 4x4 ordered dithering matrix, scaled to 0..15.
    const BAYER_4X4: [[u8; 4]; 4] = [
        [0, 8, 2, 10],
        [12, 4, 14, 6],
        [3, 11, 1, 9],
        [15, 7, 13, 5],
    ];

    /// Apply Bayer ordered dithering to an RGBA buffer using a given palette.
    pub fn ordered_dither(
        buf: &mut image::RgbaImage,
        palette: &[[u8; 3]],
    ) {
        let (w, h) = buf.dimensions();
        for y in 0..h {
            for x in 0..w {
                let px = buf.get_pixel(x, y);
                let threshold = BAYER_4X4[(y % 4) as usize][(x % 4) as usize] as f32 / 16.0;

                let r = (px[0] as f32 / 255.0 + threshold).clamp(0.0, 1.0);
                let g = (px[1] as f32 / 255.0 + threshold).clamp(0.0, 1.0);
                let b = (px[2] as f32 / 255.0 + threshold).clamp(0.0, 1.0);

                let q = closest_color(
                    &image::Rgba([(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8, px[3]]),
                    palette,
                );
                buf.put_pixel(x, y, q);
            }
        }
    }
}

#[cfg(feature = "dithering")]
pub use dither::ordered_dither;

// ---------------------------------------------------------------------------
// Main pipeline
// ---------------------------------------------------------------------------

/// Run the full Paintify pipeline on an image.
///
/// 1. Downscale with **Nearest-Neighbor** (creates hard pixel edges — no blurring)
/// 2. Quantize every pixel to the MS Paint palette
/// 3. Optionally apply Bayer dithering
/// 4. Upscale back with **Nearest-Neighbor** (preserves crunchy pixels at original size)
pub fn paintify(img: &DynamicImage, config: &PaintConfig) -> DynamicImage {
    let (orig_w, orig_h) = img.dimensions();

    let palette: &[[u8; 3]] = if config.extended_palette {
        &MS_PAINT_EXTENDED
    } else {
        &MS_PAINT_PALETTE
    };

    // Step 1: Crush resolution — Nearest-Neighbor is non-negotiable for the crunch
    let new_w = orig_w / config.downscale_factor;
    let new_h = orig_h / config.downscale_factor;
    let small = img.resize_exact(new_w, new_h, FilterType::Nearest);

    // Step 2: Convert to RGBA buffer
    let mut buf = small.to_rgba8();

    // Step 3: Color quantization
    if config.dithering {
        #[cfg(feature = "dithering")]
        ordered_dither(&mut buf, palette);
        #[cfg(not(feature = "dithering"))]
        quantize_colors(&mut buf, palette);
    } else {
        quantize_colors(&mut buf, palette);
    }

    // Step 4: Blow it back up — Nearest-Neighbor keeps every pixel a hard square
    if config.preserve_aspect {
        DynamicImage::ImageRgba8(buf).resize_exact(orig_w, orig_h, FilterType::Nearest)
    } else {
        DynamicImage::ImageRgba8(buf)
    }
}

// ---------------------------------------------------------------------------
// Convenience functions
// ---------------------------------------------------------------------------

/// Quick one-shot: paintify with default settings (16-color, 4x crunch).
pub fn paintify_default(img: &DynamicImage) -> DynamicImage {
    paintify(img, &PaintConfig::default())
}

/// Quick one-shot with custom pixel chunk size.
pub fn paintify_chunky(img: &DynamicImage, pixel_size: u32) -> DynamicImage {
    paintify(img, &PaintConfig::default().chunky(pixel_size))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use image::DynamicImage;

    fn test_img() -> DynamicImage {
        DynamicImage::new_rgba8(64, 64)
    }

    #[test]
    fn test_paintify_preserves_dimensions() {
        let img = test_img();
        let result = paintify_default(&img);
        assert_eq!(result.dimensions(), (64, 64));
    }

    #[test]
    fn test_paintify_no_preserve() {
        let img = test_img();
        let mut config = PaintConfig::default();
        config.preserve_aspect = false;
        let result = paintify(&img, &config);
        assert_eq!(result.dimensions(), (16, 16)); // 64 / 4 = 16
    }

    #[test]
    fn test_closest_color_black() {
        let px = Rgba([0, 0, 0, 255]);
        let result = closest_color(&px, &MS_PAINT_PALETTE);
        assert_eq!(&result.0[..3], &[0, 0, 0]);
    }

    #[test]
    fn test_closest_color_white() {
        let px = Rgba([255, 255, 255, 255]);
        let result = closest_color(&px, &MS_PAINT_PALETTE);
        assert_eq!(&result.0[..3], &[255, 255, 255]);
    }

    #[test]
    fn test_extended_palette_has_28_colors() {
        assert_eq!(MS_PAINT_EXTENDED.len(), 28);
    }
}
