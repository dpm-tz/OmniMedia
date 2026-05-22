import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

const hash = window.location.hash.replace(/^#/, "");
if (hash === "overlay-canvas") {
  document.body.setAttribute("data-overlay", "canvas");
} else if (hash === "overlay-toolbar") {
  document.body.setAttribute("data-overlay", "toolbar");
} else if (hash === "webcam-overlay") {
  document.body.setAttribute("data-overlay", "webcam");
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
