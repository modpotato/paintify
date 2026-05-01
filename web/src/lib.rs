//! # paintify-web
//!
//! WebAssembly bridge for `paintify-core`. Compile with `wasm-pack` and
//! call `paintify_js()` from JavaScript to crunch images in the browser.

use std::io::Cursor;
use wasm_bindgen::prelude::*;

use image::load_from_memory;
use paintify_core::{paintify, PaintConfig};

/// Global initialization — call once from JS before using paintify.
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
    // Use wee_alloc as the global allocator for smaller WASM binary
    #[cfg(feature = "wee_alloc")]
    {
        #[global_allocator]
        static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
    }
}

/// The main entry point. Takes raw image bytes (PNG, JPEG, WebP), applies
/// the Paintify pipeline, and returns a PNG byte array.
///
/// # Arguments
/// - `input_data`: Raw bytes of the input image (supports PNG, JPEG, WebP).
/// - `pixel_size`: How chunky the pixels get. 4 = default. Higher = chunkier.
/// - `extended_palette`: If `true`, uses 28-color extended palette instead of classic 16.
///
/// # Returns
/// Raw bytes of a PNG image, or an empty Vec on error.
#[wasm_bindgen]
pub fn paintify_js(input_data: &[u8], pixel_size: u32, extended_palette: bool) -> Vec<u8> {
    // Load the image from the byte buffer
    let img = match load_from_memory(input_data) {
        Ok(img) => img,
        Err(_) => return Vec::new(),
    };

    // Build configuration from JS parameters
    let config = PaintConfig::default()
        .chunky(pixel_size)
        .extended_palette(extended_palette);

    // Run the Paintify pipeline
    let processed = paintify(&img, &config);

    // Encode back to PNG
    let mut buffer = Cursor::new(Vec::new());
    if processed.write_to(&mut buffer, image::ImageFormat::Png).is_err() {
        return Vec::new();
    }

    buffer.into_inner()
}

/// Convenience: quick paintify with sensible defaults (16-color, 4x crunch).
#[wasm_bindgen]
pub fn paintify_default_js(input_data: &[u8]) -> Vec<u8> {
    paintify_js(input_data, 4, false)
}
