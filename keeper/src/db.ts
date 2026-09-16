import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { Pool } from "pg";
import type { RunStatus } from "./post";
import type { Holding } from "./valuation/index";

export interface DbClient {
  query(text: string, values?: unknown[]): Promise<{ rows: any[] }>;
}

export interface RunRecord {
  vault: string;
  epoch: number;
  status: RunStatus;
  totalAssets?: bigint;
  idleBalance?: bigint;
  breakdown?: Holding[];
  navBefore: bigint;
  navAfter?: bigint;
  signature?: string;
  error?: string;
  /** Slot of the account snapshot the valuation read. */
  slot?: number;
}

export interface RunRow {
  status: string;
  last_error: string | null;
  attempts: number;
}

const str = (v: bigint | undefined) => (v === undefined ? null : v.toString());

export class Db {
  constructor(private readonly client: DbClient & { end?: () => Promise<void> }) {}

  static connect(url: string): Db {
    return new Db(new Pool({ connectionString: url }));
  }

  /** Applies every `*.sql` in `dir` (sorted by name) that is not yet in `schema_migrations`. */
  async migrate(dir: string): Promise<string[]> {
    await this.client.query(
      "create table if not exists schema_migrations (name text primary key, applied_at timestamptz not null default now())",
    );
    const done = new Set((await this.client.query("select name from schema_migrations")).rows.map((r) => r.name as string));
    const applied: string[] = [];
    for (const name of readdirSync(dir).filter((f) => f.endsWith(".sql")).sort()) {
      if (done.has(name)) continue;
      const sql = readFileSync(join(dir, name), "utf8");
      await this.client.query("begin");
      try {
        await this.client.query(sql);
        await this.client.query("insert into schema_migrations (name) values ($1)", [name]);
        await this.client.query("commit");
      } catch (e) {
        await this.client.query("rollback");
        throw e;
      }
      applied.push(name);
    }
    return applied;
  }

  /** One row per (vault, epoch); every attempt updates it. Returns the row as it was before. */
  async upsertRun(r: RunRecord): Promise<{ previous: RunRow | null }> {
    const prev = await this.client.query("select status, last_error, attempts from nav_runs where vault = $1 and epoch = $2", [r.vault, r.epoch]);
    await this.client.query(
      `insert into nav_runs (vault, epoch, status, total_assets, idle_balance, breakdown, nav_before, nav_after, signature, last_error, snapshot_slot)
       values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
       on conflict (vault, epoch) do update set
         status = excluded.status,
         attempts = nav_runs.attempts + 1,
         total_assets = coalesce(excluded.total_assets, nav_runs.total_assets),
         idle_balance = coalesce(excluded.idle_balance, nav_runs.idle_balance),
         breakdown = coalesce(excluded.breakdown, nav_runs.breakdown),
         nav_before = excluded.nav_before,
         nav_after = coalesce(excluded.nav_after, nav_runs.nav_after),
         signature = coalesce(excluded.signature, nav_runs.signature),
         last_error = excluded.last_error,
         snapshot_slot = coalesce(excluded.snapshot_slot, nav_runs.snapshot_slot),
         last_attempt_at = now()`,
      [
        r.vault,
        r.epoch,
        r.status,
        str(r.totalAssets),
        str(r.idleBalance),
        r.breakdown ? JSON.stringify(r.breakdown) : null,
        r.navBefore.toString(),
        str(r.navAfter),
        r.signature ?? null,
        r.error ?? null,
        r.slot ?? null,
      ],
    );
    return { previous: (prev.rows[0] as RunRow | undefined) ?? null };
  }

  async close(): Promise<void> {
    await this.client.end?.();
  }
}
