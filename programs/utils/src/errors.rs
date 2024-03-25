use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("Stake pool has ended")]
    StakePoolHasEnded,
    #[msg("Please unstake all tokens before staking")]
    UnstakeAllTokens,
    #[msg("Pool is frozen")]
    PoolFrozen,
    #[msg("Invalid Pool Admin")]
    InvalidAdmin,
    #[msg("Invalid Super Admin")]
    InvalidSuperAdmin,
    #[msg("Invalid Token Authority")]
    InvalidTokenAuthority,
    #[msg("Minimum stake seconds not satisfied")]
    MinStakeSecondsNotSatisfied,
    #[msg("Invalid Staker")]
    InvalidStaker,
}
