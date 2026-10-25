// NV12 (biplanar 4:2:0, BT.709 limited range) to RGBA8, one full-screen pass per frame.

@group(0) @binding(0) var samp: sampler;
@group(0) @binding(1) var y_tex: texture_2d<f32>;
@group(0) @binding(2) var uv_tex: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(y_tex));
    let uv = pos.xy / dims;
    let y = (textureSampleLevel(y_tex, samp, uv, 0.0).r - 16.0 / 255.0) * (255.0 / 219.0);
    let c = textureSampleLevel(uv_tex, samp, uv, 0.0).rg - vec2<f32>(128.0 / 255.0, 128.0 / 255.0);
    let u = c.x * (255.0 / 224.0);
    let v = c.y * (255.0 / 224.0);
    let rgb = vec3<f32>(y + 1.5748 * v, y - 0.1873 * u - 0.4681 * v, y + 1.8556 * u);
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
