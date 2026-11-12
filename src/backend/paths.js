// d:/Antigravity/lumn-windows/src/backend/paths.js
const fs = require('fs');

function resolveUnpackedPath(targetPath) {
  if (!targetPath || typeof targetPath !== 'string') return targetPath;
  const unpacked = targetPath.replace(/([\\/])app\.asar([\\/])/i, '$1app.asar.unpacked$2');
  if (unpacked !== targetPath && fs.existsSync(unpacked)) {
    return unpacked;
  }
  return targetPath;
}

module.exports = { resolveUnpackedPath };
