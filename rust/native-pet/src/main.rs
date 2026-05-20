#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use image::ImageReader;
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::ffi::c_void;
use std::fs;
#[cfg(target_os = "macos")]
use std::os::raw::c_double;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tao::dpi::{LogicalSize, PhysicalPosition};
use tao::event::{ElementState, Event, MouseButton, StartCause, WindowEvent};
use tao::event_loop::EventLoopProxy;
use tao::event_loop::{ControlFlow, EventLoopBuilder};
#[cfg(target_os = "macos")]
use tao::platform::macos::{WindowBuilderExtMacOS, WindowExtMacOS};
#[cfg(target_os = "windows")]
use tao::platform::windows::{WindowBuilderExtWindows, WindowExtWindows};
use tao::window::{Window, WindowBuilder};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use wgpu::util::DeviceExt;
#[cfg(target_os = "windows")]
use windows_sys::Win32::{
    Foundation::{GetLastError, HWND, LPARAM, LRESULT, POINT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::{
        Shell::{
            Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
        },
        WindowsAndMessaging::{
            AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu,
            DestroyWindow, GetCursorPos, GetWindowLongPtrW, LoadIconW, PostMessageW,
            RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, TrackPopupMenu, CREATESTRUCTW,
            CW_USEDEFAULT, GWLP_USERDATA, HMENU, IDI_APPLICATION, MF_ENABLED, MF_GRAYED, MF_POPUP,
            MF_SEPARATOR, MF_STRING, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP,
            WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NCCREATE, WM_NULL, WM_RBUTTONUP,
            WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_OVERLAPPED,
        },
    },
};

const APP_NAME: &str = "Q JK 桌宠";
const FRAME_FPS: u64 = 16;
const RUN_ENTER_SPEED: f64 = 1_350.0;
const RUN_EXIT_SPEED: f64 = 820.0;
const RUN_ENTER_HOLD: Duration = Duration::from_millis(120);
const RUN_EXIT_HOLD: Duration = Duration::from_millis(180);
const DRAG_DIRECTION_VELOCITY: f64 = 120.0;
const DRAG_DIRECTION_DISTANCE: f64 = 10.0;
const MAX_DRAG_SPEED: f64 = 4_000.0;
const MOVEMENT_STOP_DURATION: Duration = Duration::from_millis(260);

const ACTION_GROUPS: &[(&str, &[&str])] = &[
    (
        "常用",
        &[
            "idle",
            "waving",
            "cheer",
            "bow",
            "hands-on-hips",
            "shy",
            "sleepy",
            "look-around",
            "surprised",
            "thinking",
        ],
    ),
    (
        "移动",
        &[
            "running-right-start",
            "running-right",
            "running-right-stop",
            "running-left-start",
            "running-left",
            "running-left-stop",
            "walk-right-stop",
            "walk-left-stop",
        ],
    ),
    (
        "反馈",
        &[
            "clicked",
            "jumping",
            "waiting",
            "mouse-near",
            "mouse-hover",
            "mouse-leave",
        ],
    ),
    (
        "工作",
        &[
            "running",
            "typing",
            "typing-stop",
            "review",
            "scrolling",
            "drag-held",
            "stretch",
        ],
    ),
];

#[derive(Clone, Debug)]
struct Action {
    id: String,
    label: String,
    frame_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Settings {
    size: u32,
    speed: f64,
    x: i32,
    y: i32,
    random_enabled: bool,
    mouse_watch_enabled: bool,
    input_watch_enabled: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            size: 300,
            speed: 1.0,
            x: 120,
            y: 220,
            random_enabled: true,
            mouse_watch_enabled: true,
            input_watch_enabled: true,
        }
    }
}

struct AppState {
    root: PathBuf,
    actions: Vec<Action>,
    frames: HashMap<String, Vec<FrameImage>>,
    settings: Settings,
    current_action: String,
    frame_index: usize,
    last_frame_at: Instant,
    next_random_at: Instant,
    pending_return: Option<(String, Instant)>,
    last_keyboard_at: Instant,
    last_mouse_move_at: Instant,
    last_wheel_at: Instant,
    last_mouse_near_at: Instant,
    last_mouse_hover_at: Instant,
    last_mouse_x: f64,
    last_mouse_y: f64,
    local_cursor_x: Option<f64>,
    local_cursor_y: Option<f64>,
    mouse_near: bool,
    mouse_inside: bool,
    drag_state: Option<DragState>,
}

#[derive(Clone, Debug)]
struct DragState {
    started_at: Instant,
    last_moved_at: Instant,
    start_cursor_x: f64,
    start_cursor_y: f64,
    start_window_x: f64,
    start_window_y: f64,
    last_cursor_x: f64,
    last_cursor_y: f64,
    dragging: bool,
    last_speed: f64,
    last_direction_right: bool,
    smoothed_velocity_x: f64,
    smoothed_velocity_y: f64,
    direction_accumulator_x: f64,
    last_running: bool,
    run_candidate_since: Option<Instant>,
    walk_candidate_since: Option<Instant>,
}

impl AppState {
    fn new(root: PathBuf) -> Self {
        let (actions, frames) = load_actions(&root);
        eprintln!(
            "native-pet: loaded {} actions from {}",
            actions.len(),
            root.join("assets").join("frames").display()
        );
        let now = Instant::now();
        Self {
            root,
            current_action: "idle".to_string(),
            actions,
            frames,
            settings: Settings::default(),
            frame_index: 0,
            last_frame_at: now,
            next_random_at: now + random_delay(),
            pending_return: None,
            last_keyboard_at: now - Duration::from_secs(10),
            last_mouse_move_at: now - Duration::from_secs(10),
            last_wheel_at: now - Duration::from_secs(10),
            last_mouse_near_at: now - Duration::from_secs(10),
            last_mouse_hover_at: now - Duration::from_secs(10),
            last_mouse_x: 0.0,
            last_mouse_y: 0.0,
            local_cursor_x: None,
            local_cursor_y: None,
            mouse_near: false,
            mouse_inside: false,
            drag_state: None,
        }
    }

    fn set_action(&mut self, id: impl Into<String>) {
        let id = id.into();
        if self.actions.iter().any(|action| action.id == id) {
            self.current_action = id;
            self.frame_index = 0;
            self.last_frame_at = Instant::now();
            self.next_random_at = Instant::now() + random_delay();
        }
    }

    fn set_action_once(
        &mut self,
        id: impl Into<String>,
        return_to: impl Into<String>,
        duration: Duration,
    ) {
        self.set_action(id);
        self.pending_return = Some((return_to.into(), Instant::now() + duration));
    }

    fn set_movement_action(&mut self, id: impl Into<String>) {
        let id = id.into();
        if !self.actions.iter().any(|action| action.id == id) {
            return;
        }
        if movement_state(&self.current_action) && movement_state(&id) {
            self.current_action = id;
            let frame_count = self
                .actions
                .iter()
                .find(|action| action.id == self.current_action)
                .map(|action| action.frame_count)
                .unwrap_or(16)
                .max(1);
            self.frame_index %= frame_count;
            self.next_random_at = Instant::now() + random_delay();
        } else {
            self.set_action(id);
        }
    }

    fn next_action(&mut self) {
        if self.actions.is_empty() {
            return;
        }
        let current = self
            .actions
            .iter()
            .position(|action| action.id == self.current_action)
            .unwrap_or(0);
        let next = (current + 1) % self.actions.len();
        self.set_action(self.actions[next].id.clone());
    }

    fn tick(&mut self) {
        let delay = Duration::from_millis(
            (1000.0 / FRAME_FPS as f64 / self.settings.speed).max(1.0) as u64,
        );
        if self.last_frame_at.elapsed() < delay {
            self.tick_transitions();
            return;
        }
        self.last_frame_at = Instant::now();
        let frame_count = self
            .actions
            .iter()
            .find(|action| action.id == self.current_action)
            .map(|action| action.frame_count)
            .unwrap_or(16);
        self.frame_index = (self.frame_index + 1) % frame_count.max(1);
        self.tick_transitions();
    }

    fn tick_transitions(&mut self) {
        let now = Instant::now();
        if let Some((return_to, at)) = &self.pending_return {
            if now >= *at {
                let target = return_to.clone();
                self.pending_return = None;
                if target == "typing-stop" {
                    self.set_action_once("typing-stop", "idle", Duration::from_millis(900));
                } else {
                    self.set_action(target);
                }
            }
        }
        if let Some(drag_state) = &self.drag_state {
            if !drag_state.dragging
                && now.duration_since(drag_state.started_at) >= Duration::from_millis(650)
                && self.current_action != "drag-held"
            {
                self.set_action("drag-held");
            }
        }
        if self.settings.random_enabled
            && now >= self.next_random_at
            && self.pending_return.is_none()
            && !self.mouse_near
            && self.drag_state.is_none()
        {
            let next = random_action_id(&self.actions, &self.current_action);
            if next != self.current_action {
                self.set_action_once(next, "idle", Duration::from_secs(2));
            } else {
                self.next_random_at = now + random_delay();
            }
        }
    }

