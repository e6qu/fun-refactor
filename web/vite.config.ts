import { defineConfig } from "vite";

// Built into the published site at docs/playground, so `base: "./"` keeps every
// asset URL relative and the same bundle works wherever Pages serves it from.
export default defineConfig({
  base: "./",
  build: {
    outDir: "../docs/playground",
    emptyOutDir: true,
    // Monaco's editor core is one large chunk, and the wasm module is larger still.
    // Both are the point; raise the advisory limit rather than warn every build.
    chunkSizeWarningLimit: 12000,
    target: "es2022",
  },
  // The wasm is an asset, not something to inline or transform.
  assetsInclude: ["**/*.wasm"],
  server: { fs: { allow: [".."] } },
});
