//! LiteSVM harness for the Phoenix strategy, run against the real Phoenix, Ember and Hawkeye
//! programs through the Rise SDK fixture (`phoenix-rise-litesvm-test`).
//!
//! The fixture deploys the exchange at its mainnet addresses, which the vault program pins, except
//! for USDC: it wraps a fake mint. [PhoenixVaultContext::new] moves that mint to the mainnet USDC
//! address so the vault's `USDC_MINT` checks hold.
//!
//! Needs `anchor build` for `target/deploy/hedge_vault.so` and `PHOENIX_MAINNET_BPF_PROGRAMS=1`
//! to fetch and cache the Phoenix program binaries on first run.

pub use hedge_vault::{
    error::HedgeVaultError,
    events::{
        PhoenixCanonicalUnwrapped, PhoenixFundsDeposited, PhoenixFundsWithdrawn, PhoenixOrderPlaced,
    },
    protocol::phoenix::{PhoenixCancelMode, PhoenixSelfTradeBehavior, PhoenixSide},
    PhoenixLimitOrderParams, PhoenixMarketOrderParams, ProtocolStatus, Strategy, StrategyType,
    VaultStatus,
};
pub use litesvm::types::{FailedTransactionMetadata, TransactionMetadata};
pub use phoenix_rise_litesvm_test::SdkLocalnetContext;
pub use solana_pubkey::Pubkey;

use anchor_lang::{
    prelude::Pubkey as AnchorPubkey, system_program, AccountDeserialize, AnchorDeserialize,
    Discriminator, InstructionData, ToAccountMetas,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use hedge_vault::{
    accounts, instruction,
    protocol::phoenix::{
        EMBER_PROGRAM_ID, EMBER_STATE, EMBER_VAULT, HAWKEYE_PROGRAM_ID,
        PHOENIX_GLOBAL_CONFIGURATION, PHOENIX_LOG_AUTHORITY, PHOENIX_PROGRAM_ID, USDC_MINT,
    },
    ConfigInitializeArgs, ConfigUpdateArgs, VaultInitializeArgs, VaultUpdateArgs,
};
use phoenix_rise_litesvm_test::{
    decode_fixture_instruction, default_sdk_localnet_fixture, find_sdk_localnet_program_paths,
    parse_pubkey, SdkLocalnetProgram,
};
use solana_instruction::{error::InstructionError, AccountMeta, Instruction};
use solana_transaction_error::TransactionError;

pub const USDC: u64 = 1_000_000;
pub const VAULT_USDC: u64 = 10_000 * USDC;
pub const SOL: &str = "SOL";

const ADMIN_SEED: &str = "hedge-vault-admin";
const TOKEN_PROGRAM: Pubkey = Pubkey::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
const ASSOCIATED_TOKEN_PROGRAM: Pubkey =
    Pubkey::from_str_const("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
const COMPUTE_BUDGET_PROGRAM: Pubkey =
    Pubkey::from_str_const("ComputeBudget111111111111111111111111111111");

const PROGRAM_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/deploy/hedge_vault.so"
);

/// Trader header offsets, mirrored from `protocol::phoenix`.
const TRADER_QUOTE_LOT_COLLATERAL: usize = 88;
const TRADER_FLAGS: usize = 96;
const TRADER_WITHDRAW_QUEUE_NODE: usize = 108;
const TRADER_POSITION_COUNT: usize = 224;

pub fn to_anchor(key: Pubkey) -> AnchorPubkey {
    AnchorPubkey::new_from_array(key.to_bytes())
}

pub fn from_anchor(key: AnchorPubkey) -> Pubkey {
    Pubkey::new_from_array(key.to_bytes())
}

fn pda(seeds: &[&[u8]], program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(seeds, program_id).0
}

fn vault_program() -> Pubkey {
    from_anchor(hedge_vault::ID)
}

pub fn config_pda() -> Pubkey {
    pda(&[b"config"], &vault_program())
}

pub fn manager_pda(authority: &Pubkey) -> Pubkey {
    pda(&[b"manager", authority.as_ref()], &vault_program())
}

pub fn vault_pda(id: u64) -> Pubkey {
    pda(&[b"vault", &id.to_le_bytes()], &vault_program())
}

pub fn strategy_pda(vault: &Pubkey, protocol_account: &Pubkey) -> Pubkey {
    pda(
        &[b"strategy", vault.as_ref(), protocol_account.as_ref()],
        &vault_program(),
    )
}

pub fn trader_pda(authority: &Pubkey) -> Pubkey {
    pda(
        &[b"trader", authority.as_ref(), &[0, 0]],
        &from_anchor(PHOENIX_PROGRAM_ID),
    )
}

pub fn ata(owner: &Pubkey, mint: &Pubkey) -> Pubkey {
    pda(
        &[owner.as_ref(), TOKEN_PROGRAM.as_ref(), mint.as_ref()],
        &ASSOCIATED_TOKEN_PROGRAM,
    )
}

/// Builds a hedge_vault instruction from its Anchor accounts and args.
pub fn ix(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: vault_program(),
        accounts: accounts
            .to_account_metas(None)
            .into_iter()
            .map(|meta| AccountMeta {
                pubkey: from_anchor(meta.pubkey),
                is_signer: meta.is_signer,
                is_writable: meta.is_writable,
            })
            .collect(),
        data: data.data(),
    }
}

