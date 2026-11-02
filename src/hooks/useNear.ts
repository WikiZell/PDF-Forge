import { useEffect, useRef, useState } from "react";

/** True while the element is within `margin` px of the scroll viewport; used to render lazily. */
export function useNear<T extends HTMLElement>(margin = 800) {
  const ref = useRef<T>(null);
  const [near, setNear] = useState(false);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const io = new IntersectionObserver(([e]) => setNear(e.isIntersecting), { rootMargin: `${margin}px 0px` });
    io.observe(el);
    return () => io.disconnect();
  }, [margin]);
  return [ref, near] as const;
}
