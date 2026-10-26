//! S5 prototype (iced 0.14): library grid + wgpu preview widget + keycap button, docs/development/design-surfaces.md dark tokens.
//! The preview is a `shader` widget: a CPU-generated plasma is uploaded each frame into a
//! wgpu texture owned by the widget pipeline, then drawn with a fullscreen triangle.
//! `SPIKE_STATIC=1` stops the preview animation (idle-memory measurement).
use std::sync::OnceLock;
use std::time::Instant;

use iced::wgpu;
use iced::widget::shader::{self, Viewport};
use iced::widget::{button, column, container, row, scrollable, text, Shader, Space};
use iced::{border, keyboard, mouse, time, Background, Border, Color, Element, Length, Rectangle, Subscription, Theme};

const W: u32 = 640;
const H: u32 = 360;

static START: OnceLock<Instant> = OnceLock::new();

fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

#[derive(Debug, Clone)]
enum Msg {
    Tick,
    Export,
    Key,
}

struct Studio {
    t: f32,
    animate: bool,
    exported: u32,
}

impl Studio {
    fn new() -> Self {
        Self { t: 0.0, animate: std::env::var_os("SPIKE_STATIC").is_none(), exported: 0 }
    }

    fn update(&mut self, m: Msg) {
        match m {
            Msg::Tick => self.t = START.get().unwrap().elapsed().as_secs_f32(),
            Msg::Export | Msg::Key => self.exported += 1,
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        let keys = keyboard::listen().filter_map(|e| match e {
            keyboard::Event::KeyPressed { key, modifiers, .. }
                if modifiers.command() && key.as_ref() == keyboard::Key::Character("e") =>
            {
                Some(Msg::Key)
            }
            _ => None,
        });
        if self.animate {
            Subscription::batch([time::every(std::time::Duration::from_millis(16)).map(|_| Msg::Tick), keys])
        } else {
            keys
        }
    }

    fn view(&self) -> Element<'_, Msg> {
        let keycap = container(text("Cmd+E").size(11).color(rgb(0xb3b8c2)))
            .padding([1, 5])
            .style(|_| container::Style {
                background: Some(Background::Color(rgb(0x22262e))),
                border: Border { color: rgb(0x343943), width: 1.0, radius: border::radius(4) },
                ..Default::default()
            });
        let export = button(row![text("Export").size(13), keycap].spacing(8).align_y(iced::Center))
            .padding([5, 12])
            .on_press(Msg::Export)
            .style(|_, status| {
                let bg = if matches!(status, button::Status::Hovered) { 0x6f5ff0 } else { 0x6352e8 };
                button::Style {
                    background: Some(Background::Color(rgb(bg))),
                    text_color: Color::WHITE,
                    border: border::rounded(6),
                    ..Default::default()
                }
            });

        // Wrap manually: 2 rows of 6.
        let grid = column![
            row((0..6).map(|i| card(i))).spacing(12),
            row((6..12).map(|i| card(i))).spacing(12),
        ]
        .spacing(12);

        let sidebar = container(
            column![
                text("LIBRARY").size(11).color(rgb(0x8b919c)),
                container(text("All captures").color(rgb(0x9d8fff))).padding([4, 8]).width(Length::Fill).style(|_| {
                    container::Style {
                        background: Some(Background::Color(Color::from_rgba8(139, 124, 255, 0.16))),
                        border: border::rounded(6),
                        ..Default::default()
                    }
                }),
                text("Recordings").color(rgb(0xb3b8c2)),
            ]
            .spacing(8),
        )
        .padding(12)
        .width(180)
        .height(Length::Fill)
        .style(|_| container::Style { background: Some(Background::Color(rgb(0x121419))), ..Default::default() });

        let preview = container(Shader::new(Plasma { t: self.t }).width(Length::Fill).height(Length::Fill))
            .height(260)
            .width(Length::Fill)
            .style(|_| container::Style {
                background: Some(Background::Color(Color::BLACK)),
                border: border::rounded(8),
                ..Default::default()
            });

        let main = column![
            preview,
            row![export, text(format!("exported {}", self.exported)).size(12).color(rgb(0x8b919c))]
                .spacing(10)
                .align_y(iced::Center),
            scrollable(grid),
        ]
        .spacing(14)
        .padding(16)
        .width(Length::Fill);

        container(row![sidebar, main])
            .style(|_| container::Style {
                background: Some(Background::Color(rgb(0x0b0c10))),
                text_color: Some(rgb(0xf4f5f7)),
                ..Default::default()
            })
            .into()
    }
}

