import { VAULT } from "./params";
import { log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("vault_update", async () => {
    const args = {
      performanceFeeBps: null,
      managementFeeBps: null,
      depositCap: null,
      minDeposit: null,
      minWithdrawalShares: null,
      status: { normal: {} },
      depositPaused: null,
      withdrawalPaused: null,
    };

    const ix = await program.methods
      .vaultUpdate(args)
      .accounts({ authority: wallet.publicKey, vault: VAULT })
      .instruction();

    await run([ix]);

    log("Vault", await program.account.vault.fetch(VAULT));
  });
});
