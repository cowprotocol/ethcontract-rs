// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/**
 * @dev Rinkeby deployed contract used in examples.
 */
contract DeployedContract {
  mapping(address => uint256) private values;

  /**
   * @dev Gets the current value set in the contract for the `msg.sender`.
   */
  function value() public view returns (uint256) {
    return values[msg.sender];
  }

  /**
   * @dev Increments the value for the `msg.sender` by 1.
   */
  function increment() public returns (uint256) {
    values[msg.sender]++;
    return (values[msg.sender]);
  }
}
