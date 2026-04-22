mod vge;

use vge::renderer::VanguardRenderer;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;
use winit::{
    event::{Event, WindowEvent, ElementState, DeviceEvent, MouseButton},
    event_loop::{ControlFlow, EventLoop},
    window::{WindowBuilder, CursorGrabMode},
    keyboard::{KeyCode, PhysicalKey},
};
use std::collections::HashSet;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = FmtSubscriber::builder().with_max_level(Level::INFO).finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("Vanguard Engine v3 Starting Up (0.29 LTS Baseline)...");

    let rt = Runtime::new()?;
    let _guard = rt.enter();

    let event_loop = EventLoop::new()?;
    let window = Arc::new(WindowBuilder::new()
        .with_title("Vanguard Engine v3 - Noclip GI Active")
        .with_inner_size(winit::dpi::LogicalSize::new(1920.0, 1080.0))
        .build(&event_loop)?);

    let mut renderer = pollster::block_on(VanguardRenderer::new(Some(window.clone())))?;
    let mut keys_pressed = HashSet::new();
    let mut last_frame = std::time::Instant::now();
    let mut cursor_visible = true;

    event_loop.run(move |event, elwt| {
        elwt.set_control_flow(ControlFlow::Poll);

        match event {
            Event::WindowEvent { event, .. } => {
                if !renderer.egui_state.on_window_event(&window, &event).consumed {
                    match event {
                        WindowEvent::CloseRequested => elwt.exit(),
                        WindowEvent::Resized(new_size) => { renderer.resize(new_size); }
                        WindowEvent::MouseInput { state: ElementState::Pressed, button: MouseButton::Left, .. } => {
                            let _ = window.set_cursor_grab(CursorGrabMode::Locked);
                            let _ = window.set_cursor_grab(CursorGrabMode::Confined);
                            window.set_cursor_visible(false);
                            cursor_visible = false;
                        }
                        WindowEvent::KeyboardInput { event: kb_event, .. } => {
                            if let winit::keyboard::PhysicalKey::Code(key) = kb_event.physical_key {
                                if key == KeyCode::Escape {
                                    let _ = window.set_cursor_grab(winit::window::CursorGrabMode::None);
                                    window.set_cursor_visible(true);
                                    cursor_visible = true;
                                }
                                if key == KeyCode::Digit5 && kb_event.state == ElementState::Pressed {
                                    renderer.show_ui = !renderer.show_ui;
                                }
                                if kb_event.state == ElementState::Pressed { keys_pressed.insert(key); } else { keys_pressed.remove(&key); }
                            }
                        },
                        WindowEvent::RedrawRequested => {
                            let now = std::time::Instant::now();
                            let dt = now.duration_since(last_frame).as_secs_f32();
                            last_frame = now;

                            let speed = 5.0 * dt;
                            let yaw_rad = renderer.camera_yaw.to_radians();
                            
                            let forward = [-yaw_rad.sin(), 0.0, -yaw_rad.cos()];
                            let right = [yaw_rad.cos(), 0.0, -yaw_rad.sin()];
                            
                            let mut moved = false;
                            if keys_pressed.contains(&KeyCode::KeyW) { renderer.camera_pos[0] += forward[0] * speed; renderer.camera_pos[1] += forward[1] * speed; renderer.camera_pos[2] += forward[2] * speed; moved = true; }
                            if keys_pressed.contains(&KeyCode::KeyS) { renderer.camera_pos[0] -= forward[0] * speed; renderer.camera_pos[1] -= forward[1] * speed; renderer.camera_pos[2] -= forward[2] * speed; moved = true; }
                            if keys_pressed.contains(&KeyCode::KeyA) { renderer.camera_pos[0] -= right[0] * speed; renderer.camera_pos[2] -= right[2] * speed; moved = true; }
                            if keys_pressed.contains(&KeyCode::KeyD) { renderer.camera_pos[0] += right[0] * speed; renderer.camera_pos[2] += right[2] * speed; moved = true; }
                            if keys_pressed.contains(&KeyCode::Space) { renderer.camera_pos[1] += speed; moved = true; }
                            if keys_pressed.contains(&KeyCode::ControlLeft) { renderer.camera_pos[1] -= speed; moved = true; }

                            if moved { renderer.frame_index = 0; }
                            renderer.render_frame(&window);
                        }
                        _ => (),
                    }
                }
            }
            Event::DeviceEvent { event: DeviceEvent::MouseMotion { delta }, .. } => {
                if !cursor_visible {
                    let sensitivity = 0.1;
                    renderer.camera_yaw -= delta.0 as f32 * sensitivity;
                    renderer.camera_pitch -= delta.1 as f32 * sensitivity;
                    renderer.camera_pitch = renderer.camera_pitch.clamp(-89.0, 89.0);
                    renderer.frame_index = 0;
                }
            }
            Event::AboutToWait => {
                window.request_redraw();
            }
            _ => (),
        }
    })?;

    Ok(())
}
