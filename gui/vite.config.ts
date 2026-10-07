import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [react()],
  // The server's CSP allows no data: URLs, so every font stays a file.
  build: { assetsInlineLimit: 0 },
});
