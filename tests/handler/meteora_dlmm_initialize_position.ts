import DLMM from "@meteora-ag/dlmm";
import { Keypair } from "@solana/web3.js";
import { DLMM_EVENT_AUTHORITY } from "../../utils/constants";
import { getConfigPda, getStrategyPda } from "../../utils/pda";
import { LB_PAIR, VAULT } from "./params";
import { connection, log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("meteora_dlmm_initialize_position", async () => {
    const position = Keypair.generate();
    const width = 69;

    const dlmm = await DLMM.create(connection, LB_PAIR);
    const activeBinId = (await dlmm.getActiveBin()).binId;
    const lowerBinId = activeBinId - Math.floor(width / 2);
    const upperBinId = lowerBinId + width;

    log("Position", position.publicKey);
    log("Strategy", getStrategyPda(VAULT, position.publicKey));
    log("Bin range", `${lowerBinId} .. ${upperBinId} (active ${activeBinId})`);

    const ix = await program.methods
      .meteoraDlmmInitializePosition(lowerBinId, upperBinId)
      .accounts({
        authority: wallet.publicKey,
        config: getConfigPda(),
        vault: VAULT,
        position: position.publicKey,
        lbPair: LB_PAIR,
        eventAuthority: DLMM_EVENT_AUTHORITY,
      })
      .instruction();

    await run([ix], [position]);
  });
});
