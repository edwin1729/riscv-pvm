use alloy_primitives::Address;
use alloy_primitives::aliases::{B256, U256};

pub type EthState = Vec<(Address, EthAccount)>;
pub struct EthAccount {
    pub nonce: u64,
    pub balance: U256,
    pub storage_root: B256,
    pub storage: Vec<(B256, U256)>,
    pub code_hash: B256,
}
