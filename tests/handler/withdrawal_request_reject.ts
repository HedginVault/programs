import {
  TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountIdempotentInstruction,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import {
  getConfigPda,
  getShareMintPda,
  getWithdrawalRequestPda,
} from "../../utils/pda";
import { REQUEST_AUTHORITY, VAULT } from "./params";
import { program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("withdrawal_request_reject", async () => {
    const shareMint = getShareMintPda(VAULT);

    // the shares go back to the withdrawer's ATA, recreated here in case it was closed
    const createAta = createAssociatedTokenAccountIdempotentInstruction(
      wallet.publicKey,
      getAssociatedTokenAddressSync(
        shareMint,
        REQUEST_AUTHORITY,
        true,
        TOKEN_PROGRAM_ID
      ),
      REQUEST_AUTHORITY,
      shareMint,
      TOKEN_PROGRAM_ID
    );

    const ix = await program.methods
      .withdrawalRequestReject()
      .accounts({
        admin: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
        withdrawer: REQUEST_AUTHORITY,
        withdrawalRequest: getWithdrawalRequestPda(VAULT, REQUEST_AUTHORITY),
        shareMint,
      })
      .instruction();

    await run([createAta, ix]);
  });
});
