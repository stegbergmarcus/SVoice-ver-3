import React from "react";
import ReactDOM from "react-dom/client";
import Palette from "./Palette";
import "../theme.css";

ReactDOM.createRoot(document.getElementById("palette-root")!).render(
  <React.StrictMode>
    <Palette />
  </React.StrictMode>,
);
