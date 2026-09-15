// Copies the Anchor build output into keeper/idl so the service builds without `target/`.
// Run after every `anchor build`: `yarn sync-idl`.
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const target = join(root, "..", "target");
const out = join(root, "idl");

mkdirSync(out, { recursive: true });
for (const [from, to] of [
  ["idl/hedge_vault.json", "hedge_vault.json"],
  ["types/hedge_vault.ts", "hedge_vault.ts"],
]) {
  const src = join(target, from);
  if (!existsSync(src)) {
    console.error(`missing ${src}, run \`anchor build\` first`);
    process.exit(1);
  }
  copyFileSync(src, join(out, to));
  console.log(`synced ${from} -> idl/${to}`);
}
