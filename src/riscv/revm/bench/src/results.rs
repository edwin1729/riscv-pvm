// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

use std::fmt;
use std::fs::read_to_string;
use std::path::Path;
use std::time::Duration;

use alloy_sol_types::SolCall;
use revm::primitives::U256;
use serde::Deserialize;
use tezos_smart_rollup::utils::inbox::file::InboxFile;
use tezos_smart_rollup::utils::inbox::file::Message;

use crate::Result;
use crate::generate::accounts_for_transfers;
use utils::data_interface::{LogType, balanceOfCall, transferCall};

// Deployment, Minting, Transfers, Balance Checks
// all contained in one level
const EXPECTED_LEVELS: usize = 1;

/// The `results` command of the cli is implemented by this function. It makes sure the `all_logs`
/// `expected_transfers` and `inbox` are all consistent with each other.
/// If so reports the TPS
pub fn handle_results(
    inbox: Box<Path>,
    all_logs: Vec<Box<Path>>,
    expected_transfers: usize,
) -> Result<()> {
    let inbox = InboxFile::load(&inbox)?;

    let all_metrics = all_logs
        .iter()
        .map(|logs| {
            let logs: Vec<ParsedLogLine> = read_to_string(logs)?
                .lines()
                .map(serde_json::from_str)
                .filter_map(|l| l.map(LogLine::classify).transpose())
                .collect::<std::result::Result<Vec<_>, _>>()?;

            let levels = logs_to_levels(logs, expected_transfers)?;

            if inbox.0.len() != levels.len() || levels.len() != EXPECTED_LEVELS {
                return Err(format!(
                    "InboxFile contains {} levels, found {} in logs, expected {EXPECTED_LEVELS}",
                    inbox.0.len(),
                    levels.len()
                )
                .into());
            }
            let expected_accounts = accounts_for_transfers(expected_transfers);

            let [results]: [_; EXPECTED_LEVELS] = levels.try_into().unwrap();

            check_counts(&results, &inbox.0[0], expected_accounts, expected_transfers)?;
            let metrics = check_transfer_metrics(&results, expected_transfers)?;
            check_balances(&results, expected_transfers)?;

            Ok(metrics)
        })
        .collect::<Result<Vec<_>>>()?;

    if all_metrics.len() > 1 {
        let len = all_metrics.len();

        for (num, metrics) in all_metrics.iter().enumerate() {
            println!("Run {} / {len} => {metrics}", num + 1);
        }

        let agg_metrics = TransferMetrics::aggregate(&all_metrics);
        println!("\nAggregate => {agg_metrics}");
    } else if let Some(metrics) = all_metrics.first() {
        println!("{metrics}");
    }

    Ok(())
}

fn check_counts(
    level: &Level,
    messages: &Vec<Message>,
    accounts: usize,
    transfers: usize,
) -> Result<()> {
    // We allow for more messages. Say there were some messages for another rollup
    // Note: 1 for deployment, `account` many for both minting and balance_checks
    // and `transfers` many for transfers
    if messages.len() < 1 + 2 * accounts + transfers {
        return Err(format!(
            "Expected atleast {} inbox messages. Found {}",
            1 + 2 * accounts + transfers,
            messages.len()
        )
        .into());
    }

    if level.deployments.len() != 1 {
        return Err("Expected ERC-20 contract deployment".into());
    }

    if level.mints.len() != accounts {
        return Err(format!(
            "Expected {} minting operations. Found {}",
            accounts,
            level.mints.len()
        )
        .into());
    }

    if level.transfers.len() != transfers {
        return Err(format!(
            "Expected {} transfer operations. Found {}",
            transfers,
            level.transfers.len()
        )
        .into());
    }

    if level.balance_checks.len() != accounts {
        return Err(format!(
            "Expected {} minting operations. Found {}",
            accounts,
            level.balance_checks.len()
        )
        .into());
    }

    Ok(())
}

#[derive(Clone, Debug, Default)]
struct TransferMetrics {
    transfers: usize,
    duration: Duration,
    tps: f64,
}

impl TransferMetrics {
    fn aggregate(metrics: &[TransferMetrics]) -> TransferMetrics {
        let summed = metrics.iter().fold(Self::default(), |acc, m| Self {
            transfers: acc.transfers + m.transfers,
            duration: acc.duration + m.duration,
            tps: acc.tps + m.tps,
        });

        Self {
            tps: summed.tps / metrics.len() as f64,
            ..summed
        }
    }
}

impl fmt::Display for TransferMetrics {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ERC-20 transfers took {:?} @ {:.3} TPS",
            self.transfers, self.duration, self.tps
        )
    }
}

