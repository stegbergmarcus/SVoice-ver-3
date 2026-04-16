import React from "react";
import ReactDOM from "react-dom/client";
import RecordingIndicator from "./RecordingIndicator";

ReactDOM.createRoot(document.getElementById("overlay-root")!).render(
  <React.StrictMode>
    <RecordingIndicator />
  </React.StrictMode>
);
