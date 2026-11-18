// Stage 2: separable box mean (Bradley local threshold denominator).
// A fixed-radius moving average along one axis; O(1) work per pixel with the
// naive windowed sum at the radii we use (< 100 taps), and no integral image
// or scan pass needed. Run once with axis=0 then axis=1.
struct Uniforms {
  width: u32,
  height: u32,
  radius: i32,
  axis: u32, // 0 = horizontal, 1 = vertical
};

@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var<storage, read> input: array<f32>;
@group(0) @binding(2) var<storage, read_write> output: array<f32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3u) {
  if (gid.x >= u.width || gid.y >= u.height) { return; }
  var sum = 0.0;
  var count = 0u;
  for (var d = -u.radius; d <= u.radius; d = d + 1) {
    var sx = gid.x;
    var sy = gid.y;
    if (u.axis == 0u) {
      let x = i32(gid.x) + d;
      if (x < 0 || x >= i32(u.width)) { continue; }
      sx = u32(x);
    } else {
      let y = i32(gid.y) + d;
      if (y < 0 || y >= i32(u.height)) { continue; }
      sy = u32(y);
    }
    sum = sum + input[sy * u.width + sx];
    count = count + 1u;
  }
  output[gid.y * u.width + gid.x] = sum / f32(count);
}
