use anchor_lang::prelude::*;
pub mod utils;
use anchor_lang::system_program;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use anchor_spl::token_interface::{transfer_checked, TransferChecked};
use crate::utils::resize_account;
use solana_program::{pubkey, pubkey::Pubkey};
mod errors;

// This is your program's public key and it will update
// automatically when you build the project.
declare_id!("6rbcJVHa32dKfw8kF1F1quSravjCLxpcjyuEqw7rP2Gc");

pub const STAKE_POOL_PREFIX: &str = "stake-pool";
pub const STAKE_POOL_DEFAULT_SIZE: usize = 8 + 1 + 32 + 16 + 16 + 32 + 16 + 16 + 32 + 1 + 24 + 24;
pub const STAKE_ENTRY_PREFIX: &str = "stake-entry";
pub const SUPER_ADMIN: Pubkey = pubkey!("Bx6Z6XxCSdwtqmiKP9prwU7m8NDuUcA11FtPdSZ5Fw9B");
pub const PLATFORM_FEE: u64 = 500000000; // 0.5 SOL

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
            token_address: ix.token_address,
            apr: ix.apr,
            end_date: ix.end_date,
            pool_name: ix.pool_name,
            default_multiplier: ix.default_multiplier,
            created_at: Clock::get().unwrap().unix_timestamp,
            is_active: true,
            identifier,
        };

        msg!("account, {:?}", ctx.accounts.mint);

        if Some(ctx.accounts.mint.mint_authority).is_some()
            && (ctx.accounts.mint.mint_authority
                != solana_program::program_option::COption::Some(ctx.accounts.payer.key()))
        {
            if ctx.accounts.payer.key() != SUPER_ADMIN {
                return err!(errors::ErrorCode::InvalidTokenAuthority);
            }
        }

        let cpi_accounts = Transfer {
            from: ctx.accounts.payer_token_account.to_account_info(),
            to: ctx.accounts.pool_token_account.to_account_info(),
            authority: ctx.accounts.payer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        token::transfer(cpi_ctx, ix.amount)?;

        let cpi_context = CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.payer.to_account_info(),
                to: ctx.accounts.super_admin.to_account_info(),
            },
        );
        system_program::transfer(cpi_context, PLATFORM_FEE)?;

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

        if !stake_pool.is_active {
            return err!(errors::ErrorCode::PoolFrozen);
        }

        if stake_pool.end_date.is_some()
            && Clock::get().unwrap().unix_timestamp > stake_pool.end_date.unwrap()
        {
            return err!(errors::ErrorCode::StakePoolHasEnded);
        }

        if stake_entry.amount > 0 {
            return err!(errors::ErrorCode::UnstakeAllTokens);
        }

        // let decimals_str = format!(
        //     "1{}",
        //     "0".repeat(stake_pool.default_multiplier.try_into().unwrap())
        // ); // Concatenating "1" with "0" repeated `number` times
        // let decimals: u64 = decimals_str.parse().expect("Failed to parse string to u64");

        let cpi_accounts = Transfer {
            from: ctx.accounts.payer_token_account.to_account_info(),
            to: ctx.accounts.entry_token_account.to_account_info(),
            authority: ctx.accounts.payer.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        token::transfer(cpi_ctx, ix.amount)?;

        stake_entry.apr = ix.apr;
        stake_entry.last_staker = ctx.accounts.payer.key();
        stake_entry.last_staked_at = Clock::get().unwrap().unix_timestamp;
        stake_entry.amount = stake_entry.amount.checked_add(ix.amount).unwrap();
        stake_entry.min_stake_seconds = ix.min_stake_seconds;
        stake_pool.total_staked = stake_pool.total_staked.checked_add(1).expect("Add error");
        Ok(())
    }

    pub fn unstake_token(ctx: Context<UnstakeCtx>, ix: UnstakeIx) -> Result<()> {
        let stake_entry = &mut ctx.accounts.stake_entry;
        let pool = &ctx.accounts.stake_pool.key();
        let stake_pool = &mut ctx.accounts.stake_pool;
        let payer = &ctx.accounts.payer.key();
        let stake_mint = &ctx.accounts.stake_mint.key();

        if !stake_pool.is_active {
            return err!(errors::ErrorCode::PoolFrozen);
        }

        let seeds = &[
            STAKE_ENTRY_PREFIX.as_bytes(),
            pool.as_ref(),
            stake_mint.as_ref(),
            payer.as_ref(),
            &[stake_entry.bump],
        ];

        let signer_seeds = &[&seeds[..]];

        if stake_entry.min_stake_seconds.is_some()
            && stake_entry.min_stake_seconds.unwrap() > 0
            && ((Clock::get().unwrap().unix_timestamp - stake_entry.last_staked_at) as u32)
                < stake_entry.min_stake_seconds.unwrap()
        {
            let deduction = ix.amount * 30 / 100;
            let remaining_amount = ix.amount - deduction;

            let deduction_accounts = TransferChecked {
                from: ctx.accounts.entry_token_account.to_account_info(),
                to: ctx.accounts.pool_token_account.to_account_info(),
                authority: stake_entry.to_account_info(),
                mint: ctx.accounts.stake_mint.to_account_info(),
            };

            let deduction_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                deduction_accounts,
                signer_seeds,
            );

            transfer_checked(
                deduction_ctx,
                deduction,
                stake_pool.default_multiplier as u8,
            )?;

            let accounts = TransferChecked {
                from: ctx.accounts.entry_token_account.to_account_info(),
                to: ctx.accounts.payer_token_account.to_account_info(),
                authority: stake_entry.to_account_info(),
                mint: ctx.accounts.stake_mint.to_account_info(),
            };

            let ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                accounts,
                signer_seeds,
            );

            transfer_checked(ctx, remaining_amount, stake_pool.default_multiplier as u8)?;
        } else {
            let pool_apr_amount = ix.amount * stake_pool.apr / 10000;
            let stake_apr_amount = pool_apr_amount * stake_entry.apr / 10000;

            let pool_seeds = &[
                STAKE_POOL_PREFIX.as_bytes(),
                stake_pool.identifier.as_ref(),
                &[stake_pool.bump],
            ];

            let pool_signer_seeds = &[&pool_seeds[..]];

            let pool_accounts = TransferChecked {
                from: ctx.accounts.pool_token_account.to_account_info(),
                to: ctx.accounts.payer_token_account.to_account_info(),
                authority: stake_pool.to_account_info(),
                mint: ctx.accounts.stake_mint.to_account_info(),
            };

            let pool_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                pool_accounts,
                pool_signer_seeds,
            );

            transfer_checked(
                pool_ctx,
                stake_apr_amount,
                stake_pool.default_multiplier as u8,
            )?;

            let accounts = TransferChecked {
                from: ctx.accounts.entry_token_account.to_account_info(),
                to: ctx.accounts.payer_token_account.to_account_info(),
                authority: stake_entry.to_account_info(),
                mint: ctx.accounts.stake_mint.to_account_info(),
            };

            let ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                accounts,
                signer_seeds,
            );

            transfer_checked(ctx, ix.amount, stake_pool.default_multiplier as u8)?;
        }

        stake_entry.amount = stake_entry.amount - ix.amount;
        if stake_entry.amount <= 0 {
            stake_pool.total_staked = stake_pool.total_staked.checked_sub(1).expect("Sub error");
        }
        Ok(())
    }

    pub fn claim_token(ctx: Context<UnstakeCtx>, ix: UnstakeIx) -> Result<()> {
        let stake_entry = &mut ctx.accounts.stake_entry;
        let stake_pool = &mut ctx.accounts.stake_pool;

        if !stake_pool.is_active {
            return err!(errors::ErrorCode::PoolFrozen);
        }

        if stake_entry.min_stake_seconds.is_some()
            && stake_entry.min_stake_seconds.unwrap() > 0
            && ((Clock::get().unwrap().unix_timestamp - stake_entry.last_staked_at) as u32)
                < stake_entry.min_stake_seconds.unwrap()
        {
            return err!(errors::ErrorCode::MinStakeSecondsNotSatisfied);
        }
        let pool_apr_amount = ix.amount * stake_pool.apr / 10000;
        let stake_apr_amount = pool_apr_amount * stake_entry.apr / 10000;

        let pool_seeds = &[
            STAKE_POOL_PREFIX.as_bytes(),
            stake_pool.identifier.as_ref(),
            &[stake_pool.bump],
        ];

        let pool_signer_seeds = &[&pool_seeds[..]];

        let pool_accounts = TransferChecked {
            from: ctx.accounts.pool_token_account.to_account_info(),
            to: ctx.accounts.payer_token_account.to_account_info(),
            authority: stake_pool.to_account_info(),
            mint: ctx.accounts.stake_mint.to_account_info(),
        };

        let pool_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            pool_accounts,
            pool_signer_seeds,
        );

        transfer_checked(
            pool_ctx,
            stake_apr_amount,
            stake_pool.default_multiplier as u8,
        )?;

        stake_entry.amount = stake_entry.amount - ix.amount;
        if stake_entry.amount <= 0 {
            stake_pool.total_staked = stake_pool.total_staked.checked_sub(1).expect("Sub error");
        }
        Ok(())
    }

    pub fn freeze_pool(ctx: Context<FreezePoolCtx>) -> Result<()> {
        let stake_pool = &mut ctx.accounts.stake_pool;
        stake_pool.is_active = false;
        Ok(())
    }

    pub fn unfreeze_pool(ctx: Context<UnfreezePoolCtx>) -> Result<()> {
        let stake_pool = &mut ctx.accounts.stake_pool;
        stake_pool.is_active = true;
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
        associated_token::mint = mint, 
        associated_token::authority = stake_pool
    )]
    pool_token_account: Account<'info, TokenAccount>,
    mint: Account<'info, Mint>,

    #[account(mut, constraint = super_admin.key() == SUPER_ADMIN @ errors::ErrorCode::InvalidSuperAdmin)]
    super_admin: UncheckedAccount<'info>,

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
        space = 134,
        seeds = [STAKE_ENTRY_PREFIX.as_bytes(), stake_pool.key().as_ref(), stake_mint.key().as_ref(), payer.key().as_ref()],
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

    #[account(mut)]
    payer: Signer<'info>,
    token_program: Program<'info, Token>,
    system_program: Program<'info, System>,
    associated_token_program: Program<'info, AssociatedToken>,
    rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct InitStakeCtx<'info> {
    #[account(mut)]
    stake_entry: Box<Account<'info, StakeEntry>>,
    #[account(mut)]
    stake_pool: Box<Account<'info, StakePool>>,
    #[account(mut)]
    entry_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    payer_token_account: Account<'info, TokenAccount>,
    token_program: Program<'info, Token>,
    associated_token_program: Program<'info, AssociatedToken>,
    rent: Sysvar<'info, Rent>,

    #[account(mut)]
    payer: Signer<'info>,
    stake_mint: Account<'info, Mint>,
    system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UnstakeCtx<'info> {
    #[account(mut, constraint = stake_entry.last_staker == payer.key() @ errors::ErrorCode::InvalidStaker)]
    stake_entry: Box<Account<'info, StakeEntry>>,
    #[account(mut)]
    stake_pool: Box<Account<'info, StakePool>>,
    #[account(mut)]
    entry_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pool_token_account: Account<'info, TokenAccount>,
    stake_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    payer_token_account: Account<'info, TokenAccount>,
    token_program: Program<'info, Token>,
    #[account(mut)]
    payer: Signer<'info>,
    system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ClaimCtx<'info> {
    #[account(mut, constraint = stake_entry.last_staker == payer.key() @ errors::ErrorCode::InvalidStaker)]
    stake_entry: Box<Account<'info, StakeEntry>>,
    #[account(mut)]
    stake_pool: Box<Account<'info, StakePool>>,
    #[account(mut)]
    entry_token_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pool_token_account: Account<'info, TokenAccount>,
    stake_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    payer_token_account: Account<'info, TokenAccount>,
    token_program: Program<'info, Token>,
    #[account(mut)]
    payer: Signer<'info>,
    system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FreezePoolCtx<'info> {
    #[account(mut, constraint = stake_pool.authority==payer.key() || payer.key()==SUPER_ADMIN @ errors::ErrorCode::InvalidAdmin)]
    stake_pool: Account<'info, StakePool>,
    #[account(mut)]
    payer: Signer<'info>,
    system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UnfreezePoolCtx<'info> {
    #[account(mut, constraint = stake_pool.authority==payer.key() || payer.key()==SUPER_ADMIN @ errors::ErrorCode::InvalidAdmin)]
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
    pub apr: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct InitStakeIx {
    amount: u64,
    min_stake_seconds: Option<u32>,
    apr: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UnstakeIx {
    pub amount: u64,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct UpdatePoolIx {
    min_stake_seconds: Option<u32>,
}
