// Builds project/components/bundle.js and bundle.css from project/components/src.
// Run: node docs/design-system/build-bundle.js
const fs = require('fs');
const path = require('path');
const dir = path.join(__dirname, 'project', 'components');
const src = path.join(dir, 'src');
const read = (f) => fs.readFileSync(path.join(src, f), 'utf8');

const header = '/* @ds-bundle: ' + JSON.stringify({
  format: 4,
  namespace: 'Kandado',
  components: ['Logo', 'Icon', 'Button', 'IconButton', 'TextField', 'Toggle', 'Badge', 'SearchBar', 'VaultChip', 'ItemIcon', 'ItemRow', 'SecretField', 'StrengthMeter', 'PasswordGenerator', 'TotpCode', 'Toast'].map((name) => ({ name }))
}) + ' */\n';
const js = header + read('components.js') + '\n' + read('screens-kit.js');
if (/<\/script|<!--/i.test(js)) throw new Error('bundle.js must not contain </script or <!--');
new Function(js); // syntax check
fs.writeFileSync(path.join(dir, 'bundle.js'), js);
fs.writeFileSync(path.join(dir, 'bundle.css'), read('components.css') + read('screens-kit.css'));
console.log('bundle.js', js.length, 'bytes');