fn compute_budget_ix(units: u32) -> Instruction {
    let mut data = vec![2];
    data.extend_from_slice(&units.to_le_bytes());

    Instruction {
        program_id: COMPUTE_BUDGET_PROGRAM,
        accounts: vec![],
        data,
    }
}

/// Asserts the vault instruction (after the compute budget one) failed with `error`.
pub fn assert_error(
    result: Result<TransactionMetadata, FailedTransactionMetadata>,
    error: HedgeVaultError,
) {
    let failed = result.expect_err("transaction should have failed");
    assert_eq!(
        failed.err,
        TransactionError::InstructionError(1, InstructionError::Custom(error.into())),
        "logs: {:#?}",
        failed.meta.logs
    );
}

/// Decodes every Anchor event of type `T` the vault emitted in a transaction.
pub fn events<T: AnchorDeserialize + Discriminator>(meta: &TransactionMetadata) -> Vec<T> {
    meta.logs
        .iter()
        .filter_map(|log| log.strip_prefix("Program data: "))
        .filter_map(|data| STANDARD.decode(data).ok())
        .filter(|bytes| bytes.starts_with(T::DISCRIMINATOR))
        .map(|bytes| T::deserialize(&mut &bytes[T::DISCRIMINATOR.len()..]).unwrap())
        .collect()
}

pub fn event<T: AnchorDeserialize + Discriminator>(meta: &TransactionMetadata) -> T {
    let mut found = events::<T>(meta);
    assert_eq!(
        found.len(),
        1,
        "expected exactly one event, logs: {:#?}",
        meta.logs
    );
    found.remove(0)
}

/// Header fields of the vault's Phoenix trader account.
#[derive(Debug, Clone, Copy)]
pub struct Trader {
    pub quote_lot_collateral: i64,
    pub flags: u32,
    pub withdraw_queue_node: u32,
    pub position_count: u64,
}

/// Phoenix exchange accounts resolved from the fixture, the way a client resolves them.
#[derive(Debug, Clone)]
pub struct Exchange {
    pub canonical_mint: Pubkey,
    pub global_vault: Pubkey,
    pub perp_asset_map: Pubkey,
    pub withdraw_queue: Pubkey,
    pub tail: Vec<Pubkey>,
}

#[derive(Debug, Clone, Copy)]
pub struct Market {
    pub orderbook: Pubkey,
    pub spline_collection: Pubkey,
}

/// Rise fixture with hedge_vault loaded, a USDC vault whose idle ATA holds [VAULT_USDC], and a
/// Phoenix strategy not yet initialized.
pub struct PhoenixVaultContext {
    pub phoenix: SdkLocalnetContext,
    pub admin: Pubkey,
    pub vault: Pubkey,
    pub exchange: Exchange,
    pub trader_account: Pubkey,
    pub strategy: Pubkey,
}

