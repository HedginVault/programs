//! LiteSVM harness for hedge_vault integration tests.
//! Loads `target/deploy/hedge_vault.so`, so run `anchor build` before `cargo test`.

pub use anchor_lang::prelude::Pubkey;
pub use anchor_spl::{token::ID as TOKEN_PROGRAM, token_2022::ID as TOKEN_2022_PROGRAM};
pub use hedge_vault::error::HedgeVaultError;
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
    accounts, instruction, Config, DepositRequest, InitializeConfigArgs, InitializeVaultArgs,
    ProtocolStatus, UpdateConfigArgs, UpdateVaultArgs, Vault, WithdrawalRequest,
};
use litesvm::{types::TransactionResult, LiteSVM};
use solana_sdk::{
    clock::Clock,
    instruction::{Instruction, InstructionError},
    system_instruction,
    transaction::{Transaction, TransactionError},
};

pub const DAY: i64 = 86_400;
pub const USDC: u64 = 1_000_000;
pub const START_TS: i64 = 100 * DAY;

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
    let failed = result.expect_err("transaction should have failed");
    assert_eq!(
        failed.err,
        TransactionError::InstructionError(0, InstructionError::Custom(error.into())),
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

        let initialize_config = ix(
            accounts::InitializeConfig {
                admin,
                config: config_pda(),
                system_program: system_program::ID,
            },
            instruction::InitializeConfig {
                args: InitializeConfigArgs {
                    nav_updater: admin,
                    treasury_authority: admin,
                    guardian: admin,
                    platform_performance_fee_bps: 0,
                    platform_management_fee_bps: 0,
                    max_nav_deviation_bps: 10_000,
                    max_epoch_outflow_bps: 10_000,
                },
            },
        );
        ctx.send(&[initialize_config], &[]).unwrap();

        let mut unpause = Self::update_config_args();
        unpause.status = Some(ProtocolStatus::Normal);
        ctx.update_config(unpause).unwrap();

        let add_manager = ix(
            accounts::AddManager {
                admin,
                config: config_pda(),
                authority: admin,
                manager: manager_pda(&admin),
                system_program: system_program::ID,
            },
            instruction::AddManager {},
        );
        ctx.send(&[add_manager], &[]).unwrap();

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

    pub fn update_config_args() -> UpdateConfigArgs {
        UpdateConfigArgs {
            pending_admin: None,
            nav_updater: None,
            treasury_authority: None,
            guardian: None,
            platform_performance_fee_bps: None,
            platform_management_fee_bps: None,
            max_nav_deviation_bps: None,
            max_epoch_outflow_bps: None,
            status: None,
        }
    }

    pub fn update_config(&mut self, args: UpdateConfigArgs) -> TransactionResult {
        let update = ix(
            accounts::UpdateConfig {
                admin: self.admin.pubkey(),
                config: config_pda(),
            },
            instruction::UpdateConfig { args },
        );
        self.send(&[update], &[])
    }

    pub fn accept_admin(&mut self, signer: &Keypair) -> TransactionResult {
        let accept = ix(
            accounts::AcceptAdmin {
                pending_admin: signer.pubkey(),
                config: config_pda(),
            },
            instruction::AcceptAdmin {},
        );
        self.send(&[accept], &[signer])
    }

    // Vault

    pub fn vault_args() -> InitializeVaultArgs {
        InitializeVaultArgs {
            name: [0; 32],
            description: [0; 64],
            performance_fee_bps: 0,
            management_fee_bps: 0,
            deposit_cap: 1_000_000 * USDC,
            min_deposit: 0,
            min_withdrawal_shares: 0,
        }
    }

    pub fn update_vault_args() -> UpdateVaultArgs {
        UpdateVaultArgs {
            description: None,
            performance_fee_bps: None,
            management_fee_bps: None,
            deposit_cap: None,
            min_deposit: None,
            min_withdrawal_shares: None,
            status: None,
        }
    }

    pub fn initialize_vault(
        &mut self,
        deposit_mint: Pubkey,
        deposit_token_program: Pubkey,
        args: InitializeVaultArgs,
    ) -> (TestVault, TransactionResult) {
        let admin = self.admin.pubkey();
        let address = vault_pda(self.config().next_vault_id);
        let share_mint = share_mint_pda(&address);

        let initialize = ix(
            accounts::InitializeVault {
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
            instruction::InitializeVault { args },
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
        let (vault, result) = self.initialize_vault(mint, TOKEN_PROGRAM, Self::vault_args());
        result.unwrap();
        vault
    }

    pub fn update_vault(&mut self, v: &TestVault, args: UpdateVaultArgs) -> TransactionResult {
        let update = ix(
            accounts::UpdateVault {
                authority: self.admin.pubkey(),
                vault: v.address,
            },
            instruction::UpdateVault { args },
        );
        self.send(&[update], &[])
    }

    pub fn update_nav(&mut self, v: &TestVault, total_assets: u64) -> TransactionResult {
        let update = ix(
            accounts::UpdateNav {
                nav_updater: self.admin.pubkey(),
                config: config_pda(),
                vault: v.address,
                deposit_mint: v.deposit_mint,
                share_mint: v.share_mint,
                vault_token_account: ata(&v.address, &v.deposit_mint, &v.deposit_token_program),
                deposit_mint_token_program: v.deposit_token_program,
            },
            instruction::UpdateNav { total_assets },
        );
        self.send(&[update], &[])
    }

    // Requests

    pub fn request_deposit(&mut self, v: &TestVault, user: &Keypair, amount: u64) -> TransactionResult {
        let depositor = user.pubkey();
        let request = ix(
            accounts::RequestDeposit {
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
            instruction::RequestDeposit { amount },
        );
        self.send(&[request], &[user])
    }

    pub fn cancel_deposit_request(&mut self, v: &TestVault, user: &Keypair) -> TransactionResult {
        let depositor = user.pubkey();
        let cancel = ix(
            accounts::CancelDepositRequest {
                depositor,
                vault: v.address,
                deposit_request: deposit_request_pda(&v.address, &depositor),
                deposit_mint: v.deposit_mint,
                depositor_token_account: ata(&depositor, &v.deposit_mint, &v.deposit_token_program),
                deposit_escrow: deposit_escrow_pda(&v.address),
                deposit_mint_token_program: v.deposit_token_program,
                system_program: system_program::ID,
            },
            instruction::CancelDepositRequest {},
        );
        self.send(&[cancel], &[user])
    }

    pub fn resolve_deposit_request(&mut self, v: &TestVault, depositor: &Pubkey) -> TransactionResult {
        let resolve = ix(
            accounts::ResolveDepositRequest {
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
            instruction::ResolveDepositRequest {},
        );
        self.send(&[resolve], &[])
    }

    pub fn request_withdrawal(&mut self, v: &TestVault, user: &Keypair, shares: u64) -> TransactionResult {
        let withdrawer = user.pubkey();
        let request = ix(
            accounts::RequestWithdrawal {
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
            instruction::RequestWithdrawal { shares },
        );
        self.send(&[request], &[user])
    }

    pub fn resolve_withdrawal_request(&mut self, v: &TestVault, withdrawer: &Pubkey) -> TransactionResult {
        let resolve = ix(
            accounts::ResolveWithdrawalRequest {
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
            instruction::ResolveWithdrawalRequest {},
        );
        self.send(&[resolve], &[])
    }

    pub fn cancel_withdrawal_request(&mut self, v: &TestVault, user: &Keypair) -> TransactionResult {
        let withdrawer = user.pubkey();
        let cancel = ix(
            accounts::CancelWithdrawalRequest {
                withdrawer,
                vault: v.address,
                withdrawal_request: withdrawal_request_pda(&v.address, &withdrawer),
                share_mint: v.share_mint,
                withdrawer_share_token_account: ata(&withdrawer, &v.share_mint, &TOKEN_PROGRAM),
                share_escrow: share_escrow_pda(&v.address),
                share_token_program: TOKEN_PROGRAM,
                system_program: system_program::ID,
            },
            instruction::CancelWithdrawalRequest {},
        );
        self.send(&[cancel], &[user])
    }

    // Fees

    /// Mints the manager fee shares into the admin's share ATA (the admin is the vault manager).
    pub fn claim_manager_fee(&mut self, v: &TestVault) -> TransactionResult {
        let admin = self.admin.pubkey();
        let claim = ix(
            accounts::ClaimManagerFee {
                authority: admin,
                vault: v.address,
                share_mint: v.share_mint,
                authority_share_token_account: ata(&admin, &v.share_mint, &TOKEN_PROGRAM),
                system_program: system_program::ID,
                share_token_program: TOKEN_PROGRAM,
                associated_token_program: anchor_spl::associated_token::ID,
            },
            instruction::ClaimManagerFee {},
        );
        self.send(&[claim], &[])
    }

    /// Mints the platform fee shares into the admin's share ATA (the admin is the treasury authority).
    pub fn claim_platform_fee(&mut self, v: &TestVault) -> TransactionResult {
        let admin = self.admin.pubkey();
        let claim = ix(
            accounts::ClaimPlatformFee {
                treasury_authority: admin,
                config: config_pda(),
                vault: v.address,
                share_mint: v.share_mint,
                treasury_authority_share_token_account: ata(&admin, &v.share_mint, &TOKEN_PROGRAM),
                system_program: system_program::ID,
                share_token_program: TOKEN_PROGRAM,
                associated_token_program: anchor_spl::associated_token::ID,
            },
            instruction::ClaimPlatformFee {},
        );
        self.send(&[claim], &[])
    }

    // Admin reject

    /// `signer` defaults to the admin.
    pub fn reject_deposit_request(
        &mut self,
        v: &TestVault,
        depositor: &Pubkey,
        signer: Option<&Keypair>,
    ) -> TransactionResult {
        let reject = ix(
            accounts::RejectDepositRequest {
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
            instruction::RejectDepositRequest {},
        );
        let signers: Vec<&Keypair> = signer.into_iter().collect();
        self.send(&[reject], &signers)
    }

    /// `signer` defaults to the admin.
    pub fn reject_withdrawal_request(
        &mut self,
        v: &TestVault,
        withdrawer: &Pubkey,
        signer: Option<&Keypair>,
    ) -> TransactionResult {
        let reject = ix(
            accounts::RejectWithdrawalRequest {
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
            instruction::RejectWithdrawalRequest {},
        );
        let signers: Vec<&Keypair> = signer.into_iter().collect();
        self.send(&[reject], &signers)
    }
}
