import { EPOCH_DURATION } from "./chain";

export type Decision =
  | { action: "skip"; reason: "current" | "offset" }
  | { action: "run"; epoch: number; overdue: boolean };

/** Same arithmetic as `Vault::epoch`: unix seconds / 14 400, floored. */
export const currentEpoch = (now: number) => Math.floor(now / EPOCH_DURATION);

/**
 * A vault is due once its on-chain `nav_epoch` is behind the current epoch and the epoch started at
 * least `offsetSecs` ago (chain clock lag margin). Two or more epochs behind is overdue.
 */
export function decide(navEpoch: number, now: number, offsetSecs: number): Decision {
  const epoch = currentEpoch(now);
  if (navEpoch >= epoch) return { action: "skip", reason: "current" };
  if (now < epoch * EPOCH_DURATION + offsetSecs) return { action: "skip", reason: "offset" };
  return { action: "run", epoch, overdue: epoch - navEpoch >= 2 };
}
