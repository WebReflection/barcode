// Stage 1: luma extraction (+ optional pad to the dst grid).
// The source arrives as an rgba8unorm texture (copyExternalImageToTexture)
// at its NATIVE size; the dst grid may be larger (32-multiple snap-up).
// Source pixels are sampled 1:1 — never resampled — and the pad margin is
// written white. Linear filtering only engages when the source EXCEEDS
// maxSize and the frame must come down (downsampling is unavoidable there;
// an upsample of a marginal symbol is not).
//
// `apply` turns on the global auto-levels stretch (lo/hi are computed by
// stats.wgsl on the 256x256 probe and written to `stretch`). It stays 0 for
// that probe pass so the statistic never feeds back into itself. The pad
// margin is written white unconditionally: 1.0 always maps to >= 1.0 under
// the stretch.
struct Uniforms {
  dstW: u32,
  dstH: u32,
  srcW: u32,
  srcH: u32,
  apply: u32,
};

@group(0) @binding(0) var srcTex: texture_2d<f32>;
@group(0) @binding(1) var srcSamp: sampler;
@group(0) @binding(2) var<uniform> u: Uniforms;
@group(0) @binding(3) var<storage, read_write> outLuma: array<f32>;
@group(0) @binding(4) var<storage, read> stretch: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= u.dstW || gid.y >= u.dstH) { return; }
  if (gid.x >= u.srcW || gid.y >= u.srcH) {
    // pad margin: white (quiet zone) — only reachable when dst > src
    outLuma[gid.y * u.dstW + gid.x] = 1.0;
    return;
  }
  // pad-up (dst >= src): 1:1 texel mapping. Downscale (dst < src): the
  // original full-frame stretch over the dst grid.
  let uv = select(
    (vec2f(gid.xy) + 0.5) / vec2f(f32(u.dstW), f32(u.dstH)),
    (vec2f(gid.xy) + 0.5) / vec2f(f32(u.srcW), f32(u.srcH)),
    u.dstW >= u.srcW && u.dstH >= u.srcH,
  );
  let c = textureSampleLevel(srcTex, srcSamp, uv, 0.0);
  var l = dot(c.rgb, vec3f(0.299, 0.587, 0.114));
  if (u.apply == 1u) {
    let lo = stretch[0];
    let hi = stretch[1];
    l = clamp((l - lo) / max(hi - lo, 1e-3), 0.0, 1.0);
  }
  outLuma[gid.y * u.dstW + gid.x] = l;
}
