//! Hand-rolled Phoenix Perpetuals, Ember and Hawkeye CPIs, mirroring the Rise SDK
//! (`phoenix-rise-ix`), whose Solana 3.x / Pinocchio dependencies do not build with Anchor 0.31.
use anchor_lang::{
    prelude::*,
    solana_program::{
        instruction::Instruction,
        program::{get_return_data, invoke, invoke_signed},
    },
};

use crate::{error::HedgeVaultError, validate};

pub const PHOENIX_PROGRAM_ID: Pubkey = pubkey!("EtrnLzgbS7nMMy5fbD42kXiUzGg8XQzJ972Xtk1cjWih");
pub const PHOENIX_LOG_AUTHORITY: Pubkey = pubkey!("GdxfTLSsdSY37G6fZoYtdGDSfgFnbT2EmRpuePZxWShS");
pub const PHOENIX_GLOBAL_CONFIGURATION: Pubkey =
    pubkey!("2zskx2iyCvb6Stg7RBZkt1f6MrF4dpYtMG3yMvKwqtUZ");
pub const HAWKEYE_PROGRAM_ID: Pubkey = pubkey!("RiSeVw3ZjNfsaXPRb4mgaqYaEEt41pNNJoDvVh7pgQj");
pub const EMBER_PROGRAM_ID: Pubkey = pubkey!("EMBERpYNE6ehWmXymZZS2skiFmCa9V5dp14e1iduM5qy");
/// PDA `[PHOENIX_PROGRAM_ID, "state"]` under Ember.
pub const EMBER_STATE: Pubkey = pubkey!("6ur7v6AXNpnHeEb6xuk7PyezvZ1i5GrgYyWZkNCpzbRz");
/// PDA `[PHOENIX_PROGRAM_ID, "vault"]` under Ember, the USDC custody.
pub const EMBER_VAULT: Pubkey = pubkey!("FKcEb4TdPDTRuMnQDpSEPQBcrm15S73xiUD6Qf8ZLUkq");
/// The only collateral Ember wraps 1:1 into the Phoenix canonical token.
pub const USDC_MINT: Pubkey = pubkey!("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");

/// A vault registers one cross-margin trader account, index `(0, 0)`.
pub const TRADER_PDA_INDEX: u8 = 0;
pub const TRADER_SUBACCOUNT_INDEX: u8 = 0;
pub const TRADER_MAX_POSITIONS: u32 = 128;

const CAPABILITY_CAN_PLACE_MARKET: u32 = 1 << 2;
const CAPABILITY_CAN_DEPOSIT: u32 = 1 << 4;
const CAPABILITY_CAN_WITHDRAW: u32 = 1 << 5;
/// Flags an onboarded trader holds, a freshly registered one has none.
const TRADER_READY_FLAGS: u32 =
    CAPABILITY_CAN_PLACE_MARKET | CAPABILITY_CAN_DEPOSIT | CAPABILITY_CAN_WITHDRAW;

const ORDER_FLAG_NONE: u8 = 0;
const ORDER_FLAG_REDUCE_ONLY: u8 = 128;

pub const MAX_CANCEL_ORDER_IDS: usize = 100;

/// `sha256("global:<name>")[..8]`
mod ix_discriminator {
    pub const REGISTER_TRADER: [u8; 8] = [75, 243, 224, 167, 1, 5, 51, 32];
    pub const DEPOSIT_FUNDS: [u8; 8] = [202, 39, 52, 211, 53, 20, 250, 88];
    pub const WITHDRAW_FUNDS: [u8; 8] = [241, 36, 29, 111, 208, 31, 104, 217];
    pub const PLACE_MARKET_ORDER: [u8; 8] = [90, 118, 192, 252, 192, 99, 39, 145];
    pub const PLACE_LIMIT_ORDER: [u8; 8] = [108, 176, 33, 186, 146, 229, 1, 197];
    pub const CANCEL_ALL: [u8; 8] = [98, 191, 75, 220, 115, 40, 71, 237];
    pub const CANCEL_UP_TO: [u8; 8] = [26, 209, 244, 253, 59, 175, 227, 54];
    pub const CANCEL_ORDERS_BY_ID: [u8; 8] = [234, 204, 126, 94, 222, 22, 141, 24];
    pub const EMBER_DEPOSIT: [u8; 8] = [242, 35, 198, 137, 82, 225, 242, 182];
    pub const EMBER_WITHDRAW: [u8; 8] = [183, 18, 70, 156, 148, 109, 161, 34];
    pub const HAWKEYE_VIEW_MARGIN: [u8; 8] = [178, 10, 124, 173, 236, 210, 117, 6];
}

/// `sha256("account:<name>")[..8]`
mod account_discriminator {
    pub const GLOBAL_CONFIGURATION: [u8; 8] = [37, 146, 212, 210, 47, 136, 111, 20];
    pub const TRADER: [u8; 8] = [41, 97, 73, 105, 110, 214, 112, 9];
    pub const GLOBAL_TRADER_INDEX: [u8; 8] = [145, 92, 169, 6, 5, 144, 1, 205];
    pub const ACTIVE_TRADER_BUFFER: [u8; 8] = [192, 255, 205, 165, 80, 154, 131, 5];
}

/// `sha256("return:phoenix_hawkeye_margin")[..8]`
const VIEW_MARGIN_RETURN_MAGIC: [u8; 8] = [63, 37, 255, 61, 157, 91, 95, 149];
const VIEW_MARGIN_RETURN_VERSION: u16 = 1;
/// 16-byte header followed by twelve 8-byte margin fields.
const VIEW_MARGIN_RETURN_LEN: usize = 112;
const VIEW_MARGIN_IS_LIQUIDATABLE_OFFSET: usize = 14;

const MATCHING_ENGINE_RESPONSE_LEN: usize = 64;

mod global_config_layout {
    pub const LEN: usize = 776;
    pub const CANONICAL_MINT: usize = 296;
    pub const GLOBAL_VAULT: usize = 328;
    pub const PERP_ASSET_MAP: usize = 360;
    pub const GLOBAL_TRADER_INDEX: usize = 392;
    pub const ACTIVE_TRADER_BUFFER: usize = 424;
    pub const WITHDRAW_QUEUE: usize = 472;
}

mod multi_arena_layout {
    pub const LEN: usize = 80;
    pub const NUM_ARENAS: usize = 52;
    pub const NUM_ACTIVE_ARENAS: usize = 54;
}

