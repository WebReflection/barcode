class q{__destroy_into_raw(){const e=this.__wbg_ptr;return this.__wbg_ptr=0,Y.unregister(this),e}free(){const e=this.__destroy_into_raw();s.__wbg_barcodescanner_free(e,0)}decode(e,i,n){try{const u=s.__wbindgen_add_to_stack_pointer(-16),c=z(n,s.__wbindgen_export),g=w;s.barcodescanner_decode(u,this.__wbg_ptr,e,i,c,g);var t=l().getInt32(u+0,!0),a=l().getInt32(u+4,!0),o=l().getInt32(u+8,!0);if(o)throw b(a);return b(t)}finally{s.__wbindgen_add_to_stack_pointer(16)}}decode_filtered(e,i,n,t){try{const c=s.__wbindgen_add_to_stack_pointer(-16),g=z(n,s.__wbindgen_export),f=w,_=X(t,s.__wbindgen_export),v=w;s.barcodescanner_decode_filtered(c,this.__wbg_ptr,e,i,g,f,_,v);var a=l().getInt32(c+0,!0),o=l().getInt32(c+4,!0),u=l().getInt32(c+8,!0);if(u)throw b(o);return b(a)}finally{s.__wbindgen_add_to_stack_pointer(16)}}decode_filtered_still(e,i,n,t,a){try{const g=s.__wbindgen_add_to_stack_pointer(-16),f=z(n,s.__wbindgen_export),_=w,v=X(t,s.__wbindgen_export),y=w;s.barcodescanner_decode_filtered_still(g,this.__wbg_ptr,e,i,f,_,v,y,a);var o=l().getInt32(g+0,!0),u=l().getInt32(g+4,!0),c=l().getInt32(g+8,!0);if(c)throw b(u);return b(o)}finally{s.__wbindgen_add_to_stack_pointer(16)}}decode_still(e,i,n,t){try{const c=s.__wbindgen_add_to_stack_pointer(-16),g=z(n,s.__wbindgen_export),f=w;s.barcodescanner_decode_still(c,this.__wbg_ptr,e,i,g,f,t);var a=l().getInt32(c+0,!0),o=l().getInt32(c+4,!0),u=l().getInt32(c+8,!0);if(u)throw b(o);return b(a)}finally{s.__wbindgen_add_to_stack_pointer(16)}}constructor(){const e=s.barcodescanner_new();return this.__wbg_ptr=e,Y.register(this,this.__wbg_ptr,this),this}qr_presence(){return s.barcodescanner_qr_presence(this.__wbg_ptr)}}Symbol.dispose&&(q.prototype[Symbol.dispose]=q.prototype.free);function ge(r,e,i,n){try{const u=s.__wbindgen_add_to_stack_pointer(-16),c=T(r,s.__wbindgen_export,s.__wbindgen_export2),g=w,f=T(e,s.__wbindgen_export,s.__wbindgen_export2),_=w;s.encode_fixture(u,c,g,f,_,i,n);var t=l().getInt32(u+0,!0),a=l().getInt32(u+4,!0),o=l().getInt32(u+8,!0);if(o)throw b(a);return b(t)}finally{s.__wbindgen_add_to_stack_pointer(16)}}function fe(r,e,i,n){try{const u=s.__wbindgen_add_to_stack_pointer(-16),c=T(r,s.__wbindgen_export,s.__wbindgen_export2),g=w,f=T(e,s.__wbindgen_export,s.__wbindgen_export2),_=w;s.encode_fixture_hex(u,c,g,f,_,i,n);var t=l().getInt32(u+0,!0),a=l().getInt32(u+4,!0),o=l().getInt32(u+8,!0);if(o)throw b(a);return b(t)}finally{s.__wbindgen_add_to_stack_pointer(16)}}function _e(r,e,i,n,t,a,o,u,c,g,f){try{const m=s.__wbindgen_add_to_stack_pointer(-16),d=T(r,s.__wbindgen_export,s.__wbindgen_export2),h=w,U=T(e,s.__wbindgen_export,s.__wbindgen_export2),I=w;s.encode_fixture_opts(m,d,h,U,I,i,n,t,a,o,u,c,g,f);var _=l().getInt32(m+0,!0),v=l().getInt32(m+4,!0),y=l().getInt32(m+8,!0);if(y)throw b(v);return b(_)}finally{s.__wbindgen_add_to_stack_pointer(16)}}function he(r,e,i,n,t,a,o,u,c,g,f){try{const m=s.__wbindgen_add_to_stack_pointer(-16),d=T(r,s.__wbindgen_export,s.__wbindgen_export2),h=w,U=z(e,s.__wbindgen_export),I=w;s.encode_fixture_raw_opts(m,d,h,U,I,i,n,t,a,o,u,c,g,f);var _=l().getInt32(m+0,!0),v=l().getInt32(m+4,!0),y=l().getInt32(m+8,!0);if(y)throw b(v);return b(_)}finally{s.__wbindgen_add_to_stack_pointer(16)}}function Q(){return{__proto__:null,"./barcode_wasm_bg.js":{__proto__:null,__wbg_Error_30c8987f7c2ed4e2:function(e,i){const n=Error(L(e,i));return M(n)},__wbg_String_8564e559799eccda:function(e,i){const n=String(k(i)),t=T(n,s.__wbindgen_export,s.__wbindgen_export2),a=w;l().setInt32(e+4,a,!0),l().setInt32(e+0,t,!0)},__wbg___wbindgen_string_get_0380ccaa2f57f0d9:function(e,i){const n=k(i),t=typeof n=="string"?n:void 0;var a=ee(t)?0:T(t,s.__wbindgen_export,s.__wbindgen_export2),o=w;l().setInt32(e+4,o,!0),l().setInt32(e+0,a,!0)},__wbg___wbindgen_throw_41e9ee4f547fc59a:function(e,i){throw new Error(L(e,i))},__wbg_new_617a8cdb8bb1130e:function(){const e=new Object;return M(e)},__wbg_new_ee2291f50781bf1d:function(){const e=new Array;return M(e)},__wbg_now_aa4ccb83129e9e55:function(){return Date.now()},__wbg_set_6be42768c690e380:function(e,i,n){k(e)[b(i)]=b(n)},__wbg_set_bea140a88be9b277:function(e,i,n){k(e)[i>>>0]=b(n)},__wbindgen_generic_0000000000000001:function(e){return M(e)},__wbindgen_generic_0000000000000002:function(e,i){const n=L(e,i);return M(n)},__wbindgen_generic_0000000000000003:function(e){const i=BigInt.asUintN(64,e);return M(i)},__wbindgen_object_clone_ref:function(e){const i=k(e);return M(i)},__wbindgen_object_drop_ref:function(e){b(e)}}}}const Y=typeof FinalizationRegistry>"u"?{register:()=>{},unregister:()=>{}}:new FinalizationRegistry(r=>s.__wbg_barcodescanner_free(r,1));function M(r){D===B.length&&B.push(B.length+1);const e=D;return D=B[e],B[e]=r,e}function Z(r){r<1028||(B[r]=D,D=r)}let P=null;function l(){return(P===null||P.buffer.detached===!0||P.buffer.detached===void 0&&P.buffer!==s.memory.buffer)&&(P=new DataView(s.memory.buffer)),P}function L(r,e){return ne(r>>>0,e)}let G=null;function E(){return(G===null||G.byteLength===0)&&(G=new Uint8Array(s.memory.buffer)),G}function k(r){return B[r]}let B=new Array(1024).fill(void 0);B.push(void 0,null,!0,!1);let D=B.length;function ee(r){return r==null}function z(r,e){const i=e(r.length*1,1)>>>0;return E().set(r,i/1),w=r.length,i}function X(r,e){const i=e(r.length*4,4)>>>0,n=l();for(let t=0;t<r.length;t++)n.setUint32(i+4*t,M(r[t]),!0);return w=r.length,i}function T(r,e,i){if(i===void 0){const u=R.encode(r),c=e(u.length,1)>>>0;return E().subarray(c,c+u.length).set(u),w=u.length,c}let n=r.length,t=e(n,1)>>>0;const a=E();let o=0;for(;o<n;o++){const u=r.charCodeAt(o);if(u>127)break;a[t+o]=u}if(o!==n){o!==0&&(r=r.slice(o)),t=i(t,n,n=o+r.length*3,1)>>>0;const u=E().subarray(t+o,t+n),c=R.encodeInto(r,u);o+=c.written,t=i(t,n,o,1)>>>0}return w=o,t}function b(r){const e=k(r);return Z(r),e}let O=new TextDecoder("utf-8",{ignoreBOM:!0,fatal:!0});O.decode();const te=2146435072;let F=0;function ne(r,e){return F+=e,F>=te&&(O=new TextDecoder("utf-8",{ignoreBOM:!0,fatal:!0}),O.decode(),F=e),O.decode(E().subarray(r,r+e))}const R=new TextEncoder;"encodeInto"in R||(R.encodeInto=function(r,e){const i=R.encode(r);return e.set(i),{read:r.length,written:i.length}});let w=0,s;function re(r,e){return s=r.exports,P=null,G=null,s}async function ie(r,e){if(typeof Response=="function"&&r instanceof Response){if(!r.ok)throw new Error(`failed to fetch Wasm: ${r.status} ${r.statusText} fetching '${r.url}'`);if(typeof WebAssembly.instantiateStreaming=="function")try{return await WebAssembly.instantiateStreaming(r,e)}catch(t){if(i(r.type)&&r.headers.get("Content-Type")!=="application/wasm")console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n",t);else throw t}const n=await r.arrayBuffer();return await WebAssembly.instantiate(n,e)}else{const n=await WebAssembly.instantiate(r,e);return n instanceof WebAssembly.Instance?{instance:n,module:r}:n}function i(n){switch(n){case"basic":case"cors":case"default":return!0}return!1}}async function oe(r){if(s!==void 0)return s;r!==void 0&&(Object.getPrototypeOf(r)===Object.prototype?{module_or_path:r}=r:console.warn("using deprecated parameters for the initialization function; pass a single object instead")),r===void 0&&(r=new URL(""+new URL("barcode_wasm_bg-Ih_yD6hG.wasm",import.meta.url).href,import.meta.url));const e=Q();(typeof r=="string"||typeof Request=="function"&&r instanceof Request||typeof URL=="function"&&r instanceof URL)&&(r=fetch(r));const{instance:i,module:n}=await ie(await r,e);return re(i)}const ae=""+new URL("barcode_wasm_bg-Ih_yD6hG.wasm",import.meta.url).href,se=()=>{try{const e=new URL(import.meta.url),i=e.searchParams.get("wasm");if(i)return i;const n=e.hash.replace(/^#/,"");if(n.startsWith("wasm=")){const t=decodeURIComponent(n.slice(5));if(t)return t}}catch{}const r=globalThis.__barcodeWasm;return typeof r=="string"?r:ae};let H=null;function be(){return H||(H=(async()=>(await oe(se()),new q))()),H}function we(r,e,i,n={}){const t=n.still===!0,a=n.latency??5e3;return(i?.length?t?r.decode_filtered_still(e.width,e.height,e.data,i,a):r.decode_filtered(e.width,e.height,e.data,i):t?r.decode_still(e.width,e.height,e.data,a):r.decode(e.width,e.height,e.data)).map(u=>({text:u.text,raw:u.raw instanceof Uint8Array?u.raw:Uint8Array.from(u.raw??[]),symbology:u.symbology,position:[0,1,2,3].map(c=>({x:u.quad[c*2]??0,y:u.quad[c*2+1]??0}))}))}const ue=`// Stage 1: luma extraction (+ optional pad to the dst grid).
// The source arrives as an rgba8unorm texture (copyExternalImageToTexture)
// at its NATIVE size; the dst grid may be larger (32-multiple snap-up).
// Source pixels are sampled 1:1 — never resampled — and the pad margin is
// written white. Linear filtering only engages when the source EXCEEDS
// maxSize and the frame must come down (downsampling is unavoidable there;
// an upsample of a marginal symbol is not).
struct Uniforms {
  dstW: u32,
  dstH: u32,
  srcW: u32,
  srcH: u32,
};

@group(0) @binding(0) var srcTex: texture_2d<f32>;
@group(0) @binding(1) var srcSamp: sampler;
@group(0) @binding(2) var<uniform> u: Uniforms;
@group(0) @binding(3) var<storage, read_write> outLuma: array<f32>;

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
  outLuma[gid.y * u.dstW + gid.x] = dot(c.rgb, vec3f(0.299, 0.587, 0.114));
}
`,ce=`// Stage 2: separable box mean (Bradley local threshold denominator).
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
`,de=`// Stage 3: Bradley adaptive binarization.
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
`,x={luma:0,blurH:256,blurV:512,binarize:768,lumaSmall:1024,globalH:1280,globalV:1536},le=7,p=64;class N{#e;#t;#i;#o;#n;#r;#c;#u;#a;#d=0;#l=0;#g=0;#f=0;#s=null;constructor(e,i){this.#e=e,this.#t=i.maxSize,this.#i=i.windowDivisor,this.#o=i.threshold;const n=e;n.addEventListener?.("uncapturederror",a=>{console.error("[WebGPU]",a.error?.message??a)}),this.#u=n.createSampler({magFilter:"linear",minFilter:"linear"});const t=a=>n.createShaderModule({code:a});this.#n=n.createComputePipeline({layout:"auto",compute:{module:t(ue),entryPoint:"main"}}),this.#r=n.createComputePipeline({layout:"auto",compute:{module:t(ce),entryPoint:"main"}}),this.#c=n.createComputePipeline({layout:"auto",compute:{module:t(de),entryPoint:"main"}})}static async create(e={}){if(!("gpu"in navigator)||!navigator.gpu)throw new Error("WebGPU unavailable. The SDK requires a WebGPU-capable browser (iOS 26+, Chrome 113+, Firefox 141+).");const i=await navigator.gpu.requestAdapter();if(!i)throw new Error("No suitable GPU adapter.");const n=await i.requestDevice();return new N(n,{maxSize:e.maxSize??1152,windowDivisor:e.windowDivisor??8,threshold:e.threshold??.85})}#_(e,i){const n=Math.min(1,this.#t/Math.max(e,i));if(n>=1){const a=o=>Math.min(this.#t,Math.max(32,Math.ceil(o/32)*32));return{w:a(e),h:a(i)}}const t=a=>Math.max(32,Math.floor(a*n/32)*32);return{w:t(e),h:t(i)}}#h(e,i){const n=this.#e,t=e*i,a=Math.ceil(t/32)*4,o=this.#s;o&&(o.uniform.destroy(),o.luma.destroy(),o.blurA.destroy(),o.blurB.destroy(),o.small.destroy(),o.smallScratch.destroy(),o.smallGlobal.destroy(),o.mask.destroy(),o.staging.destroy());const u=p*p*4;this.#s={uniform:n.createBuffer({size:256*le,usage:GPUBufferUsage.UNIFORM|GPUBufferUsage.COPY_DST}),luma:n.createBuffer({size:t*4,usage:GPUBufferUsage.STORAGE}),blurA:n.createBuffer({size:t*4,usage:GPUBufferUsage.STORAGE}),blurB:n.createBuffer({size:t*4,usage:GPUBufferUsage.STORAGE}),small:n.createBuffer({size:u,usage:GPUBufferUsage.STORAGE}),smallScratch:n.createBuffer({size:u,usage:GPUBufferUsage.STORAGE}),smallGlobal:n.createBuffer({size:u,usage:GPUBufferUsage.STORAGE}),mask:n.createBuffer({size:a,usage:GPUBufferUsage.STORAGE|GPUBufferUsage.COPY_SRC|GPUBufferUsage.COPY_DST}),staging:n.createBuffer({size:a,usage:GPUBufferUsage.MAP_READ|GPUBufferUsage.COPY_DST})},this.#d=e,this.#f=i}setThreshold(e){this.#o=e}async process(e){const i=e.videoWidth??e.width,n=e.videoHeight??e.height;if(!i||!n)throw new Error("Frame not ready.");const{w:t,h:a}=this.#_(i,n);(t!==this.#d||a!==this.#f||i!==this.#l||n!==this.#g)&&(this.#a?.destroy(),this.#a=this.#e.createTexture({size:[i,n],format:"rgba8unorm",usage:GPUTextureUsage.TEXTURE_BINDING|GPUTextureUsage.COPY_DST|GPUTextureUsage.RENDER_ATTACHMENT}),this.#l=i,this.#g=n,this.#h(t,a));const o=this.#s,u=this.#a;if(!o||!u)throw new Error("Unreachable: buffers missing.");const c=this.#e,g=Math.max(1,Math.floor(Math.min(t,a)/(2*this.#i))),f=c.queue;f.writeBuffer(o.uniform,x.luma,new Uint32Array([t,a,i,n])),f.writeBuffer(o.uniform,x.blurH,new Uint32Array([t,a,g,0])),f.writeBuffer(o.uniform,x.blurV,new Uint32Array([t,a,g,1])),f.writeBuffer(o.uniform,x.binarize,new Uint32Array([t,a,Math.round(this.#o*1e3),1e3,500,1e3,p,p])),f.writeBuffer(o.uniform,x.lumaSmall,new Uint32Array([p,p])),f.writeBuffer(o.uniform,x.globalH,new Uint32Array([p,p,p,0])),f.writeBuffer(o.uniform,x.globalV,new Uint32Array([p,p,p,1])),c.queue.copyExternalImageToTexture({source:e},{texture:u},[i,n]);const _=c.createCommandEncoder(),v=Math.ceil(t*a/32)*4;_.clearBuffer(o.mask,0,v);const y=(h,U=16)=>({buffer:o.uniform,offset:h,size:U}),m=(h,U,I=t,S=a)=>{const A=_.beginComputePass();A.setPipeline(h),A.setBindGroup(0,c.createBindGroup({layout:h.getBindGroupLayout(0),entries:U})),A.dispatchWorkgroups(Math.ceil(I/8),Math.ceil(S/8)),A.end()};m(this.#n,[{binding:0,resource:u.createView()},{binding:1,resource:this.#u},{binding:2,resource:y(x.luma)},{binding:3,resource:{buffer:o.luma}}]),m(this.#r,[{binding:0,resource:y(x.blurH)},{binding:1,resource:{buffer:o.luma}},{binding:2,resource:{buffer:o.blurA}}]),m(this.#r,[{binding:0,resource:y(x.blurV)},{binding:1,resource:{buffer:o.blurA}},{binding:2,resource:{buffer:o.blurB}}]),m(this.#n,[{binding:0,resource:u.createView()},{binding:1,resource:this.#u},{binding:2,resource:y(x.lumaSmall)},{binding:3,resource:{buffer:o.small}}],p,p),m(this.#r,[{binding:0,resource:y(x.globalH)},{binding:1,resource:{buffer:o.small}},{binding:2,resource:{buffer:o.smallScratch}}],p,p),m(this.#r,[{binding:0,resource:y(x.globalV)},{binding:1,resource:{buffer:o.smallScratch}},{binding:2,resource:{buffer:o.smallGlobal}}],p,p),m(this.#c,[{binding:0,resource:y(x.binarize,32)},{binding:1,resource:{buffer:o.luma}},{binding:2,resource:{buffer:o.blurB}},{binding:3,resource:{buffer:o.mask}},{binding:4,resource:{buffer:o.smallGlobal}}]),_.copyBufferToBuffer(o.mask,0,o.staging,0,v),c.queue.submit([_.finish()]),await o.staging.mapAsync(GPUMapMode.READ);const d=new Uint8Array(o.staging.getMappedRange().slice(0));return o.staging.unmap(),{width:t,height:a,data:d,bytesPerRow:t/8}}destroy(){this.#a?.destroy();const e=this.#s;e&&(e.luma.destroy(),e.blurA.destroy(),e.blurB.destroy(),e.small.destroy(),e.smallScratch.destroy(),e.smallGlobal.destroy(),e.mask.destroy(),e.staging.destroy(),e.uniform.destroy()),this.#e.destroy()}}class ${#e;#t;#i;#o;#n;constructor(e){this.#i=e.maxSize,this.#o=e.windowDivisor,this.#n=e.threshold,this.#e=typeof document>"u"?new OffscreenCanvas(1,1):document.createElement("canvas"),this.#t=this.#e.getContext("2d",{willReadFrequently:!0})}setThreshold(e){this.#n=e}#r(e,i){const n=Math.min(1,this.#i/Math.max(e,i));if(n>=1){const a=o=>Math.min(this.#i,Math.max(32,Math.ceil(o/32)*32));return{w:a(e),h:a(i)}}const t=a=>Math.max(32,Math.floor(a*n/32)*32);return{w:t(e),h:t(i)}}async process(e){const i=e.videoWidth??e.width,n=e.videoHeight??e.height;if(!i||!n)throw new Error("Frame not ready.");const{w:t,h:a}=this.#r(i,n);(t!==this.#e.width||a!==this.#e.height)&&(this.#e.width=t,this.#e.height=a),t>=i&&a>=n?(this.#t.fillStyle="#fff",this.#t.fillRect(0,0,t,a),this.#t.drawImage(e,0,0)):this.#t.drawImage(e,0,0,t,a);const o=this.#t.getImageData(0,0,t,a).data,u=new Float32Array(t*a);for(let d=0;d<t*a;d++)u[d]=(.299*(o[d*4]??0)+.587*(o[d*4+1]??0)+.114*(o[d*4+2]??0))/255;const c=new Float64Array((t+1)*(a+1));for(let d=0;d<a;d++)for(let h=0;h<t;h++)c[(d+1)*(t+1)+h+1]=(u[d*t+h]??0)+(c[d*(t+1)+h+1]??0)+(c[(d+1)*(t+1)+h]??0)-(c[d*(t+1)+h]??0);const g=Math.max(1,Math.floor(Math.min(t,a)/(2*this.#o))),f=Math.ceil(t/8/4)*4,_=new Uint8Array(f*a),v=this.#n;let y=0;for(let d=0;d<t*a;d++)y+=u[d]??0;const m=.5*(y/(t*a));for(let d=0;d<a;d++){const h=Math.max(0,d-g),U=Math.min(a-1,d+g),I=d*f;for(let S=0;S<t;S++){const A=Math.max(0,S-g),W=Math.min(t-1,S+g),J=(U-h+1)*(W-A+1),K=(c[(U+1)*(t+1)+W+1]??0)-(c[h*(t+1)+W+1]??0)-(c[(U+1)*(t+1)+A]??0)+(c[h*(t+1)+A]??0),C=u[d*t+S]??0,j=K/J;if(C<v*j||C<m&&C<=j+8/255){const V=I+(S>>3);_[V]=(_[V]??0)|1<<(S&7)}}}return{width:t,height:a,data:_,bytesPerRow:f}}destroy(){}}const me=Object.freeze(Object.defineProperty({__proto__:null,CpuBinarizer:$},Symbol.toStringTag,{value:"Module"}));async function pe(r={}){const e={maxSize:r.maxSize??1056,windowDivisor:r.windowDivisor??20,threshold:r.threshold??.92};if("gpu"in navigator&&navigator.gpu)try{const n=await N.create(e);return{process:t=>n.process(t),destroy:()=>n.destroy(),kind:"gpu",setThreshold:t=>n.setThreshold(t)}}catch(n){if(r.requireGpu)throw n;console.warn("GPU binarizer unavailable, falling back to CPU:",n)}else if(r.requireGpu)throw new Error("GPU binarizer required but WebGPU is unavailable in this context.");const i=new $(e);return{process:n=>i.process(n),destroy:()=>i.destroy(),kind:"cpu",setThreshold:n=>i.setThreshold(n)}}export{ge as a,he as b,pe as c,_e as d,fe as e,we as f,be as g,me as h};
