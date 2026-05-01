//! # paintify-core
//!
//! The heart of the MS Paintify pipeline. Takes any image, crunches it
//! through a series of destructive, nostalgia-maximizing transformations
//! to make it look like a Windows 95 MS Paint masterpiece.
//!
//! Pipeline: Downscale (Nearest-Neighbor) → [Kuwahara] → Color Quantize → [Edges] → Upscale (Nearest-Neighbor)

use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use image::imageops::FilterType;

const MS_PAINT_PALETTE: [[u8; 3]; 16] = [
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
];

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
    [128, 64, 0],
    [255, 128, 64],
    [0, 64, 0],
    [128, 255, 128],
    [0, 64, 128],
    [128, 128, 255],
    [128, 0, 64],
    [255, 128, 192],
    [64, 64, 64],
    [255, 128, 128],
    [255, 255, 128],
    [128, 255, 255],
];

pub struct PaintConfig {
    pub downscale_factor: u32,
    pub extended_palette: bool,
    pub dithering: bool,
    pub preserve_aspect: bool,
    /// Apply Kuwahara filter radius (1–5). Merges similar colors into solid "brush stroke" blobs.
    /// None = disabled. Default: None.
    pub kuwahara_radius: Option<u32>,
    /// Overlay hard black edges (like MS Paint pencil outlines). Default: false.
    pub edge_overlay: bool,
}

impl Default for PaintConfig {
    fn default() -> Self {
        Self {
            downscale_factor: 4,
            extended_palette: false,
            dithering: false,
            preserve_aspect: true,
            kuwahara_radius: None,
            edge_overlay: false,
        }
    }
}

impl PaintConfig {
    pub fn new() -> Self { Self::default() }
    pub fn chunky(mut self, factor: u32) -> Self { self.downscale_factor = factor; self }
    pub fn extended_palette(mut self, yes: bool) -> Self { self.extended_palette = yes; self }
    pub fn with_dithering(mut self, yes: bool) -> Self { self.dithering = yes; self }
    pub fn with_kuwahara(mut self, radius: u32) -> Self {
        self.kuwahara_radius = if radius > 0 { Some(radius) } else { None };
        self
    }
    pub fn with_edges(mut self, yes: bool) -> Self { self.edge_overlay = yes; self }
}

// ---------------------------------------------------------------------------
// Color quantization
// ---------------------------------------------------------------------------

fn closest_color(pixel: &Rgba<u8>, palette: &[[u8; 3]]) -> Rgba<u8> {
    let mut min_dist = u32::MAX;
    let mut best = palette[0];

    for color in palette {
        let dr = pixel[0] as i32 - color[0] as i32;
        let dg = pixel[1] as i32 - color[1] as i32;
        let db = pixel[2] as i32 - color[2] as i32;
        let dist = (dr * dr + dg * dg + db * db) as u32;

        if dist < min_dist {
            min_dist = dist;
            best = *color;
        }
    }

    Rgba([best[0], best[1], best[2], pixel[3]])
}

fn quantize_colors(buf: &mut RgbaImage, palette: &[[u8; 3]]) {
    for pixel in buf.pixels_mut() {
        *pixel = closest_color(pixel, palette);
    }
}

// ---------------------------------------------------------------------------
// Kuwahara filter — oil-painting / "brush stroke blob" effect
// ---------------------------------------------------------------------------

