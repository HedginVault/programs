import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { Db, type DbClient } from "../src/db";

function fakeClient(responses: { rows: any[] }[] = []) {
  const calls: { text: string; values?: unknown[] }[] = [];
  const client: DbClient = {
    query: async (text, values) => {
      calls.push({ text, values });
      return responses.shift() ?? { rows: [] };
    },
  };
  return { client, calls };
}

describe("Db.upsertRun", () => {
  it("returns the previous row and upserts with bumped attempts", async () => {
    const { client, calls } = fakeClient([{ rows: [{ status: "failed", last_error: "x", attempts: 2 }] }, { rows: [] }]);
    const db = new Db(client);
    const { previous } = await db.upsertRun({
      vault: "v", epoch: 5, status: "posted", totalAssets: 10n, idleBalance: 3n, breakdown: [], navBefore: 1_000_000_000n, navAfter: 1_010_000_000n, signature: "sig",
    });
    expect(previous).toEqual({ status: "failed", last_error: "x", attempts: 2 });
    expect(calls[0].text).toMatch(/select status, last_error, attempts from nav_runs/i);
    expect(calls[0].values).toEqual(["v", 5]);
    expect(calls[1].text).toMatch(/insert into nav_runs/i);
    expect(calls[1].text).toMatch(/on conflict \(vault, epoch\) do update/i);
    expect(calls[1].text).toMatch(/attempts = nav_runs\.attempts \+ 1/i);
    expect(calls[1].values).toEqual(["v", 5, "posted", "10", "3", "[]", "1000000000", "1010000000", "sig", null]);
  });

  it("passes nulls for absent optional fields", async () => {
    const { client, calls } = fakeClient([{ rows: [] }, { rows: [] }]);
    const { previous } = await new Db(client).upsertRun({ vault: "v", epoch: 1, status: "failed", navBefore: 1n, error: "missing_price:m" });
    expect(previous).toBeNull();
    expect(calls[1].values).toEqual(["v", 1, "failed", null, null, null, "1", null, null, "missing_price:m"]);
  });
});

describe("Db.migrate", () => {
  it("applies unapplied files in name order inside a transaction", async () => {
    const dir = mkdtempSync(join(tmpdir(), "mig-"));
    writeFileSync(join(dir, "0002_b.sql"), "select 2;");
    writeFileSync(join(dir, "0001_a.sql"), "select 1;");
    const { client, calls } = fakeClient([{ rows: [] }, { rows: [{ name: "0001_a.sql" }] }]);
    const applied = await new Db(client).migrate(dir);
    expect(applied).toEqual(["0002_b.sql"]);
    const texts = calls.map((c) => c.text.trim().toLowerCase());
    expect(texts[0]).toMatch(/create table if not exists schema_migrations/);
    expect(texts[1]).toMatch(/select name from schema_migrations/);
    expect(texts.slice(2)).toEqual(["begin", "select 2;", "insert into schema_migrations (name) values ($1)", "commit"]);
  });
});
