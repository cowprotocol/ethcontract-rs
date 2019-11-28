const HDWalletProvider = require("@truffle/hdwallet-provider");

const {
  PK,
  INFURA_PROJECT_ID,
  ETHERSCAN_API_KEY
} = process.env;

module.exports = {
  networks: {
    develop: {
      host: "127.0.0.1",
      port: 7545,
      network_id: "*",
    },

    rinkeby: {
      provider = () =>
        new HDWalletProvider(PK, `https://rinkeby.infura.io/v3/${INFURA_PROJECT_ID}`),
    },
  },

  mocha: { },

  compilers: {
    solc: { },
  },

  plugins: [
    "truffle-plugin-verify",
  ],

  api_keys: {
    etherscan: process.env.ETHERSCAN_API_KEY,
  },
};