mod trader_layout {
    pub const LEN: usize = 240;
    pub const AUTHORITY: usize = 56;
    pub const QUOTE_LOT_COLLATERAL: usize = 88;
    pub const FLAGS: usize = 96;
    pub const WITHDRAW_QUEUE_NODE: usize = 108;
    pub const PDA_INDEX: usize = 154;
    pub const SUBACCOUNT_INDEX: usize = 155;
    pub const POSITION_COUNT: usize = 224;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoenixSide {
    Bid,
    Ask,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoenixSelfTradeBehavior {
    Abort,
    CancelProvide,
    DecrementTake,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhoenixOrderKind {
    Market,
    Limit,
    PostOnly,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhoenixCancelId {
    pub node_pointer: u32,
    pub price_in_ticks: u64,
    pub order_sequence_number: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum PhoenixCancelMode {
    All,
    UpTo {
        side: PhoenixSide,
        num_orders_to_cancel: Option<u64>,
        tick_limit: Option<u64>,
    },
    ById {
        orders: Vec<PhoenixCancelId>,
    },
}

impl PhoenixCancelMode {
    pub fn instruction_data(&self) -> Result<Vec<u8>> {
        match self {
            Self::All => Ok(ix_discriminator::CANCEL_ALL.to_vec()),
            Self::UpTo {
                side,
                num_orders_to_cancel,
                tick_limit,
            } => instruction_data(
                ix_discriminator::CANCEL_UP_TO,
                &CancelUpToArgs {
                    side: *side,
                    num_orders_to_cancel: *num_orders_to_cancel,
                    tick_limit: *tick_limit,
                },
            ),
            Self::ById { orders } => {
                validate!(
                    !orders.is_empty() && orders.len() <= MAX_CANCEL_ORDER_IDS,
                    HedgeVaultError::InvalidInstructionData
                )?;

                instruction_data(ix_discriminator::CANCEL_ORDERS_BY_ID, orders)
            }
        }
    }
}

/// Borsh mirror of Phoenix's `OrderPacketKind`, variant order is the wire tag.
#[derive(AnchorSerialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderPacket {
    PostOnly {
        side: PhoenixSide,
        price_in_ticks: u64,
        num_base_lots: u64,
        client_order_id: u128,
        slide: bool,
        last_valid_slot: Option<u64>,
        order_flags: u8,
        cancel_existing: bool,
    },
    Limit {
        side: PhoenixSide,
        price_in_ticks: u64,
        num_base_lots: u64,
        self_trade_behavior: PhoenixSelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: u128,
        last_valid_slot: Option<u64>,
        order_flags: u8,
        cancel_existing: bool,
    },
    ImmediateOrCancel {
        side: PhoenixSide,
        price_in_ticks: Option<u64>,
        num_base_lots: u64,
        num_quote_lots: Option<u64>,
        min_base_lots_to_fill: u64,
        min_quote_lots_to_fill: u64,
        self_trade_behavior: PhoenixSelfTradeBehavior,
        match_limit: Option<u64>,
        client_order_id: u128,
        last_valid_slot: Option<u64>,
        order_flags: u8,
        cancel_existing: bool,
    },
}

impl OrderPacket {
    pub fn order_flags(reduce_only: bool) -> u8 {
        if reduce_only {
            ORDER_FLAG_REDUCE_ONLY
        } else {
            ORDER_FLAG_NONE
        }
    }

    pub fn instruction_data(&self) -> Result<Vec<u8>> {
        let discriminator = match self {
            Self::ImmediateOrCancel { .. } => ix_discriminator::PLACE_MARKET_ORDER,
            Self::PostOnly { .. } | Self::Limit { .. } => ix_discriminator::PLACE_LIMIT_ORDER,
        };

        instruction_data(discriminator, self)
    }
}

#[derive(AnchorSerialize)]
struct CancelUpToArgs {
    side: PhoenixSide,
    num_orders_to_cancel: Option<u64>,
    tick_limit: Option<u64>,
}

#[derive(AnchorSerialize)]
struct RegisterTraderArgs {
    max_positions: u32,
    trader_preference_bits: u32,
    trader_pda_index: u8,
    subaccount_index: u8,
}

fn instruction_data<T: AnchorSerialize>(discriminator: [u8; 8], args: &T) -> Result<Vec<u8>> {
    let mut data = discriminator.to_vec();
    args.serialize(&mut data)?;

    Ok(data)
}

/// PDA `["trader", authority, [pda_index, subaccount_index]]` under Phoenix.
pub fn trader_address(authority: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[
            b"trader",
            authority.as_ref(),
            &[TRADER_PDA_INDEX, TRADER_SUBACCOUNT_INDEX],
        ],
        &PHOENIX_PROGRAM_ID,
    )
    .0
}

/// PDA `["spline", orderbook]` under Phoenix.
pub fn spline_collection_address(orderbook: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"spline", orderbook.as_ref()], &PHOENIX_PROGRAM_ID).0
}

fn read_pubkey(data: &[u8], offset: usize) -> Pubkey {
    Pubkey::new_from_array(data[offset..offset + 32].try_into().unwrap())
}

fn read_u16(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(data[offset..offset + 2].try_into().unwrap())
}

fn read_u32(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
}

fn read_u64(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}

fn read_i64(data: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes(data[offset..offset + 8].try_into().unwrap())
}

fn validate_account_data(data: &[u8], discriminator: [u8; 8], min_len: usize) -> Result<()> {
    validate!(
        data.len() >= min_len && data[..8] == discriminator,
        HedgeVaultError::InvalidPhoenixAccount
    )?;

    Ok(())
}

fn validate_owner(account: &AccountInfo) -> Result<()> {
    validate!(
        *account.owner == PHOENIX_PROGRAM_ID,
        HedgeVaultError::InvalidPhoenixAccount
    )?;

    Ok(())
}

/// Exchange-wide accounts named by the Phoenix global configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhoenixGlobalConfig {
    pub canonical_mint: Pubkey,
    pub global_vault: Pubkey,
    pub perp_asset_map: Pubkey,
    pub global_trader_index: Pubkey,
    pub active_trader_buffer: Pubkey,
    pub withdraw_queue: Pubkey,
}

impl PhoenixGlobalConfig {
    pub fn load(account: &AccountInfo) -> Result<Self> {
        validate_owner(account)?;
        Self::parse(&account.try_borrow_data()?)
    }

