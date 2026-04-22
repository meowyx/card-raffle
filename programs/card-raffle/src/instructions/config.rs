use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{Mint, Token, TokenAccount};

use crate::errors::RaffleError;
use crate::events::*;
use crate::state::Config;

pub fn handle_initialize_config(
    ctx: Context<InitializeConfig>,
    backend_signer: Pubkey,
    max_credit_per_call: u64,
    max_credit_per_epoch: u64,
    ticket_price: u64,
) -> Result<()> {
    require!(max_credit_per_call > 0, RaffleError::ZeroMaxCredit);
    require!(max_credit_per_epoch > 0, RaffleError::ZeroMaxCredit);
    require!(ticket_price > 0, RaffleError::ZeroTicketPrice);
    require!(
        max_credit_per_call <= max_credit_per_epoch,
        RaffleError::PerCallExceedsEpochCap
    );

    let config = &mut ctx.accounts.config;
    config.admin = ctx.accounts.admin.key();
    config.pending_admin = None;
    config.backend_signer = backend_signer;
    config.paused = false;
    config.max_credit_per_call = max_credit_per_call;
    config.max_credit_per_epoch = max_credit_per_epoch;
    config.current_epoch_total = 0;
    config.current_epoch_start_ts = 0;
    config.token_mint = ctx.accounts.token_mint.key();
    config.ticket_price = ticket_price;
    config.bump = ctx.bumps.config;

    emit!(ConfigInitialized {
        admin: config.admin,
        backend_signer,
        max_credit_per_call,
        max_credit_per_epoch,
        token_mint: config.token_mint,
        ticket_price,
    });
    Ok(())
}

pub fn handle_propose_admin(ctx: Context<ProposeAdmin>, new_admin: Pubkey) -> Result<()> {
    ctx.accounts.config.pending_admin = Some(new_admin);
    emit!(AdminProposed {
        current: ctx.accounts.config.admin,
        proposed: new_admin,
    });
    Ok(())
}

pub fn handle_accept_admin(ctx: Context<AcceptAdmin>) -> Result<()> {
    let config = &mut ctx.accounts.config;
    let pending = config.pending_admin.ok_or(RaffleError::NoPendingAdmin)?;
    require_keys_eq!(
        ctx.accounts.new_admin.key(),
        pending,
        RaffleError::NotPendingAdmin
    );
    let previous = config.admin;
    config.admin = pending;
    config.pending_admin = None;
    emit!(AdminAccepted {
        previous,
        new: pending,
    });
    Ok(())
}

pub fn handle_set_pause(ctx: Context<SetPause>, paused: bool) -> Result<()> {
    ctx.accounts.config.paused = paused;
    emit!(PauseChanged { paused });
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = 8 + Config::INIT_SPACE,
        seeds = [b"config"],
        bump,
    )]
    pub config: Box<Account<'info, Config>>,
    // Account<Mint> rejects Token-2022 via its owner check.
    pub token_mint: Box<Account<'info, Mint>>,
    #[account(
        init,
        payer = admin,
        associated_token::mint = token_mint,
        associated_token::authority = config,
    )]
    pub treasury_ata: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ProposeAdmin<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
        has_one = admin @ RaffleError::NotAdmin,
    )]
    pub config: Account<'info, Config>,
}

#[derive(Accounts)]
pub struct AcceptAdmin<'info> {
    pub new_admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, Config>,
}

#[derive(Accounts)]
pub struct SetPause<'info> {
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
        has_one = admin @ RaffleError::NotAdmin,
    )]
    pub config: Account<'info, Config>,
}
