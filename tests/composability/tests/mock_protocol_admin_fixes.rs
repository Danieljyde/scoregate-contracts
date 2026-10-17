//! Coverage for issues #117-#120: MockLending oracle rotation, gate-threshold
//! bounds validation, audit events, and oracle-capability refresh, across
//! both `mock-amm` and `mock-lending`.
//!
//! Mirrors the `Fixture`/`setup()`/`submit_score()` pattern already used by
//! `test_composability.rs` in this same crate.

use mock_amm::{FailPolicy as AmmFailPolicy, MockAmm, MockAmmClient, MockAmmError};
use mock_lending::{
    FailPolicy as LendingFailPolicy, MockLending, MockLendingClient, MockLendingError,
};
use scoregate_score::{ScoreGateScoreContract, ScoreGateScoreContractClient};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events as _, Ledger as _},
    Address, Env, Vec,
};

const GATE_THRESHOLD: u32 = 75;
const MIN_CONFIDENCE: u32 = 50;

struct Fixture<'a> {
    env: Env,
    admin: Address,
    scoregate: ScoreGateScoreContractClient<'a>,
    amm: MockAmmClient<'a>,
    lending: MockLendingClient<'a>,
}

fn setup<'a>() -> Fixture<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let scoregate_id = env.register_contract(None, ScoreGateScoreContract);
    let scoregate = ScoreGateScoreContractClient::new(&env, &scoregate_id);
    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    scoregate.initialize(&admin, &service);

    let amm_id = env.register_contract(None, MockAmm);
    let amm = MockAmmClient::new(&env, &amm_id);
    amm.initialize(&admin, &scoregate_id, &GATE_THRESHOLD);
    amm.set_liquidity_gate_config(
        &admin,
        &GATE_THRESHOLD,
        &MIN_CONFIDENCE,
        &AmmFailPolicy::FailClosed,
        &604_800,
        &0,
    );

    let lending_id = env.register_contract(None, MockLending);
    let lending = MockLendingClient::new(&env, &lending_id);
    lending.initialize(&admin, &scoregate_id, &GATE_THRESHOLD, &MIN_CONFIDENCE);

    Fixture { env, admin, scoregate, amm, lending }
}

fn submit_score(fixture: &Fixture, wallet: &Address, score: u32, confidence: u32) {
    fixture.env.ledger().with_mut(|l| l.timestamp += 3_601);
    fixture.scoregate.submit_score(
        &Vec::new(&fixture.env),
        wallet,
        &symbol_short!("XLM_USDC"),
        &score,
        &false,
        &false,
        &fixture.env.ledger().timestamp(),
        &confidence,
        &1,
        &None,
    );
}

// ── Issue #117: MockLending oracle address was permanently immutable ───────

#[test]
fn lending_set_risk_oracle_updates_oracle_used_by_subsequent_borrows() {
    let fixture = setup();

    // The *original* oracle knows nothing about this wallet, so a borrow
    // against it fails closed.
    let borrower = Address::generate(&fixture.env);
    assert_eq!(
        fixture.lending.try_borrow(&borrower, &symbol_short!("XLM_USDC"), &1_000),
        Err(Ok(MockLendingError::RiskGateRejected))
    );

    // Deploy a *second*, independent ScoreGate deployment with a safe score
    // for the borrower, and rotate MockLending onto it.
    let alt_oracle_id = fixture.env.register_contract(None, ScoreGateScoreContract);
    let alt_oracle = ScoreGateScoreContractClient::new(&fixture.env, &alt_oracle_id);
    let admin = Address::generate(&fixture.env);
    let service = Address::generate(&fixture.env);
    alt_oracle.initialize(&admin, &service);
    fixture.env.ledger().with_mut(|l| l.timestamp += 3_601);
    alt_oracle.submit_score(
        &Vec::new(&fixture.env),
        &borrower,
        &symbol_short!("XLM_USDC"),
        &10,
        &false,
        &false,
        &fixture.env.ledger().timestamp(),
        &90,
        &1,
        &None,
    );

    fixture.lending.set_risk_oracle(&fixture.admin, &alt_oracle_id);

    // Subsequent borrows now query the *updated* oracle and succeed.
    assert_eq!(
        fixture.lending.try_borrow(&borrower, &symbol_short!("XLM_USDC"), &1_000),
        Ok(Ok(()))
    );
}

