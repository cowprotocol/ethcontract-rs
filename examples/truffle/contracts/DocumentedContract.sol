// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;

/**
 * @dev Simple contract with lots of documentation
 *
 * This contract does nothing besides add a bunch of documentation to everything.
 */
contract DocumentedContract {
  /*
   * @dev The owner of this documented contract
   */
  address public owner;

  /*
   * @dev Creates a new owned instance of `DocumentedContract`.
   */
  constructor(address owner_) {
    owner = owner_;
  }

  /*
   * @dev Documented fallback function that does nothing.
   */
  fallback() external { }

  /*
   * @dev Documented function that emits an event.
   *
   * Emits a {Invoked} event.
   */
  function invoke(uint256 value, bool condition) public returns (uint256) {
    uint256 result = 0;
    if (condition && msg.sender == owner) {
      result = value;
    }
    emit Invoked(msg.sender, value);

    return result;
  }

  /*
   * @dev Event emitted when the contract is invoked.
   */
  event Invoked(address indexed from, uint256 result);
}
