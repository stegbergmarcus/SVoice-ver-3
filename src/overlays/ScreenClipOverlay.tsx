import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

type ScreenClipOpenPayload = {
  x: number;
  y: number;
  width: number;
  height: number;
};

type Drag = {
  startX: number;
  startY: number;
  endX: number;
  endY: number;
};

function rectFromDrag(drag: Drag) {
  const left = Math.min(drag.startX, drag.endX);
  const top = Math.min(drag.startY, drag.endY);
  const width = Math.abs(drag.endX - drag.startX);
  const height = Math.abs(drag.endY - drag.startY);
  return { left, top, width, height };
}

export default function ScreenClipOverlay() {
  const [monitor, setMonitor] = useState<ScreenClipOpenPayload | null>(null);
  const [drag, setDrag] = useState<Drag | null>(null);
  const draggingRef = useRef(false);

  useEffect(() => {
    const unlisten = listen<ScreenClipOpenPayload>("screen_clip_open", (ev) => {
      setMonitor(ev.payload);
      setDrag(null);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    const onKeyDown = async (ev: KeyboardEvent) => {
      if (ev.key !== "Escape") return;
      ev.preventDefault();
      setDrag(null);
      await invoke("screen_clip_cancel");
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  async function commit(next: Drag) {
    if (!monitor) return;
    const rect = rectFromDrag(next);
    if (rect.width < 8 || rect.height < 8) {
      setDrag(null);
      return;
    }
    await invoke("screen_clip_commit", {
      selection: {
        startX: next.startX,
        startY: next.startY,
        endX: next.endX,
        endY: next.endY,
        scaleFactor: window.devicePixelRatio || 1,
        originX: monitor.x,
        originY: monitor.y,
      },
    });
    setDrag(null);
  }

  return (
    <main
      className="screen-clip-root"
      onContextMenu={async (ev) => {
        ev.preventDefault();
        await invoke("screen_clip_cancel");
      }}
      onMouseDown={(ev) => {
        if (ev.button !== 0) return;
        draggingRef.current = true;
        const next = {
          startX: ev.clientX,
          startY: ev.clientY,
          endX: ev.clientX,
          endY: ev.clientY,
        };
        setDrag(next);
      }}
      onMouseMove={(ev) => {
        if (!draggingRef.current || !drag) return;
        setDrag({ ...drag, endX: ev.clientX, endY: ev.clientY });
      }}
      onMouseUp={async (ev) => {
        if (!draggingRef.current || !drag) return;
        draggingRef.current = false;
        await commit({ ...drag, endX: ev.clientX, endY: ev.clientY });
      }}
    >
      <div className="screen-clip-hint">Dra runt området som ska skickas till AI</div>
      {drag && (
        <div
          className="screen-clip-selection"
          style={{
            left: rectFromDrag(drag).left,
            top: rectFromDrag(drag).top,
            width: rectFromDrag(drag).width,
            height: rectFromDrag(drag).height,
          }}
        />
      )}
    </main>
  );
}
