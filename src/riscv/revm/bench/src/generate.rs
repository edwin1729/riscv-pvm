// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

use std::error::Error;
use std::path::Path;
use std::vec;

use alloy_sol_types::{SolCall, sol};
use jstz_crypto::keypair_from_passphrase;
use revm::{
    context::TxEnv,
    primitives::{Address, TxKind, U256, address, hex},
};
use tezos_data_encoding::enc::BinWriter;
use tezos_smart_rollup::inbox::ExternalMessageFrame;
use tezos_smart_rollup::types::SmartRollupAddress;
use tezos_smart_rollup::utils::inbox::file::InboxFile;
use tezos_smart_rollup::utils::inbox::file::Message;

use utils::crypto::Operation;
use utils::crypto::SignedOperation;

const GLD_CONTRACT_BYTECODE: &str = include_str!("../../contract.bin");
// This is fragile since it is hardcoded for the GLDToken contract of originator with address 0x1
const CONTRACT_ADDRESS: Address = address!("Bd770416a3345F91E4B34576cb804a576fa48EB1");
const ALICE: Address = address!("647384899878C28EC3F61d100daF4d40471f1852"); // random
const BOB: Address = address!("983267588688C28EC3F61d100daF4d40471f1853"); // random
const MINTER: Address = address!("0000000000000000000000000000000000000001");
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
    let messages = create_operations(transfers)?
        .into_iter()
        .enumerate()
        .map(|(i, tx)| generate_message(rollup_addr, tx, i))
        .collect::<Result<Vec<_>>>()?;

    // Output inbox file
    Ok(InboxFile(vec![messages]))
}

/// `TxEnv` is the type a transaction on ethereum (revm). We serialize these transactions using the
/// external message frame protocol
/// We also sign the message with some generated public-private key pair. As its just a
/// benchmark we don't care that each address should correspond to a pair
fn generate_message(rollup_addr: &str, tx: TxEnv, i: usize) -> Result<Message> {
    let rollup_addr = SmartRollupAddress::from_b58check(rollup_addr)?;
    // Create signed operation
    let (sk, pk) = keypair_from_passphrase(&i.to_string())?;
    let op = Operation(tx);
    let sig = sk.sign(op.hash()?)?;
    let signed_op = SignedOperation::new(pk.clone(), sig, op);
    let bytes = bincode::serde::encode_to_vec(&signed_op, bincode::config::standard())?;
    let mut external = Vec::with_capacity(bytes.len() + EXTERNAL_FRAME_SIZE);
    let frame = ExternalMessageFrame::Targetted {
        contents: bytes,
        address: rollup_addr,
    };

    frame.bin_write(&mut external)?;

    Ok(Message::External { external })
}

/// Hardcode some operations to be sent through the inbox file
/// 1. Deploy the GLDToken ERC20 contract
/// 2. Mint 100 coins to Alice from the owner address (0x1)
/// 3. Alice sends 100 coins to Bob
/// 4. Alice's balance gets queried
fn create_operations(transfers: usize) -> Result<Vec<TxEnv>> {
    let mut operations = Vec::new();

    // deploy the contract
    let bytecode: Vec<u8> = hex::decode(GLD_CONTRACT_BYTECODE)?;

    operations.push(TxEnv {
        kind: TxKind::Create,
        data: bytecode.into(),
        ..TxEnv::default()
    });

    let len = accounts_for_transfers(transfers);
    let addrs: Vec<Address> = (0..len)
        .into_iter()
        .map(|i| Address::left_padding_from(&usize::to_be_bytes(i)))
        .collect();

    // Mint coins for everyone

    let mut nonce = 0; // nonce for minting
    // Generate abi for the function we want to call from the contract
    // Solidity source code from https://github.com/OpenZeppelin/openzeppelin-contracts/blob/v5.3.0/contracts/token/ERC20/IERC20.sol
    // and my extension of it `GLDToken.sol`
    sol! {
        function mint(address to, uint256 amount) public onlyOwner;
    }

    let amount = len + 1;
    for i in 0..len {
        let mint_call = mintCall {
            to: ALICE,
            amount: U256::from(amount),
        }
        .abi_encode();
        let op = TxEnv {
            kind: TxKind::Call(CONTRACT_ADDRESS),
            data: mint_call.into(),
            caller: MINTER,
            nonce: nonce,
            ..TxEnv::default()
        };
        operations.push(op);
        nonce += 1;
    }

    // Generate transfers

    let mut nonces = vec![0; len]; // for transfers

    sol! {
        function transfer(address to, uint256 value) external returns (bool);
    }

    let alice_to_bob = transferCall {
        to: BOB,
        value: U256::from(40),
    }
    .abi_encode();

    let expected_len = operations.len() + transfers;

    'outer: for token_id in 0..len {
        for (from, amount) in (token_id..(token_id + len)).zip(1..len) {
            if expected_len == operations.len() {
                break 'outer;
            }

            let to = addrs[(from + 1) % len];
            let from_addr = addrs[from % len];
            let call_data = transferCall {
                to,
                value: U256::from(len - amount),
            }
            .abi_encode();
            let op = TxEnv {
                kind: TxKind::Call(CONTRACT_ADDRESS),
                data: call_data.into(),
                caller: from_addr,
                nonce: nonces[from % len],
                ..TxEnv::default()
            };
            operations.push(op);
            nonces[from % len] += 1;
        }
    }

    // Query ALICE's balance
    //sol! {
    //    function balanceOf(address account) external view returns (uint256);
    //}

    //let query = balanceOfCall { account: ALICE }.abi_encode();

    //operations.push(TxEnv {
    //    kind: TxKind::Call(CONTRACT_ADDRESS),
    //    data: query.into(),
    //    nonce: 1,
    //    ..TxEnv::default()
    //});

    Ok(operations)
}

/// The generation strategy supports up to `num_accounts ^ 2` transfers,
/// find the smallest number of accounts which will allow for this.
fn accounts_for_transfers(transfers: usize) -> usize {
    f64::sqrt(transfers as f64).ceil() as usize + 1
}