impl PhoenixVaultContext {
    pub fn new() -> Self {
        let fixture = default_sdk_localnet_fixture().unwrap();
        let program_paths = find_sdk_localnet_program_paths()
            .expect("set PHOENIX_MAINNET_BPF_PROGRAMS=1 to load the Phoenix programs");
        let mut phoenix = SdkLocalnetContext::new_with_programs(
            fixture,
            program_paths,
            [SdkLocalnetProgram::new(vault_program(), PROGRAM_PATH)],
        );
        phoenix.execute_setup();

        let admin = phoenix.add_signer(ADMIN_SEED, 1_000_000_000_000);
        let fixture_addresses = phoenix.fixture.addresses.clone();
        let exchange = Exchange {
            canonical_mint: phoenix.phoenix_collateral_mint(),
            global_vault: parse_pubkey(&fixture_addresses.global_vault).unwrap(),
            perp_asset_map: parse_pubkey(&fixture_addresses.perp_asset_map).unwrap(),
            withdraw_queue: parse_pubkey(&fixture_addresses.withdraw_queue).unwrap(),
            tail: fixture_addresses
                .global_trader_index
                .iter()
                .chain(&fixture_addresses.active_trader_buffer)
                .map(|key| parse_pubkey(key).unwrap())
                .collect(),
        };

        let vault = vault_pda(0);
        let trader_account = trader_pda(&vault);
        let mut ctx = Self {
            phoenix,
            admin,
            vault,
            exchange,
            trader_account,
            strategy: strategy_pda(&vault, &trader_account),
        };

        ctx.use_mainnet_usdc();
        ctx.initialize_protocol_and_vault();
        ctx.mint_usdc(&ata(&vault, &from_anchor(USDC_MINT)), VAULT_USDC);

        ctx
    }

    /// Moves the fixture's fake USDC mint to the mainnet USDC address and repoints Ember at it.
    fn use_mainnet_usdc(&mut self) {
        let fake_usdc = self.phoenix.fake_usdc_mint();
        let usdc = from_anchor(USDC_MINT);

        let mint = self.phoenix.svm.get_account(&fake_usdc).unwrap();
        self.phoenix.svm.set_account(usdc, mint).unwrap();

        for address in [from_anchor(EMBER_STATE), from_anchor(EMBER_VAULT)] {
            let mut account = self.phoenix.svm.get_account(&address).unwrap();
            let replaced = replace_key(&mut account.data, &fake_usdc, &usdc);
            assert!(
                replaced > 0,
                "{address} does not reference the fake USDC mint"
            );
            self.phoenix.svm.set_account(address, account).unwrap();
        }
    }

    fn initialize_protocol_and_vault(&mut self) {
        let admin = to_anchor(self.admin);
        let config = to_anchor(config_pda());

        self.send(&[
            ix(
                accounts::ConfigInitialize {
                    admin,
                    config,
                    system_program: system_program::ID,
                },
                instruction::ConfigInitialize {
                    args: ConfigInitializeArgs {
                        nav_updater: admin,
                        treasury_authority: admin,
                        guardian: admin,
                        platform_performance_fee_bps: 0,
                        platform_management_fee_bps: 0,
                        max_nav_deviation_bps: 10_000,
                        max_epoch_outflow_bps: 10_000,
                        max_slippage_bps: 300,
                    },
                },
            ),
            ix(
                accounts::ConfigAddManager {
                    admin,
                    config,
                    authority: admin,
                    manager: to_anchor(manager_pda(&self.admin)),
                    system_program: system_program::ID,
                },
                instruction::ConfigAddManager {},
            ),
        ])
        .unwrap();
        self.set_protocol_status(ProtocolStatus::Normal).unwrap();

        let vault = to_anchor(self.vault);
        self.send(&[ix(
            accounts::VaultInitialize {
                authority: admin,
                config,
                manager: to_anchor(manager_pda(&self.admin)),
                vault,
                deposit_mint: USDC_MINT,
                share_mint: to_anchor(pda(&[b"share_mint", self.vault.as_ref()], &vault_program())),
                vault_token_account: to_anchor(ata(&self.vault, &from_anchor(USDC_MINT))),
                deposit_escrow: to_anchor(pda(
                    &[b"deposit_escrow", self.vault.as_ref()],
                    &vault_program(),
                )),
                share_escrow: to_anchor(pda(
                    &[b"share_escrow", self.vault.as_ref()],
                    &vault_program(),
                )),
                system_program: system_program::ID,
                deposit_mint_token_program: anchor_spl_token(),
                share_token_program: anchor_spl_token(),
                associated_token_program: to_anchor(ASSOCIATED_TOKEN_PROGRAM),
            },
            instruction::VaultInitialize {
                args: VaultInitializeArgs {
                    name: [0; 32],
                    performance_fee_bps: 0,
                    management_fee_bps: 0,
                    deposit_cap: u64::MAX,
                    min_deposit: 1,
                    min_withdrawal_shares: 1,
                },
            },
        )])
        .unwrap();
    }

