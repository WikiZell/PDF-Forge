#!/usr/bin/env node
// Downloads the pinned PDFium build for a target into src-tauri/resources/pdfium/.
// Usage: node scripts/fetch-pdfium.mjs [target]     (default: win-x64)
// Source: https://github.com/bblanchon/pdfium-binaries (BSD-3-Clause PDFium builds).
// The release tag must match the `pdfium_*` feature of the pdfium-render crate.
import { createHash } from "node:crypto";
import { existsSync, mkdirSync, readFileSync, copyFileSync, rmSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const TAG = "chromium/8125";

// Which binary goes with which OS/architecture. Only win-x64 is wired for shipping.
const TARGETS = {
  "win-x64": {
    archive: "pdfium-win-x64.tgz",
    sha256: "9a964261760cff52570aabf038ab346d3a5034dd8b30d0d5fe8861235d356b8b",
    file: "bin/pdfium.dll",
    wired: true,
  },
  "linux-x64": {
    archive: "pdfium-linux-x64.tgz",
    sha256: "e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659",
    file: "lib/libpdfium.so",
    wired: false, // dev/CI testing only until Linux packaging is added
  },
  "mac-x64": { archive: "pdfium-mac-x64.tgz", file: "lib/libpdfium.dylib", wired: false },
  "mac-arm64": { archive: "pdfium-mac-arm64.tgz", file: "lib/libpdfium.dylib", wired: false },
};

const target = process.argv[2] ?? "win-x64";
const t = TARGETS[target];
if (!t) throw new Error(`Unknown target "${target}". Known: ${Object.keys(TARGETS).join(", ")}`);
if (!t.sha256) throw new Error(`Target "${target}" is not wired up yet (no pinned checksum).`);

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "src-tauri", "resources", "pdfium");
const outFile = join(outDir, t.file.split("/").pop());
if (existsSync(outFile)) {
  console.log(`PDFium already present: ${outFile}`);
  process.exit(0);
}

const url = `https://github.com/bblanchon/pdfium-binaries/releases/download/${encodeURIComponent(TAG)}/${t.archive}`;
console.log(`Downloading ${url}`);
const res = await fetch(url);
if (!res.ok) throw new Error(`Download failed: HTTP ${res.status}`);
const buf = Buffer.from(await res.arrayBuffer());
const digest = createHash("sha256").update(buf).digest("hex");
if (digest !== t.sha256) throw new Error(`Checksum mismatch for ${t.archive}: got ${digest}`);

const tmp = mkdtempSync(join(tmpdir(), "pdfium-"));
try {
  const tgz = join(tmp, t.archive);
  writeFileSync(tgz, buf);
  // bsdtar ships with Windows 10+, macOS and the CI runners; GNU tar on Linux.
  execFileSync("tar", ["-xzf", tgz, "-C", tmp, t.file, "LICENSE"], { stdio: "inherit" });
  mkdirSync(outDir, { recursive: true });
  copyFileSync(join(tmp, t.file), outFile);
  copyFileSync(join(tmp, "LICENSE"), join(outDir, "PDFIUM-LICENSE.txt"));
  console.log(`Installed ${outFile}`);
} finally {
  rmSync(tmp, { recursive: true, force: true });
}
