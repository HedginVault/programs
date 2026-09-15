import { describe, expect, it } from "vitest";
import { classifySimulation, decodeAnchorError } from "../src/post";

const anchorLog = (name: string, code: number, msg: string) =>
  `Program log: AnchorError occurred. Error Code: ${name}. Error Number: ${code}. Error Message: ${msg}.`;

describe("decodeAnchorError", () => {
  it("reads the Anchor error line", () => {
    expect(decodeAnchorError([anchorLog("NavDeviationExceeded", 6044, "x")])).toEqual({
      code: "NavDeviationExceeded",
      message: expect.any(String),
    });
  });
  it("falls back to the custom program error code", () => {
    expect(decodeAnchorError(["Program failed: custom program error: 0x179a"])?.code).toBe("NavAlreadyUpdatedThisEpoch");
    expect(decodeAnchorError(["custom program error: 0x1"])?.code).toBe("Custom0x1");
    expect(decodeAnchorError(["nothing"])).toBeNull();
  });
});

describe("classifySimulation", () => {
  it("maps deviation to needs_override", () => {
    expect(classifySimulation([anchorLog("NavDeviationExceeded", 6044, "x")], { InstructionError: [1, { Custom: 6044 }] }))
      .toEqual({ status: "needs_override", error: "NavDeviationExceeded" });
  });
  it("maps already-updated to skipped", () => {
    expect(classifySimulation([anchorLog("NavAlreadyUpdatedThisEpoch", 6042, "x")], {}).status).toBe("skipped");
  });
  it("maps everything else to failed with the code or raw error", () => {
    expect(classifySimulation([anchorLog("TotalAssetsBelowIdleBalance", 6045, "x")], {}))
      .toEqual({ status: "failed", error: "TotalAssetsBelowIdleBalance" });
    expect(classifySimulation([], { InstructionError: [0, "InsufficientFunds"] }))
      .toEqual({ status: "failed", error: 'simulation:{"InstructionError":[0,"InsufficientFunds"]}' });
  });
});