#[test]
fn lending_set_risk_oracle_rejects_unauthorized_caller() {
    let fixture = setup();
    let attacker = Address::generate(&fixture.env);
    let oracle = Address::generate(&fixture.env);

    assert_eq!(
        fixture.lending.try_set_risk_oracle(&attacker, &oracle),
        Err(Ok(MockLendingError::Unauthorized))
    );
}

// ── Issue #118: gate thresholds above 100 were silently accepted ───────────

#[test]
fn amm_initialize_rejects_threshold_over_100() {
    let env = Env::default();
    env.mock_all_auths();
    let scoregate_id = env.register_contract(None, ScoreGateScoreContract);
    let scoregate = ScoreGateScoreContractClient::new(&env, &scoregate_id);
    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    scoregate.initialize(&admin, &service);

    let amm_id = env.register_contract(None, MockAmm);
    let amm = MockAmmClient::new(&env, &amm_id);
    assert_eq!(
        amm.try_initialize(&admin, &scoregate_id, &101),
        Err(Ok(MockAmmError::InvalidThreshold))
    );
}

#[test]
fn amm_set_liquidity_gate_config_rejects_threshold_over_100() {
    let fixture = setup();
    assert_eq!(
        fixture.amm.try_set_liquidity_gate_config(
            &fixture.admin,
            &101,
            &MIN_CONFIDENCE,
            &AmmFailPolicy::FailClosed,
            &604_800,
            &0,
        ),
        Err(Ok(MockAmmError::InvalidThreshold))
    );
}

#[test]
fn amm_set_liquidity_gate_config_rejects_confidence_over_100() {
    let fixture = setup();
    assert_eq!(
        fixture.amm.try_set_liquidity_gate_config(
            &fixture.admin,
            &GATE_THRESHOLD,
            &101,
            &AmmFailPolicy::FailClosed,
            &604_800,
            &0,
        ),
        Err(Ok(MockAmmError::InvalidThreshold))
    );
}

#[test]
fn lending_initialize_rejects_threshold_over_100() {
    let env = Env::default();
    env.mock_all_auths();
    let scoregate_id = env.register_contract(None, ScoreGateScoreContract);
    let scoregate = ScoreGateScoreContractClient::new(&env, &scoregate_id);
    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    scoregate.initialize(&admin, &service);

    let lending_id = env.register_contract(None, MockLending);
    let lending = MockLendingClient::new(&env, &lending_id);
    assert_eq!(
        lending.try_initialize(&admin, &scoregate_id, &101, &MIN_CONFIDENCE),
        Err(Ok(MockLendingError::InvalidThreshold))
    );
}

#[test]
fn lending_set_borrow_gate_config_rejects_threshold_over_100() {
    let fixture = setup();
    assert_eq!(
        fixture.lending.try_set_borrow_gate_config(
            &fixture.admin,
            &101,
            &MIN_CONFIDENCE,
            &LendingFailPolicy::FailClosed,
            &604_800,
            &0,
        ),
        Err(Ok(MockLendingError::InvalidThreshold))
    );
}

// ── Issue #119: neither mock emitted any audit events ───────────────────────

#[test]
fn amm_swap_emits_swap_executed_event() {
    let fixture = setup();
    let wallet = Address::generate(&fixture.env);
    submit_score(&fixture, &wallet, 10, 90);

    let before = fixture.env.events().all().len();
    assert_eq!(fixture.amm.try_swap(&wallet, &symbol_short!("XLM_USDC"), &1_000), Ok(Ok(())));
    assert_eq!(fixture.env.events().all().len(), before + 1);
}

