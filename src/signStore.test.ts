import { beforeEach, describe, expect, it } from "vitest";
import type { Placement } from "./lib/placements";
import { useSign } from "./signStore";

const p = (id: string): Placement => ({ id, page: 0, x: 0, y: 0, w: 10, h: 10, rotation: 0, kind: "text", text: id, fontSize: 12 });

describe("sign store", () => {
  beforeEach(() => useSign.getState().reset());

  it("commit, undo and redo of placements", () => {
    const s = useSign.getState();
    s.commitPlacements([p("a")]);
    useSign.getState().commitPlacements([p("a"), p("b")]);
    useSign.getState().undo();
    expect(useSign.getState().history.present.map((x) => x.id)).toEqual(["a"]);
    useSign.getState().redo();
    expect(useSign.getState().history.present.map((x) => x.id)).toEqual(["a", "b"]);
  });

  it("a drag is a single undo step", () => {
    const s = useSign.getState();
    s.commitPlacements([p("a")]);
    const before = useSign.getState().history.present;
    for (let i = 1; i <= 10; i++) useSign.getState().liveUpdate([{ ...p("a"), x: i * 5 }]);
    useSign.getState().endDrag(before);
    expect(useSign.getState().history.past.length).toBe(2); // initial [] and the pre-drag state
    useSign.getState().undo();
    expect(useSign.getState().history.present[0].x).toBe(0);
  });

  it("a drag that changed nothing adds no undo step", () => {
    useSign.getState().commitPlacements([p("a")]);
    const before = useSign.getState().history.present;
    const pastLen = useSign.getState().history.past.length;
    useSign.getState().endDrag(before);
    expect(useSign.getState().history.past.length).toBe(pastLen);
  });

  it("deleteSelected removes only the selection and is undoable", () => {
    useSign.getState().commitPlacements([p("a"), p("b")]);
    useSign.getState().select("a");
    useSign.getState().deleteSelected();
    expect(useSign.getState().history.present.map((x) => x.id)).toEqual(["b"]);
    expect(useSign.getState().selectedId).toBeNull();
    useSign.getState().undo();
    expect(useSign.getState().history.present.length).toBe(2);
  });

  it("deleteSelected without a selection does nothing", () => {
    useSign.getState().commitPlacements([p("a")]);
    useSign.getState().deleteSelected();
    expect(useSign.getState().history.present.length).toBe(1);
  });
});
