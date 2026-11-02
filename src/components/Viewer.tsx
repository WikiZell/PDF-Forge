import { useEffect, useMemo, useRef, useState } from "react";
import { groupAtScroll, groupIndexOfPage, groupPages, PAGE_GAP, referenceSize } from "../lib/layout";
import { computeScale } from "../lib/zoom";
import { useViewer } from "../store";
import { PageSlot } from "./PageSlot";

export function Viewer() {
  const { doc, page, jumpSeq, sizes, zoom, layout, search, setPageFromScroll } = useViewer();
  const host = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({ width: 800, height: 600 });

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewport({ width: el.clientWidth, height: el.clientHeight }));
    ro.observe(el);
    setViewport({ width: el.clientWidth, height: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  const ref = referenceSize(layout, sizes, page);
  const scale = ref ? Math.round(computeScale(zoom, ref, viewport, 48) * 1000) / 1000 : 1;
  useEffect(() => useViewer.getState().setEffectiveScale(scale), [scale]);
  const groups = useMemo(() => groupPages(layout, doc?.pageCount ?? 0), [layout, doc?.pageCount]);
  const shown = layout === "continuous" ? groups.map((g, i) => ({ g, i })) : [{ g: groups[groupIndexOfPage(layout, page)] ?? [], i: groupIndexOfPage(layout, page) }];

  // Navigation requests scroll to the page.
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    if (layout === "continuous") {
      const target = el.querySelector<HTMLElement>(`[data-page="${page}"]`);
      target?.parentElement?.scrollIntoView({ block: "start" });
    } else {
      el.scrollTop = 0;
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [jumpSeq, sizes.length]);

  // Scroll position drives the current page in continuous mode.
  useEffect(() => {
    const el = host.current;
    if (!el || layout !== "continuous") return;
    let raf = 0;
    const onScroll = () => {
      cancelAnimationFrame(raf);
      raf = requestAnimationFrame(() => {
        const rows = Array.from(el.querySelectorAll<HTMLElement>("[data-group]"));
        if (rows.length === 0) return;
        const base = el.getBoundingClientRect().top - el.scrollTop;
        const tops = rows.map((r) => r.getBoundingClientRect().top - base);
        const heights = rows.map((r) => r.offsetHeight);
        const gi = groupAtScroll(tops, heights, el.scrollTop);
        const first = Number(rows[gi]?.dataset.group ?? 0);
        if (first !== useViewer.getState().page) setPageFromScroll(first);
      });
    };
    el.addEventListener("scroll", onScroll, { passive: true });
    return () => {
      el.removeEventListener("scroll", onScroll);
      cancelAnimationFrame(raf);
    };
  }, [layout, setPageFromScroll, sizes.length]);

  // Ctrl + wheel zooms.
  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      const cur = Math.round(scale * 100);
      useViewer.getState().setZoom({ kind: "percent", value: cur + (e.deltaY < 0 ? 10 : -10) });
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [scale]);

  const hitsByPage = useMemo(() => {
    const m = new Map<number, { hit: (typeof search.hits)[number]; index: number }[]>();
    search.hits.forEach((hit, index) => m.set(hit.page, [...(m.get(hit.page) ?? []), { hit, index }]));
    return m;
  }, [search.hits]);

  return (
    <div
      ref={host}
      tabIndex={0}
      aria-label="Document viewer"
      className="flex-1 overflow-auto bg-neutral-300 p-4 focus-visible:outline-2 dark:bg-neutral-700"
    >
      {sizes.length === doc?.pageCount &&
        shown.map(({ g, i }) => (
          <div
            key={i}
            data-group={g[0]}
            className="mx-auto flex w-max items-start justify-center"
            style={{ gap: PAGE_GAP, marginBottom: PAGE_GAP }}
          >
            {g.map((p) => (
              <PageSlot
                key={p}
                docId={doc!.id}
                page={p}
                width={sizes[p].width}
                height={sizes[p].height}
                scale={scale}
                hits={hitsByPage.get(p) ?? []}
                currentHit={search.current}
              />
            ))}
          </div>
        ))}
    </div>
  );
}
