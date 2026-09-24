/// taken from https://github.com/L0STE/anchor-jupiter-CPI/blob/main/programs/anchor-jupiter-cpi/src/swap.rs
use anchor_lang::{
    prelude::*,
    solana_program::{instruction::Instruction, program::invoke_signed},
};
use std::mem::size_of;

use crate::{
    error::HedgeVaultError,
    jupiter::accounts::TokenLedger,
    jupiter::client::args::{
        ExactOutRoute, Route, RouteWithTokenLedger, SharedAccountsExactOutRoute,
        SharedAccountsRoute,
    },
    validate, SafeConvert, SafeMath, MAX_BPS,
};

pub const JUPITER_AGGREGATOR_EVENT_AUTHORITY: Pubkey =
    pubkey!("D8cy77BBepLMngZx6ZukaTff5hCt1HrWyKk3Hnd9oitf");

/// Every supported route instruction ends with
/// `amount: u64, quoted_amount: u64, slippage_bps: u16, platform_fee_bps: u8`.
const SLIPPAGE_TAIL: usize = size_of::<u16>() + size_of::<u8>();
const ARGS_TAIL: usize = size_of::<u64>() * 2 + SLIPPAGE_TAIL;
const TOKEN_LEDGER_ARGS_TAIL: usize = size_of::<u64>() + SLIPPAGE_TAIL;

/// Which side of a routed swap the caller fixed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapMode {
    /// `amount` is the input, `quoted_amount` is the expected output.
    ExactIn,
    /// `amount` is the output, `quoted_amount` is the expected input.
    ExactOut,
}

/// Jupiter v6 route instructions the vault is allowed to relay.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapVariant {
    Route,
    RouteWithTokenLedger,
    ExactOutRoute,
    SharedAccountsRoute,
    SharedAccountsExactOutRoute,
}

impl SwapVariant {
    pub fn from_swap_data(swap_data: &[u8]) -> Result<Self> {
        match swap_data {
            data if data.starts_with(ExactOutRoute::DISCRIMINATOR) => Ok(Self::ExactOutRoute),
            data if data.starts_with(Route::DISCRIMINATOR) => Ok(Self::Route),
            data if data.starts_with(RouteWithTokenLedger::DISCRIMINATOR) => {
                Ok(Self::RouteWithTokenLedger)
            }
            data if data.starts_with(SharedAccountsExactOutRoute::DISCRIMINATOR) => {
                Ok(Self::SharedAccountsExactOutRoute)
            }
            data if data.starts_with(SharedAccountsRoute::DISCRIMINATOR) => {
                Ok(Self::SharedAccountsRoute)
            }
            _ => Err(Error::from(ProgramError::InvalidInstructionData)),
        }
    }

    pub fn mode(self) -> SwapMode {
        match self {
            Self::Route | Self::RouteWithTokenLedger | Self::SharedAccountsRoute => {
                SwapMode::ExactIn
            }
            Self::ExactOutRoute | Self::SharedAccountsExactOutRoute => SwapMode::ExactOut,
        }
    }

    pub fn uses_token_ledger(self) -> bool {
        matches!(self, Self::RouteWithTokenLedger)
    }
}

/// Trailing arguments shared by every supported route instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RouteArgs {
    /// Input amount for an exact-in route, output amount for an exact-out route.
    pub amount: u64,
    /// Quoted output for an exact-in route, quoted input for an exact-out route.
    pub quoted_amount: u64,
    pub slippage_bps: u16,
}

impl RouteArgs {
    pub fn parse(swap_data: &[u8]) -> Result<Self> {
        validate!(
            swap_data.len() >= ARGS_TAIL,
            HedgeVaultError::InvalidInstructionData
        )?;

        let bps_offset = swap_data.len() - SLIPPAGE_TAIL;
        let quoted_offset = bps_offset - size_of::<u64>();
        let amount_offset = quoted_offset - size_of::<u64>();

        Ok(Self {
            amount: u64::from_le_bytes(
                swap_data[amount_offset..quoted_offset].try_into().unwrap(),
            ),
            quoted_amount: u64::from_le_bytes(
                swap_data[quoted_offset..bps_offset].try_into().unwrap(),
            ),
            slippage_bps: u16::from_le_bytes(
                swap_data[bps_offset..bps_offset + size_of::<u16>()]
                    .try_into()
                    .unwrap(),
            ),
        })
    }

