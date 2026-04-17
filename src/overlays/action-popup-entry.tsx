import React from "react";
import ReactDOM from "react-dom/client";
import ActionPopup from "../windows/ActionPopup";
import "../theme.css";

ReactDOM.createRoot(document.getElementById("action-popup-root")!).render(
  <React.StrictMode>
    <ActionPopup />
  </React.StrictMode>
);
