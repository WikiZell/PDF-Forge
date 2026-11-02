import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ItemDto, RuleFile } from "./bulk-rule";

export interface ProgressEvent { jobId: number; done: number; total: number; item: ItemDto }
export interface FinishedEvent { jobId: number; items: ItemDto[]; reportPath: string | null; error: string | null; cancelled: boolean }

export const bulkDefaultWorkers = () => invoke<number>("bulk_default_workers");
export const bulkPickFolder = () => invoke<string[]>("bulk_pick_folder");
export const bulkPickOutputDir = () => invoke<string | null>("bulk_pick_output_dir");
export const bulkRuleLoad = () => invoke<RuleFile | null>("bulk_rule_load");
export const bulkRuleSave = (rule: RuleFile) => invoke<string | null>("bulk_rule_save", { rule });
export const bulkPreview = (rule: RuleFile, files: string[]) => invoke<ItemDto[]>("bulk_preview", { rule, files });
export async function bulkPreviewImage(rule: RuleFile, file: string): Promise<Blob> {
  const buf = await invoke<ArrayBuffer>("bulk_preview_image", { rule, file });
  return new Blob([buf], { type: "image/png" });
}
export const bulkRun = (rule: RuleFile, files: string[], workers: number) => {
  const d = new Date();
  return invoke<number>("bulk_run", { rule, files, workers, today: { year: d.getFullYear(), month: d.getMonth() + 1, day: d.getDate() } });
};
export const bulkCancel = (jobId: number) => invoke<void>("bulk_cancel", { jobId });
export const onBulkProgress = (cb: (e: ProgressEvent) => void): Promise<UnlistenFn> => listen<ProgressEvent>("bulk-progress", (e) => cb(e.payload));
export const onBulkFinished = (cb: (e: FinishedEvent) => void): Promise<UnlistenFn> => listen<FinishedEvent>("bulk-finished", (e) => cb(e.payload));
