import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./app/App";
import { desktopApi } from "./lib/api";
import "./styles/tokens.css";
import "./styles/app.css";

const testApi = import.meta.env.DEV && new URLSearchParams(window.location.search).has("testCatalog")
  ? (await import("./test/browserMock")).browserMockApi
  : desktopApi;

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App api={testApi} />
  </StrictMode>,
);
