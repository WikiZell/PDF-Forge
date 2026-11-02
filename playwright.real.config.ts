import { defineConfig } from "@playwright/test";

// Drives the REAL installed app (Windows + WebView2) over the Chrome DevTools Protocol.
// Requires APP_EXE (installed pdf-forge.exe) and SAMPLES_DIR (sample PDFs). No dev server.
export default defineConfig({
  testDir: "./e2e-real",
  timeout: 60_000,
  workers: 1,
  fullyParallel: false,
  reporter: [["list"]],
  use: { screenshot: "off" },
});
