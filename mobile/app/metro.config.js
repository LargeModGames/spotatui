// The app imports the browser GUI's protocol client and bindings from gui/src.
const path = require('path');
const { getDefaultConfig } = require('expo/metro-config');

const config = getDefaultConfig(__dirname);
config.watchFolders = [path.resolve(__dirname, '../../gui/src')];

module.exports = config;