    fn parse(data: &[u8]) -> Result<Self> {
        use global_config_layout::*;

        validate_account_data(data, account_discriminator::GLOBAL_CONFIGURATION, LEN)?;

        Ok(Self {
            canonical_mint: read_pubkey(data, CANONICAL_MINT),
            global_vault: read_pubkey(data, GLOBAL_VAULT),
            perp_asset_map: read_pubkey(data, PERP_ASSET_MAP),
            global_trader_index: read_pubkey(data, GLOBAL_TRADER_INDEX),
            active_trader_buffer: read_pubkey(data, ACTIVE_TRADER_BUFFER),
            withdraw_queue: read_pubkey(data, WITHDRAW_QUEUE),
        })
    }

    pub fn validate_account(expected: Pubkey, key: Pubkey) -> Result<()> {
        validate!(expected == key, HedgeVaultError::InvalidPhoenixAccount)?;

        Ok(())
    }

    /// Checks the dynamic tail every trading and collateral instruction takes:
    /// `[GTI header, GTI arenas.., ATB header, ATB arenas..]`, where each header's active arena
    /// count includes the header itself.
    pub fn validate_trader_tail(&self, tail: &[AccountInfo]) -> Result<()> {
        let global_trader_index_count = arena_count(
            tail.first(),
            self.global_trader_index,
            account_discriminator::GLOBAL_TRADER_INDEX,
        )?;
        let active_trader_buffer_count = arena_count(
            tail.get(global_trader_index_count),
            self.active_trader_buffer,
            account_discriminator::ACTIVE_TRADER_BUFFER,
        )?;

        validate!(
            tail.len() == global_trader_index_count + active_trader_buffer_count,
            HedgeVaultError::InvalidPhoenixRemainingAccounts
        )?;

        Ok(())
    }
}

fn arena_count(
    header: Option<&AccountInfo>,
    expected_key: Pubkey,
    discriminator: [u8; 8],
) -> Result<usize> {
    let header = header.ok_or(HedgeVaultError::InvalidPhoenixRemainingAccounts)?;

    validate!(
        header.key() == expected_key,
        HedgeVaultError::InvalidPhoenixRemainingAccounts
    )?;
    validate_owner(header)?;

    parse_arena_count(&header.try_borrow_data()?, discriminator)
}

fn parse_arena_count(data: &[u8], discriminator: [u8; 8]) -> Result<usize> {
    validate_account_data(data, discriminator, multi_arena_layout::LEN)?;

    // the SDK resolves `min(num_arenas, num_active_arenas)` accounts, the header first
    let num_arenas = read_u16(data, multi_arena_layout::NUM_ARENAS)
        .min(read_u16(data, multi_arena_layout::NUM_ACTIVE_ARENAS)) as usize;
    validate!(
        num_arenas > 0,
        HedgeVaultError::InvalidPhoenixRemainingAccounts
    )?;

    Ok(num_arenas)
}

/// Header fields of a Phoenix trader account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhoenixTrader {
    pub authority: Pubkey,
    pub quote_lot_collateral: i64,
    pub flags: u32,
    pub withdraw_queue_node: u32,
    pub trader_pda_index: u8,
    pub trader_subaccount_index: u8,
    pub position_count: u64,
}

impl PhoenixTrader {
    pub fn load(account: &AccountInfo) -> Result<Self> {
        validate_owner(account)?;
        Self::parse(&account.try_borrow_data()?)
    }

    fn parse(data: &[u8]) -> Result<Self> {
        use trader_layout::*;

        validate_account_data(data, account_discriminator::TRADER, LEN)?;

        Ok(Self {
            authority: read_pubkey(data, AUTHORITY),
            quote_lot_collateral: read_i64(data, QUOTE_LOT_COLLATERAL),
            flags: read_u32(data, FLAGS),
            withdraw_queue_node: read_u32(data, WITHDRAW_QUEUE_NODE),
            trader_pda_index: data[PDA_INDEX],
            trader_subaccount_index: data[SUBACCOUNT_INDEX],
            position_count: read_u64(data, POSITION_COUNT),
        })
    }

    pub fn validate_authority(&self, authority: Pubkey) -> Result<()> {
        validate!(
            self.authority == authority
                && self.trader_pda_index == TRADER_PDA_INDEX
                && self.trader_subaccount_index == TRADER_SUBACCOUNT_INDEX,
            HedgeVaultError::InvalidPhoenixTrader
        )?;

        Ok(())
    }

    pub fn is_ready(&self) -> Result<()> {
        validate!(
            self.flags & TRADER_READY_FLAGS == TRADER_READY_FLAGS,
            HedgeVaultError::PhoenixTraderNotReady
        )?;

        Ok(())
    }

    pub fn is_empty(&self) -> Result<()> {
        validate!(
            self.quote_lot_collateral == 0
                && self.position_count == 0
                && self.withdraw_queue_node == 0,
            HedgeVaultError::PhoenixStrategyNotEmpty
        )?;

        Ok(())
    }
}

/// Fill summary Phoenix returns from every order instruction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MatchingEngineResponse {
    pub price_in_ticks: u64,
    pub order_sequence_number: u64,
    pub num_quote_lots_in: u64,
    pub num_base_lots_in: u64,
    pub num_quote_lots_out: u64,
    pub num_base_lots_out: u64,
    pub num_quote_lots_posted: u64,
    pub num_base_lots_posted: u64,
}

impl MatchingEngineResponse {
    pub fn from_return_data() -> Result<Self> {
        let (program_id, data) =
            get_return_data().ok_or(HedgeVaultError::PhoenixReturnDataInvalid)?;

        validate!(
            program_id == PHOENIX_PROGRAM_ID,
            HedgeVaultError::PhoenixReturnDataInvalid
        )?;

        Self::parse(&data)
    }

    fn parse(data: &[u8]) -> Result<Self> {
        validate!(
            data.len() == MATCHING_ENGINE_RESPONSE_LEN,
            HedgeVaultError::PhoenixReturnDataInvalid
        )?;

        Ok(Self {
            price_in_ticks: read_u64(data, 0),
            order_sequence_number: read_u64(data, 8),
            num_quote_lots_in: read_u64(data, 16),
            num_base_lots_in: read_u64(data, 24),
            num_quote_lots_out: read_u64(data, 32),
            num_base_lots_out: read_u64(data, 40),
            num_quote_lots_posted: read_u64(data, 48),
            num_base_lots_posted: read_u64(data, 56),
        })
    }

    /// Only one side of each pair is non-zero, depending on the order side.
    pub fn base_lots_filled(&self) -> u64 {
        self.num_base_lots_in.saturating_add(self.num_base_lots_out)
    }

