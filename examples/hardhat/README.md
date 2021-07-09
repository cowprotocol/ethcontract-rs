# HardHat

This subdirectory contains a hardhat project with sample contracts used by the
`ethcontract-rs` crate for its examples and tests.

Information about contracts ABI and their deployments is committed to the
`deployments` directory. You don't need to build them or deploy them to run
any examples.

At the moment, there's only `DeployedContract.sol`, a simple contract that
is deployed on the Rinkeby testnet. It is identical to `DeployedContract.sol`
from truffle directory. 

## Building and Deploying

Building and deploying contracts is done with the same commands as in the
Truffle package.

To build:

```sh
yarn run build
```

To deploy to Rinkeby, export private key and Infura key:

```sh
export PK="private key"
export INFURA_PROJECT_ID="Infura project ID"
```

Then run the deployment script:

```sh
yarn run deploy
```

This will run hardhat deployment process.
