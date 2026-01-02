# Kleo Protocol

A decentralized lending protocol built with Ink! v6 smart contracts for the Polkadot ecosystem. Kleo implements a reputation-based, community-vouched lending system where borrowers build creditworthiness through on-chain reputation and social guarantees.

## Table of Contents

- [Overview](#overview)
- [Architecture](#architecture)
- [Contracts](#contracts)
  - [Config](#config)
  - [Reputation](#reputation)
  - [Vouch](#vouch)
  - [Lending Pool](#lending-pool)
  - [Loan Manager](#loan-manager)
- [How It Works](#how-it-works)
- [Deployment Order](#deployment-order)
- [Building](#building)

---

## Overview

Kleo Protocol enables undercollateralized lending through a combination of:

1. **Reputation System**: Users earn "stars" that represent their creditworthiness
2. **Social Vouching**: Community members stake their reputation and capital to vouch for borrowers
3. **Dynamic Interest Rates**: Rates adjust based on pool utilization and borrower reputation
4. **Tiered Loan Requirements**: Different loan amounts require different levels of reputation and vouches

---

## Architecture

```
                    +------------------+
                    |     Config       |
                    | (Protocol Params)|
                    +--------+---------+
                             |
        +--------------------+--------------------+
        |                    |                    |
        v                    v                    v
+-------+-------+    +-------+-------+    +-------+-------+
|  Reputation   |    | Lending Pool  |    |     Vouch     |
|   (Stars)     |    |   (Liquidity) |    | (Guarantees)  |
+-------+-------+    +-------+-------+    +-------+-------+
        |                    |                    |
        +--------------------+--------------------+
                             |
                             v
                    +--------+---------+
                    |   Loan Manager   |
                    | (Orchestrator)   |
                    +------------------+
```

All contracts reference the **Config** contract for protocol parameters. The **Loan Manager** orchestrates interactions between Reputation, Vouch, and Lending Pool contracts to facilitate the complete loan lifecycle.

---

## Contracts

### Config

**Purpose**: Central configuration storage for all protocol parameters.

**Location**: `config/`

**Key Parameters**:

| Parameter | Default Value | Description |
|-----------|---------------|-------------|
| `base_interest_rate` | 10% | Base annual interest rate |
| `optimal_utilization` | 80% | Target pool utilization for rate model |
| `slope1` | 4% | Interest rate increase below optimal utilization |
| `slope2` | 75% | Interest rate increase above optimal utilization |
| `boost` | 2 | Bonus stars awarded for successful vouches |
| `min_stars_to_vouch` | 50 | Minimum stars required to vouch for others |
| `cooldown_period` | 30 days | New account cooldown before earning stars |
| `exposure_cap` | 5% | Maximum vouch exposure per borrower relative to pool |
| `reserve_factor` | 20% | Portion of interest allocated to reserves |
| `max_rate` | 100% | Maximum interest rate cap |

**Key Functions**:
- `new()` - Initialize with default values, caller becomes admin
- `update_*()` - Admin-only setters for each parameter
- `get_*()` - Public getters for each parameter

---

### Reputation

**Purpose**: Manages user reputation through a star-based credit scoring system.

**Location**: `reputation/`

**Storage**:
- Per-user reputation tracking including stars, staked stars, loan history, vouch history, and ban status

**Star System**:
- New users start with 1 star
- Stars accumulate over time after the cooldown period
- Stars can be staked when vouching for others
- Successful vouches return staked stars plus a bonus
- Failed vouches result in loss of staked stars
- Users with 0 stars are banned from the protocol

**Key Functions**:
- `get_stars(user)` - Get current star count for a user
- `add_stars(user, amount)` - Add stars to a user (respects cooldown)
- `can_vouch(user)` - Check if user meets minimum stars to vouch
- `stake_stars(user, amount)` - Lock stars for vouching
- `unstake_stars(user, amount, success)` - Release staked stars with outcome
- `slash_stars(user, amount)` - Penalty reduction of stars

---

### Vouch

**Purpose**: Manages social guarantee relationships between users.

**Location**: `vouch/`

**Vouch Relationship**:
- Vouchers stake both stars (via Reputation) and capital (via Lending Pool)
- Multiple vouchers can support a single borrower
- Exposure is capped to prevent concentration risk

**Vouch Statuses**:
- `Active` - Vouch is currently backing an active loan
- `Fulfilled` - Loan was repaid successfully
- `Defaulted` - Loan defaulted, voucher penalized

**Key Functions**:
- `vouch_for(borrower, stars, capital_percent)` - Create a vouch relationship
- `get_vouches_for(borrower)` - Count active vouches for a borrower
- `get_all_vouchers(borrower)` - List all voucher addresses
- `resolve_all(borrower, success)` - Settle all vouches when loan concludes

**Events**:
- `VouchCreated` - New vouch relationship established
- `VouchResolved` - Vouch settled with success/failure outcome

---

### Lending Pool

**Purpose**: Manages liquidity pool with automatic interest rate calculations and accruals.

**Location**: `lending_pool/`

**Interest Rate Model**:
The pool uses a two-slope interest rate model:

```
if utilization <= optimal:
    rate = base + (utilization / optimal) * slope1
else:
    rate = base + slope1 + ((utilization - optimal) / (1 - optimal)) * slope2
```

This encourages deposits when utilization is high and borrowing when utilization is low.

**Key Functions**:
- `deposit()` - Add liquidity to the pool (payable)
- `withdraw(amount)` - Remove liquidity from the pool
- `disburse(amount, to)` - Transfer funds for approved loans
- `receive_repayment(amount)` - Process loan repayments (payable)
- `slash_stake(user, amount)` - Penalize voucher deposits on default
- `get_current_rate()` - Calculate current interest rate
- `get_user_deposit(user)` - Query user deposit balance
- `get_user_yield()` - Calculate accrued yield for caller
- `get_total_liquidity()` - Query total pool liquidity

**Events**:
- `Deposit` - Funds added to pool
- `Withdraw` - Funds removed from pool
- `RepaymentReceived` - Loan repayment processed

---

### Loan Manager

**Purpose**: Central orchestrator that coordinates all contracts to manage the complete loan lifecycle.

**Location**: `loan_manager/`

**Loan Tiers**:

| Tier | Loan Size | Min Stars | Min Vouches |
|------|-----------|-----------|-------------|
| Tier 1 | < 1,000 units | 5 | 1 |
| Tier 2 | 1,000 - 10,000 units | 20 | 2 |
| Tier 3 | > 10,000 units | 50 | 3 |

**Interest Rate Adjustment**:
Borrowers with higher reputation receive discounted rates:
- Each star reduces the rate by 1%
- Maximum discount is 50%

**Loan Statuses**:
- `Active` - Loan is outstanding
- `Repaid` - Loan was fully repaid
- `Defaulted` - Loan term expired without repayment

**Key Functions**:
- `request_loan(amount, purpose)` - Apply for a new loan
- `check_default(loan_id)` - Process overdue loans

**Loan Request Flow**:
1. Validate amount is non-zero
2. Calculate tier-based requirements
3. Verify borrower has sufficient stars (Reputation)
4. Verify borrower has sufficient vouches (Vouch)
5. Calculate interest rate with star-based discount
6. Create loan record
7. Disburse funds (Lending Pool)
8. Emit `LoanRequested` event

**Default Processing Flow**:
1. Verify loan exists and is active
2. Check if loan term has expired
3. Mark loan as defaulted
4. Slash borrower stars (Reputation)
5. Resolve all vouches as failed (Vouch)
6. Emit `LoanDefaulted` event

**Events**:
- `LoanRequested` - New loan created and funded
- `LoanRepaid` - Loan successfully repaid
- `LoanDefaulted` - Loan defaulted after term expiration

---

## How It Works

### For Lenders (Liquidity Providers)

1. **Deposit**: Call `lending_pool.deposit()` with native tokens
2. **Earn Yield**: Interest accrues automatically based on pool utilization
3. **Vouch (Optional)**: Stake stars and capital to vouch for borrowers
4. **Withdraw**: Call `lending_pool.withdraw(amount)` to retrieve funds

### For Borrowers

1. **Build Reputation**: Accumulate stars over time
2. **Get Vouches**: Find community members willing to vouch
3. **Request Loan**: Call `loan_manager.request_loan(amount, purpose)`
4. **Repay**: Call `lending_pool.receive_repayment(amount)` before term expires
5. **Default Risk**: If loan expires unpaid, stars are slashed and vouchers penalized

### For Vouchers

1. **Qualify**: Accumulate at least 50 stars (configurable)
2. **Deposit Capital**: Provide liquidity to the lending pool
3. **Vouch**: Call `vouch.vouch_for(borrower, stars, capital_percent)`
4. **Outcome**:
   - **Success**: Receive staked stars back plus bonus stars
   - **Default**: Lose staked stars and capital portion

---

## Deployment Order

Contracts must be deployed in the following order due to dependencies:

1. **Config** - No dependencies
2. **Lending Pool** - Requires Config address
3. **Reputation** - Requires Config address
4. **Vouch** - Requires Config, Reputation, and Lending Pool addresses
5. **Loan Manager** - Requires Config, Reputation, Lending Pool, and Vouch addresses

---

## Building

Each contract can be built using `cargo-contract`:

```bash
# Build individual contract
cd config && cargo contract build --release

cd lending_pool && cargo contract build --release

cd reputation && cargo contract build --release

cd vouch && cargo contract build --release

cd loan_manager && cargo contract build --release
```

Build artifacts are output to `target/ink/` in each contract directory:
- `*.contract` - Bundled contract (metadata + wasm)
- `*.json` - Contract metadata/ABI
- `*.polkavm` - PolkaVM bytecode

---

## Error Handling

Each contract defines specific error types:

**Config**: `NotAdmin`

**Reputation**: `UserNotFound`, `InsufficientStars`, `InsufficientStakedStars`, `UserBanned`

**Vouch**: `NotEnoughStars`, `NotEnoughCapital`, `UnableToVouch`, `ZeroAmount`, `ExposureCapExceeded`, `AlreadyResolved`, `RelationshipNotFound`

**Lending Pool**: `ZeroAmount`, `NegativeAmount`, `Overflow`, `UnavailableFunds`, `TransactionFailed`, `AmountMismatch`

**Loan Manager**: `InsufficientReputation`, `InsufficientVouches`, `ZeroAmount`, `DisbursementFailed`, `LoanNotFound`, `LoanNotActive`, `LoanNotOverdue`, `SlashFailed`, `ResolveFailed`
