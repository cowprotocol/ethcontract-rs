const fs = require("fs").promises;
const path = require("path");
const yargs = require("yargs");

const PACKAGE_ROOT = path.resolve(__dirname, "..");
const CONTRACTS_ROOT = path.join(PACKAGE_ROOT, "build", "contracts");
const NETWORKS_JSON = path.join(PACKAGE_ROOT, "networks.json");
const DEVELOPMENT_NETWORK_ID = 5777;

/**
 * Gets a list of parsed contract artifact JSON files.
 */
async function getContractArtifacts() {
  return (await fs.readdir(CONTRACTS_ROOT))
    .filter(filename => filename.endsWith(".json"))
    .map(async (filename) => {
      const filepath = path.join(CONTRACTS_ROOT, filename);
      const contents = await fs.readFile(filepath);
      const artifact = JSON.parse(contents.toString());

      return { filepath, artifact };
    })
}

/**
 * Pretty JSON stringify.
 */
function prettyJSON(value) {
  return JSON.stringify(value, undefined, "  ") + '\n';
}

/**
 * Injects the `networks.json` contract deployment information into the contact
 * artifact and returns a Promise that resolves once complete.
 */
async function inject() {
  const networks = JSON.parse(await fs.readFile(NETWORKS_JSON));
  const artifacts = await getContractArtifacts();
  for await (let { filepath, artifact } of artifacts) {
    artifact = {
      ...artifact,
      networks: networks[artifact.contractName] || {},
      updatedAt: undefined,
    };
    await fs.writeFile(filepath, prettyJSON(artifact));
  }
}

/**
 * Injects the `networks.json` contract deployment information into the contact
 * artifact and returns a Promise that resolves once complete.
 */
async function extract() {
  const networks = {};
  const artifacts = await getContractArtifacts();
  for await (let { artifact } of artifacts) {
    const contractNetworks = { ...artifact.networks };

    // don't extract network information for the development network or events
    delete contractNetworks[DEVELOPMENT_NETWORK_ID];
    for (let networkId in contractNetworks) {
      contractNetworks[networkId].events = {};
    }

    if (Object.keys(contractNetworks).length > 0) {
      // only add to the networks JSON if we are deployed
      networks[contract.contractName] = contractNetworks;
    }
  }

  await fs.writeFile(NETWORKS_JSON, prettyJSON(networks));
}

if (require.main === module) {
  yargs
    .command({
      command: "inject",
      desc: "inject `networks.json` into truffle build artifacts",
      handler: inject,
    })
    .command({
      command: "extract",
      desc: "extract deployed contract addresses to `networks.json`",
      handler: extract,
    })
    .demandCommand()
    .help()
    .argv;
}

module.exports = {
  inject,
  extract,
}
