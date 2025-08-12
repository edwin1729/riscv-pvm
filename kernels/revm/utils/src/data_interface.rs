use alloy_sol_types::sol;
use revm::primitives::Bytes;
use serde::{Deserialize, Serialize};

// Generate abi for the function we want to call from the contract
// Solidity source code from https://github.com/OpenZeppelin/openzeppelin-contracts/blob/v5.3.0/contracts/token/ERC20/IERC20.sol
// and its extension in `src/riscv/revm/GLDToken.sol`
sol! {
    function mint(address to, uint256 amount) public onlyOwner;
    function transfer(address to, uint256 value) external returns (bool);
    function balanceOf(address account) external view returns (uint256);
}

/// The data structure the kernel uses to send messages through the log file to be interpreted by
/// benchmark cli when reporting results. Specifically this datatype is serialized in
/// `kernel/src/main.rs` and deserialized in `bench/src/results.rs`
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum LogType {
    StartOfLevel,
    Deploy,
    Execute(Bytes),
    EndOfLevel,
    Error(String),
    Info(String), // logged info that `results.rs` doesn't care about
}
