/* Kandado screen kit: frames, layouts and shared screen bodies used by the Screens cards.
   Exposed as window.Kandado.Kit. Not a product component; it documents layouts. */
(function () {
  var K = window.Kandado, React = window.React, h = React.createElement, F = React.Fragment;
  function cx() { var o = []; for (var i = 0; i < arguments.length; i++) if (arguments[i]) o.push(arguments[i]); return o.join(' '); }
  function el(tag, cls) { var a = [tag, { className: cls }]; for (var i = 2; i < arguments.length; i++) a.push(arguments[i]); return h.apply(null, a); }
  function div(cls) { var a = ['div', { className: cls }]; for (var i = 1; i < arguments.length; i++) a.push(arguments[i]); return h.apply(null, a); }

  /* ---------- sample data ---------- */
  var D = {
    github: { kind: 'login', letter: 'G', title: 'GitHub', subtitle: 'juan-dc', favorite: true },
    shopee: { kind: 'login', letter: 'S', title: 'Shopee', subtitle: 'juan.delacruz@kandado.me', badge: ['danger', 'Breached'] },
    gcash: { kind: 'login', letter: 'G', title: 'GCash', subtitle: '0917 555 0142', badge: ['warning', 'Reused'] },
    lazada: { kind: 'login', letter: 'L', title: 'Lazada', subtitle: 'juan.delacruz@kandado.me', badge: ['warning', 'Reused'] },
    netflix: { kind: 'login', letter: 'N', title: 'Netflix', subtitle: 'family@kandado.me', badge: ['danger', 'Weak'] },
    facebook: { kind: 'login', letter: 'F', title: 'Facebook', subtitle: 'juan.delacruz', badge: ['warning', 'Missing 2FA'] },
    google: { kind: 'login', letter: 'G', title: 'Google', subtitle: 'juan.delacruz@gmail.com' },
    alias: { kind: 'alias', title: 'Newsletter alias', subtitle: 'quiet.lantern42@kandado.me' },
    alias2: { kind: 'alias', title: 'Shopping alias', subtitle: 'mango.tide@kandado.me' },
    card: { kind: 'card', title: 'BPI Visa', subtitle: '•••• 4821' },
    card2: { kind: 'card', title: 'GoTyme debit', subtitle: '•••• 0193' },
    note: { kind: 'note', title: 'Wi-Fi at home', subtitle: 'Network: Bahay-5G' },
    identity: { kind: 'identity', title: 'Juan Dela Cruz', subtitle: 'Personal identity' },
    password: { kind: 'password', title: 'Generated password', subtitle: 'Created 14 Nov' },
    passkey: { kind: 'passkey', title: 'Google passkey', subtitle: 'accounts.google.com' }
  };
  var LIST = ['github', 'shopee', 'card', 'alias', 'gcash', 'note', 'identity', 'passkey'];
  function Item(key, extra) {
    var d = Object.assign({}, D[key], extra || {});
    if (d.badge && Array.isArray(d.badge)) d.badge = h(K.Badge, { tone: d.badge[0] }, d.badge[1]);
    if (d.nobadge) { delete d.badge; delete d.nobadge; }
    return h(K.ItemRow, Object.assign({ key: key }, d));
  }
  function Items(keys, extra) { return keys.map(function (k) { return Item(k, extra && extra[k]); }); }

  /* ---------- canvas ---------- */
  function Canvas(p) { return div('kt-canvas', p.title && div('kt-canvas-head', el('h2', null, p.title), p.intro && el('p', null, p.intro)), h('div', { className: 'kt-grid', style: { gridTemplateColumns: 'repeat(' + (p.cols || 5) + ', max-content)' } }, p.children)); }
  function Frame(p) {
    return h('figure', { className: cx('kt-frame', p.wide && 'kt-frame-wide'), style: p.span ? { gridColumn: 'span ' + p.span } : null }, p.children,
      h('figcaption', { className: 'kt-cap' }, div('kt-cap-top', p.id && el('span', 'kt-id', p.id), el('b', null, p.label)), p.flow && el('span', 'kt-flow', p.flow)));
  }

  /* ---------- shared bits ---------- */
  function Grp() { return h.apply(null, ['div', { className: 'kd-group' }].concat([].slice.call(arguments))); }
  function Lbl(t, right) { return div('kt-label', el('span', null, t), right); }
  function Fv(label, value, o) { return h(K.SecretField, Object.assign({ label: label, value: value, concealed: false }, o || {})); }
  function Fs(label, value, o) { return h(K.SecretField, Object.assign({ label: label, value: value }, o || {})); }
  function TF(label, value, o) { return h(K.TextField, Object.assign({ label: label, defaultValue: value }, o || {})); }
  function Select(label, value, o) { return h(K.TextField, Object.assign({ label: label, defaultValue: value, readOnly: true, trailing: h(K.Icon, { name: 'chevron-down', size: 18, className: 'kt-sel-ic' }) }, o || {})); }
  function Area(label, value, rows) { return div('kd-field', label && el('label', 'kd-field-label', label), h('textarea', { className: 'kt-area', defaultValue: value, rows: rows || 4 })); }
  function SRow(p) {
    return div(cx('kt-srow', p.danger && 'kt-danger'),
      p.lead, p.icon && h(K.Icon, { name: p.icon, size: 20, className: 'kt-srow-ic' }),
      div('kt-srow-t', div('kt-srow-l', p.label), p.desc && div('kt-srow-d', p.desc)),
      p.badge, p.value && el('span', 'kt-srow-v', p.value),
      p.toggle != null ? h(K.Toggle, { ariaLabel: p.label, defaultChecked: p.toggle }) : null,
      p.radio != null ? el('span', cx('kt-radio', p.radio && 'kt-radio-on')) : null,
      p.check != null ? el('span', cx('kt-check', p.check && 'kt-check-on'), p.check && h(K.Icon, { name: 'check', size: 14 })) : null,
      p.action, p.chev && h(K.Icon, { name: 'chevron', size: 16, className: 'kt-srow-ch' }));
  }
  function Rows(list) { return Grp.apply(null, list.map(function (r, i) { return h(SRow, Object.assign({ key: i }, r)); })); }
  function Chips(list, on) { return div('kt-chips', list.map(function (c, i) { return h('button', { key: c, type: 'button', className: cx('kt-chip', (Array.isArray(on) ? on.indexOf(i) >= 0 : i === (on || 0)) && 'kt-chip-on') }, c); })); }
  function Seg(list, on) { return div('kd-seg', list.map(function (c, i) { return h('button', { key: c, type: 'button', className: cx('kd-seg-btn', i === (on || 0) && 'kd-seg-on') }, c); })); }
  function Banner(p) { return div('kt-banner kt-tone-' + (p.tone || 'teal'), h(K.Icon, { name: p.icon || 'alert', size: 20 }), div('kt-banner-t', p.title && el('b', null, p.title), p.body && el('span', null, p.body)), p.action); }
  function Empty(p) { return div('kt-empty', div('kt-empty-ic kt-tone-' + (p.tone || 'brass'), h(K.Icon, { name: p.icon || 'vault', size: 28 })), el('h3', null, p.title), p.body && el('p', null, p.body), p.children); }
  function Stat(p) { return div('kt-stat kt-tone-' + p.tone, div('kt-stat-top', h(K.Icon, { name: p.icon, size: 18 }), el('span', null, p.label)), el('b', null, p.count)); }
  function Avatar(t, tone) { return el('span', 'kt-avatar kt-tone-' + (tone || 'teal'), t); }
  function Check(on) { return el('span', cx('kt-check', on && 'kt-check-on'), on && h(K.Icon, { name: 'check', size: 14 })); }
  function Pin(n, err) { var d = []; for (var i = 0; i < 6; i++) d.push(el('span', cx('kt-pin-dot', i < n && 'kt-pin-on', err && 'kt-pin-err'), null)); return div('kt-pin', d); }
  function Keypad(bio) {
    var k = ['1', '2', '3', '4', '5', '6', '7', '8', '9', bio ? 'bio' : '', '0', 'del'];
    return div('kt-keypad', k.map(function (x, i) { return h('button', { key: i, type: 'button', className: 'kt-key', 'aria-label': x === 'bio' ? 'Use biometrics' : x === 'del' ? 'Delete' : x || undefined, disabled: !x }, x === 'bio' ? h(K.Icon, { name: 'fingerprint', size: 24 }) : x === 'del' ? h(K.Icon, { name: 'x', size: 22 }) : x); }));
  }
  var WORDS = ['harbor', 'lantern', 'mango', 'quiet', 'cobalt', 'falcon', 'meadow', 'pebble', 'summit', 'anchor', 'saffron', 'willow'];
  function Phrase() { return div('kt-phrase', WORDS.map(function (w, i) { return div('kt-word', el('span', null, i + 1), w); })); }
  function Code(v, n) { var c = []; for (var i = 0; i < 6; i++) c.push(el('span', cx('kt-codebox', i === (n == null ? v.length : n) && 'kt-codebox-on'), v[i] || '')); return div('kt-code', c); }
  function Menu(items, style) {
    return h('div', { className: 'kt-menu', role: 'menu', style: style }, items.map(function (m, i) {
      if (m === '-') return el('div', 'kt-menu-sep', null);
      return div(cx('kt-menu-item', m.danger && 'kt-danger', m.on && 'kt-menu-on'), m.kind ? h(K.ItemIcon, { kind: m.kind, size: 28 }) : m.icon && h(K.Icon, { name: m.icon, size: 18 }), el('span', null, m.label), m.hint && el('kbd', 'kd-kbd', m.hint), m.check && h(K.Icon, { name: 'check', size: 16, className: 'kt-menu-check' }));
    }));
  }
  function Viewfinder() { return div('kt-viewfinder', el('span', 'kt-vf-c kt-vf-tl'), el('span', 'kt-vf-c kt-vf-tr'), el('span', 'kt-vf-c kt-vf-bl'), el('span', 'kt-vf-c kt-vf-br'), div('kt-vf-qr', null)); }

  /* ---------- item bodies (shared by all platforms) ---------- */
  function Meta(more) { return div('kt-meta', el('span', null, 'Created 3 Mar 2025'), el('span', null, 'Modified 12 Oct 2026'), more, el('a', { className: 'kt-link' }, 'View history')); }
  function NoteBox(t) { return Grp(div('kt-notebox', div('kd-secret-label', 'Note'), el('p', null, t))); }
  var Body = {
    login: function () {
      return [Grp(Fv('Username', 'juan-dc', { icon: 'user' }), Fs('Password', 'Tq7#vL2m!Ox0-r9P', { icon: 'key' }), h(K.TotpCode, { code: '482913' })),
        Grp(Fv('Website', 'https://github.com', { icon: 'globe', valueColor: 'var(--link)' }), Fv('Website', 'https://gist.github.com', { icon: 'globe', valueColor: 'var(--link)' })),
        Grp(Fv('Passkey', 'juan-dc · github.com', { icon: 'passkey', copyable: false })),
        NoteBox('Recovery codes are in the "GitHub recovery" note. SSH key added 12 Oct.'),
        Lbl('Custom fields'), Grp(Fv('Recovery email', 'quiet.lantern42@kandado.me', { icon: 'mail' }), Fs('Security answer', 'Bahay kubo', { icon: 'eye' })),
        Meta()];
    },
    alias: function () {
      return [Grp(Fv('Alias address', 'quiet.lantern42@kandado.me', { icon: 'alias', mono: true }), Fv('Forwards to', 'juan.delacruz@gmail.com', { icon: 'mail' })),
        Grp(h(SRow, { icon: 'bolt', label: 'Alias enabled', desc: 'Turn off to stop forwarding without deleting the alias.', toggle: true })),
        div('kt-stats3', h(Stat, { tone: 'teal', icon: 'mail', label: 'Forwarded', count: 128 }), h(Stat, { tone: 'sky', icon: 'share', label: 'Replied', count: 4 }), h(Stat, { tone: 'warning', icon: 'x', label: 'Blocked', count: 37 })),
        NoteBox('Used for newsletters and store sign-ups.'), Meta()];
    },
    card: function () {
      return [Grp(Fv('Cardholder', 'JUAN DELA CRUZ', { icon: 'user' }), Fs('Card number', '4821 0094 3310 4821', { icon: 'card', mono: true }), Fv('Expires', '08 / 29', { icon: 'clock' }), Fs('Security code', '214', { icon: 'shield' }), Fs('PIN', '0912', { icon: 'key' })),
        NoteBox('Credit limit ₱120,000. Statement every 18th.'), Meta()];
    },
    note: function () {
      return [Grp(div('kt-notebox kt-notebox-lg', div('kd-secret-label', 'Note'), el('p', null, 'Network: Bahay-5G'), el('p', null, 'Password: mangga-tide-42'), el('p', null, 'Router admin: 192.168.1.1'), el('p', null, 'Guest network: Bahay-Guest, password on the fridge.'))), Meta()];
    },
    identity: function () {
      return [Lbl('Personal'), Grp(Fv('Full name', 'Juan Dela Cruz'), Fv('Email', 'juan.delacruz@kandado.me'), Fv('Phone', '+63 917 555 0142'), Fv('Birthdate', '14 Feb 1994')),
        Lbl('Address'), Grp(Fv('Street', '12 Mabini St., Brgy. San Roque'), Fv('City', 'Quezon City'), Fv('Postal code', '1109'), Fv('Country', 'Philippines')),
        Lbl('Work'), Grp(Fv('Company', 'Acme Studio'), Fv('Job title', 'Product designer')), Meta()];
    },
    password: function () {
      return [Grp(Fs('Password', 'cobalt-Meadow-anchor-7', { icon: 'key', defaultRevealed: true })), h(K.StrengthMeter, { score: 4, detail: '86 bits' }),
        Grp(Fv('Created on', 'linear.app', { icon: 'globe', copyable: false })), Meta()];
    },
    passkey: function () {
      return [Grp(Fv('Username', 'juan.delacruz@gmail.com', { icon: 'user' }), Fv('Website', 'accounts.google.com', { icon: 'globe', valueColor: 'var(--link)' }), Fv('Created', '2 Jun 2026 on Pixel 8', { icon: 'clock', copyable: false }), Fv('Key ID', 'a91f…3c0e', { icon: 'passkey', mono: true, copyable: false })),
        Banner({ tone: 'teal', icon: 'shield', title: 'Passkeys replace passwords', body: 'Your device signs in with this key. It never leaves Kandado.' }), Meta()];
    }
  };
  var KTITLE = { login: ['github', 'GitHub'], alias: ['alias', 'Newsletter alias'], card: ['card', 'BPI Visa'], note: ['note', 'Wi-Fi at home'], identity: ['identity', 'Juan Dela Cruz'], password: ['password', 'Generated password'], passkey: ['passkey', 'Google passkey'] };

  /* ---------- forms (shared) ---------- */
  var Form = {
    login: function (gen) {
      return [TF('Title', 'GitHub'),
        TF('Username or email', 'juan-dc', { icon: 'user', trailing: h(K.IconButton, { icon: 'alias', size: 'sm', label: 'Use an alias' }) }),
        TF('Password', 'Tq7#vL2m!Ox0-r9P', { icon: 'key', mono: true, trailing: h(K.IconButton, { icon: 'refresh', size: 'sm', label: 'Generate password' }) }),
        h(K.StrengthMeter, { score: 4, detail: '98 bits' }),
        h(K.Button, { icon: 'clock', block: true }, 'Add two-factor code'),
        TF('Website', 'https://github.com', { icon: 'globe' }), h(K.Button, { variant: 'ghost', icon: 'plus', size: 'sm' }, 'Add website'),
        Area('Note', '', 3), h(K.Button, { variant: 'ghost', icon: 'plus', size: 'sm' }, 'Add custom field'),
        Select('Vault', 'Personal', { icon: 'vault' })];
    },
    alias: function () {
      return [TF('Title', 'Newsletter alias'), div('kt-row2', TF('Prefix', 'quiet.lantern42', { mono: true }), Select('Domain', '@kandado.me')),
        Banner({ tone: 'teal', icon: 'alias', title: 'quiet.lantern42@kandado.me', body: 'Mail sent here reaches your mailbox. Your real address stays hidden.' }),
        Select('Forwards to', 'juan.delacruz@gmail.com', { icon: 'mail' }), Area('Note', '', 3), Select('Vault', 'Personal', { icon: 'vault' })];
    },
    card: function () {
      return [TF('Title', 'BPI Visa'), TF('Cardholder name', 'JUAN DELA CRUZ'), TF('Card number', '4821 0094 3310 4821', { mono: true, icon: 'card' }),
        div('kt-row2', TF('Expires', '08/29', { mono: true }), TF('Security code', '214', { mono: true, type: 'password' })), TF('PIN', '0912', { mono: true, type: 'password' }), Area('Note', '', 2)];
    },
    note: function () { return [TF('Title', 'Wi-Fi at home'), Area('Note', 'Network: Bahay-5G\nPassword: mangga-tide-42\nRouter admin: 192.168.1.1', 10)]; },
    identity: function () {
      return [TF('Title', 'Juan Dela Cruz'), Lbl('Personal'), TF('Full name', 'Juan Dela Cruz'), TF('Email', 'juan.delacruz@kandado.me'), TF('Phone', '+63 917 555 0142'),
        Lbl('Address'), TF('Street', '12 Mabini St., Brgy. San Roque'), div('kt-row2', TF('City', 'Quezon City'), TF('Postal code', '1109')), Select('Country', 'Philippines'),
        h(K.Button, { variant: 'ghost', icon: 'plus', size: 'sm' }, 'Add section')];
    }
  };

  /* ---------- feature bodies (shared) ---------- */
  var Feat = {
    monitor: function () {
      return [Banner({ tone: 'danger', icon: 'alert', title: '10 items need attention', body: 'Start with the breached ones.' }),
        div('kt-stats2', h(Stat, { tone: 'danger', icon: 'alert', label: 'Breached', count: 1 }), h(Stat, { tone: 'danger', icon: 'key', label: 'Weak', count: 2 }), h(Stat, { tone: 'warning', icon: 'copy', label: 'Reused', count: 3 }), h(Stat, { tone: 'warning', icon: 'clock', label: 'Missing 2FA', count: 4 })),
        Rows([{ icon: 'alert', label: 'Breached', desc: 'Found in a data breach', badge: h(K.Badge, { tone: 'danger' }, '1'), chev: true }, { icon: 'key', label: 'Weak passwords', badge: h(K.Badge, { tone: 'danger' }, '2'), chev: true }, { icon: 'copy', label: 'Reused passwords', badge: h(K.Badge, { tone: 'warning' }, '3'), chev: true }, { icon: 'clock', label: 'Missing two-factor', badge: h(K.Badge, { tone: 'warning' }, '4'), chev: true }]),
        Lbl('Email monitoring'), Rows([{ icon: 'mail', label: 'juan.delacruz@gmail.com', desc: 'Found in 2 breaches', badge: h(K.Badge, { tone: 'danger' }, '2'), chev: true }, { icon: 'alias', label: '18 aliases monitored', desc: 'No breaches', badge: h(K.Badge, { tone: 'teal' }, 'Healthy'), chev: true }])];
    },
    breach: function () {
      return [div('kt-hero', h(K.ItemIcon, { kind: 'login', letter: 'S', size: 56 }), el('h2', null, 'Shopee'), h(K.Badge, { tone: 'danger' }, 'Breached 12 Sep 2026')),
        el('p', 'kt-p', 'A database with Shopee accounts was published online. Your password for this site should be treated as known.'),
        Lbl('Exposed data'), Chips(['Email', 'Password', 'Phone number', 'Name'], [0, 1, 2, 3]),
        Lbl('Affected items'), Grp(Item('shopee', { nobadge: true })),
        Lbl('What to do'), Rows([{ check: true, label: 'Change the password on shopee.ph' }, { check: false, label: 'Turn on two-factor for Shopee' }, { check: false, label: 'Change it anywhere you reused it' }]),
        h(K.Button, { variant: 'primary', icon: 'globe', block: true }, 'Change password on shopee.ph'), h(K.Button, { block: true }, 'Mark as resolved')];
    },
    weak: function () { return [el('p', 'kt-p', 'These passwords are easy to guess. Generate a new one for each.'), Grp(Item('netflix', { onCopy: null }), Item('facebook', { badge: ['danger', 'Weak'] }))]; },
    reused: function () {
      return [el('p', 'kt-p', 'One leak exposes every site that shares a password.'), Lbl('Same password · 3 items'), Grp(Item('gcash'), Item('lazada'), Item('netflix', { badge: ['warning', 'Reused'] })),
        Lbl('Same password · 2 items'), Grp(Item('google', { badge: ['warning', 'Reused'] }), Item('facebook', { badge: ['warning', 'Reused'] }))];
    },
    twofa: function () {
      return [el('p', 'kt-p', 'These sites support two-factor codes. Add one to stop sign-ins with a stolen password.'),
        Grp(Item('facebook', { badge: h(K.Button, { size: 'sm' }, 'Add code') }), Item('google', { badge: h(K.Button, { size: 'sm' }, 'Add code') }), Item('lazada', { badge: h(K.Button, { size: 'sm' }, 'Add code') }), Item('shopee', { badge: h(K.Button, { size: 'sm' }, 'Add code') }))];
    },
    emails: function () {
      return [el('p', 'kt-p', 'Kandado checks these addresses against new breaches every day.'),
        Rows([{ icon: 'mail', label: 'juan.delacruz@gmail.com', desc: 'Account email · 2 breaches', toggle: true }, { icon: 'mail', label: 'juan@acme.studio', desc: 'Added 4 Jan · no breaches', toggle: true }, { icon: 'alias', label: 'Aliases (18)', desc: 'No breaches', toggle: true }]),
        Lbl('Breaches for juan.delacruz@gmail.com'), Rows([{ icon: 'alert', label: 'Shopee', desc: '12 Sep 2026 · email, password', chev: true }, { icon: 'alert', label: 'Canva', desc: '24 May 2019 · email, name', chev: true }]),
        h(K.Button, { icon: 'plus' }, 'Add address')];
    },
    vaults: function () {
      return [Rows([
        { icon: 'vault', label: 'Personal', desc: '128 items', action: h(K.IconButton, { icon: 'more', size: 'sm', label: 'Vault actions' }) },
        { icon: 'user', label: 'Family', desc: '52 items · shared with 2', badge: h(K.Badge, { tone: 'brass', icon: 'share' }, 'Shared'), action: h(K.IconButton, { icon: 'more', size: 'sm', label: 'Vault actions' }) },
        { icon: 'bolt', label: 'Work', desc: '34 items · you are an editor', badge: h(K.Badge, { tone: 'brass', icon: 'share' }, 'Shared'), action: h(K.IconButton, { icon: 'more', size: 'sm', label: 'Vault actions' }) }]),
        h(K.Button, { icon: 'plus', block: true }, 'Create vault')];
    },
    vaultForm: function () {
      var tones = ['brass', 'teal', 'coral', 'sky', 'plum', 'moss'], icons = ['vault', 'user', 'bolt', 'card', 'star', 'shield', 'globe', 'key'];
      return [TF('Name', 'Family'), Lbl('Colour'), div('kt-swatches', tones.map(function (t, i) { return el('span', cx('kt-swatch', i === 2 && 'kt-swatch-on'), h('span', { style: { background: 'var(--' + t + ')' } })); })),
        Lbl('Icon'), div('kt-icongrid', icons.map(function (n, i) { return el('span', cx('kt-iconcell', i === 1 && 'kt-iconcell-on'), h(K.Icon, { name: n, size: 20 })); }))];
    },
    shareVault: function () {
      return [TF('Invite by email', 'maria.delacruz@gmail.com', { icon: 'mail' }), Lbl('Role'), Seg(['Viewer', 'Editor', 'Manager'], 1),
        el('p', 'kt-p kt-small', 'Editors can add, change and delete items. Managers can also invite people.'),
        Lbl('People with access'), Rows([{ lead: Avatar('J'), label: 'Juan Dela Cruz (you)', desc: 'Owner' }, { lead: Avatar('M', 'coral'), label: 'Maria D.', desc: 'Editor', chev: true }])];
    },
    members: function () {
      return [Lbl('Members · 3'), Rows([{ lead: Avatar('J'), label: 'Juan Dela Cruz (you)', value: 'Owner' }, { lead: Avatar('M', 'coral'), label: 'Maria D.', desc: 'maria.delacruz@gmail.com', value: 'Editor', chev: true }, { lead: Avatar('P', 'sky'), label: 'Pedro D.', desc: 'pedro.dc@yahoo.com', value: 'Viewer', chev: true }]),
        Lbl('Pending invites'), Rows([{ lead: Avatar('L', 'plum'), label: 'lola.rosa@gmail.com', desc: 'Sent 2 days ago', badge: h(K.Button, { size: 'sm', variant: 'ghost' }, 'Resend') }]),
        h(K.Button, { icon: 'plus', block: true }, 'Invite people'), h(K.Button, { variant: 'danger-quiet', icon: 'logout', block: true }, 'Leave vault'), h(K.Button, { variant: 'danger-quiet', icon: 'trash', block: true }, 'Delete vault')];
    },
    invite: function () {
      return [div('kt-hero', Avatar('M', 'coral'), el('h2', null, 'Maria D. invited you to Family'), el('p', 'kt-p', 'You’ll join as an editor. 52 items.')),
        Grp(h(SRow, { icon: 'user', label: 'Family', desc: 'Shared vault · 3 members' })), h(K.Button, { variant: 'primary', block: true, size: 'lg' }, 'Accept'), h(K.Button, { block: true, size: 'lg' }, 'Decline')];
    },
    moveTo: function () { return [Rows([{ icon: 'vault', label: 'Personal', desc: 'Current vault', radio: true }, { icon: 'user', label: 'Family', radio: false }, { icon: 'bolt', label: 'Work', radio: false }])]; },
    shareItem: function () { return [Rows([{ icon: 'user', label: 'Share with people', desc: 'They need a Kandado account', chev: true }, { icon: 'link', label: 'Create a secure link', desc: 'Anyone with the link can view it', chev: true }])]; },
    linkCreate: function () {
      return [Grp(Item('github')), Lbl('Link expires after'), Seg(['1 hour', '1 day', '7 days', '30 days'], 2),
        Rows([{ icon: 'eye', label: 'Limit views', desc: 'The link stops working after this many opens', toggle: true }]), TF('Maximum views', '3', { mono: true })];
    },
    linkReady: function () {
      return [div('kt-hero', div('kt-empty-ic kt-tone-teal', h(K.Icon, { name: 'link', size: 28 })), el('h2', null, 'Secure link ready'), el('p', 'kt-p', 'Expires 22 Nov · 3 views')),
        Grp(Fv('Link', 'https://kandado.me/s/7Hq2…xP', { mono: true, icon: 'link' })), h(K.Button, { variant: 'primary', icon: 'copy', block: true }, 'Copy link'), h(K.Button, { icon: 'share', block: true }, 'Share')];
    },
    links: function () {
      return [Rows([{ lead: h(K.ItemIcon, { kind: 'login', letter: 'G', size: 32 }), label: 'GitHub', desc: 'Expires 22 Nov · 1 of 3 views', badge: h(K.Button, { size: 'sm', variant: 'danger-quiet' }, 'Revoke') },
        { lead: h(K.ItemIcon, { kind: 'note', size: 32 }), label: 'Wi-Fi at home', desc: 'Expires in 3 hours · no limit', badge: h(K.Button, { size: 'sm', variant: 'danger-quiet' }, 'Revoke') },
        { lead: h(K.ItemIcon, { kind: 'card', size: 32 }), label: 'BPI Visa', desc: 'Expired 10 Nov', badge: h(K.Badge, null, 'Expired') }])];
    },
    trash: function () {
      return [Banner({ tone: 'warning', icon: 'clock', body: 'Items are deleted forever 30 days after you move them here.' }),
        Grp(Item('alias2', { subtitle: 'Deleted 3 days ago · 27 days left', badge: h(K.Button, { size: 'sm', variant: 'ghost' }, 'Restore') }), Item('card2', { subtitle: 'Deleted 12 days ago · 18 days left', badge: h(K.Button, { size: 'sm', variant: 'ghost' }, 'Restore') }), Item('lazada', { subtitle: 'Deleted 29 days ago · 1 day left', badge: h(K.Button, { size: 'sm', variant: 'ghost' }, 'Restore') })),
        h(K.Button, { variant: 'danger-quiet', icon: 'trash', block: true }, 'Empty trash')];
    },
    history: function () {
      return [Rows([{ icon: 'clock', label: 'Today, 09:41', desc: 'Current version', radio: false }, { icon: 'clock', label: '12 Oct 2026', desc: 'Password changed', radio: true }, { icon: 'clock', label: '3 Jun 2026', desc: 'Website added', radio: false }, { icon: 'clock', label: '3 Mar 2025', desc: 'Created', radio: false }]),
        Lbl('12 Oct 2026 version'), Grp(Fv('Username', 'juan-dc'), Fs('Password', 'gH8!kd-2LmQ', { defaultRevealed: true })), h(K.Button, { variant: 'primary', block: true }, 'Restore this version')];
    }
  };

  /* ---------- settings bodies (shared) ---------- */
  var Set = {
    root: function () {
      return [Grp(div('kt-account', Avatar('J'), div('kt-srow-t', div('kt-srow-l', 'Juan Dela Cruz'), div('kt-srow-d', 'juan.delacruz@gmail.com')), h(K.Badge, { tone: 'brass', icon: false }, 'Plus'))),
        Rows([{ icon: 'lock', label: 'Security', chev: true }, { icon: 'key', label: 'Autofill', chev: true }, { icon: 'eye', label: 'Appearance', chev: true }, { icon: 'alias', label: 'Aliases', chev: true }, { icon: 'vault', label: 'Vaults', chev: true }, { icon: 'link', label: 'Secure links', chev: true }, { icon: 'trash', label: 'Trash', chev: true }]),
        Rows([{ icon: 'refresh', label: 'Import', chev: true }, { icon: 'share', label: 'Export', chev: true }, { icon: 'grid', label: 'Devices', chev: true }, { icon: 'user', label: 'Account', chev: true }]),
        Rows([{ icon: 'logout', label: 'Sign out', danger: true }]), el('p', 'kt-p kt-small kt-center', 'Kandado 1.0.0 · Help · Privacy')];
    },
    security: function () {
      return [Lbl('Unlock'), Rows([{ icon: 'fingerprint', label: 'Unlock with biometrics', toggle: true }, { icon: 'key', label: 'Change PIN', chev: true }, { icon: 'lock', label: 'Extra password', desc: 'Ask for a second password after unlocking', toggle: false }]),
        Lbl('Locking'), Rows([{ icon: 'clock', label: 'Lock after', value: '5 minutes', chev: true }, { icon: 'lock', label: 'Lock when device sleeps', toggle: true }, { icon: 'copy', label: 'Clear clipboard after', value: '60 seconds', chev: true }]),
        Lbl('Recovery'), Rows([{ icon: 'note', label: 'View recovery phrase', chev: true }, { icon: 'shield', label: 'Two-factor for your account', value: 'On', chev: true }])];
    },
    autofill: function () {
      return [Banner({ tone: 'teal', icon: 'check', title: 'Kandado is your autofill service', body: 'Change it in system settings.', action: h(K.Button, { size: 'sm', variant: 'ghost' }, 'Open') }),
        Rows([{ icon: 'key', label: 'Autofill logins', toggle: true }, { icon: 'card', label: 'Autofill cards', toggle: true }, { icon: 'id', label: 'Autofill identities', toggle: true }, { icon: 'clock', label: 'Autofill two-factor codes', desc: 'Copy the code after filling a login', toggle: true }]),
        Rows([{ icon: 'alias', label: 'Suggest aliases in email fields', toggle: true }, { icon: 'refresh', label: 'Suggest passwords in new-password fields', toggle: true }, { icon: 'plus', label: 'Offer to save new logins', toggle: true }, { icon: 'passkey', label: 'Use Kandado for passkeys', toggle: true }]),
        Rows([{ icon: 'x', label: 'Never fill on these sites', value: '3', chev: true }])];
    },
    appearance: function () {
      return [Lbl('Theme'), Seg(['System', 'Dark', 'Light'], 0), Lbl('List'), Rows([{ icon: 'globe', label: 'Show website icons', desc: 'Loads icons through Kandado’s proxy. Sites never see you.', toggle: false }, { icon: 'grid', label: 'Compact rows', toggle: false }]),
        Rows([{ icon: 'globe', label: 'Language', value: 'English', chev: true }])];
    },
    aliases: function () {
      return [div('kt-stats2', h(Stat, { tone: 'moss', icon: 'alias', label: 'Aliases', count: 18 }), h(Stat, { tone: 'teal', icon: 'mail', label: 'Forwarded this month', count: 412 })),
        Lbl('Defaults'), Rows([{ icon: 'globe', label: 'Default domain', value: '@kandado.me', chev: true }, { icon: 'mail', label: 'Default mailbox', value: 'juan.delacruz@gmail.com', chev: true }]),
        Lbl('Mailboxes'), Rows([{ icon: 'mail', label: 'juan.delacruz@gmail.com', badge: h(K.Badge, { tone: 'teal' }, 'Verified') }, { icon: 'mail', label: 'juan@acme.studio', badge: h(K.Badge, { tone: 'warning' }, 'Unverified'), action: h(K.Button, { size: 'sm', variant: 'ghost' }, 'Verify') }]),
        h(K.Button, { icon: 'plus', block: true }, 'Add mailbox'), Lbl('Custom domains'), Rows([{ icon: 'globe', label: 'delacruz.ph', desc: 'Verified · catch-all on', chev: true }])];
    },
    importSrc: function () {
      return [Lbl('Import from'), Rows([{ icon: 'shield', label: 'Proton Pass', desc: 'ZIP or CSV export', chev: true }, { icon: 'shield', label: 'Bitwarden', desc: 'JSON or CSV', chev: true }, { icon: 'shield', label: '1Password', desc: '1PUX', chev: true }, { icon: 'shield', label: 'LastPass', desc: 'CSV', chev: true }, { icon: 'globe', label: 'Chrome, Edge, Firefox, Safari', desc: 'CSV', chev: true }, { icon: 'note', label: 'Other CSV', chev: true }])];
    },
    importRun: function () {
      return [div('kt-drop', h(K.Icon, { name: 'note', size: 28 }), el('b', null, 'proton-pass-export.zip'), el('span', null, '2.4 MB · 214 items found')), Select('Into vault', 'Personal', { icon: 'vault' }),
        div('kt-progress', h('span', { style: { width: '68%' } })), el('p', 'kt-p kt-small', 'Importing 146 of 214…'),
        Banner({ tone: 'teal', icon: 'check', title: 'Last import: 210 imported, 4 skipped', body: '4 items had no title or password.', action: h(K.Button, { size: 'sm', variant: 'ghost' }, 'Review') })];
    },
    exportBody: function () {
      return [Lbl('Format'), Seg(['Encrypted', 'JSON', 'CSV'], 0), el('p', 'kt-p kt-small', 'Encrypted exports need your account password to open. Only Kandado can import them.'),
        Banner({ tone: 'warning', icon: 'alert', title: 'JSON and CSV are not encrypted', body: 'Anyone with the file can read every password.' }),
        TF('Confirm your account password', 'secret-pass', { type: 'password', icon: 'lock' }), h(K.Button, { variant: 'primary', icon: 'share', block: true }, 'Export 214 items')];
    },
    devices: function () {
      return [Rows([{ icon: 'grid', label: 'Pixel 8', desc: 'Android · Quezon City · now', badge: h(K.Badge, { tone: 'teal', icon: false }, 'This device') }, { icon: 'grid', label: 'Windows desktop app', desc: 'Windows 11 · 2 hours ago', action: h(K.Button, { size: 'sm', variant: 'ghost' }, 'Sign out') }, { icon: 'globe', label: 'Chrome extension', desc: 'Windows 11 · 2 hours ago', action: h(K.Button, { size: 'sm', variant: 'ghost' }, 'Sign out') }, { icon: 'grid', label: 'iPad', desc: 'iPadOS · 12 days ago', action: h(K.Button, { size: 'sm', variant: 'ghost' }, 'Sign out') }]),
        h(K.Button, { variant: 'danger-quiet', icon: 'logout', block: true }, 'Sign out all other devices')];
    },
    account: function () {
      return [Rows([{ icon: 'mail', label: 'Email', value: 'juan.delacruz@gmail.com', chev: true }, { icon: 'star', label: 'Plan', value: 'Plus', chev: true }, { icon: 'lock', label: 'Change account password', chev: true }, { icon: 'shield', label: 'Two-factor for your account', value: 'Authenticator app', chev: true }, { icon: 'note', label: 'Recovery phrase', chev: true }]),
        Rows([{ icon: 'trash', label: 'Delete account', desc: 'Deletes every vault you own. This can’t be undone.', danger: true }])];
    },
    shortcuts: function () {
      var s = [['Search', 'Ctrl F'], ['Create item', 'Ctrl N'], ['Copy username', 'Ctrl B'], ['Copy password', 'Ctrl C'], ['Copy two-factor code', 'Ctrl T'], ['Edit item', 'Ctrl E'], ['Lock', 'Ctrl L'], ['Move to trash', 'Delete'], ['Next / previous item', '↓ / ↑']];
      return [Grp.apply(null, s.map(function (x) { return h(SRow, { key: x[0], label: x[0], action: el('kbd', 'kd-kbd', x[1]) }); }))];
    },
    excluded: function () {
      return [TF('Add a site', '', { icon: 'globe', placeholder: 'example.com', trailing: h(K.Button, { size: 'sm' }, 'Add') }),
        Rows([{ icon: 'globe', label: 'bank.bpi.com.ph', desc: 'Never fill · never save', action: h(K.IconButton, { icon: 'x', size: 'sm', label: 'Remove' }) }, { icon: 'globe', label: 'intranet.acme.studio', desc: 'Never save', action: h(K.IconButton, { icon: 'x', size: 'sm', label: 'Remove' }) }, { icon: 'globe', label: 'localhost', desc: 'Never fill · never save', action: h(K.IconButton, { icon: 'x', size: 'sm', label: 'Remove' }) }])];
    }
  };

  /* ---------- overlays ---------- */
  function Sheet(p) { return h(F, null, el('div', 'kt-scrim', null), h('div', { className: 'kt-sheet', role: 'dialog' }, el('div', 'kt-grab', null), p.title && div('kt-sheet-head', el('b', null, p.title), p.right), div('kt-sheet-body', p.children))); }
  function Dialog(p) {
    return h(F, null, el('div', 'kt-scrim', null), h('div', { className: cx('kt-dialog', p.wide && 'kt-dialog-wide'), role: 'dialog' },
      p.icon && div('kt-dialog-ic kt-tone-' + (p.tone || 'brass'), h(K.Icon, { name: p.icon, size: 24 })),
      div('kt-dialog-title', p.title), p.body && div('kt-dialog-body', p.body), p.children && div('kt-dialog-content', p.children),
      p.actions && div(cx('kt-dialog-acts', p.stack && 'kt-stack'), p.actions.map(function (a, i) { return h(K.Button, { key: i, variant: a[1] || 'secondary', block: p.stack }, a[0]); }))));
  }
  function Toasts(list) { return div('kt-toasts', list); }

  /* ---------- mobile ---------- */
  var TABS = [['grid', 'Items'], ['search', 'Search'], ['shield', 'Monitor'], ['settings', 'Settings']];
  function Phone(p) {
    return div(cx('kt-phone', p.os === 'ios' && 'kt-ios', p.full && 'kt-phone-full'),
      div('kt-status', el('span', null, '9:41'), div('kt-status-r', el('span', null, '5G'), el('span', 'kt-batt', null))),
      div('kt-scr', p.children),
      p.tabs != null && h('nav', { className: 'kt-tabs' }, TABS.map(function (t, i) { return div(cx('kt-tab', i === p.tabs && 'kt-tab-on'), h(K.Icon, { name: t[0], size: 22 }), t[1]); })),
      p.fab && div('kt-fab', h(K.IconButton, { icon: 'plus', label: 'Create item', variant: 'primary', size: 'lg' })),
      p.overlay, el('div', 'kt-home', null));
  }
  function Top(p) {
    return div(cx('kt-top', p.large && 'kt-top-lg'),
      p.back && h(K.IconButton, { icon: p.back === 'x' ? 'x' : 'chevron', label: p.back === 'x' ? 'Close' : 'Back', className: p.back === 'x' ? null : 'kt-flip' }),
      div('kt-top-title', p.title), p.right,
      (p.actions || []).map(function (a, i) { return h(K.IconButton, { key: i, icon: a, label: a }); }));
  }
  function MBody(p) { return h('div', { className: cx('kt-mbody', p.center && 'kt-center-col'), style: p.style }, p.children); }
  function MFoot(p) { return div('kt-mfoot', p.children); }
  function MDetail(kind) {
    var t = KTITLE[kind], d = D[t[0]];
    return h(F, null, h(Top, { back: true, actions: ['star', 'more'] }),
      h(MBody, null, div('kt-hero', h(K.ItemIcon, { kind: d.kind, letter: d.letter, size: 56 }), el('h2', null, d.title), div('kt-hero-sub', h(K.Icon, { name: 'vault', size: 14 }), 'Personal', kind === 'login' && h(K.Badge, { tone: 'teal' }, 'Strong'))), Body[kind]()),
      h(MFoot, null, h(K.Button, { icon: 'share', size: 'lg', block: true }, 'Share'), h(K.Button, { variant: 'primary', size: 'lg', block: true }, 'Edit')));
  }
  function MForm(kind, title) { return h(F, null, h(Top, { back: 'x', title: title, right: h(K.Button, { variant: 'primary', size: 'sm' }, 'Save') }), h(MBody, null, Form[kind]())); }
  function MPage(title, body, o) { o = o || {}; return h(F, null, h(Top, { back: o.back !== false, title: title, right: o.right, large: o.large }), h(MBody, null, body), o.foot && h(MFoot, null, o.foot)); }
  function ItemsList(o) {
    o = o || {};
    return h(F, null, h(Top, { title: 'Items', large: true, right: h(K.VaultChip, { name: 'Personal', dropdown: true }) }),
      h(MBody, { style: { gap: 10 } }, o.banner, h(K.SearchBar, { placeholder: 'Search in Personal' }), Chips(['All', 'Logins', 'Aliases', 'Cards', 'Notes', 'Identities']), div('kt-list', Items(o.keys || LIST))));
  }

  /* ---------- desktop ---------- */
  function Split(arr) {
    if (!Array.isArray(arr) || arr.length < 4) return div('kt-dcol', arr);
    var n = Math.ceil(arr.length / 2);
    return div('kt-split', div('kt-dcol', arr.slice(0, n)), div('kt-dcol', arr.slice(n)));
  }
  var SIDE = [['grid', 'All vaults', 214], ['vault', 'Personal', 128, 'brass'], ['user', 'Family', 52, 'coral'], ['bolt', 'Work', 34, 'sky']];
  function MonList(active) {
    var c = [['shield', 'Overview'], ['alert', 'Breached', 1], ['key', 'Weak passwords', 2], ['copy', 'Reused passwords', 3], ['clock', 'Missing two-factor', 4], ['mail', 'Email monitoring', 2]];
    return h('section', { className: 'kt-dlist kt-snav' }, div('kt-dlist-head', el('h2', null, 'Monitor')), div('kt-dlist-rows', c.map(function (x) { return div(cx('kt-nav', x[1] === active && 'kt-nav-on'), h(K.Icon, { name: x[0], size: 20 }), el('span', 'kt-nav-l', x[1]), x[2] != null && el('span', 'kt-nav-n', x[2])); })));
  }
  var TOOLS = [['shield', 'Monitor', 10], ['alias', 'Aliases', 18], ['link', 'Secure links', 3], ['trash', 'Trash', 3]];
  function Side(p) {
    var a = p.active || 'Personal';
    function nav(x, k) { return div(cx('kt-nav', x[1] === a && 'kt-nav-on'), x[3] ? el('span', 'kt-vdot', h(K.Icon, { name: x[0], size: 14 }), null) : h(K.Icon, { name: x[0], size: 20 }), el('span', 'kt-nav-l', x[1]), x[2] != null && el('span', 'kt-nav-n', x[2])); }
    if (p.rail) return h('aside', { className: 'kt-side kt-rail' }, h(K.IconButton, { icon: 'plus', label: 'Create item', variant: 'primary' }), SIDE.concat(TOOLS).map(function (x) { return h(K.IconButton, { key: x[1], icon: x[0], label: x[1], className: x[1] === a ? 'kt-rail-on' : null }); }), div('kt-grow', null), h(K.IconButton, { icon: 'settings', label: 'Settings' }));
    return h('aside', { className: 'kt-side' }, h(K.Button, { variant: 'primary', icon: 'plus', block: true }, 'Create item'), el('h6', null, 'Vaults'), SIDE.map(nav), el('h6', null, 'Tools'), TOOLS.map(nav),
      div('kt-side-foot', Avatar('J'), div('kt-who', el('b', null, 'Juan Dela Cruz'), el('span', null, p.offline ? 'Offline · changes saved locally' : 'Synced just now')), h(K.IconButton, { icon: 'settings', size: 'sm', label: 'Settings' })));
  }
  function DList(p) {
    return h('section', { className: 'kt-dlist' }, div('kt-dlist-head', el('h2', null, p.title || 'Personal'), p.head || h(K.IconButton, { icon: 'settings', label: 'Sort and filter', size: 'sm' })), p.chips !== false && Chips(['All', 'Logins', 'Aliases', 'Cards', 'Notes'], p.chip || 0),
      div('kt-dlist-rows', p.children || Items(p.keys || LIST, p.extra)));
  }
  function Win(p) {
    return div('kt-win',
      div('kt-titlebar', h(K.Logo, { size: 22, wordmark: true }), div('kt-tb-search', p.search || h(K.SearchBar, { shortcut: 'Ctrl F' })), div('kt-tb-r', h(K.IconButton, { icon: 'lock', label: 'Lock now', size: 'sm' }), el('span', 'kt-winctl', '–  □  ✕'))),
      p.banner, div('kt-wbody', p.side === false ? null : h(Side, { active: p.active, rail: p.rail, offline: p.offline }), p.list, h('main', { className: cx('kt-dmain', p.mainClass) }, p.children)), p.overlay);
  }
  function DHead(kind, actions) {
    var t = KTITLE[kind], d = D[t[0]];
    return div('kt-dhead', h(K.ItemIcon, { kind: d.kind, letter: d.letter, size: 56 }), div(null, el('h1', null, d.title), div('kt-hero-sub', h(K.Icon, { name: 'vault', size: 14 }), 'Personal')),
      div('kt-dhead-acts', actions || [h(K.IconButton, { key: 's', icon: 'star', label: 'Favourite', variant: 'secondary' }), h(K.Button, { key: 'h', icon: 'share' }, 'Share'), h(K.Button, { key: 'e', variant: 'primary' }, 'Edit'), h(K.IconButton, { key: 'm', icon: 'more', label: 'More actions', variant: 'secondary' })]));
  }
  function DDetail(kind, o) {
    o = o || {};
    var sel = KTITLE[kind][0], keys = LIST.indexOf(sel) >= 0 ? LIST : [sel].concat(LIST.slice(0, 7)), extra = {}; extra[sel] = { selected: true };
    return h(Win, { list: h(DList, { keys: keys, extra: extra }), overlay: o.overlay }, DHead(kind), Split(Body[kind]()));
  }
  function DForm(kind, title, o) {
    o = o || {};
    var extra = {}; extra[KTITLE[kind][0]] = { selected: true };
    return h(Win, { list: h(DList, { extra: extra }), overlay: o.overlay }, div('kt-dhead', el('h1', null, title), div('kt-dhead-acts', h(K.Button, null, 'Cancel'), h(K.Button, { variant: 'primary' }, 'Save'))), Split(Form[kind]()), o.pop);
  }
  function DPage(title, body, o) {
    o = o || {};
    return h(Win, { active: o.active, list: o.list, overlay: o.overlay, banner: o.banner, offline: o.offline }, div('kt-dhead', el('h1', null, title), o.sub && el('p', 'kt-p', o.sub), div('kt-dhead-acts', o.actions)), div('kt-dcol', body));
  }
  var SNAV = [['settings', 'General'], ['lock', 'Security'], ['key', 'Autofill and browser'], ['alias', 'Aliases'], ['refresh', 'Import'], ['share', 'Export'], ['grid', 'Devices'], ['user', 'Account']];
  function DSettings(active, body, overlay) {
    return h(Win, { side: false, overlay: overlay, list: h('section', { className: 'kt-dlist kt-snav' }, div('kt-dlist-head', h(K.IconButton, { icon: 'chevron', label: 'Back to vault', size: 'sm', className: 'kt-flip' }), el('h2', null, 'Settings')),
      div('kt-dlist-rows', SNAV.map(function (s) { return div(cx('kt-nav', s[1] === active && 'kt-nav-on'), h(K.Icon, { name: s[0], size: 20 }), el('span', 'kt-nav-l', s[1])); }), div('kt-grow', null), div(cx('kt-nav', 'kt-danger'), h(K.Icon, { name: 'logout', size: 20 }), el('span', 'kt-nav-l', 'Sign out')))) },
      div('kt-dhead', el('h1', null, active)), Split(body));
  }

  /* ---------- extension ---------- */
  function Pop(p) { return div('kt-pop', p.children, p.overlay); }
  function PopHead(p) {
    p = p || {};
    if (p.back) return div('kt-phead', h(K.IconButton, { icon: 'chevron', label: 'Back', size: 'sm', className: 'kt-flip' }), div('kt-top-title kt-small-title', p.title), p.right);
    return div('kt-phead', h(K.Logo, { size: 26 }), h(K.VaultChip, { name: 'All vaults', icon: 'grid', dropdown: true }), div('kt-grow', null), h(K.IconButton, { icon: 'lock', label: 'Lock now', size: 'sm' }), h(K.IconButton, { icon: 'more', label: 'Menu', size: 'sm' }));
  }
  function PBody(p) { return h('div', { className: 'kt-pbody', style: p.style }, p.children); }
  function PFoot(p) { return div('kt-pfoot', p.children || [h(K.Button, { key: 'c', variant: 'primary', icon: 'plus', block: true }, 'Create'), h(K.Button, { key: 'g', icon: 'key', block: true }, 'Generate')]); }
  function Browser(p) {
    return h('div', { className: 'kt-browser', style: p.h ? { height: p.h } : null }, div('kt-chrome', el('span', 'kt-dots', null), h(K.Icon, { name: 'lock', size: 14, className: 'kt-muted' }), div('kt-url', el('b', null, p.host), p.path || ''),
      div('kt-extbtn', h(K.Logo, { size: 18 }), p.count && el('i', null, p.count))), div('kt-site', p.children));
  }
  /* A neutral third-party page; deliberately not in Kandado's palette. */
  function Site(p) {
    return div('kt-sform', el('h3', null, p.title), (p.fields || []).map(function (f, i) {
      return h('div', { key: i, className: 'kt-sfield' }, el('label', null, f[0]), div(cx('kt-sinput', f[2] && 'kt-sinput-focus', f[3] && 'kt-sinput-filled'), el('span', null, f[1] || ' '), f[2] && el('span', 'kt-smark', h(K.Logo, { size: 18 }))));
    }), p.button && div('kt-sbtn', p.button));
  }
  function Drop(p) { return h('div', { className: 'kt-drop-in', style: { top: p.top, left: p.left, width: p.width || 320 } }, div('kt-dd-head', h(K.Logo, { size: 16 }), el('span', null, p.head || 'Kandado')), p.children); }
  function DAct(icon, label, sub, on) { return div(cx('kt-dd-act', on && 'kt-dd-on'), h(K.Icon, { name: icon, size: 18 }), div('kt-srow-t', div('kt-srow-l', label), sub && div('kt-srow-d', sub))); }
  function Prompt(p) { return div('kt-prompt', div('kt-prompt-top', h(K.Logo, { size: 22 }), div('kt-srow-t', el('b', null, p.title), p.sub && el('span', 'kt-srow-d', p.sub)), h(K.IconButton, { icon: 'x', size: 'sm', label: 'Close' })), p.children, div('kt-prompt-acts', p.actions)); }
  function OptPage(p) {
    var nav = [['settings', 'General'], ['key', 'Autofill'], ['x', 'Excluded sites'], ['lock', 'Security'], ['user', 'Account']];
    return h(Browser, { host: 'Kandado', path: ' · extension settings', h: 640 }, div('kt-opt', h('aside', { className: 'kt-opt-nav' }, h(K.Logo, { size: 26, wordmark: true }), nav.map(function (n) { return div(cx('kt-nav', n[1] === p.active && 'kt-nav-on'), h(K.Icon, { name: n[0], size: 20 }), el('span', 'kt-nav-l', n[1])); })), div('kt-opt-main', el('h1', null, p.active), p.children)));
  }

  K.Kit = { D: D, LIST: LIST, Item: Item, Items: Items, Canvas: Canvas, Frame: Frame, Grp: Grp, Lbl: Lbl, Fv: Fv, Fs: Fs, TF: TF, Select: Select, Area: Area, SRow: SRow, Rows: Rows, Chips: Chips, Seg: Seg, Banner: Banner, Empty: Empty, Stat: Stat, Avatar: Avatar, Check: Check, Pin: Pin, Keypad: Keypad, Phrase: Phrase, Code: Code, Menu: Menu, Viewfinder: Viewfinder,
    Body: Body, Form: Form, Feat: Feat, Set: Set, Meta: Meta, Sheet: Sheet, Dialog: Dialog, Toasts: Toasts,
    Phone: Phone, Top: Top, MBody: MBody, MFoot: MFoot, MDetail: MDetail, MForm: MForm, MPage: MPage, ItemsList: ItemsList,
    Win: Win, Split: Split, Side: Side, MonList: MonList, DList: DList, DHead: DHead, DDetail: DDetail, DForm: DForm, DPage: DPage, DSettings: DSettings,
    Pop: Pop, PopHead: PopHead, PBody: PBody, PFoot: PFoot, Browser: Browser, Site: Site, Drop: Drop, DAct: DAct, Prompt: Prompt, OptPage: OptPage, h: h, F: F, div: div, el: el, cx: cx };
})();
