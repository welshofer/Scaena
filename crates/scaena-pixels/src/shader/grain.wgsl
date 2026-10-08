// grain: the WGSL twin of `scaena_core::shader::grain::Frame::pixel` (SPEC §3.8).
// The same arithmetic in the same order; change one, change the other. The shader
// parity test in scaena-paint holds them together.

struct Grain {
    bbox: vec4<u32>,       // x, y, width, height in device pixels
    misc: vec4<u32>,       // key, output row stride in words, dark RGB, light RGB (bytes, red lowest)
    tune: vec4<f32>,       // amount, dark alpha, light alpha, 0
}

@group(0) @binding(0) var<uniform> grain: Grain;
@group(0) @binding(1) var<storage, read_write> pixels: array<u32>;

fn lowbias32(v: u32) -> u32 {
    var x = v;
    x = x ^ (x >> 16u);
    x = x * 0x7feb352du;
    x = x ^ (x >> 15u);
    x = x * 0x846ca68bu;
    return x ^ (x >> 16u);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= grain.bbox.z || id.y >= grain.bbox.w {
        return;
    }
    let n = f32(lowbias32(grain.misc.x ^ lowbias32(id.x ^ lowbias32(id.y))) >> 8u) * (1.0 / 16777216.0) - 0.5;
    var rgb = grain.misc.w;
    var a = grain.tune.z;
    var far = n;
    if n < 0.0 {
        rgb = grain.misc.z;
        a = grain.tune.y;
        far = -n;
    }
    let alpha = far * 2.0 * grain.tune.x * a;
    pixels[id.y * grain.misc.y + id.x] = rgb | (u32(alpha * 255.0 + 0.5) << 24u);
}
