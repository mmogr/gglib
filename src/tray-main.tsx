/**
 * Entry point for the tray panel window.
 *
 * A separate Vite entry rather than a route on the main app: the panel needs
 * none of the model library, chat or council code, and loading the full app
 * shell into a 360px popover would make opening it feel slow. `App`'s
 * providers are deliberately absent — the panel talks to the same HTTP API
 * through the shared transport and needs no global state.
 */

import React from 'react';
import ReactDOM from 'react-dom/client';
import { TrayPanel } from './pages/TrayPanel';
import './styles/tailwind.css';
import { initAppLogger } from './services/platform/logging/appLogger';

initAppLogger();

ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <React.StrictMode>
    <TrayPanel />
  </React.StrictMode>,
);