/// Apply a Kuwahara filter to an RGBA buffer.
///
/// The Kuwahara filter divides each pixel's neighborhood into four overlapping
/// quadrants, picks the quadrant with the lowest variance (most uniform color),
/// and replaces the pixel with that quadrant's mean. This creates solid color
/// "blobs" while preserving hard edges — exactly the MS Paint brush look.
fn kuwahara_filter(buf: &RgbaImage, radius: u32) -> RgbaImage {
    let r = radius as i32;
    let (w, h) = (buf.width() as i32, buf.height() as i32);
    let mut out = RgbaImage::new(w as u32, h as u32);

    for y in 0..h {
        for x in 0..w {
            // Bounds for the full window
            let x0 = (x - r).max(0);
            let y0 = (y - r).max(0);
            let x1 = (x + r).min(w - 1);
            let y1 = (y + r).min(h - 1);

            // Four quadrants relative to center pixel (x, y):
            // TL: (x0..=x, y0..=y)  TR: (x..=x1, y0..=y)
            // BL: (x0..=x, y..=y1)  BR: (x..=x1, y..=y1)
            let quads = [
                (x0, y0, x, y),       // top-left
                (x, y0, x1, y),       // top-right
                (x0, y, x, y1),       // bottom-left
                (x, y, x1, y1),       // bottom-right
            ];

            let mut best_mean = [0u8; 3];
            let mut best_var = f64::MAX;

            for &(qx0, qy0, qx1, qy1) in &quads {
                let mut count: f64 = 0.0;
                let mut sum = [0.0f64; 3];

                for qy in qy0..=qy1 {
                    for qx in qx0..=qx1 {
                        let px = buf.get_pixel(qx as u32, qy as u32);
                        sum[0] += px[0] as f64;
                        sum[1] += px[1] as f64;
                        sum[2] += px[2] as f64;
                        count += 1.0;
                    }
                }

                if count < 2.0 { continue; }

                let mean = [sum[0] / count, sum[1] / count, sum[2] / count];

                let mut variance = 0.0f64;
                for qy in qy0..=qy1 {
                    for qx in qx0..=qx1 {
                        let px = buf.get_pixel(qx as u32, qy as u32);
                        let dr = px[0] as f64 - mean[0];
                        let dg = px[1] as f64 - mean[1];
                        let db = px[2] as f64 - mean[2];
                        variance += dr * dr + dg * dg + db * db;
                    }
                }
                variance /= count;

                if variance < best_var {
                    best_var = variance;
                    best_mean = [mean[0] as u8, mean[1] as u8, mean[2] as u8];
                }
            }

            let alpha = buf.get_pixel(x as u32, y as u32)[3];
            out.put_pixel(x as u32, y as u32, Rgba([best_mean[0], best_mean[1], best_mean[2], alpha]));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Edge detection + overlay
// ---------------------------------------------------------------------------

fn sobel_edges(buf: &RgbaImage) -> RgbaImage {
    let (w, h) = (buf.width() as i32, buf.height() as i32);
    let mut edges = RgbaImage::new(w as u32, h as u32);

    for y in 1..h - 1 {
        for x in 1..w - 1 {
            // Luminance of each neighbor (ITU-R BT.601)
            let l = |px: &Rgba<u8>| -> i32 {
                (px[0] as i32 * 299 + px[1] as i32 * 587 + px[2] as i32 * 114) / 1000
            };

            let tl = l(buf.get_pixel((x - 1) as u32, (y - 1) as u32));
            let tc = l(buf.get_pixel(x as u32, (y - 1) as u32));
            let tr = l(buf.get_pixel((x + 1) as u32, (y - 1) as u32));
            let ml = l(buf.get_pixel((x - 1) as u32, y as u32));
            let mr = l(buf.get_pixel((x + 1) as u32, y as u32));
            let bl = l(buf.get_pixel((x - 1) as u32, (y + 1) as u32));
            let bc = l(buf.get_pixel(x as u32, (y + 1) as u32));
            let br = l(buf.get_pixel((x + 1) as u32, (y + 1) as u32));

            // Sobel operators
            let gx = -tl - 2 * ml - bl + tr + 2 * mr + br;
            let gy = -tl - 2 * tc - tr + bl + 2 * bc + br;

            let mag = ((gx * gx + gy * gy) as f64).sqrt() as i32;

            // Threshold: edges are where gradient is strong
            let val = if mag > 80 { 0u8 } else { 255u8 };
            edges.put_pixel(x as u32, y as u32, Rgba([val, val, val, 255]));
        }
    }

    edges
}

/// Overlay black edge pixels onto the image where the edge mask is dark.
fn overlay_edges(buf: &mut RgbaImage, edges: &RgbaImage) {
    for (x, y, pixel) in buf.enumerate_pixels_mut() {
        if let Some(edge) = edges.get_pixel_checked(x, y) {
            if edge[0] < 128 {
                *pixel = Rgba([0, 0, 0, 255]);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Ordered dithering
// ---------------------------------------------------------------------------

#[cfg(feature = "dithering")]
mod dither {
    const BAYER_4X4: [[u8; 4]; 4] = [
        [0, 8, 2, 10],
        [12, 4, 14, 6],
        [3, 11, 1, 9],
        [15, 7, 13, 5],
    ];

    pub fn ordered_dither(buf: &mut image::RgbaImage, palette: &[[u8; 3]]) {
        let (w, h) = buf.dimensions();
        for y in 0..h {
            for x in 0..w {
                let px = buf.get_pixel(x, y);
                let threshold = BAYER_4X4[(y % 4) as usize][(x % 4) as usize] as f32 / 16.0;

                let r = (px[0] as f32 / 255.0 + threshold).clamp(0.0, 1.0);
                let g = (px[1] as f32 / 255.0 + threshold).clamp(0.0, 1.0);
                let b = (px[2] as f32 / 255.0 + threshold).clamp(0.0, 1.0);

                let q = super::closest_color(
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

pub fn paintify(img: &DynamicImage, config: &PaintConfig) -> DynamicImage {
    let (orig_w, orig_h) = img.dimensions();

    let palette: &[[u8; 3]] = if config.extended_palette {
        &MS_PAINT_EXTENDED
    } else {
        &MS_PAINT_PALETTE
    };

    // Step 1: Crush resolution — Nearest-Neighbor for hard pixel edges
    let new_w = orig_w / config.downscale_factor;
    let new_h = orig_h / config.downscale_factor;
    let small = img.resize_exact(new_w, new_h, FilterType::Nearest);

    let mut buf = small.to_rgba8();

    // Step 2: Kuwahara filter — flatten textures into solid "brush stroke" blobs
    if let Some(radius) = config.kuwahara_radius {
        buf = kuwahara_filter(&buf, radius);
    }

    // Step 3: Detect edges before quantization (on the smoothed image)
    let edges = if config.edge_overlay {
        Some(sobel_edges(&buf))
    } else {
        None
    };

    // Step 4: Color quantization
    if config.dithering {
        #[cfg(feature = "dithering")]
        ordered_dither(&mut buf, palette);
        #[cfg(not(feature = "dithering"))]
        quantize_colors(&mut buf, palette);
    } else {
        quantize_colors(&mut buf, palette);
    }

    // Step 5: Overlay hard black edge lines (like MS Paint pencil)
    if let Some(edge_mask) = edges {
        overlay_edges(&mut buf, &edge_mask);
    }

    // Step 6: Blow it back up — Nearest-Neighbor keeps every pixel a hard square
    if config.preserve_aspect {
        DynamicImage::ImageRgba8(buf).resize_exact(orig_w, orig_h, FilterType::Nearest)
    } else {
        DynamicImage::ImageRgba8(buf)
    }
}

pub fn paintify_default(img: &DynamicImage) -> DynamicImage {
    paintify(img, &PaintConfig::default())
}

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
        assert_eq!(result.dimensions(), (16, 16));
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

    #[test]
    fn test_kuwahara_preserves_dimensions() {
        let mut img_buf = RgbaImage::new(32, 32);
        img_buf.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        let result = kuwahara_filter(&img_buf, 2);
        assert_eq!(result.dimensions(), (32, 32));
    }

    #[test]
    fn test_sobel_preserves_dimensions() {
        let img_buf = RgbaImage::new(16, 16);
        let edges = sobel_edges(&img_buf);
        assert_eq!(edges.dimensions(), (16, 16));
    }
}
