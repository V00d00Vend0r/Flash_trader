/// Flash loan module for Ravenslinger trading bot
/// Enables atomic borrowing and repayment within a single transaction
module raveslinger::flash_loan {
    use sui::object::{Self, UID};
    use sui::tx_context::{Self, TxContext};
    use sui::tx_context;
    use sui::coin::{Self, Coin};
    use sui::balance::{Self, Balance};
    use sui::transfer;
    use sui::event;
    use sui::clock::{Self, Clock};
    use sui::clock;
    use std::math;

    /// Error codes (grouped by category with consistent numbering)
    // Authorization errors (100-199)
    const EUnauthorized: u64 = 100;
    const EInvalidCap: u64 = 101;
    
    // Pool state errors (200-299)
    const EPoolPaused: u64 = 200;
    const EInsufficientLiquidity: u64 = 201;
    
    // Parameter validation errors (300-399)
    const EInvalidFeeRate: u64 = 300;
    const ERepaymentTooLow: u64 = 301;
    const ELoanTooLarge: u64 = 302;
    const EInvalidLoanParams: u64 = 303;

    /// Borrow capability tying a user to a specific pool
    struct BorrowCap<phantom T> has key, store {
        id: UID,
        owner: address,
        pool_id: object::ID,
    }

    /// Flash loan pool for a specific coin type
    struct FlashLoanPool<phantom T> has key, store {
        id: UID,
        balance: Balance<T>,
        fee_rate_bps: u64,        // Fee rate in basis points (e.g., 30 = 0.3%)
        min_fee: u64,             // Minimum fee charged for any loan
        max_loan_bps: u64,        // Maximum portion of pool that can be borrowed (bps)
        max_loan_amount: u64,     // Absolute maximum borrow amount
        paused: bool,             // Pauses borrowing when true
        admin: address,
        treasury: address,        // Separate address for fee collection
    }

    /// Hot potato struct that must be consumed by repaying the loan
    struct FlashLoan<phantom T> {
        amount: u64,
        fee: u64,
        pool_id: object::ID,
        borrower: address,
        epoch: u64,
        timestamp_ms: u64,
    }

    /// Events
    struct FlashLoanBorrowed<phantom T> has copy, drop {
        pool_id: object::ID,
        borrower: address,
        amount: u64,
        fee: u64,
        timestamp_ms: u64,
    }

    struct FlashLoanRepaid<phantom T> has copy, drop {
        pool_id: object::ID,
        borrower: address,
        amount: u64,
        fee: u64,
        timestamp_ms: u64,
        profit: u64,
    }

    struct PoolCreated<phantom T> has copy, drop {
        pool_id: object::ID,
        admin: address,
        treasury: address,
        initial_liquidity: u64,
        fee_rate_bps: u64,
        min_fee: u64,
        max_loan_bps: u64,
        max_loan_amount: u64,
    }

    struct AdminUpdated<phantom T> has copy, drop {
        pool_id: object::ID,
        old_admin: address,
        new_admin: address,
    }

    struct TreasuryUpdated<phantom T> has copy, drop {
        pool_id: object::ID,
        old_treasury: address,
        new_treasury: address,
    }

    /// Create a new flash loan pool
    public fun create_pool<T>(
        initial_liquidity: Coin<T>,
        fee_rate_bps: u64,
        min_fee: u64,
        max_loan_bps: u64,
        max_loan_amount: u64,
        treasury: address,
        ctx: &mut TxContext
    ): FlashLoanPool<T> {
        assert!(fee_rate_bps <= 1000, EInvalidFeeRate);
        assert!(max_loan_bps <= 10000, EInvalidFeeRate);
        assert!(min_fee <= max_loan_amount, EInvalidLoanParams);
        assert!(max_loan_amount > 0, EInvalidLoanParams);
        
        let pool_id = object::new(ctx);
        let pool_id_copy = object::uid_to_inner(&pool_id);
        let admin = tx_context::sender(ctx);
        let amount = coin::value(&initial_liquidity);
        
        event::emit(PoolCreated<T> {
            pool_id: pool_id_copy,
            admin,
            treasury,
            initial_liquidity: amount,
            fee_rate_bps,
            min_fee,
            max_loan_bps,
            max_loan_amount,
        });
        
        FlashLoanPool<T> {
            id: pool_id,
            balance: coin::into_balance(initial_liquidity),
            fee_rate_bps,
            min_fee,
            max_loan_bps,
            max_loan_amount,
            paused: false,
            admin,
            treasury,
        }
    }