    fn current_frame(&self) -> Option<&FrameImage> {
        let frames = self.frames.get(&self.current_action)?;
        frames.get(self.frame_index % frames.len().max(1))
    }
}

#[derive(Clone, Debug)]
struct FrameImage {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

#[cfg(target_os = "windows")]
type AppTray = WindowsTray;

#[cfg(not(target_os = "windows"))]
type AppTray = TrayIcon;

fn main() {
    let root = resource_root();
    let event_loop = EventLoopBuilder::<String>::with_user_event().build();
    let tray_proxy = event_loop.create_proxy();
    let mut state = AppState::new(root);
    state.settings = load_settings();

    let window = create_pet_window(&event_loop, state.settings.size);
    apply_saved_position(&window, &state.settings);
    let mut renderer =
        pollster::block_on(Renderer::new(&window)).expect("failed to create renderer");
    let mut tray: Option<AppTray> = None;
    let mut last_tray_check = Instant::now() - Duration::from_secs(10);

    MenuEvent::set_event_handler(Some({
        let proxy = event_loop.create_proxy();
        move |event: MenuEvent| {
            let _ = proxy.send_event(event.id().as_ref().to_string());
        }
    }));
    start_input_monitor(event_loop.create_proxy());

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16));
        match event {
            Event::NewEvents(StartCause::Init) => {
                ensure_tray_visible(&mut tray, &state, &tray_proxy);
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                state.tick();
                if last_tray_check.elapsed() >= Duration::from_secs(5) {
                    last_tray_check = Instant::now();
                    ensure_tray_visible(&mut tray, &state, &tray_proxy);
                }
                window.request_redraw();
            }
            Event::UserEvent(command) => {
                let should_rebuild_menu = handle_command(&mut state, &window, &command);
                if should_rebuild_menu {
                    if let Some(tray) = &mut tray {
                        refresh_tray_menu(tray, &state);
                    } else {
                        ensure_tray_visible(&mut tray, &state, &tray_proxy);
                    }
                    save_window_position(&window, &mut state.settings);
                    save_settings(&state.settings);
                }
                renderer.resize(&window);
            }
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                ..
            } => {
                handle_local_cursor_move(&mut state, &window, position.x, position.y);
            }
            Event::WindowEvent {
                event:
                    WindowEvent::MouseInput {
                        state: ElementState::Pressed,
                        button: MouseButton::Left,
                        ..
                    },
                ..
            } => {
                handle_local_mouse_down(&mut state, &window);
            }
            Event::WindowEvent {
                event:
                    WindowEvent::MouseInput {
                        state: ElementState::Released,
                        button: MouseButton::Left,
                        ..
                    },
                ..
            } => {
                handle_local_mouse_up(&mut state, &window);
            }
            Event::WindowEvent {
                event: WindowEvent::Moved(_),
                ..
            } => {
                save_window_position(&window, &mut state.settings);
                save_settings(&state.settings);
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(_),
                ..
            } => renderer.resize(&window),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                *control_flow = ControlFlow::Exit;
            }
            Event::RedrawRequested(_) => {
                if let Some(frame) = state.current_frame() {
                    renderer.note_frame_once(&state.current_action, frame);
                    renderer.render(frame);
                }
            }
            _ => {}
        }
    });
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    tex_coord: [f32; 2],
}

struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    texture_size: Option<(u32, u32)>,
    texture: Option<wgpu::Texture>,
    bind_group: Option<wgpu::BindGroup>,
    logged_first_frame: bool,
}

impl Renderer {
    async fn new(window: &Window) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });
        let surface = unsafe {
            instance
                .create_surface_unsafe(
                    wgpu::SurfaceTargetUnsafe::from_window(window)
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?
        };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| "no compatible GPU adapter".to_string())?;
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("native-pet-device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?;
        let capabilities = surface.get_capabilities(&adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb())
            .unwrap_or(capabilities.formats[0]);
        let alpha_mode = capabilities
            .alpha_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::CompositeAlphaMode::PostMultiplied)
            .or_else(|| {
                capabilities
                    .alpha_modes
                    .iter()
                    .copied()
                    .find(|mode| *mode == wgpu::CompositeAlphaMode::PreMultiplied)
            })
            .or_else(|| {
                capabilities
                    .alpha_modes
                    .iter()
                    .copied()
                    .find(|mode| *mode != wgpu::CompositeAlphaMode::Opaque)
            })
            .unwrap_or(capabilities.alpha_modes[0]);
        eprintln!(
            "native-pet: surface format={format:?} alpha_mode={alpha_mode:?} available_alpha={:?}",
            capabilities.alpha_modes
        );
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("native-pet-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("native-pet-texture-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("native-pet-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("native-pet-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let vertices = [
            Vertex {
                position: [-1.0, -1.0],
                tex_coord: [0.0, 1.0],
            },
            Vertex {
                position: [1.0, -1.0],
                tex_coord: [1.0, 1.0],
            },
            Vertex {
                position: [1.0, 1.0],
                tex_coord: [1.0, 0.0],
            },
            Vertex {
                position: [-1.0, 1.0],
                tex_coord: [0.0, 0.0],
            },
        ];
        let indices: [u16; 6] = [0, 1, 2, 0, 2, 3];
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("native-pet-vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("native-pet-indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("native-pet-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            vertex_buffer,
            index_buffer,
            sampler,
            bind_group_layout,
            texture_size: None,
            texture: None,
            bind_group: None,
            logged_first_frame: false,
        })
    }

    fn note_frame_once(&mut self, action: &str, frame: &FrameImage) {
        if self.logged_first_frame {
            return;
        }
        self.logged_first_frame = true;
        eprintln!(
            "native-pet: first frame action={action} size={}x{} alpha_nonzero={}",
            frame.width,
            frame.height,
            frame.pixels.chunks_exact(4).any(|pixel| pixel[3] > 0)
        );
    }

    fn resize(&mut self, window: &Window) {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.device, &self.config);
    }

    fn ensure_texture(&mut self, frame: &FrameImage) {
        if self.texture_size == Some((frame.width, frame.height)) {
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("native-pet-frame-texture"),
            size: wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("native-pet-frame-bind-group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.texture = Some(texture);
        self.bind_group = Some(bind_group);
        self.texture_size = Some((frame.width, frame.height));
    }

    fn render(&mut self, frame: &FrameImage) {
        self.ensure_texture(frame);
        let Some(texture) = &self.texture else { return };
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &frame.pixels,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(4 * frame.width),
                rows_per_image: Some(frame.height),
            },
            wgpu::Extent3d {
                width: frame.width,
                height: frame.height,
                depth_or_array_layers: 1,
            },
        );

        let output = match self.surface.get_current_texture() {
            Ok(output) => output,
            Err(_) => {
                self.surface.configure(&self.device, &self.config);
                match self.surface.get_current_texture() {
                    Ok(output) => output,
                    Err(error) => {
                        eprintln!("render surface error: {error}");
                        return;
                    }
                }
            }
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("native-pet-render-encoder"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("native-pet-render-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            if let Some(bind_group) = &self.bind_group {
                pass.set_bind_group(0, bind_group, &[]);
            }
            pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
            pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..6, 0, 0..1);
        }
        self.queue.submit(Some(encoder.finish()));
        output.present();
    }
}

const SHADER: &str = r#"
struct VertexOut {
  @builtin(position) position: vec4<f32>,
  @location(0) tex_coord: vec2<f32>,
};

@vertex
fn vs_main(@location(0) position: vec2<f32>, @location(1) tex_coord: vec2<f32>) -> VertexOut {
  var out: VertexOut;
  out.position = vec4<f32>(position, 0.0, 1.0);
  out.tex_coord = tex_coord;
  return out;
}

@group(0) @binding(0) var pet_texture: texture_2d<f32>;
@group(0) @binding(1) var pet_sampler: sampler;

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
  return textureSample(pet_texture, pet_sampler, in.tex_coord);
}
"#;

fn create_pet_window(event_loop: &tao::event_loop::EventLoop<String>, size: u32) -> Window {
    let builder = WindowBuilder::new()
        .with_title(APP_NAME)
        .with_inner_size(LogicalSize::new(size, (size as f64 * 1.12) as u32))
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top(true)
        .with_resizable(false);

    #[cfg(target_os = "macos")]
    let builder = builder.with_has_shadow(false);

    #[cfg(target_os = "windows")]
    let builder = builder
        .with_undecorated_shadow(false)
        .with_skip_taskbar(true);

    let window = builder
        .build(event_loop)
        .expect("failed to create native pet window");

    configure_transparent_window(&window);
    window
}

fn configure_transparent_window(window: &Window) {
    #[cfg(target_os = "macos")]
    {
        window.set_has_shadow(false);
    }
    #[cfg(target_os = "windows")]
    {
        window.set_undecorated_shadow(false);
        let _ = window.set_skip_taskbar(true);
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = window;
    }
}

