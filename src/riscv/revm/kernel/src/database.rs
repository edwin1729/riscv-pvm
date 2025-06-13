// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

use bincode::config::standard;
use revm::database::{Database, DatabaseCommit};
use revm::primitives::{Address, B256, HashMap, KECCAK_EMPTY, U256, keccak256};
use revm::state::{Account, AccountInfo, Bytecode};
use serde::{de::DeserializeOwned, ser::Serialize};
use tezos_smart_rollup::core_unsafe::MAX_FILE_CHUNK_SIZE as MAX_CHUNK;
use tezos_smart_rollup::host::{Runtime, RuntimeError};

mod database_utils;
use database_utils::{KernelError, PathBuilder, PathBuilder::*};

type Result<T> = std::result::Result<T, KernelError>;
// The durable storage Database

/// The required data structures for running revm is arranged in the durable storage as described:
/// A) The general `AccountInfo` (balance, nonce, code_hash) can be found here:
///   `/i/<address> -> AccountInfo`
/// B) Split the (max 24KB) Bytecode of smart contract accounts from the rest of the account data
///   `/c/<code_hash> -> Bytecode`
/// C) And finally the storage is an additional map under each address
///   `/s/<address>/<Uint> -> Uint`
pub struct KernelDB<'a, R: Runtime> {
    host: &'a mut R,
}

impl<'a, R: Runtime> KernelDB<'a, R> {
    /// Create a database interfacing with the kernel durable storage
    pub fn new(host: &'a mut R) -> Self {
        KernelDB { host }
    }
    fn insert_contract(&mut self, account: &mut AccountInfo) -> Result<()> {
        if let Some(code) = &account.code {
            if !code.is_empty() {
                if account.code_hash == KECCAK_EMPTY {
                    account.code_hash = code.hash_slow();
                }
                self.store_write(Code(&account.code_hash), code)?;
            }
        }
        if account.code_hash.is_zero() {
            account.code_hash = KECCAK_EMPTY;
        }

        Ok(())
    }
    fn store_write<S>(&mut self, path: PathBuilder, data: &S) -> Result<()>
    where
        S: Serialize,
    {
        let bytes = bincode::serde::encode_to_vec(data, standard())?;

        for (i, chunk) in bytes.chunks(MAX_CHUNK).enumerate() {
            self.host
                .store_write(&path.format(), chunk, i * MAX_CHUNK)?;
        }
        Ok(())
    }
    fn store_read<D>(&mut self, path: PathBuilder) -> Result<Option<D>>
    where
        D: DeserializeOwned,
    {
        let n = match self.host.store_value_size(&path.format()) {
            Ok(n) => n,
            Err(RuntimeError::PathNotFound) => return Ok(None),
            Err(err) => return Err(err.into()),
        };
        let mut buf = vec![0u8; n];
        for i in 0..n.div_ceil(MAX_CHUNK) {
            let _ = self.host.store_read_slice(
                &path.format(),
                i * MAX_CHUNK,
                &mut buf[i * MAX_CHUNK..n.min((i + 1) * MAX_CHUNK)],
            )?;
        }
        Ok(bincode::serde::decode_from_slice(&buf, standard())
            .map(|(data, _size)| Some(data))?)
    }
    fn insert_new_account(&mut self, address: &Address) -> Result<()> {
        self.store_write(Info(address), &AccountInfo::default())?;
        Ok(())
    }
    fn clear_storage(&mut self, address: &Address) -> Result<()> {
        self.host.store_delete(&Info(address).format())?;
        Ok(())
    }

    // A version of the `DatabaseCommit` trait's function with `Result` return
    fn commit_safe(&mut self, changes: HashMap<Address, Account>) -> Result<()> {
        for (address, mut account) in changes {
            if !account.is_touched() {
                continue;
            }
            if account.is_selfdestructed() {
                self.insert_new_account(&address)?;
                self.clear_storage(&address)?;
                continue;
            }
            self.insert_contract(&mut account.info)?;

            account.info.code = None;
            //Above the contract from AccountInfo is deleted so we don't store it again in the next line
            self.store_write(Info(&address), &account.info)?;

            for (key, value) in account.storage {
                self.store_write(Storage(&address, &key), &value.present_value())?;
            }
        }
        Ok(())
    }
}

// Revm trait implementations

impl<'a, R: Runtime> Database for KernelDB<'a, R> {
    type Error = KernelError;

    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>> {
        match self.store_read(Info(&address))? {
            Some(info) => Ok(Some(info)),
            None => {
                self.insert_new_account(&address)?;
                Ok(None)
            }
        }
    }
    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode> {
        self.store_read(Code(&code_hash))?
            .ok_or(KernelError::MissingCode(code_hash))
    }
    fn storage(&mut self, address: Address, index: U256) -> Result<U256> {
        match self.store_read(Storage(&address, &index))? {
            Some(val) => Ok(val),
            None => {
                if self.host.store_has(&Info(&address).format())?.is_some() {
                    self.insert_new_account(&address)?;
                }
                Ok(U256::ZERO)
            }
        }
    }
    fn block_hash(&mut self, number: u64) -> Result<B256> {
        Ok(keccak256(number.to_le_bytes())) // what CacheDB<EmptTypedDB> does
    }
}

/// Based on the impl of of this trait for CacheDB<ExtDB> from
/// https://docs.rs/revm/latest/revm/trait.DatabaseCommit.html#impl-DatabaseCommit-for-CacheDB%3CExtDB%3E
impl<'a, R: Runtime> DatabaseCommit for KernelDB<'a, R> {
    // This trait doesn't accommodate errors so we just ignore any errors
    fn commit(&mut self, changes: HashMap<Address, Account>) {
        self.commit_safe(changes).unwrap()
    }
}
