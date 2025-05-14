// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

use jstz_crypto::public_key::PublicKey;
use jstz_crypto::signature::Signature;
use revm::context::TxEnv;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tezos_crypto_rs::blake2b::digest_256;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// All the data for an evm operation is encoded in the `TxEnv` struct.
/// The wrapper is to add a hash method.
#[derive(Debug, Serialize, Deserialize)]
pub struct Operation(pub TxEnv);

impl Operation {
    /// Hash an `Operation`
    pub fn hash(&self) -> Result<Vec<u8>> {
        let bytes = bincode::serde::encode_to_vec(self.0.clone(), bincode::config::standard())?;
        Ok(digest_256(bytes.as_slice()))
    }
}
/// The Operation along with it's `signature` and `public_key` for verification
/// * Serializing this data structure uses both the serde and native backends of bincode
/// * The reason being `TxEnv` from revm does implmenent Encode and Decode. (The developers of revm
///   weren't keen on supporting this but if `TxEnv` serialization is actually a good idea a case
///   could be made)
/// * And when trying to use serde on PublicKey and Signature gives "Serde(AnyNotSupported)" (not
///   sure why)
#[derive(Serialize, Deserialize)]
pub struct SignedOperation {
    #[serde(with = "serde_bincode_native")]
    /// public key created corresponding to secret key used to generate signature
    pub public_key: PublicKey,
    #[serde(with = "serde_bincode_native")]
    signature: Signature,
    inner: Operation,
}

/// A helper module that Bincode with `serde` backend will call for delegating to native backend
mod serde_bincode_native {
    use bincode::{Decode, Encode};
    use serde::de::Error as DeError;
    use serde::ser::Error as SerError;
    use serde::{Deserializer, Serializer};

    pub fn serialize<T, S>(data: &T, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Encode,
    {
        // 1a) pack into a Vec<u8> via your native Encode
        let bytes =
            bincode::encode_to_vec(data, bincode::config::standard()).map_err(SerError::custom)?;
        // 1b) ask Serde to emit it as a byte sequence
        s.serialize_bytes(&bytes)
    }

    pub fn deserialize<'de, T, D>(d: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: Decode,
    {
        let bytes: Vec<u8> = serde::Deserialize::deserialize(d)?;
        // 2b) decode it back with your native Decode<C>
        let (data, _): (T, _) =
            bincode::decode_from_slice(bytes.as_ref(), bincode::config::standard())
                .map_err(DeError::custom)?;
        Ok(data)
    }
}

impl SignedOperation {
    /// Create a `SignedOperation`
    pub fn new(public_key: PublicKey, signature: Signature, inner: Operation) -> Self {
        Self {
            public_key,
            signature,
            inner,
        }
    }

    /// hash the payload (`Operation`)
    pub fn hash(&self) -> Result<Vec<u8>> {
        self.inner.hash()
    }

    /// return the payload if the signature is valid
    pub fn verify(self) -> Result<Operation> {
        let hash = self.inner.hash()?;
        self.signature.verify(&self.public_key, hash.as_ref())?;

        Ok(self.inner)
    }
}
