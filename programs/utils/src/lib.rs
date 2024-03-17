use anchor_lang::prelude::*;
pub mod utils;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::utils::resize_account;
mod errors;

// This is your program's public key and it will update
// automatically when you build the project.
declare_id!("2jWG6JY5EuDjFu3KHuJt4BHr433B6e8FV1LtQGXZgNqF");

pub const STAKE_POOL_PREFIX: &str = "stake-pool";
pub const STAKE_POOL_DEFAULT_SIZE: usize = 8 + 1 + 32 + 16 + 16 + 32 + 16 + 16 + 32 + 1 + 24 + 24;
pub const STAKE_ENTRY_PREFIX: &str = "stake-entry";
pub const STAKE_ENTRY_SIZE: usize = 8 + std::mem::size_of::<StakeEntry>() + 8;

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
            // min_stake_seconds: ix.min_stake_seconds,
            token_address: ix.token_address,
            apr: ix.apr,
            end_date: ix.end_date,
            pool_name: ix.pool_name,
            default_multiplier: ix.default_multiplier,
            created_at: Clock::get().unwrap().unix_timestamp,
            is_active: true,
            identifier,
        };

        let cpi_accounts = Transfer {
            from: ctx.accounts.payer_token_account.to_account_info(),
            to: ctx.accounts.pool_token_account.to_account_info(),
            authority: ctx.accounts.payer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        token::transfer(cpi_ctx, ix.amount)?;

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

    pub fn init_stake_entry(ctx: Context<InitEntryCtx>) -> Result<()> {
        let stake_entry = &mut ctx.accounts.stake_entry;
        stake_entry.bump = ctx.bumps.stake_entry;
        stake_entry.pool = ctx.accounts.stake_pool.key();
        stake_entry.stake_mint = ctx.accounts.stake_mint.key();
        stake_entry.amount = 0;
        Ok(())
    }

    pub fn stake_token(ctx: Context<InitStakeCtx>, ix: InitStakeIx) -> Result<()> {
        let stake_entry = &mut ctx.accounts.stake_entry;
        let stake_pool = &mut ctx.accounts.stake_pool;
        if stake_pool.end_date.is_some()
            && Clock::get().unwrap().unix_timestamp > stake_pool.end_date.unwrap()
        {
            return Err(errors::ErrorCode::StakePoolHasEnded.into());
        }


        let cpi_accounts = Transfer {
            from: ctx.accounts.payer_token_account.to_account_info(),
            to: ctx.accounts.entry_token_account.to_account_info(),
            authority: ctx.accounts.payer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        token::transfer(cpi_ctx, ix.amount)?;

        stake_entry.last_staker = ctx.accounts.payer.key();
        stake_entry.last_staked_at = Clock::get().unwrap().unix_timestamp;
        stake_entry.amount = stake_entry.amount.checked_add(ix.amount).unwrap();
        stake_entry.min_stake_seconds = ix.min_stake_seconds;
        stake_pool.total_staked = stake_pool.total_staked.checked_add(1).expect("Add error");
        Ok(())
    }

    // pub fn update_pool(ctx: Context<UpdatePoolCtx>, ix: UpdatePoolIx) -> Result<()> {
    //     let stake_pool = &mut ctx.accounts.stake_pool;
    //     stake_pool.min_stake_seconds = ix.min_stake_seconds;
    //     Ok(())
    // }
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
    payer_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    payer: Signer<'info>,
    token_program: Program<'info, Token>,
    system_program: Program<'info, System>,
    associated_token_program: Program<'info, AssociatedToken>,
    rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct UpdatePoolCtx<'info> {
    #[account(mut, constraint = stake_pool.authority == payer.key())]
    stake_pool: Account<'info, StakePool>,
    #[account(mut)]
    payer: Signer<'info>,
    system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitEntryCtx<'info> {
    #[account(
        init,
        payer = payer,
        space = STAKE_ENTRY_SIZE,
        seeds = [STAKE_ENTRY_PREFIX.as_bytes(), stake_pool.key().as_ref(), stake_mint.key().as_ref()],
        bump,
    )]
    stake_entry: Box<Account<'info, StakeEntry>>,
    #[account(mut)]
    stake_pool: Box<Account<'info, StakePool>>,
    #[account(
        init_if_needed,
        payer = payer, 
        associated_token::mint = stake_mint, 
        associated_token::authority = stake_entry
    )]
    entry_token_account: Account<'info, TokenAccount>,
    stake_mint: Box<Account<'info, Mint>>,
    token_program: Program<'info, Token>,
    associated_token_program: Program<'info, AssociatedToken>,
    rent: Sysvar<'info, Rent>,

    #[account(mut)]
    payer: Signer<'info>,
    system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct InitStakeCtx<'info> {
    #[account(mut)]
    stake_entry: Box<Account<'info, StakeEntry>>,
    #[account(mut)]
    stake_pool: Box<Account<'info, StakePool>>,
    #[account(mut)]
    entry_token_account: Account<'info, TokenAccount>,

    stake_mint: Box<Account<'info, Mint>>,

    #[account(mut)]
    payer_token_account: Account<'info, TokenAccount>,
    token_program: Program<'info, Token>,
    associated_token_program: Program<'info, AssociatedToken>,
    rent: Sysvar<'info, Rent>,

    #[account(mut)]
    payer: Signer<'info>,
    system_program: Program<'info, System>,
}

#[account]
pub struct StakePool {
    pub bump: u8,
    pub authority: Pubkey,
    pub total_staked: u32,
    // pub min_stake_seconds: Option<u32>,
    pub token_address: Pubkey,
    pub apr: u64,
    pub end_date: Option<i64>,
    pub is_active: bool,
    pub identifier: String,
    pub pool_name: String,
    pub default_multiplier: u64,
    pub created_at: i64,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct InitPoolIx {
    authority: Pubkey,
    // min_stake_seconds: Option<u32>,
    token_address: Pubkey,
    apr: u64,
    is_active: bool,
    end_date: Option<i64>,
    identifier: String,
    pool_name: String,
    default_multiplier: u64,
    amount: u64,
}

#[account]
pub struct StakeEntry {
    pub bump: u8,
    pub pool: Pubkey,
    pub amount: u64,
    pub stake_mint: Pubkey,
    pub last_staker: Pubkey,
    pub last_staked_at: i64,
    pub min_stake_seconds: Option<u32>,
}

#[account]
pub struct InitStakeIx {
    pub amount: u64,
    pub min_stake_seconds: Option<u32>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UpdatePoolIx {
    min_stake_seconds: Option<u32>,
}
