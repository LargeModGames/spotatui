// The app imports the browser GUI's protocol client and bindings from gui/src.
const path = require('path');
const { getDefaultConfig } = require('expo/metro-config');

const config = getDefaultConfig(__dirname);
const gui = path.resolve(__dirname, '../../gui/src');
config.watchFolders = [gui];
// gui/src has no node_modules of its own: its imports of react resolve here.
config.resolver.nodeModulesPaths = [path.resolve(__dirname, 'node_modules')];

// The tsconfig aliases, resolved here: tsconfig also maps react, for the type checker only.
const aliases = [
  ['@/assets/', path.resolve(__dirname, 'assets')],
  ['@/', path.resolve(__dirname, 'src')],
  ['@gui/', gui],
];
config.resolver.resolveRequest = (context, name, platform) => {
  const alias = aliases.find(([prefix]) => name.startsWith(prefix));
  const target = alias ? path.join(alias[1], name.slice(alias[0].length)) : name;
  return context.resolveRequest(context, target, platform);
};

module.exports = config;
