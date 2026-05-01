//! # paintify-webcam
//!
//! Captures your real webcam, applies the Paintify crunch pipeline in real-time,
//! and renders the result in a borderless window.
//!
//! Use with OBS Virtual Camera: Window Capture the paintify-webcam window,
//! then select "OBS Virtual Camera" in Zoom/Discord/Teams.
//!
//! ## Usage
//! ```sh
//! cargo run -p paintify-webcam
//! cargo run -p paintify-webcam -- --fps 60 --pixel-size 3 --palette 28 --dithering
//! ```
//!
//! ## Controls
//!   ↑ / ↓      — Adjust pixel crunch factor (more = chunkier)
//!   P          — Toggle between 16-color / 28-color palette
//!   D          — Toggle dithering
//!   Escape     — Exit

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use image::{DynamicImage, ImageBuffer};
use nokhwa::{
    nokhwa_initialize,
    pixel_format::RgbFormat,
    query,
    utils::{ApiBackend, CameraIndex, RequestedFormat, RequestedFormatType},
    Buffer, CallbackCamera,
};
use paintify_core::{paintify, PaintConfig};
use pixels::{Pixels, SurfaceTexture};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{Key, NamedKey},
    window::{Window, WindowAttributes},
};

const INITIAL_WIDTH: u32 = 640;
const INITIAL_HEIGHT: u32 = 480;

struct PaintifyApp {
    window: Option<Window>,
    pixels: Option<Pixels<'static>>,
    camera: Option<CallbackCamera>,
    frame_counter: Arc<AtomicU64>,
    last_processed_frame: u64,
    last_frame_arrival: Instant,
    config: PaintConfig,
    target_fps: u32,
    last_frame_time: Instant,
    frame_count: u64,
    last_fps_print: Instant,
}

impl PaintifyApp {
    fn new(cli: CliArgs) -> Self {
        let _ = env_logger::try_init();

        Self {
            window: None,
            pixels: None,
            camera: None,
            frame_counter: Arc::new(AtomicU64::new(0)),
            last_processed_frame: 0,
            last_frame_arrival: Instant::now(),
            config: PaintConfig::default()
                .chunky(cli.pixel_size)
                .extended_palette(cli.palette == 28)
                .with_dithering(cli.dithering)
                .with_kuwahara(if cli.kuwahara { 2 } else { 0 })
                .with_edges(cli.edges),
            target_fps: cli.fps,
            last_frame_time: Instant::now(),
            frame_count: 0,
            last_fps_print: Instant::now(),
        }
    }

    fn frame_interval(&self) -> Duration {
        Duration::from_secs_f64(1.0 / self.target_fps as f64)
    }

    fn init_camera(&mut self) {
        nokhwa_initialize(|granted| {
            log::info!("Camera permission: {}", granted);
        });

        let cameras = query(ApiBackend::Auto).unwrap_or_default();
        if cameras.is_empty() {
            log::error!("No cameras found!");
            return;
        }

        let format =
            RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
        let idx = CameraIndex::Index(0);

        let counter = self.frame_counter.clone();
        match CallbackCamera::new(idx, format, move |_buffer| {
            counter.fetch_add(1, Ordering::Relaxed);
        }) {
            Ok(mut cam) => {
                let before_fps = cam.frame_rate().unwrap_or(0);
                match cam.set_frame_rate(30) {
                    Ok(()) => log::info!("set_frame_rate(30) OK"),
                    Err(e) => log::error!("set_frame_rate(30) FAILED: {e}"),
                }
                let after_fps = cam.frame_rate().unwrap_or(0);
                log::info!("Camera frame rate: before={before_fps}, after={after_fps}");

                cam.open_stream().expect("Failed to open camera stream");
                log::info!(
                    "Camera opened: {} (threaded capture)",
                    cameras[0].human_name()
                );
                self.camera = Some(cam);
            }
            Err(e) => {
                log::error!("Failed to open camera: {e}");
            }
        }
    }

