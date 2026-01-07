# CLI Testing Guide for Kleo Contracts

## Prerequisites

1. **Install cargo-contract**:
   ```bash
   cargo install cargo-contract --force
   ```

2. **Install substrate-contracts-node** (for local testing):
   ```bash
   cargo install substrate-contracts-node --git https://github.com/paritytech/substrate-contracts-node.git --tag v1.0.0 --force
   ```

3. **Install Polkadot.js CLI** (optional, for easier interaction):
   ```bash
   npm install -g @polkadot/api-cli
   ```

## Quick Start

### Step 1: Build All Contracts

```bash
cd /Users/fabiansanchezd/Documents/kleo-contracts
./demo.sh
```

Or manually:
```bash
cd config && cargo contract build --release && cd ..
cd reputation && cargo contract build --release && cd ..
cd lending_pool && cargo contract build --release && cd ..
cd vouch && cargo contract build --release && cd ..
cd loan_manager && cargo contract build --release && cd ..
```

### Step 2: Start Local Node

In a separate terminal:
```bash
substrate-contracts-node --dev
```

This starts a local Substrate node on `ws://127.0.0.1:9944`

### Step 3: Deploy Contracts

Open Polkadot.js Apps: https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:9944#/contracts

**Deployment Order** (critical - must follow this order):

1. **Deploy Config**
   - Upload: `config/target/ink/config.contract`
   - Constructor: `new()`
   - Note the contract address: `CONFIG_ADDRESS`
   - **Important**: Call `set_admin(ADMIN_ACCOUNT_ID)` to set the admin

2. **Deploy Reputation**
   - Upload: `reputation/target/ink/reputation.contract`
   - Constructor: `new(CONFIG_ADDRESS, ADMIN_ACCOUNT_ID)`
   - Note: `REPUTATION_ADDRESS`
   - The `ADMIN_ACCOUNT_ID` parameter is the AccountId of the deployer

3. **Deploy Lending Pool**
   - Upload: `lending_pool/target/ink/lending_pool.contract`
   - Constructor: `new(CONFIG_ADDRESS)`
   - Note: `LENDING_POOL_ADDRESS`

4. **Deploy Vouch**
   - Upload: `vouch/target/ink/vouch.contract`
   - Constructor: `new(CONFIG_ADDRESS, REPUTATION_ADDRESS, LENDING_POOL_ADDRESS)`
   - Note: `VOUCH_ADDRESS`

5. **Deploy Loan Manager**
   - Upload: `loan_manager/target/ink/loan_manager.contract`
   - Constructor: `new(CONFIG_ADDRESS, REPUTATION_ADDRESS, LENDING_POOL_ADDRESS, VOUCH_ADDRESS)`
   - Note: `LOAN_MANAGER_ADDRESS`

### Step 4: Set Up Contract References

After deployment, you MUST set up the contract references. **Important**: All addresses should be passed as `Address` type (H160), not AccountId.

1. **Reputation.set_vouch_contract**
   - Contract: `REPUTATION_ADDRESS`
   - Message: `set_vouch_contract`
   - Args: `VOUCH_ADDRESS` (as Address)

2. **Reputation.set_loan_manager**
   - Contract: `REPUTATION_ADDRESS`
   - Message: `set_loan_manager`
   - Args: `LOAN_MANAGER_ADDRESS` (as Address)

3. **LendingPool.set_vouch_contract**
   - Contract: `LENDING_POOL_ADDRESS`
   - Message: `set_vouch_contract`
   - Args: `VOUCH_ADDRESS` (as Address)

4. **LendingPool.set_loan_manager**
   - Contract: `LENDING_POOL_ADDRESS`
   - Message: `set_loan_manager`
   - Args: `LOAN_MANAGER_ADDRESS` (as Address)

5. **Vouch.set_loan_manager**
   - Contract: `VOUCH_ADDRESS`
   - Message: `set_loan_manager`
   - Args: `LOAN_MANAGER_ADDRESS` (as Address)

## Testing Flow

### Setup Test Accounts

The local node comes with pre-funded accounts:
- **Alice** (default account) - Use as admin/voucher
- **Bob** (create new account) - Use as voucher
- **Charlie** (create new account) - Use as borrower

**Important**: Note down the AccountId for each account. You'll need to convert between Address (H160) and AccountId when calling functions.

### Step 1: Bootstrap Stars (Admin Functions)

As the admin (contract deployer), set stars for testing:

1. **Give Alice 100 stars** (can vouch):
   - Contract: `REPUTATION_ADDRESS`
   - Message: `admin_set_stars`
   - Args: `Alice's AccountId`, `100`

2. **Give Bob 100 stars** (can vouch):
   - Contract: `REPUTATION_ADDRESS`
   - Message: `admin_set_stars`
   - Args: `Bob's AccountId`, `100`

