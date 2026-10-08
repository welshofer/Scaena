// gradient: the WGSL twin of `scaena_core::shader::gradient::Frame::pixel` (SPEC §3.8).
// The same arithmetic in the same order; change one, change the other. The shader
// parity test in scaena-paint holds them together.

struct Gradient {
    // A device pixel's center to (u, v): u = a·px + c·py + e, v = b·px + d·py + f.
    map: vec4<f32>,        // a, b, c, d
    shift: vec4<f32>,      // e, f, grain, conic start in turns
    bbox: vec4<u32>,       // x, y, width, height in device pixels
    misc: vec4<u32>,       // shape (0 linear, 1 radial, 2 conic), stops, grain key, output row stride in words
    colors: array<vec4<f32>, 16>,       // Oklab L, a, b, alpha
    thresholds: array<vec4<f32>, 64>,   // linear light halfway between sRGB bytes
}

@group(0) @binding(0) var<uniform> gradient: Gradient;
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
        if gradient.thresholds[j / 4u][j % 4u] <= v {
            k = k + step;
        }
        step = step / 2u;
    }
    return k;
}

// The ramp at `u`: its colors evenly spaced, blended in Oklab; a cyclic ramp runs on
// from its last color to its first.
fn sample(u: f32, cyclic: bool) -> vec4<f32> {
    let n = gradient.misc.y;
    if n == 1u {
        return gradient.colors[0];
    }
    var spans = n - 1u;
    if cyclic {
        spans = n;
    }
    let s = u * f32(spans);
    let k = min(u32(max(floor(s), 0.0)), spans - 1u);
    let f = s - f32(k);
    let a = gradient.colors[k];
    let b = gradient.colors[(k + 1u) % n];
    return vec4<f32>(a.x + f * (b.x - a.x), a.y + f * (b.y - a.y), a.z + f * (b.z - a.z), a.w + f * (b.w - a.w));
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= gradient.bbox.z || id.y >= gradient.bbox.w {
        return;
    }
    let px = f32(gradient.bbox.x + id.x) + 0.5;
    let py = f32(gradient.bbox.y + id.y) + 0.5;
    let u = gradient.map.x * px + gradient.map.z * py + gradient.shift.x;
    let v = gradient.map.y * px + gradient.map.w * py + gradient.shift.y;
    var at = u;
    if gradient.misc.x == 1u {
        at = sqrt(u * u + v * v);
    } else if gradient.misc.x == 2u {
        let turn = atan2(u, -v) * 0.15915494 - gradient.shift.w;
        at = turn - floor(turn);
    }
    let c = sample(clamp(at, 0.0, 1.0), gradient.misc.x == 2u);
    let n = f32(lowbias32(gradient.misc.z ^ lowbias32(id.x ^ lowbias32(id.y))) >> 8u) * (1.0 / 16777216.0) - 0.5;
    let lg = c.x + gradient.shift.z * n;
    let lm = lg + 0.39633778 * c.y + 0.21580376 * c.z;
    let mm = lg - 0.105561346 * c.y - 0.06385417 * c.z;
    let sm = lg - 0.08948418 * c.y - 1.2914855 * c.z;
    let lc = lm * lm * lm;
    let mc = mm * mm * mm;
    let sc = sm * sm * sm;
    let r = 4.0767417 * lc - 3.3077116 * mc + 0.23096994 * sc;
    let g = -1.268438 * lc + 2.6097574 * mc - 0.34131938 * sc;
    let bl = -0.0041960864 * lc - 0.7034186 * mc + 1.7076147 * sc;
    let a8 = u32(clamp(c.w, 0.0, 1.0) * 255.0 + 0.5);
    pixels[id.y * gradient.misc.w + id.x] = encode(r) | (encode(g) << 8u) | (encode(bl) << 16u) | (a8 << 24u);
}
