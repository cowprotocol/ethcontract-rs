const DeployedContract = artifacts.require("DeployedContract");

module.exports = async function (deployer) {
  await deployer.deploy(DeployedContract);
};
