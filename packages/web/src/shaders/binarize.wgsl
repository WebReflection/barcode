// Stage 3: Bradley adaptive binarization.
// ink := luma < mean * (num/den), then packed 1bpp into u32 words with
// atomics (8 lanes share a word). Layout contract with barcode-core's
// BitImage: bit (x,y) = data[y*bytesPerRow + (x>>3)] >> (x&7) & 1.
//
// Purely local thresholds erase solid dark regions larger than the window
// (the mean converges to the luma itself — QR finder centers die exactly
// this way). The global-mean floor rescues them — but ONLY where the
// neighborhood itself is darker than the floor: in a frame with a big
// hand/table shadow the global mean drops, and an unconditional floor
// force-inks the shadow's light modules too, shredding codes that cross
// the shadow boundary (2026-10-23 receipt field bug).
struct Uniforms {
  width: u32,
  height: u32,
  thresholdNum: u32, // use 85, 100 for 0.85
  thresholdDen: u32,
  floorNum: u32, // use 500, 1000 for a 0.5 * globalMean floor
  floorDen: u32,
  gw: u32, // global-mean spread is computed on a gw x gh downsample
  gh: u32,
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<storage, read> luma: array<f32>;
@group(0) @binding(2) var<storage, read> mean: array<f32>;
@group(0) @binding(3) var<storage, read_write> mask: array<atomic<u32>>;
@group(0) @binding(4) var<storage, read> globalMean: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= u.width || gid.y >= u.height) { return; }
  let idx = gid.y * u.width + gid.x;
  let local = f32(u.thresholdNum) / f32(u.thresholdDen);
  let floor_ = f32(u.floorNum) / f32(u.floorDen);
  let gx = min(u.gw - 1u, gid.x * u.gw / u.width);
  let gy = min(u.gh - 1u, gid.y * u.gh / u.height);
  let gMean = globalMean[gy * u.gw + gx];
  let gFloor = gMean * floor_;
  // 8/255 of the 0-1 luma range: absorbs float noise in uniform regions
  // (luma == mean exactly on synthetic renders) without rescuing shadowed
  // light modules (~0.06 above their neighborhood mean on the receipt).
  let isInk = luma[idx] < mean[idx] * local || (luma[idx] < gFloor && luma[idx] <= mean[idx] + 8.0 / 255.0);
  if (isInk) {
    atomicOr(&mask[idx >> 5u], 1u << (idx & 31u));
  }
}
