import { beforeEach, describe, expect, it } from "vitest";
import { useViewer } from "./store";

const doc = { id: 1, fileName: "a.pdf", pageCount: 5 };

describe("viewer store", () => {
  beforeEach(() => useViewer.setState({ doc: null, page: 0, error: null, zoom: { kind: "fit-width" } }));

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
  it("clamps percent zoom", () => {
    useViewer.getState().setZoom({ kind: "percent", value: 1000 });
    expect(useViewer.getState().zoom).toEqual({ kind: "percent", value: 400 });
  });
  it("opening a document resets page and error", () => {
    useViewer.setState({ page: 3, error: { code: "internal", message: "x" } });
    useViewer.getState().setDoc(doc);
    expect(useViewer.getState().page).toBe(0);
    expect(useViewer.getState().error).toBeNull();
  });
});
