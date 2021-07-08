module.exports = async ({getNamedAccounts, deployments}) => {
    const {deployer} = await getNamedAccounts();
    await deployments.deploy('DeployedContract', {from: deployer});
};
