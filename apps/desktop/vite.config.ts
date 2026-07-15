import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { transform } from "esbuild";

export default defineConfig(({ command }) => ({
  plugins: [
    vue(),
    {
      name: "envweave-monaco-safari15",
      enforce: "pre",
      async transform(code, id) {
        if (
          command !== "serve" ||
          !id.includes("/monaco-editor/esm/") ||
          !id.split("?", 1)[0].endsWith(".js")
        ) {
          return null;
        }
        const result = await transform(code, {
          loader: "js",
          target: "safari15",
          format: "esm",
          sourcemap: true,
          sourcefile: id,
        });
        return { code: result.code, map: result.map };
      },
    },
  ],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  optimizeDeps: {
    exclude: ["monaco-editor"],
  },
  build: {
    target: "safari15",
    minify: "esbuild",
    sourcemap: true,
  },
}));
