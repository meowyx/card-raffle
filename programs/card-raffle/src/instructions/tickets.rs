use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

use crate::errors::RaffleError;
use crate::events::{TicketsCredited, TicketsPurchased};
use crate::state::{Config, PaymentReceipt, UserAccount};

/// Rolling window for the per-epoch credit cap.
pub const EPOCH_LENGTH_SECS: i64 = 86_400;

/// Backend-signed credit. Used when payment settles off-chain and the
/// backend issues tickets. Replay is blocked by `init` on the
/// PaymentReceipt PDA — a second call with the same `payment_id` fails
/// at account creation.
pub fn handle_credit_tickets(
    ctx: Context<CreditTickets>,
    payment_id: [u8; 32],
    amount: u64,
) -> Result<()> {
    require!(!ctx.accounts.config.paused, RaffleError::Paused);
    require!(amount > 0, RaffleError::ZeroAmount);
    require!(
        amount <= ctx.accounts.config.max_credit_per_call,
        RaffleError::ExceedsMaxCredit
    );

    let now = Clock::get()?.unix_timestamp;
    let config = &mut ctx.accounts.config;
    apply_epoch_cap(config, amount, now)?;

    let receipt = &mut ctx.accounts.payment_receipt;
    receipt.payment_id = payment_id;
    receipt.user = ctx.accounts.user_account.owner;
    receipt.amount = amount;
    receipt.credited_at = now;
    receipt.bump = ctx.bumps.payment_receipt;

    let user = &mut ctx.accounts.user_account;
    user.ticket_balance = user
        .ticket_balance
        .checked_add(amount)
        .ok_or(RaffleError::Overflow)?;
    user.total_purchased = user
        .total_purchased
        .checked_add(amount)
        .ok_or(RaffleError::Overflow)?;

    emit!(TicketsCredited {
        user: user.owner,
        amount,
        payment_id,
    });
    Ok(())
}

/// User-signed purchase. Transfers `ticket_count * ticket_price` from
/// the user's ATA to the treasury ATA.
pub fn handle_buy_tickets(
    ctx: Context<BuyTickets>,
    payment_id: [u8; 32],
    ticket_count: u64,
) -> Result<()> {
    require!(!ctx.accounts.config.paused, RaffleError::Paused);
    require!(ticket_count > 0, RaffleError::ZeroAmount);
    require!(
        ticket_count <= ctx.accounts.config.max_credit_per_call,
        RaffleError::ExceedsMaxCredit
    );

    let now = Clock::get()?.unix_timestamp;
    let config = &mut ctx.accounts.config;
    apply_epoch_cap(config, ticket_count, now)?;

    let total_price = config
        .ticket_price
        .checked_mul(ticket_count)
        .ok_or(RaffleError::Overflow)?;

    let cpi_accounts = Transfer {
        from: ctx.accounts.user_token_account.to_account_info(),
        to: ctx.accounts.treasury_ata.to_account_info(),
        authority: ctx.accounts.user_signer.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(ctx.accounts.token_program.key(), cpi_accounts);
    token::transfer(cpi_ctx, total_price)?;

    let receipt = &mut ctx.accounts.payment_receipt;
    receipt.payment_id = payment_id;
    receipt.user = ctx.accounts.user_account.owner;
    receipt.amount = ticket_count;
    receipt.credited_at = now;
    receipt.bump = ctx.bumps.payment_receipt;

    let user = &mut ctx.accounts.user_account;
    user.ticket_balance = user
        .ticket_balance
        .checked_add(ticket_count)
        .ok_or(RaffleError::Overflow)?;
    user.total_purchased = user
        .total_purchased
        .checked_add(ticket_count)
        .ok_or(RaffleError::Overflow)?;

    emit!(TicketsPurchased {
        user: user.owner,
        ticket_count,
        total_price,
        payment_id,
    });
    Ok(())
}

fn apply_epoch_cap(config: &mut Config, amount: u64, now: i64) -> Result<()> {
    let epoch_end = config
        .current_epoch_start_ts
        .checked_add(EPOCH_LENGTH_SECS)
        .ok_or(RaffleError::Overflow)?;
    if now >= epoch_end {
        config.current_epoch_start_ts = now;
        config.current_epoch_total = 0;
    }
    let new_total = config
        .current_epoch_total
        .checked_add(amount)
        .ok_or(RaffleError::Overflow)?;
    require!(
        new_total <= config.max_credit_per_epoch,
        RaffleError::ExceedsEpochCredit
    );
    config.current_epoch_total = new_total;
    Ok(())
}

#[derive(Accounts)]
#[instruction(payment_id: [u8; 32])]
pub struct CreditTickets<'info> {
    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
        has_one = backend_signer @ RaffleError::NotBackendSigner,
    )]
    pub config: Account<'info, Config>,
    #[account(mut)]
    pub backend_signer: Signer<'info>,
    #[account(
        mut,
        seeds = [b"user", user_account.owner.as_ref()],
        bump = user_account.bump,
    )]
    pub user_account: Account<'info, UserAccount>,
    #[account(
        init,
        payer = backend_signer,
        space = 8 + PaymentReceipt::INIT_SPACE,
        seeds = [b"receipt", payment_id.as_ref()],
        bump,
    )]
    pub payment_receipt: Account<'info, PaymentReceipt>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(payment_id: [u8; 32])]
pub struct BuyTickets<'info> {
    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump,
        has_one = token_mint @ RaffleError::WrongMint,
    )]
    pub config: Box<Account<'info, Config>>,
    #[account(mut)]
    pub user_signer: Signer<'info>,
    #[account(
        mut,
        seeds = [b"user", user_signer.key().as_ref()],
        bump = user_account.bump,
        constraint = user_account.owner == user_signer.key() @ RaffleError::UserOwnerMismatch,
    )]
    pub user_account: Box<Account<'info, UserAccount>>,
    #[account(
        init,
        payer = user_signer,
        space = 8 + PaymentReceipt::INIT_SPACE,
        seeds = [b"receipt", payment_id.as_ref()],
        bump,
    )]
    pub payment_receipt: Box<Account<'info, PaymentReceipt>>,
    pub token_mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        constraint = user_token_account.mint == token_mint.key() @ RaffleError::WrongMint,
        constraint = user_token_account.owner == user_signer.key()
            @ RaffleError::UserOwnerMismatch,
    )]
    pub user_token_account: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = config,
    )]
    pub treasury_ata: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
