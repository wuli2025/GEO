import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// Tauri devUrl 固定指向 1421。端口被占用时必须直接报错；若让 Vite 自动漂到 1422，
// Tauri 仍会打开 1421，可能连上旧服务或空白页，形成很难排查的假启动。
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1421,
    strictPort: true,
    host: "0.0.0.0",
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    target: "esnext",
    minify: "esbuild",
    sourcemap: false,
  },
});
