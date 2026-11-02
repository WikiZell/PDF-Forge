import { useEffect, useRef, useState } from "react";
import { pageSize, renderPage, toAppError } from "../lib/api";
import { computeScale } from "../lib/zoom";
import { useViewer } from "../store";

/** Renders the current page as a PNG produced by PDFium in the Rust backend. */
export function PageView() {
  const { doc, page, zoom, pageSize: size, setPageSize, setError } = useViewer();
  const host = useRef<HTMLDivElement>(null);
  const [viewport, setViewport] = useState({ width: 800, height: 600 });
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setViewport({ width: el.clientWidth, height: el.clientHeight }));
    ro.observe(el);
    setViewport({ width: el.clientWidth, height: el.clientHeight });
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    if (!doc) return;
    let alive = true;
    pageSize(doc.id, page)
      .then((s) => alive && setPageSize(s))
      .catch((e) => alive && setError(toAppError(e)));
    return () => {
      alive = false;
    };
  }, [doc, page, setPageSize, setError]);

  const cssScale = size ? computeScale(zoom, size, viewport) : 1;
  // Quantise so tiny resizes don't trigger a re-render of the page.
  const dpr = window.devicePixelRatio || 1;
  const renderScale = Math.round(cssScale * dpr * 100) / 100;

  useEffect(() => {
    if (!doc || !size) return;
    let alive = true;
    let made: string | null = null;
    renderPage(doc.id, page, renderScale)
      .then((blob) => {
        if (!alive) return;
        made = URL.createObjectURL(blob);
        setUrl((old) => {
          if (old) URL.revokeObjectURL(old);
          return made;
        });
      })
      .catch((e) => alive && setError(toAppError(e)));
    return () => {
      alive = false;
    };
  }, [doc, page, size, renderScale, setError]);

  useEffect(() => () => setUrl((old) => (old && URL.revokeObjectURL(old), null)), [doc]);

  return (
    <div ref={host} className="flex-1 overflow-auto bg-neutral-300 p-4 dark:bg-neutral-700" tabIndex={0} aria-label="Document viewer">
      {url && size && (
        <img
          src={url}
          alt={`Page ${page + 1} of ${doc?.pageCount ?? 0}`}
          width={Math.round(size.width * cssScale)}
          height={Math.round(size.height * cssScale)}
          className="mx-auto bg-white shadow-lg"
          draggable={false}
        />
      )}
    </div>
  );
}
