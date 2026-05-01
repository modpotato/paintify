//! # paintify-webcam
//!
//! Captures your real webcam, applies the Paintify crunch pipeline in real-time,
//! and renders the result in a borderless window.
//!
//! Use with OBS Virtual Camera: Window Capture the paintify-webcam window,
//! then select "OBS Virtual Camera" in Zoom/Discord/Teams.
//!
//! Controls:
//!   ↑ / ↓      — Adjust pixel crunch factor (more = chunkier)
//!   P          — Toggle between 16-color / 28-color palette
//!   D          — Toggle dithering (requires `dithering` feature in core)
//!   Escape     — Exit

use std::time::Instant;

use image::{DynamicImage, ImageBuffer};
use nokhwa::{
    nokhwa_initialize,
    query,
    pixel_format::RgbFormat,
    utils::{ApiBackend, CameraIndex, RequestedFormat, RequestedFormatType},
    Buffer, Camera,
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

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------
const INITIAL_WIDTH: u32 = 640;
const INITIAL_HEIGHT: u32 = 480;
const TARGET_FPS: u32 = 30;

// ---------------------------------------------------------------------------
// Application state
// ---------------------------------------------------------------------------

struct PaintifyApp {
    window: Option<Window>,
    pixels: Option<Pixels<'static>>,
    camera: Option<Camera>,
    config: PaintConfig,
    frame_count: u64,
    last_fps_print: Instant,
}

impl PaintifyApp {
    fn new() -> Self {
        let _ = env_logger::try_init();

        Self {
            window: None,
            pixels: None,
            camera: None,
            config: PaintConfig::default(),
            frame_count: 0,
            last_fps_print: Instant::now(),
        }
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

        let format = RequestedFormat::new::<RgbFormat>(RequestedFormatType::AbsoluteHighestFrameRate);
        let idx = CameraIndex::Index(0);

        match Camera::new(idx, format) {
            Ok(mut cam) => {
                cam.open_stream().expect("Failed to open camera stream");
                log::info!("Camera opened: {}", cameras[0].human_name());
                self.camera = Some(cam);
            }
            Err(e) => {
                log::error!("Failed to open camera: {e}");
            }
        }
    }

    fn process_frame(&mut self) {
        let Some(ref mut cam) = self.camera else { return };

        let frame: Buffer = match cam.frame() {
            Ok(f) => f,
            Err(_) => return,
        };

        let rgb_buf = frame.decode_image::<RgbFormat>().unwrap();
        let (cam_w, cam_h) = (rgb_buf.width(), rgb_buf.height());

        // Convert nokhwa frame to DynamicImage
        let img_buf: ImageBuffer<image::Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_fn(cam_w, cam_h, |x, y| {
                let px = rgb_buf.get_pixel(x, y);
                image::Rgba([px.0[0], px.0[1], px.0[2], 255])
            });
        let img = DynamicImage::ImageRgba8(img_buf);

        // Paintify it!
        let processed = paintify(&img, &self.config);
        let rgba = processed.to_rgba8();

        // Write to the pixels framebuffer
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

        self.frame_count += 1;

        // FPS counter
        let elapsed = self.last_fps_print.elapsed();
        if elapsed.as_secs_f64() > 5.0 {
            let fps = self.frame_count as f64 / elapsed.as_secs_f64();
            self.window.as_ref().map(|w| {
                w.set_title(&format!("Paintify Webcam — {:.0}fps | pixel_size={} | palette={}",
                    fps,
                    self.config.downscale_factor,
                    if self.config.extended_palette { "28" } else { "16" }
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
                self.config.downscale_factor = (self.config.downscale_factor.saturating_sub(1)).max(1);
                log::info!("Pixel size: {}", self.config.downscale_factor);
            }
            Key::Character(c) if c == "p" || c == "P" => {
                self.config.extended_palette = !self.config.extended_palette;
                log::info!(
                    "Palette: {}",
                    if self.config.extended_palette { "28-color" } else { "16-color" }
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

// ---------------------------------------------------------------------------
// winit ApplicationHandler
// ---------------------------------------------------------------------------

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
        let surface_texture = SurfaceTexture::new(window_size.width, window_size.height, &window);

        let pixels = Pixels::new(INITIAL_WIDTH, INITIAL_HEIGHT, surface_texture)
            .expect("Failed to create Pixels surface");

        // SAFETY: The window outlives the pixels since both are stored in the
        // same struct and the window is dropped after pixels (Drop order: fields
        // dropped in declaration order, pixels before window).
        let pixels_static: Pixels<'static> = unsafe { std::mem::transmute(pixels) };

        self.window = Some(window);
        self.pixels = Some(pixels_static);
        self.init_camera();

        // Request redraws at ~30fps
        event_loop.set_control_flow(ControlFlow::wait_duration(
            std::time::Duration::from_millis(1000 / TARGET_FPS as u64),
        ));
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
                event: KeyEvent {
                    logical_key: key,
                    state,
                    ..
                },
                ..
            } => {
                self.handle_key(&key, state);
            }
            WindowEvent::RedrawRequested => {
                self.process_frame();
                if let Some(ref window) = self.window {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() {
    let mut app = PaintifyApp::new();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop.run_app(&mut app).expect("Event loop failed");
}
