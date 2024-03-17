use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Stake pool has ended")]
    StakePoolHasEnded,
}