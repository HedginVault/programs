import { log } from "./log";
import type { RunRecord, RunRow } from "./db";

export type AlertLevel = "info" | "warn" | "error";

export interface Alert {
  level: AlertLevel;
  reason: string;
  message: string;
  vault?: string;
  epoch?: number;
}

interface AlerterOptions {
  webhookUrl?: string;
  info: boolean;
  fetchFn?: typeof fetch;
}

export class Alerter {
  private readonly last = new Map<string, string>();
  private readonly fetchFn: typeof fetch;

  constructor(private readonly opts: AlerterOptions) {
    this.fetchFn = opts.fetchFn ?? fetch;
  }

  /** Always logs; forwards to the webhook when configured (info level only with `info: true`). */
  async send(alert: Alert): Promise<void> {
    log[alert.level](`alert:${alert.reason}`, { message: alert.message, vault: alert.vault, epoch: alert.epoch });
    if (!this.opts.webhookUrl) return;
    if (alert.level === "info" && !this.opts.info) return;
    try {
      const res = await this.fetchFn(this.opts.webhookUrl, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ...alert, ts: new Date().toISOString() }),
      });
      if (!res.ok) log.warn("alert webhook rejected", { status: res.status });
    } catch (e) {
      log.warn("alert webhook failed", { error: e instanceof Error ? e.message : String(e) });
    }
  }

  /** In-memory dedupe for process-level alerts: sends when the alert under `key` changes; `null` clears it. */
  async fire(key: string, alert: Alert | null): Promise<void> {
    if (!alert) {
      this.last.delete(key);
      return;
    }
    const fingerprint = `${alert.reason}:${alert.message}`;
    if (this.last.get(key) === fingerprint) return;
    this.last.set(key, fingerprint);
    await this.send(alert);
  }
}

/** Alert for a run outcome, deduped against the row's previous status and error (survives restarts). */
export function runAlert(record: RunRecord, previous: RunRow | null): Alert | null {
  if (previous && previous.status === record.status && (previous.last_error ?? null) === (record.error ?? null)) return null;
  const base = { vault: record.vault, epoch: record.epoch };
  switch (record.status) {
    case "posted":
      return { ...base, level: "info", reason: "posted", message: `nav_update landed: ${record.signature}` };
    case "needs_override":
      return { ...base, level: "warn", reason: "needs_override", message: `deviation bound exceeded, admin must nav_override (total_assets=${record.totalAssets})` };
    case "failed":
      return { ...base, level: "error", reason: "run_failed", message: record.error ?? "unknown" };
    default:
      return null;
  }
}