    /// Mint a borrow capability for this pool (admin only)
    public fun mint_borrow_cap<T>(
        pool: &FlashLoanPool<T>,
        to: address,
        ctx: &mut TxContext
    ) {
        assert!(tx_context::sender(ctx) == pool.admin, EUnauthorized);
        let cap_id = object::new(ctx);
        let cap = BorrowCap<T> { 
            id: cap_id, 
            owner: to, 
            pool_id: object::uid_to_inner(&pool.id) 
        };
        transfer::transfer(cap, to);
    }

    /// Borrow from the flash loan pool
    public fun borrow<T>(
        pool: &mut FlashLoanPool<T>,
        amount: u64,
        cap: &BorrowCap<T>,
        clock: &Clock,
        ctx: &mut TxContext
    ): (Coin<T>, FlashLoan<T>) {
        let sender = tx_context::sender(ctx);
        let timestamp_ms = clock::timestamp_ms(clock);
        
        assert!(!pool.paused, EPoolPaused);
        assert!(cap.owner == sender, EUnauthorized);
        assert!(cap.pool_id == object::uid_to_inner(&pool.id), EInvalidCap);
        
        let available = balance::value(&pool.balance);
        let max_by_bps = (available * pool.max_loan_bps) / 10000;
        let max_allowed = math::min(max_by_bps, pool.max_loan_amount);
        
        assert!(amount <= max_allowed, ELoanTooLarge);
        assert!(amount <= available, EInsufficientLiquidity);
        
        // Calculate fee
        let raw_fee = (amount * pool.fee_rate_bps) / 10000;
        let fee = math::max(raw_fee, pool.min_fee);
        
        let borrowed_balance = balance::split(&mut pool.balance, amount);
        let borrowed_coin = coin::from_balance(borrowed_balance, ctx);
        let pool_id = object::uid_to_inner(&pool.id);
        let epoch = tx_context::epoch(ctx);
        
        event::emit(FlashLoanBorrowed<T> {
            pool_id,
            borrower: sender,
            amount,
            fee,
            timestamp_ms,
        });
        
        let ticket = FlashLoan<T> {
            amount,
            fee,
            pool_id,
            borrower: sender,
            epoch,
            timestamp_ms,
        };
        
        (borrowed_coin, ticket)
    }

    /// Repay the flash loan
    public fun repay<T>(
        pool: &mut FlashLoanPool<T>,
        repayment: Coin<T>,
        ticket: FlashLoan<T>,
        clock: &Clock,
        ctx: &mut TxContext
    ) {
        let FlashLoan { amount, fee, pool_id, borrower, epoch, timestamp_ms: _ } = ticket;
        let current_time = clock::timestamp_ms(clock);
        
        assert!(pool_id == object::uid_to_inner(&pool.id), EUnauthorized);
        assert!(borrower == tx_context::sender(ctx), EUnauthorized);
        assert!(epoch == tx_context::epoch(ctx), EUnauthorized);
        
        let repayment_amount = coin::value(&repayment);
        let required = amount + fee;
        assert!(repayment_amount >= required, ERepaymentTooLow);
        
        let profit = if (repayment_amount > required) { repayment_amount - required } else { 0 };
        
        event::emit(FlashLoanRepaid<T> {
            pool_id,
            borrower,
            amount,
            fee,
            timestamp_ms: current_time,
            profit,
        });
        
        balance::join(&mut pool.balance, coin::into_balance(repayment));
    }

    public fun add_liquidity<T>(pool: &mut FlashLoanPool<T>, liquidity: Coin<T>, ctx: &mut TxContext) {
        assert!(tx_context::sender(ctx) == pool.admin, EUnauthorized);
        balance::join(&mut pool.balance, coin::into_balance(liquidity));
    }

    public fun remove_liquidity<T>(pool: &mut FlashLoanPool<T>, amount: u64, ctx: &mut TxContext): Coin<T> {
        assert!(tx_context::sender(ctx) == pool.admin, EUnauthorized);
        let withdrawn_balance = balance::split(&mut pool.balance, amount);
        coin::from_balance(withdrawn_balance, ctx)
    }

