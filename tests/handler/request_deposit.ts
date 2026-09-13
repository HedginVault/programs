import { BN } from "bn.js";
import { getConfigPda, getDepositRequestPda, getShareMintPda } from "../../utils/pda";
import { DEPOSIT_MINT_DECIMALS, VAULT } from "./params";
import { fetchTokenProgram, log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("request_deposit", async () => {
    const amount = new BN(10 * 10 ** DEPOSIT_MINT_DECIMALS);

    const vault = await program.account.vault.fetch(VAULT);
    log("Deposit request", getDepositRequestPda(VAULT, wallet.publicKey));

    const ix = await program.methods
      .requestDeposit(amount)
      .accounts({
        depositor: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
        depositMint: vault.depositMint,
        shareMint: getShareMintPda(VAULT),
        depositMintTokenProgram: await fetchTokenProgram(vault.depositMint),
      })
      .instruction();

    await run([ix]);
  });
});
