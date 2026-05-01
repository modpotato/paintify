# Paintify

Make any image or webcam feed look like a Windows 95 MS Paint drawing.

```
1920x1080 photo → downscale → Kuwahara blobs → 16-color palette → edge outlines → upscale
```

**[Live Demo](https://modpotato.github.io/paintify/)** — drop an image, watch it paintify.

## Architecture

```
paintify/
├── core/          # Rust lib: pipeline engine
├── web/           # wasm-bindgen → browser
├── webcam/        # nokhwa + pixels + winit → OBS virtual cam
└── docs/          # GitHub Pages (index.html + compiled WASM)
```

### Pipeline

| Stage | What it does |
|---|---|
| Nearest-Neighbor downscale | Crushes resolution, preserves hard pixel edges (no blur) |
| Kuwahara filter | Flattens textures into solid "brush stroke" blobs |
| Sobel edge detection | Finds edges, overlays black pencil outlines |
| Color quantization | Maps every pixel to the classic 16-color MS Paint palette |
| Nearest-Neighbor upscale | Blows pixels back up as hard-edged squares |

## Usage

### Web App

```sh
cd web && wasm-pack build --target web
cp -r pkg ../docs/
# Deploy docs/ to GitHub Pages
```

### Virtual Webcam

```sh
# Run with defaults (60fps, 3x pixelation, 16-color, Kuwahara + edges)
cargo run --release -p paintify-webcam

# Custom settings
cargo run --release -p paintify-webcam -- --fps 30 --pixel-size 4 --palette 28 --dithering

# Disable effects
cargo run --release -p paintify-webcam -- --no-kuwahara --no-edges
```

Then Window Capture the paintify window in OBS, hit Start Virtual Camera, and select "OBS Virtual Camera" in Zoom/Discord/Teams.

#### CLI Flags

| Flag | Default | Description |
|---|---|---|
| `--fps N` | 60 | Target frames per second (1–120) |
| `--pixel-size N` | 3 | Pixel crunch factor (1–32) |
| `--palette N` | 16 | Color palette: 16 or 28 |
| `--dithering` | off | Bayer ordered dithering |
| `--no-kuwahara` | — | Disable oil-paint blob filter |
| `--no-edges` | — | Disable pencil outline overlay |

#### Keyboard Controls

| Key | Action |
|---|---|
| ↑ / ↓ | Adjust pixel size |
| P | Toggle 16/28 color palette |
| D | Toggle dithering |
| Escape | Exit |

### Rust Library

```rust
use paintify_core::{paintify, PaintConfig};

let config = PaintConfig::default()
    .chunky(4)
    .with_kuwahara(2)
    .with_edges(true);

let result = paintify(&image, &config);
```

## Building

```sh
# Full workspace
cargo build --release

# Tests
cargo test

# WebAssembly
cd web && wasm-pack build --target web

# Windows webcam (requires MSMF backend)
cargo build --release -p paintify-webcam
```

## License

MIT
