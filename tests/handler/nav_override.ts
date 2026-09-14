import { BN } from "bn.js";
import { getConfigPda, getShareMintPda } from "../../utils/pda";
import { DEPOSIT_MINT_DECIMALS, VAULT } from "./params";
import { fetchTokenProgram, log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("nav_override", async () => {
    const totalAssets = new BN(50 * 10 ** DEPOSIT_MINT_DECIMALS);

    const vault = await program.account.vault.fetch(VAULT);
    log("NAV before", vault.navPerShare.toString());

    const ix = await program.methods
      .navOverride(totalAssets)
      .accounts({
        admin: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
        depositMint: vault.depositMint,
        shareMint: getShareMintPda(VAULT),
        depositMintTokenProgram: await fetchTokenProgram(vault.depositMint),
      })
      .instruction();

    await run([ix]);

    log("NAV after", (await program.account.vault.fetch(VAULT)).navPerShare.toString());
  });
});
