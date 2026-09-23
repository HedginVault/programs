import { PublicKey } from "@solana/web3.js";
import idl from "../target/idl/hedge_vault.json";

export const HEDGE_VAULT_PROGRAM_ID = new PublicKey(idl.address);

export const MAX_BASIS_POINTS = 10_000;
export const NAV_PRECISION = 1_000_000_000;

export const USDC_MINT = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
export const USDC_MINT_DECIMALS = 6;
export const SOL_MINT = new PublicKey("So11111111111111111111111111111111111111112");
export const WBTC_MINT = new PublicKey("3NZ9JMVBmGAqocybic2c7LQCJScmgsAZ6vQqTDzcqmJh");

export const DLMM_PROGRAM_ID = new PublicKey("LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo");
export const DLMM_EVENT_AUTHORITY = new PublicKey("D1ZN9Wj1fRSUQfCjhvnu1hqDMT7hzjzBBpi12nVniYD6");
export const MEMO_PROGRAM_ID = new PublicKey("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr");

export const JUPITER_PROGRAM_ID = new PublicKey("JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4");
export const JUPITER_EVENT_AUTHORITY = new PublicKey("D8cy77BBepLMngZx6ZukaTff5hCt1HrWyKk3Hnd9oitf");

export const PHOENIX_PROGRAM_ID = new PublicKey("EtrnLzgbS7nMMy5fbD42kXiUzGg8XQzJ972Xtk1cjWih");
export const PHOENIX_LOG_AUTHORITY = new PublicKey("GdxfTLSsdSY37G6fZoYtdGDSfgFnbT2EmRpuePZxWShS");
export const PHOENIX_GLOBAL_CONFIGURATION = new PublicKey("2zskx2iyCvb6Stg7RBZkt1f6MrF4dpYtMG3yMvKwqtUZ");
export const PHOENIX_API_URL = "https://perp-api.phoenix.trade";
export const HAWKEYE_PROGRAM_ID = new PublicKey("RiSeVw3ZjNfsaXPRb4mgaqYaEEt41pNNJoDvVh7pgQj");
export const EMBER_PROGRAM_ID = new PublicKey("EMBERpYNE6ehWmXymZZS2skiFmCa9V5dp14e1iduM5qy");
export const EMBER_STATE = new PublicKey("6ur7v6AXNpnHeEb6xuk7PyezvZ1i5GrgYyWZkNCpzbRz");
export const EMBER_VAULT = new PublicKey("FKcEb4TdPDTRuMnQDpSEPQBcrm15S73xiUD6Qf8ZLUkq");
