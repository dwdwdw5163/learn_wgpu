use bytemuck::bytes_of;
use cgmath::{SquareMatrix, Rotation3, Zero, Angle};
use light::LightUniform;
use texture::Texture;
use tracing::info;
use transform::{Transform, TransformUniform};
use winit::{
    event::{Event, WindowEvent, KeyEvent, ElementState, MouseButton, DeviceEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder, keyboard::{PhysicalKey, KeyCode},
};
use wgpu::util::DeviceExt;

mod texture;
mod camera;
mod transform;
mod instance;
mod depth_pass;
mod model;
mod light;
// lib.rs
use winit::window::Window;

use camera::*;
use instance::*;
use model::{DrawModel, Vertex};

fn create_render_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    color_format: wgpu::TextureFormat,
    depth_format: Option<wgpu::TextureFormat>,
    vertex_layouts: &[wgpu::VertexBufferLayout],
    shader: wgpu::ShaderModuleDescriptor,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(shader);

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Render Pipeline"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: vertex_layouts,
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState {
                    alpha: wgpu::BlendComponent::REPLACE,
                    color: wgpu::BlendComponent::REPLACE,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: Some(wgpu::Face::Back),
            // Setting this to anything other than Fill requires Features::NON_FILL_POLYGON_MODE
            polygon_mode: wgpu::PolygonMode::Fill,
            // Requires Features::DEPTH_CLIP_CONTROL
            unclipped_depth: false,
            // Requires Features::CONSERVATIVE_RASTERIZATION
            conservative: false,
        },
        depth_stencil: depth_format.map(|format| wgpu::DepthStencilState {
            format,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState {
            count: 1,
            mask: !0,
            alpha_to_coverage_enabled: false,
        },
        multiview: None,
    })
}


struct State {
    surface: wgpu::Surface,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: winit::dpi::PhysicalSize<u32>,
    // The window must be declared after the surface so
    // it gets dropped after it as the surface contains
    // unsafe references to the window's resources.
    window: Window,
    render_pipeline: wgpu::RenderPipeline,
    obj_model: model::Model,


    texture_bind_group_layout: wgpu::BindGroupLayout,
    depth_texture: Texture,
    

    camera: camera::Camera, // UPDATED!
    projection: camera::Projection, // NEW!
    camera_controller: camera::CameraController, // UPDATED!
    // ...
    // NEW!
    mouse_pressed: bool,

    camera_uniform: CameraUniform,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,

    light_uniform: LightUniform,
    light_buffer: wgpu::Buffer,
    light_bind_group: wgpu::BindGroup,
    light_render_pipeline: wgpu::RenderPipeline,

    instances: Vec<Instance>,
    instance_buffer: wgpu::Buffer,

    

}

