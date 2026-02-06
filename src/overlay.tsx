import React from "react";
import ReactDOM from "react-dom/client";
import DictationOverlay from "./components/DictationOverlay";
import "./styles/index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <DictationOverlay />
  </React.StrictMode>
);
