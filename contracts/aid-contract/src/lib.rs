use soroban_sdk::{
    contract, contractimpl, panic_with_error, token, Address, Env, Symbol, Map, BytesN,
};
use shared::{emit_aid_created, emit, AID_CREATED, Error, auth};

/// Storage keys
const KEY_TOKEN: Symbol = Symbol::new("token");
const KEY_AID_COUNTER: Symbol = Symbol::new("aid_cnt");
const KEY_AIDS: Symbol = Symbol::new("aids");

/// Aid status enum
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
#[repr(u32)]
pub enum AidStatus {
    Created = 0,
    Claimed = 1,
    Settled = 2,
    Refunded = 3,
}

use shared::{emit, AID_CLAIMED, AID_CREATED, AID_REFUNDED, AID_SETTLED};
use shared::storage::{is_paused, set_paused as shared_set_paused};

pub mod storage;
pub mod types;

use storage::{get_aid, get_aid_counter, has_aid, set_aid, set_aid_counter};

// Re-export so test modules (and `use super::*`) have access.
pub use types::{AidRecord, AidStatus};

// ---------------------------------------------------------------------------
// Contract-specific error codes  (range 100-199 per shared/README.md)
// ---------------------------------------------------------------------------

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AidError {
    /// Caller is not the authorised recipient.
    Unauthorized = 100,
    /// The requested aid record was not found.
    NotFound = 101,
    /// The aid has already been settled or refunded.
    AlreadyClaimed = 102,
    /// The claim window has expired (past `expiry_ledger`).
    Expired = 103,
    /// The contract is paused; no state-changing operations are allowed.
    Paused = 104,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct AidContract;

#[contractimpl]
impl AidContract {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialise the contract, storing the admin address.
    ///
    /// Must be called exactly once immediately after deployment.
    pub fn initialize(env: Env, admin: Address) {
        shared::auth::set_admin(&env, &admin);
    }

    // -----------------------------------------------------------------------
    // Aid creation
    // -----------------------------------------------------------------------

    /// Create a new aid disbursement and escrow funds from the donor.
    ///
    /// Transfers `amount` of `token` from `donor` to this contract for
    /// safekeeping until the recipient claims or the aid expires.
    ///
    /// Returns the newly allocated `aid_id`.
    pub fn create_aid(
        env: Env,
        aid_id: u64,
        donor: Address,
        recipient: Address,
        token: Address,
        amount: i128,
        expiry_ledger: u32,
    ) -> u64 {
        donor.require_auth();

        if amount <= 0 {
            env.panic_with_error(shared::Error::InvalidAmount);
        }
        if expiry_ledger <= env.ledger().sequence() {
            env.panic_with_error(shared::Error::InvalidArgument);
        }
        if has_aid(&env, aid_id) {
            env.panic_with_error(shared::Error::InvalidArgument);
        }

        // Escrow funds from donor into contract.
        token::Client::new(&env, &token).transfer(
            &donor,
            &env.current_contract_address(),
            &amount,
        );

        let record = AidRecord {
            id: aid_id,
            donor: donor.clone(),
            recipient: recipient.clone(),
            token: token.clone(),
            amount,
            expiry_ledger,
            status: AidStatus::Pending,
        };
        set_aid(&env, aid_id, &record);

        // Store the aid record in a persistent map of aid_id -> AidRecord
        let mut aids: Map<u64, AidRecord> = env.storage()
            .persistent()
            .get(&KEY_AIDS)
            .unwrap_or_else(|| Map::new(&env));
        aids.insert(aid_id, aid_record);
        env.storage().persistent().set(&KEY_AIDS, &aids);

        // Emit the AidCreated event
        emit_aid_created(
            &env,
            aid_id,
            &donor,
            &recipient,
            amount,
            current_time,
            expiry,
        );

        emit(&env, AID_CREATED, (aid_id, donor, recipient, amount, expiry_ledger));
        aid_id
    }

    // -----------------------------------------------------------------------
    // Aid claiming
    // -----------------------------------------------------------------------

    /// Claim a pending aid disbursement and transfer funds to the recipient.
    ///
    /// # Errors (via `env.panic_with_error`)
    /// - [`AidError::Paused`]         — contract is paused.
    /// - [`AidError::NotFound`]       — `aid_id` does not exist.
    /// - [`AidError::Expired`]        — `expiry_ledger` has passed.
    /// - [`AidError::AlreadyClaimed`] — status is not `Pending`.
    /// - [`AidError::Unauthorized`]   — `caller` is not the recipient.
    pub fn claim_aid(env: Env, aid_id: u64, caller: Address) -> Result<(), AidError> {
        if is_paused(&env) {
            return Err(AidError::Paused);
        }
        caller.require_auth();

        let mut record = get_aid(&env, aid_id).ok_or(AidError::NotFound)?;

        if env.ledger().sequence() > record.expiry_ledger {
            return Err(AidError::Expired);
        }
        if record.status != AidStatus::Pending {
            return Err(AidError::AlreadyClaimed);
        }
        if caller != record.recipient {
            return Err(AidError::Unauthorized);
        }

        // Checks-effects-interactions: write status first, then transfer.
        record.status = AidStatus::Settled;
        set_aid(&env, aid_id, &record);

        token::Client::new(&env, &record.token).transfer(
            &env.current_contract_address(),
            &record.recipient,
            &record.amount,
        );

        emit(&env, AID_CLAIMED, aid_id);
        emit(&env, AID_SETTLED, aid_id);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Refunds
    // -----------------------------------------------------------------------

    /// Refund an expired, unclaimed aid disbursement to the original donor.
    ///
    /// Anyone may call this after expiry to trigger a refund; it is not
    /// gated to the admin so expired funds cannot be held hostage.
    ///
    /// # Errors (via `env.panic_with_error`)
    /// - [`AidError::NotFound`]       — `aid_id` does not exist.
    /// - [`AidError::AlreadyClaimed`] — already settled or refunded.
    /// - [`shared::Error::InvalidArgument`] — expiry has not yet passed.
    pub fn refund_expired(env: Env, aid_id: u64) -> Result<(), AidError> {
        let mut record = get_aid(&env, aid_id).ok_or(AidError::NotFound)?;

        if record.status != AidStatus::Pending {
            return Err(AidError::AlreadyClaimed);
        }
        if env.ledger().sequence() <= record.expiry_ledger {
            env.panic_with_error(shared::Error::InvalidArgument);
        }

        // Checks-effects-interactions.
        record.status = AidStatus::Refunded;
        set_aid(&env, aid_id, &record);

        token::Client::new(&env, &record.token).transfer(
            &env.current_contract_address(),
            &record.donor,
            &record.amount,
        );

        emit(&env, AID_REFUNDED, aid_id);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Return the aid record for `aid_id`, or `None` if it does not exist.
    pub fn get_aid(env: Env, aid_id: u64) -> Option<AidRecord> {
        storage::get_aid(&env, aid_id)
    }

    /// Refund an expired and unclaimed aid to the donor.
    pub fn refund_aid(env: Env, aid_id: u64) {
        // Ensure the caller is authorized (donor or admin)
        let aid = Self::get_aid(env.clone(), aid_id);
        if aid.donor != auth::get_invoker_address(&env) && !auth::is_admin(&env) {
            panic_with_error!(env, Error::Unauthorized);
        }

        // Verify that the aid has expired
        if env.ledger().timestamp() < aid.expiry {
            panic_with_error!(env, Error::NotExpiredYet);
        }

        // Verify that the aid is in a refundable state (Created)
        if aid.status != AidStatus::Created as u32 {
            panic_with_error!(env, Error::AlreadyRefunded);
        }

        // Update the aid status to Refunded
        let mut updated_aid = aid.clone();
        updated_aid.status = AidStatus::Refunded as u32;

        // Transfer the funds back to the donor
        let token = Self::get_token(env.clone());
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(
            &env.current_contract_address(),
            &aid.donor,
            &aid.amount,
        );

        // Update the aid record in storage
        let mut aids: Map<u64, AidRecord> = env
            .storage()
            .persistent()
            .get(&KEY_AIDS)
            .expect("no aid records found");
        aids.insert(aid_id, updated_aid);
        env.storage().persistent().set(&KEY_AIDS, &aids);

        // Emit the AidRefunded event
        emit(
            &env,
            Symbol::new("aid_refunded"),
            (aid_id, aid.donor, aid.amount),
        );
    }

    /// Claim an aid, transferring the escrowed funds to the recipient.
    pub fn claim_aid(env: Env, aid_id: u64, recipient: Address) {
        // Ensure the caller is the intended recipient
        recipient.require_auth();

        // Check if the contract is paused
        if env.storage().instance().get(&Symbol::new("paused")).unwrap_or(false) {
            panic_with_error!(env, Error::Paused);
        }

        let mut aid = Self::get_aid(env.clone(), aid_id);

        // Verify that the caller is the recipient
        if aid.recipient != recipient {
            panic_with_error!(env, Error::Unauthorized);
        }

        // Verify that the aid has not expired
        if env.ledger().timestamp() >= aid.expiry {
            panic_with_error!(env, Error::Expired);
        }

        // Verify that the aid is in a claimable state
        if aid.status != AidStatus::Created as u32 {
            panic_with_error!(env, Error::AlreadyClaimed);
        }

        // Update the aid status to Claimed
        aid.status = AidStatus::Claimed as u32;

        // Transfer the funds to the recipient
        let token = Self::get_token(env.clone());
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(
            &env.current_contract_address(),
            &recipient,
            &aid.amount,
        );

        // Update the aid status to Settled
        aid.status = AidStatus::Settled as u32;

        // Update the aid record in storage
        let mut aids: Map<u64, AidRecord> = env
            .storage()
            .persistent()
            .get(&KEY_AIDS)
            .expect("no aid records found");
        aids.insert(aid_id, aid.clone());
        env.storage().persistent().set(&KEY_AIDS, &aids);

        // Emit the AidClaimed and AidSettled events
        emit(
            &env,
            Symbol::new("aid_claimed"),
            (aid_id, recipient.clone()),
        );
        emit(
            &env,
            Symbol::new("aid_settled"),
            (aid_id, recipient, aid.amount),
        );
    }

    /// Set the paused state of the contract.
    pub fn set_paused(env: Env, admin: Address, paused: bool) {
        let contract_admin = shared::auth::get_admin(&env);
        if admin != contract_admin {
             panic_with_error!(env, Error::Unauthorized);
        }
        admin.require_auth();

        env.storage().instance().set(&Symbol::new("paused"), &paused);
    }
}
    // -----------------------------------------------------------------------
    // Admin controls
    // -----------------------------------------------------------------------

    /// Pause or resume the contract.  Admin only.
    pub fn set_paused(env: Env, caller: Address, paused: bool) {
        shared::auth::require_admin(&env, &caller).expect("unauthorized");
        shared_set_paused(&env, paused);
    }
}

#[cfg(test)]
mod tests;
