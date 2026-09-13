import { BN } from "bn.js";
import { getConfigPda, getManagerPda, getVaultPda } from "../../utils/pda";
import { DEPOSIT_MINT, DEPOSIT_MINT_DECIMALS } from "./params";
import { fetchTokenProgram, log, program, run, wallet } from "./setup";

const toBytes = (value: string, length: number) =>
  Array.from(Buffer.from(value.padEnd(length, "\0"), "utf8").subarray(0, length));

describe("hedge_vault", () => {
  it("initialize_vault", async () => {
    const args = {
      name: toBytes("Test Vault", 32),
      description: toBytes("QA vault for hedge_vault", 64),
      performanceFeeBps: 1000, // 10 %
      managementFeeBps: 200, // 2 % / year
      depositCap: new BN(1_000_000 * 10 ** DEPOSIT_MINT_DECIMALS),
      minDeposit: new BN(10 * 10 ** DEPOSIT_MINT_DECIMALS),
      minWithdrawalShares: new BN(0),
    };

    const config = await program.account.config.fetch(getConfigPda());
    const vault = getVaultPda(config.nextVaultId);
    log("Vault", vault);

    const ix = await program.methods
      .initializeVault(args)
      .accounts({
        authority: wallet.publicKey,
        config: getConfigPda(),
        manager: getManagerPda(wallet.publicKey),
        depositMint: DEPOSIT_MINT,
        depositMintTokenProgram: await fetchTokenProgram(DEPOSIT_MINT),
      })
      .instruction();

    await run([ix]);
  });
});
