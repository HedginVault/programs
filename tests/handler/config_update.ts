import { getConfigPda } from "../../utils/pda";
import { log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("config_update", async () => {
    const args = {
      pendingAdmin: null,
      navUpdater: null,
      treasuryAuthority: null,
      guardian: null,
      platformPerformanceFeeBps: null,
      platformManagementFeeBps: null,
      maxNavDeviationBps: null,
      maxEpochOutflowBps: null,
      maxSlippageBps: null,
      status: { normal: {} },
    };

    const ix = await program.methods
      .configUpdate(args)
      .accounts({ admin: wallet.publicKey, config: getConfigPda() })
      .instruction();

    await run([ix]);

    log("Config", await program.account.config.fetch(getConfigPda()));
  });
});