impl State {
    // Creating some of the wgpu types requires async code
    async fn new(window: Window) -> Self {
        let size = window.inner_size();

        // The instance is a handle to our GPU
        // Backends::all => Vulkan + Metal + DX12 + Browser WebGPU
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });
        
        // # Safety
        //
        // The surface needs to live as long as the window that created it.
        // State owns the window, so this should be safe.
        let surface = unsafe { instance.create_surface(&window) }.unwrap();

        let adapter = instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            },
        ).await.unwrap();

        let (device, queue) = adapter.request_device(
            &wgpu::DeviceDescriptor {
                features: wgpu::Features::empty(),
                // WebGL doesn't support all of wgpu's features, so if
                // we're building for the web, we'll have to disable some.
                limits: if cfg!(target_arch = "wasm32") {
                    wgpu::Limits::downlevel_webgl2_defaults()
                } else {
                    wgpu::Limits::default()
                },
                label: None,
            },
            None, // Trace path
        ).await.unwrap();


        let surface_caps = surface.get_capabilities(&adapter);
        // Shader code in this tutorial assumes an sRGB surface texture. Using a different
        // one will result in all the colors coming out darker. If you want to support non
        // sRGB surfaces, you'll need to account for that when drawing to the frame.
        let surface_format = surface_caps.formats.iter()
            .copied()
            .filter(|f| f.is_srgb())
            .next()
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        

        let texture_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
                        // This should match the filterable field of the
                        // corresponding Texture entry above.
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("texture_bind_group_layout"),
            });

        info!("Load model");
        let obj_model = model::load_obj("/Users/zhang/Downloads/GF2_Cheeta/CheetaDefault.obj", &device, &queue, &texture_bind_group_layout).unwrap();

        let depth_texture = texture::Texture::create_depth_texture(&device, &config, "depth_texture");


        let camera = camera::Camera::new((0.0, 5.0, 10.0), cgmath::Deg(-90.0), cgmath::Deg(-20.0));
        let projection = camera::Projection::new(config.width, config.height, cgmath::Deg(45.0), 0.1, 100.0);
        let camera_controller = camera::CameraController::new(4.0, 0.4);



        let mut camera_uniform = CameraUniform::new();
        camera_uniform.update_view_proj(&camera, &projection); // UPDATED!
        let camera_buffer = camera_uniform.create_camera_buffer(&device);


        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }
            ],
            label: Some("camera_bind_group_layout"),
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &camera_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                }
            ],
            label: Some("camera_bind_group"),
        });

        let light_uniform = light::LightUniform {
            position: [2.0, 2.0, 2.0, 1.0],
            color: [1.0, 1.0, 1.0, 1.0],
        };
        let (light_buffer, light_bind_group_layout, light_bind_group) =
            light::create_light_buffer_and_bind_group(&device, &light_uniform);


        //shader file & render pipeline
        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[
                    &texture_bind_group_layout,
                    &camera_bind_group_layout,
                    &light_bind_group_layout,
                ],
                push_constant_ranges: &[],
            });
        
        let render_pipeline = {
            let shader = wgpu::include_wgsl!("shader.wgsl");

            create_render_pipeline(
                &device,
                &render_pipeline_layout,
                config.format,
                Some(texture::Texture::DEPTH_FORMAT),
                &[model::ModelVertex::desc(), InstanceRaw::desc()],
                shader,
            )
        };

  
        
        let instances = vec![
            Instance {
                position: cgmath::Vector3::zero(),
                rotation: cgmath::Quaternion::from_angle_y(cgmath::Deg(0.0)),
                scaling: cgmath::Vector3::new(1.0, 1.0, 1.0),
            },
        ];
        let instance_data = instances.iter().map(Instance::to_raw).collect::<Vec<_>>();
        let instance_buffer = instance::create_instances_buffer(&device, &instance_data);

        // lib.rs
        let light_render_pipeline = {
            let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Light Pipeline Layout"),
                bind_group_layouts: &[
                    &camera_bind_group_layout,
                    &light_bind_group_layout
                ],
                push_constant_ranges: &[],
            });
            let shader = wgpu::include_wgsl!("light.wgsl");

            create_render_pipeline(
                &device,
                &layout,
                config.format,
                Some(texture::Texture::DEPTH_FORMAT),
                &[model::ModelVertex::desc()],
                shader,
            )
        };


        Self {
            window,
            surface,
            device,
            queue,
            config,
            size,
            render_pipeline,
        

            obj_model,
            
            texture_bind_group_layout,
            depth_texture,
            
            camera,
            projection,
            camera_controller,
            mouse_pressed: false,
            
            camera_uniform,
            camera_buffer,
            camera_bind_group,

            light_uniform,
            light_buffer,
            light_bind_group,
            light_render_pipeline,
           

            instances,
            instance_buffer,
        }

    }

    pub fn window(&self) -> &Window {
        &self.window
    }

    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
            self.depth_texture = texture::Texture::create_depth_texture(&self.device, &self.config, "depth_texture");
            self.projection.resize(new_size.width, new_size.height);

            
        }
    }

    fn input(&mut self, event: &WindowEvent) -> bool {
        match event {
            WindowEvent::KeyboardInput {
                event:
                KeyEvent {
                    physical_key: PhysicalKey::Code(key),
                    state,
                    ..
                },
                ..
            } => self.camera_controller.process_keyboard(*key, *state),
            WindowEvent::MouseWheel { delta, .. } => {
                self.camera_controller.process_scroll(delta);
                true
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                self.mouse_pressed = *state == ElementState::Pressed;
                true
            }
            _ => false,
        }


    }

    fn update(&mut self, dt: instant::Duration) {
        self.camera_controller.update_camera(&mut self.camera, dt);
        self.camera_uniform.update_view_proj(&self.camera, &self.projection);
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[self.camera_uniform]));

        // Update the light
        let old_position = cgmath::Vector4::from(self.light_uniform.position);
        // self.light_uniform.position =
        //     (cgmath::Matrix4::from_axis_angle((0.0, 1.0, 0.0).into(), cgmath::Deg(1.0))
        //      * old_position)
        //     .into();

        self.light_uniform.position =
            (cgmath::Matrix4::from_axis_angle((0.0, 1.0, 0.0).into(), cgmath::Deg(60.0 * dt.as_secs_f32()))
             * old_position).into(); // UPDATED!

        self.queue.write_buffer(&self.light_buffer, 0, bytemuck::cast_slice(&[self.light_uniform]));

    }

    fn render(&mut self, color: wgpu::Color) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });
        
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth_texture.view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            occlusion_query_set: None,
            timestamp_writes: None,
        });
        // NEW!
        render_pass.set_vertex_buffer(1, self.instance_buffer.slice(..));

        info!("start light render pipeline");
        use crate::model::DrawLight; // NEW!
        render_pass.set_pipeline(&self.light_render_pipeline); // NEW!
        render_pass.draw_light_model(
            &self.obj_model,
            &self.camera_bind_group,
            &self.light_bind_group,
        ); // NEW!

        info!("start model render pipeline");
        render_pass.set_pipeline(&self.render_pipeline); // 2.
        render_pass.draw_model_instanced(
            &self.obj_model,
            0..self.instances.len() as u32,
            &self.camera_bind_group,
            &self.light_bind_group,
        );

        drop(render_pass);


        // submit will accept anything that implements IntoIter
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())

    }
}