    pub fn quote_lots_filled(&self) -> u64 {
        self.num_quote_lots_in
            .saturating_add(self.num_quote_lots_out)
    }

    /// Set only when part of the order rested on the book.
    pub fn order_sequence_number(&self) -> Option<u64> {
        (self.price_in_ticks != 0 || self.order_sequence_number != 0)
            .then_some(self.order_sequence_number)
    }
}

fn parse_is_liquidatable(data: &[u8]) -> Result<bool> {
    validate!(
        data.len() == VIEW_MARGIN_RETURN_LEN
            && data[..8] == VIEW_MARGIN_RETURN_MAGIC
            && read_u16(data, 8) == VIEW_MARGIN_RETURN_VERSION,
        HedgeVaultError::PhoenixReturnDataInvalid
    )?;

    Ok(data[VIEW_MARGIN_IS_LIQUIDATABLE_OFFSET] != 0)
}

/// The accounts Phoenix needs to act on one market.
pub struct PhoenixMarket<'info> {
    pub perp_asset_map: AccountInfo<'info>,
    pub orderbook: AccountInfo<'info>,
    pub spline_collection: AccountInfo<'info>,
}

impl<'info> PhoenixMarket<'info> {
    pub fn validate(&self, global_config: &PhoenixGlobalConfig) -> Result<()> {
        PhoenixGlobalConfig::validate_account(
            global_config.perp_asset_map,
            self.perp_asset_map.key(),
        )?;
        validate!(
            self.spline_collection.key() == spline_collection_address(self.orderbook.key),
            HedgeVaultError::InvalidPhoenixAccount
        )?;

        Ok(())
    }
}

/// Phoenix instructions issued on behalf of `trader`, the vault PDA.
pub struct PhoenixCpi<'a, 'info> {
    pub phoenix_program: AccountInfo<'info>,
    pub log_authority: AccountInfo<'info>,
    pub global_config: AccountInfo<'info>,
    pub trader: AccountInfo<'info>,
    pub trader_account: AccountInfo<'info>,
    /// Global trader index and active trader buffer accounts, see
    /// [PhoenixGlobalConfig::validate_trader_tail].
    pub tail: &'a [AccountInfo<'info>],
}

impl<'a, 'info> PhoenixCpi<'a, 'info> {
    /// Phoenix only reads `trader`, the payer funds the trader account.
    pub fn register_trader(
        &self,
        payer: &AccountInfo<'info>,
        system_program: &AccountInfo<'info>,
    ) -> Result<()> {
        let data = instruction_data(
            ix_discriminator::REGISTER_TRADER,
            &RegisterTraderArgs {
                max_positions: TRADER_MAX_POSITIONS,
                trader_preference_bits: 0,
                trader_pda_index: TRADER_PDA_INDEX,
                subaccount_index: TRADER_SUBACCOUNT_INDEX,
            },
        )?;

        let accounts = vec![
            AccountMeta::new_readonly(self.phoenix_program.key(), false),
            AccountMeta::new_readonly(self.log_authority.key(), false),
            AccountMeta::new_readonly(self.global_config.key(), false),
            AccountMeta::new(payer.key(), true),
            AccountMeta::new_readonly(self.trader.key(), false),
            AccountMeta::new(self.trader_account.key(), false),
            AccountMeta::new_readonly(system_program.key(), false),
        ];
        let account_infos = [
            self.phoenix_program.clone(),
            self.log_authority.clone(),
            self.global_config.clone(),
            payer.clone(),
            self.trader.clone(),
            self.trader_account.clone(),
            system_program.clone(),
        ];

        invoke(
            &Instruction {
                program_id: PHOENIX_PROGRAM_ID,
                accounts,
                data,
            },
            &account_infos,
        )?;

        Ok(())
    }

    pub fn deposit_funds(
        &self,
        trader_token_account: &AccountInfo<'info>,
        global_vault: &AccountInfo<'info>,
        token_program: &AccountInfo<'info>,
        amount: u64,
        signer_seeds: &[&[u8]],
    ) -> Result<()> {
        let (mut accounts, mut account_infos) = self.prefix();

        accounts.extend([
            AccountMeta::new(trader_token_account.key(), false),
            AccountMeta::new(self.trader_account.key(), false),
            AccountMeta::new(global_vault.key(), false),
            AccountMeta::new_readonly(token_program.key(), false),
        ]);
        account_infos.extend([
            trader_token_account.clone(),
            self.trader_account.clone(),
            global_vault.clone(),
            token_program.clone(),
        ]);
        self.extend_tail(&mut accounts, &mut account_infos);

        self.invoke(
            accounts,
            account_infos,
            instruction_data(ix_discriminator::DEPOSIT_FUNDS, &amount)?,
            signer_seeds,
        )
    }

    /// Either pays `amount` into `trader_token_account` now or queues it behind the
    /// exchange withdraw throttle.
    #[allow(clippy::too_many_arguments)]
    pub fn withdraw_funds(
        &self,
        perp_asset_map: &AccountInfo<'info>,
        global_vault: &AccountInfo<'info>,
        trader_token_account: &AccountInfo<'info>,
        token_program: &AccountInfo<'info>,
        withdraw_queue: &AccountInfo<'info>,
        amount: u64,
        signer_seeds: &[&[u8]],
    ) -> Result<()> {
        let (mut accounts, mut account_infos) = self.prefix();

        accounts.extend([
            AccountMeta::new(self.trader_account.key(), false),
            AccountMeta::new(perp_asset_map.key(), false),
            AccountMeta::new(global_vault.key(), false),
            AccountMeta::new(trader_token_account.key(), false),
            AccountMeta::new_readonly(token_program.key(), false),
        ]);
        account_infos.extend([
            self.trader_account.clone(),
            perp_asset_map.clone(),
            global_vault.clone(),
            trader_token_account.clone(),
            token_program.clone(),
        ]);
        self.extend_tail(&mut accounts, &mut account_infos);
        accounts.push(AccountMeta::new(withdraw_queue.key(), false));
        account_infos.push(withdraw_queue.clone());

        self.invoke(
            accounts,
            account_infos,
            instruction_data(ix_discriminator::WITHDRAW_FUNDS, &amount)?,
            signer_seeds,
        )
    }

    pub fn place_order(
        &self,
        market: &PhoenixMarket<'info>,
        packet: &OrderPacket,
        signer_seeds: &[&[u8]],
    ) -> Result<MatchingEngineResponse> {
        self.market_action(market, packet.instruction_data()?, signer_seeds)?;

        MatchingEngineResponse::from_return_data()
    }

