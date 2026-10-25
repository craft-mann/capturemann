//! The `wgpu` side of the compositor: device, pipelines, texture reuse, one draw, readback.

use std::borrow::Cow;

use bytemuck::{Pod, Zeroable};
use cm_core::CmError;
use image::RgbaImage;

use super::caption;
use super::plan::{plan, Plan, Tile};
use super::{Background, ClickStyle, Frame, Layout, PixelFormat, Scene, MAX_CLICKS};
use crate::text::Fonts;

const OUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TileU {
    rect: [f32; 4],
    src: [f32; 4],
    params: [f32; 4],
    ring_color: [f32; 4],
    flags: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ParamsU {
    size: [f32; 4],
    bg_a: [f32; 4],
    bg_b: [f32; 4],
    bg_dir: [f32; 4],
    tiles: [TileU; 2],
    cur_a: [f32; 4],
    cur_b: [f32; 4],
    fx_meta: [f32; 4],
    fx_color: [f32; 4],
    fx: [[f32; 4]; 3],
}

/// A reusable RGBA-ish texture sized to the last frame uploaded to it.
struct Slot {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
}

/// Luma and chroma planes of an NV12 upload, kept between frames.
struct Planes {
    y: wgpu::Texture,
    y_view: wgpu::TextureView,
    uv: wgpu::Texture,
    uv_view: wgpu::TextureView,
    width: u32,
    height: u32,
}

#[derive(Default)]
struct SlotState {
    slot: Option<Slot>,
    planes: Option<Planes>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Which {
    Screen,
    Camera,
    Overlay,
    BgImage,
    Cursor,
}

/// The headless GPU compositor. Creating one opens the default GPU; keep it for many frames.
pub struct Compositor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_name: String,
    main_pipeline: wgpu::RenderPipeline,
    main_layout: wgpu::BindGroupLayout,
    nv12_pipeline: wgpu::RenderPipeline,
    nv12_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    dummy: wgpu::TextureView,
    slots: [SlotState; 5],
    out: Option<(wgpu::Texture, wgpu::TextureView, u32, u32)>,
    readback: Option<(wgpu::Buffer, u64)>,
    /// Bundled fonts for the caption layer, loaded on the first caption.
    fonts: Option<Fonts>,
}

fn gpu_err(what: &str, e: impl std::fmt::Display) -> CmError {
    CmError::platform_rejected(format!("GPU compositor: {what}: {e}"), None)
}

impl Compositor {
    /// Opens the default GPU (Metal on macOS). Fails with `platform_rejected` when there is no
    /// adapter, so callers can fall back to a software path or skip.
    pub fn new() -> Result<Self, CmError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .map_err(|e| gpu_err("no GPU adapter", e))?;
        let adapter_name = adapter.get_info().name;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("cm-render"),
            required_limits: adapter.limits(),
            ..Default::default()
        }))
        .map_err(|e| gpu_err("cannot open the device", e))?;

        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let main_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("compose"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                sampler_entry(1),
                tex_entry(2),
                tex_entry(3),
                tex_entry(4),
                tex_entry(5),
                tex_entry(6),
            ],
        });
        let nv12_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("nv12"),
            entries: &[sampler_entry(0), tex_entry(1), tex_entry(2)],
        });
        let main_pipeline = pipeline(
            &device,
            "compose",
            include_str!("shader.wgsl"),
            &main_layout,
        );
        let nv12_pipeline = pipeline(&device, "nv12", include_str!("nv12.wgsl"), &nv12_layout);
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("params"),
            size: std::mem::size_of::<ParamsU>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let dummy_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dummy"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            dummy_tex.as_image_copy(),
            &[0, 0, 0, 0],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let dummy = dummy_tex.create_view(&wgpu::TextureViewDescriptor::default());
        Ok(Compositor {
            device,
            queue,
            adapter_name,
            main_pipeline,
            main_layout,
            nv12_pipeline,
            nv12_layout,
            sampler,
            uniforms,
            dummy,
            slots: Default::default(),
            out: None,
            readback: None,
            fonts: None,
        })
    }

    /// The GPU in use, for logs and reports.
    pub fn adapter_name(&self) -> &str {
        &self.adapter_name
    }

    /// Draws the scene and reads the frame back as RGBA (alpha is always 255).
    pub fn render(&mut self, scene: &Scene<'_>) -> Result<RgbaImage, CmError> {
        let p = plan(scene)?;
        self.render_with_plan(scene, p)
    }

    /// [`Compositor::render`] with the tiles chosen by the caller, for a layout change under way
    /// (`plan_blend`, CAM-3.AC3). `scene` still supplies the frames, the background and the
    /// overlay; a tile in `p` needs its layer in the scene.
    pub fn render_with_plan(&mut self, scene: &Scene<'_>, p: Plan) -> Result<RgbaImage, CmError> {
        let (w, h) = scene.output;
        if let Background::Image(f) = &scene.background {
            f.validate("background")?;
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("compose"),
            });

        if let Some(s) = &scene.screen {
            if p.screen.is_some() {
                self.upload(Which::Screen, &s.frame, &mut encoder);
            }
        }
        if let Some(c) = &scene.camera {
            if p.camera.is_some() {
                self.upload(Which::Camera, &c.frame, &mut encoder);
            }
        }
        let has_overlay = match (&scene.annotations, p.screen.is_some()) {
            (Some(a), true) => {
                self.upload(Which::Overlay, a, &mut encoder);
                true
            }
            _ => false,
        };
        // The cursor sprite is uploaded premultiplied, so bilinear sampling has no dark fringe.
        let premul: Vec<u8>;
        let has_cursor_sprite = match (
            scene.cursor.as_ref().and_then(|c| c.sprite.as_ref()),
            p.screen.is_some(),
        ) {
            (Some(sp), true) => {
                premul = premultiply(sp.frame.data);
                let f = Frame::rgba(sp.frame.width, sp.frame.height, &premul);
                self.upload(Which::Cursor, &f, &mut encoder);
                true
            }
            _ => false,
        };
        let bg_aspect = match &scene.background {
            Background::Image(f) => {
                self.upload(Which::BgImage, f, &mut encoder);
                f.width as f32 / f.height as f32
            }
            _ => 1.0,
        };

        let mut params = params_for(scene, &p.screen, &p.camera, has_overlay, bg_aspect);
        if p.screen.is_some() {
            set_cursor(&mut params, scene);
        }
        self.queue
            .write_buffer(&self.uniforms, 0, bytemuck::bytes_of(&params));

        self.ensure_out(w, h);
        let (_, out_view, _, _) = self
            .out
            .as_ref()
            .ok_or_else(|| CmError::internal("no target"))?;
        let view_of = |s: &SlotState, dummy: &wgpu::TextureView| -> wgpu::TextureView {
            // Views are cheap handles; cloning keeps the bind group independent of `self`.
            s.slot
                .as_ref()
                .map_or_else(|| dummy.clone(), |x| x.view.clone())
        };
        let screen_v = view_of(&self.slots[Which::Screen as usize], &self.dummy);
        let camera_v = view_of(&self.slots[Which::Camera as usize], &self.dummy);
        let overlay_v = if has_overlay {
            view_of(&self.slots[Which::Overlay as usize], &self.dummy)
        } else {
            self.dummy.clone()
        };
        let cursor_v = if has_cursor_sprite {
            view_of(&self.slots[Which::Cursor as usize], &self.dummy)
        } else {
            self.dummy.clone()
        };
        let bg_v = if matches!(scene.background, Background::Image(_)) {
            view_of(&self.slots[Which::BgImage as usize], &self.dummy)
        } else {
            self.dummy.clone()
        };
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compose"),
            layout: &self.main_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&screen_v),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&camera_v),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&overlay_v),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&bg_v),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&cursor_v),
                },
            ],
        });
        full_screen_pass(
            &mut encoder,
            "compose",
            out_view,
            &self.main_pipeline,
            &bind,
        );

        // Readback.
        let row = u64::from(w) * 4;
        let padded = row.div_ceil(256) * 256;
        let size = padded * u64::from(h);
        if self.readback.as_ref().map(|r| r.1) != Some(size) {
            self.readback = Some((
                self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("readback"),
                    size,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                }),
                size,
            ));
        }
        let (out_tex, _, _, _) = self
            .out
            .as_ref()
            .ok_or_else(|| CmError::internal("no target"))?;
        let (buffer, _) = self
            .readback
            .as_ref()
            .ok_or_else(|| CmError::internal("no readback buffer"))?;
        encoder.copy_texture_to_buffer(
            out_tex.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded as u32),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = buffer.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| gpu_err("poll failed", e))?;
        rx.recv()
            .map_err(|e| gpu_err("readback channel closed", e))?
            .map_err(|e| gpu_err("readback failed", e))?;
        let mapped = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((row * u64::from(h)) as usize);
        for y in 0..h as usize {
            let start = y * padded as usize;
            pixels.extend_from_slice(&mapped[start..start + row as usize]);
        }
        drop(mapped);
        buffer.unmap();
        let mut img = RgbaImage::from_raw(w, h, pixels)
            .ok_or_else(|| CmError::internal("readback size mismatch"))?;
        if let Some(layer) = &scene.captions {
            // The caption keeps off a bubble it would cover (module docs of `caption`).
            let bubble = match (&scene.layout, &p.camera) {
                (Layout::ScreenBubble(_), Some(t)) => Some(t.rect),
                _ => None,
            };
            // The screen picture's tile: with padding the caption stays inside it.
            let content = p.screen.as_ref().map(|t| t.rect);
            let fonts = self.fonts.get_or_insert_with(Fonts::bundled);
            caption::blend(&mut img, layer, bubble, content, fonts)?;
        }
        Ok(img)
    }

    fn ensure_out(&mut self, w: u32, h: u32) {
        if matches!(&self.out, Some((_, _, ow, oh)) if *ow == w && *oh == h) {
            return;
        }
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output"),
            size: extent(w, h),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: OUT_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.out = Some((tex, view, w, h));
    }

    /// Uploads a frame into a slot; NV12 is converted to RGBA by a render pass.
    fn upload(&mut self, which: Which, f: &Frame<'_>, encoder: &mut wgpu::CommandEncoder) {
        let state = &mut self.slots[which as usize];
        let (w, h) = (f.width, f.height);
        let format = match f.format {
            PixelFormat::Rgba8 | PixelFormat::Nv12 => wgpu::TextureFormat::Rgba8Unorm,
            PixelFormat::Bgra8 => wgpu::TextureFormat::Bgra8Unorm,
        };
        let stale =
            !matches!(&state.slot, Some(s) if s.width == w && s.height == h && s.format == format);
        if stale {
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("frame"),
                size: extent(w, h),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_DST
                    | wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            state.slot = Some(Slot {
                texture,
                view,
                width: w,
                height: h,
                format,
            });
        }
        let Some(slot) = state.slot.as_ref() else {
            return;
        };
        match f.format {
            PixelFormat::Rgba8 | PixelFormat::Bgra8 => {
                self.queue.write_texture(
                    slot.texture.as_image_copy(),
                    f.data,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(w * 4),
                        rows_per_image: Some(h),
                    },
                    extent(w, h),
                );
            }
            PixelFormat::Nv12 => {
                let stale_planes =
                    !matches!(&state.planes, Some(p) if p.width == w && p.height == h);
                if stale_planes {
                    let make = |label, fmt, pw, ph| {
                        let t = self.device.create_texture(&wgpu::TextureDescriptor {
                            label: Some(label),
                            size: extent(pw, ph),
                            mip_level_count: 1,
                            sample_count: 1,
                            dimension: wgpu::TextureDimension::D2,
                            format: fmt,
                            usage: wgpu::TextureUsages::TEXTURE_BINDING
                                | wgpu::TextureUsages::COPY_DST,
                            view_formats: &[],
                        });
                        let v = t.create_view(&wgpu::TextureViewDescriptor::default());
                        (t, v)
                    };
                    let (y, y_view) = make("nv12-y", wgpu::TextureFormat::R8Unorm, w, h);
                    let (uv, uv_view) =
                        make("nv12-uv", wgpu::TextureFormat::Rg8Unorm, w / 2, h / 2);
                    state.planes = Some(Planes {
                        y,
                        y_view,
                        uv,
                        uv_view,
                        width: w,
                        height: h,
                    });
                }
                let Some(planes) = state.planes.as_ref() else {
                    return;
                };
                let luma = (w * h) as usize;
                self.queue.write_texture(
                    planes.y.as_image_copy(),
                    &f.data[..luma],
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(w),
                        rows_per_image: Some(h),
                    },
                    extent(w, h),
                );
                self.queue.write_texture(
                    planes.uv.as_image_copy(),
                    &f.data[luma..],
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(w),
                        rows_per_image: Some(h / 2),
                    },
                    extent(w / 2, h / 2),
                );
                let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("nv12"),
                    layout: &self.nv12_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&planes.y_view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&planes.uv_view),
                        },
                    ],
                });
                full_screen_pass(encoder, "nv12", &slot.view, &self.nv12_pipeline, &bind);
            }
        }
    }
}

