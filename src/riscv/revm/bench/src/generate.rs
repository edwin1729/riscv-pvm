// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

use std::path::Path;
use std::vec;
use std::{error::Error, u64};

use alloy_sol_types::SolCall; // for using `abi_encode`
use jstz_crypto::{keypair_from_passphrase, public_key::PublicKey, secret_key::SecretKey};
use revm::{
    context::TxEnv,
    primitives::{Address, Bytes, TxKind, U256, address, hex},
};
use tezos_data_encoding::enc::BinWriter;
use tezos_smart_rollup::inbox::ExternalMessageFrame;
use tezos_smart_rollup::types::SmartRollupAddress;
use tezos_smart_rollup::utils::inbox::file::InboxFile;
use tezos_smart_rollup::utils::inbox::file::Message;

use utils::crypto::Operation;
use utils::crypto::SignedOperation;
use utils::data_interface::{balanceOfCall, mintCall, transferCall};

const GLD_CONTRACT_BYTECODE: &str = include_str!("../../contract.bin");
// Big enough that it doesn't clash with the 0..num accounts
const MINTER: Address = address!("9999999999999999999999999999999999999999");
const EXTERNAL_FRAME_SIZE: usize = 21;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

/// Generate the requested ' transfers', writing to `./inbox.json`.
///
/// This includes setup (contract deployment/minting) as well as balance checks at the end.
/// The transfers are generated with a 'follow on' strategy. For example 'account 0' will
/// have `num_accounts` minted of 'token 0'. It will then transfer all of them to 'account 1',
/// which will transfer `num_accounts - 1` to the next account, etc.
pub fn handle_generate(rollup_addr: &str, inbox_file: &Path, transfers: usize) -> Result<()> {
    generate_inbox(rollup_addr, transfers)?.save(inbox_file)
}

/// Like [`handle_generate`] but writes the inbox as a shell script.
pub fn handle_generate_script(
    rollup_addr: &str,
    script_file: &Path,
    transfers: usize,
) -> Result<()> {
    let inbox = generate_inbox(rollup_addr, transfers)?;
    inbox.save_script(script_file)?;
    Ok(())
}

fn generate_inbox(rollup_addr: &str, transfers: usize) -> Result<InboxFile> {
    let rollup_addr = SmartRollupAddress::from_b58check(rollup_addr)?;
    let messages = create_operations(&rollup_addr, transfers)?;

    // Output inbox file
    Ok(InboxFile(vec![messages]))
}

struct Account {
    nonce: u64,
    sk: SecretKey,
    pk: PublicKey,
    address: Address,
}

impl Account {
    /// `TxEnv` is the type a transaction on ethereum (revm). We serialize these transactions using the
    /// external message frame protocol
    fn operation_to_message(
        &mut self,
        rollup_addr: &SmartRollupAddress,
        kind: TxKind,
        abi_call: Bytes,
    ) -> Result<Message> {
        let tx = TxEnv {
            kind,
            data: abi_call,
            caller: self.address,
            nonce: self.nonce,
            //gas_limit: u64::MAX,
            ..TxEnv::default()
        };
        self.nonce += 1;
        // Create signed operation
        let op = Operation(tx);
        let sig = self.sk.sign(op.hash()?)?;
        let signed_op = SignedOperation::new(self.pk.clone(), sig, op);
        let bytes = bincode::serde::encode_to_vec(&signed_op, bincode::config::standard())?;
        let mut external = Vec::with_capacity(bytes.len() + EXTERNAL_FRAME_SIZE);
        let frame = ExternalMessageFrame::Targetted {
            contents: bytes,
            address: rollup_addr.clone(),
        };

        frame.bin_write(&mut external)?;

        Ok(Message::External { external })
    }
}

/// 1. Deploy the GLDToken ERC20 contract
/// 2. Mint fixed amount of coins to each address
/// 3. Generate trasnfers between the accounts
/// 4. Query balance of all accounts
fn create_operations(rollup_addr: &SmartRollupAddress, transfers: usize) -> Result<Vec<Message>> {
    // setup
    let mut messages = Vec::new();

    let (sk, pk) = keypair_from_passphrase("foobar")?;
    let mut minter = Account {
        nonce: 0,
        sk,
        pk,
        address: MINTER,
    };

    let len = accounts_for_transfers(transfers);
    // Account address cannot be 0. This is reserved in ethereum and transactions revert if we try
    // to use it
    let mut accounts: Vec<Account> = (1..=len)
        .map(|i| {
            let (sk, pk) = keypair_from_passphrase(&i.to_string())?;
            Ok(Account {
                nonce: 0,
                sk,
                pk,
                address: Address::left_padding_from(&usize::to_be_bytes(i)),
            })
        })
        .collect::<Result<_>>()?;

    // contract addresses in ethereum are a function of the originator and the nonce
    // we need to calculate it beforehand in order to send transaction to that address
    let contract: Address = minter.address.create(minter.nonce);
    // deploy the contract
    let bytecode: Vec<u8> = hex::decode(GLD_CONTRACT_BYTECODE)?;
    messages.push(minter.operation_to_message(rollup_addr, TxKind::Create, bytecode.into())?);

    // mint coins for everyone

    let amount = len + 1;
    for acc in &accounts {
        let mint_call = mintCall {
            to: acc.address,
            amount: U256::from(amount),
        }
        .abi_encode();
        let msg =
            minter.operation_to_message(rollup_addr, TxKind::Call(contract), mint_call.into())?;
        messages.push(msg);
    }

    // Generate transfers

    let expected_len = messages.len() + transfers;

    'outer: for token_id in 0..len {
        for (from, amount) in (token_id..(token_id + len)).zip(1..len) {
            if expected_len == messages.len() {
                break 'outer;
            }

            let call_data = transferCall {
                to: accounts[(from + 1) % len].address,
                value: U256::from(len - amount),
            }
            .abi_encode();
            let msg = accounts[from % len].operation_to_message(
                rollup_addr,
                TxKind::Call(contract),
                call_data.into(),
            )?;
            messages.push(msg);
        }
    }

    // Query everyone's balance

    for acc in &accounts {
        let balance_call = balanceOfCall {
            account: acc.address,
        }
        .abi_encode();
        let msg = minter.operation_to_message(
            rollup_addr,
            TxKind::Call(contract),
            balance_call.into(),
        )?;
        messages.push(msg);
    }

    Ok(messages)
}

/// The generation strategy supports up to `num_accounts ^ 2` transfers,
/// find the smallest number of accounts which will allow for this.
pub fn accounts_for_transfers(transfers: usize) -> usize {
    f64::sqrt(transfers as f64).ceil() as usize + 1
}