fn card<'a>(i: u32) -> Element<'a, Msg> {
    column![
        container(Space::new()).width(150).height(84).style(|_| container::Style {
            background: Some(Background::Color(rgb(0x1a1d23))),
            border: border::rounded(8),
            ..Default::default()
        }),
        text(format!("Capture {:02}", i + 1)).size(13).color(rgb(0xf4f5f7)),
    ]
    .spacing(6)
    .into()
}

// ---- wgpu preview widget ----

#[derive(Debug)]
struct Plasma {
    t: f32,
}

#[derive(Debug)]
struct PlasmaPrimitive {
    pixels: Vec<u8>,
}

struct PlasmaPipeline {
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    pipeline: wgpu::RenderPipeline,
}

fn plasma(t: f32) -> Vec<u8> {
    let mut buf = vec![0u8; (W * H * 4) as usize];
    for y in 0..H {
        for x in 0..W {
            let (fx, fy) = (x as f32 / 40.0, y as f32 / 40.0);
            let v = (fx + t).sin() + (fy * 1.3 - t).sin() + ((fx + fy + t) * 0.7).sin();
            let c = ((v / 3.0 + 1.0) * 0.5 * 255.0) as u8;
            let i = ((y * W + x) * 4) as usize;
            buf[i..i + 4].copy_from_slice(&[c, c / 2 + 60, 255 - c, 255]);
        }
    }
    buf
}

impl<Message> shader::Program<Message> for Plasma {
    type State = ();
    type Primitive = PlasmaPrimitive;
    fn draw(&self, _: &(), _: mouse::Cursor, _: Rectangle) -> PlasmaPrimitive {
        PlasmaPrimitive { pixels: plasma(self.t) }
    }
}

const SHADER: &str = r#"
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
struct VOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> VOut {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    var o: VOut;
    o.pos = vec4(p[i], 0.0, 1.0);
    o.uv = vec2((p[i].x + 1.0) * 0.5, (1.0 - p[i].y) * 0.5);
    return o;
}
@fragment fn fs(v: VOut) -> @location(0) vec4<f32> { return textureSample(tex, samp, v.uv); }
"#;

impl shader::Pipeline for PlasmaPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        println!("first_gpu_pipeline_ms={}", START.get().unwrap().elapsed().as_millis());
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("preview"),
            size: wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), buffers: &[], compilation_options: Default::default() },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                targets: &[Some(format.into())],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        Self { texture, bind_group, pipeline }
    }
}

impl shader::Primitive for PlasmaPrimitive {
    type Pipeline = PlasmaPipeline;

    fn prepare(&self, p: &mut PlasmaPipeline, _d: &wgpu::Device, queue: &wgpu::Queue, _b: &Rectangle, _v: &Viewport) {
        queue.write_texture(
            p.texture.as_image_copy(),
            &self.pixels,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(W * 4), rows_per_image: Some(H) },
            wgpu::Extent3d { width: W, height: H, depth_or_array_layers: 1 },
        );
    }

    fn draw(&self, p: &PlasmaPipeline, pass: &mut wgpu::RenderPass<'_>) -> bool {
        pass.set_pipeline(&p.pipeline);
        pass.set_bind_group(0, &p.bind_group, &[]);
        pass.draw(0..3, 0..1);
        true
    }
}

fn main() -> iced::Result {
    START.set(Instant::now()).ok();
    println!("main_entered");
    iced::application(Studio::new, Studio::update, Studio::view)
        .subscription(Studio::subscription)
        .theme(|_: &Studio| Theme::Dark)
        .window_size((1000.0, 680.0))
        .run()
}
