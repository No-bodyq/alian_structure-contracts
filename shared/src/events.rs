use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol};

/// Legacy single-topic constants retained for backward compatibility.
pub const AID_CREATED: Symbol = symbol_short!("aid_crt");
pub const AID_CLAIMED: Symbol = symbol_short!("aid_clm");
pub const AID_SETTLED: Symbol = symbol_short!("aid_stl");
pub const AID_REFUNDED: Symbol = symbol_short!("aid_ref");
pub const COMMISSION_PAID: Symbol = symbol_short!("com_paid");
pub const REFERRAL_ACCRUED: Symbol = symbol_short!("ref_acc");
pub const REFERRER_SET: Symbol = symbol_short!("ref_set");
pub const TIER_CONFIG_SET: Symbol = symbol_short!("tier_cfg");
pub const TREASURY_SET: Symbol = symbol_short!("trs_set");
pub const TREASURY_DEPOSIT: Symbol = symbol_short!("t_dep");
pub const TREASURY_WITHDRAW: Symbol = symbol_short!("t_wdw");
pub const TREASURY_EMERGENCY_WITHDRAW: Symbol = symbol_short!("t_emrg");
pub const PARAMETER_CHANGED: Symbol = symbol_short!("param_chg");
pub const CONTRACT_PAUSED: Symbol = symbol_short!("paused");
pub const CONTRACT_RESUMED: Symbol = symbol_short!("resumed");
pub const CONTRACT_UPGRADED: Symbol = symbol_short!("upgraded");

/// Emits `AidCreated`.
///
/// Topics: `("aid", "created")`
///
/// Data:
/// `(aid_id, donor, recipient, amount, created_at, expires_at)`
pub fn emit_aid_created(
    env: &Env,
    aid_id: u64,
    donor: &Address,
    recipient: &Address,
    amount: i128,
    created_at: u64,
    expires_at: u64,
) {
    env.events().publish(
        (symbol_short!("aid"), symbol_short!("created")),
        (
            aid_id,
            donor.clone(),
            recipient.clone(),
            amount,
            created_at,
            expires_at,
        ),
    );
}

/// Emits `AidClaimed`.
///
/// Topics: `("aid", "claimed")`
///
/// Data: `(aid_id, claimant, claimed_at)`
pub fn emit_aid_claimed(env: &Env, aid_id: u64, claimant: &Address, claimed_at: u64) {
    env.events().publish(
        (symbol_short!("aid"), symbol_short!("claimed")),
        (aid_id, claimant.clone(), claimed_at),
    );
}

/// Emits `AidSettled`.
///
/// Topics: `("aid", "settled")`
///
/// Data: `(aid_id, recipient, amount, settled_at)`
pub fn emit_aid_settled(
    env: &Env,
    aid_id: u64,
    recipient: &Address,
    amount: i128,
    settled_at: u64,
) {
    env.events().publish(
        (symbol_short!("aid"), symbol_short!("settled")),
        (aid_id, recipient.clone(), amount, settled_at),
    );
}

/// Emits `AidRefunded`.
///
/// Topics: `("aid", "refunded")`
///
/// Data: `(aid_id, donor, amount, refunded_at)`
pub fn emit_aid_refunded(env: &Env, aid_id: u64, donor: &Address, amount: i128, refunded_at: u64) {
    env.events().publish(
        (symbol_short!("aid"), symbol_short!("refunded")),
        (aid_id, donor.clone(), amount, refunded_at),
    );
}

/// Emits `CommissionPaid`.
///
/// Topics: `("comm", "paid")`
///
/// Data: `(recipient, amount, paid_at)`
pub fn emit_commission_paid(env: &Env, recipient: &Address, amount: i128, paid_at: u64) {
    env.events().publish(
        (symbol_short!("comm"), symbol_short!("paid")),
        (recipient.clone(), amount, paid_at),
    );
}