    pub fn parse_with_token_ledger(swap_data: &[u8]) -> Result<Self> {
        validate!(
            swap_data.len() >= TOKEN_LEDGER_ARGS_TAIL,
            HedgeVaultError::InvalidInstructionData
        )?;

        let bps_offset = swap_data.len() - SLIPPAGE_TAIL;
        let quoted_offset = bps_offset - size_of::<u64>();

        Ok(Self {
            // Jupiter reads the input from the token ledger captured earlier in the transaction.
            amount: 0,
            quoted_amount: u64::from_le_bytes(
                swap_data[quoted_offset..bps_offset].try_into().unwrap(),
            ),
            slippage_bps: u16::from_le_bytes(
                swap_data[bps_offset..bps_offset + size_of::<u16>()]
                    .try_into()
                    .unwrap(),
            ),
        })
    }
}

/// Smallest destination gain accepted for an exact-in route at the vault's slippage cap.
pub fn min_amount_out(quoted_out_amount: u64, max_slippage_bps: u16) -> Result<u64> {
    (quoted_out_amount as u128)
        .safe_mul(MAX_BPS.safe_sub(max_slippage_bps)? as u128)?
        .safe_div(MAX_BPS as u128)?
        .safe_to_u64()
}

/// Largest source spend accepted for an exact-out route at the vault's slippage cap.
pub fn max_amount_in(quoted_in_amount: u64, max_slippage_bps: u16) -> Result<u64> {
    (quoted_in_amount as u128)
        .safe_mul((MAX_BPS as u128).safe_add(max_slippage_bps as u128)?)?
        .safe_div(MAX_BPS as u128)?
        .safe_to_u64()
}

pub struct JupiterSwapCpi<'info> {
    pub source_token_program: AccountInfo<'info>,
    pub destination_token_program: AccountInfo<'info>,
    pub token_account_authority: AccountInfo<'info>,
    pub source_token_account: AccountInfo<'info>,
    pub destination_token_account: AccountInfo<'info>,
    pub source_mint: AccountInfo<'info>,
    pub destination_mint: AccountInfo<'info>,
    pub event_authority: AccountInfo<'info>,
    pub jupiter_program: AccountInfo<'info>,
}

impl<'info> JupiterSwapCpi<'info> {
    pub fn check_amount_and_slippage(
        swap_data: &[u8],
        amount: u64,
        slippage_bps: u16,
        max_slippage_bps: u16,
    ) -> Result<RouteArgs> {
        validate!(
            swap_data.len() >= TOKEN_LEDGER_ARGS_TAIL,
            HedgeVaultError::InvalidInstructionData
        )?;
        let variant = SwapVariant::from_swap_data(swap_data)?;
        let args = if variant.uses_token_ledger() {
            validate!(amount > 0, HedgeVaultError::InvalidInstructionData)?;
            RouteArgs::parse_with_token_ledger(swap_data)?
        } else {
            let args = RouteArgs::parse(swap_data)?;
            require_eq!(amount, args.amount);
            args
        };
        require_gte!(slippage_bps, args.slippage_bps);

        validate!(
            args.slippage_bps <= max_slippage_bps,
            HedgeVaultError::SlippageExceedsCap
        )?;
        validate!(
            slippage_bps <= max_slippage_bps,
            HedgeVaultError::SlippageExceedsCap
        )?;

        Ok(args)
    }

