//! LiteSVM harness for hedge_vault integration tests.
//! Loads `target/deploy/hedge_vault.so`, so run `anchor build` before `cargo test`.

pub use anchor_lang::prelude::Pubkey;
pub use anchor_spl::{token::ID as TOKEN_PROGRAM, token_2022::ID as TOKEN_2022_PROGRAM};
pub use hedge_vault::{error::HedgeVaultError, ProtocolStatus, Strategy, VaultStatus};
pub use solana_sdk::{signature::Keypair, signer::Signer};

use anchor_lang::{system_program, AccountDeserialize, InstructionData, ToAccountMetas};
use anchor_spl::{
    associated_token::{
        get_associated_token_address_with_program_id,
        spl_associated_token_account::instruction::create_associated_token_account,
    },
    token_2022::spl_token_2022::{
        self,
        extension::{transfer_fee::instruction::initialize_transfer_fee_config, ExtensionType},
        state::Mint,
    },
};
use hedge_vault::{
    accounts, instruction, Config, DepositRequest, ConfigInitializeArgs, VaultInitializeArgs,
    ConfigUpdateArgs, VaultUpdateArgs, Vault, WithdrawalRequest,
};
use litesvm::{types::TransactionResult, LiteSVM};
use solana_sdk::{
    clock::Clock,
    instruction::{AccountMeta, Instruction, InstructionError},
    system_instruction,
    transaction::{Transaction, TransactionError},
};

pub const DAY: i64 = 86_400;
pub const USDC: u64 = 1_000_000;
pub const START_TS: i64 = 100 * DAY;

/// `anchor_lang::error::ErrorCode::ConstraintAssociated`, raised when an account fails an
/// `associated_token::` constraint.
pub const ANCHOR_CONSTRAINT_ASSOCIATED: u32 =
    anchor_lang::error::ErrorCode::ConstraintAssociated as u32;

/// Mirrors `protocol::jupiter::JUPITER_AGGREGATOR_EVENT_AUTHORITY`.
pub const JUPITER_EVENT_AUTHORITY: Pubkey =
    hedge_vault::protocol::jupiter::JUPITER_AGGREGATOR_EVENT_AUTHORITY;

const PROGRAM_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/deploy/hedge_vault.so"
);

// PDAs

pub fn pda(seeds: &[&[u8]]) -> Pubkey {
    Pubkey::find_program_address(seeds, &hedge_vault::ID).0
}

pub fn config_pda() -> Pubkey {
    pda(&[b"config"])
}

pub fn manager_pda(authority: &Pubkey) -> Pubkey {
    pda(&[b"manager", authority.as_ref()])
}

pub fn vault_pda(id: u64) -> Pubkey {
    pda(&[b"vault", &id.to_le_bytes()])
}

pub fn share_mint_pda(vault: &Pubkey) -> Pubkey {
    pda(&[b"share_mint", vault.as_ref()])
}

pub fn deposit_escrow_pda(vault: &Pubkey) -> Pubkey {
    pda(&[b"deposit_escrow", vault.as_ref()])
}

pub fn share_escrow_pda(vault: &Pubkey) -> Pubkey {
    pda(&[b"share_escrow", vault.as_ref()])
}

pub fn deposit_request_pda(vault: &Pubkey, authority: &Pubkey) -> Pubkey {
    pda(&[b"deposit_request", vault.as_ref(), authority.as_ref()])
}

pub fn withdrawal_request_pda(vault: &Pubkey, authority: &Pubkey) -> Pubkey {
    pda(&[b"withdrawal_request", vault.as_ref(), authority.as_ref()])
}

/// `protocol_account` is the Jupiter target mint or the DLMM position.
pub fn strategy_pda(vault: &Pubkey, protocol_account: &Pubkey) -> Pubkey {
    pda(&[b"strategy", vault.as_ref(), protocol_account.as_ref()])
}

pub fn ata(owner: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
    get_associated_token_address_with_program_id(owner, mint, token_program)
}

