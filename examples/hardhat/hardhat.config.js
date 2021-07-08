require("hardhat-deploy");

const {PK, INFURA_PROJECT_ID} = process.env;

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
            url: `https://rinkeby.infura.io/v3/${INFURA_PROJECT_ID}`,
        },
    },

    namedAccounts: {
        deployer: 0,
    },
};
