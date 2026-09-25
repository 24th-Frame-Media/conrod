// node scripts/check-version.mjs [tag]   e.g. v1.0.0-beta.1
// Cargo.toml's [workspace.package] version is the single source of truth
// (tauri.conf.json and frontend/package.json both omit "version" and fall
// back to it); this just checks it against an optional release tag.
import { readFileSync } from 'node:fs'

const here = new URL('..', import.meta.url)
const read = (p) => readFileSync(new URL(p, here), 'utf8')
const workspace = /^\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/ms.exec(read('Cargo.toml'))?.[1]
const tag = process.argv[2]?.replace(/^v/, '')
const versions = { 'Cargo.toml [workspace.package]': workspace, ...(tag ? { tag } : {}) }
console.log(versions)
if (new Set(Object.values(versions)).size !== 1 || Object.values(versions).includes(undefined)) {
  console.error('version mismatch')
  process.exit(1)
}
