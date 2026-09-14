import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { BN } from "bn.js";
import { getConfigPda, getShareMintPda } from "../../utils/pda";
import { VAULT } from "./params";
import {
  connection,
  fetchTokenProgram,
  log,
  program,
  run,
  wallet,
} from "./setup";

describe("hedge_vault", () => {
  it("nav_update", async () => {
    const vault = await program.account.vault.fetch(VAULT);
    const depositMintTokenProgram = await fetchTokenProgram(vault.depositMint);

    // total assets in deposit mint base units. Defaults to the idle balance so a fresh vault
    // never records phantom assets; set TOTAL_ASSETS to include funds deployed in strategies.
    const vaultTokenAccount = getAssociatedTokenAddressSync(
      vault.depositMint,
      VAULT,
      true,
      depositMintTokenProgram
    );
    const idleBalance = new BN(
      (await connection.getTokenAccountBalance(vaultTokenAccount)).value.amount
    );
    const totalAssets = process.env.TOTAL_ASSETS
      ? new BN(process.env.TOTAL_ASSETS)
      : idleBalance;

    log("Idle balance", idleBalance.toString());
    log("Total assets", totalAssets.toString());
    log("NAV before", vault.navPerShare.toString());

    const ix = await program.methods
      .navUpdate(totalAssets)
      .accounts({
        navUpdater: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
        depositMint: vault.depositMint,
        shareMint: getShareMintPda(VAULT),
        depositMintTokenProgram,
      })
      .instruction();

    await run([ix]);

    log(
      "NAV after",
      (await program.account.vault.fetch(VAULT)).navPerShare.toString()
    );
  });
});
