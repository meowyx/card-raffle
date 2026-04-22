use anchor_lang::prelude::*;

use crate::errors::RaffleError;
use crate::events::{DropInitialized, EntriesClosed, EntryCreated};
use crate::state::{Config, DropStatus, Entry, Raffle, UserAccount};

pub fn handle_initialize_drop(
    ctx: Context<InitializeDrop>,
    drop_id: u64,
    ticket_cost: u64,
    max_entries: u64,
    deadline_ts: i64,
    prize_amount: u64,
) -> Result<()> {
    require!(ticket_cost > 0, RaffleError::ZeroCost);
    require!(max_entries > 0, RaffleError::ZeroMaxEntries);
    let now = Clock::get()?.unix_timestamp;
    require!(deadline_ts > now, RaffleError::DeadlineInPast);

    let drop = &mut ctx.accounts.drop;
    drop.drop_id = drop_id;
    drop.ticket_cost = ticket_cost;
    drop.max_entries = max_entries;
    drop.total_entries = 0;
    drop.deadline_ts = deadline_ts;
    drop.status = DropStatus::Open;
    drop.randomness_account = None;
    drop.commit_slot = 0;
    drop.random_value = [0; 32];
    drop.winner = None;
    drop.winning_index = 0;
    drop.prize_amount = prize_amount;
    drop.bump = ctx.bumps.drop;

    emit!(DropInitialized {
        drop: drop.key(),
        drop_id,
        ticket_cost,
        max_entries,
        deadline_ts,
        prize_amount,
    });
    Ok(())
}

pub fn handle_enter_drop(ctx: Context<EnterDrop>, entry_count: u64) -> Result<()> {
    require!(!ctx.accounts.config.paused, RaffleError::Paused);
    require!(entry_count > 0, RaffleError::ZeroAmount);

    let drop = &mut ctx.accounts.drop;
    require!(drop.status == DropStatus::Open, RaffleError::DropNotOpen);
    let now = Clock::get()?.unix_timestamp;
    require!(now < drop.deadline_ts, RaffleError::DeadlinePassed);

    let new_total = drop
        .total_entries
        .checked_add(entry_count)
        .ok_or(RaffleError::Overflow)?;
    require!(new_total <= drop.max_entries, RaffleError::DropFull);

    let cost = drop
        .ticket_cost
        .checked_mul(entry_count)
        .ok_or(RaffleError::Overflow)?;

    let user = &mut ctx.accounts.user_account;
    require!(user.ticket_balance >= cost, RaffleError::InsufficientTickets);
    user.ticket_balance = user
        .ticket_balance
        .checked_sub(cost)
        .ok_or(RaffleError::Underflow)?;
    user.total_spent = user
        .total_spent
        .checked_add(cost)
        .ok_or(RaffleError::Overflow)?;

    let entry = &mut ctx.accounts.entry;
    entry.drop = drop.key();
    entry.user = user.owner;
    entry.entry_count = entry_count;
    entry.start_index = drop.total_entries;
    entry.bump = ctx.bumps.entry;

    drop.total_entries = new_total;

    emit!(EntryCreated {
        drop: drop.key(),
        user: user.owner,
        entry_count,
        start_index: entry.start_index,
    });
    Ok(())
}

pub fn handle_close_entries(ctx: Context<CloseEntries>) -> Result<()> {
    let drop = &mut ctx.accounts.drop;
    require!(drop.status == DropStatus::Open, RaffleError::DropNotOpen);
    let now = Clock::get()?.unix_timestamp;
    require!(now >= drop.deadline_ts, RaffleError::DeadlineNotReached);
    drop.status = DropStatus::Closed;
    emit!(EntriesClosed {
        drop: drop.key(),
        total_entries: drop.total_entries,
    });
    Ok(())
}

#[derive(Accounts)]
#[instruction(drop_id: u64)]
pub struct InitializeDrop<'info> {
    #[account(
        seeds = [b"config"],
        bump = config.bump,
        has_one = admin @ RaffleError::NotAdmin,
    )]
    pub config: Account<'info, Config>,
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init,
        payer = admin,
        space = 8 + Raffle::INIT_SPACE,
        seeds = [b"drop", drop_id.to_le_bytes().as_ref()],
        bump,
    )]
    pub drop: Box<Account<'info, Raffle>>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct EnterDrop<'info> {
    #[account(
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, Config>,
    #[account(mut)]
    pub user_signer: Signer<'info>,
    #[account(
        mut,
        seeds = [b"user", user_signer.key().as_ref()],
        bump = user_account.bump,
        constraint = user_account.owner == user_signer.key() @ RaffleError::UserOwnerMismatch,
    )]
    pub user_account: Account<'info, UserAccount>,
    #[account(
        mut,
        seeds = [b"drop", drop.drop_id.to_le_bytes().as_ref()],
        bump = drop.bump,
    )]
    pub drop: Box<Account<'info, Raffle>>,
    #[account(
        init,
        payer = user_signer,
        space = 8 + Entry::INIT_SPACE,
        seeds = [b"entry", drop.key().as_ref(), user_signer.key().as_ref()],
        bump,
    )]
    pub entry: Box<Account<'info, Entry>>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CloseEntries<'info> {
    #[account(
        mut,
        seeds = [b"drop", drop.drop_id.to_le_bytes().as_ref()],
        bump = drop.bump,
    )]
    pub drop: Box<Account<'info, Raffle>>,
}
