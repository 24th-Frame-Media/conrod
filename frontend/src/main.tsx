import React from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import { ToastProvider } from './state/toast';
import './styles/base.css';
import './styles/screens.css';

createRoot(document.getElementById('root')!).render(
  <React.StrictMode><ToastProvider><App /></ToastProvider></React.StrictMode>,
);
