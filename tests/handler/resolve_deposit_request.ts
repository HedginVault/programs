import { getConfigPda, getDepositRequestPda, getShareMintPda } from "../../utils/pda";
import { REQUEST_AUTHORITY, VAULT } from "./params";
import { fetchTokenProgram, log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("resolve_deposit_request", async () => {
    const vault = await program.account.vault.fetch(VAULT);
    const depositRequest = getDepositRequestPda(VAULT, REQUEST_AUTHORITY);
    log("Deposit request", await program.account.depositRequest.fetch(depositRequest));

    const ix = await program.methods
      .resolveDepositRequest()
      .accounts({
        resolver: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
        depositor: REQUEST_AUTHORITY,
        depositRequest,
        depositMint: vault.depositMint,
        shareMint: getShareMintPda(VAULT),
        depositMintTokenProgram: await fetchTokenProgram(vault.depositMint),
      })
      .instruction();

    await run([ix]);
  });
});
