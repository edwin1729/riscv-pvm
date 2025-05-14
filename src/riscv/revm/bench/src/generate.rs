// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
// SPDX-License-Identifier: MIT

use std::error::Error;
use std::path::Path;
use std::vec;

use alloy_sol_types::{SolCall, sol};
use revm::{
    context::TxEnv,
    primitives::{Address, TxKind, U256, address, hex},
};
use tezos_data_encoding::enc::BinWriter;
use tezos_smart_rollup::inbox::ExternalMessageFrame;
use tezos_smart_rollup::types::SmartRollupAddress;
use tezos_smart_rollup::utils::inbox::file::InboxFile;
use tezos_smart_rollup::utils::inbox::file::Message;

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
pub fn handle_generate(rollup_addr: &str, inbox_file: &Path) -> Result<()> {
    generate_inbox(rollup_addr)?.save(inbox_file)
}

fn generate_inbox(rollup_addr: &str) -> Result<InboxFile> {
    let messages = create_operations()?
        .into_iter()
        .map(|tx| generate_message(rollup_addr, tx))
        .collect::<Result<Vec<_>>>()?;

    // Output inbox file
    Ok(InboxFile(vec![messages]))
}

/// `TxEnv` is the type a transaction on ethereum (revm). We serialize these transactions using the
/// external message frame protocol
fn generate_message(rollup_addr: &str, tx: TxEnv) -> Result<Message> {
    let rollup_addr = SmartRollupAddress::from_b58check(rollup_addr)?;
    let bytes = bincode::serde::encode_to_vec(&tx, bincode::config::standard())?;
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
fn create_operations() -> Result<Vec<TxEnv>> {
    let mut operations = Vec::new();

    // deploy the contract
    let bytecode: Vec<u8> = hex::decode(GLD_CONTRACT_BYTECODE)?;

    operations.push(TxEnv {
        kind: TxKind::Create,
        data: bytecode.into(),
        ..TxEnv::default()
    });

    // mint coins for ALICE

    // Generate abi for the function we want to call from the contract
    // Solidity source code from https://github.com/OpenZeppelin/openzeppelin-contracts/blob/v5.3.0/contracts/token/ERC20/IERC20.sol
    // and my extension of it `GLDToken.sol`
    sol! {
        function mint(address to, uint256 amount) public onlyOwner;
    }

    let mint_to_alice = mintCall {
        to: ALICE,
        amount: U256::from(100),
    }
    .abi_encode();

    operations.push(TxEnv {
        kind: TxKind::Call(CONTRACT_ADDRESS),
        data: mint_to_alice.into(),
        //Mint coins if caller ("from" in this case) is Zero
        caller: MINTER,
        // "transaction value in wei" Huh what does that mean. Setting gas cost?
        value: U256::from(0),
        ..TxEnv::default()
    });

    // Transfer from ALICE to BOB

    sol! {
        function transfer(address to, uint256 value) external returns (bool);
    }

    let alice_to_bob = transferCall {
        to: BOB,
        value: U256::from(40),
    }
    .abi_encode();

    operations.push(TxEnv {
        kind: TxKind::Call(CONTRACT_ADDRESS),
        data: alice_to_bob.into(),
        caller: ALICE,
        nonce: 0,
        ..TxEnv::default()
    });

    // Query ALICE's balance
    sol! {
        function balanceOf(address account) external view returns (uint256);
    }

    let query = balanceOfCall { account: ALICE }.abi_encode();

    operations.push(TxEnv {
        kind: TxKind::Call(CONTRACT_ADDRESS),
        data: query.into(),
        nonce: 1,
        ..TxEnv::default()
    });

    Ok(operations)
}
