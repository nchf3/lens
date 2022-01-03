mod camera;
mod light;
mod object;
mod renderer;
mod texture;

pub use object::Object;
pub use renderer::InstanceRaw;
use renderer::{DrawModel, ModelRenderer};
use winit::{
    dpi::PhysicalPosition,
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::Window,
    window::WindowBuilder,
};

struct Scene {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    depth_texture: texture::Texture,
    mouse_pressed: bool,
    // camera & light binders
    camera_binder: camera::Camera,
    light_binder: light::Light,
    // models to draw
    // renderers for each model to draw
    model_renderers: Vec<ModelRenderer>,
}

impl<'a> Scene {
    // Creating some of the wgpu types requires async code
    async fn new(window: &Window, lens_objects: &mut Vec<LensObject<'a>>) -> Scene {
        let size = window.inner_size();

        // The instance is a handle to our GPU
        // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(wgpu::Backends::all());
        let surface = unsafe { instance.create_surface(window) };
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                    label: None,
                },
                None, // Trace path
            )
            .await
            .unwrap();

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface.get_preferred_format(&adapter).unwrap(),
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
        };
        surface.configure(&device, &config);

        let depth_texture =
            texture::Texture::create_depth_texture(&device, &config, "depth_texture");

        // create the camera
        let camera_binder = camera::Camera::new(&device, &config);

        // create light bind_group_layout and bind group
        let light_uniform = light::LightUniform {
            position: [2.0, 2.0, 2.0],
            _padding: 0,
            color: [0.2, 0.5, 0.7],
        };
        let light_binder = light::Light::bind(&device, light_uniform);

        let mut model_renderers = Vec::new();
        for _ in 0..lens_objects.len() {
            let object = lens_objects.pop().unwrap();
            let (instances_data, instances_len) =
                if let Some((data, len)) = object.instances.clone() {
                    (Some(data.clone()), Some(len.clone()))
                } else {
                    (None, None)
                };
            let cube_renderer = ModelRenderer::new_renderer(
                renderer::Model::load(&device, &queue, object.object).unwrap(),
                &device,
                &config,
                &camera_binder,
                &light_binder,
                std::borrow::Cow::Borrowed(object.shader_file),
                instances_data,
                instances_len,
            );
            model_renderers.push(cube_renderer);
        }

        Self {
            surface,
            device,
            queue,
            config,
            size,
            depth_texture,
            mouse_pressed: false,
            camera_binder,
            light_binder,
            model_renderers,
        }
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.camera_binder
                .projection
                .resize(new_size.width, new_size.height);
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.depth_texture =
                texture::Texture::create_depth_texture(&self.device, &self.config, "depth_texture");
        }
    }

    fn input(&mut self, event: &DeviceEvent) -> bool {
        match event {
            DeviceEvent::Key(KeyboardInput {
                virtual_keycode: Some(key),
                state,
                ..
            }) => self
                .camera_binder
                .camera_controller
                .process_keyboard(*key, *state),
            DeviceEvent::MouseWheel { delta, .. } => {
                self.camera_binder.camera_controller.process_scroll(delta);
                true
            }
            DeviceEvent::Button {
                button: 1, // Left Mouse Button
                state,
            } => {
                self.mouse_pressed = *state == ElementState::Pressed;
                true
            }
            DeviceEvent::MouseMotion { delta } => {
                if self.mouse_pressed {
                    self.camera_binder
                        .camera_controller
                        .process_mouse(delta.0, delta.1);
                }
                true
            }
            _ => false,
        }
    }

    fn update(&mut self, dt: std::time::Duration) {
        // update camera position
        self.camera_binder.update(&self.queue, dt);

        // Update the light
        self.light_binder.update(&self.queue, dt);
    }

    fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // create bind_groups for each model to render
        let bind_groups = &[
            &self.camera_binder.bind_group,
            &self.light_binder.bind_group,
        ];

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[
                    // This is what [[location(0)]] in the fragment shader targets
                    wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.1,
                                g: 0.2,
                                b: 0.3,
                                a: 1.0,
                            }),
                            store: true,
                        },
                    },
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });

            for renderer in &self.model_renderers {
                render_pass.draw_model(renderer, bind_groups);
            }
        }
        // submit will accept anything that implements IntoIter
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

pub struct LensObject<'a> {
    pub object: Object,
    pub shader_file: &'a str,
    pub instances: Option<(Vec<InstanceRaw>, usize)>,
}

pub struct Lens<'a> {
    // add a light
    // add a camera
    // add meshes
    lens_objects: Vec<LensObject<'a>>,
}

impl<'a> Lens<'a> {
    pub fn new() -> Lens<'a> {
        Lens {
            lens_objects: Vec::new(),
        }
    }

    pub fn add_object(&mut self, lens_object: LensObject<'a>) {
        self.lens_objects.push(lens_object);
    }

    pub fn run(&mut self) {
        env_logger::init();
        let mut last_render_time = std::time::Instant::now();

        let event_loop = EventLoop::new();
        let window = WindowBuilder::new().build(&event_loop).unwrap();
        // Scene::new uses async code, so we're going to wait for it to finish
        let mut scene = pollster::block_on(Scene::new(&window, &mut self.lens_objects));

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Poll;
            match event {
                Event::DeviceEvent {
                    ref event,
                    .. // We're not using device_id currently
                } => {
                    scene.input(event);
                }
                Event::WindowEvent {
                    ref event,
                    window_id,
                } if window_id == window.id() => {
                    match event {
                        WindowEvent::CloseRequested
                        | WindowEvent::KeyboardInput {
                            input:
                                KeyboardInput {
                                    state: ElementState::Pressed,
                                    virtual_keycode: Some(VirtualKeyCode::Escape),
                                    ..
                                },
                            ..
                        } => *control_flow = ControlFlow::Exit,
                        WindowEvent::Resized(physical_size) => {
                            scene.resize(*physical_size);
                        }
                        WindowEvent::ScaleFactorChanged { new_inner_size, .. } => {
                            // new_inner_size is &&mut so we have to dereference it twice
                            scene.resize(**new_inner_size);
                        }
                        _ => {}
                    }
                }
                Event::RedrawRequested(_) => {
                    let now = std::time::Instant::now();
                    let dt = now - last_render_time;
                    last_render_time = now;
                    scene.update(dt);
                    match scene.render() {
                        Ok(_) => {}
                        // Reconfigure the surface if lost
                        Err(wgpu::SurfaceError::Lost) => scene.resize(scene.size),
                        // The system is out of memory, we should probably quit
                        Err(wgpu::SurfaceError::OutOfMemory) => *control_flow = ControlFlow::Exit,
                        // All other errors (Outdated, Timeout) should be resolved by the next frame
                        Err(e) => eprintln!("Error : {:?}", e),
                    }
                }
                Event::MainEventsCleared => {
                    // RedrawRequested will only trigger once, unless we manually
                    // request it.
                    window.request_redraw();
                }
                _ => {}
            }
        });
    }
}
