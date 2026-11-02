import { invoke } from "@tauri-apps/api/core";
import type { Placement } from "./placements";

export interface Drawn {
  strokes: [number, number, number][][];
  view: [number, number];
  width: number;
  color: [number, number, number];
}
export interface SigMeta {
  id: string;
  name: string;
  kind: "drawn" | "typed" | "imported";
  created: number;
  aspect: number;
  drawn?: Drawn | null;
  text?: string | null;
  font?: string | null;
}
export interface SignedResult {
  path: string;
  applied: number;
  warnings: string[];
}

export const sigList = () => invoke<SigMeta[]>("sig_list");
export const sigRename = (id: string, name: string) => invoke<void>("sig_rename", { id, name });
export const sigDelete = (id: string) => invoke<void>("sig_delete", { id });
export const sigCreateDrawn = (d: { name: string } & Drawn) => invoke<SigMeta>("sig_create_drawn", { d });

export async function sigAsset(id: string): Promise<Blob> {
  const buf = await invoke<ArrayBuffer>("sig_asset", { id });
  return new Blob([buf], { type: "image/png" });
}

/** Header values must be ASCII; the backend percent-decodes them. */
const enc = (s: string) => encodeURIComponent(s);

export async function sigProcessImport(bytes: Uint8Array, threshold: number): Promise<Blob> {
  const buf = await invoke<ArrayBuffer>("sig_process_import", bytes, { headers: { threshold: String(threshold) } });
  return new Blob([buf], { type: "image/png" });
}

export const sigCreateImage = (png: Uint8Array, meta: { name: string; kind: "typed" | "imported"; text?: string; font?: string }) =>
  invoke<SigMeta>("sig_create_image", png, {
    headers: { name: enc(meta.name), kind: meta.kind, ...(meta.text ? { text: enc(meta.text) } : {}), ...(meta.font ? { font: enc(meta.font) } : {}) },
  });

export const signDocument = (path: string, placements: Placement[], flatten: boolean, password?: string) =>
  invoke<SignedResult | null>("sign_document", {
    path,
    password: password ?? null,
    flatten,
    placements: placements.map((p) => ({
      page: p.page, x: p.x, y: p.y, w: p.w, h: p.h, rotation: p.rotation, kind: p.kind,
      sigId: p.sigId ?? null, text: p.text ?? null, fontSize: p.fontSize ?? null,
    })),
  });

/** Typed-signature fonts (all SIL OFL 1.1; files in src/assets/fonts). */
export const SIGNATURE_FONTS = [
  { id: "Dancing Script", css: '"PF Dancing Script"' },
  { id: "Great Vibes", css: '"PF Great Vibes"' },
  { id: "Caveat", css: '"PF Caveat"' },
  { id: "Sacramento", css: '"PF Sacramento"' },
] as const;
