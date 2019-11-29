const path = require("path");

module.exports = {
  buildPath: path.join(__dirname, "build", "contracts"),
  buildDirDependencies: [],
  networkFilePath: path.join(__dirname, "networks.json"),
};
