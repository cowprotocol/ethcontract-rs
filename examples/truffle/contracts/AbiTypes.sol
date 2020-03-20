pragma solidity ^0.5.0;

/**
 * @dev Contract to illustract support for various Solidity types.
 */
contract DeployedContract {
  function getAddress() public view returns (address) {
    return address(this);
  } 

  function getBytes() public view returns (bytes memory) {
    return abi.encodePacked(this.getU32());
  }

  function getU8() public view returns (uint8) {
    return uint8(this.getU256() & 0xff);
  }
  function getU16() public view returns (uint16) {
    return uint16(this.getU256() & 0xffff);
  }
  function getU32() public view returns (uint32) {
    return uint32(this.getU256() & 0xffffffff);
  }
  function getU64() public view returns (uint64) {
    return uint64(this.getU256() & 0xffffffffffffffff);
  }
  function getU128() public view returns (uint128) {
    return uint128(this.getU256() & 0xffffffffffffffffffffffffffffffff);
  }
  function getU256() public view returns (uint256) {
    return uint256(blockhash(block.number - 1));
  }

  function getI8() public view returns (int8) {
    return int8(this.getI256() & 0xff);
  }
  function getI16() public view returns (int16) {
    return int16(this.getI256() & 0xffff);
  }
  function getI32() public view returns (int32) {
    return int32(this.getI256() & 0xffffffff);
  }
  function getI64() public view returns (int64) {
    return int64(this.getI256() & 0xffffffffffffffff);
  }
  function getI128() public view returns (int128) {
    return int128(this.getI256() & 0xffffffffffffffffffffffffffffffff);
  }
  function getI256() public view returns (int256) {
    return int256(this.getU256());
  }

  function getBool() public view returns (bool) {
    return this.getU256() & 0x1 != 0;
  }

  function getString() public view returns (string memory) {
    uint8 value = this.getU8();
    if (value == 0) {
      return "0";
    }
    uint256 j = value;
    uint256 len;
    while (j != 0) {
      len++;
      j /= 10;
    }
    bytes memory bstr = new bytes(len);
    uint256 k = len - 1;
    while (value != 0) {
      bstr[k--] = byte(uint8(48 + value % 10));
      value /= 10;
    }
    return string(bstr);
  }

  function getArray() public view returns (uint64[] memory) {
    uint256 value = this.getU256();
    uint64[] memory buf = new uint64[](4);
    for (uint256 i = 0; i < 32; i++) {
      buf[0] = uint64(value & 0xffffffffffffffff);
      value = value >> 64;
    }
    return buf;
  }

  function getFixedBytes() public view returns (bytes6) {
    if (this.getBool()) {
      return hex"000102030405";
    } else {
      return hex"fffefdfcfbfa";
    }
  }
  function getFixedArray() public view returns (int32[3] memory) {
    uint256 value = this.getU256();
    int32[3] memory buf = [int32(0), int32(0), int32(0)];
    for (uint256 i = 0; i < 32; i++) {
      buf[0] = int32(value & 0xffffffff);
      value = value >> 32;
    }
    return buf;
  }
}
