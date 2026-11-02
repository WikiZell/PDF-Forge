import { beforeEach, describe, expect, it } from "vitest";
import { useViewer } from "./store";

const doc = { id: 1, path: "/a.pdf", fileName: "a.pdf", pageCount: 5, viewState: null };

describe("viewer store", () => {
  beforeEach(() =>
    useViewer.setState({ doc: null, page: 0, error: null, zoom: { kind: "fit-width" }, layout: "continuous" }),
  );

  it("clamps page navigation to the document", () => {
    useViewer.getState().setDoc(doc);
    useViewer.getState().goToPage(99);
    expect(useViewer.getState().page).toBe(4);
    useViewer.getState().goToPage(-3);
    expect(useViewer.getState().page).toBe(0);
    useViewer.getState().goToPage(NaN);
    expect(useViewer.getState().page).toBe(0);
  });
  it("ignores navigation without a document", () => {
    useViewer.getState().goToPage(2);
    expect(useViewer.getState().page).toBe(0);
  });
  it("goToPage bumps jumpSeq but scroll updates do not", () => {
    useViewer.getState().setDoc(doc);
    const seq = useViewer.getState().jumpSeq;
    useViewer.getState().setPageFromScroll(3);
    expect(useViewer.getState().jumpSeq).toBe(seq);
    useViewer.getState().goToPage(2);
    expect(useViewer.getState().jumpSeq).toBe(seq + 1);
  });
  it("clamps percent zoom", () => {
    useViewer.getState().setZoom({ kind: "percent", value: 1000 });
    expect(useViewer.getState().zoom).toEqual({ kind: "percent", value: 400 });
  });
  it("opening a document resets error and restores the saved view state", () => {
    useViewer.setState({ page: 3, error: { code: "internal", message: "x" } });
    useViewer.getState().setDoc({ ...doc, viewState: { page: 2, zoom: "150", layout: "two-page" } });
    const s = useViewer.getState();
    expect(s.page).toBe(2);
    expect(s.error).toBeNull();
    expect(s.zoom).toEqual({ kind: "percent", value: 150 });
    expect(s.layout).toBe("two-page");
  });
  it("ignores a hostile saved view state", () => {
    useViewer.getState().setDoc({ ...doc, viewState: { page: 999, zoom: "<x>", layout: "evil" } });
    const s = useViewer.getState();
    expect(s.page).toBe(4);
    expect(s.zoom).toEqual({ kind: "fit-width" });
    expect(s.layout).toBe("continuous");
  });
  it("steps through search hits with wraparound and jumps to their page", () => {
    useViewer.getState().setDoc(doc);
    const r = { x: 0, y: 0, w: 1, h: 1 };
    useViewer.getState().setSearch({ hits: [{ page: 1, rects: [r] }, { page: 3, rects: [r] }] });
    useViewer.getState().stepHit(1);
    expect(useViewer.getState().search.current).toBe(0);
    expect(useViewer.getState().page).toBe(1);
    useViewer.getState().stepHit(1);
    useViewer.getState().stepHit(1);
    expect(useViewer.getState().search.current).toBe(0);
    useViewer.getState().stepHit(-1);
    expect(useViewer.getState().search.current).toBe(1);
    expect(useViewer.getState().page).toBe(3);
  });
});
