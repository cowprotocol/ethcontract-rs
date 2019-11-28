const HDWalletProvider = require("@truffle/hdwallet-provider");

const {
  PK,
  INFURA_PROJECT_ID,
  ETHERSCAN_API_KEY
} = process.env;

module.exports = {
  networks: {
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