#[cfg(target_os = "windows")]
fn create_tray(state: &AppState, proxy: EventLoopProxy<String>) -> Result<AppTray, String> {
    WindowsTray::new(proxy, build_windows_tray_entries(state))
}

#[cfg(not(target_os = "windows"))]
fn create_tray(state: &AppState, _proxy: EventLoopProxy<String>) -> Result<AppTray, String> {
    let menu = build_tray_menu(state)?;
    let icon = load_tray_icon(&state.root)
        .or_else(|| Icon::from_rgba(tray_icon_rgba(32), 32, 32).ok())
        .ok_or_else(|| "failed to create tray icon".to_string())?;
    TrayIconBuilder::new()
        .with_id("q-jk-desktop-pet")
        .with_tooltip(APP_NAME)
        .with_icon(icon)
        .with_icon_as_template(cfg!(target_os = "macos"))
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(true)
        .with_menu_on_right_click(true)
        .build()
        .map_err(|error| error.to_string())
}

fn ensure_tray_visible(
    tray: &mut Option<AppTray>,
    state: &AppState,
    proxy: &EventLoopProxy<String>,
) {
    if tray.is_none() {
        match create_tray(state, proxy.clone()) {
            Ok(new_tray) => {
                eprintln!("native-pet: tray icon created");
                *tray = Some(new_tray);
            }
            Err(error) => {
                eprintln!("native-pet: failed to create tray icon: {error}");
                return;
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let mut should_recreate = false;
        if let Some(current_tray) = tray.as_ref() {
            if !current_tray.is_registered() {
                eprintln!(
                    "native-pet: Windows tray is not registered; asking shell to show it again"
                );
                if let Err(error) = current_tray.set_visible(true) {
                    eprintln!("native-pet: failed to show tray icon: {error}");
                    should_recreate = true;
                }
                if !current_tray.is_registered() {
                    should_recreate = true;
                }
            }
        }
        if should_recreate {
            eprintln!("native-pet: recreating Windows tray icon");
            *tray = None;
            match create_tray(state, proxy.clone()) {
                Ok(new_tray) => *tray = Some(new_tray),
                Err(error) => eprintln!("native-pet: failed to recreate tray icon: {error}"),
            }
        }
    }
}

fn load_tray_icon(root: &Path) -> Option<Icon> {
    let image = ImageReader::open(root.join("assets").join("icons").join("tray-32.png"))
        .ok()?
        .decode()
        .ok()?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).ok()
}

fn build_tray_menu(state: &AppState) -> Result<Menu, String> {
    let menu = Menu::new();
    let show = MenuItem::with_id("show", "显示桌宠", true, None);
    let hide = MenuItem::with_id("hide", "隐藏桌宠", true, None);
    let separator = PredefinedMenuItem::separator();
    menu.append_items(&[&show, &hide, &separator])
        .map_err(|error| error.to_string())?;

    let actions = Submenu::new("动作", true);
    let random_once = MenuItem::with_id("random-once", "马上随机一次", true, None);
    let next_action = MenuItem::with_id("next-action", "下一个动作", true, None);
    let action_separator = PredefinedMenuItem::separator();
    actions
        .append_items(&[&random_once, &next_action, &action_separator])
        .map_err(|error| error.to_string())?;

    let by_id: HashMap<&str, &Action> = state
        .actions
        .iter()
        .map(|action| (action.id.as_str(), action))
        .collect();
    for (group_label, ids) in ACTION_GROUPS {
        let group = Submenu::new(*group_label, true);
        for id in *ids {
            if let Some(action) = by_id.get(id) {
                let item = MenuItem::with_id(format!("action::{id}"), &action.label, true, None);
                group.append(&item).map_err(|error| error.to_string())?;
            }
        }
        actions.append(&group).map_err(|error| error.to_string())?;
    }
    menu.append(&actions).map_err(|error| error.to_string())?;

    let appearance = Submenu::new("外观", true);
    let size = Submenu::new(format!("大小：{}px", state.settings.size), true);
    let size_plus = MenuItem::with_id("size-plus", "放大", true, None);
    let size_minus = MenuItem::with_id("size-minus", "缩小", true, None);
    let size_reset = MenuItem::with_id("size-reset", "恢复默认大小", true, None);
    size.append_items(&[&size_plus, &size_minus, &size_reset])
        .map_err(|error| error.to_string())?;
    let speed = Submenu::new(format!("速度：{:.2}x", state.settings.speed), true);
    let speed_plus = MenuItem::with_id("speed-plus", "加快", true, None);
    let speed_minus = MenuItem::with_id("speed-minus", "减慢", true, None);
    let speed_reset = MenuItem::with_id("speed-reset", "恢复默认速度", true, None);
    speed
        .append_items(&[&speed_plus, &speed_minus, &speed_reset])
        .map_err(|error| error.to_string())?;
    appearance
        .append_items(&[&size, &speed])
        .map_err(|error| error.to_string())?;
    menu.append(&appearance)
        .map_err(|error| error.to_string())?;

    let monitor = Submenu::new("监视", true);
    let random = MenuItem::with_id(
        "toggle-random",
        if state.settings.random_enabled {
            "随机动作：开"
        } else {
            "随机动作：关"
        },
        true,
        None,
    );
    let mouse = MenuItem::with_id(
        "toggle-mouse-watch",
        if state.settings.mouse_watch_enabled {
            "鼠标位置监视：开"
        } else {
            "鼠标位置监视：关"
        },
        true,
        None,
    );
    let input = MenuItem::with_id(
        "toggle-input-watch",
        if state.settings.input_watch_enabled {
            "键盘/滚轮监视：开"
        } else {
            "键盘/滚轮监视：关"
        },
        true,
        None,
    );
    let autostart = MenuItem::with_id(
        "toggle-autostart",
        if autostart_enabled() {
            "开机自启：开"
        } else {
            "开机自启：关"
        },
        autostart_supported(),
        None,
    );
    let accessibility = MenuItem::with_id(
        "open-accessibility-settings",
        if accessibility_granted() {
            "辅助功能权限：已允许"
        } else {
            "打开辅助功能设置"
        },
        true,
        None,
    );
    monitor
        .append_items(&[&random, &mouse, &input, &autostart, &accessibility])
        .map_err(|error| error.to_string())?;
    menu.append(&monitor).map_err(|error| error.to_string())?;

    let separator = PredefinedMenuItem::separator();
    let quit = MenuItem::with_id("quit", "退出", true, None);
    menu.append_items(&[&separator, &quit])
        .map_err(|error| error.to_string())?;
    Ok(menu)
}

fn refresh_tray_menu(tray: &mut AppTray, state: &AppState) {
    #[cfg(target_os = "windows")]
    {
        tray.set_entries(build_windows_tray_entries(state));
    }
    #[cfg(not(target_os = "windows"))]
    match build_tray_menu(state) {
        Ok(menu) => tray.set_menu(Some(Box::new(menu))),
        Err(error) => eprintln!("failed to refresh tray menu: {error}"),
    }
}

fn handle_command(state: &mut AppState, window: &Window, command: &str) -> bool {
    if let Some(action) = command.strip_prefix("action::") {
        state.set_action(action);
        return true;
    }
    if let Some(payload) = command.strip_prefix("native-mousemove::") {
        if let Some((x, y)) = parse_point(payload) {
            handle_global_mouse_move(state, window, x, y);
        }
        return false;
    }
    if let Some(payload) = command.strip_prefix("native-mousedown::") {
        if let Some((x, y)) = parse_point(payload) {
            handle_global_mouse_down(state, window, x, y);
        }
        return false;
    }
    if let Some(payload) = command.strip_prefix("native-mouseup::") {
        if let Some((x, y)) = parse_point(payload) {
            handle_global_mouse_up(state, window, x, y);
        }
        return false;
    }

    match command {
        "show" => window.set_visible(true),
        "hide" => window.set_visible(false),
        "random-once" => {
            let action = random_action_id(&state.actions, &state.current_action);
            state.set_action_once(action, "idle", Duration::from_secs(2));
        }
        "next-action" => state.next_action(),
        "native-keydown" => handle_keyboard_activity(state),
        "native-wheel" => handle_wheel_activity(state),
        "size-plus" => set_size(state, window, state.settings.size + 40),
        "size-minus" => set_size(state, window, state.settings.size.saturating_sub(40)),
        "size-reset" => set_size(state, window, 300),
        "speed-plus" => state.settings.speed = (state.settings.speed + 0.1).min(2.0),
        "speed-minus" => state.settings.speed = (state.settings.speed - 0.1).max(0.5),
        "speed-reset" => state.settings.speed = 1.0,
        "toggle-random" => {
            state.settings.random_enabled = !state.settings.random_enabled;
            state.next_random_at = Instant::now() + random_delay();
        }
        "toggle-mouse-watch" => {
            state.settings.mouse_watch_enabled = !state.settings.mouse_watch_enabled
        }
        "toggle-input-watch" => {
            state.settings.input_watch_enabled = !state.settings.input_watch_enabled
        }
        "toggle-autostart" => {
            if let Err(error) = set_autostart(!autostart_enabled()) {
                eprintln!("failed to toggle autostart: {error}");
            }
        }
        "open-accessibility-settings" => {
            if let Err(error) = open_accessibility_settings() {
                eprintln!("failed to open accessibility settings: {error}");
            }
        }
        "quit" => std::process::exit(0),
        _ => {}
    }
    matches!(
        command,
        "size-plus"
            | "size-minus"
            | "size-reset"
            | "speed-plus"
            | "speed-minus"
            | "speed-reset"
            | "toggle-random"
            | "toggle-mouse-watch"
            | "toggle-input-watch"
            | "toggle-autostart"
            | "open-accessibility-settings"
            | "next-action"
            | "random-once"
    )
}

fn random_action_id(actions: &[Action], current: &str) -> String {
    let pool = actions
        .iter()
        .filter(|action| action.id != "idle" && action.id != current)
        .collect::<Vec<_>>();
    if pool.is_empty() {
        return current.to_string();
    }
    pool.choose(&mut rand::thread_rng())
        .map(|action| action.id.clone())
        .unwrap_or_else(|| current.to_string())
}

fn random_delay() -> Duration {
    let millis = 9000 + (rand::random::<u64>() % 11001);
    Duration::from_millis(millis)
}

fn start_input_monitor(proxy: tao::event_loop::EventLoopProxy<String>) {
    #[cfg(target_os = "macos")]
    {
        start_macos_input_monitor(proxy);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = proxy;
        eprintln!("global input monitor is not implemented for this platform yet");
    }
}

#[cfg(target_os = "macos")]
fn start_macos_input_monitor(proxy: tao::event_loop::EventLoopProxy<String>) {
    std::thread::spawn(move || unsafe {
        let mask = cg_event_mask(&[
            K_CG_EVENT_LEFT_MOUSE_DOWN,
            K_CG_EVENT_LEFT_MOUSE_UP,
            K_CG_EVENT_RIGHT_MOUSE_DOWN,
            K_CG_EVENT_RIGHT_MOUSE_UP,
            K_CG_EVENT_MOUSE_MOVED,
            K_CG_EVENT_LEFT_MOUSE_DRAGGED,
            K_CG_EVENT_RIGHT_MOUSE_DRAGGED,
            K_CG_EVENT_KEY_DOWN,
            K_CG_EVENT_SCROLL_WHEEL,
            K_CG_EVENT_OTHER_MOUSE_DOWN,
            K_CG_EVENT_OTHER_MOUSE_UP,
            K_CG_EVENT_OTHER_MOUSE_DRAGGED,
        ]);
        let context = Box::new(MonitorContext {
            proxy,
            tap: std::ptr::null_mut(),
        });
        let user_info = Box::into_raw(context) as *mut c_void;
        let tap = CGEventTapCreate(
            K_CG_HID_EVENT_TAP,
            K_CG_HEAD_INSERT_EVENT_TAP,
            K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
            mask,
            macos_event_tap_callback,
            user_info,
        );
        if tap.is_null() {
            drop(Box::from_raw(user_info as *mut MonitorContext));
            eprintln!("global input monitor unavailable; grant Accessibility permission");
            return;
        }
        (*(user_info as *mut MonitorContext)).tap = tap;
        let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
        if source.is_null() {
            CFRelease(tap);
            drop(Box::from_raw(user_info as *mut MonitorContext));
            eprintln!("global input monitor failed to create runloop source");
            return;
        }
        let run_loop = CFRunLoopGetCurrent();
        CFRunLoopAddSource(run_loop, source, K_CF_RUN_LOOP_COMMON_MODES);
        CGEventTapEnable(tap, true);
        CFRunLoopRun();
        CFRelease(source);
        CFRelease(tap);
        drop(Box::from_raw(user_info as *mut MonitorContext));
    });
}

#[cfg(target_os = "macos")]
struct MonitorContext {
    proxy: tao::event_loop::EventLoopProxy<String>,
    tap: CFMachPortRef,
}

#[cfg(target_os = "macos")]
type CGEventType = u32;

#[cfg(target_os = "macos")]
type CGEventTapProxy = *mut c_void;

#[cfg(target_os = "macos")]
type CGEventRef = *mut c_void;

#[cfg(target_os = "macos")]
type CFMachPortRef = *mut c_void;

#[cfg(target_os = "macos")]
type CFRunLoopRef = *mut c_void;

#[cfg(target_os = "macos")]
type CFRunLoopSourceRef = *mut c_void;

#[cfg(target_os = "macos")]
type CFAllocatorRef = *const c_void;

#[cfg(target_os = "macos")]
type CFStringRef = *const c_void;

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: c_double,
    y: c_double,
}

