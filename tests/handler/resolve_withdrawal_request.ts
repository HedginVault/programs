import { getConfigPda, getShareMintPda, getWithdrawalRequestPda } from "../../utils/pda";
import { REQUEST_AUTHORITY, VAULT } from "./params";
import { fetchTokenProgram, log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("resolve_withdrawal_request", async () => {
    const vault = await program.account.vault.fetch(VAULT);
    const withdrawalRequest = getWithdrawalRequestPda(VAULT, REQUEST_AUTHORITY);
    log("Withdrawal request", await program.account.withdrawalRequest.fetch(withdrawalRequest));

    const ix = await program.methods
      .resolveWithdrawalRequest()
      .accounts({
        resolver: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
        withdrawer: REQUEST_AUTHORITY,
        withdrawalRequest,
        depositMint: vault.depositMint,
        shareMint: getShareMintPda(VAULT),
        depositMintTokenProgram: await fetchTokenProgram(vault.depositMint),
      })
      .instruction();

    await run([ix]);
  });
});
