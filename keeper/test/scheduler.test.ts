import { describe, expect, it } from "vitest";
import { EPOCH_DURATION } from "../src/chain";
import { currentEpoch, decide } from "../src/scheduler";

const E = EPOCH_DURATION;

describe("scheduler", () => {
  it("computes the epoch like the program", () => {
    expect(currentEpoch(0)).toBe(0);
    expect(currentEpoch(E - 1)).toBe(0);
    expect(currentEpoch(E)).toBe(1);
    expect(currentEpoch(124266 * E + 5)).toBe(124266);
  });
  it("skips when the vault is current", () => {
    expect(decide(10, 10 * E + 1000, 300)).toEqual({ action: "skip", reason: "current" });
    expect(decide(11, 10 * E + 1000, 300)).toEqual({ action: "skip", reason: "current" });
  });
  it("waits out the offset after an epoch boundary", () => {
    expect(decide(9, 10 * E + 299, 300)).toEqual({ action: "skip", reason: "offset" });
    expect(decide(9, 10 * E + 300, 300)).toEqual({ action: "run", epoch: 10, overdue: false });
  });
  it("flags overdue when two or more epochs behind", () => {
    expect(decide(8, 10 * E + 300, 300)).toEqual({ action: "run", epoch: 10, overdue: true });
    expect(decide(0, 10 * E + 300, 300)).toEqual({ action: "run", epoch: 10, overdue: true });
  });
});
