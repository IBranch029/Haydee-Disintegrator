import { useCallback, useEffect, useState } from "react";

import { getCurrentWindow } from "@tauri-apps/api/window";

interface TitleBarProps {
  /** Contextual title rendered in the middle of the bar. */
  context?: string;
}

/** Minimal, perfectly balanced glyphs for the window controls. */
function MinimizeGlyph() {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
      <path
        d="M3.2 7h7.6"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  );
}

function MaximizeGlyph({ maximized }: { maximized: boolean }) {
  return maximized ? (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
      <rect
        x="2.6"
        y="5.4"
        width="6.4"
        height="6.4"
        rx="1.4"
        stroke="currentColor"
        strokeWidth="1.2"
      />
      <path
        d="M5.2 5.4V4.2a1.6 1.6 0 0 1 1.6-1.6h3.6A1.6 1.6 0 0 1 12 4.2v3.6a1.6 1.6 0 0 1-1.6 1.6h-1.2"
        stroke="currentColor"
        strokeWidth="1.2"
        strokeLinecap="round"
      />
    </svg>
  ) : (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
      <rect
        x="2.8"
        y="2.8"
        width="8.4"
        height="8.4"
        rx="1.6"
        stroke="currentColor"
        strokeWidth="1.2"
      />
    </svg>
  );
}

function CloseGlyph() {
  return (
    <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
      <path
        d="M4 4l6 6M10 4l-6 6"
        stroke="currentColor"
        strokeWidth="1.3"
        strokeLinecap="round"
      />
    </svg>
  );
}

/**
 * Custom window chrome.
 *
 * The application is shipped frameless (`decorations: false`), therefore the
 * title bar implements dragging, minimising, maximising and closing on its
 * own - in the same soft, rounded language as the rest of the console.
 */
export default function TitleBar({ context }: TitleBarProps) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    const appWindow = getCurrentWindow();

    const sync = async () => {
      try {
        const isMaximized = await appWindow.isMaximized();
        setMaximized(isMaximized);
        // A maximised window has no rounded corners to show.
        document.body.dataset.maximized = isMaximized ? "1" : "0";
      } catch {
        /* the window may already be gone */
      }
    };

    void sync();
    const unlisten = appWindow.onResized(() => {
      void sync();
    });

    return () => {
      unlisten.then((dispose) => dispose()).catch(() => undefined);
    };
  }, []);

  const minimize = useCallback(() => {
    void getCurrentWindow().minimize();
  }, []);

  const toggleMaximize = useCallback(() => {
    void getCurrentWindow().toggleMaximize();
  }, []);

  const close = useCallback(() => {
    void getCurrentWindow().close();
  }, []);

  return (
    <div
      className="titlebar"
      data-tauri-drag-region
      onDoubleClick={toggleMaximize}
    >
      <div className="titlebar-brand" data-tauri-drag-region>
        <span className="titlebar-mark" data-tauri-drag-region>
          H
        </span>
        <span className="titlebar-text" data-tauri-drag-region>
          DISINTEGRATOR
          <i>//</i>
          DEEP PURGE CONSOLE
        </span>
      </div>

      <div className="titlebar-context" data-tauri-drag-region>
        {context}
      </div>

      <div className="window-controls">
        <button
          className="window-button"
          onClick={minimize}
          aria-label="Minimise"
          title="Minimise"
        >
          <MinimizeGlyph />
        </button>
        <button
          className="window-button"
          onClick={toggleMaximize}
          aria-label={maximized ? "Restore" : "Maximise"}
          title={maximized ? "Restore" : "Maximise"}
        >
          <MaximizeGlyph maximized={maximized} />
        </button>
        <button
          className="window-button window-button-close"
          onClick={close}
          aria-label="Close"
          title="Close"
        >
          <CloseGlyph />
        </button>
      </div>
    </div>
  );
}
