// SPDX-FileCopyrightText: 2025 Nomadic Labs <contact@nomadic-labs.com>
//
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

contract GLDToken is ERC20 {
    constructor() ERC20("GOLD", "GLD") {
    }

    // Allow the owner to mint additional tokens later
    function mint(address to, uint256 amount) public {
        _mint(to, amount);
    }
}