    pub fn swap(
        &mut self,
        swap_data: &[u8],
        remaining_accounts: &[AccountInfo<'info>],
        signer_seeds: &[&[u8]],
    ) -> Result<()> {
        let token_programs = [&self.source_token_program, &self.destination_token_program];
        // Jupiter always takes SPL Token as `token_program` and moves Token-2022 legs through the
        // optional `token_2022_program` slot, left as `None` when neither leg is Token-2022
        let token_program = anchor_spl::token::ID;
        let token_2022_program = token_programs
            .into_iter()
            .find(|program| program.key() == anchor_spl::token_2022::ID)
            .map_or_else(|| self.jupiter_program.key(), |program| program.key());

        let (account_infos, accounts) = match SwapVariant::from_swap_data(swap_data)? {
            SwapVariant::ExactOutRoute => {
                let mut account_infos = vec![
                    self.source_token_program.to_account_info(),
                    self.destination_token_program.to_account_info(),
                    self.token_account_authority.to_account_info(),
                    self.source_token_account.to_account_info(),
                    self.destination_token_account.to_account_info(),
                    self.source_mint.to_account_info(),
                    self.destination_mint.to_account_info(),
                    self.event_authority.to_account_info(),
                    self.jupiter_program.to_account_info(),
                ];
                account_infos.extend(
                    remaining_accounts
                        .iter()
                        .map(|acc| AccountInfo { ..acc.clone() }),
                );

                let mut accounts = vec![
                    AccountMeta::new_readonly(token_program, false), // token program
                    AccountMeta::new_readonly(self.token_account_authority.key(), true), // user transfer authority
                    AccountMeta::new(self.source_token_account.key(), false), // user source token account
                    AccountMeta::new(self.destination_token_account.key(), false), // user destination token account
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // [optional] destination token account
                    AccountMeta::new_readonly(self.source_mint.key(), false),     // source mint
                    AccountMeta::new_readonly(self.destination_mint.key(), false), // destination mint
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // [optional] platform fee account
                    AccountMeta::new_readonly(token_2022_program, false), // [optional] token 2022 program
                    AccountMeta::new_readonly(self.event_authority.key(), false), // event authority
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // jupiter program
                ];
                accounts.extend(remaining_accounts.iter().map(|acc| AccountMeta {
                    pubkey: *acc.key,
                    is_signer: acc.is_signer,
                    is_writable: acc.is_writable,
                }));

                (account_infos, accounts)
            }
            SwapVariant::Route => {
                let mut account_infos = vec![
                    self.source_token_program.to_account_info(),
                    self.destination_token_program.to_account_info(),
                    self.token_account_authority.to_account_info(),
                    self.source_token_account.to_account_info(),
                    self.destination_token_account.to_account_info(),
                    self.destination_mint.to_account_info(),
                    self.event_authority.to_account_info(),
                    self.jupiter_program.to_account_info(),
                ];
                account_infos.extend(
                    remaining_accounts
                        .iter()
                        .map(|acc| AccountInfo { ..acc.clone() }),
                );

                let mut accounts = vec![
                    AccountMeta::new_readonly(token_program, false), // token program
                    AccountMeta::new_readonly(self.token_account_authority.key(), true), // user transfer authority
                    AccountMeta::new(self.source_token_account.key(), false), // user source token account
                    AccountMeta::new(self.destination_token_account.key(), false), // user destination token account
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // [optional] destination token account
                    AccountMeta::new_readonly(self.destination_mint.key(), false), // destination mint
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // [optional] platform fee account
                    AccountMeta::new_readonly(self.event_authority.key(), false), // event authority
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // jupiter program
                ];
                accounts.extend(remaining_accounts.iter().map(|acc| AccountMeta {
                    pubkey: *acc.key,
                    is_signer: acc.is_signer,
                    is_writable: acc.is_writable,
                }));

                (account_infos, accounts)
            }
            SwapVariant::RouteWithTokenLedger => {
                validate!(
                    !remaining_accounts.is_empty(),
                    HedgeVaultError::InvalidRemainingAccounts
                )?;
                let token_ledger = &remaining_accounts[0];
                validate!(
                    *token_ledger.owner == self.jupiter_program.key(),
                    HedgeVaultError::InvalidProgramId
                )?;
                validate!(
                    TokenLedger::try_deserialize(&mut &token_ledger.try_borrow_data()?[..])?
                        .token_account
                        == self.source_token_account.key(),
                    HedgeVaultError::InvalidTokenAccountOwner
                )?;
                let mut account_infos = vec![
                    self.source_token_program.to_account_info(),
                    self.destination_token_program.to_account_info(),
                    self.token_account_authority.to_account_info(),
                    self.source_token_account.to_account_info(),
                    self.destination_token_account.to_account_info(),
                    self.destination_mint.to_account_info(),
                    token_ledger.to_account_info(),
                    self.event_authority.to_account_info(),
                    self.jupiter_program.to_account_info(),
                ];
                account_infos.extend(
                    remaining_accounts
                        .iter()
                        .skip(1)
                        .map(|acc| AccountInfo { ..acc.clone() }),
                );

                let mut accounts = vec![
                    AccountMeta::new_readonly(token_program, false), // token program
                    AccountMeta::new_readonly(self.token_account_authority.key(), true), // user transfer authority
                    AccountMeta::new(self.source_token_account.key(), false), // user source token account
                    AccountMeta::new(self.destination_token_account.key(), false), // user destination token account
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // [optional] destination token account
                    AccountMeta::new_readonly(self.destination_mint.key(), false), // destination mint
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // [optional] platform fee account
                    AccountMeta::new_readonly(token_ledger.key(), false),
                    AccountMeta::new_readonly(self.event_authority.key(), false),
                    AccountMeta::new_readonly(self.jupiter_program.key(), false),
                ];
                accounts.extend(remaining_accounts.iter().skip(1).map(|acc| AccountMeta {
                    pubkey: *acc.key,
                    is_signer: acc.is_signer,
                    is_writable: acc.is_writable,
                }));

                (account_infos, accounts)
            }
            SwapVariant::SharedAccountsExactOutRoute | SwapVariant::SharedAccountsRoute => {
                let mut account_infos = vec![
                    self.source_token_program.to_account_info(),
                    self.destination_token_program.to_account_info(),
                    remaining_accounts[0].to_account_info(),
                    self.token_account_authority.to_account_info(),
                    self.source_token_account.to_account_info(),
                    remaining_accounts[1].to_account_info(),
                    remaining_accounts[2].to_account_info(),
                    self.destination_token_account.to_account_info(),
                    self.source_mint.to_account_info(),
                    self.destination_mint.to_account_info(),
                    self.event_authority.to_account_info(),
                    self.jupiter_program.to_account_info(),
                ];
                account_infos.extend(
                    remaining_accounts
                        .iter()
                        .map(|acc| AccountInfo { ..acc.clone() }),
                );

                let mut accounts = vec![
                    AccountMeta::new_readonly(token_program, false), // token program
                    AccountMeta::new_readonly(remaining_accounts[0].key(), false), // program authority
                    AccountMeta::new_readonly(self.token_account_authority.key(), true), // user transfer authority
                    AccountMeta::new(self.source_token_account.key(), false), // source token account
                    AccountMeta::new(remaining_accounts[1].key(), false), // program source token account
                    AccountMeta::new(remaining_accounts[2].key(), false), // program destination token account
                    AccountMeta::new(self.destination_token_account.key(), false), // destination token account
                    AccountMeta::new_readonly(self.source_mint.key(), false),      // source mint
                    AccountMeta::new_readonly(self.destination_mint.key(), false), // destination mint
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // [optional] platform fee account
                    AccountMeta::new_readonly(token_2022_program, false), // [optional] token 2022 program
                    AccountMeta::new_readonly(self.event_authority.key(), false), // event authority
                    AccountMeta::new_readonly(self.jupiter_program.key(), false), // jupiter program
                ];
                accounts.extend(remaining_accounts.iter().skip(3).map(|acc| AccountMeta {
                    pubkey: *acc.key,
                    is_signer: acc.is_signer,
                    is_writable: acc.is_writable,
                }));

                (account_infos, accounts)
            }
        };

        invoke_signed(
            &Instruction {
                program_id: self.jupiter_program.key(),
                accounts,
                data: swap_data.to_vec(),
            },
            &account_infos,
            &[signer_seeds],
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `route` instruction data with an empty route plan.
    fn route_data(amount: u64, quoted_amount: u64, slippage_bps: u16) -> Vec<u8> {
        let mut data = Route::DISCRIMINATOR.to_vec();
        data.extend_from_slice(&0u32.to_le_bytes()); // empty route_plan vec
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&quoted_amount.to_le_bytes());
        data.extend_from_slice(&slippage_bps.to_le_bytes());
        data.push(0); // platform_fee_bps

        data
    }

    fn exact_out_route_data(amount: u64, quoted_amount: u64, slippage_bps: u16) -> Vec<u8> {
        let mut data = ExactOutRoute::DISCRIMINATOR.to_vec();
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&amount.to_le_bytes());
        data.extend_from_slice(&quoted_amount.to_le_bytes());
        data.extend_from_slice(&slippage_bps.to_le_bytes());
        data.push(0);

        data
    }

    fn token_ledger_route_data(quoted_amount: u64, slippage_bps: u16) -> Vec<u8> {
        let mut data = RouteWithTokenLedger::DISCRIMINATOR.to_vec();
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&quoted_amount.to_le_bytes());
        data.extend_from_slice(&slippage_bps.to_le_bytes());
        data.push(0);

        data
    }

