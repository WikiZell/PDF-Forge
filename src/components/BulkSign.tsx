import { useEffect, useMemo, useRef, useState } from "react";
import { toAppError } from "../lib/api";
import * as api from "../lib/bulk-api";
import { defaultForm, formToRule, ruleToForm, summarize, type ItemDto, type RuleForm } from "../lib/bulk-rule";
import { pickPdfs } from "../lib/organize-api";
import { useSign } from "../signStore";

const base = (p: string) => p.split(/[\\/]/).pop() ?? p;
const input = "input";
const btn = "btn btn-sm text-sm disabled:opacity-40";

type Phase = "edit" | "running" | "done";

export function BulkSign({ onClose }: { onClose: () => void }) {
  const library = useSign((s) => s.library);
  const [files, setFiles] = useState<string[]>([]);
  const [form, setForm] = useState<RuleForm>(defaultForm);
  const [outDir, setOutDir] = useState<string | null>(null);
  const [workers, setWorkers] = useState(1);
  const [maxWorkers, setMaxWorkers] = useState(1);
  const [error, setError] = useState<string | null>(null);
  const [preview, setPreview] = useState<ItemDto[] | null>(null);
  const [previewImg, setPreviewImg] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase>("edit");
  const [progress, setProgress] = useState({ done: 0, total: 0 });
  const [items, setItems] = useState<ItemDto[]>([]);
  const [reportPath, setReportPath] = useState<string | null>(null);
  const [cancelled, setCancelled] = useState(false);
  const [busy, setBusy] = useState(false);
  const job = useRef<number | null>(null);

  useEffect(() => {
    void useSign.getState().loadLibrary();
    api.bulkDefaultWorkers().then((n) => { setWorkers(n); setMaxWorkers(Math.max(n, navigator.hardwareConcurrency || n)); }).catch(() => {});
    const subs = [
      api.onBulkProgress((e) => {
        if (e.jobId !== job.current) return;
        setProgress({ done: e.done, total: e.total });
        setItems((l) => [...l, e.item]);
      }),
      api.onBulkFinished((e) => {
        if (e.jobId !== job.current) return;
        setItems(e.items.length ? e.items : []);
        setReportPath(e.reportPath);
        setCancelled(e.cancelled);
        if (e.error) setError(e.error);
        setPhase("done");
      }),
    ];
    return () => { subs.forEach((s) => s.then((f) => f())); };
  }, []);

  useEffect(() => {
    if (!form.signature && library.length) setForm((f) => ({ ...f, signature: library[0].id }));
  }, [library, form.signature]);

  const set = <K extends keyof RuleForm>(k: K, v: RuleForm[K]) => { setForm((f) => ({ ...f, [k]: v })); setPreview(null); setPreviewImg(null); };
  const ruleResult = useMemo(() => formToRule(form), [form]);
  const guard = async (fn: () => Promise<void>) => {
    setBusy(true); setError(null);
    try { await fn(); } catch (e) { setError(toAppError(e).message); } finally { setBusy(false); }
  };
  const addFiles = (paths: string[]) => { setFiles((cur) => [...new Set([...cur, ...paths])]); setPreview(null); setPreviewImg(null); };

  const runPreview = () => guard(async () => {
    if ("error" in ruleResult) throw new Error(ruleResult.error);
    if (files.length === 0) throw new Error("Add some PDF files first.");
    const res = await api.bulkPreview(ruleResult.rule, files);
    setPreview(res);
    setPreviewImg((old) => (old && URL.revokeObjectURL(old), null));
    const first = res.find((r) => r.status === "signed");
    if (first) {
      const blob = await api.bulkPreviewImage(ruleResult.rule, first.input);
      setPreviewImg(URL.createObjectURL(blob));
    }
  });

  const start = () => guard(async () => {
    if ("error" in ruleResult) throw new Error(ruleResult.error);
    if (files.length === 0) throw new Error("Add some PDF files first.");
    if (!outDir) throw new Error("Choose an output folder.");
    setItems([]); setProgress({ done: 0, total: files.length }); setReportPath(null); setCancelled(false);
    job.current = await api.bulkRun(ruleResult.rule, files, workers);
    setPhase("running");
  });

  const counts = summarize(preview ?? []);
  const missing = (preview ?? []).filter((p) => p.status !== "signed");
  const field = (label: string, el: React.ReactNode) => <label className="flex items-center justify-between gap-2 text-sm">{label}{el}</label>;
  const num = (k: keyof RuleForm, min?: number, max?: number, w = "w-20") => (
    <input type="number" className={`${input} ${w}`} min={min} max={max} value={form[k] as number} onChange={(e) => set(k, Number(e.target.value) as never)} />
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex items-center gap-2 border-b px-3 py-2">
        <button className={btn} onClick={onClose} disabled={phase === "running"}>← Back</button>
        <h2 className="text-base font-semibold">Bulk sign</h2>
        <span className="text-xs text-[var(--muted)]">Originals are never modified. Outputs are new files named <code>name_signed.pdf</code>.</span>
      </div>
      {error && <div role="alert" className="border-b border-amber-300 bg-amber-100 px-3 py-2 text-sm text-amber-900">{error}</div>}

      <div className="grid min-h-0 flex-1 grid-cols-1 gap-3 overflow-auto p-3 lg:grid-cols-3">
        {/* 1 files */}
        <section aria-labelledby="b1" className="flex min-h-0 flex-col card">
          <h3 id="b1" className="mb-2 font-semibold">1. Files</h3>
          <div className="mb-2 flex flex-wrap gap-2">
            <button className={btn} disabled={phase === "running"} onClick={() => guard(async () => addFiles(await pickPdfs()))}>Add files…</button>
            <button className={btn} disabled={phase === "running"} onClick={() => guard(async () => addFiles(await api.bulkPickFolder()))}>Add folder…</button>
            <button className={btn} disabled={phase === "running" || files.length === 0} onClick={() => { setFiles([]); setPreview(null); }}>Clear</button>
          </div>
          <p className="mb-1 text-sm" aria-live="polite">{files.length} file(s) selected</p>
          <ul className="min-h-24 flex-1 overflow-auto rounded border text-sm">
            {files.map((f) => (
              <li key={f} className="flex items-center gap-1 px-2 py-0.5 hover:bg-[var(--surface-2)]" title={f}>
                <span className="min-w-0 flex-1 truncate">{base(f)}</span>
                <button aria-label={`Remove ${base(f)}`} className="px-1 text-[var(--muted)]" disabled={phase === "running"} onClick={() => { setFiles((l) => l.filter((x) => x !== f)); setPreview(null); }}>✕</button>
              </li>
            ))}
          </ul>
          <div className="mt-3 space-y-2">
            <button className={btn} disabled={phase === "running"} onClick={() => guard(async () => setOutDir(await api.bulkPickOutputDir()))}>Choose output folder…</button>
            <p className="break-all text-xs text-[var(--muted)]">{outDir ?? "No output folder chosen."}</p>
            {field("Parallel workers", <input type="number" className={`${input} w-16`} min={1} max={maxWorkers} value={workers} onChange={(e) => setWorkers(Math.max(1, Math.min(64, Number(e.target.value) || 1)))} />)}
          </div>
        </section>

        {/* 2 rule */}
        <section aria-labelledby="b2" className="space-y-2 rounded border p-3">
          <h3 id="b2" className="font-semibold">2. Signing rule</h3>
          {field("Signature",
            <select className={`${input} max-w-44`} value={form.signature} onChange={(e) => set("signature", e.target.value)}>
              {library.length === 0 && <option value="">(none – create one in Fill & Sign)</option>}
              {library.map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
            </select>)}
          <div className="flex gap-3">{field("Width (pt)", num("width", 4, 2000))}{field("Height (pt)", num("height", 4, 2000))}</div>
          {field("Pages",
            <span className="flex items-center gap-1">
              <select className={input} value={form.pages} onChange={(e) => set("pages", e.target.value as RuleForm["pages"])}>
                <option value="first">First</option><option value="last">Last</option><option value="all">All</option><option value="number">Page number…</option>
              </select>
              {form.pages === "number" && num("pageNumber", 1, undefined, "w-16")}
            </span>)}

          <fieldset className="space-y-2 card">
            <legend className="px-1 text-sm">Position</legend>
            <div className="flex gap-4 text-sm" role="radiogroup" aria-label="Position type">
              <label><input type="radio" checked={form.position === "anchor"} onChange={() => set("position", "anchor")} /> Anchor text</label>
              <label><input type="radio" checked={form.position === "fixed"} onChange={() => set("position", "fixed")} /> Fixed coordinates</label>
            </div>
            {form.position === "fixed" ? (
              <>
                <div className="flex gap-3">{field("X", num("x"))}{field("Y", num("y"))}</div>
                <p className="text-xs text-[var(--muted)]">Points from the page's top-left corner (1 pt = 1/72 in). A4 is 595 × 842.</p>
              </>
            ) : (
              <>
                {field("Find text", <input className={`${input} w-40`} value={form.anchorText} onChange={(e) => set("anchorText", e.target.value)} placeholder="Signature:" />)}
                <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={form.matchCase} onChange={(e) => set("matchCase", e.target.checked)} /> Match case</label>
                {field("Occurrence", <select className={input} value={form.occurrence} onChange={(e) => set("occurrence", e.target.value as RuleForm["occurrence"])}><option value="first">First on page</option><option value="last">Last on page</option></select>)}
                {field("Place", <select className={input} value={form.side} onChange={(e) => set("side", e.target.value as RuleForm["side"])}><option value="right">Right of text</option><option value="below">Below text</option><option value="same">At text origin</option></select>)}
                <div className="flex gap-3">{field("Offset X", num("offsetX"))}{field("Offset Y", num("offsetY"))}</div>
              </>
            )}
          </fieldset>

          <fieldset className="space-y-2 card">
            <legend className="px-1 text-sm">Date / text stamp</legend>
            {field("Stamp", <select className={input} value={form.stamp} onChange={(e) => set("stamp", e.target.value as RuleForm["stamp"])}><option value="none">None</option><option value="date">Date</option><option value="text">Text</option></select>)}
            {form.stamp === "date" && field("Format", <select className={input} value={form.dateFormat} onChange={(e) => set("dateFormat", e.target.value as RuleForm["dateFormat"])}><option value="iso">2026-11-02</option><option value="dmy">02/11/2026</option><option value="mdy">11/02/2026</option></select>)}
            {form.stamp === "text" && field("Text", <input className={`${input} w-40`} value={form.stampText} onChange={(e) => set("stampText", e.target.value)} />)}
            {form.stamp !== "none" && (
              <>
                <div className="flex gap-3">{field("Size", num("stampFontSize", 4, 200, "w-16"))}</div>
                <div className="flex gap-3">{field("Offset X", num("stampOffsetX"))}{field("Offset Y", num("stampOffsetY"))}</div>
                <p className="text-xs text-[var(--muted)]">Offsets are from the signature's top-left corner.</p>
              </>
            )}
          </fieldset>
          <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={form.flatten} onChange={(e) => set("flatten", e.target.checked)} /> Flatten on save</label>
          <div className="flex gap-2">
            <button className={btn} disabled={"error" in ruleResult} onClick={() => guard(async () => { if ("rule" in ruleResult) await api.bulkRuleSave(ruleResult.rule); })}>Save rule…</button>
            <button className={btn} onClick={() => guard(async () => { const r = await api.bulkRuleLoad(); if (r) { setForm(ruleToForm(r)); setPreview(null); } })}>Load rule…</button>
          </div>
          {"error" in ruleResult && <p className="text-xs text-amber-800 dark:text-amber-300" role="status">{ruleResult.error}</p>}
        </section>

        {/* 3 preview + run */}
        <section aria-labelledby="b3" className="flex min-h-0 flex-col card">
          <h3 id="b3" className="mb-2 font-semibold">3. Preview and run</h3>
          <div className="mb-2 flex flex-wrap gap-2">
            <button className={btn} disabled={busy || phase === "running"} onClick={runPreview}>Preview rule</button>
            <button className="btn btn-primary btn-sm" disabled={busy || phase === "running" || files.length === 0 || !outDir || "error" in ruleResult} onClick={start}>Run on {files.length} file(s)</button>
            {phase === "running" && <button className={btn} onClick={() => job.current !== null && api.bulkCancel(job.current)}>Cancel</button>}
          </div>

          {phase === "edit" && preview && (
            <div className="min-h-0 flex-1 overflow-auto text-sm">
              <p aria-live="polite">Would sign {counts.signed}, skip {counts.skipped}, fail {counts.failed}.</p>
              {previewImg && <img src={previewImg} alt="Preview of the rule applied to the first signable file" className="my-2 max-h-72 rounded-lg border bg-white" />}
              {missing.length > 0 && (
                <>
                  <h4 className="mt-2 font-semibold">Files that would not be signed</h4>
                  <ul>{missing.map((m) => <li key={m.input} className="py-0.5"><span className={m.status === "failed" ? "text-[var(--danger)]" : "text-amber-600"}>{m.status}</span> {base(m.input)}: {m.detail}</li>)}</ul>
                </>
              )}
            </div>
          )}

          {(phase === "running" || phase === "done") && (
            <div className="min-h-0 flex-1 overflow-auto text-sm">
              <progress className="w-full" max={progress.total || 1} value={progress.done} aria-label="Bulk signing progress" />
              <p aria-live="polite">{progress.done} of {progress.total} processed{phase === "done" && (cancelled ? " (cancelled)" : " — finished")}</p>
              {phase === "done" && (() => { const c = summarize(items); return <p className="font-semibold">{c.signed} signed, {c.skipped} skipped, {c.failed} failed</p>; })()}
              {reportPath && <p className="break-all text-xs">Report saved: {reportPath}</p>}
              <ul className="mt-1">
                {items.map((m, i) => (
                  <li key={i} className="py-0.5">
                    <span className={m.status === "signed" ? "text-green-700" : m.status === "failed" ? "text-[var(--danger)]" : "text-amber-600"}>{m.status}</span> {base(m.input)} <span className="text-[var(--muted)]">{m.detail}</span>
                  </li>
                ))}
              </ul>
              {phase === "done" && <button className={`${btn} mt-2`} onClick={() => setPhase("edit")}>Back to settings</button>}
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
