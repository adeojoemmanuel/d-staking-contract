use anchor_lang::prelude::*;
pub mod utils;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::utils::resize_account;

// This is your program's public key and it will update
// automatically when you build the project.
declare_id!("2jWG6JY5EuDjFu3KHuJt4BHr433B6e8FV1LtQGXZgNqF");

pub const STAKE_POOL_DEFAULT_SIZE: usize = 8 + 1 + 32 + 8 + 32 + 32 + 24;
pub const STAKE_POOL_PREFIX: &str = "stake-pool";

#[program]
mod dyme_staking {
    use super::*;
    pub fn init_pool(ctx: Context<InitPoolCtx>, ix: InitPoolIx) -> Result<()> {
        let bump = ctx.bumps.stake_pool;
        let identifier = ix.identifier;
        let new_stake_pool = StakePool {
            bump,
            authority: ix.authority,
            total_staked: 0,
            min_stake_seconds: ix.min_stake_seconds,
            stake_payment_info: ix.stake_payment_info,
            allowed_creators: ix.allowed_creators,
            reward_amount: ix.reward_amount,
            reward_seconds: ix.reward_seconds,
            is_active: true,
            identifier,
        };

        let stake_pool = &mut ctx.accounts.stake_pool;
        let new_space = new_stake_pool.try_to_vec()?.len() + 8;

        resize_account(
            &stake_pool.to_account_info(),
            new_space,
            &ctx.accounts.payer.to_account_info(),
            &ctx.accounts.system_program.to_account_info(),
        )?;

        stake_pool.set_inner(new_stake_pool);
        Ok(())
    }

    pub fn update_pool(ctx: Context<UpdatePoolCtx>, ix: UpdatePoolIx) -> Result<()> {
        let stake_pool = &mut ctx.accounts.stake_pool;
        stake_pool.min_stake_seconds = ix.min_stake_seconds;
        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(ix: InitPoolIx)]
pub struct InitPoolCtx<'info> {
    #[account(
        init,
        payer = payer,
        space = STAKE_POOL_DEFAULT_SIZE,
        seeds = [STAKE_POOL_PREFIX.as_bytes(), ix.identifier.as_ref()],
        bump
    )]
    stake_pool: Account<'info, StakePool>,
    #[account(
        init_if_needed,
        payer = payer, 
        associated_token::mint = token_address, 
        associated_token::authority = stake_pool
    )]
    pool_token_account: Account<'info, TokenAccount>,
    token_address: Account<'info, Mint>,

    #[account(mut)]
    payer: Signer<'info>,
    token_program: Program<'info, Token>,
    system_program: Program<'info, System>,
    associated_token_program: Program<'info, AssociatedToken>,
    rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct UpdatePoolCtx<'info> {
    #[account(mut)]
    stake_pool: Account<'info, StakePool>,
    #[account(mut)]
    payer: Signer<'info>,
    system_program: Program<'info, System>,
}

#[account]
pub struct StakePool {
    pub bump: u8,
    pub authority: Pubkey,
    pub total_staked: u32,
    pub min_stake_seconds: Option<u32>,
    pub stake_payment_info: Pubkey,
    pub allowed_creators: Vec<Pubkey>,
    pub reward_amount: u32,
    pub reward_seconds: Option<u32>,
    pub is_active: bool,
    pub identifier: String,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct InitPoolIx {
    allowed_creators: Vec<Pubkey>,
    authority: Pubkey,
    min_stake_seconds: Option<u32>,
    stake_payment_info: Pubkey,
    reward_amount: u32,
    reward_seconds: Option<u32>,
    is_active: bool,
    identifier: String,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UpdatePoolIx {
    min_stake_seconds: Option<u32>,
}
