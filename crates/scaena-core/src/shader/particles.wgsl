// particles: the WGSL twin of `scaena_core::shader::particles::Frame::pixel` (SPEC §3.8).
// The same arithmetic in the same order; change one, change the other. The shader
// parity test in scaena-paint holds them together.

struct Particles {
    // A device pixel's center to shader space: x = a·px + c·py + e, y = b·px + d·py + f.
    map: vec4<f32>,        // a, b, c, d
    shift: vec4<f32>,      // e, f, 0, 0
    bbox: vec4<u32>,       // x, y, width, height in device pixels
    misc: vec4<u32>,       // particle count, output row stride in words, 0, 0
    discs: array<vec4<f32>, 64>,        // center x, y, radius², 1 / the radius² the edge fades over
    colors: array<vec4<f32>, 64>,       // linear r, g, b, alpha
    thresholds: array<vec4<f32>, 64>,   // linear light halfway between sRGB bytes
}

@group(0) @binding(0) var<uniform> particles: Particles;
@group(0) @binding(1) var<storage, read_write> pixels: array<u32>;

// Linear light to an sRGB byte: how many thresholds lie at or below `v`.
fn encode(v: f32) -> u32 {
    var k = 0u;
    var step = 128u;
    loop {
        if step == 0u {
            break;
        }
        let j = k + step - 1u;
        if particles.thresholds[j / 4u][j % 4u] <= v {
            k = k + step;
        }
        step = step / 2u;
    }
    return k;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= particles.bbox.z || id.y >= particles.bbox.w {
        return;
    }
    let px = f32(particles.bbox.x + id.x) + 0.5;
    let py = f32(particles.bbox.y + id.y) + 0.5;
    let x = particles.map.x * px + particles.map.z * py + particles.shift.x;
    let y = particles.map.y * px + particles.map.w * py + particles.shift.y;
    var r = 0.0;
    var g = 0.0;
    var bl = 0.0;
    var alpha = 0.0;
    for (var i = 0u; i < particles.misc.x; i = i + 1u) {
        let disc = particles.discs[i];
        let k = particles.colors[i];
        let dx = x - disc.x;
        let dy = y - disc.y;
        let d2 = dx * dx + dy * dy;
        if d2 < disc.z {
            let cover = min((disc.z - d2) * disc.w, 1.0) * k.w;
            let rest = 1.0 - cover;
            r = k.x * cover + r * rest;
            g = k.y * cover + g * rest;
            bl = k.z * cover + bl * rest;
            alpha = cover + alpha * rest;
        }
    }
    var out = 0u;
    if alpha > 0.0 {
        let inv = 1.0 / alpha;
        let a8 = u32(min(alpha, 1.0) * 255.0 + 0.5);
        out = encode(r * inv) | (encode(g * inv) << 8u) | (encode(bl * inv) << 16u) | (a8 << 24u);
    }
    pixels[id.y * particles.misc.y + id.x] = out;
}
