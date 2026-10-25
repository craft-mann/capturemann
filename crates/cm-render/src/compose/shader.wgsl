// Headless compositor (EDT-3, CAM-3): background, up to two tiles (screen, camera) with rounded
// corners, shadow and ring, drawn in one full-screen pass. Colors are blended in sRGB-encoded
// space, like the CPU annotation renderer and FFmpeg's default filters.

struct Tile {
    rect: vec4<f32>,      // x, y, w, h in output pixels
    src: vec4<f32>,       // u0, v0, u1, v1 of the source region shown
    params: vec4<f32>,    // radius px, ring width px, shadow opacity, enabled
    ring_color: vec4<f32>,
    flags: vec4<f32>,     // mirror, has_overlay, 0, 0
};

struct Params {
    size: vec4<f32>,      // output w, h, background kind, background image aspect (w / h)
    bg_a: vec4<f32>,
    bg_b: vec4<f32>,
    bg_dir: vec4<f32>,    // gradient direction x, y (unit), shadow offset y px, shadow blur px
    tiles: array<Tile, 2>,
    cur_a: vec4<f32>,     // cursor hotspot x, y in screen-frame px, px per sprite px, opacity
    cur_b: vec4<f32>,     // sprite hotspot x, y, sprite w, h in px (w = 0: no sprite)
    fx_meta: vec4<f32>,   // click effect kind (0 none, 1 ring, 2 ripple), radius px, count, 0
    fx_color: vec4<f32>,
    fx: array<vec4<f32>, 3>, // click x, y in screen-frame px, progress, 0
};

@group(0) @binding(0) var<uniform> u: Params;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var screen_tex: texture_2d<f32>;
@group(0) @binding(3) var camera_tex: texture_2d<f32>;
@group(0) @binding(4) var overlay_tex: texture_2d<f32>;
@group(0) @binding(5) var bg_tex: texture_2d<f32>;
@group(0) @binding(6) var cursor_tex: texture_2d<f32>;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // One oversized triangle covers the target.
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, 1.0 - y * 2.0, 0.0, 1.0);
}

fn sd_rbox(p: vec2<f32>, c: vec2<f32>, half: vec2<f32>, r: f32) -> f32 {
    let q = abs(p - c) - half + vec2<f32>(r, r);
    return length(max(q, vec2<f32>(0.0, 0.0))) + min(max(q.x, q.y), 0.0) - r;
}

fn background(p: vec2<f32>) -> vec3<f32> {
    let kind = u.size.z;
    let wh = u.size.xy;
    if (kind < 0.5) {
        return vec3<f32>(0.0, 0.0, 0.0);
    }
    if (kind < 1.5) {
        return u.bg_a.rgb;
    }
    if (kind < 2.5) {
        let d = u.bg_dir.xy;
        let extent = abs(wh.x * d.x) + abs(wh.y * d.y);
        let t = clamp(0.5 + dot(p - wh * 0.5, d) / max(extent, 1.0), 0.0, 1.0);
        return mix(u.bg_a.rgb, u.bg_b.rgb, t);
    }
    // Image, "cover": scale to fill, center, no distortion.
    let aspect = u.size.w;
    let frame_aspect = wh.x / wh.y;
    var uv = p / wh - vec2<f32>(0.5, 0.5);
    if (frame_aspect > aspect) {
        uv.y = uv.y * aspect / frame_aspect;
    } else {
        uv.x = uv.x * frame_aspect / aspect;
    }
    return textureSampleLevel(bg_tex, samp, uv + vec2<f32>(0.5, 0.5), 0.0).rgb;
}

fn ring_alpha(d: f32, r: f32, w: f32) -> f32 {
    return 1.0 - smoothstep(w * 0.5 - 0.75, w * 0.5 + 0.75, abs(d - r));
}