    /// Mints USDC with the fixture payer, which is the fake USDC mint authority.
    pub fn mint_usdc(&mut self, token_account: &Pubkey, amount: u64) {
        let mut data = vec![7];
        data.extend_from_slice(&amount.to_le_bytes());

        self.phoenix.send_instructions(
            vec![Instruction {
                program_id: TOKEN_PROGRAM,
                accounts: vec![
                    AccountMeta::new(from_anchor(USDC_MINT), false),
                    AccountMeta::new(*token_account, false),
                    AccountMeta::new_readonly(self.phoenix.signer_pubkey("payer"), true),
                ],
                data,
            }],
            "payer",
            "mint-usdc",
        );
    }

    /// Sends vault instructions with the admin (also the vault manager) as fee payer.
    pub fn send(
        &mut self,
        instructions: &[Instruction],
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let mut all = vec![compute_budget_ix(1_400_000)];
        all.extend_from_slice(instructions);

        self.phoenix
            .try_send_instructions_with_metadata(all, ADMIN_SEED)
    }

    pub fn warp_slots(&mut self, slots: u64) {
        let slot = self.phoenix.svm.get_sysvar::<solana_clock::Clock>().slot;
        self.phoenix.svm.warp_to_slot(slot + slots);
    }

    // State readers

    pub fn token_balance(&self, address: &Pubkey) -> u64 {
        match self.phoenix.svm.get_account(address) {
            Some(account) if account.data.len() >= 72 => {
                u64::from_le_bytes(account.data[64..72].try_into().unwrap())
            }
            _ => 0,
        }
    }

    pub fn account_exists(&self, address: &Pubkey) -> bool {
        self.phoenix
            .svm
            .get_account(address)
            .is_some_and(|account| account.lamports > 0)
    }

    pub fn vault_usdc(&self) -> u64 {
        self.token_balance(&ata(&self.vault, &from_anchor(USDC_MINT)))
    }

    pub fn vault_canonical_token_account(&self) -> Pubkey {
        ata(&self.vault, &self.exchange.canonical_mint)
    }

    pub fn trader(&self) -> Trader {
        let data = self.phoenix.account_data(&self.trader_account);
        let read_u32 =
            |offset: usize| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        let read_u64 =
            |offset: usize| u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap());

