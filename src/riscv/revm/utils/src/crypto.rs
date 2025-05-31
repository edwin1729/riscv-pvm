// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

use libsecp256k1::{Error, Message, Signature, sign, verify};
pub use libsecp256k1::{PublicKey, SecretKey, curve::Scalar};
use revm::context::TxEnv;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};

/// All the data for an evm operation is encoded in the `TxEnv` struct.
/// The wrapper is to add a hash method.
#[derive(Debug, Serialize, Deserialize)]
pub struct Operation(pub TxEnv);

impl From<&Operation> for Vec<u8> {
    fn from(op: &Operation) -> Self {
        bincode::serde::encode_to_vec(&op.0, bincode::config::standard()).unwrap()
    }
}

/// The Operation along with it's `signature` and `address` for verification
#[derive(Serialize, Deserialize)]
pub struct SignedOperation {
    // public key
    pub pk: PublicKey,
    #[serde(with = "serde_sig")]
    signature: Signature,
    inner: Operation,
}

mod serde_sig {
    use super::*;
    use serde::de::Error as DeError;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S>(sig: &Signature, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bytes = sig.serialize();
        s.serialize_bytes(bytes.as_slice())
    }
    pub fn deserialize<'de, D>(d: D) -> Result<Signature, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Vec<u8> = Deserialize::deserialize(d)?;
        let exact_bytes: &[u8; 64] = bytes.as_slice().try_into().map_err(DeError::custom)?;
        Signature::parse_standard(exact_bytes).map_err(DeError::custom)
    }
}

impl SignedOperation {
    /// Create a `SignedOperation`
    pub fn sign(pk: PublicKey, sk: SecretKey, inner: Operation) -> Self {
        let signature = sign(&Self::message_from_op(&inner), &sk).0;
        Self {
            pk,
            signature,
            inner,
        }
    }
    fn message_from_op(op: &Operation) -> Message {
        let bytes: Vec<u8> = (op).into();
        let hash: [u8; 32] = Keccak256::digest(bytes).into();
        Message::parse(&hash)
    }

    /// return the payload if the signature is valid
    pub fn verify(self) -> Option<Operation> {
        verify(
            &Self::message_from_op(&self.inner),
            &self.signature,
            &self.pk,
        )
        .then_some(self.inner)
    }
}

pub fn address_from_pk(pk: &PublicKey) -> [u8; 20] {
    // Drop the initial byte which is a tag and hash
    let hash: [u8; 32] = Keccak256::digest(&pk.serialize()[1..]).into();

    let mut addr = [0u8; 20];
    addr.copy_from_slice(&hash[12..]);
    addr
}

pub fn keypair_from_int(n: u32) -> Result<(SecretKey, PublicKey), Error> {
    let sk = SecretKey::try_from(Scalar::from_int(n))?;
    let pk = PublicKey::from_secret_key(&sk);
    Ok((sk, pk))
}
