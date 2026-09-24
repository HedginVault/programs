//! Real Jupiter v6 swaps: routes run through the mainnet Jupiter program and an SPL token-swap pool
//! the test creates. Run `fixtures/dump.sh` and `anchor build` first.
use anchor_lang::{system_program, AccountSerialize, InstructionData};
use anchor_spl::token_2022::spl_token_2022;
use hedge_vault::jupiter::{
    accounts::TokenLedger,
    client::args::{Route, RouteWithTokenLedger, SetTokenLedger},
    types::{RoutePlanStep, Swap},
};
use hedge_vault_litesvm::*;
use solana_sdk::{
    account::Account,
    instruction::{AccountMeta, Instruction},
    program_pack::Pack,
    pubkey,
    system_instruction,
};

const TOKEN_SWAP_PROGRAM: Pubkey = pubkey!("SwaPpA9LAaLfeLi3a68M4DjnLqgtticKg6CnyNwgAC8");
/// Pool fee owner the mainnet token-swap build requires.
const TOKEN_SWAP_FEE_OWNER: Pubkey = pubkey!("HfoTxFR1Tm6kGmWgYWD6J7YHVy1UwqSULUGVLXkJqaKN");
const SWAP_STATE_LEN: usize = 324;
const POOL_DEPTH: u64 = 1_000 * USDC;
const SLIPPAGE_BPS: u16 = 50;

/// Constant-product pool of `mint_a`/`mint_b` on the mainnet token-swap program.
struct Pool {
    swap: Pubkey,
    authority: Pubkey,
    token_a: Pubkey,
    token_b: Pubkey,
    pool_mint: Pubkey,
    pool_fee: Pubkey,
}

fn load_programs(ctx: &mut TestContext) {
    let fixture = |name: &str| format!("{}/fixtures/{name}.so", env!("CARGO_MANIFEST_DIR"));
    ctx.svm
        .add_program_from_file(hedge_vault::jupiter::ID, fixture("jupiter"))
        .expect("run fixtures/dump.sh first");
    ctx.svm
        .add_program_from_file(TOKEN_SWAP_PROGRAM, fixture("spl_token_swap"))
        .expect("run fixtures/dump.sh first");
}

fn token_account(ctx: &mut TestContext, mint: &Pubkey, owner: &Pubkey) -> Pubkey {
    let account = Keypair::new();
    let len = spl_token_2022::state::Account::LEN;
    let create = system_instruction::create_account(
        &ctx.admin.pubkey(),
        &account.pubkey(),
        ctx.svm.minimum_balance_for_rent_exemption(len),
        len as u64,
        &TOKEN_PROGRAM,
    );
    let initialize =
        spl_token_2022::instruction::initialize_account3(&TOKEN_PROGRAM, &account.pubkey(), mint, owner)
            .unwrap();
    ctx.send(&[create, initialize], &[&account]).unwrap();
    account.pubkey()
}

fn mint_to(ctx: &TestContext, mint: &Pubkey, account: &Pubkey, amount: u64) -> Instruction {
    spl_token_2022::instruction::mint_to(&TOKEN_PROGRAM, mint, account, &ctx.admin.pubkey(), &[], amount)
        .unwrap()
}

fn create_pool(ctx: &mut TestContext, mint_a: &Pubkey, mint_b: &Pubkey) -> Pool {
    let swap = Keypair::new();
    let (authority, bump) = Pubkey::find_program_address(&[swap.pubkey().as_ref()], &TOKEN_SWAP_PROGRAM);

    let token_a = token_account(ctx, mint_a, &authority);
    let token_b = token_account(ctx, mint_b, &authority);
    let fund = [mint_to(ctx, mint_a, &token_a, POOL_DEPTH), mint_to(ctx, mint_b, &token_b, POOL_DEPTH)];
    ctx.send(&fund, &[]).unwrap();

    let pool_mint = Keypair::new();
    let mint_len = spl_token_2022::state::Mint::LEN;
    let create_mint = system_instruction::create_account(
        &ctx.admin.pubkey(),
        &pool_mint.pubkey(),
        ctx.svm.minimum_balance_for_rent_exemption(mint_len),
        mint_len as u64,
        &TOKEN_PROGRAM,
    );
    let initialize_mint =
        spl_token_2022::instruction::initialize_mint2(&TOKEN_PROGRAM, &pool_mint.pubkey(), &authority, None, 6)
            .unwrap();
    ctx.send(&[create_mint, initialize_mint], &[&pool_mint]).unwrap();
    let pool_mint = pool_mint.pubkey();
    let pool_fee = token_account(ctx, &pool_mint, &TOKEN_SWAP_FEE_OWNER);
    let admin = ctx.admin.pubkey();
    let pool_destination = token_account(ctx, &pool_mint, &admin);

    // Initialize { nonce, fees, swap_curve }: 0.25% trade fee + 0.05% owner fee, constant product
    let mut data = vec![0u8, bump];
    for value in [25u64, 10_000, 5, 10_000, 0, 0, 20, 100] {
        data.extend_from_slice(&value.to_le_bytes());
    }
    data.push(0);
    data.extend_from_slice(&[0u8; 32]);

    let create_swap = system_instruction::create_account(
        &admin,
        &swap.pubkey(),
        ctx.svm.minimum_balance_for_rent_exemption(SWAP_STATE_LEN),
        SWAP_STATE_LEN as u64,
        &TOKEN_SWAP_PROGRAM,
    );
    let initialize = Instruction {
        program_id: TOKEN_SWAP_PROGRAM,
        accounts: vec![
            AccountMeta::new(swap.pubkey(), true),
            AccountMeta::new_readonly(authority, false),
            AccountMeta::new(token_a, false),
            AccountMeta::new(token_b, false),
            AccountMeta::new(pool_mint, false),
            AccountMeta::new(pool_fee, false),
            AccountMeta::new(pool_destination, false),
            AccountMeta::new_readonly(TOKEN_PROGRAM, false),
        ],
        data,
    };
    ctx.send(&[create_swap, initialize], &[&swap]).unwrap();

    Pool {
        swap: swap.pubkey(),
        authority,
        token_a,
        token_b,
        pool_mint,
        pool_fee,
    }
}

