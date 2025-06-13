// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

use core::fmt::Debug;
use revm::context::DBErrorMarker;
use revm::primitives::{Address, B256, U256};
use tezos_smart_rollup::host::RuntimeError;
use tezos_smart_rollup::storage::path::OwnedPath;
use thiserror::Error;

macro_rules! format_path {
    ($fmt:literal $(, $arg:expr )* ) => {
        {
            let path: OwnedPath = format!($fmt $(, $arg)*)
                .try_into()
                .unwrap();
            path
        }
    };
}

/// Abstraction representing path with possibly infinite storage
/// In practice this is done by read/write a path in chunks of 2kB from specified offsets
pub(crate) enum PathBuilder<'a, 'b> {
    Info(&'a Address),
    Code(&'a B256),
    Storage(&'a Address, &'b U256),
}

impl PathBuilder<'_, '_> {
    /// The prefixes are just to prevent path clashes
    /// Also keeps things structured in the filesystem allowing cleanly deleting directories
    /// fmt::Debug gives the raw bytes not the checksum
    pub(crate) fn format(&self) -> OwnedPath {
        use PathBuilder::*;
        match self {
            Info(addr) => format_path!("/{}/{:?}", "i", addr),
            Code(code_hash) => format_path!("/{}/{:?}", "c", code_hash),
            Storage(addr, key) => format_path!("/{}/{:?}/{}", "s", addr, key),
        }
    }
}

/// Propagates errors from internal operations mostly
#[derive(Error, Debug)]
pub enum KernelError {
    #[error("The code corresponding to the following hash doesn't exist: {}", .0)]
    MissingCode(B256),
    #[error("Runtime error when querying storage")]
    DurableStorage(#[from] RuntimeError),
    #[error("Serialization error occurred")]
    Encode(#[from] bincode::error::EncodeError),
    #[error("Deserialization error occurred")]
    Decode(#[from] bincode::error::DecodeError),
}

impl DBErrorMarker for KernelError {}
