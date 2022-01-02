mod camera;
mod light;
mod object;
mod renderer;
mod texture;

use cgmath::prelude::*;
use object::Object;
use renderer::{DrawModel, InstanceRaw, ModelRenderer};
use winit::{
    dpi::PhysicalPosition,
    event::*,
    event_loop::{ControlFlow, EventLoop},
    window::Window,
    window::WindowBuilder,
};

const NUM_INSTANCES_PER_ROW: u32 = 10;

struct Instance {
    position: cgmath::Vector3<f32>,
    rotation: cgmath::Quaternion<f32>,
}

impl Instance {
    fn to_raw(&self) -> InstanceRaw {
        let model =
            cgmath::Matrix4::from_translation(self.position) * cgmath::Matrix4::from(self.rotation);
        InstanceRaw {
            model: model.into(),
            normal: cgmath::Matrix3::from(self.rotation).into(),
        }
    }
}

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

        const SPACE_BETWEEN: f32 = 3.0;
        let instances = (0..NUM_INSTANCES_PER_ROW)
            .flat_map(|z| {
                (0..NUM_INSTANCES_PER_ROW).map(move |x| {
                    let x = SPACE_BETWEEN * (x as f32 - NUM_INSTANCES_PER_ROW as f32 / 2.0);
                    let z = SPACE_BETWEEN * (z as f32 - NUM_INSTANCES_PER_ROW as f32 / 2.0);

                    let position = cgmath::Vector3 { x, y: 0.0, z };

                    let rotation = if position.is_zero() {
                        cgmath::Quaternion::from_axis_angle(
                            cgmath::Vector3::unit_z(),
                            cgmath::Deg(0.0),
                        )
                    } else {
                        cgmath::Quaternion::from_axis_angle(position.normalize(), cgmath::Deg(45.0))
                    };

                    Instance { position, rotation }
                })
            })
            .collect::<Vec<_>>();

        let instance_data = instances.iter().map(Instance::to_raw).collect::<Vec<_>>();
        let instance_len = instance_data.len();

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

        let cube_object = lens_objects.pop().unwrap();
        let obj_renderer = ModelRenderer::new_renderer(
            renderer::Model::load(&device, &queue, cube_object.object).unwrap(),
            &device,
            &config,
            &camera_binder,
            &light_binder,
            std::borrow::Cow::Borrowed(cube_object.shader_file),
            Some(instance_data),
            Some(instance_len),
        );

        let light_object = lens_objects.pop().unwrap();
        let light_renderer = ModelRenderer::new_renderer(
            renderer::Model::load(&device, &queue, light_object.object).unwrap(),
            &device,
            &config,
            &camera_binder,
            &light_binder,
            std::borrow::Cow::Borrowed(light_object.shader_file),
            None,
            None,
        );

        let mut model_renderers = Vec::new();
        model_renderers.push(light_renderer);
        model_renderers.push(obj_renderer);

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
                render_pass.draw_model(&renderer, bind_groups);
            }
        }
        // submit will accept anything that implements IntoIter
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

struct LensObject<'a> {
    object: Object,
    shader_file: &'a str,
}

pub struct Lens<'a> {
    // add a light
    // add a camera
    // add meshes
    lens_objects: Vec<LensObject<'a>>,
}

impl<'a> Lens<'a> {
    pub fn new() -> Lens<'a> {
        let res_dir = std::path::Path::new(env!("OUT_DIR")).join("res");
        let cube_object = object::Object::load_from(res_dir.join("cube").join("cube.obj"));

        let mut light_object = object::Object::load_from(res_dir.join("cube").join("cube.obj"));
        light_object.textures = None;

        let mut lens_objects = Vec::new();
        lens_objects.push(LensObject {
            object: light_object,
            shader_file: include_str!("light.wgsl").into(),
        });
        lens_objects.push(LensObject {
            object: cube_object,
            shader_file: include_str!("shader.wgsl").into(),
        });

        Lens { lens_objects }
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

pub fn hello_from_lens() {
    println!("Hello from lens.");
}
