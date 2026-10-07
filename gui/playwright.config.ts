import { defineConfig, devices } from "@playwright/test";

const viewport = { width: 1440, height: 900 };

/** Renders the built page against a fake bridge and saves one PNG per scene and browser in `shots/`. */
export default defineConfig({
  testDir: "e2e",
  testMatch: "*.shot.ts",
  outputDir: "test-results",
  reporter: "list",
  use: { baseURL: "http://localhost:4173" },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"], viewport, deviceScaleFactor: 1 },
    },
    {
      name: "firefox",
      use: { ...devices["Desktop Firefox"], viewport, deviceScaleFactor: 1 },
    },
    {
      name: "webkit",
      use: { ...devices["Desktop Safari"], viewport, deviceScaleFactor: 1 },
    },
  ],
  webServer: {
    command: "npx vite preview --port 4173 --strictPort",
    url: "http://localhost:4173",
    reuseExistingServer: true,
  },
});
