import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { brotliCompressSync, constants as compression } from 'node:zlib';

const [candidatePath, baselinePath] = process.argv.slice(2);
assert(candidatePath, 'usage: node measure.mjs candidate.wasm [packed-baseline.wasm]');
const profile = JSON.parse(await readFile(new URL('profile.json', import.meta.url)));
const binaries = { candidate: await readFile(candidatePath) };
if (baselinePath) binaries.baseline = await readFile(baselinePath);

const decoder = new TextDecoder();
const encoder = new TextEncoder();
const elapsed = (start) => performance.now() - start;
const median = (samples) => [...samples].sort((a, b) => a - b)[Math.floor(samples.length / 2)];

async function initialize(bytes) {
  const start = performance.now();
  const { instance } = await WebAssembly.instantiate(bytes);
  const moduleMs = elapsed(start);
  const { input, output, memory, init, run } = instance.exports;
  const read = (len) => JSON.parse(decoder.decode(new Uint8Array(memory.buffer, output(), len)));
  const initStart = performance.now();
  const metadata = read(init());
  const initMs = elapsed(initStart);
  assert(!metadata.error, metadata.error);
  return {
    metadata, moduleMs, initMs,
    memoryBytes: () => memory.buffer.byteLength,
    request(command) {
      const bytes = encoder.encode(JSON.stringify(command));
      const address = input(bytes.length);
      assert(address > 0, 'request exceeds module input bound');
      new Uint8Array(memory.buffer, address, bytes.length).set(bytes);
      return read(run());
    },
  };
}

function findKind(value, kind) {
  if (!value || typeof value !== 'object') return undefined;
  if (value.kind === kind) return value;
  for (const item of Object.values(value)) {
    const found = findKind(item, kind);
    if (found) return found;
  }
}

function exercise(engine, label) {
  const inputs = [...profile.cases, {
    name: 'Guardian', types: ['Creature'],
    text: 'When Guardian enters, create a Treasure token if an opponent controls more creatures than you. If an opponent has more life than you, you gain 12 life. If an opponent has more cards in hand than you, draw a card.',
  }];
  const cases = inputs.map(({ name, types, text }) => {
    const card = { name, types };
    const parsed = engine.request({ ...card, op: 'parse', text });
    assert(parsed.readings?.length > 0, JSON.stringify(parsed));
    for (const reading of parsed.readings) {
      const printed = engine.request({ ...card, op: 'print', reading });
      assert(printed.text, JSON.stringify(printed));
      const reparse = engine.request({ ...card, op: 'parse', text: printed.text });
      assert(reparse.readings.some((value) => JSON.stringify(value) === JSON.stringify(reading)));
    }
    return { card, text, readings: parsed.readings };
  });

  const be = cases[0];
  const edited = structuredClone(be.readings[0]);
  const gain = findKind(edited, 'GainLife');
  assert.equal(gain.args.amount.args, 4);
  gain.args.amount.args = 9;
  const changed = engine.request({ ...be.card, op: 'print', reading: edited });
  assert(changed.text?.includes('9 life'), JSON.stringify(changed));
  assert(engine.request({ ...be.card, op: 'parse', text: `${be.text} Unrecognized trailing words.` }).error);
  assert(engine.request({ ...be.card, op: 'parse', text: 'When this creature enters, its controller wins the game.' }).error || label === 'baseline');

  const proof = label === 'candidate'
    ? engine.request({ ...be.card, op: 'verify', text: changed.text, reading: edited }).verified
    : engine.request({ ...be.card, op: 'parse', text: changed.text }).readings
      .some((value) => JSON.stringify(value) === JSON.stringify(edited));
  assert(proof, 'edited reading failed verification');
  return { cases, edited, editedText: changed.text };
}

function warm(engine, label, { cases, edited, editedText }) {
  const phase = (items, operation) => {
    const start = performance.now();
    for (let round = 0; round < 25; round += 1) {
      for (const item of items) operation(item);
    }
    return elapsed(start) / (25 * items.length);
  };
  const parseMs = phase(cases, ({ card, text }) => {
    assert(engine.request({ ...card, op: 'parse', text }).readings?.length);
  });
  const print = ({ card, readings }) => {
    assert(engine.request({ ...card, op: 'print', reading: readings[0] }).text);
  };
  const printVerifiedMs = phase(cases, print);
  const bezaPrintVerifiedMs = phase(cases.slice(0, 1), print);
  const verifyMs = phase(cases.slice(0, 1), ({ card }) => {
    if (label === 'candidate') {
      assert(engine.request({ ...card, op: 'verify', text: editedText, reading: edited }).verified);
    } else {
      assert(engine.request({ ...card, op: 'parse', text: editedText }).readings
        .some((value) => JSON.stringify(value) === JSON.stringify(edited)));
    }
  });
  return { parseMs, printVerifiedMs, bezaPrintVerifiedMs, verifyMs };
}

const samples = Object.fromEntries(Object.keys(binaries).map((label) => [label, []]));
for (let round = 0; round < 5; round += 1) {
  const order = Object.keys(binaries);
  if (round % 2) order.reverse();
  const parity = {};
  for (const label of order) {
    const engine = await initialize(binaries[label]);
    const coldBytes = engine.memoryBytes();
    const result = exercise(engine, label);
    parity[label] = result.cases.map(({ readings }) => readings);
    samples[label].push({
      moduleMs: engine.moduleMs, initMs: engine.initMs, coldBytes,
      ...warm(engine, label, result), warmBytes: engine.memoryBytes(),
      rules: engine.metadata.rules,
    });
  }
  if (parity.baseline) assert.deepEqual(parity.candidate, parity.baseline);
}

for (const [label, bytes] of Object.entries(binaries)) {
  const size = brotliCompressSync(bytes, {
    params: { [compression.BROTLI_PARAM_QUALITY]: 9 },
  }).byteLength;
  const rows = samples[label];
  console.log(JSON.stringify({
    label, rawBytes: bytes.length, brotliBytes: size,
    medians: Object.fromEntries(Object.keys(rows[0]).map((key) => [key, median(rows.map((row) => row[key]))])),
    samples: rows,
  }));
}