    fn process_frame(&mut self) {
        let current = self.frame_counter.load(Ordering::Relaxed);
        if current == self.last_processed_frame {
            return;
        }
        let since_last = self.last_frame_arrival.elapsed();
        self.last_frame_arrival = Instant::now();
        self.last_processed_frame = current;

        let t0 = Instant::now();

        let Some(ref mut cam) = self.camera else {
            return;
        };

        let frame: Buffer = match cam.last_frame() {
            Ok(f) => f,
            Err(_) => return,
        };

        let rgb_buf = frame.decode_image::<RgbFormat>().unwrap();
        let (cam_w, cam_h) = (rgb_buf.width(), rgb_buf.height());

        let img_buf: ImageBuffer<image::Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_fn(cam_w, cam_h, |x, y| {
                let px = rgb_buf.get_pixel(x, y);
                image::Rgba([px.0[0], px.0[1], px.0[2], 255])
            });
        let img = DynamicImage::ImageRgba8(img_buf);

        let processed = paintify(&img, &self.config);
        let rgba = processed.to_rgba8();

        if let Some(ref mut px) = self.pixels {
            let fb_w = px.context().texture_extent.width;
            let fb_h = px.context().texture_extent.height;
            let fb = px.frame_mut();

            for y in 0..fb_h.min(rgba.height()) {
                for x in 0..fb_w.min(rgba.width()) {
                    let src = rgba.get_pixel(x, y);
                    let idx = ((y * fb_w + x) * 4) as usize;
                    fb[idx] = src[0];
                    fb[idx + 1] = src[1];
                    fb[idx + 2] = src[2];
                    fb[idx + 3] = 255;
                }
            }

            if px.render().is_err() {
                log::warn!("Render failed");
            }
        }

        let pipeline_ms = t0.elapsed().as_secs_f64() * 1000.0;
        if self.frame_count % 30 == 0 {
            log::info!(
                "Frame #{current}: gap={:.0}ms, pipeline={:.0}ms, resolution={}x{}",
                since_last.as_secs_f64() * 1000.0,
                pipeline_ms,
                rgb_buf.width(),
                rgb_buf.height(),
            );
        }

        self.frame_count += 1;

        let elapsed = self.last_fps_print.elapsed();
        if elapsed.as_secs_f64() > 5.0 {
            let fps = self.frame_count as f64 / elapsed.as_secs_f64();
            self.window.as_ref().map(|w| {
                w.set_title(&format!(
                    "Paintify Webcam — {:.0}fps | pixel={} | palette={}",
                    fps,
                    self.config.downscale_factor,
                    if self.config.extended_palette {
                        "28"
                    } else {
                        "16"
                    }
                ));
            });
            self.frame_count = 0;
            self.last_fps_print = Instant::now();
        }
    }

    fn handle_key(&mut self, key: &Key, state: ElementState) {
        if state != ElementState::Pressed {
            return;
        }

        match key {
            Key::Named(NamedKey::ArrowUp) => {
                self.config.downscale_factor = (self.config.downscale_factor + 1).min(32);
                log::info!("Pixel size: {}", self.config.downscale_factor);
            }
            Key::Named(NamedKey::ArrowDown) => {
                self.config.downscale_factor =
                    (self.config.downscale_factor.saturating_sub(1)).max(1);
                log::info!("Pixel size: {}", self.config.downscale_factor);
            }
            Key::Character(c) if c == "p" || c == "P" => {
                self.config.extended_palette = !self.config.extended_palette;
                log::info!(
                    "Palette: {}",
                    if self.config.extended_palette {
                        "28-color"
                    } else {
                        "16-color"
                    }
                );
            }
            Key::Character(c) if c == "d" || c == "D" => {
                self.config.dithering = !self.config.dithering;
                log::info!(
                    "Dithering: {}",
                    if self.config.dithering { "ON" } else { "OFF" }
                );
            }
            Key::Named(NamedKey::Escape) => {
                std::process::exit(0);
            }
            _ => {}
        }
    }
}

