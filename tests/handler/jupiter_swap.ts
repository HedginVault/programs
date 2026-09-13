import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import { BN } from "bn.js";
import { getJupiterSwap } from "../../utils/jupiter";
import { getConfigPda, getStrategyPda } from "../../utils/pda";
import { DEPOSIT_MINT_DECIMALS, TARGET_MINT, VAULT } from "./params";
import { connection, fetchTokenProgram, program, run, wallet } from "./setup";

/// false swaps deposit mint into TARGET_MINT, true swaps the full TARGET_MINT balance back.
const SWAP_BACK = false;

describe("hedge_vault", () => {
  it("jupiter_swap", async () => {
    const slippageBps = 100; // 1 %

    const vault = await program.account.vault.fetch(VAULT);
    const [sourceMint, destinationMint] = SWAP_BACK
      ? [TARGET_MINT, vault.depositMint]
      : [vault.depositMint, TARGET_MINT];

    const vaultSourceTokenAccount = getAssociatedTokenAddressSync(
      sourceMint,
      VAULT,
      true,
      await fetchTokenProgram(sourceMint),
    );
    const amount = SWAP_BACK
      ? Number((await connection.getTokenAccountBalance(vaultSourceTokenAccount)).value.amount)
      : 1 * 10 ** DEPOSIT_MINT_DECIMALS;

    const { swapData, remainingAccounts, lookupTables } = await getJupiterSwap(
      connection,
      sourceMint,
      destinationMint,
      amount,
      slippageBps,
      VAULT,
    );

    const ix = await program.methods
      .jupiterSwap(swapData, new BN(amount), slippageBps)
      .accounts({
        authority: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
        strategy: getStrategyPda(VAULT, TARGET_MINT),
        sourceMint,
        destinationMint,
        vaultSourceTokenAccount,
        destinationTokenProgram: await fetchTokenProgram(destinationMint),
      })
      .remainingAccounts(remainingAccounts)
      .instruction();

    await run([ix], [], lookupTables);
  });
});