fn extent(w: u32, h: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: w,
        height: h,
        depth_or_array_layers: 1,
    }
}

fn pipeline(
    device: &wgpu::Device,
    label: &str,
    source: &str,
    layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(Cow::Owned(source.to_owned())),
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&pl),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                // Also the target of the NV12 pass: slots filled from NV12 are Rgba8Unorm.
                format: OUT_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview_mask: None,
        cache: None,
    })
}

fn full_screen_pass(
    encoder: &mut wgpu::CommandEncoder,
    label: &str,
    target: &wgpu::TextureView,
    pipeline: &wgpu::RenderPipeline,
    bind: &wgpu::BindGroup,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind, &[]);
    pass.draw(0..3, 0..1);
}

fn rgb(c: [u8; 3]) -> [f32; 4] {
    [
        f32::from(c[0]) / 255.0,
        f32::from(c[1]) / 255.0,
        f32::from(c[2]) / 255.0,
        1.0,
    ]
}

fn tile_u(t: &Option<Tile>, has_overlay: bool) -> TileU {
    let Some(t) = t else {
        return TileU::zeroed();
    };
    let (ring_w, ring_c) = t.ring.map_or((0.0, [0.0; 4]), |r| {
        (
            r.width_px.max(0.0),
            [
                f32::from(r.color[0]) / 255.0,
                f32::from(r.color[1]) / 255.0,
                f32::from(r.color[2]) / 255.0,
                f32::from(r.color[3]) / 255.0,
            ],
        )
    });
    TileU {
        rect: t.rect,
        src: t.src,
        params: [t.radius, ring_w, t.shadow, 1.0],
        ring_color: ring_c,
        flags: [
            if t.mirror { 1.0 } else { 0.0 },
            if has_overlay { 1.0 } else { 0.0 },
            0.0,
            0.0,
        ],
    }
}

