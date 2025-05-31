// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

use revm::{
    ExecuteCommitEvm, MainBuilder, MainContext,
    context::{Context, TxEnv},
    context_interface::result::{ExecutionResult, Output},
    database::CacheDB,
    database_interface::EmptyDB,
};
use tezos_crypto_rs::hash::SmartRollupHash;
use tezos_smart_rollup::entrypoint;
use tezos_smart_rollup::inbox::ExternalMessageFrame;
use tezos_smart_rollup::inbox::{InboxMessage, InternalInboxMessage};
use tezos_smart_rollup::michelson::MichelsonUnit;
use tezos_smart_rollup::prelude::Runtime;
use tezos_smart_rollup::prelude::*;
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
) -> InboxResult {
    to_inbox_result(host.read_input(), |maybe_inp| match maybe_inp {
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
                                    to_inbox_result(
                                        signed_op.verify().ok_or("verification failed"),
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
    let mut evm = Context::mainnet()
        .with_db(CacheDB::<EmptyDB>::default())
        .build_mainnet();

    let rollup_address_hash = host.reveal_metadata().address();
    loop {
        match get_inbox_message(host, &rollup_address_hash) {
            TxEnv(tx) => match evm.transact_commit(tx) {
                Ok(res) => {
                    let log = handle_res(res);
                    if let Ok(ser) = serde_json::to_string(&log) {
                        debug_msg!(host, "{}\n", ser);
                    }
                }
                Err(err) => {
                    let err = LogType::Error(format!("Unsuccessful transaction: \n{:?}", err));
                    if let Ok(ser) = serde_json::to_string(&err) {
                        debug_msg!(host, "{}\n", ser);
                    }
                }
            },
            InboxEmpty => {
                break;
            }
            Log(log) => {
                if let Ok(ser) = serde_json::to_string(&log) {
                    debug_msg!(host, "{}\n", ser);
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
