import { defineConfig } from "vite";

// A janela do Tauri carrega o dev server; o mesmo build roda no navegador
// (modo preview com dados falsos) para validar o design sem MSVC.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
  },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
  },
});
