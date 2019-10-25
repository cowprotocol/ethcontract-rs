const SimpleLibrary = artifacts.require("SimpleLibrary");
const LinkedContract = artifacts.require("LinkedContract");

module.exports = async function (deployer) {
  await deployer.deploy(SimpleLibrary);

  deployer.link(SimpleLibrary, LinkedContract);
  await deployer.deploy(LinkedContract);
};
