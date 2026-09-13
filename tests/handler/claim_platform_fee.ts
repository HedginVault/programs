import { getConfigPda, getShareMintPda } from "../../utils/pda";
import { VAULT } from "./params";
import { log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("claim_platform_fee", async () => {
    const vault = await program.account.vault.fetch(VAULT);
    log("Unclaimed platform fee shares", vault.unclaimedPlatformFeeShares.toString());

    const ix = await program.methods
      .claimPlatformFee()
      .accounts({
        treasuryAuthority: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
        shareMint: getShareMintPda(VAULT),
      })
      .instruction();

    await run([ix]);
  });
});