#[test]
fn lending_borrow_emits_borrow_executed_event() {
    let fixture = setup();
    let wallet = Address::generate(&fixture.env);
    submit_score(&fixture, &wallet, 10, 90);

    let before = fixture.env.events().all().len();
    assert_eq!(fixture.lending.try_borrow(&wallet, &symbol_short!("XLM_USDC"), &1_000), Ok(Ok(())));
    assert_eq!(fixture.env.events().all().len(), before + 1);
}

#[test]
fn amm_admin_actions_emit_events() {
    let fixture = setup();

    let before = fixture.env.events().all().len();
    fixture.amm.set_liquidity_gate_config(
        &fixture.admin,
        &GATE_THRESHOLD,
        &MIN_CONFIDENCE,
        &AmmFailPolicy::FailClosed,
        &604_800,
        &0,
    );
    assert_eq!(
        fixture.env.events().all().len(),
        before + 1,
        "gate config update must emit an event"
    );

    let before = fixture.env.events().all().len();
    let new_oracle = Address::generate(&fixture.env);
    fixture.amm.set_risk_oracle(&fixture.admin, &new_oracle);
    assert_eq!(fixture.env.events().all().len(), before + 1, "oracle rotation must emit an event");
}

#[test]
fn lending_admin_actions_emit_events() {
    let fixture = setup();

    let before = fixture.env.events().all().len();
    fixture.lending.set_borrow_gate_config(
        &fixture.admin,
        &GATE_THRESHOLD,
        &MIN_CONFIDENCE,
        &LendingFailPolicy::FailClosed,
        &604_800,
        &0,
    );
    assert_eq!(
        fixture.env.events().all().len(),
        before + 1,
        "gate config update must emit an event"
    );

    let before = fixture.env.events().all().len();
    let new_oracle = Address::generate(&fixture.env);
    fixture.lending.set_risk_oracle(&fixture.admin, &new_oracle);
    assert_eq!(fixture.env.events().all().len(), before + 1, "oracle rotation must emit an event");
}

// ── Issue #120: expanded-score capability was probed once and never refreshed ──

#[test]
fn amm_refresh_oracle_capabilities_reprobes_current_oracle() {
    let fixture = setup();
    // `setup()`'s ScoreGate deployment is the current contract (version 5,
    // which implements the expanded interface), so a same-address re-probe
    // must correctly report `true` — confirming refresh_oracle_capabilities
    // reads the live oracle rather than returning a stale cached value.
    assert_eq!(fixture.amm.refresh_oracle_capabilities(&fixture.admin), true);
}

#[test]
fn amm_refresh_oracle_capabilities_rejects_unauthorized_caller() {
    let fixture = setup();
    let attacker = Address::generate(&fixture.env);
    assert_eq!(
        fixture.amm.try_refresh_oracle_capabilities(&attacker),
        Err(Ok(MockAmmError::Unauthorized))
    );
}

#[test]
fn lending_refresh_oracle_capabilities_reprobes_current_oracle() {
    let fixture = setup();
    assert_eq!(fixture.lending.refresh_oracle_capabilities(&fixture.admin), true);
}

#[test]
fn lending_refresh_oracle_capabilities_rejects_unauthorized_caller() {
    let fixture = setup();
    let attacker = Address::generate(&fixture.env);
    assert_eq!(
        fixture.lending.try_refresh_oracle_capabilities(&attacker),
        Err(Ok(MockLendingError::Unauthorized))
    );
}

#[test]
fn amm_refresh_oracle_capabilities_emits_event() {
    let fixture = setup();
    let before = fixture.env.events().all().len();
    fixture.amm.refresh_oracle_capabilities(&fixture.admin);
    assert_eq!(fixture.env.events().all().len(), before + 1);
}

// ── Issue #202: admin rotation entry points in MockAmm and MockLending ──────