3. **Give Charlie 7 stars** (can request Tier 1 loans):
   - Contract: `REPUTATION_ADDRESS`
   - Message: `admin_set_stars`
   - Args: `Charlie's AccountId`, `7`

### Step 2: Add Liquidity to Pool

1. **Alice deposits 100 tokens**:
   - Contract: `LENDING_POOL_ADDRESS`
   - Message: `deposit`
   - Args: `Alice's AccountId`
   - Value: `100000000000000000000` (100 tokens with 18 decimals - chain format)
   - Note: The contract converts to 10 decimals for storage

2. **Bob deposits 100 tokens**:
   - Contract: `LENDING_POOL_ADDRESS`
   - Message: `deposit`
   - Args: `Bob's AccountId`
   - Value: `100000000000000000000` (100 tokens with 18 decimals - chain format)

3. **Verify deposits**:
   - Contract: `LENDING_POOL_ADDRESS`
   - Message: `get_user_deposit`
   - Args: `Alice's AccountId` → Should return `10000000000` (100 tokens with 10 decimals - storage format)
   - Message: `get_total_liquidity` → Should return `200000000000000000000` (200 tokens with 18 decimals - chain format)

### Step 3: Request a Loan

**Charlie requests a Tier 1 loan** (needs 5 stars, 1 vouch):

1. **Request loan**:
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `request_loan`
   - Args: 
     - `amount`: `50000000000` (500 tokens with 10 decimals - storage format)
     - `loan_term`: `2592000000` (30 days in milliseconds)
     - `account_id`: `Charlie's AccountId`
   - Note the returned `loan_id` (should be `1`)

2. **Check loan status**:
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `get_loan`
   - Args: `1`
   - Should show: `status: Pending`, `borrower: Charlie's AccountId`

3. **Get repayment amount**:
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `get_repayment_amount`
   - Args: `1`
   - Returns the fixed repayment amount in 18 decimals (principal + interest calculated at creation)
   - Example: If rate is 10%, 500 tokens → 550 tokens repayment
   - The returned value will be in 18 decimals (e.g., `550000000000000000000` for 550 tokens)

### Step 4: Vouch for the Loan

**Alice vouches for Charlie's loan**:

