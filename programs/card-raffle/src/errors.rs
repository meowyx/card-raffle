use anchor_lang::prelude::*;

// New variants append at the end — inserting mid-enum shifts downstream
// Anchor error codes and breaks off-chain tooling.
#[error_code]
pub enum RaffleError {
    #[msg("Caller is not the admin")]
    NotAdmin,
    #[msg("Caller is not the registered backend signer")]
    NotBackendSigner,
    #[msg("No pending admin to accept")]
    NoPendingAdmin,
    #[msg("Signer is not the pending admin")]
    NotPendingAdmin,
    #[msg("Protocol is paused")]
    Paused,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("max_credit_per_call must be greater than zero")]
    ZeroMaxCredit,
    #[msg("Credit exceeds configured per-call maximum")]
    ExceedsMaxCredit,
    #[msg("User account owner does not match signer")]
    UserOwnerMismatch,
    #[msg("Drop ticket cost must be greater than zero")]
    ZeroCost,
    #[msg("Drop max entries must be greater than zero")]
    ZeroMaxEntries,
    #[msg("Drop deadline must be in the future")]
    DeadlineInPast,
    #[msg("Drop is not in Open status")]
    DropNotOpen,
    #[msg("Drop deadline has already passed")]
    DeadlinePassed,
    #[msg("Drop deadline has not yet been reached")]
    DeadlineNotReached,
    #[msg("Drop is full")]
    DropFull,
    #[msg("Insufficient tickets for entry cost")]
    InsufficientTickets,
    #[msg("Drop is not in Closed status")]
    DropNotClosed,
    #[msg("Drop has no entries to settle")]
    NoEntries,
    #[msg("Entry account does not reference this drop")]
    EntryDropMismatch,
    #[msg("Winning index falls outside this witness entry's range")]
    WinnerWitnessMismatch,
    #[msg("Randomness account does not match the one bound at request time")]
    InvalidRandomnessAccount,
    #[msg("Invalid random value")]
    InvalidRandom,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Arithmetic underflow")]
    Underflow,
    #[msg("Drop is not in RandomnessRequested status")]
    DropNotInRandomnessRequested,
    #[msg("Randomness account is not owned by the Switchboard program")]
    WrongRandomnessOwner,
    #[msg("Randomness commit is stale (seed_slot does not match expected slot)")]
    RandomnessExpired,
    #[msg("Randomness was already revealed before commit was bound")]
    RandomnessAlreadyRevealed,
    #[msg("Randomness has not been revealed yet")]
    RandomnessNotResolved,
    #[msg("Credit would exceed the per-epoch (24h rolling) cap")]
    ExceedsEpochCredit,
    #[msg("max_credit_per_call must be <= max_credit_per_epoch")]
    PerCallExceedsEpochCap,
    #[msg("ticket_price must be greater than zero")]
    ZeroTicketPrice,
    #[msg("Token account mint does not match the configured token_mint")]
    WrongMint,
    #[msg("Winner token account is not owned by the winning entry's user")]
    WinnerAtaOwnerMismatch,
}