        Trader {
            quote_lot_collateral: read_u64(TRADER_QUOTE_LOT_COLLATERAL) as i64,
            flags: read_u32(TRADER_FLAGS),
            withdraw_queue_node: read_u32(TRADER_WITHDRAW_QUEUE_NODE),
            position_count: read_u64(TRADER_POSITION_COUNT),
        }
    }

    /// Overwrites the trader's capability flags, to simulate a trader Phoenix has not onboarded.
    pub fn set_trader_flags(&mut self, flags: u32) {
        let mut account = self.phoenix.svm.get_account(&self.trader_account).unwrap();
        account.data[TRADER_FLAGS..TRADER_FLAGS + 4].copy_from_slice(&flags.to_le_bytes());
        self.phoenix
            .svm
            .set_account(self.trader_account, account)
            .unwrap();
    }

    pub fn strategy(&self) -> Option<Strategy> {
        let account = self.phoenix.svm.get_account(&self.strategy)?;
        Strategy::try_deserialize(&mut account.data.as_slice()).ok()
    }

    pub fn open_strategy_count(&self) -> u32 {
        let data = self.phoenix.account_data(&self.vault);
        let vault: hedge_vault::Vault = bytemuck_read(&data[8..]);
        vault.open_strategy_count
    }

    pub fn market(&self, symbol: &str) -> Market {
        let market = self.phoenix.market(symbol);

        Market {
            orderbook: parse_pubkey(&market.orderbook).unwrap(),
            spline_collection: parse_pubkey(&market.spline).unwrap(),
        }
    }

    // Admin

    pub fn set_protocol_status(
        &mut self,
        status: ProtocolStatus,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        self.send(&[ix(
            accounts::ConfigUpdate {
                admin: to_anchor(self.admin),
                config: to_anchor(config_pda()),
            },
            instruction::ConfigUpdate {
                args: ConfigUpdateArgs {
                    pending_admin: None,
                    nav_updater: None,
                    treasury_authority: None,
                    guardian: None,
                    platform_performance_fee_bps: None,
                    platform_management_fee_bps: None,
                    max_nav_deviation_bps: None,
                    max_epoch_outflow_bps: None,
                    max_slippage_bps: None,
                    status: Some(status),
                },
            },
        )])
    }

    pub fn set_vault_status(
        &mut self,
        status: VaultStatus,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        self.send(&[ix(
            accounts::VaultUpdate {
                authority: to_anchor(self.admin),
                vault: to_anchor(self.vault),
            },
            instruction::VaultUpdate {
                args: VaultUpdateArgs {
                    performance_fee_bps: None,
                    management_fee_bps: None,
                    deposit_cap: None,
                    min_deposit: None,
                    min_withdrawal_shares: None,
                    status: Some(status),
                    deposit_paused: None,
                    withdrawal_paused: None,
                },
            },
        )])
    }

    /// Registers the vault's trader straight on Phoenix, as the builder API does when onboarding
    /// runs before `phoenix_initialize_strategy`.
    pub fn register_trader_externally(&mut self) {
        let template = self.fixture_instruction("registerTaker0Trader");
        let taker = self.phoenix.actor_pubkey("taker0");
        let taker_trader = self.phoenix.actor_trader("taker0");

        let mut register = template;
        for meta in &mut register.accounts {
            if meta.pubkey == taker {
                meta.pubkey = self.vault;
            } else if meta.pubkey == taker_trader {
                meta.pubkey = self.trader_account;
            }
        }

        self.phoenix
            .send_instructions(vec![register], "payer", "register-vault-trader");
    }

    /// Replays the fixture's withdraw throttle update with a smaller budget, in quote lots.
    pub fn set_withdraw_budget(&mut self, max_budget: u64, replenish_per_slot: u64) {
        let mut update = self.fixture_instruction("updateWithdrawRateLimits");
        update.data.truncate(8);
        for value in [max_budget, replenish_per_slot] {
            update.data.push(1);
            update.data.extend_from_slice(&value.to_le_bytes());
        }

        self.phoenix
            .send_instructions(vec![update], "payer", "update-withdraw-rate-limits");
    }

    fn fixture_instruction(&self, name: &str) -> Instruction {
        let instruction = self
            .phoenix
            .fixture
            .setup_transactions
            .iter()
            .flat_map(|transaction| &transaction.instructions)
            .find(|instruction| instruction.name == name)
            .unwrap_or_else(|| panic!("missing fixture instruction {name}"));

        decode_fixture_instruction(instruction).unwrap()
    }

    /// Puts canonical tokens in the vault's canonical ATA the way the withdraw queue crank pays out
    /// a queued withdrawal: the fixture payer wraps USDC through Ember and transfers the result.
    pub fn deliver_canonical(&mut self, amount: u64) {
        let payer = self.phoenix.signer_pubkey("payer");
        let usdc = from_anchor(USDC_MINT);
        let payer_usdc = ata(&payer, &usdc);
        let payer_canonical = ata(&payer, &self.exchange.canonical_mint);

        let create_ata = |mint: Pubkey, account: Pubkey| Instruction {
            program_id: ASSOCIATED_TOKEN_PROGRAM,
            accounts: vec![
                AccountMeta::new(payer, true),
                AccountMeta::new(account, false),
                AccountMeta::new_readonly(payer, false),
                AccountMeta::new_readonly(mint, false),
                AccountMeta::new_readonly(from_anchor(system_program::ID), false),
                AccountMeta::new_readonly(TOKEN_PROGRAM, false),
            ],
            data: vec![1],
        };
        self.phoenix.send_instructions(
            vec![
                create_ata(usdc, payer_usdc),
                create_ata(self.exchange.canonical_mint, payer_canonical),
            ],
            "payer",
            "create-payer-atas",
        );
        self.mint_usdc(&payer_usdc, amount);

        let mut ember_deposit = vec![242, 35, 198, 137, 82, 225, 242, 182];
        ember_deposit.extend_from_slice(&amount.to_le_bytes());
        let mut transfer = vec![3];
        transfer.extend_from_slice(&amount.to_le_bytes());

        self.phoenix.send_instructions(
            vec![
                Instruction {
                    program_id: from_anchor(EMBER_PROGRAM_ID),
                    accounts: vec![
                        AccountMeta::new_readonly(payer, true),
                        AccountMeta::new_readonly(from_anchor(EMBER_STATE), false),
                        AccountMeta::new_readonly(usdc, false),
                        AccountMeta::new(self.exchange.canonical_mint, false),
                        AccountMeta::new(payer_usdc, false),
                        AccountMeta::new(payer_canonical, false),
                        AccountMeta::new(from_anchor(EMBER_VAULT), false),
                        AccountMeta::new_readonly(TOKEN_PROGRAM, false),
                    ],
                    data: ember_deposit,
                },
                Instruction {
                    program_id: TOKEN_PROGRAM,
                    accounts: vec![
                        AccountMeta::new(payer_canonical, false),
                        AccountMeta::new(self.vault_canonical_token_account(), false),
                        AccountMeta::new_readonly(payer, true),
                    ],
                    data: transfer,
                },
            ],
            "payer",
            "deliver-canonical",
        );
    }

    // Phoenix strategy instructions

    fn phoenix_accounts(
        &self,
    ) -> (
        AnchorPubkey,
        AnchorPubkey,
        AnchorPubkey,
        AnchorPubkey,
        AnchorPubkey,
    ) {
        (
            to_anchor(self.admin),
            to_anchor(config_pda()),
            to_anchor(self.vault),
            to_anchor(self.strategy),
            to_anchor(self.trader_account),
        )
    }

    fn with_tail(&self, mut instruction: Instruction) -> Instruction {
        instruction.accounts.extend(
            self.exchange
                .tail
                .iter()
                .map(|key| AccountMeta::new(*key, false)),
        );
        instruction
    }

    pub fn initialize_strategy_ix(&self, trader_account: Pubkey) -> Instruction {
        let (authority, config, vault, _, _) = self.phoenix_accounts();

        ix(
            accounts::PhoenixInitializeStrategy {
                authority,
                config,
                vault,
                strategy: to_anchor(strategy_pda(&self.vault, &trader_account)),
                trader_account: to_anchor(trader_account),
                canonical_mint: to_anchor(self.exchange.canonical_mint),
                vault_canonical_token_account: to_anchor(self.vault_canonical_token_account()),
                global_config: PHOENIX_GLOBAL_CONFIGURATION,
                log_authority: PHOENIX_LOG_AUTHORITY,
                phoenix_program: PHOENIX_PROGRAM_ID,
                token_program: anchor_spl_token(),
                associated_token_program: to_anchor(ASSOCIATED_TOKEN_PROGRAM),
                system_program: system_program::ID,
            },
            instruction::PhoenixInitializeStrategy {},
        )
    }

    pub fn initialize_strategy(
        &mut self,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let initialize = self.initialize_strategy_ix(self.trader_account);
        self.send(&[initialize])
    }

    pub fn deposit_ix(&self, amount: u64) -> Instruction {
        let (authority, config, vault, strategy, trader_account) = self.phoenix_accounts();

        self.with_tail(ix(
            accounts::PhoenixDepositFunds {
                authority,
                config,
                vault,
                strategy,
                usdc_mint: USDC_MINT,
                canonical_mint: to_anchor(self.exchange.canonical_mint),
                vault_usdc_token_account: to_anchor(ata(&self.vault, &from_anchor(USDC_MINT))),
                vault_canonical_token_account: to_anchor(self.vault_canonical_token_account()),
                trader_account,
                ember_state: EMBER_STATE,
                ember_vault: EMBER_VAULT,
                global_vault: to_anchor(self.exchange.global_vault),
                global_config: PHOENIX_GLOBAL_CONFIGURATION,
                log_authority: PHOENIX_LOG_AUTHORITY,
                phoenix_program: PHOENIX_PROGRAM_ID,
                ember_program: EMBER_PROGRAM_ID,
                token_program: anchor_spl_token(),
            },
            instruction::PhoenixDepositFunds { amount },
        ))
    }

    pub fn deposit(
        &mut self,
        amount: u64,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let deposit = self.deposit_ix(amount);
        self.send(&[deposit])
    }

    pub fn withdraw_ix(&self, amount: u64) -> Instruction {
        let (authority, config, vault, strategy, trader_account) = self.phoenix_accounts();

        self.with_tail(ix(
            accounts::PhoenixWithdrawFunds {
                authority,
                config,
                vault,
                strategy,
                usdc_mint: USDC_MINT,
                canonical_mint: to_anchor(self.exchange.canonical_mint),
                vault_usdc_token_account: to_anchor(ata(&self.vault, &from_anchor(USDC_MINT))),
                vault_canonical_token_account: to_anchor(self.vault_canonical_token_account()),
                trader_account,
                perp_asset_map: to_anchor(self.exchange.perp_asset_map),
                withdraw_queue: to_anchor(self.exchange.withdraw_queue),
                ember_state: EMBER_STATE,
                ember_vault: EMBER_VAULT,
                global_vault: to_anchor(self.exchange.global_vault),
                global_config: PHOENIX_GLOBAL_CONFIGURATION,
                log_authority: PHOENIX_LOG_AUTHORITY,
                phoenix_program: PHOENIX_PROGRAM_ID,
                ember_program: EMBER_PROGRAM_ID,
                token_program: anchor_spl_token(),
            },
            instruction::PhoenixWithdrawFunds { amount },
        ))
    }

    pub fn withdraw(
        &mut self,
        amount: u64,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let withdraw = self.withdraw_ix(amount);
        self.send(&[withdraw])
    }

    pub fn ember_withdraw(&mut self) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let (authority, config, vault, strategy, _) = self.phoenix_accounts();

        let unwrap = ix(
            accounts::PhoenixEmberWithdraw {
                authority,
                config,
                vault,
                strategy,
                usdc_mint: USDC_MINT,
                canonical_mint: to_anchor(self.exchange.canonical_mint),
                vault_usdc_token_account: to_anchor(ata(&self.vault, &from_anchor(USDC_MINT))),
                vault_canonical_token_account: to_anchor(self.vault_canonical_token_account()),
                ember_state: EMBER_STATE,
                ember_vault: EMBER_VAULT,
                global_config: PHOENIX_GLOBAL_CONFIGURATION,
                ember_program: EMBER_PROGRAM_ID,
                token_program: anchor_spl_token(),
            },
            instruction::PhoenixEmberWithdraw {},
        );
        self.send(&[unwrap])
    }

    pub fn market_order_ix(&self, symbol: &str, params: PhoenixMarketOrderParams) -> Instruction {
        let (authority, config, vault, strategy, trader_account) = self.phoenix_accounts();
        let market = self.market(symbol);

        self.with_tail(ix(
            accounts::PhoenixPlaceMarketOrder {
                authority,
                config,
                vault,
                strategy,
                trader_account,
                perp_asset_map: to_anchor(self.exchange.perp_asset_map),
                orderbook: to_anchor(market.orderbook),
                spline_collection: to_anchor(market.spline_collection),
                global_config: PHOENIX_GLOBAL_CONFIGURATION,
                log_authority: PHOENIX_LOG_AUTHORITY,
                phoenix_program: PHOENIX_PROGRAM_ID,
                hawkeye_program: HAWKEYE_PROGRAM_ID,
            },
            instruction::PhoenixPlaceMarketOrder { params },
        ))
    }

    pub fn market_order(
        &mut self,
        symbol: &str,
        params: PhoenixMarketOrderParams,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let order = self.market_order_ix(symbol, params);
        self.send(&[order])
    }

    pub fn limit_order(
        &mut self,
        symbol: &str,
        params: PhoenixLimitOrderParams,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let (authority, config, vault, strategy, trader_account) = self.phoenix_accounts();
        let market = self.market(symbol);

        let order = self.with_tail(ix(
            accounts::PhoenixPlaceLimitOrder {
                authority,
                config,
                vault,
                strategy,
                trader_account,
                perp_asset_map: to_anchor(self.exchange.perp_asset_map),
                orderbook: to_anchor(market.orderbook),
                spline_collection: to_anchor(market.spline_collection),
                global_config: PHOENIX_GLOBAL_CONFIGURATION,
                log_authority: PHOENIX_LOG_AUTHORITY,
                phoenix_program: PHOENIX_PROGRAM_ID,
                hawkeye_program: HAWKEYE_PROGRAM_ID,
            },
            instruction::PhoenixPlaceLimitOrder { params },
        ));
        self.send(&[order])
    }

    pub fn cancel_orders(
        &mut self,
        symbol: &str,
        mode: PhoenixCancelMode,
    ) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let (authority, config, vault, strategy, trader_account) = self.phoenix_accounts();
        let market = self.market(symbol);

        let cancel = self.with_tail(ix(
            accounts::PhoenixCancelOrders {
                authority,
                config,
                vault,
                strategy,
                trader_account,
                perp_asset_map: to_anchor(self.exchange.perp_asset_map),
                orderbook: to_anchor(market.orderbook),
                spline_collection: to_anchor(market.spline_collection),
                global_config: PHOENIX_GLOBAL_CONFIGURATION,
                log_authority: PHOENIX_LOG_AUTHORITY,
                phoenix_program: PHOENIX_PROGRAM_ID,
            },
            instruction::PhoenixCancelOrders { mode },
        ));
        self.send(&[cancel])
    }

    pub fn close_strategy(&mut self) -> Result<TransactionMetadata, FailedTransactionMetadata> {
        let (authority, config, vault, strategy, _) = self.phoenix_accounts();

        let mut close = ix(
            accounts::VaultCloseStrategy {
                authority,
                config,
                vault,
                strategy,
                system_program: system_program::ID,
            },
            instruction::VaultCloseStrategy {},
        );
        close.accounts.extend([
            AccountMeta::new_readonly(self.trader_account, false),
            AccountMeta::new_readonly(from_anchor(PHOENIX_GLOBAL_CONFIGURATION), false),
            AccountMeta::new(self.vault_canonical_token_account(), false),
            AccountMeta::new_readonly(TOKEN_PROGRAM, false),
        ]);
        self.send(&[close])
    }
}

