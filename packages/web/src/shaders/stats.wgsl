// Global auto-levels statistic: p2/p98 on a 256x256 luma probe.
//
// Two dispatches, both tiny:
//   hist_main — 256-bin histogram of the probe (atomic adds).
//   scan_main — one invocation walks the 256 bins, finds the 2% and 98%
//               bins, and writes [lo, hi] to `stretch`.
//
// The stretch only engages when the frame is genuinely low-contrast. The
// 0.4 gate leaves every still frame in the real field corpus untouched (its
// minimum p98-p2 is 0.47 at the 256x256 probe), while the synthetic
// dim+veil variants sit at 0.09-0.19. Min/max on the same probe is too
// noise-sensitive: it stretched 3 real photos into failures
// (notes/STATUS.md low-light A/B).
//
// A small symbol on a large uniform background can cover < 2% of the frame,
// which collapses p2 and p98 onto the background bin. In that case fall
// back to the first/last populated bin (min/max quantized by the 256-bin
// histogram). Real photos never take that branch: their percentiles are
// never degenerate, so the noisy extremes never set the stretch.
struct HistUniforms {
  width: u32,
  height: u32,
};

@group(0) @binding(0) var<uniform> hu: HistUniforms;
@group(0) @binding(1) var<storage, read> luma: array<f32>;
@group(0) @binding(2) var<storage, read_write> hist: array<atomic<u32>>;
@group(0) @binding(3) var<storage, read_write> stretch: array<f32>;

@compute @workgroup_size(8, 8)
fn hist_main(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= hu.width || gid.y >= hu.height) { return; }
  let v = clamp(luma[gid.y * hu.width + gid.x], 0.0, 1.0);
  let bin = min(255u, u32(v * 255.0));
  atomicAdd(&hist[bin], 1u);
}

@compute @workgroup_size(1)
fn scan_main() {
  var total = 0u;
  var minBin = 0u;
  var maxBin = 255u;
  var gotMin = false;
  for (var i = 0u; i < 256u; i = i + 1u) {
    let count = atomicLoad(&hist[i]);
    total = total + count;
    if (count > 0u) {
      if (!gotMin) {
        minBin = i;
        gotMin = true;
      }
      maxBin = i;
    }
  }
  if (total == 0u) {
    stretch[0] = 0.0;
    stretch[1] = 1.0;
    return;
  }
  let tLo = total * 2u / 100u;
  let tHi = total * 98u / 100u;
  var cum = 0u;
  var loBin = 0u;
  var hiBin = 255u;
  var gotLo = false;
  for (var i = 0u; i < 256u; i = i + 1u) {
    cum = cum + atomicLoad(&hist[i]);
    if (!gotLo && cum >= tLo) {
      loBin = i;
      gotLo = true;
    }
    if (cum >= tHi) {
      hiBin = i;
      break;
    }
  }
  var lo = f32(loBin) / 255.0;
  var hi = f32(hiBin) / 255.0;
  // Percentiles collapsed (small feature on a uniform field): use the
  // populated extremes instead, still quantized to 1/255.
  if (hi - lo <= 2.0 / 255.0) {
    lo = f32(minBin) / 255.0;
    hi = f32(maxBin) / 255.0;
  }
  // GATE: dim (white point still low) AND low-contrast frames only.
  // Everything else stays bit-identical to the pre-auto-levels pipeline.
  // The brightness arm separates a blurred bright code (white point 0.84+,
  // do not touch) from a genuinely dim capture; the real field corpus'
  // minimum p98 is 0.72.
  if (hi < 0.65 && hi - lo < 0.40 && hi - lo > 2.0 / 255.0) {
    stretch[0] = lo;
    stretch[1] = hi;
  } else {
    stretch[0] = 0.0;
    stretch[1] = 1.0;
  }
}
