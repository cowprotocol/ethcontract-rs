/**
 * Module and script for transforming compiled truffle artifacts so that we get
 * uniform build outputs regardless of where/when it is built. This is so we
 * can check-in truffle artifacts as well as verify that they were correctly
 * checked-in with CI.
 */

const assert = require("assert");
const fs = require("fs").promises;
const path = require("path");
const yargs = require("yargs");
const injectNetworks = require("@gnosis.pm/util-contracts/src/util/injectNetworks");
const extractNetworks = require("@gnosis.pm/util-contracts/src/util/extractNetworks");

const PACKAGE_ROOT = path.resolve(__dirname, "..");
const NETWORK_RESTORE_CONF = path.join(PACKAGE_ROOT, ".network-restore.conf.js");
const CONTRACTS_ROOT = require(NETWORK_RESTORE_CONF).buildPath;
const NETWORKS_JSON = require(NETWORK_RESTORE_CONF).networkFilePath;
const DEVELOPMENT_NETWORK_ID = 5777;
const ZERO_HASH = "".padEnd(64, "0");

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
 * COBR encode swarm hash.
 */
function cobrEncode(swarmHash, compilerVersion) {
  return `a265627a7a72315820${swarmHash}64736f6c6343${compilerVersion}0032`;
}

/**
 * Sets the COBR encoded swarm hash of HEX encoded bytecode to 0.
 *
 * This is needed for reproduceable builds since, unfortunately, this hash is
 * derived from the metadata string which included absolute paths to source.
 * This means that this value can change dependending on where the contract is
 * compiled from. The end of the bytecode contains 52 bytes of COBR encoded
 * metadata which is the 32 byte swarm hash of the `artifact.metadata` string in
 * bytes [10..41] and a 3 byte encoded solidity compiler version in bytes
 * [48..50].
 *
 * https://solidity.readthedocs.io/en/latest/metadata.html#encoding-of-the-metadata-hash-in-the-bytecode
 */
function clearBytecodeSwarmHash(bytecode) {
  if (bytecode === "" || bytecode === "0x") {
    // empty bytecode, nothing to do
    return bytecode;
  }

  const cobrStart = bytecode.length - 104;
  const swarmHash = bytecode.substr(cobrStart + 18, 64);
  const compilerVersion = bytecode.substr(cobrStart + 94, 6);

  // check that the compiler still behaves as we expect it to:
  assert.equal(
    cobrEncode(swarmHash, compilerVersion),
    bytecode.substr(cobrStart),
    "solc COBR encoded metadata is not as expected");

  return bytecode.substr(0, cobrStart) + cobrEncode(ZERO_HASH, compilerVersion);
}

/**
 * Fixes paths in `solc` metadata string
 */
function fixRelativePaths(metadata) {
  return metadata.replace(new RegExp(PACKAGE_ROOT, "g"), ".");
}

/**
 * Fixes absolute paths from an AST
 */
function fixAstPaths(ast) {
  // this is super hacky but its the most relyable way to remove absolute paths
  // from an AST since they are included in things like imports
  return JSON.parse(
    JSON.stringify(ast)
      .replace(new RegExp(PACKAGE_ROOT, "g"), "")
  );
}

/**
 * Normalizes contract artifacts so that consecutive builds in different
 * environments are identical.
 *
 * Truffle does a few things that make checking-in build artifacts painful. In
 * order to normalize a build artifact we have to:
 * - inject `networks.json` deployed contract information into the artifact
 * - remove the `updatedAt` property from the artifact
 * - update the metadata to use relative paths
 * - clear the swarm hash from the bytecode (this is a known issue and can be
 *   tracked here: https://github.com/trufflesuite/truffle/issues/1621)
 */
async function normalizeArtifacts() {
  const networks = JSON.parse(await fs.readFile(NETWORKS_JSON));
  const artifacts = await getContractArtifacts();
  for await (let { filepath, artifact } of artifacts) {
    artifact = {
      ...artifact,
      metadata: fixRelativePaths(artifact.metadata),
      bytecode: clearBytecodeSwarmHash(artifact.bytecode),
      deployedBytecode: clearBytecodeSwarmHash(artifact.deployedBytecode),
      sourcePath: fixRelativePaths(artifact.sourcePath),
      ast: fixAstPaths(artifact.ast),
      legacyAST: fixAstPaths(artifact.ast),
      networks: networks[artifact.contractName] || {},
      updatedAt: undefined,
    };

    await fs.writeFile(filepath, prettyJSON(artifact));
  }
}

/**
 * Extracts deployed contract addresses and writes them to `networks.json`.
 * Returns a Promise that resolves once complete.
 */
async function extractArtifactNetworks() {
  await extractNetworks(NETWORK_RESTORE_CONF);
}

if (require.main === module) {
  yargs
    .command({
      command: "normalize",
      desc: "nomalizes contract artifacts so that build outputs are identical.",
      handler: normalizeArtifacts,
    })
    .command({
      command: "inject-networks",
      desc: "inject network data from `networks.json` to contract artifacts",
      handler: () => injectNetworks(NETWORK_RESTORE_CONF),
    })
    .command({
      command: "extract-networks",
      desc: "extract deployed contract addresses to `networks.json`",
      handler: () => extractNetworks(NETWORK_RESTORE_CONF),
    })
    .demandCommand()
    .help()
    .argv;
}

module.exports = {
  normalizeArtifacts,
  extractArtifactNetworks,
}
