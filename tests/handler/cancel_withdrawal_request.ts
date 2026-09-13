import { getShareMintPda, getWithdrawalRequestPda } from "../../utils/pda";
import { VAULT } from "./params";
import { program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("cancel_withdrawal_request", async () => {
    const ix = await program.methods
      .cancelWithdrawalRequest()
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
