import React from "react";
import ReactDOM from "react-dom/client";

import App from "./App";

import "./styles/theme.css";
import "./styles/app.css";

const container = document.getElementById("root");

if (!container) {
  throw new Error("root container is missing from the document");
}

ReactDOM.createRoot(container).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
