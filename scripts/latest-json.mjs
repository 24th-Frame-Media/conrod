// Build the Tauri updater manifest for one Windows NSIS release.
//
// Tauri looks up `{os}-{arch}-{installer}` first and `{os}-{arch}` second, so both
// keys carry the same signature: newer clients pick the NSIS one, older ones the
// plain target. The signature is minisign over the installer bytes, so renaming
// the installer for humans is fine.
//
// Usage: node scripts/latest-json.mjs <version> <installer.sig> <installer-url> <out.json> [notes]
import { readFileSync, writeFileSync } from 'node:fs';

const [, , version, sigPath, url, out, notes] = process.argv;
if (!version || !sigPath || !url || !out) {
  console.error('usage: node scripts/latest-json.mjs <version> <sig> <url> <out.json> [notes]');
  process.exit(2);
}
const signature = readFileSync(sigPath, 'utf8').trim();
if (!signature) {
  console.error(`${sigPath} is empty: the build was not signed`);
  process.exit(1);
}
const platform = { signature, url };
const manifest = {
  version,
  notes: notes || 'See the release notes on GitHub.',
  pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, 'Z'),
  platforms: {
    'windows-x86_64-nsis': platform,
    'windows-x86_64': platform,
  },
};
writeFileSync(out, `${JSON.stringify(manifest, null, 2)}\n`);
console.log(`wrote ${out} for ${version}`);
