import { useEffect, useState } from "react";
import { forgetRecent, recentFiles, type RecentFile } from "../lib/api";
import { Icon } from "./Icon";

export function Welcome({ onOpenRecent }: { onOpenRecent: (path: string) => void }) {
  const [recent, setRecent] = useState<RecentFile[]>([]);
  useEffect(() => {
    recentFiles().then(setRecent).catch(() => {});
  }, []);
  return (
    <main className="grid flex-1 place-items-center overflow-auto p-8" style={{ background: "radial-gradient(1200px 500px at 50% -10%, var(--accent-soft), transparent 70%)" }}>
      <div className="w-full max-w-2xl">
        <div className="mb-8 text-center">
          <span className="mx-auto mb-4 grid h-14 w-14 place-items-center rounded-2xl text-white" style={{ background: "linear-gradient(135deg, var(--accent), #7c3aed)", boxShadow: "0 10px 30px color-mix(in srgb, var(--accent) 40%, transparent)" }} aria-hidden>
            <Icon name="pdf" size={28} />
          </span>
          <h1 className="text-3xl font-semibold tracking-tight">PDF Forge</h1>
          <p className="mx-auto mt-2 max-w-md" style={{ color: "var(--muted)" }}>
            Open a PDF with <span className="kbd">Ctrl</span> + <span className="kbd">O</span> or drop a file on this window. Everything stays on this computer.
          </p>
          <p className="mt-3 inline-flex items-center gap-1.5 text-xs" style={{ color: "var(--muted)" }}>
            <Icon name="shield" size={14} /> Offline by design: no network, no telemetry
          </p>
        </div>
        {recent.length > 0 && (
          <section aria-labelledby="recent-h" className="card !p-2">
            <h2 id="recent-h" className="eyebrow px-3 pb-1 pt-2">Recent files</h2>
            <ul>
              {recent.map((r) => (
                <li key={r.path} className="group flex items-center gap-1">
                  <button className="list-btn flex min-w-0 flex-1 items-center gap-3 !py-2" title={r.path} onClick={() => onOpenRecent(r.path)}>
                    <span className="grid h-8 w-8 shrink-0 place-items-center rounded-lg" style={{ background: "var(--accent-soft)", color: "var(--accent)" }} aria-hidden><Icon name="pdf" size={16} /></span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-medium">{r.name}</span>
                      <span className="block truncate text-xs" style={{ color: "var(--muted)" }}>{r.path}</span>
                    </span>
                  </button>
                  <button
                    aria-label={`Remove ${r.name} from recent files`}
                    className="btn btn-quiet btn-icon"
                    onClick={() => forgetRecent(r.path).then(() => setRecent((l) => l.filter((x) => x.path !== r.path)))}
                  >
                    <Icon name="close" size={16} />
                  </button>
                </li>
              ))}
            </ul>
          </section>
        )}
      </div>
    </main>
  );
}