    public fun update_fee_rate<T>(pool: &mut FlashLoanPool<T>, new_fee_rate_bps: u64, ctx: &mut TxContext) {
        assert!(tx_context::sender(ctx) == pool.admin, EUnauthorized);
        assert!(new_fee_rate_bps <= 1000, EInvalidFeeRate);
        pool.fee_rate_bps = new_fee_rate_bps;
    }

    public fun update_min_fee<T>(pool: &mut FlashLoanPool<T>, new_min_fee: u64, ctx: &mut TxContext) {
        assert!(tx_context::sender(ctx) == pool.admin, EUnauthorized);
        pool.min_fee = new_min_fee;
    }

    public fun update_max_loan_bps<T>(pool: &mut FlashLoanPool<T>, new_max_bps: u64, ctx: &mut TxContext) {
        assert!(tx_context::sender(ctx) == pool.admin, EUnauthorized);
        assert!(new_max_bps <= 10000, EInvalidFeeRate);
        pool.max_loan_bps = new_max_bps;
    }

    public fun update_max_loan_amount<T>(pool: &mut FlashLoanPool<T>, new_max_amount: u64, ctx: &mut TxContext) {
        assert!(tx_context::sender(ctx) == pool.admin, EUnauthorized);
        pool.max_loan_amount = new_max_amount;
    }

    public fun set_paused<T>(pool: &mut FlashLoanPool<T>, paused: bool, ctx: &mut TxContext) {
        assert!(tx_context::sender(ctx) == pool.admin, EUnauthorized);
        pool.paused = paused;
    }

    public fun transfer_admin<T>(pool: &mut FlashLoanPool<T>, new_admin: address, ctx: &mut TxContext) {
        let old_admin = pool.admin;
        assert!(tx_context::sender(ctx) == old_admin, EUnauthorized);
        pool.admin = new_admin;
        event::emit(AdminUpdated<T> { pool_id: object::uid_to_inner(&pool.id), old_admin, new_admin });
    }

    public fun update_treasury<T>(pool: &mut FlashLoanPool<T>, new_treasury: address, ctx: &mut TxContext) {
        let old_treasury = pool.treasury;
        assert!(tx_context::sender(ctx) == pool.admin, EUnauthorized);
        pool.treasury = new_treasury;
        event::emit(TreasuryUpdated<T> { pool_id: object::uid_to_inner(&pool.id), old_treasury, new_treasury });
    }

    public fun emergency_withdraw<T>(pool: &mut FlashLoanPool<T>, amount: u64, to: address, ctx: &mut TxContext): Coin<T> {
        assert!(tx_context::sender(ctx) == pool.admin, EUnauthorized);
        let withdrawn_balance = balance::split(&mut pool.balance, amount);
        coin::from_balance(withdrawn_balance, ctx)
    }

    public fun pool_balance<T>(pool: &FlashLoanPool<T>): u64 { balance::value(&pool.balance) }
    public fun pool_fee_rate<T>(pool: &FlashLoanPool<T>): u64 { pool.fee_rate_bps }
    public fun pool_admin<T>(pool: &FlashLoanPool<T>): address { pool.admin }
    public fun pool_treasury<T>(pool: &FlashLoanPool<T>): address { pool.treasury }
    public fun pool_min_fee<T>(pool: &FlashLoanPool<T>): u64 { pool.min_fee }
    public fun pool_max_loan_bps<T>(pool: &FlashLoanPool<T>): u64 { pool.max_loan_bps }
    public fun pool_max_loan_amount<T>(pool: &FlashLoanPool<T>): u64 { pool.max_loan_amount }
    public fun pool_paused<T>(pool: &FlashLoanPool<T>): bool { pool.paused }
    public fun calculate_max_borrowable<T>(pool: &FlashLoanPool<T>): u64 {
        let available = balance::value(&pool.balance);
        let max_by_bps = (available * pool.max_loan_bps) / 10000;
        math::min(max_by_bps, pool.max_loan_amount)
    }
    public fun calculate_fee<T>(pool: &FlashLoanPool<T>, amount: u64): u64 {
        let raw_fee = (amount * pool.fee_rate_bps) / 10000;
        math::max(raw_fee, pool.min_fee)
    }
    public fun share_pool<T>(pool: FlashLoanPool<T>) { transfer::share_object(pool); }
}
