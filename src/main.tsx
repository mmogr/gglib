import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./styles/tailwind.css";
import { initAppLogger } from "./services/platform/logging/appLogger";
import { takeDaemonToken } from "./services/transport/api/daemonToken";

// Initialize unified logging system
initAppLogger();
// Out of the address bar before anything renders; see daemonToken.ts.
takeDaemonToken();

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
