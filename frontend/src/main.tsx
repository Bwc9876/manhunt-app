import React from "react";
import ReactDOM from "react-dom/client";

const App = React.lazy(() => import("@/components/App"));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
        <React.Suspense>
            <App />
        </React.Suspense>
    </React.StrictMode>
);
