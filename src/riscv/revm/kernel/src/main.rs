// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

use revm::{
    ExecuteCommitEvm, MainBuilder, MainContext,
    context::{Context, TxEnv},
    database::CacheDB,
    database_interface::EmptyDB,
};
use std::error::Error;
use tezos_crypto_rs::hash::SmartRollupHash;
use tezos_smart_rollup::entrypoint;
use tezos_smart_rollup::inbox::ExternalMessageFrame;
use tezos_smart_rollup::inbox::InboxMessage;
use tezos_smart_rollup::michelson::MichelsonUnit;
use tezos_smart_rollup::prelude::Runtime;
use tezos_smart_rollup::prelude::*;
use utils::crypto::Operation;
use utils::crypto::SignedOperation;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// # Returns
/// Err(_) . Failed to retrieve a valid external failed message. In either case we recover and
///   parse the next message:
///   1) Received a well-formed message that was not a transaction (internal messages for
///       other rollups) OR
///   2) Parsing failed in ways that shouldn't happen in production code (eg. ERC-20 bytecode was invalid)
/// Ok(None) if no more input to be parsed.
/// Ok(Some(...)) valid message triggering an EVM transaction
fn get_inbox_message(
    host: &mut impl Runtime,
    rollup_address_hash: &SmartRollupHash,
) -> Result<Option<TxEnv>> {
    match host.read_input()? {
        None => Ok(None),
        Some(input) => {
            let (_, message) = InboxMessage::<MichelsonUnit>::parse(input.as_ref())
                .map_err(|e| (format!("{:?}", e)))?;
            match message {
                InboxMessage::External(bytes) => {
                    let ExternalMessageFrame::Targetted { address, contents } =
                    // err in Result<_,err> returned by parse does not implement std::error:Error so we
                    // map it to a str
                    ExternalMessageFrame::parse(bytes)
                        .map_err(|e| format!("{:?}", e))?;
                    if rollup_address_hash != address.hash() {
                        Err(format!(
                            "Skipping message: External message targets another rollup. Expected: {}. Found: {}\n",
                            rollup_address_hash,
                            address.hash()
                        ).into())
                    } else {
                        let (signed_op, _): (SignedOperation, usize) =
                            bincode::serde::decode_from_slice(
                                contents,
                                bincode::config::standard(),
                            )?;
                        let Operation(tx) = signed_op.verify()?;
                        Ok(Some(tx))
                    }
                }
                InboxMessage::Internal(_) => {
                    // Ignore any other message
                    Err("ignore internal message\n".into())
                }
            }
        }
    }
}

#[entrypoint::main]
#[cfg_attr(
    feature = "static-inbox",
    entrypoint::runtime(static_inbox = "$INBOX_FILE")
)]
pub fn entry(host: &mut impl Runtime) {
    let mut evm = Context::mainnet()
        .with_db(CacheDB::<EmptyDB>::default())
        .build_mainnet();

    let rollup_address_hash = host.reveal_metadata().address();
    loop {
        match get_inbox_message(host, &rollup_address_hash) {
            Ok(Some(tx)) => match evm.transact_commit(tx) {
                Ok(_res) => {
                    debug_msg!(host, "Successful transaction\n");
                }
                Err(err) => {
                    debug_msg!(host, "Unsuccessful transaction: \n{:?}\n", err);
                }
            },
            Ok(None) => {
                break;
            }
            Err(e) => debug_msg!(host, "{}", e),
        }
    }
}
