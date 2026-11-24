// node scripts/check-version.mjs [tag]   e.g. v1.0.0-beta.1
// Cargo.toml's [workspace.package] version is the single source of truth.
// tauri.conf.json, frontend/package.json and every crate normally omit their own
// version and fall back to it (crates use `version.workspace = true`). This fails
// if any of them sets a version that differs, or if the optional release tag
// differs from Cargo.toml.
import { readFileSync, readdirSync, existsSync } from 'node:fs'

const here = new URL('..', import.meta.url)
const read = (p) => readFileSync(new URL(p, here), 'utf8')
const workspace = /^\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/ms.exec(read('Cargo.toml'))?.[1]
const tag = process.argv[2]?.replace(/^v/, '')

const versions = { 'Cargo.toml [workspace.package]': workspace }
if (tag) versions.tag = tag

// A file that sets its own version must match; one that omits it inherits.
for (const file of ['crates/conrod-app/tauri.conf.json', 'frontend/package.json']) {
  const own = JSON.parse(read(file)).version
  if (own !== undefined) versions[file] = own
}
for (const dir of readdirSync(new URL('crates/', here))) {
  const manifest = `crates/${dir}/Cargo.toml`
  if (!existsSync(new URL(manifest, here))) continue
  const section = /^\[package\](.*?)(?=^\[|(?![\s\S]))/ms.exec(read(manifest))?.[1] ?? ''
  const own = /^version\s*=\s*"([^"]+)"/m.exec(section)?.[1]
  const inherits = /^version\.workspace\s*=\s*true/m.test(section)
  if (own !== undefined) versions[manifest] = own
  else if (!inherits) {
    console.error(`${manifest}: no version and not version.workspace = true`)
    process.exit(1)
  }
}

console.log(versions)
if (new Set(Object.values(versions)).size !== 1 || Object.values(versions).includes(undefined)) {
  console.error('version mismatch')
  process.exit(1)
}
