// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/**
 * @dev Contract to illustrate how to work around overloaded Solidity methods.
 */
contract OverloadedMethods {
  function getValue(uint256 value) public pure returns (uint256) {
    return value / 2;
  }

  function getValue(bool value) public pure returns (bool) {
    return !value;
  }
}