fn params_for(
    scene: &Scene<'_>,
    screen: &Option<Tile>,
    camera: &Option<Tile>,
    has_overlay: bool,
    bg_aspect: f32,
) -> ParamsU {
    let (w, h) = scene.output;
    let (kind, a, b, dir) = match &scene.background {
        Background::None => (0.0, [0.0; 4], [0.0; 4], [0.0, -1.0]),
        Background::Color(c) => (1.0, rgb(*c), [0.0; 4], [0.0, -1.0]),
        Background::Gradient {
            from,
            to,
            angle_deg,
        } => {
            let a = if angle_deg.is_finite() {
                *angle_deg
            } else {
                180.0
            }
            .to_radians();
            (2.0, rgb(*from), rgb(*to), [a.sin(), -a.cos()])
        }
        Background::Image(_) => (3.0, [0.0; 4], [0.0; 4], [0.0, -1.0]),
    };
    let short = w.min(h) as f32;
    ParamsU {
        size: [w as f32, h as f32, kind, bg_aspect],
        bg_a: a,
        bg_b: b,
        bg_dir: [
            dir[0],
            dir[1],
            (0.012 * short).round(),
            (0.02 * short).round().max(1.0),
        ],
        tiles: [tile_u(screen, has_overlay), tile_u(camera, false)],
        cur_a: [0.0; 4],
        cur_b: [0.0; 4],
        fx_meta: [0.0; 4],
        fx_color: [0.0; 4],
        fx: [[0.0; 4]; 3],
    }
}

