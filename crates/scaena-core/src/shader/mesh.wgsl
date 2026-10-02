// mesh: the WGSL twin of `scaena_core::shader::mesh::Frame::pixel` (SPEC §3.8).
// The same arithmetic in the same order; change one, change the other. The shader
// parity test in scaena-paint holds them together.

struct Mesh {
    // A device pixel's center to shader space: x = a·px + c·py + e, y = b·px + d·py + f.
    map: vec4<f32>,        // a, b, c, d
    shift: vec4<f32>,      // e, f, grain, 1/σ²
    bbox: vec4<u32>,       // x, y, width, height in device pixels
    misc: vec4<u32>,       // point count, grain key, output row stride in words, 0
    points: array<vec4<f32>, 16>,       // x, y, alpha, 0
    colors: array<vec4<f32>, 16>,       // Oklab L, a, b, 0
    thresholds: array<vec4<f32>, 64>,   // linear light halfway between sRGB bytes
}

@group(0) @binding(0) var<uniform> mesh: Mesh;
@group(0) @binding(1) var<storage, read_write> pixels: array<u32>;

fn lowbias32(v: u32) -> u32 {
    var x = v;
    x = x ^ (x >> 16u);
    x = x * 0x7feb352du;
    x = x ^ (x >> 15u);
    x = x * 0x846ca68bu;
    return x ^ (x >> 16u);
}

// Linear light to an sRGB byte: how many thresholds lie at or below `v`.
fn encode(v: f32) -> u32 {
    var k = 0u;
    var step = 128u;
    loop {
        if step == 0u {
            break;
        }
        let j = k + step - 1u;
        if mesh.thresholds[j / 4u][j % 4u] <= v {
            k = k + step;
        }
        step = step / 2u;
    }
    return k;
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= mesh.bbox.z || id.y >= mesh.bbox.w {
        return;
    }
    let px = f32(mesh.bbox.x + id.x) + 0.5;
    let py = f32(mesh.bbox.y + id.y) + 0.5;
    let x = mesh.map.x * px + mesh.map.z * py + mesh.shift.x;
    let y = mesh.map.y * px + mesh.map.w * py + mesh.shift.y;
    var sum = 0.0;
    var l = 0.0;
    var ca = 0.0;
    var cb = 0.0;
    var alpha = 0.0;
    for (var i = 0u; i < mesh.misc.x; i = i + 1u) {
        let p = mesh.points[i];
        let k = mesh.colors[i];
        let dx = x - p.x;
        let dy = y - p.y;
        let u = 1.0 + (dx * dx + dy * dy) * mesh.shift.w;
        let w = 1.0 / (u * u);
        sum = sum + w;
        l = l + w * k.x;
        ca = ca + w * k.y;
        cb = cb + w * k.z;
        alpha = alpha + w * p.z;
    }
    let inv = 1.0 / sum;
    let n = f32(lowbias32(mesh.misc.y ^ lowbias32(id.x ^ lowbias32(id.y))) >> 8u) * (1.0 / 16777216.0) - 0.5;
    let lg = l * inv + mesh.shift.z * n;
    let a = ca * inv;
    let b = cb * inv;
    let lm = lg + 0.39633778 * a + 0.21580376 * b;
    let mm = lg - 0.105561346 * a - 0.06385417 * b;
    let sm = lg - 0.08948418 * a - 1.2914855 * b;
    let lc = lm * lm * lm;
    let mc = mm * mm * mm;
    let sc = sm * sm * sm;
    let r = 4.0767417 * lc - 3.3077116 * mc + 0.23096994 * sc;
    let g = -1.268438 * lc + 2.6097574 * mc - 0.34131938 * sc;
    let bl = -0.0041960864 * lc - 0.7034186 * mc + 1.7076147 * sc;
    let a8 = u32(clamp(alpha * inv, 0.0, 1.0) * 255.0 + 0.5);
    pixels[id.y * mesh.misc.z + id.x] = encode(r) | (encode(g) << 8u) | (encode(bl) << 16u) | (a8 << 24u);
}
