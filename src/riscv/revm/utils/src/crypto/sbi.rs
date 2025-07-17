// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

// TODO: RV-121: We want to access the crypto functions through the Tezos crypto crate instead of
// needing to define them here.

use super::*;
use alloy_rlp::Encodable;
use alloy_trie::HashBuilder;
use alloy_trie::TrieAccount;
use alloy_trie::root::{state_root_unhashed, storage_root_unhashed};
use revm::database::in_memory_db::Cache;
use revm::primitives::B256;
use revm::primitives::U256;
use root_hash::{EthAccount, EthState};
use tezos_smart_rollup_constants::riscv::SBI_FIRMWARE_TEZOS;

// TODO: RV-691: Move constant to kernel_sdk
/// Function ID for `sbi_tezos_secp256k1_verify`
pub const SBI_TEZOS_SECP256K1_VERIFY: u64 = 0x0a;

// TODO: RV-691: Move constant to kernel_sdk
/// Function ID for `sbi_tezos_keccak_hash256`
pub const SBI_TEZOS_KECCAK256_HASH: u64 = 0x0b;

// TODO: RV-691: Move constant to kernel_sdk
/// Function ID for `sbi_tezos_eth_state_root`
pub const SBI_TEZOS_ETH_STATE_ROOT: u64 = 0x0c;

// TODO: RV-691: Move constant to kernel_sdk
/// Function ID for `sbi_tezos_eth_storage_root`
pub const SBI_TEZOS_ETH_STORAGE_ROOT: u64 = 0x0d;

// TODO: RV-691: Move constant to kernel_sdk
/// Function ID for `sbi_tezos_eth_state_root`
pub const SBI_TEZOS_ETH_INC_HASH_BUILD: u64 = 0x0e;
//
// TODO: RV-691: Move constant to kernel_sdk
/// Function ID for `sbi_tezos_eth_state_root`
pub const SBI_TEZOS_ETH_FIN_HASH_BUILD: u64 = 0x0f;

// TODO: RV-691: Move constant to kernel_sdk
/// Maximum size of pvm memory access by a host function in bytes
/// To limit size of proofs in refutation games
pub const MAX_PVM_MEMORY_ACCESS: usize = 4096;

impl SignedOperation {
    // Secp256k1 verification using a system call
    pub fn host_verify(self) -> Option<Operation> {
        let result: isize;

        let pk = self.pk.serialize();
        let sig = self.signature.serialize();
        let msg = Self::host_message_from_op(&self.inner).serialize();
        unsafe {
            core::arch::asm!(
                "ecall",
                in("a6") SBI_TEZOS_SECP256K1_VERIFY,
                in("a7") SBI_FIRMWARE_TEZOS,
                in("a0") pk.as_ptr(),
                in("a1") sig.as_ptr(),
                in("a2") msg.as_ptr(),
                lateout("a0") result,
            );
        }

        (result == 1).then_some(self.inner)
    }

    // Keccak-256 hashing using a system call
    fn host_message_from_op(op: &Operation) -> Message {
        let bytes: Vec<u8> = (op).into();

        // Use the system call to hash only if the message is not too large
        // to make the proofs too large
        if bytes.len() <= MAX_PVM_MEMORY_ACCESS {
            let mut hash = [0u8; 32];
            let result: isize;

            unsafe {
                core::arch::asm!(
                    "ecall",
                    in("a6") SBI_TEZOS_KECCAK256_HASH,
                    in("a7") SBI_FIRMWARE_TEZOS,
                    in("a0") hash.as_mut_ptr(),
                    in("a1") bytes.as_ptr(),
                    in("a2") bytes.len(),
                    lateout("a0") result,
                );
            }
            assert_eq!(
                result, 32,
                "SBI_TEZOS_KECCAK_HASH256 call returned unexpected value: {result}"
            );
            Message::parse(&hash)
        } else {
            // emulated hashing
            Self::message_from_op(op)
        }
    }
}

/// Does not check whether the byte array is too large
fn keccak256(bytes: &[u8]) -> [u8; 32] {
    let mut hash = [0u8; 32];
    let result: isize;

    unsafe {
        core::arch::asm!(
            "ecall",
            in("a6") SBI_TEZOS_KECCAK256_HASH,
            in("a7") SBI_FIRMWARE_TEZOS,
            in("a0") hash.as_mut_ptr(),
            in("a1") bytes.as_ptr(),
            in("a2") bytes.len(),
            lateout("a0") result,
        );
    }
    assert_eq!(
        result, 32,
        "SBI_TEZOS_KECCAK_HASH256 call returned unexpected value: {result}"
    );
    hash
}

#[inline(always)]
fn inc_hash_build(hashed_slot: &B256, value_rlp: &[u8]) {
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a6") SBI_TEZOS_ETH_INC_HASH_BUILD,
            in("a7") SBI_FIRMWARE_TEZOS,
            in("a1") hashed_slot.as_ptr(),
            in("a2") value_rlp.as_ptr(),
            in("a3") value_rlp.len(),
        );
    }
}

#[inline(always)]
fn fin_hash_build() -> [u8; 32] {
    let mut hash = [0u8; 32];
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a6") SBI_TEZOS_ETH_FIN_HASH_BUILD,
            in("a7") SBI_FIRMWARE_TEZOS,
            in("a0") hash.as_mut_ptr(),
        );
    }
    hash
}

pub fn calculate_state_root(db: &Cache) -> B256 {
    let mut state: Vec<(B256, TrieAccount)> = db
        .accounts
        .iter()
        .map(|(address, account)| {
            // k (key) converted from U256 to B256
            let mut storage: Vec<(B256, U256)> = account
                .storage
                .iter()
                .map(|(k, v)| (B256::new(keccak256(k.to_be_bytes::<32>().as_ref())), *v))
                .collect();
            storage.sort_unstable_by_key(|(key, _)| *key);

            // storage is now in the form of a linearized tree which we can incrementally hash

            for (hashed_slot, value) in storage {
                let value_rlp: Vec<u8> = alloy_rlp::encode_fixed_size(&value).to_vec();
                inc_hash_build(&hashed_slot, value_rlp.as_ref());
            }

            let hash = fin_hash_build();
            (
                B256::new(keccak256(address.as_ref())),
                TrieAccount {
                    nonce: account.info.nonce,
                    balance: account.info.balance,
                    storage_root: hash.into(),
                    code_hash: account.info.code_hash,
                },
            )
        })
        .collect();
    state.sort_unstable_by_key(|(key, _)| *key);
    let mut account_rlp_buf = Vec::new();
    for (hashed_key, account) in state {
        account_rlp_buf.clear();

        account.encode(&mut account_rlp_buf);

        inc_hash_build(&hashed_key, account_rlp_buf.as_ref());
    }

    fin_hash_build().into()
}
