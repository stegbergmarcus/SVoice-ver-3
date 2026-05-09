import React from "react";
import { createRoot } from "react-dom/client";
import ScreenClipOverlay from "./ScreenClipOverlay";
import "./ScreenClipOverlay.css";

createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ScreenClipOverlay />
  </React.StrictMode>,
);
