pragma solidity ^0.5.0;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/token/ERC20/ERC20Detailed.sol";

contract RustCoin is ERC20, ERC20Detailed {
  constructor() ERC20Detailed("Rust Coin", "RUST", 18) public {
    _mint(msg.sender, 1337 * (10 ** uint256(decimals())));
  }
}
