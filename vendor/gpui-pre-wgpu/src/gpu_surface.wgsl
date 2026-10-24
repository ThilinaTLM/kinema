// Experimental opaque RGBA8Unorm SDR surface; no YUV/HDR/color-management claim.
struct Globals {
    viewport_size: vec2<f32>,
    premultiplied_alpha: u32,
    pad: u32,
}
struct Bounds {
    origin: vec2<f32>,
    size: vec2<f32>,
}
struct SurfaceParams {
    bounds: Bounds,
    content_mask: Bounds,
    opacity: f32,
    pad0: f32,
    pad1: f32,
    pad2: f32,
}
@group(0) @binding(0) var<uniform> globals: Globals;
@group(1) @binding(0) var<uniform> surface: SurfaceParams;
@group(1) @binding(1) var image: texture_2d<f32>;
@group(1) @binding(2) var image_sampler: sampler;
struct Varying {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) pixel_position: vec2<f32>,
}
@vertex
fn vs_main(@builtin(vertex_index) id: u32) -> Varying {
    let uv = vec2<f32>(f32(id & 1u), 0.5 * f32(id & 2u));
    let pixel = surface.bounds.origin + uv * surface.bounds.size;
    var out: Varying;
    out.position = vec4<f32>(pixel / globals.viewport_size * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    out.pixel_position = pixel;
    return out;
}
@fragment
fn fs_main(input: Varying) -> @location(0) vec4<f32> {
    if (any(input.pixel_position < surface.content_mask.origin) ||
        any(input.pixel_position >= surface.content_mask.origin + surface.content_mask.size)) {
        discard;
    }
    let rgb = textureSampleLevel(image, image_sampler, input.uv, 0.0).rgb;
    let alpha = clamp(surface.opacity, 0.0, 1.0);
    if (globals.premultiplied_alpha != 0u) {
        return vec4<f32>(rgb * alpha, alpha);
    }
    return vec4<f32>(rgb, alpha);
}
