# card-raffle

A Solana raffle / drop primitive with verifiable Switchboard randomness, SPL token ticket purchases, and on-chain prize payout.

![Rust](https://img.shields.io/badge/Rust-1.89-B7410E?logo=rust&logoColor=white)
![Solana](https://img.shields.io/badge/Solana-3.x-9945FF?logo=solana&logoColor=white)
![Anchor](https://img.shields.io/badge/Anchor-1.0.1-512BD4)
![Switchboard](https://img.shields.io/badge/Switchboard-Randomness-00D4AA)
![Tests](https://img.shields.io/badge/tests-20%20passing-brightgreen)
![License](https://img.shields.io/badge/license-MIT-blue)

Users buy tickets in a legacy SPL token bound at config time (USDC in the demo), enter drops, and a Switchboard commit-reveal picks a winner. Settle is permissionless and optionally pays an on-chain prize from the protocol treasury.

## Features

- **Verifiable randomness.** Switchboard commit-reveal with freshness and pre-reveal checks. Settle is permissionless; outcome is non-manipulable by any single party.
- **Two ticket paths.** `credit_tickets` is backend-signed for off-chain-settled payments; `buy_tickets` is user-signed with a real SPL token CPI from the buyer's ATA to the treasury.
- **Protocol-owned treasury.** Treasury ATA is authority-owned by the Config PDA. Prize payout at settle uses `invoke_signed` with the config seeds.
- **Token-2022 rejected at the boundary.** Enforced at the type level via Anchor's `Account<'info, Mint>` owner check — no CPI path to a Token-2022 mint.
- **Per-epoch rate cap.** 24h rolling window on ticket mints, applied to both ticket paths. Bounds blast radius from a compromised backend signer.
- **Replay-guarded credits.** `PaymentReceipt` PDAs use `init`, not `init_if_needed`. A duplicate `payment_id` fails at account creation before any program logic runs.
- **Two-step admin rotation.** `propose_admin` / `accept_admin` prevents fat-finger lockout.
- **Absorbing terminal state.** `Settled` is a state-machine guard, not a boolean flag that could be flipped.
- **O(1) winner selection.** Witness-based. Caller supplies the entry whose cumulative range contains the winning index; program verifies the range.

## Architecture

```
Open ──enter──► Closed ──request_randomness──► RandomnessRequested ──settle_drop──► Settled
```

| Instruction | Signer | Purpose |
|---|---|---|
| `initialize_config` | admin | One-time setup; binds token_mint, creates treasury ATA |
| `propose_admin` / `accept_admin` | admin, then new admin | Two-step rotation |
| `set_pause` | admin | Emergency pause |
| `initialize_user` | user | Per-user account PDA |
| `credit_tickets` | backend signer | Off-chain payment flow |
| `buy_tickets` | user | On-chain SPL token purchase |
| `initialize_drop` | admin | Open a drop (deadline, ticket cost, prize amount) |
| `enter_drop` | user | Spend tickets for entries |
| `close_entries` | anyone | Lock entries after the deadline |
| `request_randomness` | anyone | Commit phase; binds a Switchboard Randomness account |
| `settle_drop` | anyone | Reveal phase; pick winner and pay the prize |

## Quick start

Prerequisites: Rust 1.89, the Anchor CLI (1.0+), Solana CLI.

```bash
anchor build
cargo test -p card-raffle
```

All 20 tests run in ~0.3s. LiteSVM loads the built `.so` in-process and no validator required.

## Project structure

```
programs/card-raffle/
├── src/
│   ├── lib.rs                 program module, declare_id
│   ├── state.rs               account structs
│   ├── errors.rs              RaffleError
│   ├── events.rs              emitted events
│   └── instructions/
│       ├── config.rs          admin + treasury setup
│       ├── user.rs            user account init
│       ├── tickets.rs         credit_tickets, buy_tickets, epoch cap
│       ├── drops.rs           init_drop, enter_drop, close_entries
│       └── randomness.rs      request_randomness, settle_drop, prize payout
└── tests/
    ├── happy_path.rs            lifecycle via credit_tickets (prize 0)
    ├── full_drop_with_prize.rs  end-to-end: USDC purchase → drop → VRF → prize
    ├── buy_tickets.rs           purchase flow, wrong-mint, Token-2022 reject
    ├── security.rs              12 negative tests, each pinned to an error code
    ├── initialize_config.rs     config state assertions
    ├── smoke.rs                 loads program into LiteSVM
    └── common/mod.rs            helpers: PDA derivation, mint setup, randomness mocks
```

## Security claims → tests

Each claim in the program is backed by a specific test.

| Property | Test |
|---|---|
| Replay blocked at account creation | `security::replay_credit_tickets_fails` |
| Backend-key blast radius bounded | `security::credit_from_wrong_signer_fails` |
| Stale VRF seeds rejected | `security::request_randomness_with_stale_seed_fails` |
| Pre-revealed VRF rejected | `security::request_randomness_with_already_revealed_fails` |
| Non-Switchboard owner rejected | `security::request_randomness_wrong_owner_fails` |
| Post-commit randomness swap rejected | `security::settle_with_wrong_randomness_account_fails` |
| Unrevealed randomness rejected on settle | `security::settle_with_unrevealed_randomness_fails` |
| Double-settle blocked by state machine | `security::double_settle_fails` |
| 24h rolling epoch cap enforced | `security::credit_exceeds_epoch_cap_fails` |
| Epoch counter rolls over | `security::credit_rolls_over_after_epoch` |
| Deadline-gated close | `security::close_before_deadline_fails` |
| Wrong mint rejected on purchase | `buy_tickets::buy_tickets_wrong_mint_fails` |
| Token-2022 mint rejected at init | `buy_tickets::initialize_config_rejects_token_2022_mint` |
| End-to-end USDC → drop → prize payout | `full_drop_with_prize::full_drop_with_prize_payout` |

## Stack

- **Anchor 1.0.1** -program framework
- **Switchboard Randomness** via the `switchboard-on-demand` crate (0.12) - commit-reveal
- **anchor-spl 1.0.1** - SPL Token + Associated Token Program integration
- **LiteSVM 0.11** - in-process integration tests. Loads the compiled `.so` directly and supports `set_sysvar::<Clock>` for deadline-gated tests

<details>
<summary><b>What's next</b></summary>

- **Devnet deployment.** `anchor deploy --provider.cluster devnet` plus a TypeScript demo script that drives the full lifecycle against a real Switchboard queue and prints tx signatures.
- **Frontend preview.** Next.js + `@coral-xyz/anchor` + `@solana/wallet-adapter`. Connect wallet → buy tickets → enter drop → settle, using `@switchboard-xyz/on-demand` to generate commit/reveal instructions per the [Switchboard randomness tutorial](https://docs.switchboard.xyz/docs-by-chain/solana-svm/randomness/randomness-tutorial).
- **Privy embedded wallet integration.** Swap `wallet-adapter` for `@privy-io/react-auth` to support email / OTP onboarding. No contract changes required — Privy wallets are keypair-based from the program's point of view.
- **Admin multisig.** Wrap the admin key in a Squads multisig for production deployment; composes cleanly with the existing two-step rotation.
- **Admin rotation test.** The two-step rotation is implemented but not yet exercised in the Rust suite; add a dedicated test that drives `propose_admin` → `accept_admin` and asserts `Config.admin`.
- **Event emission test.** Parse `TransactionMetadata.logs` on the happy path and verify every emitted event fires with expected fields.
- **Witness-check property test.** Fuzz the winner-witness bounds (`start_index = 0, entry_count = u64::MAX`, etc.) to validate the `checked_add` overflow guard.

</details>

## License

MIT. See [LICENSE](./LICENSE).