/// Emits `TreasuryDeposit`.
///
/// Topics: `("treasury", "deposit")`
///
/// Data: `(category, depositor, amount, new_balance)`
pub fn emit_treasury_deposit(
    env: &Env,
    category: Symbol,
    depositor: &Address,
    amount: i128,
    new_balance: i128,
) {
    env.events().publish(
        (symbol_short!("treasury"), symbol_short!("deposit")),
        (category, depositor.clone(), amount, new_balance),
    );
}

/// Emits `TreasuryWithdrawal`.
///
/// Topics: `("treasury", "withdraw")`
///
/// Data: `(category, recipient, amount, remaining_balance)`
pub fn emit_treasury_withdrawal(
    env: &Env,
    category: Symbol,
    recipient: &Address,
    amount: i128,
    remaining_balance: i128,
) {
    env.events().publish(
        (symbol_short!("treasury"), symbol_short!("withdraw")),
        (category, recipient.clone(), amount, remaining_balance),
    );
}

/// Emits `ContractPaused`.
///
/// Topics: `("contract", "paused")`
///
/// Data: `(actor, paused_at)`
pub fn emit_contract_paused(env: &Env, actor: &Address, paused_at: u64) {
    env.events().publish(
        (symbol_short!("contract"), symbol_short!("paused")),
        (actor.clone(), paused_at),
    );
}

/// Emits `ContractResumed`.
///
/// Topics: `("contract", "resumed")`
///
/// Data: `(actor, resumed_at)`
pub fn emit_contract_resumed(env: &Env, actor: &Address, resumed_at: u64) {
    env.events().publish(
        (symbol_short!("contract"), symbol_short!("resumed")),
        (actor.clone(), resumed_at),
    );
}

/// Emits `ContractUpgraded`.
///
/// Topics: `("contract", "upgraded")`
///
/// Data: `(actor, wasm_hash, upgraded_at)`
pub fn emit_contract_upgraded(
    env: &Env,
    actor: &Address,
    wasm_hash: &BytesN<32>,
    upgraded_at: u64,
) {
    env.events().publish(
        (symbol_short!("contract"), symbol_short!("upgraded")),
        (actor.clone(), wasm_hash.clone(), upgraded_at),
    );
}

/// Emits an event using a legacy single-symbol topic.
///
/// New protocol events should use one of the typed helpers above.
pub fn emit<T: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(env: &Env, topic: Symbol, data: T) {
    env.events().publish((topic,), data);
}

#[cfg(test)]
mod tests {
    use super::emit_aid_created;
    use soroban_sdk::{
        contract, contractimpl, symbol_short,
        testutils::{Address as _, Events},
        Address, Env, FromVal, IntoVal,
    };

    #[contract]
    struct EventTestContract;

    #[contractimpl]
    impl EventTestContract {
        pub fn publish_aid_created(
            env: Env,
            aid_id: u64,
            donor: Address,
            recipient: Address,
            amount: i128,
            created_at: u64,
            expires_at: u64,
        ) {
            emit_aid_created(
                &env, aid_id, &donor, &recipient, amount, created_at, expires_at,
            );
        }
    }

    #[test]
    fn aid_created_has_stable_topics_and_data() {
        let env = Env::default();
        let donor = Address::generate(&env);
        let recipient = Address::generate(&env);
        let contract_id = env.register(EventTestContract, ());
        let client = EventTestContractClient::new(&env, &contract_id);

        client.publish_aid_created(&7, &donor, &recipient, &500, &100, &1_000);

        let events = env.events().all();
        assert_eq!(events.len(), 1);

        let (emitter, topics, data) = events.get(0).unwrap();

        assert_eq!(emitter, contract_id);
        assert_eq!(
            topics,
            (symbol_short!("aid"), symbol_short!("created"),).into_val(&env)
        );
        let decoded_data: (u64, Address, Address, i128, u64, u64) =
            FromVal::from_val(&env, &data);

        assert_eq!(
            decoded_data,
            (7u64, donor, recipient, 500i128, 100u64, 1_000u64)
        );
    }
}
