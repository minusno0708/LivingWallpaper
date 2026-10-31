use crate::simulation::Fish;
use anyhow::Context;
use bytemuck::{Pod, Zeroable};
use std::sync::Arc;
use winit::window::Window;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Instance {
    position: [f32; 2],
    velocity: [f32; 2],
    phase: f32,
    hue: f32,
}
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct View {
    size: [f32; 2],
    time: f32,
    padding: f32,
}
pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    instances: wgpu::Buffer,
    view: wgpu::Buffer,
    bindings: wgpu::BindGroup,
    pub adapter_name: String,
}

#[cfg(test)]
mod tests {
    #[test]
    fn fish_shader_is_valid() {
        let module =
            naga::front::wgsl::parse_str(include_str!("fish.wgsl")).expect("parse fish shader");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("validate fish shader");
    }
}
impl Renderer {
    pub async fn new(window: Arc<Window>) -> anyhow::Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window)?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .context("No compatible GPU adapter")?;
        let adapter_name = format!(
            "{} ({:?})",
            adapter.get_info().name,
            adapter.get_info().backend
        );
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Fish GPU"),
                    required_features: wgpu::Features::empty(),
                    // Keep conservative buffer limits, but allow surfaces up to the GPU's
                    // texture resolution (downlevel defaults cap this at 2048).
                    required_limits: wgpu::Limits::downlevel_defaults()
                        .using_resolution(adapter.limits()),
                },
                None,
            )
            .await?;
        let mut config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .context("No surface configuration")?;
        // Non-blocking mailbox where available, avoiding an extra refresh wait at capped FPS.
        if surface
            .get_capabilities(&adapter)
            .present_modes
            .contains(&wgpu::PresentMode::Mailbox)
        {
            config.present_mode = wgpu::PresentMode::Mailbox;
        }
        config.desired_maximum_frame_latency = 1;
        anyhow::ensure!(
            config.width <= device.limits().max_texture_dimension_2d
                && config.height <= device.limits().max_texture_dimension_2d,
            "Wallpaper size {}x{} exceeds the GPU texture limit {}",
            config.width,
            config.height,
            device.limits().max_texture_dimension_2d
        );
        surface.configure(&device, &config);
        let view = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("view"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view.as_entire_binding(),
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("fish"),
            source: wgpu::ShaderSource::Wgsl(include_str!("fish.wgsl").into()),
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let attributes = wgpu::vertex_attr_array![0=>Float32x2,1=>Float32x2,2=>Float32,3=>Float32];
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("fish instancing"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 24,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &attributes,
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
        });
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("fish instances"),
            size: 1000 * 24,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            surface,
            device,
            queue,
            config,
            pipeline,
            instances,
            view,
            bindings,
            adapter_name,
        })
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }
    pub fn render(&mut self, fish: &[Fish], time: f32) -> Result<(), wgpu::SurfaceError> {
        let frame = self.surface.get_current_texture()?;
        let data: Vec<_> = fish
            .iter()
            .enumerate()
            .map(|(i, f)| Instance {
                position: f.position.to_array(),
                velocity: f.velocity.to_array(),
                phase: f.phase,
                hue: (i % 7) as f32 / 7.0,
            })
            .collect();
        self.queue
            .write_buffer(&self.instances, 0, bytemuck::cast_slice(&data));
        self.queue.write_buffer(
            &self.view,
            0,
            bytemuck::bytes_of(&View {
                size: [self.config.width as f32, self.config.height as f32],
                time,
                padding: 0.0,
            }),
        );
        let target = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("ocean"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.006,
                            g: 0.025,
                            b: 0.045,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bindings, &[]);
            pass.set_vertex_buffer(0, self.instances.slice(..));
            pass.draw(0..6, 0..data.len() as u32);
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        Ok(())
    }
}