#[cfg(target_os = "macos")]
const K_CG_HID_EVENT_TAP: u32 = 0;
#[cfg(target_os = "macos")]
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
#[cfg(target_os = "macos")]
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;
#[cfg(target_os = "macos")]
const K_CG_EVENT_LEFT_MOUSE_DOWN: CGEventType = 1;
#[cfg(target_os = "macos")]
const K_CG_EVENT_LEFT_MOUSE_UP: CGEventType = 2;
#[cfg(target_os = "macos")]
const K_CG_EVENT_RIGHT_MOUSE_DOWN: CGEventType = 3;
#[cfg(target_os = "macos")]
const K_CG_EVENT_RIGHT_MOUSE_UP: CGEventType = 4;
#[cfg(target_os = "macos")]
const K_CG_EVENT_MOUSE_MOVED: CGEventType = 5;
#[cfg(target_os = "macos")]
const K_CG_EVENT_LEFT_MOUSE_DRAGGED: CGEventType = 6;
#[cfg(target_os = "macos")]
const K_CG_EVENT_RIGHT_MOUSE_DRAGGED: CGEventType = 7;
#[cfg(target_os = "macos")]
const K_CG_EVENT_KEY_DOWN: CGEventType = 10;
#[cfg(target_os = "macos")]
const K_CG_EVENT_SCROLL_WHEEL: CGEventType = 22;
#[cfg(target_os = "macos")]
const K_CG_EVENT_OTHER_MOUSE_DOWN: CGEventType = 25;
#[cfg(target_os = "macos")]
const K_CG_EVENT_OTHER_MOUSE_UP: CGEventType = 26;
#[cfg(target_os = "macos")]
const K_CG_EVENT_OTHER_MOUSE_DRAGGED: CGEventType = 27;
#[cfg(target_os = "macos")]
const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: CGEventType = 0xFFFFFFFE;
#[cfg(target_os = "macos")]
const K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT: CGEventType = 0xFFFFFFFF;

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: extern "C" fn(
            CGEventTapProxy,
            CGEventType,
            CGEventRef,
            *mut c_void,
        ) -> CGEventRef,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    fn AXIsProcessTrusted() -> u8;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(
        allocator: CFAllocatorRef,
        port: CFMachPortRef,
        order: isize,
    ) -> CFRunLoopSourceRef;
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(run_loop: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRun();
    fn CFRelease(value: *const c_void);
}

#[cfg(target_os = "macos")]
extern "C" {
    #[link_name = "kCFRunLoopCommonModes"]
    static K_CF_RUN_LOOP_COMMON_MODES: CFStringRef;
}

#[cfg(target_os = "macos")]
fn cg_event_mask(types: &[CGEventType]) -> u64 {
    types
        .iter()
        .fold(0_u64, |mask, event_type| mask | (1_u64 << *event_type))
}