fn premultiply(rgba: &[u8]) -> Vec<u8> {
    let mut out = rgba.to_vec();
    for px in out.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        for c in &mut px[..3] {
            *c = ((u32::from(*c) * a + 127) / 255) as u8;
        }
    }
    out
}

/// Fills the cursor uniforms (EDT-4): positions are converted to screen-frame px here, so the
/// shader works in the picture's own pixels and the cursor follows the crop.
fn set_cursor(params: &mut ParamsU, scene: &Scene<'_>) {
    let (Some(c), Some(screen)) = (&scene.cursor, &scene.screen) else {
        return;
    };
    let (sw, sh) = (screen.frame.width as f32, screen.frame.height as f32);
    params.cur_a = [
        c.pos.0 * sw,
        c.pos.1 * sh,
        c.scale,
        if c.sprite.is_some() { c.opacity } else { 0.0 },
    ];
    if let Some(sp) = &c.sprite {
        params.cur_b = [
            sp.hotspot.0,
            sp.hotspot.1,
            sp.frame.width as f32,
            sp.frame.height as f32,
        ];
    }
    let kind = match c.click_style {
        ClickStyle::None => 0.0,
        ClickStyle::Ring => 1.0,
        ClickStyle::Ripple => 2.0,
    };
    let n = c.clicks.len().min(MAX_CLICKS);
    if kind > 0.0 && n > 0 {
        params.fx_meta = [kind, c.effect_radius_px, n as f32, 0.0];
        params.fx_color = [
            f32::from(c.effect_color[0]) / 255.0,
            f32::from(c.effect_color[1]) / 255.0,
            f32::from(c.effect_color[2]) / 255.0,
            f32::from(c.effect_color[3]) / 255.0,
        ];
        for (slot, m) in params.fx.iter_mut().zip(c.clicks.iter().take(n)) {
            *slot = [m.pos.0 * sw, m.pos.1 * sh, m.progress.clamp(0.0, 1.0), 0.0];
        }
    }
}
