import React from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { applyTheme, storedTheme } from './lib/theme.mjs';
import { ToastProvider } from './state/toast';
import './styles/base.css';
import './styles/screens.css';

// The dev mock can open straight into the light theme: `?mock=1&theme=light`.
const asked = import.meta.env.DEV ? new URLSearchParams(location.search).get('theme') : null;
applyTheme(asked ?? storedTheme());

createRoot(document.getElementById('root')!).render(
  <React.StrictMode><ToastProvider><App /></ToastProvider></React.StrictMode>,
);
