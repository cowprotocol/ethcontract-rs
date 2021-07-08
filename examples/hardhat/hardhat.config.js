require("hardhat-deploy");

const {PK, INFURA_KEY} = process.env;

const sharedNetworkConfig = {
    accounts: [PK],
};

module.exports = {
    solidity: "0.8.0",

    networks: {
        localhost: {
            ...sharedNetworkConfig,
            live: false,
        },
        rinkeby: {
            ...sharedNetworkConfig,
            url: `https://rinkeby.infura.io/v3/${INFURA_KEY}`,
        },
    },

    namedAccounts: {
        deployer: 0,
    },
};
