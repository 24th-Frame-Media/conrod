import { useState } from 'react';
import { call } from '../lib/api';
import { Confirm } from './basics';
import { useRun } from '../state/hooks';
export function Maintenance() {
  const run = useRun();
  const [update, setUpdate] = useState<{current: string; latest?: string; newer?: boolean; installable?: boolean; error?: string; notes?: string} | null>(null);
  const [cache, setCache] = useState<{total: {files: number; bytes: number}} | null>(null);
  const [health, setHealth] = useState<{name: string; ready: boolean; detail?: string}[]>([]);
  const [clear, setClear] = useState<string | null>(null);
  return <section className="setting-group"><h3>Maintenance</h3><div className="actions-row">
    <button onClick={() => void run(async () => setHealth(await call('health')))}>Check setup</button>
    <button onClick={() => void run(async () => { await call('install_models'); })}>Install missing models</button>
    <button onClick={() => void run(async () => setUpdate(await call('check_update')))}>Check for updates</button>
    <button onClick={() => void run(async () => setCache(await call('cache_info')))}>Cache usage</button>
    <button onClick={() => setClear('orphaned')}>Clear unused cache</button><button onClick={() => setClear('previews')}>Clear previews</button>
    <button className="danger" onClick={() => setClear('all')}>Reset library</button>
  </div>{cache && <p>{cache.total.files} cached files · {(cache.total.bytes / 1048576).toFixed(1)} MB</p>}
  {health.map(h => <p key={h.name}>{h.ready ? 'Ready' : 'Missing'} — {h.name}: {h.detail ?? (h.ready ? 'Available' : 'Not installed')}</p>)}
  {update && <div><p>{update.error ?? (update.newer ? `Update available: ${update.latest}` : `Up to date: ${update.current}`)}</p><p>{update.notes}</p>{update.newer && update.installable && <button onClick={() => void run(async () => { await call('install_update'); })}>Update and restart</button>}</div>}
  {clear && <Confirm title={clear === 'all' ? 'Reset the library?' : 'Clear cached files?'} action="Clear" onCancel={() => setClear(null)} onConfirm={() => { void run(async () => { await call(clear === 'all' ? 'reset_all' : 'cache_clear', {[clear]: true}); setCache(await call('cache_info')); }); setClear(null); }}><p>{clear === 'all' ? 'All albums and their results will be removed. Known vehicles, training and original photographs are kept.' : 'Original photographs are kept. Previews can be recreated when opened.'}</p></Confirm>}
  </section>;
}