fn check_transfer_metrics(level: &Level, transfers: usize) -> Result<TransferMetrics> {
    if transfers != level.transfers.len() {
        return Err(format!(
            "Expected {transfers} transfers, got {}.",
            level.transfers.len()
        )
        .into());
    }

    // The first `account` executions are the minting calls. We collect the time elapsed at the _end_ of the
    // minting, all the way up to the _end_ of the last execution (transfer).
    let duration = level.transfers.last().unwrap().elapsed - level.mints.last().unwrap().elapsed;
    let tps = (transfers as f64) / duration.as_secs_f64();

    Ok(TransferMetrics {
        transfers,
        duration,
        tps,
    })
}

// The generated transfers (for a number of accounts N), has a target final state:
// Every account should hold one of every token.
//
// This requires (N - 1) * num_tokens transfers.
//
// Therefore, if an account has `0` of a token, there's a transfer missing below this maximum
// number.
fn check_balances(level: &Level, transfers: usize) -> Result<()> {
    // rerun transfer generation and check if the balances match

    // The same transfer generation strategy from `generate.rs` is adapted here
    // to calculate what the expected balances would be if all the transactions were
    // successful
    let len = accounts_for_transfers(transfers);
    let mut balances = vec![len + 1; len];
    let mut i = 0;

    'outer: for token_id in 0..len {
        for (from, amount) in (token_id..(token_id + len)).zip(1..len) {
            if i == transfers {
                break 'outer;
            }
            let value = len - amount;
            balances[from % len] -= value;
            balances[(from + 1) % len] += value;
            i += 1;
        }
    }

    let observed_balances: Vec<usize> = level.balance_checks.iter().map(|x| x.1).collect();
    if balances == observed_balances {
        Ok(())
    } else {
        Err(format!(
            "Balances didn't match expected {:?} got {:?}",
            observed_balances, balances
        )
        .into())
    }
}

fn logs_to_levels(logs: Vec<ParsedLogLine>, transfers: usize) -> Result<Vec<Level>> {
    let accounts = accounts_for_transfers(transfers);
    let mut levels = Vec::new();

    let mut level = Level::default();

    let mut i = 0;
    for line in logs.into_iter() {
        match line.log_type {
            LogType::StartOfLevel => {
                if level != Level::default() {
                    return Err(
                        format!("StartOfLevel message not at start of level {level:?}").into(),
                    );
                }
            }
            LogType::EndOfLevel => {
                levels.push(level);
                level = Default::default();
            }
            LogType::Deploy => level.deployments.push(line),
            LogType::Execute(ref bytes) => {
                if i < accounts {
                    level.mints.push(line);
                } else if i < accounts + transfers {
                    let success = transferCall::abi_decode_returns(bytes)?;
                    if !success {
                        return Err("Revm transfer transaction didn't succeed".into());
                    }
                    level.transfers.push(line);
                } else if i < 2 * accounts + transfers {
                    let balance: U256 = balanceOfCall::abi_decode_returns(bytes)?;
                    level.balance_checks.push((line, balance.try_into()?));
                } else {
                    return Err(
                        "More transactions (either of mints transfers or balance checks) than expected
Expected {i+1} got more than that"
                            .into(),
                    );
                }
                i += 1;
            }
            LogType::Error(e) => return Err(e.into()),
            LogType::Info(_) => (),
        }
    }

    if level != Level::default() {
        return Err("Final level missing EndOfLevel message {last:?}".into());
    }

    Ok(levels)
}

// There are 3 layers of parsing going on here and 3 data structures representing the target of
// each stage
// 1) Parse from PVM's json format to `LogLine`
// 2) Parse `message` within a `LogLine` into `LogType`, which was constructed by the kernel
// 3) Abi decode the `LogType::Execute`'s `bytes` which was the smart contract's result value as
//    returned by revm
#[derive(Deserialize, Debug, PartialEq)]
struct LogLine {
    elapsed: Duration,
    message: String,
}

#[derive(Deserialize, Debug, PartialEq)]
struct ParsedLogLine {
    elapsed: Duration,
    log_type: LogType,
}

impl LogLine {
    fn classify(self) -> Option<ParsedLogLine> {
        // If it can't be parsed it's some other message like level info which is dropped
        let log_type: LogType = serde_json::from_str(&self.message).ok()?;
        Some(ParsedLogLine {
            elapsed: self.elapsed,
            log_type,
        })
    }
}

#[derive(Default, Debug, PartialEq)]
struct Level {
    deployments: Vec<ParsedLogLine>,
    mints: Vec<ParsedLogLine>,
    transfers: Vec<ParsedLogLine>,
    balance_checks: Vec<(ParsedLogLine, usize)>, // contains a balance as well
}
