use soroban_sdk::{
    contract, contractimpl, panic_with_error, token, Address, Env, Symbol, Map, BytesN,
};
use shared::{emit, AID_CREATED, Error, auth};

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

impl From<u32> for AidStatus {
    fn from(value: u32) -> Self {
        match value {
            0 => AidStatus::Created,
            1 => AidStatus::Claimed,
            2 => AidStatus::Settled,
            3 => AidStatus::Refunded,
            _ => panic!("invalid aid status"),
        }
    }
}

/// AidRecord structure that stores all aid information
#[soroban_sdk::contracttype]
#[derive(Debug, Clone)]
pub struct AidRecord {
    pub id: u64,
    pub donor: Address,
    pub recipient: Address,
    pub amount: i128,
    pub status: u32,
    pub timestamp: u64,
    pub expiry: u64,
}

#[contract]
pub struct AidContract;

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AidClaimMetadata {
    pub claim_hash: Option<BytesN<32>>,
    pub max_claims: u32,
    pub claims_used: u32,
    pub expires_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
enum DataKey {
    Aid(u64),
    ClaimNonce(u64, BytesN<32>),
}

#[contractimpl]
impl AidContract {
    /// Initialise the contract, setting the admin address and token address.
    pub fn initialize(env: Env, admin: Address, token: Address) {
        shared::auth::set_admin(&env, &admin);
        env.storage().instance().set(&KEY_TOKEN, &token);
        // Initialize aid counter to 0
        env.storage().instance().set(&KEY_AID_COUNTER, &0u64);
    }

    /// Create a new aid record, escrowing funds from the donor.
    pub fn create_aid(
        env: Env,
        donor: Address,
        recipient: Address,
        amount: i128,
        expiry: u64,
    ) -> u64 {
        // Verify the donor is the caller and has authorized this action
        donor.require_auth();

        // Validate amount > 0
        if amount <= 0 {
            panic_with_error!(env, Error::InvalidArgument);
        }

        // Validate expiry is in the future
        let current_time = env.ledger().timestamp();
        if expiry <= current_time {
            panic_with_error!(env, Error::InvalidArgument);
        }

        // Get the token address
        let token = env.storage()
            .instance()
            .get::<Symbol, Address>(&KEY_TOKEN)
            .expect("token not initialized");

        // Transfer the amount from donor to this contract (escrow)
        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&donor, &env.current_contract_address(), &amount);

        // Get the current counter and increment it to generate unique aid_id
        let mut current_counter = env.storage()
            .instance()
            .get::<Symbol, u64>(&KEY_AID_COUNTER)
            .unwrap_or(0);
        current_counter += 1;
        let aid_id = current_counter;
        env.storage().instance().set(&KEY_AID_COUNTER, &current_counter);

        // Create and store the AidRecord
        let aid_record = AidRecord {
            id: aid_id,
            donor: donor.clone(),
            recipient: recipient.clone(),
            amount,
            status: AidStatus::Created as u32,
            timestamp: current_time,
            expiry,
        };

        // Store the aid record in a persistent map of aid_id -> AidRecord
        let mut aids: Map<u64, AidRecord> = env.storage()
            .persistent()
            .get(&KEY_AIDS)
            .unwrap_or_else(|| Map::new(&env));
        aids.insert(aid_id, aid_record);
        env.storage().persistent().set(&KEY_AIDS, &aids);

        // Emit the AidCreated event
        emit(&env, AID_CREATED, (aid_id, donor, recipient, amount, current_time, expiry));

        aid_id
    }

    /// Helper function to get an aid record by ID (useful for testing and other functions)
    pub fn get_aid(env: Env, aid_id: u64) -> AidRecord {
        let aids: Map<u64, AidRecord> = env.storage()
            .persistent()
            .get(&KEY_AIDS)
            .expect("no aid records found");
        aids.get(aid_id).expect("aid record not found")
    }

    /// Get the token address used by the contract
    pub fn get_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<Symbol, Address>(&KEY_TOKEN)
            .expect("token not initialized")
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