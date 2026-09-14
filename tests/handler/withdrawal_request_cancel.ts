import { getShareMintPda, getWithdrawalRequestPda } from "../../utils/pda";
import { VAULT } from "./params";
import { program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("withdrawal_request_cancel", async () => {
    const ix = await program.methods
      .withdrawalRequestCancel()
      .accounts({
        withdrawer: wallet.publicKey,
        vault: VAULT,
        withdrawalRequest: getWithdrawalRequestPda(VAULT, wallet.publicKey),
        shareMint: getShareMintPda(VAULT),
      })
      .instruction();

    await run([ix]);
  });
});
