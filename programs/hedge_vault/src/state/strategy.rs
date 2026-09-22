use anchor_lang::prelude::*;
use core::mem::size_of;

use crate::{error::HedgeVaultError, validate_pda, STRATEGY_VERSION};

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, Debug, PartialEq)]
pub enum StrategyType {
    /// Swap on Jupiter
    JupiterSwap { target_mint: Pubkey },
    /// Liquidity providing on Meteora DLMM
    MeteoraDlmm { position: Pubkey },
}

impl StrategyType {
    pub fn space(&self) -> usize {
        match self {
            StrategyType::JupiterSwap { .. } => 1 + size_of::<Pubkey>(),
            StrategyType::MeteoraDlmm { .. } => 1 + size_of::<Pubkey>(),
        }
    }
}

pub struct NewStrategyArgs {
    pub vault: Pubkey,
    pub id: u32,
    pub bump: u8,
    pub created_ts: i64,
    pub strategy_type: StrategyType,
}

/// Record of where a vault's funds are utilized. Holds no accounting, NAV is tracked off-chain.
#[account]
pub struct Strategy {
    /// The vault this strategy belongs to.
    pub vault: Pubkey,
    /// Timestamp the strategy was created.
    pub created_ts: i64,
    /// Timestamp of the last execute/exit on the strategy.
    pub last_action_ts: i64,
    /// ID unique to the strategy within the vault.
    pub id: u32,
    pub bump: u8,
    /// Layout version, see [STRATEGY_VERSION].
    pub version: u8,
    padding0: [u8; 2],
    /// Reserved for future fields, the enum has to stay last.
    pub reserved: [u8; 32],
    /// Details about the underlying protocol and action of the strategy.
    pub strategy_type: StrategyType,
}

impl Strategy {
    pub fn new(args: NewStrategyArgs) -> Self {
        Self {
            vault: args.vault,
            created_ts: args.created_ts,
            last_action_ts: args.created_ts,
            id: args.id,
            bump: args.bump,
            version: STRATEGY_VERSION,
            padding0: [0; 2],
            reserved: [0; 32],
            strategy_type: args.strategy_type,
        }
    }

    pub fn validate_address(seeds: &[&[u8]], key: Pubkey) -> Result<()> {
        validate_pda(seeds, key, HedgeVaultError::InvalidStrategy.into())
    }

    pub fn record_action(&mut self, now: i64) {
        self.last_action_ts = now;
    }
}

impl Space for Strategy {
    const INIT_SPACE: usize = size_of::<Pubkey>()
        + size_of::<i64>()
        + size_of::<i64>()
        + size_of::<u32>()
        + size_of::<u8>()
        + size_of::<u8>()
        + size_of::<u8>() * 2
        + size_of::<u8>() * 32;
}
