import { useEffect, useState } from "react";
import { useModal } from "../hooks/useModal";
import { clearAppData, getSettings, setSettings, toAppError, type Settings } from "../lib/api";
import { useSign } from "../signStore";

/** Settings: file-size/page limits, privacy statement and "clear all app data". */
export function SettingsDialog({ onClose, onCleared }: { onClose: () => void; onCleared: () => void }) {
  const dlg = useModal<HTMLDivElement>(onClose);
  const [s, setS] = useState<Settings | null>(null);
  const [msg, setMsg] = useState<{ ok: boolean; text: string } | null>(null);

  useEffect(() => {
    getSettings().then(setS).catch((e) => setMsg({ ok: false, text: toAppError(e).message }));
  }, []);

  const save = async () => {
    if (!s) return;
    try {
      setS(await setSettings(s));
      setMsg({ ok: true, text: "Saved. New limits apply to files opened from now on." });
    } catch (e) {
      setMsg({ ok: false, text: toAppError(e).message });
    }
  };

  const clear = async () => {
    if (!window.confirm("Delete all PDF Forge data on this computer?\n\nThis removes your saved signatures, recent files, remembered view positions and settings. Your PDF files are not touched.")) return;
    try {
      await clearAppData();
      useSign.setState({ library: [] });
      setMsg({ ok: true, text: "All app data was deleted." });
      onCleared();
      setS(await getSettings());
    } catch (e) {
      setMsg({ ok: false, text: toAppError(e).message });
    }
  };

  const field = "w-28 rounded border px-2 py-1 dark:bg-neutral-900";
  return (
    <div className="fixed inset-0 z-30 grid place-items-center bg-black/40" role="presentation">
      <div ref={dlg} role="dialog" aria-modal="true" aria-labelledby="set-h" className="max-h-[90vh] w-[34rem] max-w-[95vw] overflow-auto rounded-lg bg-white p-5 shadow-xl dark:bg-neutral-800">
        <h2 id="set-h" className="mb-3 text-lg font-semibold">Settings</h2>

        <section aria-labelledby="lim-h" className="mb-4">
          <h3 id="lim-h" className="mb-1 font-semibold">Safety limits</h3>
          <p className="mb-2 text-xs text-neutral-500">Every PDF is treated as untrusted input. Files above these limits are refused.</p>
          {s && (
            <div className="space-y-2 text-sm">
              <label className="flex items-center justify-between">Maximum file size (MB)
                <input type="number" className={field} min={1} max={16384} value={s.maxFileSizeMb} onChange={(e) => setS({ ...s, maxFileSizeMb: Number(e.target.value) })} /></label>
              <label className="flex items-center justify-between">Maximum page count
                <input type="number" className={field} min={1} max={1000000} value={s.maxPages} onChange={(e) => setS({ ...s, maxPages: Number(e.target.value) })} /></label>
              <button className="rounded bg-blue-600 px-3 py-1 text-white" onClick={save}>Save limits</button>
            </div>
          )}
        </section>

        <section aria-labelledby="priv-h" className="mb-4 text-sm">
          <h3 id="priv-h" className="mb-1 font-semibold">Privacy</h3>
          <ul className="list-inside list-disc space-y-1 text-neutral-700 dark:text-neutral-300">
            <li>PDF Forge works fully offline. It makes no network requests, sends no telemetry and never checks for updates.</li>
            <li>Stored locally: recent file names, the last page/zoom per file, your saved signatures and these settings. Passwords are never stored. No PDF content is kept.</li>
            <li>Your original files are never modified; edits are written to new files.</li>
          </ul>
        </section>

        <section aria-labelledby="clr-h" className="mb-2">
          <h3 id="clr-h" className="mb-1 font-semibold">Clear all app data</h3>
          <button className="rounded border border-red-600 px-3 py-1 text-sm text-red-700" onClick={clear}>Clear all app data…</button>
        </section>

        {msg && <p role={msg.ok ? "status" : "alert"} className={`mt-2 text-sm ${msg.ok ? "text-green-700" : "text-red-700"}`}>{msg.text}</p>}
        <div className="mt-4 flex justify-end"><button className="rounded border px-3 py-1" onClick={onClose}>Close</button></div>
      </div>
    </div>
  );
}