/// Accounts of one `Swap::TokenSwap` leg from `source` (pool side A) to `destination` (side B).
fn token_swap_leg(pool: &Pool, user: &Pubkey, source: &Pubkey, destination: &Pubkey) -> Vec<AccountMeta> {
    vec![
        AccountMeta::new_readonly(TOKEN_SWAP_PROGRAM, false),
        AccountMeta::new_readonly(TOKEN_PROGRAM, false),
        AccountMeta::new_readonly(pool.swap, false),
        AccountMeta::new_readonly(pool.authority, false),
        AccountMeta::new_readonly(*user, false),
        AccountMeta::new(*source, false),
        AccountMeta::new(pool.token_a, false),
        AccountMeta::new(pool.token_b, false),
        AccountMeta::new(*destination, false),
        AccountMeta::new(pool.pool_mint, false),
        AccountMeta::new(pool.pool_fee, false),
    ]
}

fn route_plan() -> Vec<RoutePlanStep> {
    vec![RoutePlanStep {
        swap: Swap::TokenSwap,
        percent: 100,
        input_index: 0,
        output_index: 1,
    }]
}

/// Constant-product output for `amount_in` after the pool's 0.30% fee, less 1% so the quote
/// is always met.
fn quote(amount_in: u64) -> u64 {
    let after_fee = amount_in as u128 * 9_970 / 10_000;
    let out = after_fee * POOL_DEPTH as u128 / (POOL_DEPTH as u128 + after_fee);
    (out * 99 / 100) as u64
}

struct Setup {
    ctx: TestContext,
    v: TestVault,
    target_mint: Pubkey,
    pool: Pool,
    vault_source: Pubkey,
    vault_destination: Pubkey,
}

/// Vault holding `idle` deposit tokens, a Jupiter strategy for a fresh target mint and a
/// deposit/target pool.
fn setup(idle: u64) -> Setup {
    let mut ctx = TestContext::new();
    load_programs(&mut ctx);
    let v = ctx.setup_vault();
    let target_mint = ctx.create_mint(&TOKEN_PROGRAM, None);
    ctx.jupiter_initialize_strategy(&v, &target_mint).unwrap();
    let pool = create_pool(&mut ctx, &v.deposit_mint, &target_mint);
    // vault_initialize already created the vault's deposit ATA
    let vault_source = ata(&v.address, &v.deposit_mint, &TOKEN_PROGRAM);
    let fund = mint_to(&ctx, &v.deposit_mint, &vault_source, idle);
    ctx.send(&[fund], &[]).unwrap();
    let vault_destination = ata(&v.address, &target_mint, &TOKEN_PROGRAM);

    Setup {
        ctx,
        v,
        target_mint,
        pool,
        vault_source,
        vault_destination,
    }
}