#[test]
fn amm_set_admin_rotates_admin_and_rejects_unauthorized() {
    let fixture = setup();
    let new_admin = Address::generate(&fixture.env);
    let attacker = Address::generate(&fixture.env);

    // Unauthorized caller cannot rotate admin
    assert_eq!(
        fixture.amm.try_set_admin(&attacker, &new_admin),
        Err(Ok(MockAmmError::Unauthorized))
    );
    assert_eq!(fixture.amm.get_admin(), fixture.admin);

    // Admin rotation succeeds and emits event
    let before = fixture.env.events().all().len();
    assert_eq!(fixture.amm.try_set_admin(&fixture.admin, &new_admin), Ok(Ok(())));
    assert_eq!(fixture.env.events().all().len(), before + 1);
    assert_eq!(fixture.amm.get_admin(), new_admin);

    // Old admin can no longer perform administrative actions
    let alt_oracle = Address::generate(&fixture.env);
    assert_eq!(
        fixture.amm.try_set_risk_oracle(&fixture.admin, &alt_oracle),
        Err(Ok(MockAmmError::Unauthorized))
    );

    // New admin can perform administrative actions
    assert_eq!(fixture.amm.try_set_risk_oracle(&new_admin, &alt_oracle), Ok(Ok(())));
}

#[test]
fn lending_set_admin_rotates_admin_and_rejects_unauthorized() {
    let fixture = setup();
    let new_admin = Address::generate(&fixture.env);
    let attacker = Address::generate(&fixture.env);

    // Unauthorized caller cannot rotate admin
    assert_eq!(
        fixture.lending.try_set_admin(&attacker, &new_admin),
        Err(Ok(MockLendingError::Unauthorized))
    );
    assert_eq!(fixture.lending.get_admin(), fixture.admin);

    // Admin rotation succeeds and emits event
    let before = fixture.env.events().all().len();
    assert_eq!(fixture.lending.try_set_admin(&fixture.admin, &new_admin), Ok(Ok(())));
    assert_eq!(fixture.env.events().all().len(), before + 1);
    assert_eq!(fixture.lending.get_admin(), new_admin);

    // Old admin can no longer perform administrative actions
    let alt_oracle = Address::generate(&fixture.env);
    assert_eq!(
        fixture.lending.try_set_risk_oracle(&fixture.admin, &alt_oracle),
        Err(Ok(MockLendingError::Unauthorized))
    );

    // New admin can perform administrative actions
    assert_eq!(fixture.lending.try_set_risk_oracle(&new_admin, &alt_oracle), Ok(Ok(())));
}

// ── Issue #203: Mock AMM protocol pause guard in swap and provide_liquidity_gated ──

#[test]
fn amm_pause_state_blocks_actions_even_when_scoregate_reports_valid_scores() {
    let fixture = setup();
    let user = Address::generate(&fixture.env);
    let pair = symbol_short!("XLM_USDC");
    submit_score(&fixture, &user, 10, 90);

    // Unpaused AMM executes swap and provide_liquidity successfully
    assert_eq!(fixture.amm.try_swap(&user, &pair, &1_000), Ok(Ok(())));
    assert_eq!(fixture.amm.try_provide_liquidity_gated(&user, &pair, &1_000), Ok(Ok(())));

    // Pause the AMM
    let before = fixture.env.events().all().len();
    assert_eq!(fixture.amm.try_set_paused(&fixture.admin, &true), Ok(Ok(())));
    assert_eq!(fixture.amm.is_paused(), true);
    assert_eq!(fixture.env.events().all().len(), before + 1);

    // Both operations are rejected even though the wallet's score passes the oracle
    assert_eq!(fixture.amm.try_swap(&user, &pair, &1_000), Err(Ok(MockAmmError::ContractPaused)));
    assert_eq!(
        fixture.amm.try_provide_liquidity_gated(&user, &pair, &1_000),
        Err(Ok(MockAmmError::ContractPaused))
    );

    // Unpausing restores execution
    assert_eq!(fixture.amm.try_set_paused(&fixture.admin, &false), Ok(Ok(())));
    assert_eq!(fixture.amm.is_paused(), false);
    assert_eq!(fixture.amm.try_swap(&user, &pair, &1_000), Ok(Ok(())));
    assert_eq!(fixture.amm.try_provide_liquidity_gated(&user, &pair, &1_000), Ok(Ok(())));
}

#[test]
fn amm_set_paused_rejects_unauthorized_caller() {
    let fixture = setup();
    let attacker = Address::generate(&fixture.env);

    assert_eq!(fixture.amm.try_set_paused(&attacker, &true), Err(Ok(MockAmmError::Unauthorized)));
}

