const HDWalletProvider = require("@truffle/hdwallet-provider");

const {
  PK,
  INFURA_PROJECT_ID,
  ETHERSCAN_API_KEY
} = process.env;

module.exports = {
  networks: {
    rinkeby: {
      provider: () =>
        new HDWalletProvider(PK, `https://rinkeby.infura.io/v3/${INFURA_PROJECT_ID}`),
      network_id: 4,
    },
  },

  mocha: { },

  compilers: {
    solc: {
      version: "^0.8.0",
    },
  },

  plugins: [
    "truffle-plugin-verify",
  ],

  api_keys: {
    etherscan: process.env.ETHERSCAN_API_KEY,
  },
};