impl Setup {
    /// `jupiter_swap` relaying `swap_data`, with `extra_accounts` ahead of the pool leg.
    fn jupiter_swap_ix(&self, swap_data: Vec<u8>, amount: u64, extra_accounts: &[AccountMeta]) -> Instruction {
        let mut swap = ix(
            hedge_vault::accounts::JupiterSwap {
                authority: self.ctx.admin.pubkey(),
                config: config_pda(),
                vault: self.v.address,
                strategy: strategy_pda(&self.v.address, &self.target_mint),
                source_mint: self.v.deposit_mint,
                destination_mint: self.target_mint,
                vault_source_token_account: self.vault_source,
                vault_destination_token_account: self.vault_destination,
                system_program: system_program::ID,
                source_token_program: TOKEN_PROGRAM,
                destination_token_program: TOKEN_PROGRAM,
                associated_token_program: anchor_spl::associated_token::ID,
                event_authority: JUPITER_EVENT_AUTHORITY,
                jupiter_program: hedge_vault::jupiter::ID,
            },
            hedge_vault::instruction::JupiterSwap {
                swap_data,
                amount,
                slippage_bps: SLIPPAGE_BPS,
            },
        );
        swap.accounts.extend_from_slice(extra_accounts);
        swap.accounts.extend(token_swap_leg(
            &self.pool,
            &self.v.address,
            &self.vault_source,
            &self.vault_destination,
        ));
        swap
    }

    /// An empty Jupiter token ledger. Written directly: `create_token_ledger` funds the account
    /// with a fixed lamport amount below LiteSVM's rent-exempt minimum.
    fn create_token_ledger(&mut self) -> Pubkey {
        let ledger = Pubkey::new_unique();
        let mut data = Vec::new();
        TokenLedger {
            token_account: Pubkey::default(),
            amount: 0,
        }
        .try_serialize(&mut data)
        .unwrap();
        let account = Account {
            lamports: self.ctx.svm.minimum_balance_for_rent_exemption(data.len()),
            data,
            owner: hedge_vault::jupiter::ID,
            executable: false,
            rent_epoch: 0,
        };
        self.ctx.svm.set_account(ledger, account).unwrap();
        ledger
    }

    fn set_token_ledger_ix(&self, ledger: &Pubkey, token_account: &Pubkey) -> Instruction {
        Instruction {
            program_id: hedge_vault::jupiter::ID,
            accounts: vec![
                AccountMeta::new(*ledger, false),
                AccountMeta::new_readonly(*token_account, false),
            ],
            data: SetTokenLedger {}.data(),
        }
    }
}

#[test]
fn route_swaps_the_fixed_input_through_jupiter() {
    let mut s = setup(100 * USDC);
    let amount = 10 * USDC;
    let swap_data = Route {
        route_plan: route_plan(),
        in_amount: amount,
        quoted_out_amount: quote(amount),
        slippage_bps: SLIPPAGE_BPS,
        platform_fee_bps: 0,
    }
    .data();

    let swap = s.jupiter_swap_ix(swap_data, amount, &[]);
    s.ctx.send(&[swap], &[]).unwrap();

    assert_eq!(s.ctx.token_balance(&s.vault_source), 90 * USDC);
    assert!(s.ctx.token_balance(&s.vault_destination) >= quote(amount));
}

#[test]
fn token_ledger_route_swaps_only_what_arrived_after_the_ledger_was_set() {
    let idle = 100 * USDC;
    let mut s = setup(idle);
    let ledger = s.create_token_ledger();
    let arrived = 10 * USDC;
    let swap_data = RouteWithTokenLedger {
        route_plan: route_plan(),
        quoted_out_amount: quote(arrived),
        slippage_bps: SLIPPAGE_BPS,
        platform_fee_bps: 0,
    }
    .data();

    // the app's zap-out shape: set the ledger, receive tokens (here a mint stands in for the
    // DLMM withdrawal), then swap the difference
    let set_ledger = s.set_token_ledger_ix(&ledger, &s.vault_source);
    let receive = mint_to(&s.ctx, &s.v.deposit_mint, &s.vault_source, arrived);
    let swap = s.jupiter_swap_ix(swap_data, arrived, &[AccountMeta::new_readonly(ledger, false)]);
    s.ctx.send(&[set_ledger, receive, swap], &[]).unwrap();

    assert_eq!(s.ctx.token_balance(&s.vault_source), idle);
    assert!(s.ctx.token_balance(&s.vault_destination) >= quote(arrived));
}

#[test]
fn token_ledger_route_rejects_a_ledger_for_another_token_account() {
    let mut s = setup(100 * USDC);
    let ledger = s.create_token_ledger();
    let admin = s.ctx.admin.pubkey();
    let deposit_mint = s.v.deposit_mint;
    let other = token_account(&mut s.ctx, &deposit_mint, &admin);
    let swap_data = RouteWithTokenLedger {
        route_plan: route_plan(),
        quoted_out_amount: quote(USDC),
        slippage_bps: SLIPPAGE_BPS,
        platform_fee_bps: 0,
    }
    .data();

    let set_ledger = s.set_token_ledger_ix(&ledger, &other);
    s.ctx.send(&[set_ledger], &[]).unwrap();
    let swap = s.jupiter_swap_ix(swap_data, USDC, &[AccountMeta::new_readonly(ledger, false)]);

    assert_error(
        s.ctx.send(&[swap], &[]),
        HedgeVaultError::InvalidTokenAccountOwner,
    );
}