#[cfg(target_os = "macos")]
extern "C" fn macos_event_tap_callback(
    _proxy: CGEventTapProxy,
    event_type: CGEventType,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef {
    unsafe {
        if event_type == K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT
            || event_type == K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT
        {
            let Some(context) = (user_info as *mut MonitorContext).as_mut() else {
                return event;
            };
            CGEventTapEnable(context.tap, true);
            return event;
        }
        let Some(context) = (user_info as *mut MonitorContext).as_mut() else {
            return event;
        };
        match event_type {
            K_CG_EVENT_MOUSE_MOVED
            | K_CG_EVENT_LEFT_MOUSE_DRAGGED
            | K_CG_EVENT_RIGHT_MOUSE_DRAGGED
            | K_CG_EVENT_OTHER_MOUSE_DRAGGED => {
                let point = CGEventGetLocation(event);
                let _ = context
                    .proxy
                    .send_event(format!("native-mousemove::{},{}", point.x, point.y));
            }
            K_CG_EVENT_LEFT_MOUSE_DOWN
            | K_CG_EVENT_RIGHT_MOUSE_DOWN
            | K_CG_EVENT_OTHER_MOUSE_DOWN => {
                let point = CGEventGetLocation(event);
                let _ = context
                    .proxy
                    .send_event(format!("native-mousedown::{},{}", point.x, point.y));
            }
            K_CG_EVENT_LEFT_MOUSE_UP | K_CG_EVENT_RIGHT_MOUSE_UP | K_CG_EVENT_OTHER_MOUSE_UP => {
                let point = CGEventGetLocation(event);
                let _ = context
                    .proxy
                    .send_event(format!("native-mouseup::{},{}", point.x, point.y));
            }
            K_CG_EVENT_KEY_DOWN => {
                let _ = context.proxy.send_event("native-keydown".to_string());
            }
            K_CG_EVENT_SCROLL_WHEEL => {
                let _ = context.proxy.send_event("native-wheel".to_string());
            }
            _ => {}
        }
        event
    }
}

fn parse_point(payload: &str) -> Option<(f64, f64)> {
    let (x, y) = payload.split_once(',')?;
    Some((x.parse().ok()?, y.parse().ok()?))
}

fn window_rect(window: &Window) -> Option<(f64, f64, f64, f64)> {
    let position = window.outer_position().ok()?;
    let size = window.inner_size();
    Some((
        position.x as f64,
        position.y as f64,
        size.width as f64,
        size.height as f64,
    ))
}

fn point_inside_window(window: &Window, x: f64, y: f64) -> bool {
    let Some((left, top, width, height)) = window_rect(window) else {
        return false;
    };
    x >= left && x <= left + width && y >= top && y <= top + height
}

fn normalize_screen_point(window: &Window, x: f64, y: f64) -> (f64, f64) {
    let scale = window.scale_factor();
    (x * scale, y * scale)
}

#[cfg(not(target_os = "macos"))]
fn local_to_screen_point(window: &Window, x: f64, y: f64) -> Option<(f64, f64)> {
    let position = window.outer_position().ok()?;
    let scale = window.scale_factor();
    Some((position.x as f64 + x * scale, position.y as f64 + y * scale))
}

fn point_to_rect_distance(x: f64, y: f64, rect: (f64, f64, f64, f64)) -> f64 {
    let (left, top, width, height) = rect;
    let right = left + width;
    let bottom = top + height;
    let dx = if x < left {
        left - x
    } else if x > right {
        x - right
    } else {
        0.0
    };
    let dy = if y < top {
        top - y
    } else if y > bottom {
        y - bottom
    } else {
        0.0
    };
    dx.hypot(dy)
}

fn can_react(state: &AppState) -> bool {
    state.pending_return.is_none() && state.drag_state.is_none()
}

fn handle_keyboard_activity(state: &mut AppState) {
    if !state.settings.input_watch_enabled || state.drag_state.is_some() {
        return;
    }
    let now = Instant::now();
    if now.duration_since(state.last_keyboard_at) > Duration::from_millis(700) {
        state.pending_return = None;
        state.set_action("typing");
    }
    state.last_keyboard_at = now;
    state.pending_return = Some(("typing-stop".to_string(), now + Duration::from_millis(1600)));
}

fn handle_wheel_activity(state: &mut AppState) {
    if !state.settings.input_watch_enabled || !can_react(state) {
        return;
    }
    let now = Instant::now();
    if now.duration_since(state.last_wheel_at) < Duration::from_millis(1400) {
        return;
    }
    state.last_wheel_at = now;
    state.set_action_once("scrolling", "idle", Duration::from_millis(1400));
}

fn handle_local_cursor_move(state: &mut AppState, window: &Window, x: f64, y: f64) {
    state.local_cursor_x = Some(x);
    state.local_cursor_y = Some(y);
    #[cfg(not(target_os = "macos"))]
    {
        if state.drag_state.is_some() {
            if let Some((screen_x, screen_y)) = local_to_screen_point(window, x, y) {
                update_drag_motion(state, window, screen_x, screen_y);
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = window;
    }
}

fn handle_local_mouse_down(state: &mut AppState, window: &Window) {
    #[cfg(not(target_os = "macos"))]
    {
        state.pending_return = None;
        if let (Some(x), Some(y)) = (state.local_cursor_x, state.local_cursor_y) {
            if let Some((screen_x, screen_y)) = local_to_screen_point(window, x, y) {
                begin_drag(state, window, screen_x, screen_y, true);
                return;
            }
        }
        state.set_action("drag-held");
    }
    #[cfg(target_os = "macos")]
    {
        let _ = state;
        let _ = window;
    }
}

fn handle_local_mouse_up(state: &mut AppState, window: &Window) {
    #[cfg(not(target_os = "macos"))]
    {
        if state.drag_state.is_some() {
            finish_drag(state, window);
        } else if state.current_action == "drag-held" {
            state.set_action("idle");
        }
        save_window_position(window, &mut state.settings);
        save_settings(&state.settings);
    }
    #[cfg(target_os = "macos")]
    {
        let _ = state;
        let _ = window;
    }
}

fn handle_global_mouse_down(state: &mut AppState, window: &Window, x: f64, y: f64) {
    let (x, y) = normalize_screen_point(window, x, y);
    if !point_inside_window(window, x, y) {
        return;
    }
    begin_drag(state, window, x, y, false);
}

fn begin_drag(state: &mut AppState, window: &Window, x: f64, y: f64, local_drag: bool) {
    let position = window.outer_position().ok();
    let Some(position) = position else {
        return;
    };
    let last_direction_right = !state.current_action.contains("left");
    state.pending_return = None;
    if movement_state(&state.current_action) {
        state.set_action("idle");
    }
    state.drag_state = Some(DragState {
        started_at: Instant::now(),
        last_moved_at: Instant::now(),
        start_cursor_x: x,
        start_cursor_y: y,
        start_window_x: position.x as f64,
        start_window_y: position.y as f64,
        last_cursor_x: x,
        last_cursor_y: y,
        dragging: false,
        last_speed: 0.0,
        last_direction_right,
        smoothed_velocity_x: 0.0,
        smoothed_velocity_y: 0.0,
        direction_accumulator_x: 0.0,
        last_running: false,
        run_candidate_since: None,
        walk_candidate_since: None,
    });
    if local_drag {
        eprintln!("native-pet: local drag started");
    }
}

fn handle_global_mouse_up(state: &mut AppState, window: &Window, _x: f64, _y: f64) {
    finish_drag(state, window);
}

fn finish_drag(state: &mut AppState, window: &Window) {
    let Some(drag) = state.drag_state.take() else {
        return;
    };
    if drag.dragging {
        state.pending_return = None;
        if let Some(stop_action) = movement_stop_action(drag.last_direction_right, drag.last_speed)
        {
            state.set_action_once(stop_action, "idle", MOVEMENT_STOP_DURATION);
        } else {
            state.set_action("idle");
        }
        save_window_position(window, &mut state.settings);
        save_settings(&state.settings);
    } else {
        state.set_action_once("clicked", "idle", Duration::from_millis(1300));
    }
}

fn handle_global_mouse_move(state: &mut AppState, window: &Window, x: f64, y: f64) {
    let (x, y) = normalize_screen_point(window, x, y);
    if update_drag_motion(state, window, x, y) {
        return;
    }

    handle_mouse_reactivity(state, window, x, y);
}

fn update_drag_motion(state: &mut AppState, window: &Window, x: f64, y: f64) -> bool {
    let now = Instant::now();
    let moved = (x - state.last_mouse_x).hypot(y - state.last_mouse_y);
    if moved >= 2.0 {
        state.last_mouse_move_at = now;
    }
    state.last_mouse_x = x;
    state.last_mouse_y = y;

    let mut drag_action = None;
    if let Some(drag) = state.drag_state.as_mut() {
        let total_delta_x = x - drag.start_cursor_x;
        let total_delta_y = y - drag.start_cursor_y;
        let step_delta_x = x - drag.last_cursor_x;
        let step_delta_y = y - drag.last_cursor_y;
        let step_dt = now
            .duration_since(drag.last_moved_at)
            .as_secs_f64()
            .clamp(0.004, 0.05);
        if !drag.dragging && total_delta_x.hypot(total_delta_y) >= 5.0 {
            drag.dragging = true;
        }
        if drag.dragging {
            let next_x = (drag.start_window_x + total_delta_x).round() as i32;
            let next_y = (drag.start_window_y + total_delta_y).round() as i32;
            window.set_outer_position(PhysicalPosition::new(next_x, next_y));
            let raw_velocity_x = step_delta_x / step_dt;
            let raw_velocity_y = step_delta_y / step_dt;
            let velocity_alpha = (step_dt / 0.08).clamp(0.18, 0.45);
            drag.smoothed_velocity_x =
                drag.smoothed_velocity_x * (1.0 - velocity_alpha) + raw_velocity_x * velocity_alpha;
            drag.smoothed_velocity_y =
                drag.smoothed_velocity_y * (1.0 - velocity_alpha) + raw_velocity_y * velocity_alpha;
            let speed = drag
                .smoothed_velocity_x
                .hypot(drag.smoothed_velocity_y)
                .min(MAX_DRAG_SPEED);
            if step_delta_x.abs() >= 0.2 || step_delta_y.abs() >= 0.2 {
                drag.last_speed = if drag.last_speed <= 0.0 {
                    speed
                } else {
                    drag.last_speed * 0.72 + speed * 0.28
                };
                drag.direction_accumulator_x =
                    (drag.direction_accumulator_x * 0.78 + step_delta_x).clamp(-48.0, 48.0);
                if let Some(direction_right) = movement_direction(
                    drag.smoothed_velocity_x,
                    drag.direction_accumulator_x,
                    drag.last_direction_right,
                ) {
                    if direction_right != drag.last_direction_right {
                        drag.last_direction_right = direction_right;
                        drag.direction_accumulator_x = 0.0;
                        drag.run_candidate_since = None;
                        drag.walk_candidate_since = None;
                    }
                }
                drag.last_running = update_movement_running_state(
                    drag.last_speed,
                    drag.last_running,
                    &mut drag.run_candidate_since,
                    &mut drag.walk_candidate_since,
                    now,
                );
                drag_action = Some(movement_action(
                    drag.last_direction_right,
                    drag.last_running,
                ));
            }
        }
        drag.last_cursor_x = x;
        drag.last_cursor_y = y;
        drag.last_moved_at = now;
    }
    if let Some(action) = drag_action {
        if state.current_action != action {
            state.pending_return = None;
            eprintln!(
                "native-pet: drag action={action} speed={:.0}px/s",
                state
                    .drag_state
                    .as_ref()
                    .map(|drag| drag.last_speed)
                    .unwrap_or_default()
            );
            state.set_movement_action(action);
        }
        return true;
    }

    state.drag_state.is_some()
}

fn handle_mouse_reactivity(state: &mut AppState, window: &Window, x: f64, y: f64) {
    let now = Instant::now();
    if !state.settings.mouse_watch_enabled || !can_react(state) {
        return;
    }

    let Some(rect) = window_rect(window) else {
        return;
    };
    let was_near = state.mouse_near;
    let was_inside = state.mouse_inside;
    let distance = point_to_rect_distance(x, y, rect);
    let near = distance <= (220.0_f64).max(state.settings.size as f64 * 0.78);
    let inside = point_inside_window(window, x, y);
    let hovering = inside || distance <= (90.0_f64).max(state.settings.size as f64 * 0.28);
    state.mouse_near = near;
    state.mouse_inside = inside;

    if near
        && !was_near
        && now.duration_since(state.last_mouse_near_at) > Duration::from_millis(2800)
    {
        state.last_mouse_near_at = now;
        state.set_action_once("mouse-hover", "idle", Duration::from_millis(1300));
        return;
    }

    if hovering
        && !was_inside
        && now.duration_since(state.last_mouse_hover_at) > Duration::from_millis(3200)
    {
        state.last_mouse_hover_at = now;
        state.set_action_once("mouse-hover", "idle", Duration::from_millis(1300));
        return;
    }

    if !near && was_near && reactive_state(&state.current_action) {
        state.set_action_once("mouse-leave", "idle", Duration::from_millis(1000));
    }
}

fn update_movement_running_state(
    speed_px_per_second: f64,
    was_running: bool,
    run_candidate_since: &mut Option<Instant>,
    walk_candidate_since: &mut Option<Instant>,
    now: Instant,
) -> bool {
    if was_running {
        *run_candidate_since = None;
        if speed_px_per_second <= RUN_EXIT_SPEED {
            let since = *walk_candidate_since.get_or_insert(now);
            return now.duration_since(since) < RUN_EXIT_HOLD;
        }
        *walk_candidate_since = None;
        true
    } else {
        *walk_candidate_since = None;
        if speed_px_per_second >= RUN_ENTER_SPEED {
            let since = *run_candidate_since.get_or_insert(now);
            return now.duration_since(since) >= RUN_ENTER_HOLD;
        }
        *run_candidate_since = None;
        false
    }
}

fn movement_direction(velocity_x: f64, accumulated_x: f64, current_right: bool) -> Option<bool> {
    if velocity_x.abs() >= DRAG_DIRECTION_VELOCITY {
        return Some(velocity_x > 0.0);
    }
    if current_right && accumulated_x <= -DRAG_DIRECTION_DISTANCE {
        return Some(false);
    }
    if !current_right && accumulated_x >= DRAG_DIRECTION_DISTANCE {
        return Some(true);
    }
    None
}

fn movement_action(direction_right: bool, running: bool) -> &'static str {
    if running {
        if direction_right {
            "running-right"
        } else {
            "running-left"
        }
    } else if direction_right {
        "walk-right-stop"
    } else {
        "walk-left-stop"
    }
}

fn movement_stop_action(direction_right: bool, speed_px_per_second: f64) -> Option<&'static str> {
    let speed_px_per_second = speed_px_per_second.min(MAX_DRAG_SPEED);
    if speed_px_per_second <= 0.0 {
        return None;
    }
    Some(if speed_px_per_second >= RUN_EXIT_SPEED {
        if direction_right {
            "running-right-stop"
        } else {
            "running-left-stop"
        }
    } else if direction_right {
        "walk-right-stop"
    } else {
        "walk-left-stop"
    })
}

fn movement_state(action: &str) -> bool {
    matches!(
        action,
        "running-right"
            | "running-left"
            | "running-right-start"
            | "running-left-start"
            | "running-right-stop"
            | "running-left-stop"
            | "walk-right-stop"
            | "walk-left-stop"
            | "drag-held"
    )
}

fn set_size(state: &mut AppState, window: &Window, size: u32) {
    state.settings.size = size.clamp(160, 720);
    window.set_inner_size(LogicalSize::new(
        state.settings.size,
        (state.settings.size as f64 * 1.12) as u32,
    ));
}

fn reactive_state(action: &str) -> bool {
    matches!(
        action,
        "waiting" | "review" | "waving" | "typing" | "mouse-hover" | "drag-held"
    )
}

fn normalize_settings(mut settings: Settings) -> Settings {
    settings.size = settings.size.clamp(160, 720);
    settings.speed = settings.speed.clamp(0.5, 2.0);
    settings
}

fn resource_root() -> PathBuf {
    let mut candidates = Vec::new();
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            candidates.push(exe_dir.to_path_buf());
            if let Some(contents_dir) = exe_dir.parent() {
                candidates.push(contents_dir.join("Resources"));
            }
        }
    }
    candidates
        .into_iter()
        .find(|root| root.join("assets").join("frames").is_dir())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn settings_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join(APP_NAME)
                .join("settings.json");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join(APP_NAME).join("settings.json");
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home)
                .join(".config")
                .join("q-jk-desktop-pet")
                .join("settings.json");
        }
    }
    PathBuf::from("settings.json")
}