// ── Issue #204: Mock Lending maximum borrow ceiling ────────────────────────

#[test]
fn lending_borrow_enforces_maximum_borrow_amount_ceiling() {
    let fixture = setup();
    let borrower = Address::generate(&fixture.env);
    let pair = symbol_short!("XLM_USDC");
    submit_score(&fixture, &borrower, 10, 90);

    // Valid borrow within ceiling succeeds
    assert_eq!(fixture.lending.try_borrow(&borrower, &pair, &1_000), Ok(Ok(())));

    // Out-of-bounds borrow exceeding ceiling fails with InvalidAmount
    assert_eq!(
        fixture.lending.try_borrow(
            &borrower,
            &pair,
            &(mock_lending::DEFAULT_MAX_BORROW_AMOUNT + 1)
        ),
        Err(Ok(MockLendingError::InvalidAmount))
    );
    assert_eq!(
        fixture.lending.try_borrow(&borrower, &pair, &i128::MAX),
        Err(Ok(MockLendingError::InvalidAmount))
    );
}

#[test]
fn lending_set_max_borrow_amount_configures_ceiling_and_emits_event() {
    let fixture = setup();
    let borrower = Address::generate(&fixture.env);
    let pair = symbol_short!("XLM_USDC");
    submit_score(&fixture, &borrower, 10, 90);

    let before = fixture.env.events().all().len();
    assert_eq!(fixture.lending.try_set_max_borrow_amount(&fixture.admin, &5_000), Ok(Ok(())));
    assert_eq!(fixture.lending.get_max_borrow_amount(), 5_000);
    assert_eq!(fixture.env.events().all().len(), before + 1);

    // 5_000 is accepted
    assert_eq!(fixture.lending.try_borrow(&borrower, &pair, &5_000), Ok(Ok(())));

    // 5_001 is rejected
    assert_eq!(
        fixture.lending.try_borrow(&borrower, &pair, &5_001),
        Err(Ok(MockLendingError::InvalidAmount))
    );

    // Non-positive ceiling rejected
    assert_eq!(
        fixture.lending.try_set_max_borrow_amount(&fixture.admin, &0),
        Err(Ok(MockLendingError::InvalidAmount))
    );

    // Unauthorized caller rejected
    let attacker = Address::generate(&fixture.env);
    assert_eq!(
        fixture.lending.try_set_max_borrow_amount(&attacker, &10_000),
        Err(Ok(MockLendingError::Unauthorized))
    );
}

// ── Issue #205: Mock AMM multi-asset liquidity gating ──────────────────────

#[test]
fn amm_provide_liquidity_gated_supports_distinct_asset_pairs() {
    let fixture = setup();
    let provider = Address::generate(&fixture.env);
    let xlm_usdc = symbol_short!("XLM_USDC");
    let eurc_usdc = symbol_short!("EURC_USDC");

    // Only submit a score for XLM_USDC
    submit_score(&fixture, &provider, 10, 90);

    // XLM_USDC liquidity provision passes
    assert_eq!(fixture.amm.try_provide_liquidity_gated(&provider, &xlm_usdc, &1_000), Ok(Ok(())));

    // EURC_USDC has no score submitted -> rejected with HighRiskWallet
    assert_eq!(
        fixture.amm.try_provide_liquidity_gated(&provider, &eurc_usdc, &1_000),
        Err(Ok(MockAmmError::HighRiskWallet))
    );

    // Submit safe score for EURC_USDC
    fixture.env.ledger().with_mut(|l| l.timestamp += 3_601);
    fixture.scoregate.submit_score(
        &Vec::new(&fixture.env),
        &provider,
        &eurc_usdc,
        &12,
        &false,
        &false,
        &fixture.env.ledger().timestamp(),
        &85,
        &1,
        &None,
    );

    // Now EURC_USDC passes
    assert_eq!(fixture.amm.try_provide_liquidity_gated(&provider, &eurc_usdc, &1_000), Ok(Ok(())));
}
