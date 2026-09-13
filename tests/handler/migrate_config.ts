import { getConfigPda } from "../../utils/pda";
import { connection, log, program, run, wallet } from "./setup";

describe("hedge_vault", () => {
  it("migrate_config", async () => {
    const config = getConfigPda();
    log("Config size before", (await connection.getAccountInfo(config))?.data.length);

    const ix = await program.methods
      .migrateConfig()
      .accounts({ admin: wallet.publicKey })
      .instruction();

    await run([ix]);

    const account = await connection.getAccountInfo(config);
    log("Config size after", account?.data.length);
    if (account?.data.length === 352) log("Config", await program.account.config.fetch(config));
  });
});