fn load_settings() -> Settings {
    fs::read_to_string(settings_path())
        .ok()
        .and_then(|data| serde_json::from_str::<Settings>(&data).ok())
        .map(normalize_settings)
        .unwrap_or_default()
}

fn save_settings(settings: &Settings) {
    let path = settings_path();
    if let Some(parent) = path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            eprintln!("failed to create settings dir: {error}");
            return;
        }
    }
    match serde_json::to_string_pretty(settings) {
        Ok(data) => {
            if let Err(error) = fs::write(path, data) {
                eprintln!("failed to save settings: {error}");
            }
        }
        Err(error) => eprintln!("failed to serialize settings: {error}"),
    }
}

fn apply_saved_position(window: &Window, settings: &Settings) {
    window.set_outer_position(PhysicalPosition::new(settings.x, settings.y));
    if let Ok(position) = window.outer_position() {
        let size = window.inner_size();
        eprintln!(
            "native-pet: window position=({}, {}) size={}x{}",
            position.x, position.y, size.width, size.height
        );
    }
}

fn save_window_position(window: &Window, settings: &mut Settings) {
    if let Ok(position) = window.outer_position() {
        settings.x = position.x;
        settings.y = position.y;
    }
}

#[cfg(target_os = "macos")]
fn accessibility_granted() -> bool {
    unsafe { AXIsProcessTrusted() != 0 }
}

#[cfg(not(target_os = "macos"))]
fn accessibility_granted() -> bool {
    false
}

#[cfg(target_os = "macos")]
fn open_accessibility_settings() -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .output()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[cfg(not(target_os = "macos"))]
fn open_accessibility_settings() -> Result<(), String> {
    Err("当前平台还没有实现辅助功能设置入口".to_string())
}

#[cfg(target_os = "macos")]
fn autostart_supported() -> bool {
    true
}

#[cfg(not(target_os = "macos"))]
fn autostart_supported() -> bool {
    false
}

#[cfg(target_os = "macos")]
fn launch_agent_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library")
            .join("LaunchAgents")
            .join("local.q-jk-desktop-pet.plist"),
    )
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
fn user_domain() -> String {
    format!("gui/{}", unsafe { libc::geteuid() })
}

#[cfg(target_os = "macos")]
fn run_launchctl(args: &[&str]) {
    let _ = std::process::Command::new("/bin/launchctl")
        .args(args)
        .output();
}

