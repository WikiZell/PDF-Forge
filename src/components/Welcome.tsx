import { useEffect, useState } from "react";
import { forgetRecent, recentFiles, type RecentFile } from "../lib/api";

export function Welcome({ onOpenRecent }: { onOpenRecent: (path: string) => void }) {
  const [recent, setRecent] = useState<RecentFile[]>([]);
  useEffect(() => {
    recentFiles().then(setRecent).catch(() => {});
  }, []);
  return (
    <main className="grid flex-1 place-items-center overflow-auto p-6">
      <div className="w-full max-w-xl">
        <h1 className="mb-1 text-center text-2xl font-semibold">PDF Forge</h1>
        <p className="mb-6 text-center text-neutral-600 dark:text-neutral-300">
          Open a PDF with Ctrl+O or drop a file on this window. Everything stays on this computer.
        </p>
        {recent.length > 0 && (
          <section aria-labelledby="recent-h">
            <h2 id="recent-h" className="mb-1 text-sm font-semibold uppercase tracking-wide text-neutral-500">Recent files</h2>
            <ul>
              {recent.map((r) => (
                <li key={r.path} className="flex items-center gap-2">
                  <button className="min-w-0 flex-1 truncate rounded px-2 py-1.5 text-left hover:bg-neutral-200 dark:hover:bg-neutral-800" title={r.path} onClick={() => onOpenRecent(r.path)}>
                    {r.name}
                    <span className="ml-2 text-xs text-neutral-500">{r.path}</span>
                  </button>
                  <button
                    aria-label={`Remove ${r.name} from recent files`}
                    className="rounded px-2 py-1 text-neutral-500 hover:bg-neutral-200 dark:hover:bg-neutral-800"
                    onClick={() => forgetRecent(r.path).then(() => setRecent((l) => l.filter((x) => x.path !== r.path)))}
                  >
                    ✕
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
