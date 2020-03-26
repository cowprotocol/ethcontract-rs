pragma solidity ^0.5.0;

/**
 * @dev Contract to illustract how to work around overloaded Solidity methods.
 */
contract OverloadedMethods {
  function getValue(uint256 value) public pure returns (uint256) {
    return value / 2;
  }

  function getValue(bool value) public pure returns (bool) {
    return !value;
  }
}