// The cursor and its click effects (EDT-4), in the screen picture's own pixels `sp`, so they
// follow the crop and the zoom like the content under them.
fn cursor_over(rgb_in: vec3<f32>, sp: vec2<f32>) -> vec3<f32> {
    var col = rgb_in;
    let kind = u.fx_meta.x;
    let radius = u.fx_meta.y;
    let count = i32(u.fx_meta.z);
    for (var k = 0; k < 3; k = k + 1) {
        if (k >= count || kind < 0.5) {
            break;
        }
        let f = u.fx[k];
        let d = length(sp - f.xy);
        let p = f.z;
        var a = 0.0;
        if (kind < 1.5) {
            let r = radius * (0.35 + 0.65 * p);
            a = ring_alpha(d, r, max(2.0, radius * 0.09)) * (1.0 - p);
        } else {
            let r = radius * (0.2 + 0.8 * p);
            let disc = (1.0 - smoothstep(r * 0.55, r, d)) * 0.35;
            a = max(disc, ring_alpha(d, r, max(2.0, radius * 0.05)) * 0.7) * (1.0 - p);
        }
        col = mix(col, u.fx_color.rgb, clamp(a * u.fx_color.a, 0.0, 1.0));
    }
    if (u.cur_b.z > 0.5 && u.cur_a.w > 0.0) {
        let size = u.cur_b.zw * u.cur_a.z;
        let tl = u.cur_a.xy - u.cur_b.xy * u.cur_a.z;
        let q = (sp - tl) / size;
        if (q.x >= 0.0 && q.x <= 1.0 && q.y >= 0.0 && q.y <= 1.0) {
            // The sprite is uploaded premultiplied.
            let s = textureSampleLevel(cursor_tex, samp, q, 0.0);
            col = s.rgb * u.cur_a.w + col * (1.0 - s.a * u.cur_a.w);
        }
    }
    return col;
}

fn draw_tile(col_in: vec3<f32>, i: i32, p: vec2<f32>) -> vec3<f32> {
    let t = u.tiles[i];
    if (t.params.w < 0.5) {
        return col_in;
    }
    var col = col_in;
    let half = t.rect.zw * 0.5;
    let c = t.rect.xy + half;
    let r = min(t.params.x, min(half.x, half.y));

    if (t.params.z > 0.0) {
        let sd_s = sd_rbox(p, c + vec2<f32>(0.0, u.bg_dir.z), half, r);
        let a = (1.0 - smoothstep(-u.bg_dir.w, u.bg_dir.w, sd_s)) * t.params.z;
        col = mix(col, vec3<f32>(0.0, 0.0, 0.0), a);
    }

    let sd = sd_rbox(p, c, half, r);
    let cover = clamp(0.5 - sd, 0.0, 1.0);
    if (cover > 0.0) {
        var f = clamp((p - t.rect.xy) / t.rect.zw, vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 1.0));
        if (t.flags.x > 0.5) {
            f.x = 1.0 - f.x;
        }
        let uv = mix(t.src.xy, t.src.zw, f);
        var dims: vec2<f32>;
        if (i == 0) {
            dims = vec2<f32>(textureDimensions(screen_tex));
        } else {
            dims = vec2<f32>(textureDimensions(camera_tex));
        }
        // Area filter: when the source is larger than the tile, average an n x n grid of
        // bilinear taps spread over the output pixel's footprint in the source, so a 4K screen
        // shown at 1080p does not alias. At 1:1 or magnified, n is 1 (one bilinear tap).
        let fp = (t.src.zw - t.src.xy) / t.rect.zw;
        let ratio = max(fp.x * dims.x, fp.y * dims.y);
        let n = i32(clamp(ceil(ratio), 1.0, 8.0));
        var sum = vec3<f32>(0.0, 0.0, 0.0);
        for (var jy = 0; jy < n; jy = jy + 1) {
            for (var jx = 0; jx < n; jx = jx + 1) {
                let o = (vec2<f32>(f32(jx), f32(jy)) + vec2<f32>(0.5, 0.5)) / f32(n) - vec2<f32>(0.5, 0.5);
                let tuv = uv + o * fp;
                var px: vec4<f32>;
                if (i == 0) {
                    px = textureSampleLevel(screen_tex, samp, tuv, 0.0);
                } else {
                    px = textureSampleLevel(camera_tex, samp, tuv, 0.0);
                }
                var rgb_t = px.rgb;
                if (t.flags.y > 0.5) {
                    let ov = textureSampleLevel(overlay_tex, samp, tuv, 0.0);
                    rgb_t = mix(rgb_t, ov.rgb, ov.a);
                }
                if (i == 0 && (u.cur_a.w > 0.0 || u.fx_meta.z > 0.5)) {
                    rgb_t = cursor_over(rgb_t, tuv * dims);
                }
                sum = sum + rgb_t;
            }
        }
        let rgb = sum / f32(n * n);
        col = mix(col, rgb, cover);
    }

    if (t.params.y > 0.0) {
        let outer = clamp(0.5 - sd, 0.0, 1.0);
        let inner = clamp(0.5 - (sd + t.params.y), 0.0, 1.0);
        col = mix(col, t.ring_color.rgb, (outer - inner) * t.ring_color.a);
    }
    return col;
}

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let p = pos.xy;
    var col = background(p);
    col = draw_tile(col, 0, p);
    col = draw_tile(col, 1, p);
    return vec4<f32>(col, 1.0);
}
