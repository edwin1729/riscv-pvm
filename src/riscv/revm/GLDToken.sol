// SPDX-License-Identifier: MIT

// This is the source code of `contract.bin`. Included here for completeness.

// Can recompile this to contract.bin with the the following commands
// If not using nix the first line installs nodejs npm and solc (soliditiy compiler)
// nix shell nixpkgs#nodejs github:hellwolf/solc.nix#solc_0_8_20
// npm init -y
// npm install @openzeppelin/contracts
// solc-0.8.20 GLDToken.sol --bin --abi --optimize -o build/ --base-path . --include-path node_modules/
pragma solidity ^0.8.20;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {Ownable} from "@openzeppelin/contracts/access/Ownable.sol";

contract GLDToken is ERC20, Ownable {
    constructor() ERC20("GOLD", "GLD") Ownable(0x0000000000000000000000000000000000000001) {
        // Mint some initial supply to the contract deployer
        //_mint(0x0000000000000000000000000000000000000001, 1000000 * 10 ** decimals()); // 1,000,000 tokens
    }

    // Allow the owner to mint additional tokens later
    function mint(address to, uint256 amount) public onlyOwner {
        _mint(to, amount);
    }
}
