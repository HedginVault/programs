import { PublicKey } from "@solana/web3.js";
import { getConfigPda } from "../../utils/pda";
import { log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("config_initialize", async () => {
    const args = {
      navUpdater: wallet.publicKey,
      treasuryAuthority: wallet.publicKey,
      guardian: wallet.publicKey,
      platformPerformanceFeeBps: 1000, // 10 %
      platformManagementFeeBps: 50, // 0.5 % / year
      maxNavDeviationBps: 2000, // 20 % per update
      maxEpochOutflowBps: 2000, // 20 % per epoch
      maxSlippageBps: 300, // 3 % per swap
    };

    log("Config", getConfigPda());

    const ix = await program.methods
      .configInitialize(args)
      .accounts({ admin: wallet.publicKey })
      .instruction();

    await run([ix]);
  });
});
