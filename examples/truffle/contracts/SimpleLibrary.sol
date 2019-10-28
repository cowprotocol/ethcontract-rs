pragma solidity ^0.5.0;

library SimpleLibrary {
  function answer(uint256 self) public pure returns (uint256) {
    return self + 42;
  }
}