impl ApplicationHandler for PaintifyApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attrs = WindowAttributes::default()
            .with_title("Paintify Webcam — OBS Window Capture me!")
            .with_inner_size(LogicalSize::new(INITIAL_WIDTH as f64, INITIAL_HEIGHT as f64))
            .with_resizable(true);

        let window = event_loop
            .create_window(window_attrs)
            .expect("Failed to create window");

        let window_size = window.inner_size();
        let surface_texture =
            SurfaceTexture::new(window_size.width, window_size.height, &window);

        let pixels = Pixels::new(INITIAL_WIDTH, INITIAL_HEIGHT, surface_texture)
            .expect("Failed to create Pixels surface");

        // SAFETY: The window outlives the pixels since both are stored in the
        // same struct and the window is dropped after pixels (Drop order: fields
        // dropped in declaration order, pixels before window).
        let pixels_static: Pixels<'static> = unsafe { std::mem::transmute(pixels) };

        self.window = Some(window);
        self.pixels = Some(pixels_static);
        self.init_camera();

        self.last_frame_time = Instant::now();
        if let Some(ref window) = self.window {
            window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::Poll);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(ref mut px) = self.pixels {
                    if size.width > 0 && size.height > 0 {
                        px.resize_surface(size.width, size.height)
                            .expect("Failed to resize surface");
                    }
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: key,
                        state,
                        ..
                    },
                ..
            } => {
                self.handle_key(&key, state);
            }
            WindowEvent::RedrawRequested => {
                let elapsed = self.last_frame_time.elapsed();
                if elapsed >= self.frame_interval() {
                    self.process_frame();
                    self.last_frame_time = Instant::now();
                }
                if let Some(ref window) = self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

const HELP: &str = "\
paintify-webcam — make your webcam look like MS Paint

USAGE:
    paintify-webcam [OPTIONS]

OPTIONS:
    --fps N           Target frames per second (default: 60, range: 1–120)
    --pixel-size N    Pixel crunch factor, higher = chunkier (default: 3, range: 1–32)
    --palette N       Color palette: 16 or 28 (default: 16)
    --dithering       Enable Bayer ordered dithering
    --no-kuwahara     Disable Kuwahara filter (oil-paint blobs)
    --no-edges        Disable edge overlay (pencil outlines)
    --help, -h        Show this help message

CONTROLS (in window):
    ↑ / ↓             Adjust pixel size
    P                 Toggle 16/28 color palette
    D                 Toggle dithering
    Escape            Exit
";

struct CliArgs {
    fps: u32,
    pixel_size: u32,
    palette: u32,
    dithering: bool,
    kuwahara: bool,
    edges: bool,
}

impl Default for CliArgs {
    fn default() -> Self {
        Self {
            fps: 60,
            pixel_size: 3,
            palette: 16,
            dithering: false,
            kuwahara: true,
            edges: true,
        }
    }
}

fn parse_args() -> CliArgs {
    let args: Vec<String> = std::env::args().collect();
    let mut cli = CliArgs::default();
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                eprintln!("{HELP}");
                std::process::exit(0);
            }
            "--fps" => {
                i += 1;
                if let Some(val) = args.get(i) {
                    cli.fps = val.parse::<u32>().unwrap_or(60).clamp(1, 120);
                }
            }
            "--pixel-size" => {
                i += 1;
                if let Some(val) = args.get(i) {
                    cli.pixel_size = val.parse::<u32>().unwrap_or(3).clamp(1, 32);
                }
            }
            "--palette" => {
                i += 1;
                if let Some(val) = args.get(i) {
                    let p = val.parse::<u32>().unwrap_or(16);
                    cli.palette = if p == 28 { 28 } else { 16 };
                }
            }
            "--dithering" => {
                cli.dithering = true;
            }
            "--no-kuwahara" => {
                cli.kuwahara = false;
            }
            "--no-edges" => {
                cli.edges = false;
            }
            other => {
                eprintln!("Unknown flag: {other}");
                eprintln!("Run with --help for usage.");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    cli
}

fn main() {
    let cli = parse_args();
    let mut app = PaintifyApp::new(cli);

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop
        .run_app(&mut app)
        .expect("Event loop failed");
}
