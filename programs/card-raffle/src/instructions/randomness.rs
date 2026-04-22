use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use switchboard_on_demand::accounts::RandomnessAccountData;
use switchboard_on_demand::SWITCHBOARD_PROGRAM_ID;

use crate::errors::RaffleError;
use crate::events::{DropSettled, RandomnessRequested};
use crate::state::{Config, DropStatus, Entry, Raffle};

/// Commit phase. Binds a Switchboard Randomness account to the drop after
/// validating it's fresh (committed in the previous slot) and not yet
/// revealed. Permissionless — anyone can call; the freshness + pre-reveal
/// checks prevent manipulation.
pub fn handle_request_randomness(ctx: Context<RequestRandomness>) -> Result<()> {
    let drop = &mut ctx.accounts.drop;
    require!(drop.status == DropStatus::Closed, RaffleError::DropNotClosed);
    require!(drop.total_entries > 0, RaffleError::NoEntries);

    let randomness_account_info = &ctx.accounts.randomness_account_data;

    require_keys_eq!(
        *randomness_account_info.owner,
        SWITCHBOARD_PROGRAM_ID,
        RaffleError::WrongRandomnessOwner
    );

    let randomness_data = RandomnessAccountData::parse(randomness_account_info.data.borrow())
        .map_err(|_| RaffleError::InvalidRandomnessAccount)?;

    let clock = Clock::get()?;

    // Stale seeds would let an attacker watch the chain for revealed values
    // and bind a "request" to an already-known seed.
    require!(
        randomness_data.seed_slot == clock.slot.saturating_sub(1),
        RaffleError::RandomnessExpired
    );

    // If the oracle has already revealed, a selective-revelation attack
    // becomes possible — bail before binding.
    require!(
        randomness_data.get_value(clock.slot).is_err(),
        RaffleError::RandomnessAlreadyRevealed
    );

    drop.randomness_account = Some(randomness_account_info.key());
    drop.commit_slot = randomness_data.seed_slot;
    drop.status = DropStatus::RandomnessRequested;

    emit!(RandomnessRequested {
        drop: drop.key(),
        randomness_account: randomness_account_info.key(),
        commit_slot: randomness_data.seed_slot,
    });
    Ok(())
}

/// Reveal phase. Reads the revealed randomness, picks the winner via an
/// O(1) witness check (caller supplies the entry whose range contains the
/// winning index, program verifies), and transfers `prize_amount` tokens
/// from the treasury to the winner's ATA if prize > 0.
pub fn handle_settle_drop(ctx: Context<SettleDrop>) -> Result<()> {
    let drop = &mut ctx.accounts.drop;
    require!(
        drop.status == DropStatus::RandomnessRequested,
        RaffleError::DropNotInRandomnessRequested
    );

    let randomness_account_info = &ctx.accounts.randomness_account_data;

    let bound = drop
        .randomness_account
        .ok_or(RaffleError::InvalidRandomnessAccount)?;
    require_keys_eq!(
        randomness_account_info.key(),
        bound,
        RaffleError::InvalidRandomnessAccount
    );

    require_keys_eq!(
        *randomness_account_info.owner,
        SWITCHBOARD_PROGRAM_ID,
        RaffleError::WrongRandomnessOwner
    );

    let randomness_data = RandomnessAccountData::parse(randomness_account_info.data.borrow())
        .map_err(|_| RaffleError::InvalidRandomnessAccount)?;

    require!(
        randomness_data.seed_slot == drop.commit_slot,
        RaffleError::RandomnessExpired
    );

    let clock = Clock::get()?;
    let revealed_value = randomness_data
        .get_value(clock.slot)
        .map_err(|_| RaffleError::RandomnessNotResolved)?;

    let random_u64 = u64::from_le_bytes(
        revealed_value[0..8]
            .try_into()
            .map_err(|_| RaffleError::InvalidRandom)?,
    );
    let winning_index = random_u64 % drop.total_entries;

    let witness = &ctx.accounts.winner_entry;
    require_keys_eq!(witness.drop, drop.key(), RaffleError::EntryDropMismatch);
    let end = witness
        .start_index
        .checked_add(witness.entry_count)
        .ok_or(RaffleError::Overflow)?;
    require!(
        witness.start_index <= winning_index && winning_index < end,
        RaffleError::WinnerWitnessMismatch
    );

    drop.random_value = revealed_value;
    drop.winner = Some(witness.user);
    drop.winning_index = winning_index;
    drop.status = DropStatus::Settled;

    let prize = drop.prize_amount;
    let drop_winner = witness.user;
    if prize > 0 {
        // Config PDA signs; insufficient treasury reverts the entire settle.
        let config_bump = ctx.accounts.config.bump;
        let signer_seeds: &[&[u8]] = &[b"config", std::slice::from_ref(&config_bump)];
        let signers = &[signer_seeds];
        let cpi_accounts = Transfer {
            from: ctx.accounts.treasury_ata.to_account_info(),
            to: ctx.accounts.winner_token_account.to_account_info(),
            authority: ctx.accounts.config.to_account_info(),
        };
        let cpi_ctx =
            CpiContext::new_with_signer(ctx.accounts.token_program.key(), cpi_accounts, signers);
        token::transfer(cpi_ctx, prize)?;
    }

    emit!(DropSettled {
        drop: drop.key(),
        winner: drop_winner,
        winning_index,
        random_value: revealed_value,
        prize_paid: prize,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct RequestRandomness<'info> {
    #[account(
        mut,
        seeds = [b"drop", drop.drop_id.to_le_bytes().as_ref()],
        bump = drop.bump,
    )]
    pub drop: Box<Account<'info, Raffle>>,
    /// CHECK: Switchboard Randomness account; validated inline (owner +
    /// seed_slot freshness + pre-reveal status).
    pub randomness_account_data: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct SettleDrop<'info> {
    #[account(
        mut,
        seeds = [b"drop", drop.drop_id.to_le_bytes().as_ref()],
        bump = drop.bump,
    )]
    pub drop: Box<Account<'info, Raffle>>,
    /// CHECK: Switchboard Randomness account; validated inline against the
    /// pubkey bound at request time.
    pub randomness_account_data: UncheckedAccount<'info>,
    #[account(
        seeds = [b"entry", drop.key().as_ref(), winner_entry.user.as_ref()],
        bump = winner_entry.bump,
    )]
    pub winner_entry: Box<Account<'info, Entry>>,
    #[account(
        seeds = [b"config"],
        bump = config.bump,
        has_one = token_mint @ RaffleError::WrongMint,
    )]
    pub config: Box<Account<'info, Config>>,
    pub token_mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = config,
    )]
    pub treasury_ata: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = token_mint,
        associated_token::authority = winner_entry.user,
    )]
    pub winner_token_account: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}