    pub fn cancel_orders(
        &self,
        market: &PhoenixMarket<'info>,
        mode: &PhoenixCancelMode,
        signer_seeds: &[&[u8]],
    ) -> Result<()> {
        self.market_action(market, mode.instruction_data()?, signer_seeds)
    }

    /// Hawkeye margin view of the trader account at current marks.
    pub fn is_liquidatable(
        &self,
        hawkeye_program: &AccountInfo<'info>,
        perp_asset_map: &AccountInfo<'info>,
    ) -> Result<bool> {
        let mut accounts = vec![
            AccountMeta::new_readonly(self.phoenix_program.key(), false),
            AccountMeta::new_readonly(self.global_config.key(), false),
        ];
        let mut account_infos = vec![
            hawkeye_program.clone(),
            self.phoenix_program.clone(),
            self.global_config.clone(),
        ];

        accounts.extend(
            self.tail
                .iter()
                .map(|account| AccountMeta::new_readonly(account.key(), false)),
        );
        account_infos.extend(self.tail.iter().cloned());
        accounts.extend([
            AccountMeta::new_readonly(perp_asset_map.key(), false),
            AccountMeta::new_readonly(self.trader_account.key(), false),
        ]);
        account_infos.extend([perp_asset_map.clone(), self.trader_account.clone()]);

        invoke(
            &Instruction {
                program_id: HAWKEYE_PROGRAM_ID,
                accounts,
                data: ix_discriminator::HAWKEYE_VIEW_MARGIN.to_vec(),
            },
            &account_infos,
        )?;

        let (program_id, data) =
            get_return_data().ok_or(HedgeVaultError::PhoenixReturnDataInvalid)?;

        validate!(
            program_id == HAWKEYE_PROGRAM_ID,
            HedgeVaultError::PhoenixReturnDataInvalid
        )?;

        parse_is_liquidatable(&data)
    }

    fn market_action(
        &self,
        market: &PhoenixMarket<'info>,
        data: Vec<u8>,
        signer_seeds: &[&[u8]],
    ) -> Result<()> {
        let (mut accounts, mut account_infos) = self.prefix();

        accounts.extend([
            AccountMeta::new(self.trader_account.key(), false),
            AccountMeta::new(market.perp_asset_map.key(), false),
        ]);
        account_infos.extend([self.trader_account.clone(), market.perp_asset_map.clone()]);
        self.extend_tail(&mut accounts, &mut account_infos);
        accounts.extend([
            AccountMeta::new(market.orderbook.key(), false),
            AccountMeta::new(market.spline_collection.key(), false),
        ]);
        account_infos.extend([market.orderbook.clone(), market.spline_collection.clone()]);

        self.invoke(accounts, account_infos, data, signer_seeds)
    }

    /// `[phoenix_program, log_authority, global_config (mut), trader (signer)]`
    fn prefix(&self) -> (Vec<AccountMeta>, Vec<AccountInfo<'info>>) {
        (
            vec![
                AccountMeta::new_readonly(self.phoenix_program.key(), false),
                AccountMeta::new_readonly(self.log_authority.key(), false),
                AccountMeta::new(self.global_config.key(), false),
                AccountMeta::new_readonly(self.trader.key(), true),
            ],
            vec![
                self.phoenix_program.clone(),
                self.log_authority.clone(),
                self.global_config.clone(),
                self.trader.clone(),
            ],
        )
    }

    fn extend_tail(
        &self,
        accounts: &mut Vec<AccountMeta>,
        account_infos: &mut Vec<AccountInfo<'info>>,
    ) {
        accounts.extend(
            self.tail
                .iter()
                .map(|account| AccountMeta::new(account.key(), false)),
        );
        account_infos.extend(self.tail.iter().cloned());
    }

    fn invoke(
        &self,
        accounts: Vec<AccountMeta>,
        account_infos: Vec<AccountInfo<'info>>,
        data: Vec<u8>,
        signer_seeds: &[&[u8]],
    ) -> Result<()> {
        invoke_signed(
            &Instruction {
                program_id: PHOENIX_PROGRAM_ID,
                accounts,
                data,
            },
            &account_infos,
            &[signer_seeds],
        )?;

        Ok(())
    }
}

/// Ember wraps USDC 1:1 into the Phoenix canonical token and back, on behalf of `trader`.
pub struct EmberCpi<'info> {
    pub ember_program: AccountInfo<'info>,
    pub trader: AccountInfo<'info>,
    pub ember_state: AccountInfo<'info>,
    pub usdc_mint: AccountInfo<'info>,
    pub canonical_mint: AccountInfo<'info>,
    pub trader_usdc_account: AccountInfo<'info>,
    pub trader_canonical_account: AccountInfo<'info>,
    pub ember_vault: AccountInfo<'info>,
    pub token_program: AccountInfo<'info>,
}

impl<'info> EmberCpi<'info> {
    pub fn deposit(&self, amount: u64, signer_seeds: &[&[u8]]) -> Result<()> {
        self.invoke(
            instruction_data(ix_discriminator::EMBER_DEPOSIT, &amount)?,
            signer_seeds,
        )
    }

    /// `None` unwraps the whole canonical balance.
    pub fn withdraw(&self, amount: Option<u64>, signer_seeds: &[&[u8]]) -> Result<()> {
        self.invoke(
            instruction_data(ix_discriminator::EMBER_WITHDRAW, &amount)?,
            signer_seeds,
        )
    }

