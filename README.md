# Kleo Protocol [![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/Kleo-Protocol/kleo-contracts)

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
- [Deployment](#deployment)
- [Building](#building)
- [Testing](#testing)
- [Key Features](#key-features)

---

## Overview

Kleo Protocol enables undercollateralized lending through a combination of:

1. **Reputation System**: Users earn "stars" that represent their creditworthiness
2. **Social Vouching**: Community members stake their reputation and capital to vouch for borrowers
3. **Fixed Interest Rates**: Interest rates are calculated and fixed at loan creation (stable coin compatible)
4. **Tiered Loan Requirements**: Different loan amounts require different levels of reputation and vouches
5. **Dynamic Pool Rates**: Lending pool interest rates adjust based on utilization

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
| `base_interest_rate` | 10% | Base annual interest rate (scaled by 1e9) |
| `optimal_utilization` | 80% | Target pool utilization for rate model |
| `slope1` | 4% | Interest rate increase below optimal utilization |
| `slope2` | 75% | Interest rate increase above optimal utilization |
| `boost` | 2 | Bonus stars awarded for successful vouches |
| `min_stars_to_vouch` | 50 | Minimum stars required to vouch for others |
| `cooldown_period` | 60 seconds | New account cooldown before earning stars |
| `loan_term` | 30 days | Default loan term |
| `exposure_cap` | 5% | Maximum vouch exposure per borrower relative to pool |
| `reserve_factor` | 20% | Portion of interest allocated to reserves |
| `max_rate` | 100% | Maximum interest rate cap |

**Key Functions**:
- `new()` - Initialize with default values
- `set_admin(admin_account_id)` - Set the admin account
- `update_*()` - Admin-only setters for each parameter
- `get_*()` - Public getters for each parameter

---

### Reputation

**Purpose**: Manages user reputation through a star-based credit scoring system.

**Location**: `reputation/`

**Storage**:
- Per-user reputation tracking including stars, staked stars, loan history, vouch history, and ban status

**Star System**:
- New users start with 7 stars
- Stars accumulate over time after the cooldown period
- Stars can be staked when vouching for others
- Successful vouches return staked stars plus a bonus (configurable boost)
- Failed vouches result in loss of staked stars
- Users with 0 stars are banned from the protocol

**Key Functions**:
- `new(config_address, admin_account_id)` - Initialize, deployer becomes admin
- `get_stars(user)` - Get current star count for a user
- `add_stars(user, amount)` - Add stars to a user (respects cooldown)
- `can_vouch(user)` - Check if user meets minimum stars to vouch
- `stake_stars(user, amount)` - Lock stars for vouching
- `unstake_stars(user, amount, borrower, success)` - Release staked stars with outcome
- `slash_stars(user, amount)` - Penalty reduction of stars
- `admin_set_stars(user, stars)` - Admin function to set stars
- `admin_add_stars(user, amount)` - Admin function to add stars
- `admin_unban_user(user)` - Admin function to unban a user

---

### Vouch

**Purpose**: Manages social guarantee relationships between users.

**Location**: `vouch/`

**Vouch Relationship**:
- Vouchers stake both stars (via Reputation) and capital (via Lending Pool)
- Multiple vouchers can support a single borrower
- Exposure is capped to prevent concentration risk
- Vouches are tracked per loan (not just per borrower)

**Vouch Statuses**:
- `Active` - Vouch is currently backing an active loan
- `Fulfilled` - Loan was repaid successfully
- `Defaulted` - Loan defaulted, voucher penalized

**Key Functions**:
- `new(config_address, reputation_address, lending_pool_address)` - Initialize
- `set_loan_manager(loan_manager_address)` - Set authorized loan manager
- `vouch_for_loan(loan_id, borrower, voucher, stars, capital_percent, loan_manager_address)` - Create a vouch for a specific loan
- `get_vouches_for_loan(loan_id)` - Count active vouches for a loan
- `get_vouchers_for_loan(loan_id)` - List all voucher addresses for a loan
- `resolve_loan(loan_id, borrower, success, loan_manager_address)` - Settle all vouches when loan concludes
- `resolve_all(borrower, success, loan_manager_address)` - Settle all vouches for a borrower (backward compatibility)

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
- `new(config_address)` - Initialize
- `set_vouch_contract(vouch_address)` - Set authorized vouch contract
- `set_loan_manager(loan_manager_address)` - Set authorized loan manager
- `deposit(account_id)` - Add liquidity to the pool (payable, accepts 18 decimals)
- `withdraw(amount, account_id)` - Remove liquidity from the pool (amount in 10 decimals)
- `disburse(amount, to)` - Transfer funds for approved loans (only loan manager, amount in 10 decimals)
- `receive_repayment(amount)` - Process loan repayments (payable, amount in 18 decimals)
- `slash_stake(user, amount)` - Penalize voucher deposits on default (only vouch contract, amount in 10 decimals)
- `get_current_rate()` - Calculate current interest rate
- `get_user_deposit(user)` - Query user deposit balance (returns 10 decimals)
- `get_user_yield(account_id)` - Calculate accrued yield for a user (read-only, returns 18 decimals)
- `accrue_interest_and_get_user_yield(account_id)` - Accrue interest then calculate yield (returns 18 decimals)
- `get_total_liquidity()` - Query total pool liquidity (returns 18 decimals)

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
- Each star reduces the rate by 1% (configurable)
- Maximum discount is 50% (configurable)

**Interest Rate Calculation**:
- Interest rates are **fixed at loan creation** (not time-based)
- Repayment = `amount × (1 + interest_rate_percentage)`
- Example: 100 tokens at 10% = 110 tokens repayment
- The repayment amount is stored in the loan and can be queried

**Loan Statuses**:
- `Pending` - Loan requested, waiting for vouches
- `Active` - Loan disbursed, borrower must repay
- `Repaid` - Loan fully repaid
- `Defaulted` - Loan term expired without repayment

**Key Functions**:
- `new(config_address, reputation_address, lending_pool_address, vouch_address)` - Initialize
- `request_loan(amount, loan_term, account_id)` - Apply for a new loan
- `vouch_for_loan(loan_id, stars, capital_percent, voucher_account_id, loan_manager_address)` - Vouch for a pending loan
- `repay_loan(loan_id, borrower_account_id, loan_manager_address)` - Repay an active loan (payable)
- `check_default(loan_id, loan_manager_address, vouch_contract_address)` - Process overdue loans
- `get_loan(loan_id)` - Get loan information
- `get_repayment_amount(loan_id)` - Get the fixed repayment amount for a loan
- `get_all_pending_loans()` - Get all loan IDs with Pending status
- `get_all_active_loans()` - Get all loan IDs with Active status

**Loan Request Flow**:
1. Validate amount is non-zero
2. Calculate tier-based requirements
3. Verify borrower has sufficient stars (Reputation)
4. Fetch current rate from lending pool
5. Adjust rate based on borrower's stars
6. Calculate fixed repayment amount: `amount × (1 + interest_rate_percentage)`
7. Create loan record with Pending status
8. Emit `LoanRequested` event

**Loan Disbursement Flow**:
1. Vouchers call `vouch_for_loan` to stake stars and capital
2. When minimum vouches threshold is met, loan auto-disburses
3. Loan status changes to Active
4. Funds transferred to borrower via Lending Pool

**Repayment Flow**:
1. Borrower calls `repay_loan` with exact repayment amount
2. Funds transferred to Lending Pool
3. Loan status changes to Repaid
4. All vouches resolved as successful
5. Vouchers receive staked stars back plus bonus
6. Emit `LoanRepaid` event

**Default Processing Flow**:
1. Anyone can call `check_default` for an overdue loan
2. Verify loan exists and is active
3. Check if loan term has expired (with grace period)
4. Mark loan as defaulted
5. Slash borrower stars (Reputation)
6. Resolve all vouches as failed (Vouch)
7. Slash voucher capital (Lending Pool)
8. Emit `LoanDefaulted` event

**Events**:
- `LoanRequested` - New loan created
- `LoanRepaid` - Loan successfully repaid
- `LoanDefaulted` - Loan defaulted after term expiration

---

## How It Works

### For Lenders (Liquidity Providers)

1. **Deposit**: Call `lending_pool.deposit(account_id)` with native tokens
2. **Earn Yield**: Interest accrues automatically based on pool utilization
3. **Vouch (Optional)**: Stake stars and capital to vouch for borrowers
4. **Withdraw**: Call `lending_pool.withdraw(amount, account_id)` to retrieve funds

### For Borrowers

1. **Build Reputation**: Accumulate stars over time (starts with 7 stars)
2. **Get Vouches**: Find community members willing to vouch
3. **Request Loan**: Call `loan_manager.request_loan(amount, loan_term, account_id)`
4. **Wait for Disbursement**: Loan auto-disburses when enough vouches collected
5. **Repay**: Call `loan_manager.repay_loan(loan_id, borrower_account_id, loan_manager_address)` with exact repayment amount before term expires
6. **Default Risk**: If loan expires unpaid, stars are slashed and vouchers penalized

### For Vouchers

1. **Qualify**: Accumulate at least 50 stars (configurable)
2. **Deposit Capital**: Provide liquidity to the lending pool
3. **Vouch**: Call `loan_manager.vouch_for_loan(...)` to stake stars and capital
4. **Outcome**:
   - **Success**: Receive staked stars back plus bonus stars (default +2)
   - **Default**: Lose staked stars and capital portion

---

## Deployment

### Deployment Order

Contracts must be deployed in the following order due to dependencies:

1. **Config** - No dependencies
2. **Reputation** - Requires Config address
3. **Lending Pool** - Requires Config address
4. **Vouch** - Requires Config, Reputation, and Lending Pool addresses
5. **Loan Manager** - Requires Config, Reputation, Lending Pool, and Vouch addresses

### Post-Deployment Setup

After deployment, you must set up contract references:

1. **Config.set_admin(admin_account_id)** - Set the admin account
2. **Reputation.set_vouch_contract(vouch_address)** - Set vouch contract reference
3. **Reputation.set_loan_manager(loan_manager_address)** - Set loan manager reference
4. **LendingPool.set_vouch_contract(vouch_address)** - Set vouch contract reference
5. **LendingPool.set_loan_manager(loan_manager_address)** - Set loan manager reference
6. **Vouch.set_loan_manager(loan_manager_address)** - Set loan manager reference

**Important**: Pay attention to whether functions require `Address` (H160) or `AccountId` (32-byte) types.

---

## Building

Each contract can be built using `cargo-contract`:

```bash
# Build individual contract
cd config && cargo contract build --release
cd reputation && cargo contract build --release
cd lending_pool && cargo contract build --release
cd vouch && cargo contract build --release
cd loan_manager && cargo contract build --release
```

Or build all at once:
```bash
./demo.sh
```

Build artifacts are output to `target/ink/` in each contract directory:
- `*.contract` - Bundled contract (metadata + wasm)
- `*.json` - Contract metadata/ABI
- `*.polkavm` - PolkaVM bytecode

---

## Testing

See [CLI_TESTING_GUIDE.md](./CLI_TESTING_GUIDE.md) for detailed testing instructions.

Quick test flow:
1. Deploy all contracts in order
2. Set up contract references
3. Bootstrap test accounts with stars
4. Add liquidity to pool
5. Request a loan
6. Vouch for the loan
7. Repay the loan
8. Verify outcomes

---

## Key Features

### Fixed Interest Rates
- Interest rates are calculated and fixed at loan creation
- Compatible with stable coins (no time-based accrual)
- Repayment amount = `principal × (1 + interest_rate_percentage)`
- Query repayment amount with `get_repayment_amount(loan_id)`

### AccountId Parameters
- Many functions now require `account_id` as a parameter (temporary workaround for `to_account_id` issues)
- Functions that check authorization use `caller()` internally
- Pay attention to function signatures when calling

### Decimal Precision
- **Storage format (10 decimals)**: `user_deposits` are stored in 10 decimals
- **Chain format (18 decimals)**: All transfers and `total_liquidity` use 18 decimals
- **Function parameters**:
  - `deposit()`: Accepts value in 18 decimals (from `transferred_value()`)
  - `withdraw(amount, account_id)`: `amount` parameter in 10 decimals
  - `disburse(amount, to)`: `amount` parameter in 10 decimals
  - `receive_repayment(amount)`: `amount` parameter in 18 decimals (matches `transferred_value()`)
  - `get_repayment_amount(loan_id)`: Returns 18 decimals (chain format)
  - `get_user_yield(account_id)`: Returns 18 decimals (chain format)
  - `get_total_liquidity()`: Returns 18 decimals (chain format)

### Query Functions
- `get_all_pending_loans()` - Get all pending loan IDs
- `get_all_active_loans()` - Get all active loan IDs
- `get_repayment_amount(loan_id)` - Get fixed repayment amount (returns 18 decimals)
- `get_user_yield(account_id)` - Get user yield without accruing interest (read-only, returns 18 decimals)
- `accrue_interest_and_get_user_yield(account_id)` - Get user yield with interest accrual (returns 18 decimals)

---

## Error Handling

Each contract defines specific error types:

**Config**: `NotAdmin`, `InvalidValue`, `AlreadyAdmin`

**Reputation**: `UserNotFound`, `InsufficientStars`, `InsufficientStakedStars`, `UserBanned`, `Unauthorized`

**Vouch**: `NotEnoughStars`, `NotEnoughCapital`, `UnableToVouch`, `ZeroAmount`, `ExposureCapExceeded`, `AlreadyResolved`, `RelationshipNotFound`, `Unauthorized`

**Lending Pool**: `ZeroAmount`, `NegativeAmount`, `Overflow`, `UnavailableFunds`, `TransactionFailed`, `AmountMismatch`, `Unauthorized`

**Loan Manager**: `InsufficientReputation`, `InsufficientVouches`, `ZeroAmount`, `DisbursementFailed`, `LoanNotFound`, `LoanNotActive`, `LoanNotPending`, `LoanNotOverdue`, `SlashFailed`, `ResolveFailed`, `Unauthorized`, `RepaymentFailed`, `InvalidRepaymentAmount`, `Overflow`

---

## License

MIT License
