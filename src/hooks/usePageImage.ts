import { useEffect, useState } from "react";
import { renderPage, toAppError } from "../lib/api";
import { useViewer } from "../store";

/** Renders a page via the backend while `enabled`; releases the bitmap when disabled. */
export function usePageImage(docId: number, page: number, scale: number, enabled: boolean) {
  const [url, setUrl] = useState<string | null>(null);
  const setError = useViewer((s) => s.setError);

  useEffect(() => {
    if (!enabled) {
      setUrl((old) => (old && URL.revokeObjectURL(old), null));
      return;
    }
    let alive = true;
    const t = setTimeout(() => {
      renderPage(docId, page, scale)
        .then((blob) => {
          if (!alive) return;
          const next = URL.createObjectURL(blob);
          setUrl((old) => (old && URL.revokeObjectURL(old), next));
        })
        .catch((e) => alive && setError(toAppError(e)));
    }, 40); // debounce rapid zoom/scroll changes
    return () => {
      alive = false;
      clearTimeout(t);
    };
  }, [docId, page, scale, enabled, setError]);

  useEffect(() => () => setUrl((old) => (old && URL.revokeObjectURL(old), null)), []);
  return url;
}
