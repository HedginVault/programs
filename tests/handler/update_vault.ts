import { VAULT } from "./params";
import { log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("update_vault", async () => {
    const args = {
      description: null,
      performanceFeeBps: null,
      managementFeeBps: null,
      depositCap: null,
      minDeposit: null,
      minWithdrawalShares: null,
      status: { normal: {} },
    };

    const ix = await program.methods
      .updateVault(args)
      .accounts({ authority: wallet.publicKey, vault: VAULT })
      .instruction();

    await run([ix]);

    log("Vault", await program.account.vault.fetch(VAULT));
  });
});
