use anchor_lang::prelude::*;
use num_traits::ToPrimitive;
use std::panic::Location;

use crate::error::HedgeVaultError;

pub trait SafeMath: Sized {
    fn safe_add(self, rhs: Self) -> Result<Self>;
    fn safe_sub(self, rhs: Self) -> Result<Self>;
    fn safe_mul(self, rhs: Self) -> Result<Self>;
    fn safe_div(self, rhs: Self) -> Result<Self>;
}

macro_rules! checked_impl {
    ($t:ty) => {
        impl SafeMath for $t {
            #[track_caller]
            #[inline(always)]
            fn safe_add(self, rhs: $t) -> Result<$t> {
                match self.checked_add(rhs) {
                    Some(result) => Ok(result),
                    None => {
                        let caller = Location::caller();
                        msg!("Math overflow at {}:{}", caller.file(), caller.line());
                        err!(HedgeVaultError::MathOverflow)
                    }
                }
            }

            #[track_caller]
            #[inline(always)]
            fn safe_sub(self, rhs: $t) -> Result<$t> {
                match self.checked_sub(rhs) {
                    Some(result) => Ok(result),
                    None => {
                        let caller = Location::caller();
                        msg!("Math underflow at {}:{}", caller.file(), caller.line());
                        err!(HedgeVaultError::MathOverflow)
                    }
                }
            }

            #[track_caller]
            #[inline(always)]
            fn safe_mul(self, rhs: $t) -> Result<$t> {
                match self.checked_mul(rhs) {
                    Some(result) => Ok(result),
                    None => {
                        let caller = Location::caller();
                        msg!("Math overflow at {}:{}", caller.file(), caller.line());
                        err!(HedgeVaultError::MathOverflow)
                    }
                }
            }

            #[track_caller]
            #[inline(always)]
            fn safe_div(self, rhs: $t) -> Result<$t> {
                match self.checked_div(rhs) {
                    Some(result) => Ok(result),
                    None => {
                        let caller = Location::caller();
                        msg!("Division error at {}:{}", caller.file(), caller.line());
                        err!(HedgeVaultError::MathOverflow)
                    }
                }
            }
        }
    };
}

checked_impl!(u8);
checked_impl!(u16);
checked_impl!(u32);
checked_impl!(u64);
checked_impl!(u128);
checked_impl!(i64);
checked_impl!(i128);

pub trait SafeMathAssign: Sized {
    fn safe_add_assign(&mut self, rhs: Self) -> Result<()>;
    fn safe_sub_assign(&mut self, rhs: Self) -> Result<()>;
}

macro_rules! assign_impl {
    ($t:ty) => {
        impl SafeMathAssign for $t {
            #[track_caller]
            #[inline(always)]
            fn safe_add_assign(&mut self, rhs: $t) -> Result<()> {
                *self = self.safe_add(rhs)?;
                Ok(())
            }

            #[track_caller]
            #[inline(always)]
            fn safe_sub_assign(&mut self, rhs: $t) -> Result<()> {
                *self = self.safe_sub(rhs)?;
                Ok(())
            }
        }
    };
}

assign_impl!(u8);
assign_impl!(u16);
assign_impl!(u32);
assign_impl!(u64);
assign_impl!(i64);
assign_impl!(u128);

pub trait SafeConvert {
    fn safe_to_u64(self) -> Result<u64>;
}

macro_rules! to_impl {
    ($t:ty) => {
        impl SafeConvert for $t {
            #[track_caller]
            #[inline(always)]
            fn safe_to_u64(self) -> Result<u64> {
                match self.to_u64() {
                    Some(result) => Ok(result),
                    None => {
                        let caller = Location::caller();
                        msg!(
                            "Conversion to u64 failed at {}:{}",
                            caller.file(),
                            caller.line()
                        );
                        err!(HedgeVaultError::ConversionFailed)
                    }
                }
            }
        }
    };
}

to_impl!(u128);
to_impl!(i64);
