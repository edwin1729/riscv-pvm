use alloy_primitives::Address;
use alloy_primitives::aliases; //::{B256, U256};

use serde::Serialize;
use serde::de::{Error as DeError, Visitor};
use serde::ser::Serializer;
use serde::{Deserialize, Deserializer};
use std::fmt;
use std::ops::{Deref, DerefMut};

pub struct U256(pub aliases::U256);
impl Serialize for U256 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(self.0.as_le_slice())
    }
}

impl<'de> Deserialize<'de> for U256 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct U256Visitor;

        impl<'de> Visitor<'de> for U256Visitor {
            type Value = U256;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a 32-byte array representing a U256")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                if v.len() != 32 {
                    return Err(DeError::invalid_length(v.len(), &"32 bytes"));
                }

                let mut arr = [0u8; 32];
                arr.copy_from_slice(v);
                Ok(U256(aliases::U256::from_le_slice(v)))
            }
        }

        deserializer.deserialize_bytes(U256Visitor)
    }
}

impl Deref for U256 {
    type Target = aliases::U256;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for U256 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
pub struct B256(pub aliases::B256);
impl Serialize for B256 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(self.0.as_ref())
    }
}

impl<'de> Deserialize<'de> for B256 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct B256Visitor;

        impl<'de> Visitor<'de> for B256Visitor {
            type Value = B256;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a 32-byte array representing a B256")
            }

            fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                if v.len() != 32 {
                    return Err(DeError::invalid_length(v.len(), &"32 bytes"));
                }
                let mut arr = [0u8; 32];
                arr.copy_from_slice(v);
                Ok(B256(aliases::B256::new(arr)))
            }
        }

        deserializer.deserialize_bytes(B256Visitor)
    }
}

impl Deref for B256 {
    type Target = aliases::B256;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for B256 {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[derive(Serialize, Deserialize)]
pub struct EthAccount {
    pub nonce: u64,
    pub balance: U256,
    //pub storage_root: B256,
    pub storage: Vec<(B256, U256)>,
    pub code_hash: B256,
}

pub type EthState = Vec<(Address, EthAccount)>;

//impl Serialize for EthAccount {
//    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
//    where
//        S: Serializer,
//    {
//        // 3 is the number of fields in the struct.
//        let mut state = serializer.serialize_struct("EthAccount", 5)?;
//        state.serialize_field("nonce", &self.nonce)?;
//        state.serialize_field("balance", &self.balance)?;
//        //state.serialize_field("storage_root", &self.storage_root)?;
//        state.serialize_field("storage", &self.storage)?;
//        state.serialize_field("code_hash", &self.code_hash)?;
//        state.end()
//    }
//}