/// Market order defaults: IOC at any price, no fill minimum, abort on self-trade.
pub fn market_order(
    side: PhoenixSide,
    num_base_lots: u64,
    reduce_only: bool,
) -> PhoenixMarketOrderParams {
    PhoenixMarketOrderParams {
        side,
        price_in_ticks: None,
        num_base_lots,
        num_quote_lots: None,
        min_base_lots_to_fill: 0,
        min_quote_lots_to_fill: 0,
        self_trade_behavior: PhoenixSelfTradeBehavior::Abort,
        match_limit: None,
        client_order_id: 0,
        last_valid_slot: None,
        reduce_only,
        cancel_existing: false,
    }
}

/// Post-only limit order defaults.
pub fn post_only_order(
    side: PhoenixSide,
    price_in_ticks: u64,
    num_base_lots: u64,
) -> PhoenixLimitOrderParams {
    PhoenixLimitOrderParams {
        side,
        price_in_ticks,
        num_base_lots,
        post_only: true,
        slide: false,
        self_trade_behavior: PhoenixSelfTradeBehavior::Abort,
        match_limit: None,
        client_order_id: 0,
        last_valid_slot: None,
        reduce_only: false,
        cancel_existing: false,
    }
}

fn anchor_spl_token() -> AnchorPubkey {
    to_anchor(TOKEN_PROGRAM)
}

fn replace_key(data: &mut [u8], from: &Pubkey, to: &Pubkey) -> usize {
    let mut replaced = 0;
    let mut offset = 0;

    while offset + 32 <= data.len() {
        if data[offset..offset + 32] == from.to_bytes() {
            data[offset..offset + 32].copy_from_slice(&to.to_bytes());
            replaced += 1;
            offset += 32;
        } else {
            offset += 1;
        }
    }

    replaced
}

fn bytemuck_read<T: bytemuck::Pod>(data: &[u8]) -> T {
    bytemuck::pod_read_unaligned(&data[..core::mem::size_of::<T>()])
}
