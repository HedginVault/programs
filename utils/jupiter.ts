import { getAssociatedTokenAddressSync } from "@solana/spl-token";
import {
  AccountMeta,
  AddressLookupTableAccount,
  Connection,
  PublicKey,
  TransactionInstruction,
} from "@solana/web3.js";

const JUPITER_API_BASE_URL = process.env.JUPITER_API_BASE_URL ?? "https://lite-api.jup.ag/swap/v1";
const JUPITER_API_KEY = process.env.JUPITER_API_KEY;

const ROUTE = [229, 23, 203, 151, 122, 227, 173, 42];
const EXACT_OUT_ROUTE = [208, 51, 239, 151, 123, 43, 237, 92];
const SHARED_ACCOUNTS_ROUTE = [193, 32, 155, 51, 65, 214, 156, 129];
const SHARED_ACCOUNTS_EXACT_OUT_ROUTE = [176, 209, 105, 168, 154, 125, 69, 62];

const headers = () => ({
  "Content-Type": "application/json",
  ...(JUPITER_API_KEY ? { "x-api-key": JUPITER_API_KEY } : {}),
});

function deserializeInstruction(instruction: any): TransactionInstruction {
  return new TransactionInstruction({
    programId: new PublicKey(instruction.programId),
    keys: instruction.accounts.map((key: any) => ({
      pubkey: new PublicKey(key.pubkey),
      isSigner: key.isSigner,
      isWritable: key.isWritable,
    })),
    data: Buffer.from(instruction.data, "base64"),
  });
}

async function getAddressLookupTableAccounts(
  keys: string[],
  connection: Connection,
): Promise<AddressLookupTableAccount[]> {
  const infos = await connection.getMultipleAccountsInfo(keys.map((key) => new PublicKey(key)));

  return infos.flatMap((info, index) =>
    info
      ? [
          new AddressLookupTableAccount({
            key: new PublicKey(keys[index]),
            state: AddressLookupTableAccount.deserialize(info.data),
          }),
        ]
      : [],
  );
}

/// Reorders Jupiter's account list into what the program's `JupiterSwap::swap` expects as remaining accounts.
function extractRemainingAccounts(swapInstruction: TransactionInstruction): AccountMeta[] {
  const discriminator = Array.from(swapInstruction.data.subarray(0, 8));
  const keys = swapInstruction.keys;
  const is = (d: number[]) => d.every((v, i) => v === discriminator[i]);

  if (is(ROUTE)) return keys.slice(9);
  if (is(EXACT_OUT_ROUTE)) return keys.slice(11);
  if (is(SHARED_ACCOUNTS_ROUTE) || is(SHARED_ACCOUNTS_EXACT_OUT_ROUTE)) {
    return [keys[1], keys[4], keys[5], ...keys.slice(13)];
  }

  throw new Error(`Unknown Jupiter instruction discriminator: ${discriminator}`);
}

export async function getJupiterSwap(
  connection: Connection,
  inputMint: PublicKey,
  outputMint: PublicKey,
  amount: number,
  slippageBps: number,
  vault: PublicKey,
) {
  const quote = await fetch(
    `${JUPITER_API_BASE_URL}/quote?inputMint=${inputMint}&outputMint=${outputMint}&amount=${amount}&slippageBps=${slippageBps}&swapMode=ExactIn`,
    { headers: headers() },
  );
  if (!quote.ok) throw new Error(`Jupiter quote failed: ${await quote.text()}`);

  const outputMintTokenProgram = (await connection.getAccountInfo(outputMint))!.owner;
  const response = await fetch(`${JUPITER_API_BASE_URL}/swap-instructions`, {
    method: "POST",
    headers: headers(),
    body: JSON.stringify({
      quoteResponse: await quote.json(),
      userPublicKey: vault.toBase58(),
      destinationTokenAccount: getAssociatedTokenAddressSync(
        outputMint,
        vault,
        true,
        outputMintTokenProgram,
      ).toBase58(),
      useSharedAccounts: true,
      wrapAndUnwrapSol: false,
      dynamicSlippage: false,
    }),
  });
  if (!response.ok) throw new Error(`Jupiter swap-instructions failed: ${await response.text()}`);

  const { swapInstruction, addressLookupTableAddresses } = (await response.json()) as any;
  const instruction = deserializeInstruction(swapInstruction);

  return {
    swapData: instruction.data,
    remainingAccounts: extractRemainingAccounts(instruction),
    lookupTables: await getAddressLookupTableAccounts(addressLookupTableAddresses, connection),
  };
}
