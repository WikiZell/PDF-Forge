import { useEffect, useState } from "react";
import { sigAsset } from "../lib/sig-api";

const cache = new Map<string, Promise<string>>();

/** Object URL for a saved raster signature (cached for the app's lifetime; tiny PNGs). */
export function useSigImage(id: string | undefined, enabled = true): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!id || !enabled) return;
    let alive = true;
    let p = cache.get(id);
    if (!p) {
      p = sigAsset(id).then((b) => URL.createObjectURL(b));
      cache.set(id, p);
      p.catch(() => cache.delete(id));
    }
    p.then((u) => alive && setUrl(u)).catch(() => {});
    return () => {
      alive = false;
    };
  }, [id, enabled]);
  return url;
}

export function forgetSigImage(id: string) {
  cache.delete(id);
}