    fn assert_err<T: core::fmt::Debug>(result: Result<T>, error: HedgeVaultError) {
        assert_eq!(result.unwrap_err(), anchor_lang::error::Error::from(error));
    }

    #[test]
    fn parse_reads_the_trailing_route_arguments() {
        let args = RouteArgs::parse(&route_data(1_000, 2_000, 50)).unwrap();

        assert_eq!(
            args,
            RouteArgs {
                amount: 1_000,
                quoted_amount: 2_000,
                slippage_bps: 50,
            }
        );
    }

    #[test]
    fn parse_rejects_data_shorter_than_the_argument_tail() {
        for len in 0..ARGS_TAIL {
            assert_err(
                RouteArgs::parse(&vec![0u8; len]),
                HedgeVaultError::InvalidInstructionData,
            );
        }

        assert!(RouteArgs::parse(&vec![0u8; ARGS_TAIL]).is_ok());
    }

    #[test]
    fn token_ledger_route_reads_quote_without_a_fixed_input() {
        let data = token_ledger_route_data(2_000, 50);
        let args = JupiterSwapCpi::check_amount_and_slippage(&data, 1_000, 50, 300).unwrap();

        assert_eq!(args.amount, 0);
        assert_eq!(args.quoted_amount, 2_000);
        assert_eq!(args.slippage_bps, 50);
    }

