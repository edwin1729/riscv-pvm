// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

#[cfg(not(feature = "in-memory-db"))]
mod database;
use std::sync::Arc;
use std::sync::Mutex;

#[cfg(not(feature = "in-memory-db"))]
use database::KernelDB;
use revm::context::Context;
use revm::context::TxEnv;
use revm::context_interface::result::ExecutionResult;
use revm::context_interface::result::Output;
#[cfg(feature = "in-memory-db")]
use revm::database::CacheDB;
#[cfg(feature = "in-memory-db")]
use revm::database_interface::EmptyDB;
use revm::ExecuteCommitEvm;
use revm::MainBuilder;
use revm::MainContext;
use tezos_crypto_rs::hash::SmartRollupHash;
use tezos_smart_rollup::entrypoint;
use tezos_smart_rollup::host::RuntimeError;
use tezos_smart_rollup::inbox::ExternalMessageFrame;
use tezos_smart_rollup::inbox::InboxMessage;
use tezos_smart_rollup::inbox::InternalInboxMessage;
use tezos_smart_rollup::michelson::MichelsonUnit;
use tezos_smart_rollup::prelude::Runtime;
use tezos_smart_rollup::prelude::*;
use tezos_smart_rollup::types::Message;
use utils::crypto::Operation;
use utils::crypto::SignedOperation;
use utils::data_interface::LogType;

enum InboxResult {
    InboxEmpty,
    Log(LogType),
    TxEnv(TxEnv),
}
use InboxResult::*;

fn to_inbox_result<T, R, F>(res: Result<T, R>, f: F) -> InboxResult
where
    F: FnOnce(T) -> InboxResult,
    R: std::fmt::Debug,
{
    match res {
        Err(e) => Log(LogType::Error(format!("{:?}", e))),
        Ok(t) => f(t),
    }
}

/// # Returns
/// 1. Err(_) Failed to retrieve a valid external failed message.
//
///   1) Received a well-formed message that was not a transaction (internal messages for
///      other rollups) OR
///   2) Parsing failed in ways that shouldn't happen in production code (eg. ERC-20 bytecode was invalid)
///
///   In either case we recover and parse the next message.
///
/// 2. Ok(None) if no more input to be parsed.
/// 3. Ok(Some(...)) valid message triggering an EVM transaction
fn get_inbox_message(
    //host: &mut impl Runtime,
    input: Result<Option<Message>, RuntimeError>,
    rollup_address_hash: &SmartRollupHash,
) -> InboxResult {
    //let foo = host.read_input(    )
    to_inbox_result(input, |maybe_inp| match maybe_inp {
        None => InboxEmpty,
        Some(input) => to_inbox_result(
            InboxMessage::<MichelsonUnit>::parse(input.as_ref()),
            |(_, message)| match message {
                InboxMessage::External(bytes) => to_inbox_result(
                    ExternalMessageFrame::parse(bytes),
                    |ExternalMessageFrame::Targetted { address, contents }| {
                        if rollup_address_hash != address.hash() {
                            Log(LogType::Info(format!(
                                "Skipping message: External message targets another rollup. Expected: {}. Found: {}",
                                rollup_address_hash,
                                address.hash()
                            )))
                        } else {
                            to_inbox_result(
                                bincode::serde::decode_from_slice(
                                    contents,
                                    bincode::config::standard(),
                                ),
                                |(signed_op, _): (SignedOperation, usize)| {
                                    #[cfg(not(feature = "no-verify"))]
                                    let op = signed_op.verify();
                                    #[cfg(feature = "no-verify")]
                                    let op = signed_op.no_verify();
                                    to_inbox_result(
                                        op.ok_or("verification failed"),
                                        |Operation(tx)| TxEnv(tx),
                                    )
                                },
                            )
                        }
                    },
                ),
                InboxMessage::Internal(msg) => match msg {
                    InternalInboxMessage::StartOfLevel => Log(LogType::StartOfLevel),
                    InternalInboxMessage::InfoPerLevel(info) => Log(LogType::Info(format!(
                        "Internal message: level info \
                            (block predecessor: {}, predecessor_timestamp: {}",
                        info.predecessor, info.predecessor_timestamp
                    ))),
                    InternalInboxMessage::EndOfLevel => Log(LogType::EndOfLevel),
                    InternalInboxMessage::Transfer(_) => {
                        Log(LogType::Info("Internal message: transfer".into()))
                    }
                },
            },
        ),
    })
}

#[entrypoint::main]
#[cfg_attr(
    feature = "static-inbox",
    entrypoint::runtime(static_inbox = "$INBOX_FILE")
)]
pub fn entry(host: &mut impl Runtime) {
    let rollup_address_hash = host.reveal_metadata().address();

    let wrapped_host = Arc::new(Mutex::new(host));

    #[cfg(not(feature = "in-memory-db"))]
    let db = KernelDB::new(Arc::clone(&wrapped_host));
    #[cfg(feature = "in-memory-db")]
    let db = CacheDB::<EmptyDB>::default();

    let mut evm = Context::mainnet().with_db(db).build_mainnet();
    loop {
        let parsed_message = { wrapped_host.lock().unwrap().read_input() };
        match get_inbox_message(parsed_message, &rollup_address_hash) {
            TxEnv(tx) => match evm.transact_commit(tx) {
                Ok(res) => {
                    let log = handle_res(res);
                    if let Ok(ser) = serde_json::to_string(&log) {
                        debug_msg!(wrapped_host.lock().unwrap(), "{}\n", ser);
                    }
                }
                Err(err) => {
                    let err = LogType::Error(format!("Unsuccessful transaction: \n{:?}", err));
                    if let Ok(ser) = serde_json::to_string(&err) {
                        debug_msg!(wrapped_host.lock().unwrap(), "{}\n", ser);
                    }
                }
            },
            InboxEmpty => {
                break;
            }
            Log(log) => {
                if let Ok(ser) = serde_json::to_string(&log) {
                    debug_msg!(wrapped_host.lock().unwrap(), "{}\n", ser);
                }
            }
        }
    }
}

fn handle_res(res: ExecutionResult) -> LogType {
    match res {
        ExecutionResult::Success {
            output, //Output::Call(value),
            ..
        } => match output {
            Output::Create(_, _) => LogType::Deploy,
            Output::Call(bytes) => LogType::Execute(bytes),
        },
        ExecutionResult::Revert { .. } => {
            LogType::Error("Smart contract execution reverted".into())
        }
        ExecutionResult::Halt { reason, .. } => {
            LogType::Error(format!("Halt: reason - {:?}", reason))
        }
    }
}
