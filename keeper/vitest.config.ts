import { createRequire } from "node:module";
import { defineConfig } from "vitest/config";

const require = createRequire(import.meta.url);

export default defineConfig({
  // The DLMM SDK's ESM build has a broken directory import; production loads its CJS build via tsc's require.
  resolve: { alias: { "@meteora-ag/dlmm": require.resolve("@meteora-ag/dlmm") } },
  test: { environment: "node", include: ["test/**/*.test.ts"] },
});