#[cfg(target_os = "macos")]
fn autostart_enabled() -> bool {
    launch_agent_path()
        .map(|path| path.exists())
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn autostart_enabled() -> bool {
    false
}

#[cfg(target_os = "macos")]
fn set_autostart(enabled: bool) -> Result<(), String> {
    let Some(path) = launch_agent_path() else {
        return Err("无法解析 LaunchAgents 路径".to_string());
    };
    if enabled {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let parent = path
            .parent()
            .ok_or_else(|| "LaunchAgents 路径无效".to_string())?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        let executable = executable.to_string_lossy();
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>local.q-jk-desktop-pet</string>
  <key>RunAtLoad</key>
  <true/>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
  </array>
  <key>WorkingDirectory</key>
  <string>{}</string>
</dict>
</plist>
"#,
            xml_escape(&executable),
            xml_escape(
                &std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::from("."))
                    .to_string_lossy(),
            )
        );
        fs::write(&path, plist).map_err(|error| error.to_string())?;
        let domain = user_domain();
        let path_string = path.to_string_lossy().to_string();
        run_launchctl(&["bootout", &domain, &path_string]);
        run_launchctl(&["bootstrap", &domain, &path_string]);
        Ok(())
    } else {
        let domain = user_domain();
        let path_string = path.to_string_lossy().to_string();
        run_launchctl(&["bootout", &domain, &path_string]);
        if path.exists() {
            fs::remove_file(&path).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
fn set_autostart(_enabled: bool) -> Result<(), String> {
    Err("当前平台还没有实现开机自启".to_string())
}

fn load_actions(root: &Path) -> (Vec<Action>, HashMap<String, Vec<FrameImage>>) {
    let frames_root = root.join("assets").join("frames");
    let mut labels = action_labels();
    let mut actions = Vec::new();
    let mut frames_by_action = HashMap::new();
    let Ok(entries) = fs::read_dir(&frames_root) else {
        return (actions, frames_by_action);
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let id = entry.file_name().to_string_lossy().to_string();
        if id.contains("before") || id.contains("wrong") {
            continue;
        }
        let frames = load_action_frames(&entry.path());
        if frames.is_empty() {
            continue;
        }
        let frame_count = frames.len();
        let label = labels.remove(id.as_str()).unwrap_or_else(|| id.clone());
        frames_by_action.insert(id.clone(), frames);
        actions.push(Action {
            id,
            label,
            frame_count,
        });
    }

    actions.sort_by_key(|action| {
        ACTION_GROUPS
            .iter()
            .flat_map(|(_, ids)| ids.iter())
            .position(|id| *id == action.id)
            .unwrap_or(usize::MAX)
    });
    (actions, frames_by_action)
}

fn load_action_frames(path: &Path) -> Vec<FrameImage> {
    let mut frame_paths = fs::read_dir(path)
        .map(|frames| {
            frames
                .flatten()
                .map(|frame| frame.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    frame_paths.sort();
    frame_paths
        .iter()
        .filter_map(|path| {
            let (width, height, pixels) = load_frame_rgba(path)?;
            Some(FrameImage {
                width,
                height,
                pixels,
            })
        })
        .collect()
}

fn action_labels() -> HashMap<&'static str, String> {
    HashMap::from([
        ("idle", "待机".to_string()),
        ("running-right-start", "向右起跑".to_string()),
        ("running-right", "向右跑".to_string()),
        ("running-right-stop", "向右停下".to_string()),
        ("running-left-start", "向左起跑".to_string()),
        ("running-left", "向左跑".to_string()),
        ("running-left-stop", "向左停下".to_string()),
        ("clicked", "单击反馈".to_string()),
        ("waving", "挥手".to_string()),
        ("jumping", "跳跃".to_string()),
        ("waiting", "等待".to_string()),
        ("running", "工作中".to_string()),
        ("typing", "键盘输入".to_string()),
        ("typing-stop", "输入停止".to_string()),
        ("review", "检查".to_string()),
        ("scrolling", "滚轮浏览".to_string()),
        ("mouse-near", "鼠标靠近".to_string()),
        ("mouse-hover", "鼠标悬停".to_string()),
        ("mouse-leave", "鼠标离开".to_string()),
        ("drag-held", "按住拖拽".to_string()),
        ("bow", "鞠躬".to_string()),
        ("cheer", "开心欢呼".to_string()),
        ("hands-on-hips", "叉腰".to_string()),
        ("look-around", "四处张望".to_string()),
        ("shy", "害羞".to_string()),
        ("sleepy", "犯困".to_string()),
        ("stretch", "伸懒腰".to_string()),
        ("surprised", "惊讶".to_string()),
        ("thinking", "思考".to_string()),
        ("walk-left-stop", "向左停步".to_string()),
        ("walk-right-stop", "向右停步".to_string()),
    ])
}

fn tray_icon_rgba(size: u32) -> Vec<u8> {
    let mut rgba = vec![0; (size * size * 4) as usize];
    let center = size as f32 / 2.0;
    for y in 0..size {
        for x in 0..size {
            let xf = x as f32;
            let yf = y as f32;
            let dx = xf - center;
            let dy = yf - center * 0.82;
            let head = dx * dx / (center * 0.62).powi(2) + dy * dy / (center * 0.58).powi(2) <= 1.0;
            let hair = head && (yf < center * 0.82 || xf < center * 0.54 || xf > center * 1.46);
            let face = head && !hair;
            let shirt = yf >= center * 1.1
                && yf <= center * 1.72
                && xf >= center * 0.58
                && xf <= center * 1.42;
            let eye = ((xf - center * 0.78).abs() <= 1.0 || (xf - center * 1.22).abs() <= 1.0)
                && (yf - center * 0.76).abs() <= 1.0;
            let smile = yf >= center * 0.98
                && yf <= center * 1.04
                && xf >= center * 0.82
                && xf <= center * 1.18;
            let outline = dx * dx / (center * 0.67).powi(2) + dy * dy / (center * 0.63).powi(2)
                <= 1.0
                && !head;

            let color = if eye || smile {
                Some([25, 25, 25, 255])
            } else if hair || outline {
                Some([18, 18, 18, 255])
            } else if face {
                Some([255, 236, 220, 255])
            } else if shirt {
                Some([255, 126, 170, 255])
            } else {
                None
            };
            if let Some(color) = color {
                let offset = ((y * size + x) * 4) as usize;
                rgba[offset..offset + 4].copy_from_slice(&color);
            }
        }
    }
    rgba
}

#[cfg(target_os = "windows")]
#[derive(Clone, Debug)]
enum TrayMenuEntry {
    Command {
        label: String,
        command: String,
        enabled: bool,
    },
    Separator,
    Submenu {
        label: String,
        children: Vec<TrayMenuEntry>,
    },
}

#[cfg(target_os = "windows")]
fn build_windows_tray_entries(state: &AppState) -> Vec<TrayMenuEntry> {
    let mut entries = Vec::new();
    entries.push(TrayMenuEntry::Command {
        label: "显示桌宠".to_string(),
        command: "show".to_string(),
        enabled: true,
    });
    entries.push(TrayMenuEntry::Command {
        label: "隐藏桌宠".to_string(),
        command: "hide".to_string(),
        enabled: true,
    });
    entries.push(TrayMenuEntry::Separator);

    let by_id: HashMap<&str, &Action> = state
        .actions
        .iter()
        .map(|action| (action.id.as_str(), action))
        .collect();
    let mut action_children = vec![
        TrayMenuEntry::Command {
            label: "马上随机一次".to_string(),
            command: "random-once".to_string(),
            enabled: true,
        },
        TrayMenuEntry::Command {
            label: "下一个动作".to_string(),
            command: "next-action".to_string(),
            enabled: true,
        },
        TrayMenuEntry::Separator,
    ];
    for (group_label, ids) in ACTION_GROUPS {
        let mut group_children = Vec::new();
        for id in *ids {
            if let Some(action) = by_id.get(id) {
                group_children.push(TrayMenuEntry::Command {
                    label: action.label.clone(),
                    command: format!("action::{id}"),
                    enabled: true,
                });
            }
        }
        if !group_children.is_empty() {
            action_children.push(TrayMenuEntry::Submenu {
                label: (*group_label).to_string(),
                children: group_children,
            });
        }
    }
    entries.push(TrayMenuEntry::Submenu {
        label: "动作".to_string(),
        children: action_children,
    });

    entries.push(TrayMenuEntry::Submenu {
        label: "外观".to_string(),
        children: vec![
            TrayMenuEntry::Submenu {
                label: format!("大小：{}px", state.settings.size),
                children: vec![
                    TrayMenuEntry::Command {
                        label: "放大".to_string(),
                        command: "size-plus".to_string(),
                        enabled: true,
                    },
                    TrayMenuEntry::Command {
                        label: "缩小".to_string(),
                        command: "size-minus".to_string(),
                        enabled: true,
                    },
                    TrayMenuEntry::Command {
                        label: "恢复默认大小".to_string(),
                        command: "size-reset".to_string(),
                        enabled: true,
                    },
                ],
            },
            TrayMenuEntry::Submenu {
                label: format!("速度：{:.2}x", state.settings.speed),
                children: vec![
                    TrayMenuEntry::Command {
                        label: "加快".to_string(),
                        command: "speed-plus".to_string(),
                        enabled: true,
                    },
                    TrayMenuEntry::Command {
                        label: "减慢".to_string(),
                        command: "speed-minus".to_string(),
                        enabled: true,
                    },
                    TrayMenuEntry::Command {
                        label: "恢复默认速度".to_string(),
                        command: "speed-reset".to_string(),
                        enabled: true,
                    },
                ],
            },
        ],
    });

    entries.push(TrayMenuEntry::Submenu {
        label: "监视".to_string(),
        children: vec![
            TrayMenuEntry::Command {
                label: if state.settings.random_enabled {
                    "随机动作：开"
                } else {
                    "随机动作：关"
                }
                .to_string(),
                command: "toggle-random".to_string(),
                enabled: true,
            },
            TrayMenuEntry::Command {
                label: if state.settings.mouse_watch_enabled {
                    "鼠标位置监视：开"
                } else {
                    "鼠标位置监视：关"
                }
                .to_string(),
                command: "toggle-mouse-watch".to_string(),
                enabled: true,
            },
            TrayMenuEntry::Command {
                label: if state.settings.input_watch_enabled {
                    "键盘/滚轮监视：开"
                } else {
                    "键盘/滚轮监视：关"
                }
                .to_string(),
                command: "toggle-input-watch".to_string(),
                enabled: true,
            },
            TrayMenuEntry::Command {
                label: if autostart_enabled() {
                    "开机自启：开"
                } else {
                    "开机自启：关"
                }
                .to_string(),
                command: "toggle-autostart".to_string(),
                enabled: autostart_supported(),
            },
            TrayMenuEntry::Command {
                label: if accessibility_granted() {
                    "辅助功能权限：已允许"
                } else {
                    "打开辅助功能设置"
                }
                .to_string(),
                command: "open-accessibility-settings".to_string(),
                enabled: true,
            },
        ],
    });

    entries.push(TrayMenuEntry::Separator);
    entries.push(TrayMenuEntry::Command {
        label: "退出".to_string(),
        command: "quit".to_string(),
        enabled: true,
    });
    entries
}

#[cfg(target_os = "windows")]
struct WindowsTray {
    hwnd: HWND,
}

#[cfg(target_os = "windows")]
struct WindowsTrayData {
    proxy: EventLoopProxy<String>,
    entries: Vec<TrayMenuEntry>,
    commands: Vec<(u16, String)>,
    registered: bool,
}

#[cfg(target_os = "windows")]
const WM_NATIVE_TRAY: u32 = WM_APP + 77;

#[cfg(target_os = "windows")]
impl WindowsTray {
    fn new(proxy: EventLoopProxy<String>, entries: Vec<TrayMenuEntry>) -> Result<Self, String> {
        unsafe {
            let class_name = wide_null("q_jk_desktop_pet_native_tray");
            let instance = GetModuleHandleW(std::ptr::null());
            let window_class = WNDCLASSW {
                lpfnWndProc: Some(windows_tray_proc),
                hInstance: instance,
                lpszClassName: class_name.as_ptr(),
                ..std::mem::zeroed()
            };
            RegisterClassW(&window_class);

            let data = Box::new(WindowsTrayData {
                proxy,
                entries,
                commands: Vec::new(),
                registered: false,
            });
            let raw_data = Box::into_raw(data);
            let hwnd = CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                class_name.as_ptr(),
                std::ptr::null(),
                WS_OVERLAPPED,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                raw_data.cast(),
            );
            if hwnd.is_null() {
                drop(Box::from_raw(raw_data));
                return Err(format!("CreateWindowExW failed: {}", GetLastError()));
            }

            let tray = Self { hwnd };
            tray.register()?;
            Ok(tray)
        }
    }

    fn set_entries(&self, entries: Vec<TrayMenuEntry>) {
        unsafe {
            if let Some(data) = self.data_mut() {
                data.entries = entries;
            }
        }
    }

    fn is_registered(&self) -> bool {
        unsafe { self.data_mut().map(|data| data.registered).unwrap_or(false) }
    }

    fn set_visible(&self, visible: bool) -> Result<(), String> {
        if visible {
            self.register()
        } else {
            self.unregister();
            Ok(())
        }
    }

    fn register(&self) -> Result<(), String> {
        unsafe {
            let icon = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION);
            let mut tip = [0_u16; 128];
            let tip_wide = wide_null(APP_NAME);
            for (index, value) in tip_wide.iter().take(127).enumerate() {
                tip[index] = *value;
            }
            let mut data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                uCallbackMessage: WM_NATIVE_TRAY,
                hIcon: icon,
                szTip: tip,
                ..std::mem::zeroed()
            };
            if Shell_NotifyIconW(NIM_ADD, &mut data) == 0 {
                let error = GetLastError();
                if let Some(tray_data) = self.data_mut() {
                    tray_data.registered = false;
                }
                return Err(format!("Shell_NotifyIconW(NIM_ADD) failed: {error}"));
            }
            if let Some(tray_data) = self.data_mut() {
                tray_data.registered = true;
            }
            Ok(())
        }
    }

    fn unregister(&self) {
        unsafe {
            let mut data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: 1,
                ..std::mem::zeroed()
            };
            Shell_NotifyIconW(NIM_DELETE, &mut data);
            if let Some(tray_data) = self.data_mut() {
                tray_data.registered = false;
            }
        }
    }

    unsafe fn data_mut(&self) -> Option<&mut WindowsTrayData> {
        let ptr = GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as *mut WindowsTrayData;
        ptr.as_mut()
    }
}

