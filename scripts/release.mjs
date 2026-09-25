// node scripts/release.mjs --beta|--patch|--minor|--major|--set X.Y.Z [--tag]
// node scripts/release.mjs --selftest
//
// Bumps the workspace version (the single source of truth, see
// check-version.mjs), refreshes Cargo.lock, and re-checks. With --tag,
// commits and tags the release (never pushes).
import { readFileSync, writeFileSync } from 'node:fs'
import { execFileSync } from 'node:child_process'

const here = new URL('..', import.meta.url)
const path = (p) => new URL(p, here)
const read = (p) => readFileSync(path(p), 'utf8')

function parse(version) {
  const m = /^(\d+)\.(\d+)\.(\d+)(?:-(.+))?$/.exec(version)
  if (!m) throw new Error(`not a version: ${version}`)
  return { major: +m[1], minor: +m[2], patch: +m[3], pre: m[4] ?? null }
}

const fmt = ({ major, minor, patch, pre }) => `${major}.${minor}.${patch}${pre ? `-${pre}` : ''}`

// The core bump logic, pure so --selftest can exercise it without touching the repo.
export function bump(version, kind) {
  const v = parse(version)
  if (kind === 'beta') {
    const betaN = /^beta\.(\d+)$/.exec(v.pre ?? '')
    if (betaN) return fmt({ ...v, pre: `beta.${+betaN[1] + 1}` })
    return fmt({ ...v, patch: v.patch + 1, pre: 'beta.1' })
  }
  if (kind === 'patch' || kind === 'minor' || kind === 'major') {
    // A prerelease is released as-is: dropping "-beta.N" IS the bump.
    if (v.pre) return fmt({ ...v, pre: null })
    if (kind === 'patch') return fmt({ ...v, patch: v.patch + 1 })
    if (kind === 'minor') return fmt({ ...v, minor: v.minor + 1, patch: 0 })
    return fmt({ ...v, major: v.major + 1, minor: 0, patch: 0 })
  }
  throw new Error(`unknown bump kind: ${kind}`)
}

function selftest() {
  const cases = [
    [['1.0.0-beta.5', 'beta'], '1.0.0-beta.6'],
    [['1.0.0', 'beta'], '1.0.1-beta.1'],
    [['1.0.0-beta.6', 'patch'], '1.0.0'],
    [['1.0.0-beta.6', 'minor'], '1.0.0'],
    [['1.0.0-beta.6', 'major'], '1.0.0'],
    [['1.0.0', 'patch'], '1.0.1'],
    [['1.0.0', 'minor'], '1.1.0'],
    [['1.2.3', 'major'], '2.0.0'],
  ]
  for (const [args, want] of cases) {
    const got = bump(...args)
    if (got !== want) throw new Error(`bump(${args.join(', ')}) = ${got}, want ${want}`)
  }
  console.log(`selftest ok (${cases.length} cases)`)
}

function currentVersion() {
  return /^\[workspace\.package\][^[]*?^version\s*=\s*"([^"]+)"/ms.exec(read('Cargo.toml'))?.[1]
}

function writeVersion(next) {
  const cargoToml = read('Cargo.toml')
  const updated = cargoToml.replace(
    /(^\[workspace\.package\][^[]*?^version\s*=\s*")[^"]+(")/ms,
    `$1${next}$2`
  )
  if (updated === cargoToml) throw new Error('could not find [workspace.package] version in Cargo.toml')
  writeFileSync(path('Cargo.toml'), updated)
}

function main() {
  const args = process.argv.slice(2)
  if (args.includes('--selftest')) return selftest()

  const tag = args.includes('--tag')
  const setIdx = args.indexOf('--set')
  const kind = ['beta', 'patch', 'minor', 'major'].find((k) => args.includes(`--${k}`))

  if (tag) {
    const dirty = execFileSync('git', ['status', '--porcelain'], { cwd: here, encoding: 'utf8' })
    if (dirty.trim()) {
      console.error('working tree is dirty; commit or stash before --tag')
      process.exit(1)
    }
  }

  const current = currentVersion()
  if (!current) throw new Error('could not read current workspace version')

  let next
  if (setIdx !== -1) {
    next = args[setIdx + 1]
    if (!next) throw new Error('--set requires a version, e.g. --set 1.0.0')
    parse(next) // validate
  } else if (kind) {
    next = bump(current, kind)
  } else {
    console.error('usage: node scripts/release.mjs --beta|--patch|--minor|--major|--set X.Y.Z [--tag]')
    process.exit(1)
  }

  console.log(`${current} -> ${next}`)
  writeVersion(next)
  execFileSync('cargo', ['update', '-w'], { cwd: here, stdio: 'inherit' })
  execFileSync('node', ['scripts/check-version.mjs'], { cwd: here, stdio: 'inherit' })

  if (tag) {
    execFileSync('git', ['commit', '-am', `chore: release v${next}`], { cwd: here, stdio: 'inherit' })
    execFileSync('git', ['tag', `v${next}`], { cwd: here, stdio: 'inherit' })
    console.log(`\nTagged v${next}. To publish: git push origin main v${next}`)
  }
}

main()
