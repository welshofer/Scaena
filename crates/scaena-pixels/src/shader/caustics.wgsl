// caustics: the WGSL twin of `scaena_core::shader::caustics::Frame::pixel` (SPEC §3.8).
// The same arithmetic in the same order; change one, change the other. The shader
// parity test in scaena-paint holds them together.

struct Caustics {
    // Each layer's map from a device pixel's center to its cells: u = a·px + c·py + e,
    // v = b·px + d·py + f.
    map0: vec4<f32>,       // the coarse layer's a, b, c, d
    map1: vec4<f32>,       // the fine layer's a, b, c, d
    shift: vec4<f32>,      // e, f of the coarse layer, then of the fine one
    tune: vec4<f32>,       // 1 / a line's half width, intensity, grain, 0
    bbox: vec4<u32>,       // x, y, width, height in device pixels
    misc: vec4<u32>,       // the coarse layer's key, the fine layer's, the grain key, output row stride in words
    stops: vec4<u32>,      // palette stops, 0, 0, 0
    cells: vec4<i32>,      // the whole cells each layer's map leaves out: across and down, coarse then fine
    waves: vec4<f32>,      // where each layer's waves are: across and down, coarse then fine
    paths: array<vec4<f32>, 16>,        // a path's offset across and down, coarse then fine
    colors: array<vec4<f32>, 16>,       // Oklab L, a, b, alpha
    thresholds: array<vec4<f32>, 64>,   // linear light halfway between sRGB bytes
}

@group(0) @binding(0) var<uniform> caustics: Caustics;
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
        if caustics.thresholds[j / 4u][j % 4u] <= v {
            k = k + step;
        }
        step = step / 2u;
    }
    return k;
}

// Where the point of cell (i, j) of layer `k` is, in the layer's cells.
fn at(k: u32, i: i32, j: i32) -> vec2<f32> {
    let ci = bitcast<u32>(i + caustics.cells[2u * k]);
    let cj = bitcast<u32>(j + caustics.cells[2u * k + 1u]);
    let h = lowbias32(caustics.misc[k] ^ lowbias32(ci ^ lowbias32(cj)));
    let g = lowbias32(h);
    let across = caustics.paths[g & 15u][2u * k];
    let down = caustics.paths[(g >> 4u) & 15u][2u * k + 1u];
    let x = 0.2 + 0.6 * (f32(h & 0xffffu) * (1.0 / 65536.0)) + across;
    let y = 0.2 + 0.6 * (f32(h >> 16u) * (1.0 / 65536.0)) + down;
    return vec2<f32>(f32(i) + x, f32(j) + y);
}

// A smooth wave, 1 long, from -1 to 1: two halves of a parabola.
fn wave(x: f32) -> f32 {
    let y = 2.0 * (x - round(x));
    return 4.0 * y * (1.0 - abs(y));
}

// Layer `k`'s light at the device pixel (px, py): its place in the layer's cells, bent by the
// layer's two waves; then how near it lies to the edge between the two points nearest it, among
// the nine cells around its own.
fn light(k: u32, map: vec4<f32>, e: f32, f: f32, px: f32, py: f32) -> f32 {
    let u0 = map.x * px + map.z * py + e;
    let v0 = map.y * px + map.w * py + f;
    let u = u0 + 0.3 * wave(v0 * 0.31 + caustics.waves[2u * k]);
    let v = v0 + 0.3 * wave(u0 * 0.31 + caustics.waves[2u * k + 1u]);
    let i = i32(floor(u));
    let j = i32(floor(v));
    var f1 = bitcast<f32>(0x7f7fffffu);
    var f2 = bitcast<f32>(0x7f7fffffu);
    for (var cj = j - 1; cj <= j + 1; cj = cj + 1) {
        for (var ci = i - 1; ci <= i + 1; ci = ci + 1) {
            let q = at(k, ci, cj);
            let dx = u - q.x;
            let dy = v - q.y;
            let dd = dx * dx + dy * dy;
            f2 = min(f2, max(f1, dd));
            f1 = min(f1, dd);
        }
    }
    let s = 0.5 * (sqrt(f2) - sqrt(f1)) * caustics.tune.x;
    let w = 1.0 + s * s;
    return 1.0 / (w * w);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= caustics.bbox.z || id.y >= caustics.bbox.w {
        return;
    }
    let px = f32(caustics.bbox.x + id.x) + 0.5;
    let py = f32(caustics.bbox.y + id.y) + 0.5;
    let coarse = light(0u, caustics.map0, caustics.shift.x, caustics.shift.y, px, py);
    let fine = 0.55 * light(1u, caustics.map1, caustics.shift.z, caustics.shift.w, px, py);
    let lit = coarse + fine - coarse * fine;
    // Each stop's share of the ramp at that place: the Oklab blend of the two either side.
    let n = caustics.stops.x;
    let along = clamp(caustics.tune.y * lit, 0.0, 1.0) * f32(n - 1u);
    var c = vec4<f32>(0.0, 0.0, 0.0, 0.0);
    for (var s = 0u; s < n; s = s + 1u) {
        let w = max(0.0, 1.0 - abs(along - f32(s)));
        c = vec4<f32>(c.x + w * caustics.colors[s].x, c.y + w * caustics.colors[s].y, c.z + w * caustics.colors[s].z, c.w + w * caustics.colors[s].w);
    }
    let g = f32(lowbias32(caustics.misc.z ^ lowbias32(id.x ^ lowbias32(id.y))) >> 8u) * (1.0 / 16777216.0) - 0.5;
    let lg = c.x + caustics.tune.z * g;
    let lm = lg + 0.39633778 * c.y + 0.21580376 * c.z;
    let mm = lg - 0.105561346 * c.y - 0.06385417 * c.z;
    let sm = lg - 0.08948418 * c.y - 1.2914855 * c.z;
    let lc = lm * lm * lm;
    let mc = mm * mm * mm;
    let sc = sm * sm * sm;
    let r = 4.0767417 * lc - 3.3077116 * mc + 0.23096994 * sc;
    let gr = -1.268438 * lc + 2.6097574 * mc - 0.34131938 * sc;
    let bl = -0.0041960864 * lc - 0.7034186 * mc + 1.7076147 * sc;
    let a8 = u32(clamp(c.w, 0.0, 1.0) * 255.0 + 0.5);
    pixels[id.y * caustics.misc.w + id.x] = encode(r) | (encode(gr) << 8u) | (encode(bl) << 16u) | (a8 << 24u);
}