    #[test]
    fn token_ledger_route_rejects_a_zero_quote_input() {
        assert_err(
            JupiterSwapCpi::check_amount_and_slippage(
                &token_ledger_route_data(2_000, 50),
                0,
                50,
                300,
            ),
            HedgeVaultError::InvalidInstructionData,
        );
    }

    #[test]
    fn check_amount_and_slippage_accepts_a_quote_within_the_cap() {
        let data = route_data(1_000, 2_000, 100);

        let args = JupiterSwapCpi::check_amount_and_slippage(&data, 1_000, 100, 300).unwrap();
        assert_eq!(args.quoted_amount, 2_000);
    }

    #[test]
    fn check_amount_and_slippage_rejects_an_embedded_slippage_above_the_cap() {
        let data = route_data(1_000, 2_000, 400);

        assert_err(
            JupiterSwapCpi::check_amount_and_slippage(&data, 1_000, 400, 300),
            HedgeVaultError::SlippageExceedsCap,
        );
    }

    #[test]
    fn check_amount_and_slippage_rejects_a_caller_slippage_above_the_cap() {
        let data = route_data(1_000, 2_000, 100);

        assert_err(
            JupiterSwapCpi::check_amount_and_slippage(&data, 1_000, 400, 300),
            HedgeVaultError::SlippageExceedsCap,
        );
    }

    #[test]
    fn check_amount_and_slippage_rejects_short_data() {
        assert_err(
            JupiterSwapCpi::check_amount_and_slippage(&[0u8; 8], 0, 0, 300),
            HedgeVaultError::InvalidInstructionData,
        );
    }

    #[test]
    fn swap_variant_maps_discriminators_to_modes() {
        assert_eq!(
            SwapVariant::from_swap_data(&route_data(0, 0, 0)).unwrap().mode(),
            SwapMode::ExactIn
        );
        assert_eq!(
            SwapVariant::from_swap_data(&exact_out_route_data(0, 0, 0))
                .unwrap()
                .mode(),
            SwapMode::ExactOut
        );
        assert_eq!(
            SwapVariant::from_swap_data(&token_ledger_route_data(0, 0))
                .unwrap()
                .mode(),
            SwapMode::ExactIn
        );
        assert!(SwapVariant::from_swap_data(&[7u8; 32]).is_err());
    }

    #[test]
    fn min_amount_out_floors_the_quote_at_the_cap() {
        assert_eq!(min_amount_out(1_000_000, 300).unwrap(), 970_000);
        assert_eq!(min_amount_out(1_000_000, 0).unwrap(), 1_000_000);
        assert_eq!(min_amount_out(1_000_000, MAX_BPS).unwrap(), 0);
        // 999 * 9_700 / 10_000 = 969.03, floored
        assert_eq!(min_amount_out(999, 300).unwrap(), 969);
    }

    #[test]
    fn max_amount_in_caps_the_quoted_input() {
        assert_eq!(max_amount_in(1_000_000, 300).unwrap(), 1_030_000);
        assert_eq!(max_amount_in(1_000_000, 0).unwrap(), 1_000_000);
        // 999 * 10_300 / 10_000 = 1028.97, floored
        assert_eq!(max_amount_in(999, 300).unwrap(), 1_028);
    }
}
