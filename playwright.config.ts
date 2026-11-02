import { defineConfig } from "@playwright/test";

// Frontend end-to-end tests run against the Vite dev server with a mocked Tauri IPC layer
// (e2e/mock-backend.ts). The real backend is covered by the Rust integration tests and by
// running the built app (see docs/MANUAL-QA.md).
export default defineConfig({
  testDir: "./e2e",
  timeout: 30_000,
  fullyParallel: true,
  reporter: [["list"]],
  use: {
    baseURL: "http://localhost:1420",
    launchOptions: process.env.PLAYWRIGHT_CHROMIUM ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM } : {},
  },
  webServer: { command: "npm run dev", url: "http://localhost:1420", reuseExistingServer: true, timeout: 60_000 },
});
