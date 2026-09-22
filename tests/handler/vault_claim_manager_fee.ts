import { getShareMintPda } from "../../utils/pda";
import { VAULT } from "./params";
import { log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("vault_claim_manager_fee", async () => {
    const vault = await program.account.vault.fetch(VAULT);
    log("Unclaimed manager fee shares", vault.unclaimedManagerFeeShares.toString());

    const ix = await program.methods
      .vaultClaimManagerFee()
      .accounts({
        authority: wallet.publicKey,
        vault: VAULT,
        shareMint: getShareMintPda(VAULT),
      })
      .instruction();

    await run([ix]);
  });
});
