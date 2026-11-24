use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use cosmic_text::{
    Attrs, Buffer, Ellipsize, EllipsizeHeightLimit, Family, FontSystem, Metrics, Shaping,
    SwashCache, Weight, Wrap,
};
use glyphon::{ColorMode, Resolution, TextArea, TextAtlas, TextBounds, TextRenderer, Viewport};
use rb_ui::frame::{Align, FontFamily, Frame, Icon, IconShape, RectShape, TextShape};
use rb_ui::Color;
use wgpu::util::DeviceExt;
use winit::window::Window;

const SHADER: &str = include_str!("shader.wgsl");
const INITIAL_INSTANCES: u64 = 512;
const BACKGROUND: wgpu::Color = wgpu::Color {
    r: 0.024,
    g: 0.024,
    b: 0.027,
    a: 1.0,
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct RectInstance {
    rect: [f32; 4],
    color: [f32; 4],
    params: [f32; 4],
    border_color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct Uniforms {
    screen: [f32; 2],
    _pad: [f32; 2],
}

struct PreparedText {
    left: f32,
    top: f32,
    bounds: TextBounds,
    color: cosmic_text::Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderError {
    Outdated,
    Lost,
    Validation,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    size: (u32, u32),
    scale: f32,
    linearize: bool,

    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    instance_buffer: wgpu::Buffer,
    instance_capacity: u32,

    font_system: FontSystem,
    swash_cache: SwashCache,
    atlas: TextAtlas,
    viewport: Viewport,
    base_text: TextRenderer,
    overlay_text: TextRenderer,
    ui_family: &'static str,
}

impl Renderer {
    pub async fn new(window: Arc<Window>) -> Result<Self, String> {
        let size = window.inner_size();
        let width = size.width.max(1);
        let height = size.height.max(1);
        let scale = window.scale_factor() as f32;

        let backends = if cfg!(windows) {
            wgpu::Backends::DX12
        } else {
            wgpu::Backends::PRIMARY
        };
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let surface = instance
            .create_surface(window)
            .map_err(|error| format!("create surface: {error}"))?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|error| format!("request adapter: {error}"))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("rushbross device"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
                ..Default::default()
            })
            .await
            .map_err(|error| format!("request device: {error}"))?;

        let capabilities = surface.get_capabilities(&adapter);
        let srgb = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| format.is_srgb());
        let format = srgb
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| "surface has no supported formats".to_string())?;
        let linearize = format.is_srgb();

        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or_else(|| "surface has no default configuration".to_string())?;
        config.format = format;
        config.width = width;
        config.height = height;
        surface.configure(&device, &config);

        let cache = glyphon::Cache::new(&device);

        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rushbross uniforms"),
            contents: bytemuck::bytes_of(&Uniforms {
                screen: [width as f32, height as f32],
                _pad: [0.0; 2],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("rushbross bind group layout"),
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

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rushbross bind group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("rushbross rect shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("rushbross pipeline layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });

        let vertex_layouts = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<RectInstance>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x4,
                1 => Float32x4,
                2 => Float32x4,
                3 => Float32x4,
            ],
        })];

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("rushbross rect pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &vertex_layouts,
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });

        let instance_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rushbross instances"),
            size: INITIAL_INSTANCES * std::mem::size_of::<RectInstance>() as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let font_system = FontSystem::new();
        let ui_family = pick_family(&font_system);

        let color_mode = if linearize {
            ColorMode::Accurate
        } else {
            ColorMode::Web
        };
        let mut atlas = TextAtlas::with_color_mode(&device, &queue, &cache, format, color_mode);
        let base_text =
            TextRenderer::new(&mut atlas, &device, wgpu::MultisampleState::default(), None);
        let overlay_text =
            TextRenderer::new(&mut atlas, &device, wgpu::MultisampleState::default(), None);
        let viewport = Viewport::new(&device, &cache);

        Ok(Self {
            surface,
            device,
            queue,
            config,
            size: (width, height),
            scale,
            linearize,
            pipeline,
            uniform_buffer,
            bind_group,
            instance_buffer,
            instance_capacity: INITIAL_INSTANCES as u32,
            font_system,
            swash_cache: SwashCache::new(),
            atlas,
            viewport,
            base_text,
            overlay_text,
            ui_family,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if (width, height) == self.size {
            return;
        }
        self.size = (width, height);
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&Uniforms {
                screen: [width as f32, height as f32],
                _pad: [0.0; 2],
            }),
        );
    }

    pub fn reconfigure(&mut self) {
        self.surface.configure(&self.device, &self.config);
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale
    }

    pub fn set_scale_factor(&mut self, scale: f32) {
        self.scale = scale.max(0.5);
    }

    pub fn render(&mut self, frame: &Frame) -> Result<bool, RenderError> {
        let (output, suboptimal) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => (texture, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => (texture, true),
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                return Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Outdated => return Err(RenderError::Outdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(RenderError::Lost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(RenderError::Validation),
        };
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut base_instances = self.shape_instances(&frame.base.shapes);
        base_instances.extend(self.icon_instances(&frame.base.icons));
        let mut overlay_instances = self.shape_instances(&frame.overlay.shapes);
        overlay_instances.extend(self.icon_instances(&frame.overlay.icons));

        let mut base_buffers: Vec<Buffer> = Vec::with_capacity(frame.base.texts.len());
        let mut base_prepared: Vec<PreparedText> = Vec::with_capacity(frame.base.texts.len());
        let mut overlay_buffers: Vec<Buffer> = Vec::with_capacity(frame.overlay.texts.len());
        let mut overlay_prepared: Vec<PreparedText> = Vec::with_capacity(frame.overlay.texts.len());

        for text in &frame.base.texts {
            let prepared = self.build_text(text, &mut base_buffers, &mut overlay_instances);
            base_prepared.push(prepared);
        }
        for text in &frame.overlay.texts {
            let prepared = self.build_text(text, &mut overlay_buffers, &mut overlay_instances);
            overlay_prepared.push(prepared);
        }

        let base_count = base_instances.len() as u32;
        let overlay_count = overlay_instances.len() as u32;
        base_instances.extend_from_slice(&overlay_instances);
        let total = base_instances.len() as u32;

        if total > self.instance_capacity {
            self.instance_capacity = total.next_power_of_two();
            self.instance_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("rushbross instances"),
                size: self.instance_capacity as u64 * std::mem::size_of::<RectInstance>() as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if total > 0 {
            self.queue.write_buffer(
                &self.instance_buffer,
                0,
                bytemuck::cast_slice(&base_instances),
            );
        }

        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.size.0,
                height: self.size.1,
            },
        );

        let base_areas = base_buffers
            .iter()
            .zip(base_prepared.iter())
            .map(|(buffer, prepared)| TextArea {
                buffer,
                left: prepared.left,
                top: prepared.top,
                scale: 1.0,
                bounds: prepared.bounds,
                default_color: prepared.color,
                custom_glyphs: &[],
            })
            .collect::<Vec<_>>();

        let overlay_areas = overlay_buffers
            .iter()
            .zip(overlay_prepared.iter())
            .map(|(buffer, prepared)| TextArea {
                buffer,
                left: prepared.left,
                top: prepared.top,
                scale: 1.0,
                bounds: prepared.bounds,
                default_color: prepared.color,
                custom_glyphs: &[],
            })
            .collect::<Vec<_>>();

        self.base_text
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                base_areas,
                &mut self.swash_cache,
            )
            .map_err(|error| {
                eprintln!("prepare base text: {error}");
                RenderError::Validation
            })?;

        self.overlay_text
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                overlay_areas,
                &mut self.swash_cache,
            )
            .map_err(|error| {
                eprintln!("prepare overlay text: {error}");
                RenderError::Validation
            })?;

        self.atlas.trim();

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("rushbross encoder"),
            });

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rushbross pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(BACKGROUND),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            if base_count > 0 {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                pass.draw(0..6, 0..base_count);
            }

            let _ = self
                .base_text
                .render(&self.atlas, &self.viewport, &mut pass);

            if overlay_count > 0 {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.set_vertex_buffer(0, self.instance_buffer.slice(..));
                pass.draw(0..6, base_count..base_count + overlay_count);
            }

            let _ = self
                .overlay_text
                .render(&self.atlas, &self.viewport, &mut pass);
        }

        self.queue.submit(Some(encoder.finish()));
        self.queue.present(output);
        if suboptimal {
            self.surface.configure(&self.device, &self.config);
        }
        Ok(true)
    }

    fn build_text(
        &mut self,
        text: &TextShape,
        buffers: &mut Vec<Buffer>,
        overlay_instances: &mut Vec<RectInstance>,
    ) -> PreparedText {
        let scale = self.scale;
        let font_size = (text.size * scale).max(1.0);
        let line_height = font_size * text.line_height;
        let metrics = Metrics::new(font_size, line_height);
        let family = match text.family {
            FontFamily::Sans => Family::Name(self.ui_family),
        };
        let attrs = Attrs::new().family(family).weight(Weight(text.weight));

        let mut buffer = Buffer::new(&mut self.font_system, metrics);
        buffer.set_text(
            &text.text,
            &attrs,
            Shaping::Advanced,
            Some(text_align(text.align)),
        );
        let width = text.width;
        let wraps = text.wrap && width.is_finite() && width > 0.0;
        buffer.set_wrap(if wraps { Wrap::Word } else { Wrap::None });

        if width.is_finite() && width > 0.0 {
            buffer.set_size(Some(width * scale), None);
            if text.ellipsis {
                buffer.set_ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)));
            }
        } else {
            buffer.set_size(None, None);
        }
        buffer.shape_until_scroll(&mut self.font_system, false);

        let measured = buffer
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0f32, f32::max);
        let lines = buffer.layout_runs().count().max(1) as f32;

        let left = text.x * scale;
        let top = text.y * scale;
        let resolved_left = left;

        let mut bounds_left = if width.is_finite() && width > 0.0 {
            left.floor() as i32
        } else {
            0
        };
        let mut bounds_right = if width.is_finite() && width > 0.0 {
            (left + width * scale).ceil() as i32
        } else {
            self.size.0 as i32
        };
        let mut bounds_top = (top - font_size).floor() as i32;
        let mut bounds_bottom = (top + line_height * lines + font_size * 0.25).ceil() as i32;
        if let Some(clip) = text.clip {
            bounds_left = bounds_left.max((clip.x * scale).floor() as i32);
            bounds_right = bounds_right.min((clip.right() * scale).ceil() as i32);
            bounds_top = bounds_top.max((clip.y * scale).floor() as i32);
            bounds_bottom = bounds_bottom.min((clip.bottom() * scale).ceil() as i32);
        }
        bounds_right = bounds_right.max(bounds_left);
        bounds_bottom = bounds_bottom.max(bounds_top);

        if let Some(caret) = text.caret {
            let caret = caret.min(text.text.len());
            let caret = floor_char_boundary(&text.text, caret);
            let prefix = &text.text[..caret];
            let mut prefix_buffer = Buffer::new(&mut self.font_system, metrics);
            prefix_buffer.set_text(prefix, &attrs, Shaping::Advanced, None);
            prefix_buffer.set_size(None, None);
            prefix_buffer.set_wrap(Wrap::None);
            prefix_buffer.shape_until_scroll(&mut self.font_system, false);
            let prefix_width = prefix_buffer
                .layout_runs()
                .map(|run| run.line_w)
                .fold(0.0f32, f32::max);

            let max_x = if width.is_finite() && width > 0.0 {
                left + width * scale
            } else {
                resolved_left + measured
            };
            let caret_x = (resolved_left + prefix_width).min(max_x);
            let caret_top = top + font_size * 0.12;
            let caret_height = font_size * 1.1;
            let caret_color = self.instance_color(text.caret_color);
            overlay_instances.push(RectInstance {
                rect: [
                    caret_x.round(),
                    caret_top,
                    (1.5 * scale).max(1.0),
                    caret_height,
                ],
                color: caret_color,
                params: [0.5 * scale, 0.0, 0.0, 0.0],
                border_color: [0.0; 4],
            });
        }

        buffers.push(buffer);

        PreparedText {
            left: resolved_left,
            top,
            bounds: TextBounds {
                left: bounds_left.max(0),
                top: bounds_top.max(0),
                right: bounds_right.max(0),
                bottom: bounds_bottom.max(0),
            },
            color: cosmic_text::Color::rgba(text.color.r, text.color.g, text.color.b, text.color.a),
        }
    }

    fn shape_instances(&self, shapes: &[RectShape]) -> Vec<RectInstance> {
        shapes
            .iter()
            .map(|shape| RectInstance {
                rect: [
                    shape.rect.x * self.scale,
                    shape.rect.y * self.scale,
                    shape.rect.width * self.scale,
                    shape.rect.height * self.scale,
                ],
                color: self.instance_color(shape.color),
                params: [
                    shape.radius * self.scale,
                    shape.border_width * self.scale,
                    0.0,
                    0.0,
                ],
                border_color: self.instance_color(shape.border_color),
            })
            .collect()
    }

    fn icon_instances(&self, icons: &[IconShape]) -> Vec<RectInstance> {
        let mut out = Vec::with_capacity(icons.len() * 4);
        for shape in icons {
            let unit = shape.rect.width.min(shape.rect.height);
            let center = shape.rect.center();
            let point =
                |p: (f32, f32)| (center.x + (p.0 - 0.5) * unit, center.y + (p.1 - 0.5) * unit);
            let stroke = shape.stroke;
            let color = self.instance_color(shape.color);

            match shape.icon {
                Icon::Search => {
                    self.push_circle(&mut out, point((0.42, 0.42)), 0.5, stroke, color);
                    self.push_line(
                        &mut out,
                        point((0.58, 0.58)),
                        point((0.82, 0.82)),
                        stroke,
                        color,
                    );
                }
                Icon::Lock => {
                    self.push_line(
                        &mut out,
                        point((0.34, 0.46)),
                        point((0.34, 0.34)),
                        stroke,
                        color,
                    );
                    self.push_line(
                        &mut out,
                        point((0.34, 0.34)),
                        point((0.66, 0.34)),
                        stroke,
                        color,
                    );
                    self.push_line(
                        &mut out,
                        point((0.66, 0.34)),
                        point((0.66, 0.46)),
                        stroke,
                        color,
                    );
                    self.push_outline_rect(&mut out, point((0.3, 0.46)), 0.4, 0.32, stroke, color);
                }
                Icon::Star | Icon::StarFilled => {
                    let star = [
                        (0.5, 0.16),
                        (0.6, 0.39),
                        (0.84, 0.4),
                        (0.65, 0.55),
                        (0.72, 0.79),
                        (0.5, 0.65),
                        (0.28, 0.79),
                        (0.35, 0.55),
                        (0.16, 0.4),
                        (0.4, 0.39),
                    ];
                    let weight = if shape.icon == Icon::StarFilled {
                        stroke * 1.6
                    } else {
                        stroke
                    };
                    for index in 0..star.len() {
                        self.push_line(
                            &mut out,
                            point(star[index]),
                            point(star[(index + 1) % star.len()]),
                            weight,
                            color,
                        );
                    }
                    if shape.icon == Icon::StarFilled {
                        self.push_dot(&mut out, point((0.5, 0.51)), unit * 0.18, color);
                    }
                }
                Icon::Reload => {
                    let arc = [
                        (0.78, 0.36),
                        (0.7, 0.26),
                        (0.54, 0.21),
                        (0.36, 0.24),
                        (0.24, 0.36),
                        (0.2, 0.54),
                    ];
                    for index in 0..arc.len() - 1 {
                        self.push_line(
                            &mut out,
                            point(arc[index]),
                            point(arc[index + 1]),
                            stroke,
                            color,
                        );
                    }
                    self.push_line(
                        &mut out,
                        point((0.2, 0.54)),
                        point((0.29, 0.47)),
                        stroke,
                        color,
                    );
                    self.push_line(
                        &mut out,
                        point((0.2, 0.54)),
                        point((0.14, 0.43)),
                        stroke,
                        color,
                    );
                }
                Icon::Plus => {
                    self.push_line(
                        &mut out,
                        point((0.2, 0.5)),
                        point((0.8, 0.5)),
                        stroke,
                        color,
                    );
                    self.push_line(
                        &mut out,
                        point((0.5, 0.2)),
                        point((0.5, 0.8)),
                        stroke,
                        color,
                    );
                }
                Icon::Close => {
                    self.push_line(
                        &mut out,
                        point((0.25, 0.25)),
                        point((0.75, 0.75)),
                        stroke,
                        color,
                    );
                    self.push_line(
                        &mut out,
                        point((0.75, 0.25)),
                        point((0.25, 0.75)),
                        stroke,
                        color,
                    );
                }
                Icon::More => {
                    for x in [0.25, 0.5, 0.75] {
                        self.push_dot(&mut out, point((x, 0.5)), stroke * 2.6, color);
                    }
                }
                Icon::Bookmark => {
                    let path = [
                        (0.3, 0.2),
                        (0.7, 0.2),
                        (0.7, 0.8),
                        (0.5, 0.64),
                        (0.3, 0.8),
                        (0.3, 0.2),
                    ];
                    for index in 0..path.len() - 1 {
                        self.push_line(
                            &mut out,
                            point(path[index]),
                            point(path[index + 1]),
                            stroke,
                            color,
                        );
                    }
                }
                Icon::Edit => {
                    self.push_line(
                        &mut out,
                        point((0.24, 0.76)),
                        point((0.7, 0.3)),
                        stroke,
                        color,
                    );
                    self.push_line(
                        &mut out,
                        point((0.62, 0.22)),
                        point((0.78, 0.38)),
                        stroke,
                        color,
                    );
                    self.push_line(
                        &mut out,
                        point((0.24, 0.76)),
                        point((0.34, 0.78)),
                        stroke,
                        color,
                    );
                }
                Icon::Trash => {
                    self.push_line(
                        &mut out,
                        point((0.24, 0.3)),
                        point((0.76, 0.3)),
                        stroke,
                        color,
                    );
                    self.push_outline_rect(&mut out, point((0.3, 0.38)), 0.4, 0.42, stroke, color);
                    self.push_line(
                        &mut out,
                        point((0.43, 0.3)),
                        point((0.43, 0.22)),
                        stroke,
                        color,
                    );
                    self.push_line(
                        &mut out,
                        point((0.57, 0.3)),
                        point((0.57, 0.22)),
                        stroke,
                        color,
                    );
                    self.push_line(
                        &mut out,
                        point((0.42, 0.48)),
                        point((0.44, 0.7)),
                        stroke,
                        color,
                    );
                    self.push_line(
                        &mut out,
                        point((0.58, 0.48)),
                        point((0.56, 0.7)),
                        stroke,
                        color,
                    );
                }
                Icon::Minimize => {
                    self.push_line(
                        &mut out,
                        point((0.24, 0.5)),
                        point((0.76, 0.5)),
                        stroke,
                        color,
                    );
                }
                Icon::Maximize => {
                    self.push_outline_rect(&mut out, point((0.5, 0.5)), 0.52, 0.52, stroke, color);
                }
                Icon::Restore => {
                    self.push_outline_rect(
                        &mut out,
                        point((0.42, 0.42)),
                        0.48,
                        0.48,
                        stroke,
                        color,
                    );
                    self.push_outline_rect(
                        &mut out,
                        point((0.58, 0.58)),
                        0.48,
                        0.48,
                        stroke,
                        color,
                    );
                }
                Icon::ChevronLeft => {
                    self.push_line(
                        &mut out,
                        point((0.62, 0.24)),
                        point((0.36, 0.5)),
                        stroke,
                        color,
                    );
                    self.push_line(
                        &mut out,
                        point((0.36, 0.5)),
                        point((0.62, 0.76)),
                        stroke,
                        color,
                    );
                }
                Icon::ChevronRight => {
                    self.push_line(
                        &mut out,
                        point((0.38, 0.24)),
                        point((0.64, 0.5)),
                        stroke,
                        color,
                    );
                    self.push_line(
                        &mut out,
                        point((0.64, 0.5)),
                        point((0.38, 0.76)),
                        stroke,
                        color,
                    );
                }
            }
        }
        out
    }

    fn push_line(
        &self,
        out: &mut Vec<RectInstance>,
        start: (f32, f32),
        end: (f32, f32),
        width: f32,
        color: [f32; 4],
    ) {
        let dx = end.0 - start.0;
        let dy = end.1 - start.1;
        let length = dx.hypot(dy).max(0.001);
        let angle = dy.atan2(dx);
        out.push(RectInstance {
            rect: [
                (start.0 + end.0) * 0.5 * self.scale,
                (start.1 + end.1) * 0.5 * self.scale,
                length * self.scale,
                width * self.scale,
            ],
            color,
            params: [width * 0.5 * self.scale, 0.0, angle, 0.0],
            border_color: [0.0; 4],
        });
    }

    fn push_circle(
        &self,
        out: &mut Vec<RectInstance>,
        center: (f32, f32),
        diameter: f32,
        stroke: f32,
        color: [f32; 4],
    ) {
        let diameter = diameter * self.scale;
        out.push(RectInstance {
            rect: [
                center.0 * self.scale - diameter * 0.5,
                center.1 * self.scale - diameter * 0.5,
                diameter,
                diameter,
            ],
            color: [0.0; 4],
            params: [diameter * 0.5, stroke * self.scale, 0.0, 0.0],
            border_color: color,
        });
    }

    fn push_dot(
        &self,
        out: &mut Vec<RectInstance>,
        center: (f32, f32),
        diameter: f32,
        color: [f32; 4],
    ) {
        let diameter = diameter * self.scale;
        out.push(RectInstance {
            rect: [
                center.0 * self.scale - diameter * 0.5,
                center.1 * self.scale - diameter * 0.5,
                diameter,
                diameter,
            ],
            color,
            params: [diameter * 0.5, 0.0, 0.0, 0.0],
            border_color: [0.0; 4],
        });
    }

    fn push_outline_rect(
        &self,
        out: &mut Vec<RectInstance>,
        center: (f32, f32),
        width: f32,
        height: f32,
        stroke: f32,
        color: [f32; 4],
    ) {
        let diameter = width.min(height) * self.scale;
        out.push(RectInstance {
            rect: [
                center.0 * self.scale - width * 0.5 * self.scale,
                center.1 * self.scale - height * 0.5 * self.scale,
                width * self.scale,
                height * self.scale,
            ],
            color: [0.0; 4],
            params: [diameter * 0.5, stroke * self.scale, 0.0, 0.0],
            border_color: color,
        });
    }

    fn instance_color(&self, color: Color) -> [f32; 4] {
        if self.linearize {
            color.to_linear()
        } else {
            [
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0,
            ]
        }
    }
}

fn text_align(value: Align) -> cosmic_text::Align {
    match value {
        Align::Left => cosmic_text::Align::Left,
        Align::Center => cosmic_text::Align::Center,
        Align::Right => cosmic_text::Align::Right,
    }
}

fn floor_char_boundary(text: &str, mut index: usize) -> usize {
    if index >= text.len() {
        return text.len();
    }
    while index > 0 && !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

fn pick_family(font_system: &FontSystem) -> &'static str {
    let db = font_system.db();
    let has = |name: &str| {
        db.faces().any(|face| {
            face.families
                .iter()
                .any(|(family, _)| family.eq_ignore_ascii_case(name))
        })
    };

    if has("Segoe UI") {
        "Segoe UI"
    } else {
        "sans-serif"
    }
}
