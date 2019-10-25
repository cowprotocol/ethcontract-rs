//! Simple truffle configuration with configured develop network for testing
//! our rust examples.

module.exports = {
  networks: {
    develop: {
      host: "127.0.0.1",
      port: 7545,
      network_id: "*",
    },
  },

  mocha: { },

  compilers: {
    solc: { },
  },
};
