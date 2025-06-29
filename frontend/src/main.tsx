import React from "react";
import ReactDOM from "react-dom/client";

const App = React.lazy(() => import("@/components/App"));

import { warn, debug, trace, info, error } from "@tauri-apps/plugin-log";

function forwardConsole(
    fnName: "log" | "debug" | "info" | "warn" | "error",
    logger: (message: string) => Promise<void>
) {
    const original = console[fnName];
    console[fnName] = (message) => {
        original(message);
        logger(message);
    };
}

forwardConsole("log", trace);
forwardConsole("debug", debug);
forwardConsole("info", info);
forwardConsole("warn", warn);
forwardConsole("error", error);

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
        <React.Suspense>
            <App />
        </React.Suspense>
    </React.StrictMode>
);
