// node rust/scripts/check-version.mjs [tag]   e.g. v1.0.0-beta.1
// One version across the workspace, the Tauri config and the frontend package.
import { readFileSync } from 'node:fs'

const here = new URL('..', import.meta.url)
const read = (p) => readFileSync(new URL(p, here), 'utf8')
const workspace = /^\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/ms.exec(read('Cargo.toml'))?.[1]
const tauri = JSON.parse(read('crates/conrod-app/tauri.conf.json')).version
const pkg = JSON.parse(read('frontend/package.json')).version
const tag = process.argv[2]?.replace(/^v/, '')
const versions = { 'Cargo.toml [workspace.package]': workspace, 'tauri.conf.json': tauri, 'frontend/package.json': pkg, ...(tag ? { tag } : {}) }
console.log(versions)
if (new Set(Object.values(versions)).size !== 1 || Object.values(versions).includes(undefined)) {
  console.error('version mismatch')
  process.exit(1)
}
