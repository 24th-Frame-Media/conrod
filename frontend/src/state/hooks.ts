import { useCallback, useEffect, useRef, useState } from 'react';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { engine, inTauri } from '../lib/api';
import { useToast } from './toast';

export type Runner = <T>(work: () => Promise<T>) => Promise<T | undefined>;

/** Runs engine work; a failure becomes an error toast instead of an unhandled rejection. */
export function useRun(): Runner {
  const toast = useToast();
  return useCallback(async <T,>(work: () => Promise<T>) => {
    try { return await work(); } catch (e) { toast(String(e), { tone: 'error' }); return undefined; }
  }, [toast]);
}

/** A boolean remembered per viewer; storage can be blocked, so it degrades to plain state. */
export function useLocalFlag(key: string, initial: boolean): [boolean, (v: boolean | ((p: boolean) => boolean)) => void] {
  const [value, setValue] = useState<boolean>(() => {
    try { const raw = localStorage.getItem(key); return raw == null ? initial : raw === '1'; } catch { return initial; }
  });
  useEffect(() => { try { localStorage.setItem(key, value ? '1' : '0'); } catch { /* not persisted */ } }, [key, value]);
  return [value, setValue];
}

/** The full-size preview path of one frame, or null while it is being pulled. */
export function usePreview(imageId: number | null, run: Runner): string | null {
  const [path, setPath] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    setPath(null);
    if (imageId != null) void run(async () => { const p = await engine.preview(imageId); if (live) setPath(p); });
    return () => { live = false; };
  }, [imageId, run]);
  return path;
}

const PHOTO = /\.(cr2|cr3|crw|nef|arw|dng|raf|orf|rw2|jpe?g|png|tiff?|heic)$/i;

/** True while a file drag is over the window; calls onDrop with the folder that was dropped. */
export function useFolderDrop(onDrop: (folder: string) => void): boolean {
  const [over, setOver] = useState(false);
  const latest = useRef(onDrop);
  useEffect(() => { latest.current = onDrop; });
  useEffect(() => {
    if (!inTauri) return;
    let off: (() => void) | undefined;
    let dead = false;
    void getCurrentWebview().onDragDropEvent((event) => {
      const p = event.payload;
      if (p.type === 'enter' || p.type === 'over') setOver(true);
      else if (p.type === 'leave') setOver(false);
      else {
        setOver(false);
        const path = p.paths[0];
        // A photo dropped instead of its folder means "that folder".
        if (path) latest.current(PHOTO.test(path) ? path.replace(/[\\/][^\\/]+$/, '') : path);
      }
    }).then((u) => { if (dead) u(); else off = u; });
    return () => { dead = true; off?.(); };
  }, []);
  return over;
}