pub fn ix(accounts: impl ToAccountMetas, data: impl InstructionData) -> Instruction {
    Instruction {
        program_id: hedge_vault::ID,
        accounts: accounts.to_account_metas(None),
        data: data.data(),
    }
}

/// Asserts the transaction failed in its first instruction with `error`.
pub fn assert_error(result: TransactionResult, error: HedgeVaultError) {
    assert_error_code(result, error.into());
}

/// Asserts the transaction failed in its first instruction with a raw custom error code,
/// used for Anchor's own constraint errors.
pub fn assert_error_code(result: TransactionResult, code: u32) {
    let failed = result.expect_err("transaction should have failed");
    assert_eq!(
        failed.err,
        TransactionError::InstructionError(0, InstructionError::Custom(code)),
        "logs: {:#?}",
        failed.meta.logs
    );
}

#[derive(Clone, Copy)]
pub struct TestVault {
    pub address: Pubkey,
    pub deposit_mint: Pubkey,
    pub deposit_token_program: Pubkey,
    pub share_mint: Pubkey,
}

/// VM with the program loaded. The admin keypair pays every transaction and holds
/// every config role (admin, nav updater, treasury, guardian) and is a whitelisted manager.
pub struct TestContext {
    pub svm: LiteSVM,
    pub admin: Keypair,
}

