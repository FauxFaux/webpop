// Test-only host for the retained full quad-image module. Node enables Wasm
// SIMD by default. Follow the recorded standalone Emscripten initialization.
import { readFileSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import assert from 'node:assert/strict';

const binary = readFileSync(new URL('./webp-encode.wasm', import.meta.url));
assert.equal(createHash('sha256').update(binary).digest('hex'),
  'f6317d0cce217547b165a768f82647c2e923c489b5ab75b8dd0310722cffa477');
const { instance } = await WebAssembly.instantiate(binary, {
  env: { emscripten_notify_memory_growth() {} },
});
const e = instance.exports;
e.emscripten_stack_init();
e.__set_stack_limits(e.emscripten_stack_get_base(), e.emscripten_stack_get_end());
e._initialize();
assert.equal(typeof e.WebPEncodeRGBAWithNearLossless, 'function');
assert.equal(typeof e.WebPEncodeLosslessRGBA, 'function');

function encode(pixels, w, h, stride, strength) {
  const input = e.malloc(pixels.length + 1);
  const slot = e.malloc(4);
  assert(input && slot);
  let output = 0;
  try {
    new Uint8Array(e.memory.buffer).set(pixels, input + 1);
    const invoke = (strength) => {
      new DataView(e.memory.buffer).setUint32(slot, 0, true);
      const size = strength === null
        ? e.WebPEncodeLosslessRGBA(input + 1, w, h, stride, slot)
        : e.WebPEncodeRGBAWithNearLossless(input + 1, w, h, stride, strength, slot);
      output = new DataView(e.memory.buffer).getUint32(slot, true);
      assert(size > 0 && output !== 0);
      const bytes = Buffer.from(new Uint8Array(e.memory.buffer, output, size));
      e.WebPFree(output);
      output = 0;
      return bytes;
    };
    const bytes = invoke(strength);
    if (strength === 100) assert.deepEqual(bytes, invoke(null));
    for (const invalid of [-1, 101]) {
      new DataView(e.memory.buffer).setUint32(slot, 0x12345678, true);
      assert.equal(e.WebPEncodeRGBAWithNearLossless(input + 1, w, h, stride, invalid, slot), 0);
      assert.equal(new DataView(e.memory.buffer).getUint32(slot, true), 0);
    }
    assert.deepEqual(Buffer.from(new Uint8Array(e.memory.buffer, input + 1, pixels.length)), pixels);
    return bytes;
  } finally {
    if (output) e.WebPFree(output);
    e.free(slot);
    e.free(input);
  }
}
const outputs = [];
for (const line of readFileSync(process.argv[2], 'utf8').trim().split('\n')) {
  const [name, count, ...words] = line.split(/\s+/);
  assert.equal(name, 'WebPEncodeRGBAWithNearLossless');
  assert.equal(words.length, Number(count));
  const [w, h, stride, strength, ...pixels] = words.map(word => Number.parseInt(word, 16));
  const result = encode(Buffer.from(pixels), w, h, stride, strength);
  outputs.push(`${result.length} ${Array.from(result, b => b.toString(16).padStart(8, '0')).join(' ')}`);
}
writeFileSync(process.argv[3], outputs.join('\n') + '\n');
