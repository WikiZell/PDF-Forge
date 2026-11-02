import type { Size } from "./zoom";

export type Layout = "single" | "continuous" | "two-page";

export const PAGE_GAP = 12;

/** Groups of page indices shown side by side. Two-page layout pairs (0,1), (2,3), ... */
export function groupPages(layout: Layout, count: number): number[][] {
  const groups: number[][] = [];
  const per = layout === "two-page" ? 2 : 1;
  for (let i = 0; i < count; i += per) {
    groups.push(Array.from({ length: Math.min(per, count - i) }, (_, k) => i + k));
  }
  return groups;
}

export function groupIndexOfPage(layout: Layout, page: number): number {
  return layout === "two-page" ? Math.floor(page / 2) : page;
}

/** Size, in PDF points, that fit-width / fit-page should fit for the current layout. */
export function referenceSize(layout: Layout, sizes: Size[], page: number): Size | null {
  if (sizes.length === 0) return null;
  if (layout === "continuous") {
    return { width: Math.max(...sizes.map((s) => s.width)), height: Math.max(...sizes.map((s) => s.height)) };
  }
  const group = groupPages(layout, sizes.length)[groupIndexOfPage(layout, Math.min(page, sizes.length - 1))];
  const gs = group.map((i) => sizes[i]);
  return {
    width: gs.reduce((a, s) => a + s.width, 0) + (gs.length - 1) * (PAGE_GAP / 1),
    height: Math.max(...gs.map((s) => s.height)),
  };
}

/** Which group is "current" while scrolling: the first group whose bottom is below `scrollTop`. */
export function groupAtScroll(tops: number[], heights: number[], scrollTop: number): number {
  for (let i = 0; i < tops.length; i++) {
    if (tops[i] + heights[i] > scrollTop + 8) return i;
  }
  return Math.max(0, tops.length - 1);
}
