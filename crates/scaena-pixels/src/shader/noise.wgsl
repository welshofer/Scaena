// noise: the WGSL twin of `scaena_core::shader::noise::Frame::pixel` (SPEC §3.8).
// The same arithmetic in the same order; change one, change the other. The shader
// parity test in scaena-paint holds them together.

struct Noise {
    // A device pixel's center to noise space: x = a·px + c·py + e, y = b·px + d·py + f.
    map: vec4<f32>,        // a, b, c, d
    shift: vec4<f32>,      // e, f, z (the time axis), grain
    bbox: vec4<u32>,       // x, y, width, height in device pixels
    misc: vec4<u32>,       // octaves, stops, key, output row stride in words
    tune: vec4<f32>,       // contrast, 0, 0, 0
    colors: array<vec4<f32>, 16>,       // Oklab L, a, b, alpha
    thresholds: array<vec4<f32>, 64>,   // linear light halfway between sRGB bytes
}

@group(0) @binding(0) var<uniform> noise: Noise;
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
        if noise.thresholds[j / 4u][j % 4u] <= v {
            k = k + step;
        }
        step = step / 2u;
    }
    return k;
}

fn sample(u: f32) -> vec4<f32> {
    let n = noise.misc.y;
    if n == 1u {
        return noise.colors[0];
    }
    let spans = n - 1u;
    let s = u * f32(spans);
    let k = min(u32(max(floor(s), 0.0)), spans - 1u);
    let f = s - f32(k);
    let a = noise.colors[k];
    let b = noise.colors[(k + 1u) % n];
    return vec4<f32>(a.x + f * (b.x - a.x), a.y + f * (b.y - a.y), a.z + f * (b.z - a.z), a.w + f * (b.w - a.w));
}

// Ken Perlin's gradient pick, dotted with (x, y, z).
fn grad(hash: u32, x: f32, y: f32, z: f32) -> f32 {
    let h = hash & 15u;
    var u = y;
    if h < 8u {
        u = x;
    }
    var v = z;
    if h < 4u {
        v = y;
    } else if h == 12u || h == 14u {
        v = x;
    }
    var a = u;
    if (h & 1u) != 0u {
        a = -u;
    }
    var b = v;
    if (h & 2u) != 0u {
        b = -v;
    }
    return a + b;
}

fn corner(key: u32, lattice: vec3<u32>, x: f32, y: f32, z: f32) -> f32 {
    let t = 0.6 - x * x - y * y - z * z;
    if t < 0.0 {
        return 0.0;
    }
    let h = lowbias32(key ^ lowbias32(lattice.x ^ lowbias32(lattice.y ^ lowbias32(lattice.z))));
    let t2 = t * t;
    return t2 * t2 * grad(h, x, y, z);
}

fn simplex(x: f32, y: f32, z: f32, key: u32) -> f32 {
    let F3 = 0.33333334;
    let G3 = 0.16666667;
    let s = (x + y + z) * F3;
    let i = floor(x + s);
    let j = floor(y + s);
    let k = floor(z + s);
    let t = (i + j + k) * G3;
    let x0 = x - (i - t);
    let y0 = y - (j - t);
    let z0 = z - (k - t);
    var o1 = vec3<u32>(0u, 1u, 0u);
    var o2 = vec3<u32>(1u, 1u, 0u);
    if x0 >= y0 {
        if y0 >= z0 {
            o1 = vec3<u32>(1u, 0u, 0u);
            o2 = vec3<u32>(1u, 1u, 0u);
        } else if x0 >= z0 {
            o1 = vec3<u32>(1u, 0u, 0u);
            o2 = vec3<u32>(1u, 0u, 1u);
        } else {
            o1 = vec3<u32>(0u, 0u, 1u);
            o2 = vec3<u32>(1u, 0u, 1u);
        }
    } else if y0 < z0 {
        o1 = vec3<u32>(0u, 0u, 1u);
        o2 = vec3<u32>(0u, 1u, 1u);
    } else if x0 < z0 {
        o1 = vec3<u32>(0u, 1u, 0u);
        o2 = vec3<u32>(0u, 1u, 1u);
    }
    let x1 = x0 - f32(o1.x) + G3;
    let y1 = y0 - f32(o1.y) + G3;
    let z1 = z0 - f32(o1.z) + G3;
    let x2 = x0 - f32(o2.x) + F3;
    let y2 = y0 - f32(o2.y) + F3;
    let z2 = z0 - f32(o2.z) + F3;
    let x3 = x0 - 0.5;
    let y3 = y0 - 0.5;
    let z3 = z0 - 0.5;
    let at = vec3<u32>(bitcast<u32>(i32(i)), bitcast<u32>(i32(j)), bitcast<u32>(i32(k)));
    let n0 = corner(key, at, x0, y0, z0);
    let n1 = corner(key, at + o1, x1, y1, z1);
    let n2 = corner(key, at + o2, x2, y2, z2);
    let n3 = corner(key, at + vec3<u32>(1u, 1u, 1u), x3, y3, z3);
    return 32.0 * (n0 + n1 + n2 + n3);
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= noise.bbox.z || id.y >= noise.bbox.w {
        return;
    }
    let px = f32(noise.bbox.x + id.x) + 0.5;
    let py = f32(noise.bbox.y + id.y) + 0.5;
    let x = noise.map.x * px + noise.map.z * py + noise.shift.x;
    let y = noise.map.y * px + noise.map.w * py + noise.shift.y;
    var sum = 0.0;
    var norm = 0.0;
    var amp = 1.0;
    var freq = 1.0;
    for (var o = 0u; o < noise.misc.x; o = o + 1u) {
        sum = sum + amp * simplex(x * freq, y * freq, noise.shift.z * freq, lowbias32(noise.misc.z + o));
        norm = norm + amp;
        amp = amp * 0.5;
        freq = freq * 2.0;
    }
    let n = sum / norm;
    let c = sample(clamp(0.5 + 0.5 * noise.tune.x * n, 0.0, 1.0));
    let g = f32(lowbias32(noise.misc.z ^ lowbias32(id.x ^ lowbias32(id.y))) >> 8u) * (1.0 / 16777216.0) - 0.5;
    let lg = c.x + noise.shift.w * g;
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
    pixels[id.y * noise.misc.w + id.x] = encode(r) | (encode(gr) << 8u) | (encode(bl) << 16u) | (a8 << 24u);
}