pub async fn run() {
    
    let event_loop = EventLoop::new().unwrap();
    let window = WindowBuilder::new().build(&event_loop).unwrap();

    // ControlFlow::Poll continuously runs the event loop, even if the OS hasn't
    // dispatched any events. This is ideal for games and similar applications.
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut state = State::new(window).await;
    let mut last_render_time = instant::Instant::now();  // NEW!


    // run()
    event_loop.run(move |event, elwh| {
        match event {
            Event::AboutToWait => {
                // Application update code.

                // Queue a RedrawRequested event.
                //
                // You only need to call this if you've determined that you need to redraw in
                // applications which do not always need to. Applications that redraw continuously
                // can render here instead.
                state.window.request_redraw();
            },
            Event::DeviceEvent {
                event: DeviceEvent::MouseMotion{ delta, },
                .. // We're not using device_id currently
            } => if state.mouse_pressed {
                state.camera_controller.process_mouse(delta.0, delta.1)
            },
            Event::WindowEvent {
                ref event,
                ..
            } => {
                if !state.input(event) { // UPDATED!
                    match event {
                        WindowEvent::CloseRequested
                            | WindowEvent::KeyboardInput {
                                event:
                                KeyEvent {
                                    state: ElementState::Pressed,
                                    physical_key: PhysicalKey::Code(KeyCode::Escape),
                                    ..
                                },
                                ..
                            } => elwh.exit(),
                        WindowEvent::Resized(physical_size) => {
                            state.resize(*physical_size);
                            state.window.request_redraw();
                        },
                        _ => {}
                    }
                } else {
                    state.window.request_redraw();
                }

                if *event == WindowEvent::RedrawRequested {
                    let now = instant::Instant::now();
                    let dt = now - last_render_time;
                    last_render_time = now;
                    state.update(dt);
                    match state.render(wgpu::Color::BLACK) {
                        Ok(_) => {}
                        // Reconfigure the surface if lost
                        Err(wgpu::SurfaceError::Lost) => state.resize(state.size),
                        // The system is out of memory, we should probably quit
                        Err(wgpu::SurfaceError::OutOfMemory) => elwh.exit(),
                        // All other errors (Outdated, Timeout) should be resolved by the next frame
                        Err(e) => tracing::error!("{:?}", e),
                    }
                }
            },

            _ => {}
        }
    }).unwrap();



}
