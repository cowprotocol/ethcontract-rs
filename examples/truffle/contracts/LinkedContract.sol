pragma solidity ^0.5.0;

import "./SimpleLibrary.sol";

contract LinkedContract {
  using SimpleLibrary for uint256;

  function callAnswer() public pure returns (uint256) {
    return uint256(0).answer();
  }
}
