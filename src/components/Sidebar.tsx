import { useEffect, useRef, useState } from "react";
import { outline as fetchOutline, searchDoc, toAppError, type OutlineItem } from "../lib/api";
import { useNear } from "../hooks/useNear";
import { usePageImage } from "../hooks/usePageImage";
import { useViewer, type SidebarTab } from "../store";

const TABS: { id: SidebarTab; label: string }[] = [
  { id: "thumbnails", label: "Pages" },
  { id: "outline", label: "Bookmarks" },
  { id: "search", label: "Search" },
];

export function Sidebar() {
  const sidebar = useViewer((s) => s.sidebar);
  const setSidebar = useViewer((s) => s.setSidebar);
  if (!sidebar) return null;
  return (
    <aside className="flex w-64 shrink-0 flex-col border-r" style={{ background: "var(--surface)", borderColor: "var(--border)" }} aria-label="Sidebar">
      <div role="tablist" aria-label="Sidebar panels" className="flex gap-4 border-b px-3 text-sm" style={{ borderColor: "var(--border)" }}>
        {TABS.map((t) => (
          <button
            key={t.id}
            role="tab"
            aria-selected={sidebar === t.id}
            className="tab"
            onClick={() => setSidebar(t.id)}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div role="tabpanel" className="min-h-0 flex-1 overflow-auto">
        {sidebar === "thumbnails" && <Thumbnails />}
        {sidebar === "outline" && <Outline />}
        {sidebar === "search" && <SearchPanel />}
      </div>
    </aside>
  );
}

function Thumb({ page }: { page: number }) {
  const { doc, sizes, page: current, goToPage } = useViewer();
  const [ref, near] = useNear<HTMLButtonElement>(400);
  const s = sizes[page];
  const w = 140;
  const scale = s ? w / s.width : 1;
  const url = usePageImage(doc!.id, page, Math.round(scale * (window.devicePixelRatio || 1) * 100) / 100, near && !!s);
  return (
    <button
      ref={ref}
      aria-label={`Go to page ${page + 1}`}
      aria-current={current === page ? "page" : undefined}
      onClick={() => goToPage(page)}
      className={`mx-auto mb-3 block rounded-lg p-1.5 transition-colors ${current === page ? "ring-2 ring-[var(--accent)] bg-[var(--accent-soft)]" : "hover:bg-[var(--surface-2)]"}`}
    >
      <div className="page-sheet" style={{ width: w, height: s ? Math.round(s.height * scale) : 180 }}>
        {url && <img src={url} alt="" width={w} draggable={false} />}
      </div>
      <div className="mt-1 text-center text-xs" style={{ color: "var(--muted)" }}>{page + 1}</div>
    </button>
  );
}

function Thumbnails() {
  const { doc, sizes } = useViewer();
  if (!doc || sizes.length !== doc.pageCount) return null;
  return (
    <div className="p-2">
      {Array.from({ length: doc.pageCount }, (_, i) => (
        <Thumb key={i} page={i} />
      ))}
    </div>
  );
}

function Outline() {
  const doc = useViewer((s) => s.doc);
  const goToPage = useViewer((s) => s.goToPage);
  const [items, setItems] = useState<OutlineItem[] | null>(null);
  useEffect(() => {
    if (!doc) return;
    let alive = true;
    setItems(null);
    fetchOutline(doc.id).then((o) => alive && setItems(o)).catch(() => alive && setItems([]));
    return () => {
      alive = false;
    };
  }, [doc]);
  if (!items) return <p className="p-3 text-sm">Loading…</p>;
  if (items.length === 0) return <p className="p-3 text-sm text-[var(--muted)]">This document has no bookmarks.</p>;
  return (
    <ul className="p-1 text-sm">
      {items.map((it, i) => (
        <li key={i}>
          <button
            className="list-btn truncate disabled:opacity-50"
            style={{ paddingLeft: 8 + it.depth * 14 }}
            disabled={it.page === null}
            title={it.title}
            onClick={() => it.page !== null && goToPage(it.page)}
          >
            {it.title || "(untitled)"}
          </button>
        </li>
      ))}
    </ul>
  );
}

export function SearchPanel() {
  const { doc, search, setSearch, stepHit, setError } = useViewer();
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);

  const run = async () => {
    if (!doc || !search.query) return;
    setSearch({ running: true, hits: [], current: -1, done: false });
    try {
      const hits = await searchDoc(doc.id, search.query, search.matchCase);
      setSearch({ hits, running: false, done: true });
      if (hits.length) stepHit(1);
    } catch (e) {
      setSearch({ running: false });
      setError(toAppError(e));
    }
  };

  return (
    <div className="p-2 text-sm">
      <form
        role="search"
        onSubmit={(e) => {
          e.preventDefault();
          run();
        }}
      >
        <label className="sr-only" htmlFor="q">Search text</label>
        <input
          id="q"
          ref={input}
          value={search.query}
          onChange={(e) => setSearch({ query: e.target.value })}
          placeholder="Find in document"
          className="input w-full"
        />
        <label className="mt-1 flex items-center gap-1">
          <input type="checkbox" checked={search.matchCase} onChange={(e) => setSearch({ matchCase: e.target.checked })} />
          Match case
        </label>
        <div className="mt-2 flex gap-1">
          <button type="submit" className="btn btn-primary btn-sm" disabled={search.running}>Find</button>
          <button type="button" className="btn btn-sm" aria-label="Previous match" onClick={() => stepHit(-1)}>↑</button>
          <button type="button" className="btn btn-sm" aria-label="Next match" onClick={() => stepHit(1)}>↓</button>
        </div>
      </form>
      <p className="mt-2" aria-live="polite">
        {search.running
          ? "Searching…"
          : search.done
            ? search.hits.length === 0
              ? "No matches."
              : `Match ${search.current + 1} of ${search.hits.length}`
            : ""}
      </p>
      <ul className="mt-1">
        {search.hits.slice(0, 200).map((h, i) => (
          <li key={i}>
            <button
              className={`list-btn ${i === search.current ? "!bg-[var(--accent-soft)]" : ""}`}
              onClick={() => useViewer.setState((s) => ({ search: { ...s.search, current: i }, page: h.page, jumpSeq: s.jumpSeq + 1 }))}
            >
              Page {h.page + 1}
            </button>
          </li>
        ))}
      </ul>
    </div>
  );
}