#[cfg(target_os = "windows")]
impl Drop for WindowsTray {
    fn drop(&mut self) {
        unsafe {
            self.unregister();
            let ptr = GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as *mut WindowsTrayData;
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            if !ptr.is_null() {
                drop(Box::from_raw(ptr));
            }
            DestroyWindow(self.hwnd);
        }
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn windows_tray_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_NCCREATE => {
            let create = lparam as *const CREATESTRUCTW;
            if !create.is_null() {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, (*create).lpCreateParams as isize);
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_NATIVE_TRAY => {
            let event = lparam as u32;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP || event == WM_LBUTTONDBLCLK {
                show_windows_tray_menu(hwnd);
                return 0;
            }
            0
        }
        WM_DESTROY => 0,
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

#[cfg(target_os = "windows")]
unsafe fn show_windows_tray_menu(hwnd: HWND) {
    let Some(data) = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowsTrayData).as_mut()
    else {
        return;
    };
    data.commands.clear();
    let menu = CreatePopupMenu();
    if menu.is_null() {
        return;
    }
    let mut next_id = 1000_u16;
    append_windows_menu_entries(menu, &data.entries, &mut data.commands, &mut next_id);

    let mut cursor = POINT { x: 0, y: 0 };
    if GetCursorPos(&mut cursor) == 0 {
        DestroyMenu(menu);
        return;
    }
    SetForegroundWindow(hwnd);
    let selected = TrackPopupMenu(
        menu,
        TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
        cursor.x,
        cursor.y,
        0,
        hwnd,
        std::ptr::null(),
    );
    PostMessageW(hwnd, WM_NULL, 0, 0);
    if selected != 0 {
        if let Some((_, command)) = data.commands.iter().find(|(id, _)| *id == selected as u16) {
            let _ = data.proxy.send_event(command.clone());
        }
    }
    DestroyMenu(menu);
}

#[cfg(target_os = "windows")]
unsafe fn append_windows_menu_entries(
    menu: HMENU,
    entries: &[TrayMenuEntry],
    commands: &mut Vec<(u16, String)>,
    next_id: &mut u16,
) {
    for entry in entries {
        match entry {
            TrayMenuEntry::Separator => {
                AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            }
            TrayMenuEntry::Command {
                label,
                command,
                enabled,
            } => {
                let id = *next_id;
                *next_id = next_id.saturating_add(1);
                commands.push((id, command.clone()));
                let label = wide_null(label);
                let enabled_flag = if *enabled { MF_ENABLED } else { MF_GRAYED };
                AppendMenuW(menu, MF_STRING | enabled_flag, id as usize, label.as_ptr());
            }
            TrayMenuEntry::Submenu { label, children } => {
                let submenu = CreatePopupMenu();
                if !submenu.is_null() {
                    append_windows_menu_entries(submenu, children, commands, next_id);
                    let label = wide_null(label);
                    AppendMenuW(menu, MF_POPUP | MF_STRING, submenu as usize, label.as_ptr());
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[allow(dead_code)]
fn load_frame_rgba(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let image = ImageReader::open(path).ok()?.decode().ok()?.into_rgba8();
    let (width, height) = image.dimensions();
    Some((width, height, image.into_raw()))
}
