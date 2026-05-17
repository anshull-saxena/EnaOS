import { useEffect, useState } from "react";
import { EnaBar } from "./components/EnaBar";

/// Check if running inside Tauri by attempting to detect the Tauri API.
function useIsTauri() {
  const [isTauri, setIsTauri] = useState(false);
  useEffect(() => {
    // In Tauri v2, __TAURI__ is injected by the runtime.
    setIsTauri(typeof window !== "undefined" && "__TAURI__" in window);
  }, []);
  return isTauri;
}

export default function App() {
  const isTauri = useIsTauri();

  return (
    <div className="relative flex h-screen w-screen flex-col overflow-hidden bg-transparent select-none">
      {/* In production (Tauri), the window is transparent.
          In dev (browser), show a dark backdrop so the bar is visible. */}
      {!isTauri && (
        <div className="absolute inset-0 bg-black" />
      )}

      {/* Just the bar — no desktop, no system tray, no wallpaper.
          Tauri handles window positioning and transparency. */}
      <EnaBar isTauri={isTauri} />
    </div>
  );
}
