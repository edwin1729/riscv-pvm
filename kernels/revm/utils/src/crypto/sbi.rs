// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

// TODO: RV-121: We want to access the crypto functions through the Tezos crypto crate instead of
// needing to define them here.

use super::*;
use tezos_smart_rollup_constants::riscv::SBI_FIRMWARE_TEZOS;

// TODO: RV-691: Move constant to kernel_sdk
/// Function ID for `sbi_tezos_secp256k1_verify`
pub const SBI_TEZOS_SECP256K1_VERIFY: u64 = 0x0a;
pub const SBI_TEZOS_SECP256K1_BULK_VERIFY: u64 = 0x0d;

// TODO: RV-691: Move constant to kernel_sdk
/// Function ID for `sbi_tezos_keccak_hash256`
pub const SBI_TEZOS_KECCAK256_HASH: u64 = 0x0b;

// TODO: RV-691: Move constant to kernel_sdk
/// Maximum size of pvm memory access by a host function in bytes
/// To limit size of proofs in refutation games
pub const MAX_PVM_MEMORY_ACCESS: usize = 4096;

pub fn batch_verify(txs: &[SignedOperation]) -> Vec<bool> {
    let mut pks = Vec::new();
    let mut sigs = Vec::new();
    let mut msg_hashes = Vec::new();
    for op in txs {
        pks.push(op.pk.serialize());
        sigs.push(op.signature.serialize());
        msg_hashes.push(SignedOperation::host_message_from_op(&op.inner).serialize());
    }
    let mut result = [false; 256]; // TODO magic number hack fix this
    let mut a0 = pks.len();
    let mut a1 = pks.as_mut_ptr();
    let mut a2 = sigs.as_mut_ptr();
    let mut a3 = msg_hashes.as_mut_ptr();
    let mut a4 = result.as_mut_ptr();
    unsafe {
        // TODO more specific assembly annotation to declare I don't need the final values of a1..4
        core::arch::asm!(
            "ecall",
            in("a6") SBI_TEZOS_SECP256K1_BULK_VERIFY,
            in("a7") SBI_FIRMWARE_TEZOS,
            inout("a0") a0,
            inout("a1") a1,
            inout("a2") a2,
            inout("a3") a3,
            inout("a4") a4,
        );
    }

    result[..pks.len()].to_vec()
}

impl SignedOperation {
    // Secp256k1 verification using a system call
    pub fn verify(&self) -> bool {
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

        result == 1
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