1. **Vouch for loan**:
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `vouch_for_loan`
   - Args:
     - `loan_id`: `1`
     - `stars`: `10`
     - `capital_percent`: `10` (10% of Alice's deposit = 100 tokens)
     - `voucher_account_id`: `Alice's AccountId`
     - `loan_manager_address`: `LOAN_MANAGER_ADDRESS` (as Address)

2. **Check if loan disbursed**:
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `get_loan`
   - Args: `1`
   - Should show: `status: Active` (if enough vouches collected)
   - `start_time` should be set

3. **Check vouches**:
   - Contract: `VOUCH_ADDRESS`
   - Message: `get_vouches_for_loan`
   - Args: `1`
   - Should return: `1`

### Step 5: Repay the Loan

**Charlie repays the loan**:

1. **Get repayment amount**:
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `get_repayment_amount`
   - Args: `1`
   - Returns the fixed repayment amount (calculated at loan creation)

2. **Repay loan**:
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `repay_loan`
   - Args:
     - `loan_id`: `1`
     - `borrower_account_id`: `Charlie's AccountId`
     - `loan_manager_address`: `LOAN_MANAGER_ADDRESS` (as Address)
   - Value: `repayment_amount` (the amount returned from `get_repayment_amount` in 18 decimals)
   - **Important**: The value must be in 18 decimals (chain format), matching what `get_repayment_amount` returns

3. **Verify repayment**:
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `get_loan`
   - Args: `1`
   - Should show: `status: Repaid`

4. **Check Alice's stars** (should have bonus):
   - Contract: `REPUTATION_ADDRESS`
   - Message: `get_stars`
   - Args: `Alice's AccountId`
   - Should be: `100 - 10 + 10 + 2 = 102` (staked 10, got back 10 + 2 bonus)

### Step 6: Query Functions

**Get all pending loans**:
- Contract: `LOAN_MANAGER_ADDRESS`
- Message: `get_all_pending_loans`
- Returns: `Vec<u64>` of loan IDs with Pending status

**Get all active loans**:
- Contract: `LOAN_MANAGER_ADDRESS`
- Message: `get_all_active_loans`
- Returns: `Vec<u64>` of loan IDs with Active status

**Get user yield** (read-only, doesn't accrue interest):
- Contract: `LENDING_POOL_ADDRESS`
- Message: `get_user_yield`
- Args: `account_id`
- Returns: Yield in 18 decimals (chain format)

**Get user yield with interest accrual**:
- Contract: `LENDING_POOL_ADDRESS`
- Message: `accrue_interest_and_get_user_yield`
- Args: `account_id`
- Returns: Yield in 18 decimals (chain format)
- Note: This function mutates state to accrue interest before calculating yield

## Important Notes

### AccountId vs Address

The contracts use both `AccountId` and `Address` types:
- **AccountId**: 32-byte identifier used for storage and internal operations
- **Address**: 20-byte H160 identifier used for contract addresses

**When calling functions**:
- Functions that require `account_id` parameter: Pass the AccountId of the user
- Functions that require `*_address` parameter: Pass the Address (H160) of the contract

### Decimal Precision

The contracts use two decimal formats:
- **10 decimals (storage format)**: Used for `user_deposits` storage
- **18 decimals (chain format)**: Used for all transfers and `total_liquidity`

**Important conversions**:
- `deposit()`: Send value in 18 decimals (e.g., 100 tokens = `100000000000000000000`)
- `withdraw(amount, account_id)`: Pass `amount` in 10 decimals (e.g., 100 tokens = `10000000000`)
- `get_user_deposit(user)`: Returns value in 10 decimals
- `get_total_liquidity()`: Returns value in 18 decimals
- `get_repayment_amount(loan_id)`: Returns value in 18 decimals
- `repay_loan()`: Send value in 18 decimals (use the value from `get_repayment_amount`)

**Example**:
- To deposit 100 tokens: Send `100000000000000000000` (18 decimals)
- To withdraw 5 tokens: Call `withdraw(5000000000, account_id)` (5 * 10^10)
- To repay 550 tokens: Send `550000000000000000000` (18 decimals)

### Interest Rate Calculation

- Interest rates are **fixed at loan creation** (not time-based)
- Repayment amount = `amount × (1 + interest_rate_percentage)`
- Example: 100 tokens at 10% = 110 tokens repayment
- The repayment amount is stored in the loan and can be queried with `get_repayment_amount(loan_id)`

### Loan Lifecycle

1. **Pending**: Loan requested, waiting for vouches
2. **Active**: Loan disbursed, borrower must repay
3. **Repaid**: Loan fully repaid, vouchers rewarded
4. **Defaulted**: Loan expired unpaid, borrower penalized, vouchers slashed

## Common Issues

1. **"Unauthorized" errors**: 
   - Make sure all contract references are set up
   - Verify you're using the correct AccountId/Address types
   - Check that admin functions are called by the admin account

2. **"InsufficientVouches"**: 
   - Need to wait for enough vouches or add more vouchers
   - Check tier requirements: Tier 1 needs 1 vouch, Tier 2 needs 2, Tier 3 needs 3

3. **"ExposureCapExceeded"**: 
   - Borrower has too much exposure relative to total pool liquidity
   - Need more liquidity in the pool or reduce vouch amounts

4. **"LoanNotPending"**: 
   - Loan already disbursed or doesn't exist
   - Check loan status with `get_loan(loan_id)`

5. **"NotEnoughStars"**: 
   - Voucher doesn't have enough stars to stake
   - Use `admin_set_stars` to give more stars

6. **"InvalidRepaymentAmount"**: 
   - The transferred value doesn't match the required repayment amount
   - Use `get_repayment_amount(loan_id)` to get the exact amount in 18 decimals
   - Make sure you're sending the value in 18 decimals (chain format), not 10 decimals
   - The value from `get_repayment_amount` is already in 18 decimals - use it directly

7. **"Overflow"**: 
   - Transferred value exceeds u128::MAX
   - Reduce the repayment amount

## Quick Test Checklist

- [ ] All contracts deployed in correct order
- [ ] Config admin set with `set_admin`
- [ ] All contract references set up (6 total)
- [ ] Admin set stars for test accounts (Alice, Bob, Charlie)
- [ ] Liquidity added to pool (Alice and Bob deposit)
- [ ] Loan requested (status: Pending)
- [ ] Vouches added (loan auto-disburses when threshold met)
- [ ] Loan status changed to Active
- [ ] Repayment amount queried with `get_repayment_amount`
- [ ] Loan repaid (vouchers get bonus stars)
- [ ] Loan status changed to Repaid
- [ ] Query functions tested (`get_all_pending_loans`, `get_all_active_loans`)

## Advanced Testing Scenarios

### Test Default Scenario

1. Request a loan
2. Get vouches and disburse
3. Wait for loan term to expire (or manually advance time)
4. Call `check_default(loan_id, loan_manager_address, vouch_contract_address)`
5. Verify:
   - Loan status is `Defaulted`
   - Borrower's stars are slashed
   - Vouchers lost their staked stars and capital

### Test Multiple Loans

1. Request multiple loans with different borrowers
2. Use `get_all_pending_loans()` to see all pending loans
3. Use `get_all_active_loans()` to see all active loans
4. Test repayment of specific loans

### Test Interest Rate Calculation

1. Request a loan with a known amount (e.g., 100 tokens)
2. Check the interest rate applied (depends on pool utilization and borrower stars)
3. Call `get_repayment_amount(loan_id)` to verify:
   - Repayment = amount × (1 + interest_rate_percentage)
   - Example: 100 tokens at 10% = 110 tokens