impl TestContext {
    pub fn new() -> Self {
        let mut svm = LiteSVM::new();
        svm.add_program_from_file(hedge_vault::ID, PROGRAM_PATH)
            .expect("run `anchor build` first");

        let mut clock = svm.get_sysvar::<Clock>();
        clock.unix_timestamp = START_TS;
        svm.set_sysvar::<Clock>(&clock);

        let admin = Keypair::new();
        svm.airdrop(&admin.pubkey(), 100_000_000_000).unwrap();

        let mut ctx = Self { svm, admin };
        let admin = ctx.admin.pubkey();

        let config_initialize = ix(
            accounts::ConfigInitialize {
                admin,
                config: config_pda(),
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
        );
        ctx.send(&[config_initialize], &[]).unwrap();

        let mut unpause = Self::update_config_args();
        unpause.status = Some(ProtocolStatus::Normal);
        ctx.config_update(unpause).unwrap();

        let config_add_manager = ix(
            accounts::ConfigAddManager {
                admin,
                config: config_pda(),
                authority: admin,
                manager: manager_pda(&admin),
                system_program: system_program::ID,
            },
            instruction::ConfigAddManager {},
        );
        ctx.send(&[config_add_manager], &[]).unwrap();

        ctx
    }

    /// Signs with the admin as fee payer plus `signers`, then expires the blockhash so
    /// an identical transaction can be sent again.
    pub fn send(&mut self, instructions: &[Instruction], signers: &[&Keypair]) -> TransactionResult {
        let mut all_signers = vec![&self.admin];
        all_signers.extend_from_slice(signers);

        let tx = Transaction::new_signed_with_payer(
            instructions,
            Some(&self.admin.pubkey()),
            &all_signers,
            self.svm.latest_blockhash(),
        );
        let result = self.svm.send_transaction(tx);
        self.svm.expire_blockhash();

        result
    }

    pub fn new_user(&mut self) -> Keypair {
        let user = Keypair::new();
        self.svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();
        user
    }

    pub fn warp_days(&mut self, days: i64) {
        let mut clock = self.svm.get_sysvar::<Clock>();
        clock.unix_timestamp += days * DAY;
        self.svm.set_sysvar::<Clock>(&clock);
    }

    /// 6-decimal mint with the admin as mint authority. `transfer_fee_bps` adds a
    /// Token-2022 TransferFeeConfig extension (use with `TOKEN_2022_PROGRAM`).
    pub fn create_mint(&mut self, token_program: &Pubkey, transfer_fee_bps: Option<u16>) -> Pubkey {
        let mint = Keypair::new();
        let admin = self.admin.pubkey();

        let extensions: &[ExtensionType] = match transfer_fee_bps {
            Some(_) => &[ExtensionType::TransferFeeConfig],
            None => &[],
        };
        let space = ExtensionType::try_calculate_account_len::<Mint>(extensions).unwrap();

        let mut instructions = vec![system_instruction::create_account(
            &admin,
            &mint.pubkey(),
            self.svm.minimum_balance_for_rent_exemption(space),
            space as u64,
            token_program,
        )];
        if let Some(bps) = transfer_fee_bps {
            instructions.push(
                initialize_transfer_fee_config(token_program, &mint.pubkey(), None, None, bps, u64::MAX)
                    .unwrap(),
            );
        }
        instructions.push(
            spl_token_2022::instruction::initialize_mint2(token_program, &mint.pubkey(), &admin, None, 6)
                .unwrap(),
        );

        self.send(&instructions, &[&mint]).unwrap();

        mint.pubkey()
    }

    /// Creates `owner`'s ATA for `mint` and mints `amount` into it.
    pub fn fund(&mut self, owner: &Pubkey, mint: &Pubkey, token_program: &Pubkey, amount: u64) -> Pubkey {
        let account = ata(owner, mint, token_program);
        let admin = self.admin.pubkey();

        let create = create_associated_token_account(&admin, owner, mint, token_program);
        let mint_to =
            spl_token_2022::instruction::mint_to(token_program, mint, &account, &admin, &[], amount)
                .unwrap();
        self.send(&[create, mint_to], &[]).unwrap();

        account
    }

    pub fn token_balance(&self, address: &Pubkey) -> u64 {
        match self.svm.get_account(address) {
            Some(account) if account.data.len() >= 72 => {
                u64::from_le_bytes(account.data[64..72].try_into().unwrap())
            }
            _ => 0,
        }
    }

    pub fn strategy(&self, address: &Pubkey) -> Option<Strategy> {
        let account = self.svm.get_account(address)?;
        Strategy::try_deserialize(&mut account.data.as_slice()).ok()
    }

    pub fn config(&self) -> Config {
        let data = self.svm.get_account(&config_pda()).unwrap().data;
        bytemuck::pod_read_unaligned(&data[8..8 + core::mem::size_of::<Config>()])
    }

    pub fn vault(&self, vault: &Pubkey) -> Vault {
        let data = self.svm.get_account(vault).unwrap().data;
        bytemuck::pod_read_unaligned(&data[8..8 + core::mem::size_of::<Vault>()])
    }

    pub fn deposit_request(&self, address: &Pubkey) -> Option<DepositRequest> {
        let account = self.svm.get_account(address)?;
        DepositRequest::try_deserialize(&mut account.data.as_slice()).ok()
    }

    pub fn withdrawal_request(&self, address: &Pubkey) -> Option<WithdrawalRequest> {
        let account = self.svm.get_account(address)?;
        WithdrawalRequest::try_deserialize(&mut account.data.as_slice()).ok()
    }

    // Config

    pub fn update_config_args() -> ConfigUpdateArgs {
        ConfigUpdateArgs {
            pending_admin: None,
            nav_updater: None,
            treasury_authority: None,
            guardian: None,
            platform_performance_fee_bps: None,
            platform_management_fee_bps: None,
            max_nav_deviation_bps: None,
            max_epoch_outflow_bps: None,
            max_slippage_bps: None,
            status: None,
        }
    }

    pub fn config_update(&mut self, args: ConfigUpdateArgs) -> TransactionResult {
        let update = ix(
            accounts::ConfigUpdate {
                admin: self.admin.pubkey(),
                config: config_pda(),
            },
            instruction::ConfigUpdate { args },
        );
        self.send(&[update], &[])
    }

    pub fn admin_accept(&mut self, signer: &Keypair) -> TransactionResult {
        let accept = ix(
            accounts::AdminAccept {
                pending_admin: signer.pubkey(),
                config: config_pda(),
            },
            instruction::AdminAccept {},
        );
        self.send(&[accept], &[signer])
    }

    // Vault

    pub fn vault_args() -> VaultInitializeArgs {
        VaultInitializeArgs {
            name: [0; 32],
            performance_fee_bps: 0,
            management_fee_bps: 0,
            deposit_cap: 1_000_000 * USDC,
            min_deposit: 1,
            min_withdrawal_shares: 1,
        }
    }

    pub fn update_vault_args() -> VaultUpdateArgs {
        VaultUpdateArgs {
            performance_fee_bps: None,
            management_fee_bps: None,
            deposit_cap: None,
            min_deposit: None,
            min_withdrawal_shares: None,
            status: None,
        }
    }

    pub fn vault_initialize(
        &mut self,
        deposit_mint: Pubkey,
        deposit_token_program: Pubkey,
        args: VaultInitializeArgs,
    ) -> (TestVault, TransactionResult) {
        let admin = self.admin.pubkey();
        let address = vault_pda(self.config().next_vault_id);
        let share_mint = share_mint_pda(&address);

        let initialize = ix(
            accounts::VaultInitialize {
                authority: admin,
                config: config_pda(),
                manager: manager_pda(&admin),
                vault: address,
                deposit_mint,
                share_mint,
                vault_token_account: ata(&address, &deposit_mint, &deposit_token_program),
                deposit_escrow: deposit_escrow_pda(&address),
                share_escrow: share_escrow_pda(&address),
                system_program: system_program::ID,
                deposit_mint_token_program: deposit_token_program,
                share_token_program: TOKEN_PROGRAM,
                associated_token_program: anchor_spl::associated_token::ID,
            },
            instruction::VaultInitialize { args },
        );
        let result = self.send(&[initialize], &[]);

        let vault = TestVault {
            address,
            deposit_mint,
            deposit_token_program,
            share_mint,
        };
        (vault, result)
    }

    /// Vault with a fresh SPL Token deposit mint and default args.
    pub fn setup_vault(&mut self) -> TestVault {
        let mint = self.create_mint(&TOKEN_PROGRAM, None);
        let (vault, result) = self.vault_initialize(mint, TOKEN_PROGRAM, Self::vault_args());
        result.unwrap();
        vault
    }

    pub fn vault_update(&mut self, v: &TestVault, args: VaultUpdateArgs) -> TransactionResult {
        let update = ix(
            accounts::VaultUpdate {
                authority: self.admin.pubkey(),
                vault: v.address,
            },
            instruction::VaultUpdate { args },
        );
        self.send(&[update], &[])
    }

    pub fn nav_update(&mut self, v: &TestVault, total_assets: u64) -> TransactionResult {
        let update = ix(
            accounts::NavUpdate {
                nav_updater: self.admin.pubkey(),
                config: config_pda(),
                vault: v.address,
                deposit_mint: v.deposit_mint,
                share_mint: v.share_mint,
                vault_token_account: ata(&v.address, &v.deposit_mint, &v.deposit_token_program),
                deposit_mint_token_program: v.deposit_token_program,
            },
            instruction::NavUpdate { total_assets },
        );
        self.send(&[update], &[])
    }

    // Requests

    pub fn deposit_request_create(&mut self, v: &TestVault, user: &Keypair, amount: u64) -> TransactionResult {
        let depositor = user.pubkey();
        let request = ix(
            accounts::DepositRequestCreate {
                depositor,
                config: config_pda(),
                vault: v.address,
                deposit_request: deposit_request_pda(&v.address, &depositor),
                deposit_mint: v.deposit_mint,
                share_mint: v.share_mint,
                depositor_token_account: ata(&depositor, &v.deposit_mint, &v.deposit_token_program),
                depositor_share_token_account: ata(&depositor, &v.share_mint, &TOKEN_PROGRAM),
                deposit_escrow: deposit_escrow_pda(&v.address),
                system_program: system_program::ID,
                deposit_mint_token_program: v.deposit_token_program,
                share_token_program: TOKEN_PROGRAM,
                associated_token_program: anchor_spl::associated_token::ID,
            },
            instruction::DepositRequestCreate { amount },
        );
        self.send(&[request], &[user])
    }

    pub fn deposit_request_cancel(&mut self, v: &TestVault, user: &Keypair) -> TransactionResult {
        let depositor = user.pubkey();
        let cancel = ix(
            accounts::DepositRequestCancel {
                depositor,
                vault: v.address,
                deposit_request: deposit_request_pda(&v.address, &depositor),
                deposit_mint: v.deposit_mint,
                depositor_token_account: ata(&depositor, &v.deposit_mint, &v.deposit_token_program),
                deposit_escrow: deposit_escrow_pda(&v.address),
                deposit_mint_token_program: v.deposit_token_program,
                system_program: system_program::ID,
            },
            instruction::DepositRequestCancel {},
        );
        self.send(&[cancel], &[user])
    }

    pub fn deposit_request_resolve(&mut self, v: &TestVault, depositor: &Pubkey) -> TransactionResult {
        let resolve = ix(
            accounts::DepositRequestResolve {
                resolver: self.admin.pubkey(),
                config: config_pda(),
                vault: v.address,
                depositor: *depositor,
                deposit_request: deposit_request_pda(&v.address, depositor),
                deposit_mint: v.deposit_mint,
                share_mint: v.share_mint,
                depositor_share_token_account: ata(depositor, &v.share_mint, &TOKEN_PROGRAM),
                vault_token_account: ata(&v.address, &v.deposit_mint, &v.deposit_token_program),
                deposit_escrow: deposit_escrow_pda(&v.address),
                deposit_mint_token_program: v.deposit_token_program,
                share_token_program: TOKEN_PROGRAM,
                system_program: system_program::ID,
            },
            instruction::DepositRequestResolve {},
        );
        self.send(&[resolve], &[])
    }

    pub fn withdrawal_request_create(&mut self, v: &TestVault, user: &Keypair, shares: u64) -> TransactionResult {
        let withdrawer = user.pubkey();
        let request = ix(
            accounts::WithdrawalRequestCreate {
                withdrawer,
                config: config_pda(),
                vault: v.address,
                withdrawal_request: withdrawal_request_pda(&v.address, &withdrawer),
                deposit_mint: v.deposit_mint,
                share_mint: v.share_mint,
                withdrawer_share_token_account: ata(&withdrawer, &v.share_mint, &TOKEN_PROGRAM),
                withdrawer_token_account: ata(&withdrawer, &v.deposit_mint, &v.deposit_token_program),
                share_escrow: share_escrow_pda(&v.address),
                system_program: system_program::ID,
                deposit_mint_token_program: v.deposit_token_program,
                share_token_program: TOKEN_PROGRAM,
                associated_token_program: anchor_spl::associated_token::ID,
            },
            instruction::WithdrawalRequestCreate { shares },
        );
        self.send(&[request], &[user])
    }

    pub fn withdrawal_request_resolve(&mut self, v: &TestVault, withdrawer: &Pubkey) -> TransactionResult {
        let resolve = ix(
            accounts::WithdrawalRequestResolve {
                resolver: self.admin.pubkey(),
                config: config_pda(),
                vault: v.address,
                withdrawer: *withdrawer,
                withdrawal_request: withdrawal_request_pda(&v.address, withdrawer),
                deposit_mint: v.deposit_mint,
                share_mint: v.share_mint,
                withdrawer_token_account: ata(withdrawer, &v.deposit_mint, &v.deposit_token_program),
                vault_token_account: ata(&v.address, &v.deposit_mint, &v.deposit_token_program),
                share_escrow: share_escrow_pda(&v.address),
                deposit_mint_token_program: v.deposit_token_program,
                share_token_program: TOKEN_PROGRAM,
                system_program: system_program::ID,
            },
            instruction::WithdrawalRequestResolve {},
        );
        self.send(&[resolve], &[])
    }

    pub fn withdrawal_request_cancel(&mut self, v: &TestVault, user: &Keypair) -> TransactionResult {
        let withdrawer = user.pubkey();
        let cancel = ix(
            accounts::WithdrawalRequestCancel {
                withdrawer,
                vault: v.address,
                withdrawal_request: withdrawal_request_pda(&v.address, &withdrawer),
                share_mint: v.share_mint,
                withdrawer_share_token_account: ata(&withdrawer, &v.share_mint, &TOKEN_PROGRAM),
                share_escrow: share_escrow_pda(&v.address),
                share_token_program: TOKEN_PROGRAM,
                system_program: system_program::ID,
            },
            instruction::WithdrawalRequestCancel {},
        );
        self.send(&[cancel], &[user])
    }

    // Fees

    /// Mints the manager fee shares into the admin's share ATA (the admin is the vault manager).
    pub fn vault_claim_manager_fee(&mut self, v: &TestVault) -> TransactionResult {
        let admin = self.admin.pubkey();
        let claim = ix(
            accounts::VaultClaimManagerFee {
                authority: admin,
                vault: v.address,
                share_mint: v.share_mint,
                authority_share_token_account: ata(&admin, &v.share_mint, &TOKEN_PROGRAM),
                system_program: system_program::ID,
                share_token_program: TOKEN_PROGRAM,
                associated_token_program: anchor_spl::associated_token::ID,
            },
            instruction::VaultClaimManagerFee {},
        );
        self.send(&[claim], &[])
    }

    /// Mints the platform fee shares into the admin's share ATA (the admin is the treasury authority).
    pub fn config_claim_platform_fee(&mut self, v: &TestVault) -> TransactionResult {
        let admin = self.admin.pubkey();
        let claim = ix(
            accounts::ConfigClaimPlatformFee {
                treasury_authority: admin,
                config: config_pda(),
                vault: v.address,
                share_mint: v.share_mint,
                treasury_authority_share_token_account: ata(&admin, &v.share_mint, &TOKEN_PROGRAM),
                system_program: system_program::ID,
                share_token_program: TOKEN_PROGRAM,
                associated_token_program: anchor_spl::associated_token::ID,
            },
            instruction::ConfigClaimPlatformFee {},
        );
        self.send(&[claim], &[])
    }

    // Admin reject

    /// `signer` defaults to the admin.
    pub fn deposit_request_reject(
        &mut self,
        v: &TestVault,
        depositor: &Pubkey,
        signer: Option<&Keypair>,
    ) -> TransactionResult {
        let reject = ix(
            accounts::DepositRequestReject {
                admin: signer.map_or(self.admin.pubkey(), |s| s.pubkey()),
                config: config_pda(),
                vault: v.address,
                depositor: *depositor,
                deposit_request: deposit_request_pda(&v.address, depositor),
                deposit_mint: v.deposit_mint,
                depositor_token_account: ata(depositor, &v.deposit_mint, &v.deposit_token_program),
                deposit_escrow: deposit_escrow_pda(&v.address),
                deposit_mint_token_program: v.deposit_token_program,
                system_program: system_program::ID,
            },
            instruction::DepositRequestReject {},
        );
        let signers: Vec<&Keypair> = signer.into_iter().collect();
        self.send(&[reject], &signers)
    }

    /// `signer` defaults to the admin.
    pub fn withdrawal_request_reject(
        &mut self,
        v: &TestVault,
        withdrawer: &Pubkey,
        signer: Option<&Keypair>,
    ) -> TransactionResult {
        let reject = ix(
            accounts::WithdrawalRequestReject {
                admin: signer.map_or(self.admin.pubkey(), |s| s.pubkey()),
                config: config_pda(),
                vault: v.address,
                withdrawer: *withdrawer,
                withdrawal_request: withdrawal_request_pda(&v.address, withdrawer),
                share_mint: v.share_mint,
                withdrawer_share_token_account: ata(withdrawer, &v.share_mint, &TOKEN_PROGRAM),
                share_escrow: share_escrow_pda(&v.address),
                share_token_program: TOKEN_PROGRAM,
                system_program: system_program::ID,
            },
            instruction::WithdrawalRequestReject {},
        );
        let signers: Vec<&Keypair> = signer.into_iter().collect();
        self.send(&[reject], &signers)
    }

    // Vault status

    pub fn vault_pause(&mut self, v: &TestVault) -> TransactionResult {
        let pause = ix(
            accounts::VaultPause {
                guardian: self.admin.pubkey(),
                config: config_pda(),
                vault: v.address,
            },
            instruction::VaultPause {},
        );
        self.send(&[pause], &[])
    }

    pub fn vault_close(&mut self, v: &TestVault) -> TransactionResult {
        let close = ix(
            accounts::VaultClose {
                authority: self.admin.pubkey(),
                vault: v.address,
                deposit_mint: v.deposit_mint,
                share_mint: v.share_mint,
                vault_token_account: ata(&v.address, &v.deposit_mint, &v.deposit_token_program),
                deposit_escrow: deposit_escrow_pda(&v.address),
                share_escrow: share_escrow_pda(&v.address),
                deposit_mint_token_program: v.deposit_token_program,
                share_token_program: TOKEN_PROGRAM,
                system_program: system_program::ID,
            },
            instruction::VaultClose {},
        );
        self.send(&[close], &[])
    }

    // Strategies

    pub fn jupiter_initialize_strategy(
        &mut self,
        v: &TestVault,
        destination_mint: &Pubkey,
    ) -> TransactionResult {
        let initialize = ix(
            accounts::JupiterInitializeStrategy {
                authority: self.admin.pubkey(),
                config: config_pda(),
                vault: v.address,
                strategy: strategy_pda(&v.address, destination_mint),
                destination_mint: *destination_mint,
                system_program: system_program::ID,
            },
            instruction::JupiterInitializeStrategy {},
        );
        self.send(&[initialize], &[])
    }

    /// Closes a Jupiter strategy. `vault` may differ from the strategy's own vault to
    /// exercise the cross-vault guard.
    pub fn close_jupiter_strategy(
        &mut self,
        v: &TestVault,
        strategy_vault: &Pubkey,
        target_mint: &Pubkey,
    ) -> TransactionResult {
        let mut close = ix(
            accounts::VaultCloseStrategy {
                authority: self.admin.pubkey(),
                config: config_pda(),
                vault: v.address,
                strategy: strategy_pda(strategy_vault, target_mint),
                system_program: system_program::ID,
            },
            instruction::VaultCloseStrategy {},
        );
        close.accounts.push(AccountMeta::new(
            ata(&v.address, target_mint, &TOKEN_PROGRAM),
            false,
        ));
        close
            .accounts
            .push(AccountMeta::new_readonly(TOKEN_PROGRAM, false));

        self.send(&[close], &[])
    }

    /// `source` is passed as `vault_source_token_account`, so a non-ATA can be tried.
    pub fn jupiter_swap(
        &mut self,
        v: &TestVault,
        source: &Pubkey,
        source_mint: &Pubkey,
        destination_mint: &Pubkey,
        amount: u64,
    ) -> TransactionResult {
        let swap = ix(
            accounts::JupiterSwap {
                authority: self.admin.pubkey(),
                config: config_pda(),
                vault: v.address,
                strategy: strategy_pda(&v.address, destination_mint),
                source_mint: *source_mint,
                destination_mint: *destination_mint,
                vault_source_token_account: *source,
                vault_destination_token_account: ata(
                    &v.address,
                    destination_mint,
                    &TOKEN_PROGRAM,
                ),
                system_program: system_program::ID,
                source_token_program: TOKEN_PROGRAM,
                destination_token_program: TOKEN_PROGRAM,
                associated_token_program: anchor_spl::associated_token::ID,
                event_authority: JUPITER_EVENT_AUTHORITY,
                jupiter_program: hedge_vault::jupiter::ID,
            },
            instruction::JupiterSwap {
                swap_data: vec![0u8; 32],
                amount,
                slippage_bps: 100,
            },
        );
        self.send(&[swap], &[])
    }
}
