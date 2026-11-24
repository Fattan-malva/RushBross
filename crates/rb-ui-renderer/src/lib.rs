mod icons;

use std::sync::Arc;

use bytemuck::{Pod, Zeroable};
use cosmic_text::{
    Attrs, Buffer, Ellipsize, EllipsizeHeightLimit, Family, FontSystem, Metrics, Shaping,
    SwashCache, Weight, Wrap,
};
use glyphon::{
    ColorMode, ContentType, CustomGlyph, RasterizedCustomGlyph, Resolution, TextArea, TextAtlas,
    TextBounds, TextRenderer, Viewport,
};
use icons::IconRegistry;
use rb_ui::frame::{Align, FontFamily, Frame, IconShape, RectShape, TextShape};
use rb_ui::Color;
use wgpu::util::DeviceExt;
use winit::window::Window;

const SHADER: &str = include_str!("shader.wgsl");
const INITIAL_INSTANCES: u64 = 512;

/// Font UI dibundel ke dalam binary agar tampilan sama di semua mesin Windows.
const UI_FAMILY: &str = "Manrope";
const UI_FONTS: [&[u8]; 4] = [
    include_bytes!("../../../assets/fonts/Manrope-400.ttf"),
    include_bytes!("../../../assets/fonts/Manrope-500.ttf"),
    include_bytes!("../../../assets/fonts/Manrope-600.ttf"),
    include_bytes!("../../../assets/fonts/Manrope-700.ttf"),
];
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
    icons: IconRegistry,
    /// Buffer kosong sebagai pembawa glyph ikon di jalur teks glyphon.
    empty_text: Buffer,
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

        let mut font_system = FontSystem::new();
        for font in UI_FONTS {
            font_system.db_mut().load_font_data(font.to_vec());
        }
        let ui_family = UI_FAMILY;
        let empty_text = Buffer::new(&mut font_system, Metrics::new(1.0, 1.0));

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
            icons: IconRegistry::default(),
            empty_text,
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
        let mut overlay_instances = self.shape_instances(&frame.overlay.shapes);
        let base_glyphs = self.icon_glyphs(&frame.base.icons);
        let overlay_glyphs = self.icon_glyphs(&frame.overlay.icons);

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

        let bounds = TextBounds {
            left: 0,
            top: 0,
            right: self.size.0 as i32,
            bottom: self.size.1 as i32,
        };
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
            .chain(std::iter::once(icon_area(
                &self.empty_text,
                &base_glyphs,
                self.scale,
                bounds,
            )))
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
            .chain(std::iter::once(icon_area(
                &self.empty_text,
                &overlay_glyphs,
                self.scale,
                bounds,
            )))
            .collect::<Vec<_>>();

        let icons = &self.icons;
        self.base_text
            .prepare_with_custom(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                base_areas,
                &mut self.swash_cache,
                |request| {
                    let data = icons.rasterize(request.id, request.width, request.height)?;
                    Some(RasterizedCustomGlyph {
                        data,
                        content_type: ContentType::Mask,
                    })
                },
            )
            .map_err(|error| {
                eprintln!("prepare base text: {error}");
                RenderError::Validation
            })?;

        self.overlay_text
            .prepare_with_custom(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                overlay_areas,
                &mut self.swash_cache,
                |request| {
                    let data = icons.rasterize(request.id, request.width, request.height)?;
                    Some(RasterizedCustomGlyph {
                        data,
                        content_type: ContentType::Mask,
                    })
                },
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
            if text.ellipsis && text.offset_x <= 0.0 {
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

        let left = (text.x - text.offset_x) * scale;
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

        let selection_color = self.instance_color(text.selection_color);
        let mut prefix_width = |index: usize| {
            let index = floor_char_boundary(&text.text, index.min(text.text.len()));
            let mut prefix_buffer = Buffer::new(&mut self.font_system, metrics);
            prefix_buffer.set_text(&text.text[..index], &attrs, Shaping::Advanced, None);
            prefix_buffer.set_size(None, None);
            prefix_buffer.set_wrap(Wrap::None);
            prefix_buffer.shape_until_scroll(&mut self.font_system, false);
            prefix_buffer
                .layout_runs()
                .map(|run| run.line_w)
                .fold(0.0f32, f32::max)
        };

        if let Some((start, end)) = text.selection {
            let start = floor_char_boundary(&text.text, start.min(text.text.len()));
            let end = floor_char_boundary(&text.text, end.min(text.text.len()));
            let left_edge = left + prefix_width(start);
            let right_edge = left + prefix_width(end);
            if right_edge > left_edge {
                overlay_instances.push(RectInstance {
                    rect: [
                        left_edge,
                        top + font_size * 0.08,
                        right_edge - left_edge,
                        font_size * 1.15,
                    ],
                    color: selection_color,
                    params: [2.0 * scale, 0.0, 0.0, 0.0],
                    border_color: [0.0; 4],
                });
            }
        }

        if let Some(caret) = text.caret {
            let caret = caret.min(text.text.len());
            let caret = floor_char_boundary(&text.text, caret);
            let prefix_width = prefix_width(caret);

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

    /// Mengubah ikon menjadi custom glyph glyphon. Posisi dan ukuran tetap dalam
    /// piksel logis; skala ke piksel fisik dilakukan lewat `TextArea::scale`.
    fn icon_glyphs(&mut self, icons: &[IconShape]) -> Vec<CustomGlyph> {
        let mut glyphs = Vec::with_capacity(icons.len());
        for shape in icons {
            let unit = shape.rect.width.min(shape.rect.height);
            if unit <= 0.0 {
                continue;
            }
            let stroke = icons::stroke_units(shape.stroke, unit);
            let Some(id) = self.icons.id(shape.icon, stroke) else {
                continue;
            };
            let color = shape.color;
            glyphs.push(CustomGlyph {
                id,
                left: shape.rect.x,
                top: shape.rect.y,
                width: shape.rect.width,
                height: shape.rect.height,
                color: Some(cosmic_text::Color::rgba(color.r, color.g, color.b, color.a)),
                snap_to_physical_pixel: true,
                metadata: 0,
            });
        }
        glyphs
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

fn icon_area<'a>(
    buffer: &'a Buffer,
    glyphs: &'a [CustomGlyph],
    scale: f32,
    bounds: TextBounds,
) -> TextArea<'a> {
    TextArea {
        buffer,
        left: 0.0,
        top: 0.0,
        scale,
        bounds,
        default_color: cosmic_text::Color::rgba(255, 255, 255, 255),
        custom_glyphs: glyphs,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_manrope_weights_are_registered_and_shape_text() {
        let mut font_system = FontSystem::new();
        for font in UI_FONTS {
            font_system.db_mut().load_font_data(font.to_vec());
        }
        let manrope_faces = font_system
            .db()
            .faces()
            .filter(|face| {
                face.families
                    .iter()
                    .any(|(family, _)| family.eq_ignore_ascii_case(UI_FAMILY))
            })
            .count();
        assert!(manrope_faces >= UI_FONTS.len());

        let mut buffer = Buffer::new(&mut font_system, Metrics::new(14.0, 18.0));
        let attrs = Attrs::new()
            .family(Family::Name(UI_FAMILY))
            .weight(Weight(600));
        buffer.set_text("RushBross", &attrs, Shaping::Advanced, None);
        buffer.shape_until_scroll(&mut font_system, false);
        let width = buffer
            .layout_runs()
            .map(|run| run.line_w)
            .fold(0.0f32, f32::max);
        assert!(width > 40.0, "shaped width was {width}");
    }
}
