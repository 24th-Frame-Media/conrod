import test from 'node:test';
import assert from 'node:assert/strict';
import { fileURLToPath } from 'node:url';
import { dirname, join, relative } from 'node:path';
import { readFileSync, readdirSync } from 'node:fs';

// §6.2 / §9: no hardcoded car / motorsport / vehicle UI WORDING anywhere.
// Everything the shoot is called derives from the resolved settings via
// subjects.mjs. This test walks the UI source (screens, components, App) and
// fails on any NEW user-facing string that hardcodes a subject literal.
//
// Data modules are whitelisted by design (plan §9): detector class names,
// model file names, CSS classes, preset ids and engine vocabulary legitimately
// contain "vehicle"/"car". The scanner only looks at STRING LITERALS and prose
// JSX TEXT, strips comments, skips identifier-like strings (lowercase data ids,
// hyphenated class names) and an explicit allowlist of feature / registry /
// model names. A new line like "Reading cars" or "3 cars read" matches none of
// those and fails.

const HERE = dirname(fileURLToPath(import.meta.url));
const SRC = join(HERE, '..', 'src');

const BANNED = /\b(cars?|motorsport|vehicles?)\b/i;
const BANNED_G = /\b(cars?|motorsport|vehicles?)\b/gi;
const SINGLE_IDENT = /^[a-z0-9._/+&#:-]+$/;          // a preset id / detector category / kind tag
const IDENT_TOKEN = /^[a-z0-9._/+&#:()-]+$/;         // one CSS-class-like token
const JSX_PROSE = /^[A-Za-z0-9 ,.'’…·—–:!%+\-]+$/;    // prose text node, not a code expression

/** Legitimate feature / registry / detector / model names (exact phrases).
 *  A hardcoded shoot-subject literal never matches one of these. */
const ALLOWED = [
  /\bknown vehicles?\b/i,
  /\banother vehicle\b/i,
  /\bof vehicles\b/i,
  /\bvehicle (data|metadata|details|registry|stacks|detector|detections|competition|licence|identification|plates)\b/i,
  /\bvehicle detector \(yolo\)/i,
  /\bby vehicle\b/i,
  /\bre-group vehicles\b/i,
  /\bmerge vehicles\b/i,
  /\bmerge with another vehicle\b/i,
  /\bmerge \d+ selected vehicles\b/i,
  /\bgroup vehicles\b/i,
  /\bidentical vehicles\b/i,
  /\bmatching vehicles\b/i,
  /\bvehicles re-grouped\b/i,
  /\bshow vehicle detections\b/i,
  /\bsettings and vehicles\b/i,
  /\bimported\s+vehicles\b/i,
  /\bremove\s+vehicles\b/i,
  /\bsearch vehicles\b/i,
  /\badd vehicle\b/i,
  /\bvehicle identifications\b/i,
  /\bvehicle plates\b/i,
  /^\s*vehicles\s*$/i, // the detector-category tab label
];

/** True for DATA identifiers, not wording: a lowercase single token ('motorsport',
 *  'vehicle', 'car'), or a CSS-class-like list where every banned word is part
 *  of a hyphenated token ('viewer-vehicle', 'tab-vehicles', 'cars-people'). */
function dataOnly(text) {
  if (SINGLE_IDENT.test(text)) return true;
  const tokens = text.split(/\s+/);
  if (!tokens.every((t) => IDENT_TOKEN.test(t))) return false;
  let m;
  BANNED_G.lastIndex = 0;
  while ((m = BANNED_G.exec(text))) {
    const before = text[m.index - 1] ?? '', after = text[m.index + m[0].length] ?? '';
    if (before !== '-' && after !== '-') return false; // a standalone word in prose
  }
  return true;
}

/** Replace comments with spaces (keep newlines for line numbers); leave
 *  string literals intact so they can be extracted next. */
function stripComments(src) {
  let out = '', i = 0, state = 'code', delim = '';
  while (i < src.length) {
    const c = src[i], d = src[i + 1] ?? '';
    if (state === 'code') {
      if (c === '/' && d === '/') { state = 'line'; out += '  '; i += 2; continue; }
      if (c === '/' && d === '*') { state = 'block'; out += '  '; i += 2; continue; }
      if (c === "'" || c === '"' || c === '`') { state = 'str'; delim = c; out += c; i += 1; continue; }
      out += c; i += 1; continue;
    }
    if (state === 'line') { if (c === '\n') { state = 'code'; out += c; } else out += ' '; i += 1; continue; }
    if (state === 'block') {
      if (c === '*' && d === '/') { state = 'code'; out += '  '; i += 2; continue; }
      out += c === '\n' ? '\n' : ' '; i += 1; continue;
    }
    if (c === '\\') { out += c + d; i += 2; continue; }
    if (c === delim) { state = 'code'; out += c; i += 1; continue; }
    out += c; i += 1;
  }
  return out;
}

const clean = (text) => text.replace(/\$?\{[^}]*\}/g, ' ').replace(/\s+/g, ' ').trim();

/** User-facing strings in one source file: quoted/template literals + prose JSX text. */
function uiStrings(src) {
  const stripped = stripComments(src);
  const strRe = /(['"`])((?:\\.|(?!\1)[^\\])*)\1/gs;
  const out = [];
  let m;
  while ((m = strRe.exec(stripped))) out.push({ text: m[2], index: m.index });
  // Mask string literals, then read prose JSX text between > and <.
  let masked = '', last = 0;
  const maskRe = /(['"`])((?:\\.|(?!\1)[^\\])*)\1/gs;
  let mm;
  while ((mm = maskRe.exec(stripped))) {
    masked += stripped.slice(last, mm.index) + mm[0].replace(/[^\n]/g, ' ');
    last = mm.index + mm[0].length;
  }
  masked += stripped.slice(last);
  const jsxRe = />\s*([^<>]+?)\s*</g;
  let j;
  while ((j = jsxRe.exec(masked))) {
    const t = clean(j[1]);
    if (t && /[A-Za-z]/.test(t) && JSX_PROSE.test(t)) out.push({ text: j[1], index: j.index });
  }
  return { stripped, out };
}

function violations(src, file) {
  const { stripped, out } = uiStrings(src);
  const bad = [];
  for (const c of out) {
    const text = clean(c.text);
    if (!text || dataOnly(text) || ALLOWED.some((re) => re.test(text))) continue;
    if (BANNED.test(text)) {
      const line = stripped.slice(0, c.index).split('\n').length;
      bad.push(`${relative(SRC, file)}:${line} → ${JSON.stringify(text.slice(0, 120))}`);
    }
  }
  return bad;
}

function uiFiles() {
  const files = [];
  for (const dir of ['screens', 'components']) {
    for (const name of readdirSync(join(SRC, dir))) if (name.endsWith('.tsx')) files.push(join(SRC, dir, name));
  }
  files.push(join(SRC, 'App.tsx'));
  // The dev mock's decision questions are shown verbatim in the DECIDER lane
  // (`?mock=1`), so its wording follows the same rule as the real screens (§6.2:
  // the sweep covers mock data). Other data modules stay whitelisted.
  files.push(join(SRC, 'lib', 'mockDecider.ts'));
  return files;
}

test('no hardcoded car/motorsport/vehicle wording in UI strings (data ids whitelisted)', () => {
  const bad = [];
  for (const file of uiFiles()) bad.push(...violations(readFileSync(file, 'utf8'), file));
  assert.deepEqual(bad, [], `hardcoded subject literals must derive from subjects.mjs:\n${bad.join('\n')}`);
});

test('the scanner catches a NEW hardcoded subject literal but allows data identifiers', () => {
  const bad = violations([
    "const a = 'Reading cars now';",             // subject wording → must fail
    'const b = <b>Cars already read</b>;',      // JSX prose → must fail
    "const c = `${done} cars read`;",           // template prose → must fail
    "const d = 'motorsport';",                  // preset id → allowed
    "const e = 'vehicle';",                     // detector category → allowed
    "const f = 'viewer-vehicle-number mono';",  // CSS classes → allowed
    "const g = 'Known vehicles';",              // registry feature name → allowed
    '// a comment about cars',                  // comment → ignored
    "const h = 'Reading ' + subject.many;",    // derived, no literal → allowed
  ].join('\n'), 'synthetic.tsx');
  assert.equal(bad.length, 3, `only the three subject literals fail:\n${bad.join('\n')}`);
  const joined = bad.join('\n');
  assert.match(joined, /Reading cars now/);
  assert.match(joined, /Cars already read/);
  assert.match(joined, /cars read/);
});

test('derived wording (subjects.mjs) is the rule, not a hardcoded word', () => {
  const subjects = readFileSync(join(SRC, 'lib', 'subjects.mjs'), 'utf8');
  assert.match(subjects, /subjectWords|SUBJECT_WORDS/, 'subjects.mjs is the one subject-wording source');
  const ingest = readFileSync(join(SRC, 'screens', 'Ingest.tsx'), 'utf8');
  assert.match(ingest, /subjectWords\(/, 'the ingest screen derives its words');
  // A rodeo run never claims a vehicle/car in its flowchart wording.
  assert.doesNotMatch(readFileSync(join(SRC, 'lib', 'surface.mjs'), 'utf8'), /'Reading cars'|'cars read'|'Recent cars'/);
});
