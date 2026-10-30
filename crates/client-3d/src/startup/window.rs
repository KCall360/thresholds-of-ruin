use super::{paint::Painter, Message, Phase, Progress};
use std::{
    io::{BufRead, BufReader},
    num::NonZeroU32,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

type Surface = softbuffer::Surface<OwnedDisplayHandle, Arc<Window>>;

fn fitting_size(width: f64, height: f64) -> LogicalSize<f64> {
    let scale = (width * 0.9 / 1152.0).min(height * 0.9 / 720.0).min(1.0);
    LogicalSize::new(1152.0 * scale, 720.0 * scale)
}

struct SplashWindow {
    native: Option<(Arc<Window>, Surface)>,
    painter: Painter,
    progress: Progress,
    captures: Option<PathBuf>,
    reported: Option<(Phase, bool)>,
    next_redraw: Instant,
    close_at: Option<Instant>,
    cancelled: bool,
    error: Option<String>,
}

impl SplashWindow {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: impl ToString) {
        let message = error.to_string();
        println!(
            "{}",
            serde_json::json!({"type":"startup_splash","event":"error","message":message,"pid":std::process::id()})
        );
        self.error = Some(message);
        event_loop.exit();
    }

    fn present(&mut self) -> Result<(), crate::Error> {
        let Some((window, surface)) = &mut self.native else {
            return Ok(());
        };
        let size = window.inner_size();
        let (Some(width), Some(height)) =
            (NonZeroU32::new(size.width), NonZeroU32::new(size.height))
        else {
            return Ok(());
        };
        surface.resize(width, height).map_err(|e| e.to_string())?;
        let pixels = self.painter.render(size.width, size.height, &self.progress);
        let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
        buffer.copy_from_slice(&pixels);
        buffer.present().map_err(|e| e.to_string())?;
        let identity = (self.progress.phase, self.progress.error.is_some());
        if self.reported != Some(identity) {
            let png = if let Some(directory) = &self.captures {
                std::fs::create_dir_all(directory)?;
                let path = directory.join(format!(
                    "startup-{}{}.png",
                    self.progress.phase.completed(),
                    if self.progress.error.is_some() {
                        "-error"
                    } else {
                        ""
                    }
                ));
                let bytes: Vec<_> = pixels
                    .iter()
                    .flat_map(|pixel| [(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8])
                    .collect();
                image::save_buffer(
                    &path,
                    &bytes,
                    size.width,
                    size.height,
                    image::ColorType::Rgb8,
                )?;
                Some(path)
            } else {
                None
            };
            println!(
                "{}",
                serde_json::json!({"type":"startup_splash","event":"presented","pid":std::process::id(),
                "phase":self.progress.phase,"completed":self.progress.phase.completed(),"total":4,
                "detail":self.progress.detail,"failed":self.progress.error.is_some(),"software_presented":true,
                "width":size.width,"height":size.height,"png":png})
            );
            self.reported = Some(identity);
            if self.progress.phase == Phase::Ready || self.progress.error.is_some() {
                self.close_at = Some(
                    Instant::now()
                        + if self.progress.error.is_some() {
                            Duration::from_secs(2)
                        } else {
                            Duration::from_millis(200)
                        },
                );
            }
        }
        Ok(())
    }
}

impl ApplicationHandler<Message> for SplashWindow {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.native.is_some() {
            return;
        }
        let mut attributes = Window::default_attributes()
            .with_title("Thresholds of Ruin — Starting")
            .with_inner_size(LogicalSize::new(1152.0, 720.0))
            .with_resizable(false)
            .with_decorations(false);
        if let Some(monitor) = event_loop.primary_monitor() {
            let available = monitor.size().to_logical::<f64>(monitor.scale_factor());
            let size = fitting_size(available.width, available.height);
            let physical = size.to_physical::<u32>(monitor.scale_factor());
            let origin = monitor.position();
            let bounds = monitor.size();
            attributes = attributes
                .with_inner_size(size)
                .with_position(PhysicalPosition::new(
                    origin.x + (bounds.width as i32 - physical.width as i32) / 2,
                    origin.y + (bounds.height as i32 - physical.height as i32) / 2,
                ));
        }
        let result = (|| -> Result<_, crate::Error> {
            let window = Arc::new(
                event_loop
                    .create_window(attributes)
                    .map_err(|e| e.to_string())?,
            );
            if let Some(monitor) = window.current_monitor() {
                let size = monitor.size();
                let own = window.outer_size();
                let origin = monitor.position();
                window.set_outer_position(PhysicalPosition::new(
                    origin.x + (size.width as i32 - own.width as i32) / 2,
                    origin.y + (size.height as i32 - own.height as i32) / 2,
                ));
            }
            let context = softbuffer::Context::new(event_loop.owned_display_handle())
                .map_err(|e| e.to_string())?;
            let surface =
                softbuffer::Surface::new(&context, window.clone()).map_err(|e| e.to_string())?;
            window.request_redraw();
            Ok((window, surface))
        })();
        match result {
            Ok(native) => self.native = Some(native),
            Err(error) => self.fail(event_loop, error),
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: Message) {
        match event {
            Message::Progress(progress) => {
                if progress.phase >= self.progress.phase {
                    self.progress = progress;
                    if let Some((window, _)) = &self.native {
                        window.request_redraw();
                    }
                }
            }
            Message::Close => event_loop.exit(),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::RedrawRequested => {
                if let Err(error) = self.present() {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::CloseRequested
            | WindowEvent::KeyboardInput {
                event:
                    winit::event::KeyEvent {
                        physical_key: PhysicalKey::Code(KeyCode::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                if !self.cancelled {
                    self.cancelled = true;
                    self.progress.detail = "Closing startup...".into();
                    println!(
                        "{}",
                        serde_json::json!({"type":"startup_splash","event":"cancelled","pid":std::process::id()})
                    );
                }
            }
            WindowEvent::Resized(_) => {
                if let Some((window, _)) = &self.native {
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self.close_at.is_some_and(|deadline| now >= deadline) {
            event_loop.exit();
            return;
        }
        if now >= self.next_redraw {
            if let Some((window, _)) = &self.native {
                window.request_redraw();
            }
            self.next_redraw = now + Duration::from_millis(100);
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(
            self.close_at
                .map_or(self.next_redraw, |close| close.min(self.next_redraw)),
        ));
    }
}

pub(super) fn run(arguments: &[String]) -> Result<(), crate::Error> {
    let captures = match arguments {
        [] => None,
        [flag, path] if flag == "--capture" => Some(PathBuf::from(path)),
        _ => return Err("Invalid internal startup-window arguments".into()),
    };
    let event_loop = EventLoop::<Message>::with_user_event()
        .build()
        .map_err(|e| e.to_string())?;
    let proxy = event_loop.create_proxy();
    std::thread::spawn(move || {
        for line in BufReader::new(std::io::stdin())
            .lines()
            .map_while(Result::ok)
        {
            let Ok(message) = serde_json::from_str::<Message>(&line) else {
                break;
            };
            if proxy.send_event(message).is_err() {
                return;
            }
        }
        let _ = proxy.send_event(Message::Close);
    });
    let mut app = SplashWindow {
        native: None,
        painter: Painter::new()?,
        progress: Progress::default(),
        captures,
        reported: None,
        next_redraw: Instant::now(),
        close_at: None,
        cancelled: false,
        error: None,
    };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    println!(
        "{}",
        serde_json::json!({"type":"startup_splash","event":"closed","pid":std::process::id()})
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_splash_fits_small_and_high_dpi_displays_without_stretching_the_art() {
        assert_eq!(
            fitting_size(1920.0, 1080.0),
            LogicalSize::new(1152.0, 720.0)
        );
        for (width, height) in [(853.0, 480.0), (640.0, 400.0), (1280.0, 720.0)] {
            let size = fitting_size(width, height);
            assert!(size.width <= width * 0.9 + 1e-6 && size.height <= height * 0.9 + 1e-6);
            assert!((size.width / size.height - 1.6).abs() < 1e-6);
        }
    }
}