    fn invoke(&self, data: Vec<u8>, signer_seeds: &[&[u8]]) -> Result<()> {
        invoke_signed(
            &Instruction {
                program_id: EMBER_PROGRAM_ID,
                accounts: vec![
                    AccountMeta::new_readonly(self.trader.key(), true),
                    AccountMeta::new_readonly(self.ember_state.key(), false),
                    AccountMeta::new_readonly(self.usdc_mint.key(), false),
                    AccountMeta::new(self.canonical_mint.key(), false),
                    AccountMeta::new(self.trader_usdc_account.key(), false),
                    AccountMeta::new(self.trader_canonical_account.key(), false),
                    AccountMeta::new(self.ember_vault.key(), false),
                    AccountMeta::new_readonly(self.token_program.key(), false),
                ],
                data,
            },
            &[
                self.ember_program.clone(),
                self.trader.clone(),
                self.ember_state.clone(),
                self.usdc_mint.clone(),
                self.canonical_mint.clone(),
                self.trader_usdc_account.clone(),
                self.trader_canonical_account.clone(),
                self.ember_vault.clone(),
                self.token_program.clone(),
            ],
            &[signer_seeds],
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use anchor_lang::solana_program::hash::hash;

    use super::*;

    fn sighash(preimage: &str) -> [u8; 8] {
        hash(preimage.as_bytes()).to_bytes()[..8]
            .try_into()
            .unwrap()
    }

    fn assert_err<T: core::fmt::Debug>(result: Result<T>, error: HedgeVaultError) {
        assert_eq!(result.unwrap_err(), anchor_lang::error::Error::from(error));
    }

    #[test]
    fn discriminators_match_their_preimages() {
        let cases = [
            (ix_discriminator::REGISTER_TRADER, "global:register_trader"),
            (ix_discriminator::DEPOSIT_FUNDS, "global:deposit_funds"),
            (ix_discriminator::WITHDRAW_FUNDS, "global:withdraw_funds"),
            (
                ix_discriminator::PLACE_MARKET_ORDER,
                "global:place_market_order",
            ),
            (
                ix_discriminator::PLACE_LIMIT_ORDER,
                "global:place_limit_order",
            ),
            (ix_discriminator::CANCEL_ALL, "global:cancel_all"),
            (ix_discriminator::CANCEL_UP_TO, "global:cancel_up_to"),
            (
                ix_discriminator::CANCEL_ORDERS_BY_ID,
                "global:cancel_orders_by_id",
            ),
            (ix_discriminator::EMBER_DEPOSIT, "global:deposit"),
            (ix_discriminator::EMBER_WITHDRAW, "global:withdraw"),
            (ix_discriminator::HAWKEYE_VIEW_MARGIN, "global:view_margin"),
            (
                account_discriminator::GLOBAL_CONFIGURATION,
                "account:global_configuration",
            ),
            (account_discriminator::TRADER, "account:trader"),
            (
                account_discriminator::GLOBAL_TRADER_INDEX,
                "account:global_trader_index",
            ),
            (
                account_discriminator::ACTIVE_TRADER_BUFFER,
                "account:active_trader_buffer",
            ),
            (VIEW_MARGIN_RETURN_MAGIC, "return:phoenix_hawkeye_margin"),
        ];

        for (discriminator, preimage) in cases {
            assert_eq!(discriminator, sighash(preimage), "{preimage}");
        }
    }

    #[test]
    fn ember_accounts_are_derived_from_the_phoenix_program() {
        let derive = |seed: &[u8]| {
            Pubkey::find_program_address(&[PHOENIX_PROGRAM_ID.as_ref(), seed], &EMBER_PROGRAM_ID).0
        };

        assert_eq!(derive(b"state"), EMBER_STATE);
        assert_eq!(derive(b"vault"), EMBER_VAULT);
        assert_eq!(
            Pubkey::find_program_address(&[b"log"], &PHOENIX_PROGRAM_ID).0,
            PHOENIX_LOG_AUTHORITY
        );
        assert_eq!(
            Pubkey::find_program_address(&[b"global"], &PHOENIX_PROGRAM_ID).0,
            PHOENIX_GLOBAL_CONFIGURATION
        );
    }

    #[test]
    fn register_trader_data_matches_the_sdk_layout() {
        let data = instruction_data(
            ix_discriminator::REGISTER_TRADER,
            &RegisterTraderArgs {
                max_positions: TRADER_MAX_POSITIONS,
                trader_preference_bits: 0,
                trader_pda_index: 0,
                subaccount_index: 0,
            },
        )
        .unwrap();

        let mut expected = ix_discriminator::REGISTER_TRADER.to_vec();
        expected.extend_from_slice(&128u32.to_le_bytes());
        expected.extend_from_slice(&0u32.to_le_bytes());
        expected.extend_from_slice(&[0, 0]);

        assert_eq!(data, expected);
        assert_eq!(data.len(), 18);
    }

    #[test]
    fn market_order_packet_matches_the_sdk_layout() {
        let packet = OrderPacket::ImmediateOrCancel {
            side: PhoenixSide::Ask,
            price_in_ticks: Some(50_000),
            num_base_lots: 1_000,
            num_quote_lots: None,
            min_base_lots_to_fill: 10,
            min_quote_lots_to_fill: 0,
            self_trade_behavior: PhoenixSelfTradeBehavior::DecrementTake,
            match_limit: None,
            client_order_id: 0x0102,
            last_valid_slot: Some(7),
            order_flags: OrderPacket::order_flags(true),
            cancel_existing: true,
        };

        let mut expected = ix_discriminator::PLACE_MARKET_ORDER.to_vec();
        expected.push(2); // ImmediateOrCancel
        expected.push(1); // Ask
        expected.push(1);
        expected.extend_from_slice(&50_000u64.to_le_bytes());
        expected.extend_from_slice(&1_000u64.to_le_bytes());
        expected.push(0);
        expected.extend_from_slice(&10u64.to_le_bytes());
        expected.extend_from_slice(&0u64.to_le_bytes());
        expected.push(2); // DecrementTake
        expected.push(0);
        expected.extend_from_slice(&0x0102u128.to_le_bytes());
        expected.push(1);
        expected.extend_from_slice(&7u64.to_le_bytes());
        expected.push(ORDER_FLAG_REDUCE_ONLY);
        expected.push(1);

        assert_eq!(packet.instruction_data().unwrap(), expected);
    }

    #[test]
    fn limit_and_post_only_packets_share_the_limit_instruction() {
        let limit = OrderPacket::Limit {
            side: PhoenixSide::Bid,
            price_in_ticks: 100,
            num_base_lots: 5,
            self_trade_behavior: PhoenixSelfTradeBehavior::Abort,
            match_limit: Some(3),
            client_order_id: 0,
            last_valid_slot: None,
            order_flags: OrderPacket::order_flags(false),
            cancel_existing: false,
        }
        .instruction_data()
        .unwrap();

        let mut expected = ix_discriminator::PLACE_LIMIT_ORDER.to_vec();
        expected.push(1); // Limit
        expected.push(0); // Bid
        expected.extend_from_slice(&100u64.to_le_bytes());
        expected.extend_from_slice(&5u64.to_le_bytes());
        expected.push(0); // Abort
        expected.push(1);
        expected.extend_from_slice(&3u64.to_le_bytes());
        expected.extend_from_slice(&[0; 16]);
        expected.push(0);
        expected.push(ORDER_FLAG_NONE);
        expected.push(0);
        assert_eq!(limit, expected);

        let post_only = OrderPacket::PostOnly {
            side: PhoenixSide::Ask,
            price_in_ticks: 100,
            num_base_lots: 5,
            client_order_id: 0,
            slide: true,
            last_valid_slot: None,
            order_flags: OrderPacket::order_flags(false),
            cancel_existing: false,
        }
        .instruction_data()
        .unwrap();

        let mut expected = ix_discriminator::PLACE_LIMIT_ORDER.to_vec();
        expected.push(0); // PostOnly
        expected.push(1); // Ask
        expected.extend_from_slice(&100u64.to_le_bytes());
        expected.extend_from_slice(&5u64.to_le_bytes());
        expected.extend_from_slice(&[0; 16]);
        expected.push(1);
        expected.push(0);
        expected.push(ORDER_FLAG_NONE);
        expected.push(0);
        assert_eq!(post_only, expected);
    }

    #[test]
    fn cancel_modes_encode_their_instruction() {
        assert_eq!(
            PhoenixCancelMode::All.instruction_data().unwrap(),
            ix_discriminator::CANCEL_ALL
        );

        let up_to = PhoenixCancelMode::UpTo {
            side: PhoenixSide::Ask,
            num_orders_to_cancel: None,
            tick_limit: Some(9),
        }
        .instruction_data()
        .unwrap();
        let mut expected = ix_discriminator::CANCEL_UP_TO.to_vec();
        expected.extend_from_slice(&[1, 0, 1]);
        expected.extend_from_slice(&9u64.to_le_bytes());
        assert_eq!(up_to, expected);

        let by_id = PhoenixCancelMode::ById {
            orders: vec![PhoenixCancelId {
                node_pointer: 4,
                price_in_ticks: 100,
                order_sequence_number: 42,
            }],
        }
        .instruction_data()
        .unwrap();
        let mut expected = ix_discriminator::CANCEL_ORDERS_BY_ID.to_vec();
        expected.extend_from_slice(&1u32.to_le_bytes());
        expected.extend_from_slice(&4u32.to_le_bytes());
        expected.extend_from_slice(&100u64.to_le_bytes());
        expected.extend_from_slice(&42u64.to_le_bytes());
        assert_eq!(by_id, expected);
    }

    #[test]
    fn cancel_by_id_rejects_empty_and_oversized_lists() {
        let id = PhoenixCancelId {
            node_pointer: 0,
            price_in_ticks: 0,
            order_sequence_number: 0,
        };

        assert_err(
            PhoenixCancelMode::ById { orders: vec![] }.instruction_data(),
            HedgeVaultError::InvalidInstructionData,
        );
        assert_err(
            PhoenixCancelMode::ById {
                orders: vec![id; MAX_CANCEL_ORDER_IDS + 1],
            }
            .instruction_data(),
            HedgeVaultError::InvalidInstructionData,
        );
        assert!(PhoenixCancelMode::ById {
            orders: vec![id; MAX_CANCEL_ORDER_IDS],
        }
        .instruction_data()
        .is_ok());
    }

    #[test]
    fn ember_data_encodes_amounts() {
        let deposit = instruction_data(ix_discriminator::EMBER_DEPOSIT, &5u64).unwrap();
        assert_eq!(deposit.len(), 16);
        assert_eq!(deposit[8..], 5u64.to_le_bytes());

        let withdraw_all =
            instruction_data(ix_discriminator::EMBER_WITHDRAW, &None::<u64>).unwrap();
        assert_eq!(withdraw_all[8..], [0]);

        let withdraw = instruction_data(ix_discriminator::EMBER_WITHDRAW, &Some(5u64)).unwrap();
        assert_eq!(withdraw.len(), 17);
    }

    #[test]
    fn global_config_reads_keys_at_their_offsets() {
        let mut data = vec![0u8; global_config_layout::LEN];
        data[..8].copy_from_slice(&account_discriminator::GLOBAL_CONFIGURATION);

        let keys: Vec<Pubkey> = (1..=6).map(|i| Pubkey::new_from_array([i; 32])).collect();
        for (offset, key) in [
            global_config_layout::CANONICAL_MINT,
            global_config_layout::GLOBAL_VAULT,
            global_config_layout::PERP_ASSET_MAP,
            global_config_layout::GLOBAL_TRADER_INDEX,
            global_config_layout::ACTIVE_TRADER_BUFFER,
            global_config_layout::WITHDRAW_QUEUE,
        ]
        .into_iter()
        .zip(&keys)
        {
            data[offset..offset + 32].copy_from_slice(key.as_ref());
        }

        assert_eq!(
            PhoenixGlobalConfig::parse(&data).unwrap(),
            PhoenixGlobalConfig {
                canonical_mint: keys[0],
                global_vault: keys[1],
                perp_asset_map: keys[2],
                global_trader_index: keys[3],
                active_trader_buffer: keys[4],
                withdraw_queue: keys[5],
            }
        );

        assert_err(
            PhoenixGlobalConfig::parse(&data[..global_config_layout::LEN - 1]),
            HedgeVaultError::InvalidPhoenixAccount,
        );
        data[0] ^= 1;
        assert_err(
            PhoenixGlobalConfig::parse(&data),
            HedgeVaultError::InvalidPhoenixAccount,
        );
    }

    fn arena_header(num_arenas: u16, num_active_arenas: u16) -> Vec<u8> {
        use multi_arena_layout::*;

        let mut data = vec![0u8; LEN];
        data[..8].copy_from_slice(&account_discriminator::GLOBAL_TRADER_INDEX);
        data[NUM_ARENAS..NUM_ARENAS + 2].copy_from_slice(&num_arenas.to_le_bytes());
        data[NUM_ACTIVE_ARENAS..NUM_ACTIVE_ARENAS + 2]
            .copy_from_slice(&num_active_arenas.to_le_bytes());

        data
    }

    #[test]
    fn arena_count_reads_the_active_arenas_of_the_superblock() {
        let discriminator = account_discriminator::GLOBAL_TRADER_INDEX;

        assert_eq!(
            parse_arena_count(&arena_header(3, 3), discriminator).unwrap(),
            3
        );
        assert_eq!(
            parse_arena_count(&arena_header(3, 2), discriminator).unwrap(),
            2
        );
        assert_err(
            parse_arena_count(
                &arena_header(3, 3),
                account_discriminator::ACTIVE_TRADER_BUFFER,
            ),
            HedgeVaultError::InvalidPhoenixAccount,
        );
        assert_err(
            parse_arena_count(&arena_header(1, 0), discriminator),
            HedgeVaultError::InvalidPhoenixRemainingAccounts,
        );
    }

    fn trader_data(trader: &PhoenixTrader) -> Vec<u8> {
        use trader_layout::*;

        let mut data = vec![0u8; LEN];
        data[..8].copy_from_slice(&account_discriminator::TRADER);
        data[AUTHORITY..AUTHORITY + 32].copy_from_slice(trader.authority.as_ref());
        data[QUOTE_LOT_COLLATERAL..QUOTE_LOT_COLLATERAL + 8]
            .copy_from_slice(&trader.quote_lot_collateral.to_le_bytes());
        data[FLAGS..FLAGS + 4].copy_from_slice(&trader.flags.to_le_bytes());
        data[WITHDRAW_QUEUE_NODE..WITHDRAW_QUEUE_NODE + 4]
            .copy_from_slice(&trader.withdraw_queue_node.to_le_bytes());
        data[PDA_INDEX] = trader.trader_pda_index;
        data[SUBACCOUNT_INDEX] = trader.trader_subaccount_index;
        data[POSITION_COUNT..POSITION_COUNT + 8]
            .copy_from_slice(&trader.position_count.to_le_bytes());

        data
    }

    fn empty_trader(authority: Pubkey) -> PhoenixTrader {
        PhoenixTrader {
            authority,
            quote_lot_collateral: 0,
            flags: 0,
            withdraw_queue_node: 0,
            trader_pda_index: TRADER_PDA_INDEX,
            trader_subaccount_index: TRADER_SUBACCOUNT_INDEX,
            position_count: 0,
        }
    }

    #[test]
    fn trader_header_round_trips_through_its_offsets() {
        let trader = PhoenixTrader {
            quote_lot_collateral: -7,
            flags: TRADER_READY_FLAGS | 1,
            withdraw_queue_node: 3,
            position_count: 2,
            ..empty_trader(Pubkey::new_unique())
        };

        assert_eq!(PhoenixTrader::parse(&trader_data(&trader)).unwrap(), trader);
    }

    #[test]
    fn trader_validates_authority_and_indices() {
        let vault = Pubkey::new_unique();
        let trader = empty_trader(vault);

        assert!(trader.validate_authority(vault).is_ok());
        assert_err(
            trader.validate_authority(Pubkey::new_unique()),
            HedgeVaultError::InvalidPhoenixTrader,
        );
        assert_err(
            PhoenixTrader {
                trader_subaccount_index: 1,
                ..trader
            }
            .validate_authority(vault),
            HedgeVaultError::InvalidPhoenixTrader,
        );
    }

    #[test]
    fn trader_is_ready_only_with_every_ready_flag() {
        let trader = empty_trader(Pubkey::new_unique());

        assert_err(trader.is_ready(), HedgeVaultError::PhoenixTraderNotReady);
        assert_err(
            PhoenixTrader {
                flags: CAPABILITY_CAN_DEPOSIT | CAPABILITY_CAN_WITHDRAW,
                ..trader
            }
            .is_ready(),
            HedgeVaultError::PhoenixTraderNotReady,
        );
        assert!(PhoenixTrader {
            flags: TRADER_READY_FLAGS,
            ..trader
        }
        .is_ready()
        .is_ok());
    }

    #[test]
    fn trader_is_empty_without_collateral_positions_or_queued_withdrawal() {
        let trader = empty_trader(Pubkey::new_unique());
        assert!(trader.is_empty().is_ok());

        for not_empty in [
            PhoenixTrader {
                quote_lot_collateral: 1,
                ..trader
            },
            PhoenixTrader {
                position_count: 1,
                ..trader
            },
            PhoenixTrader {
                withdraw_queue_node: 1,
                ..trader
            },
        ] {
            assert_err(
                not_empty.is_empty(),
                HedgeVaultError::PhoenixStrategyNotEmpty,
            );
        }
    }

    #[test]
    fn matching_engine_response_reads_fills() {
        let words: [u64; 8] = [0, 0, 300, 0, 0, 5, 0, 0];
        let data: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();

        let response = MatchingEngineResponse::parse(&data).unwrap();
        assert_eq!(response.base_lots_filled(), 5);
        assert_eq!(response.quote_lots_filled(), 300);
        assert_eq!(response.order_sequence_number(), None);

        let words: [u64; 8] = [100, 42, 0, 0, 0, 0, 20, 2];
        let data: Vec<u8> = words.iter().flat_map(|word| word.to_le_bytes()).collect();

        let response = MatchingEngineResponse::parse(&data).unwrap();
        assert_eq!(response.order_sequence_number(), Some(42));
        assert_eq!(response.num_base_lots_posted, 2);

        assert_err(
            MatchingEngineResponse::parse(&data[..63]),
            HedgeVaultError::PhoenixReturnDataInvalid,
        );
    }

    /// `ViewMargin` return data captured from mainnet Hawkeye after a 1 SOL long.
    const MAINNET_VIEW_MARGIN_RETURN: [u8; 112] = [
        63, 37, 255, 61, 157, 91, 95, 149, 1, 0, 1, 0, 0, 0, 0, 0, 61, 252, 153, 59, 0, 0, 0, 0,
        29, 91, 146, 59, 0, 0, 0, 0, 61, 234, 31, 59, 0, 0, 0, 0, 61, 234, 31, 59, 0, 0, 0, 0, 224,
        112, 114, 0, 0, 0, 0, 0, 112, 56, 57, 0, 0, 0, 0, 0, 72, 241, 62, 0, 0, 0, 0, 0, 96, 227,
        22, 0, 0, 0, 0, 0, 176, 113, 11, 0, 0, 0, 0, 0, 224, 94, 248, 255, 255, 255, 255, 255, 224,
        94, 248, 255, 255, 255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0,
    ];

    #[test]
    fn view_margin_return_decodes_a_mainnet_response() {
        let mut data = MAINNET_VIEW_MARGIN_RETURN.to_vec();
        assert!(!parse_is_liquidatable(&data).unwrap());

        data[VIEW_MARGIN_IS_LIQUIDATABLE_OFFSET] = 1;
        assert!(parse_is_liquidatable(&data).unwrap());

        data[8] = 2;
        assert_err(
            parse_is_liquidatable(&data),
            HedgeVaultError::PhoenixReturnDataInvalid,
        );
        assert_err(
            parse_is_liquidatable(&MAINNET_VIEW_MARGIN_RETURN[..111]),
            HedgeVaultError::PhoenixReturnDataInvalid,
        );
    }
}
