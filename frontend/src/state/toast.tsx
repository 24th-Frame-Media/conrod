import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from 'react';
import type { Toaster } from '../lib/types';

type Item = { id: number; message: string; tone?: 'error' | 'ok' };
const Context = createContext<Toaster>(() => {});
export const useToast = (): Toaster => useContext(Context);

/** Bottom-centre pills, like the Python #toast; a repeated message replaces itself instead of stacking. */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<Item[]>([]);
  const counter = useRef(0);
  const dismiss = useCallback((id: number) => setItems((list) => list.filter((t) => t.id !== id)), []);
  const toast = useCallback<Toaster>((message, { tone, ms = tone === 'error' ? 6500 : 3200 } = {}) => {
    const id = ++counter.current;
    setItems((list) => [...list.filter((t) => t.message !== message).slice(-2), { id, message, tone }]);
    window.setTimeout(() => dismiss(id), ms);
  }, [dismiss]);
  return (
    <Context value={toast}>
      {children}
      <div className="toasts" role="status" aria-live="polite">
        {items.map((t) => (
          <button key={t.id} className={`toast ${t.tone ?? ''}`} onClick={() => dismiss(t.id)}>{t.message}</button>
        ))}
      </div>
    </Context>
  );
}
