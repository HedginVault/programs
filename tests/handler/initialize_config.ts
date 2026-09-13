import { PublicKey } from "@solana/web3.js";
import { getConfigPda } from "../../utils/pda";
import { log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("initialize_config", async () => {
    const args = {
      navUpdater: wallet.publicKey,
      treasuryAuthority: wallet.publicKey,
      guardian: wallet.publicKey,
      platformPerformanceFeeBps: 1000, // 10 %
      platformManagementFeeBps: 50, // 0.5 % / year
      maxNavDeviationBps: 2000, // 20 % per update
      maxEpochOutflowBps: 2000, // 20 % per epoch
    };

    log("Config", getConfigPda());

    const ix = await program.methods
      .initializeConfig(args)
      .accounts({ admin: wallet.publicKey })
      .instruction();

    await run([ix]);
  });
});
