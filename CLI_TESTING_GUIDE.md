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

2. **Deploy Reputation**
   - Upload: `reputation/target/ink/reputation.contract`
   - Constructor: `new(CONFIG_ADDRESS)`
   - Note: `REPUTATION_ADDRESS`
   - **Important**: The deployer becomes the admin

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

After deployment, you MUST set up the contract references:

1. **Reputation.set_vouch_contract**
   - Contract: `REPUTATION_ADDRESS`
   - Message: `set_vouch_contract`
   - Args: `VOUCH_ADDRESS`

2. **Reputation.set_loan_manager**
   - Contract: `REPUTATION_ADDRESS`
   - Message: `set_loan_manager`
   - Args: `LOAN_MANAGER_ADDRESS`

3. **LendingPool.set_vouch_contract**
   - Contract: `LENDING_POOL_ADDRESS`
   - Message: `set_vouch_contract`
   - Args: `VOUCH_ADDRESS`

4. **LendingPool.set_loan_manager**
   - Contract: `LENDING_POOL_ADDRESS`
   - Message: `set_loan_manager`
   - Args: `LOAN_MANAGER_ADDRESS`

5. **Vouch.set_loan_manager**
   - Contract: `VOUCH_ADDRESS`
   - Message: `set_loan_manager`
   - Args: `LOAN_MANAGER_ADDRESS`

6. **LoanManager.set_lending_pool**
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `set_lending_pool`
   - Args: `LENDING_POOL_ADDRESS`

## Testing Flow

### Setup Test Accounts

The local node comes with pre-funded accounts:
- **Alice** (default account)
- **Bob** (create new account)
- **Charlie** (create new account)

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

1. **Alice deposits 1000 tokens**:
   - Contract: `LENDING_POOL_ADDRESS`
   - Message: `deposit`
   - Value: `100` (with 9 decimals: `1000000000000`)

2. **Bob deposits 1000 tokens**:
   - Contract: `LENDING_POOL_ADDRESS`
   - Message: `deposit`
   - Value: `100` (with 9 decimals: `1000000000000`)

3. **Verify deposits**:
   - Contract: `LENDING_POOL_ADDRESS`
   - Message: `get_user_deposit`
   - Args: `Alice's AccountId` → Should return `100000000000`
   - Message: `get_total_liquidity` → Should return `200000000000`

### Step 3: Request a Loan

**Charlie requests a Tier 1 loan** (needs 5 stars, 1 vouch):

1. **Request loan**:
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `request_loan`
   - Args: 
     - `amount`: `500000000000` (50 tokens with 10 decimals)
     - `purpose`: `[]` (empty Vec<u8>)
   - Note the returned `loan_id` (should be `0`)

2. **Check loan status**:
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `get_loan`
   - Args: `0`
   - Should show: `status: Pending`, `vouchers: []`

### Step 4: Vouch for the Loan

**Alice vouches for Charlie's loan**:

1. **Vouch for loan**:
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `vouch_for_loan`
   - Args:
     - `loan_id`: `0`
     - `stars`: `10`
     - `capital_percent`: `10` (10% of Alice's deposit = 100 tokens)

2. **Check if loan disbursed**:
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `get_loan`
   - Args: `0`
   - Should show: `status: Active` (if enough vouches collected)
   - `start_time` should be set

3. **Check vouches**:
   - Contract: `VOUCH_ADDRESS`
   - Message: `get_vouches_for_loan`
   - Args: `0`
   - Should return: `1`

### Step 5: Repay the Loan

**Charlie repays the loan**:

1. **Calculate repayment** (you'll need to calculate principal + interest):
   - Principal: `500000000000`
   - Interest: depends on time elapsed and rate
   - For quick test, you can estimate: `principal * 1.01` (1% interest if repaid quickly)

2. **Repay loan**:
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `repay_loan`
   - Args: `0`
   - Value: `repayment_amount` (principal + interest)

3. **Verify repayment**:
   - Contract: `LOAN_MANAGER_ADDRESS`
   - Message: `get_loan`
   - Args: `0`
   - Should show: `status: Repaid`

4. **Check Alice's stars** (should have bonus):
   - Contract: `REPUTATION_ADDRESS`
   - Message: `get_stars`
   - Args: `Alice's AccountId`
   - Should be: `100 - 10 + 10 + 2 = 102` (staked 10, got back 10 + 2 bonus)


## Common Issues

1. **"Unauthorized" errors**: Make sure all contract references are set up
2. **"InsufficientVouches"**: Need to wait for enough vouches or add more
3. **"ExposureCapExceeded"**: Borrower has too much exposure, need more liquidity
4. **"LoanNotPending"**: Loan already disbursed or doesn't exist
5. **"NotEnoughStars"**: Voucher doesn't have enough stars to stake

## Quick Test Checklist

- [ ] All contracts deployed
- [ ] All contract references set
- [ ] Admin set stars for test accounts
- [ ] Liquidity added to pool
- [ ] Loan requested (status: Pending)
- [ ] Vouches added (loan auto-disburses)
- [ ] Loan repaid (vouchers get bonus)
- [ ] Default scenario tested

## Next Steps

For production:
1. Remove admin functions or add proper access control
2. Add more comprehensive error messages
3. Add events for all state changes
4. Add query functions for better UX
5. Implement proper exposure tracking per loan

