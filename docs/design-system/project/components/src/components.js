(function () {
  var React = window.React;
  var h = React.createElement;
  var useState = React.useState, useEffect = React.useEffect, useRef = React.useRef;

  function cx() {
    var out = [];
    for (var i = 0; i < arguments.length; i++) if (arguments[i]) out.push(arguments[i]);
    return out.join(' ');
  }
  function omit(obj, keys) {
    var o = {};
    for (var k in obj) if (Object.prototype.hasOwnProperty.call(obj, k) && keys.indexOf(k) < 0) o[k] = obj[k];
    return o;
  }

  /* ---------- Icon ---------- */
  var P = function (d) { return ['path', { d: d }]; };
  var C = function (x, y, r) { return ['circle', { cx: x, cy: y, r: r }]; };
  var R = function (x, y, w, hh, rx) { return ['rect', { x: x, y: y, width: w, height: hh, rx: rx }]; };
  var DOT = function (x, y) { return ['circle', { cx: x, cy: y, r: 1.4, fill: 'currentColor', stroke: 'none' }]; };
  var ICONS = {
    lock: [R(5, 11, 14, 10, 2.5), P('M8 11V8a4 4 0 0 1 8 0v3')],
    unlock: [R(5, 11, 14, 10, 2.5), P('M8 11V8a4 4 0 0 1 7.6-1.8')],
    key: [C(8, 15, 4), P('M11 12l9-9M17 6l3 3M14.5 8.5l2 2')],
    user: [C(12, 8, 4), P('M4 21a8 8 0 0 1 16 0')],
    mail: [R(3, 5, 18, 14, 2.5), P('M3.5 7l8.5 6 8.5-6')],
    alias: [C(12, 12, 3.5), P('M15.5 12v1.5a2.5 2.5 0 0 0 5 0V12a8.5 8.5 0 1 0-3.4 6.8')],
    card: [R(2.5, 5, 19, 14, 2.5), P('M2.5 10h19M6.5 15h4')],
    note: [P('M6 3h8.5L19 7.5V21H6z'), P('M14 3v5h5M9 12.5h7M9 16.5h5')],
    id: [R(3, 4, 18, 16, 2.5), C(9, 10.5, 2.3), P('M5.8 16.5a3.6 3.6 0 0 1 6.4 0M14.5 10h3.5M14.5 14h2.5')],
    password: [R(2.5, 7, 19, 10, 2.5), DOT(7.5, 12), DOT(12, 12), DOT(16.5, 12)],
    passkey: [C(9, 8, 3.8), P('M3 20a6 6 0 0 1 9.5-4.9'), C(17.5, 12.5, 2.3), P('M17.5 14.8V21M17.5 18.5h2')],
    shield: [P('M12 3l8 3v5.5c0 5-3.4 8.4-8 9.5-4.6-1.1-8-4.5-8-9.5V6z'), P('M8.8 12l2.2 2.2 4.2-4.4')],
    search: [C(11, 11, 6.5), P('M20 20l-4.2-4.2')],
    plus: [P('M12 5v14M5 12h14')],
    copy: [R(9, 9, 11, 11, 2.5), P('M5 15V6a2 2 0 0 1 2-2h8')],
    eye: [P('M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z'), C(12, 12, 3)],
    'eye-off': [P('M3.5 3.5l17 17'), P('M10.4 5.7A9.7 9.7 0 0 1 12 5.5c6 0 9.5 6.5 9.5 6.5a16 16 0 0 1-2.7 3.4M6.5 6.9C4 8.6 2.5 12 2.5 12S6 18.5 12 18.5c1.6 0 3-.4 4.3-1.1'), P('M9.9 9.9a3 3 0 0 0 4.2 4.2')],
    globe: [C(12, 12, 9), P('M3 12h18M12 3c2.6 2.8 3.8 5.8 3.8 9s-1.2 6.2-3.8 9c-2.6-2.8-3.8-5.8-3.8-9S9.4 5.8 12 3z')],
    star: [P('M12 3.5l2.7 5.5 6 .9-4.35 4.25 1 6L12 17.3l-5.35 2.85 1-6L3.3 9.9l6-.9z')],
    trash: [P('M4 7h16M9.5 7V4.5h5V7M6 7l1 13h10l1-13M10 11v5.5M14 11v5.5')],
    settings: [P('M4 6h9.5M18.5 6H20M4 12h3.5M12.5 12H20M4 18h11.5M20 18h0'), C(16, 6, 2.3), C(10, 12, 2.3), C(18, 18, 2.3)],
    vault: [R(3, 4, 18, 15, 2.5), C(12, 11.5, 3.5), P('M12 6.5V8M12 15v1.5M7 11.5h1.5M15.5 11.5H17M6.5 19v1.5M17.5 19v1.5')],
    share: [C(18, 5.5, 2.5), C(6, 12, 2.5), C(18, 18.5, 2.5), P('M8.2 10.8l7.6-4.1M8.2 13.2l7.6 4.1')],
    refresh: [P('M20 11.5a8 8 0 1 1-2.4-5.6L20 8.3'), P('M20 3.5v4.8h-4.8')],
    clock: [C(12, 12, 9), P('M12 7v5l3.2 2')],
    alert: [P('M12 3.5l9.5 16.5h-19z'), P('M12 10v4.2'), DOT(12, 17.2)],
    check: [P('M5 12.5l4.5 4.5L19 7.5')],
    chevron: [P('M9.5 6l6 6-6 6')],
    'chevron-down': [P('M6 9.5l6 6 6-6')],
    more: [DOT(12, 5.5), DOT(12, 12), DOT(12, 18.5)],
    x: [P('M6.5 6.5l11 11M17.5 6.5l-11 11')],
    link: [P('M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1'), P('M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1')],
    fingerprint: [P('M12 11.5v3.5c0 2.4-.5 4.3-1.5 6'), P('M8.5 8.8A4.5 4.5 0 0 1 16.5 11.5v2c0 2.8-.5 5-1.4 7'), P('M7.5 11.5v2.5c0 1.6-.3 3-.8 4.2'), P('M5 7a8.6 8.6 0 0 1 14 0'), P('M19.6 11.2c.2 1 .2 2.3.2 3.3')],
    logout: [P('M14.5 4H18a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2h-3.5'), P('M10 8l-4 4 4 4M6 12h10')],
    grid: [R(4, 4, 7, 7, 1.8), R(13, 4, 7, 7, 1.8), R(4, 13, 7, 7, 1.8), R(13, 13, 7, 7, 1.8)],
    bolt: [P('M13 3L5 13.5h6L10 21l8-10.5h-6z')]
  };

  function Icon(props) {
    var name = props.name, size = props.size || 20;
    var parts = ICONS[name] || ICONS.lock;
    var rest = omit(props, ['name', 'size', 'label', 'className']);
    return h('svg', Object.assign({
      className: cx('kd-icon', props.className),
      width: size, height: size, viewBox: '0 0 24 24', fill: 'none',
      stroke: 'currentColor', strokeWidth: 1.75, strokeLinecap: 'round', strokeLinejoin: 'round',
      role: props.label ? 'img' : undefined,
      'aria-label': props.label, 'aria-hidden': props.label ? undefined : true, focusable: 'false'
    }, rest), parts.map(function (p, i) { return h(p[0], Object.assign({ key: i }, p[1])); }));
  }

  /* ---------- Logo ---------- */
  var logoSeq = 0;
  function Logo(props) {
    var size = props.size || 32;
    var idRef = useRef(null);
    if (idRef.current == null) idRef.current = 'kd-keyhole-' + (++logoSeq);
    var mid = idRef.current;
    var mark = h('svg', { className: 'kd-logo-mark', width: size, height: size, viewBox: '0 0 32 32', 'aria-hidden': props.wordmark ? true : undefined, role: props.wordmark ? undefined : 'img', 'aria-label': props.wordmark ? undefined : 'Kandado' },
      h('defs', null, h('mask', { id: mid },
        h('rect', { x: 0, y: 0, width: 32, height: 32, fill: '#fff' }),
        h('circle', { cx: 16, cy: 19.5, r: 2.6, fill: '#000' }),
        h('rect', { x: 14.8, y: 20, width: 2.4, height: 5, rx: 1.2, fill: '#000' })
      )),
      h('path', { d: 'M10 14V10.5a6 6 0 0 1 12 0V14', fill: 'none', stroke: 'var(--brass)', strokeWidth: 3.2, strokeLinecap: 'round' }),
      h('rect', { x: 5, y: 12.5, width: 22, height: 17, rx: 5, fill: 'var(--brass)', mask: 'url(#' + mid + ')' })
    );
    if (!props.wordmark) return mark;
    return h('span', { className: 'kd-logo', style: { fontSize: Math.round(size * 0.66) + 'px' } }, mark, h('span', { className: 'kd-logo-word' }, 'Kandado'));
  }

  /* ---------- Button ---------- */
  function Button(props) {
    var variant = props.variant || 'secondary', size = props.size || 'md';
    var rest = omit(props, ['variant', 'size', 'icon', 'trailingIcon', 'block', 'className', 'children']);
    return h('button', Object.assign({ type: 'button' }, rest, {
      className: cx('kd-btn', 'kd-btn-' + variant, 'kd-btn-' + size, props.block && 'kd-btn-block', props.className)
    }), props.icon && h(Icon, { name: props.icon, size: size === 'sm' ? 16 : 18 }), props.children != null && h('span', null, props.children), props.trailingIcon && h(Icon, { name: props.trailingIcon, size: 16 }));
  }

  function IconButton(props) {
    var variant = props.variant || 'ghost', size = props.size || 'md';
    var rest = omit(props, ['variant', 'size', 'icon', 'label', 'className']);
    return h('button', Object.assign({ type: 'button', title: props.label, 'aria-label': props.label }, rest, {
      className: cx('kd-iconbtn', 'kd-iconbtn-' + variant, 'kd-iconbtn-' + size, props.className)
    }), h(Icon, { name: props.icon, size: size === 'sm' ? 16 : 20 }));
  }

  /* ---------- TextField ---------- */
  var fieldSeq = 0;
  function TextField(props) {
    var idRef = useRef(null);
    if (idRef.current == null) idRef.current = 'kd-field-' + (++fieldSeq);
    var id = props.id || idRef.current;
    var rest = omit(props, ['label', 'hint', 'error', 'icon', 'trailing', 'mono', 'className', 'id']);
    return h('div', { className: cx('kd-field', props.error && 'kd-field-error', props.className) },
      props.label && h('label', { className: 'kd-field-label', htmlFor: id }, props.label),
      h('div', { className: 'kd-field-box' },
        props.icon && h(Icon, { name: props.icon, size: 18, className: 'kd-field-icon' }),
        h('input', Object.assign({ id: id, className: cx('kd-field-input', props.mono && 'kd-mono'), 'aria-invalid': props.error ? true : undefined }, rest)),
        props.trailing
      ),
      (props.error || props.hint) && h('div', { className: props.error ? 'kd-field-msg kd-field-msg-error' : 'kd-field-msg' },
        props.error && h(Icon, { name: 'alert', size: 14 }), props.error || props.hint)
    );
  }

  /* ---------- Toggle ---------- */
  function Toggle(props) {
    var controlled = props.checked !== undefined;
    var s = useState(!!props.defaultChecked), inner = s[0], setInner = s[1];
    var on = controlled ? props.checked : inner;
    function flip() {
      if (props.disabled) return;
      if (!controlled) setInner(!on);
      if (props.onChange) props.onChange(!on);
    }
    return h('label', { className: cx('kd-toggle', props.disabled && 'kd-disabled') },
      props.label && h('span', { className: 'kd-toggle-text' }, h('span', { className: 'kd-toggle-label' }, props.label), props.description && h('span', { className: 'kd-toggle-desc' }, props.description)),
      h('button', { type: 'button', role: 'switch', 'aria-checked': on, 'aria-label': props.label ? undefined : props.ariaLabel, className: cx('kd-switch', on && 'kd-switch-on'), onClick: flip, disabled: props.disabled },
        h('span', { className: 'kd-switch-thumb' }))
    );
  }

  /* ---------- Badge ---------- */
  var TONE_ICON = { danger: 'alert', warning: 'alert', teal: 'check' };
  function Badge(props) {
    var tone = props.tone || 'neutral';
    var icon = props.icon === false ? null : (props.icon || TONE_ICON[tone]);
    return h('span', { className: cx('kd-badge', 'kd-badge-' + tone) }, icon && h(Icon, { name: icon, size: 13 }), props.children);
  }

  /* ---------- SearchBar ---------- */
  function SearchBar(props) {
    var rest = omit(props, ['shortcut', 'className']);
    return h('div', { className: cx('kd-search', props.className) },
      h(Icon, { name: 'search', size: 18, className: 'kd-search-icon' }),
      h('input', Object.assign({ type: 'search', className: 'kd-search-input', placeholder: 'Search in all vaults', 'aria-label': props.placeholder || 'Search in all vaults' }, rest)),
      props.shortcut && h('kbd', { className: 'kd-kbd' }, props.shortcut)
    );
  }

  /* ---------- VaultChip ---------- */
  function VaultChip(props) {
    var tone = props.tone || 'brass';
    var rest = omit(props, ['name', 'count', 'tone', 'icon', 'active', 'dropdown', 'className']);
    return h('button', Object.assign({ type: 'button' }, rest, {
      className: cx('kd-vault', props.active && 'kd-vault-active', props.className), 'aria-pressed': props.active ? true : undefined
    }),
      h('span', { className: 'kd-vault-dot', style: { background: 'var(--' + tone + '-soft)', color: 'var(--' + tone + ')' } }, h(Icon, { name: props.icon || 'vault', size: 14 })),
      h('span', { className: 'kd-vault-name' }, props.name),
      props.count != null && h('span', { className: 'kd-vault-count' }, props.count),
      props.dropdown && h(Icon, { name: 'chevron-down', size: 16 })
    );
  }

  /* ---------- ItemIcon ---------- */
  var KIND = {
    login: { icon: 'key', tone: 'teal', label: 'Login' },
    alias: { icon: 'alias', tone: 'moss', label: 'Alias' },
    card: { icon: 'card', tone: 'coral', label: 'Card' },
    note: { icon: 'note', tone: 'sky', label: 'Note' },
    identity: { icon: 'id', tone: 'plum', label: 'Identity' },
    password: { icon: 'password', tone: 'brass', label: 'Password' },
    passkey: { icon: 'passkey', tone: 'brass', label: 'Passkey' }
  };
  function ItemIcon(props) {
    var k = KIND[props.kind] || KIND.login, size = props.size || 40;
    return h('span', {
      className: 'kd-itemicon', 'aria-hidden': true,
      style: { width: size, height: size, background: 'var(--' + k.tone + '-soft)', color: 'var(--' + k.tone + ')', borderRadius: size >= 36 ? 'var(--radius-md)' : 'var(--radius-sm)' }
    }, props.letter ? h('span', { className: 'kd-itemicon-letter', style: { fontSize: Math.round(size * 0.42) } }, props.letter) : h(Icon, { name: k.icon, size: Math.round(size * 0.5) }));
  }

  /* ---------- ItemRow ---------- */
  function ItemRow(props) {
    var rest = omit(props, ['kind', 'title', 'subtitle', 'selected', 'badge', 'favorite', 'onCopy', 'letter', 'dense', 'className']);
    return h('div', Object.assign({ role: 'option', 'aria-selected': !!props.selected, tabIndex: 0 }, rest, {
      className: cx('kd-row', props.selected && 'kd-row-selected', props.dense && 'kd-row-dense', props.className)
    }),
      h(ItemIcon, { kind: props.kind, size: props.dense ? 32 : 40, letter: props.letter }),
      h('span', { className: 'kd-row-text' },
        h('span', { className: 'kd-row-title' }, props.title, props.favorite && h(Icon, { name: 'star', size: 14, className: 'kd-row-fav', label: 'Favourite' })),
        props.subtitle && h('span', { className: 'kd-row-sub' }, props.subtitle)
      ),
      props.badge,
      props.onCopy && h(IconButton, { icon: 'copy', size: 'sm', label: 'Copy password', className: 'kd-row-copy', onClick: function (e) { e.stopPropagation(); props.onCopy(); } })
    );
  }

  /* ---------- SecretField ---------- */
  function SecretField(props) {
    var concealed = props.concealed !== false;
    var s = useState(!!props.defaultRevealed), shown = s[0], setShown = s[1];
    var c = useState(false), copied = c[0], setCopied = c[1];
    useEffect(function () {
      if (!concealed) return;
      function hide() { setShown(false); }
      window.addEventListener('blur', hide);
      return function () { window.removeEventListener('blur', hide); };
    }, [concealed]);
    function copy() {
      try { if (navigator.clipboard) navigator.clipboard.writeText(props.value); } catch (e) { /* clipboard may be blocked */ }
      setCopied(true);
      setTimeout(function () { setCopied(false); }, 1400);
      if (props.onCopy) props.onCopy();
    }
    var display = concealed && !shown ? '••••••••••••' : props.value;
    return h('div', { className: cx('kd-secret', props.className) },
      props.icon && h(Icon, { name: props.icon, size: 18, className: 'kd-secret-icon' }),
      h('div', { className: 'kd-secret-body' },
        h('div', { className: 'kd-secret-label' }, props.label),
        h('div', { className: cx('kd-secret-value', (props.mono || (concealed && shown)) && 'kd-mono', concealed && !shown && 'kd-secret-masked'), style: props.valueColor ? { color: props.valueColor } : null }, display)
      ),
      concealed && h(IconButton, { icon: shown ? 'eye-off' : 'eye', size: 'sm', label: shown ? 'Hide' : 'Reveal', onClick: function () { setShown(!shown); } }),
      props.copyable !== false && h(IconButton, { icon: copied ? 'check' : 'copy', size: 'sm', label: copied ? 'Copied' : 'Copy ' + String(props.label || '').toLowerCase(), onClick: copy, className: copied ? 'kd-copied' : null })
    );
  }

  /* ---------- StrengthMeter ---------- */
  var STRENGTH = [
    { label: 'Weak', tone: 'danger' }, { label: 'Weak', tone: 'danger' },
    { label: 'Fair', tone: 'warning' }, { label: 'Good', tone: 'teal' }, { label: 'Strong', tone: 'teal' }
  ];
  function StrengthMeter(props) {
    var score = Math.max(0, Math.min(4, props.score == null ? 0 : props.score));
    var st = STRENGTH[score];
    var segs = [];
    for (var i = 1; i <= 4; i++) segs.push(h('span', { key: i, className: 'kd-meter-seg', style: i <= Math.max(score, 1) ? { background: 'var(--' + st.tone + ')' } : null }));
    return h('div', { className: 'kd-meter', role: 'meter', 'aria-valuemin': 0, 'aria-valuemax': 4, 'aria-valuenow': score, 'aria-label': 'Password strength' },
      h('div', { className: 'kd-meter-bar' }, segs),
      h(Badge, { tone: st.tone }, st.label),
      props.detail && h('span', { className: 'kd-meter-detail' }, props.detail)
    );
  }

  /* ---------- PasswordGenerator ---------- */
  var WORDS = ['brass', 'harbor', 'lantern', 'mango', 'tide', 'quiet', 'ember', 'island', 'cobalt', 'thread', 'falcon', 'meadow', 'cipher', 'pebble', 'summit', 'violet', 'anchor', 'nimbus', 'orbit', 'saffron', 'willow', 'canyon', 'jasmine', 'garnet'];
  function rand(n) {
    var a = new Uint32Array(1);
    if (window.crypto && window.crypto.getRandomValues) { window.crypto.getRandomValues(a); return a[0] % n; }
    return Math.floor(Math.random() * n);
  }
  function generate(o) {
    if (o.mode === 'memorable') {
      var w = [];
      for (var i = 0; i < o.words; i++) { var word = WORDS[rand(WORDS.length)]; if (o.upper) word = word.charAt(0).toUpperCase() + word.slice(1); w.push(word); }
      if (o.numbers) w[w.length - 1] += rand(10);
      return { value: w.join(o.symbols ? '-' : ' '), bits: o.words * Math.log2(WORDS.length * 40) };
    }
    var pool = 'abcdefghijkmnpqrstuvwxyz';
    if (o.upper) pool += 'ABCDEFGHJKLMNPQRSTUVWXYZ';
    if (o.numbers) pool += '23456789';
    if (o.symbols) pool += '!#$%&*+-=?@^_';
    var s = '';
    for (var j = 0; j < o.length; j++) s += pool.charAt(rand(pool.length));
    return { value: s, bits: o.length * Math.log2(pool.length) };
  }
  function scoreBits(b) { return b < 36 ? 1 : b < 60 ? 2 : b < 80 ? 3 : 4; }
  function PasswordGenerator(props) {
    var st = useState({ mode: 'random', length: props.defaultLength || 20, words: 4, upper: true, numbers: true, symbols: true }), o = st[0], setO = st[1];
    var pw = useState(function () { return generate(o); }), res = pw[0], setRes = pw[1];
    useEffect(function () { setRes(generate(o)); }, [o.mode, o.length, o.words, o.upper, o.numbers, o.symbols]);
    function set(k, v) { var n = Object.assign({}, o); n[k] = v; setO(n); }
    var isWords = o.mode === 'memorable';
    return h('div', { className: 'kd-gen' },
      h('div', { className: 'kd-gen-out' },
        h('div', { className: 'kd-gen-value kd-mono', 'aria-live': 'polite' }, colorize(res.value)),
        h('div', { className: 'kd-gen-actions' },
          h(IconButton, { icon: 'refresh', label: 'Generate again', onClick: function () { setRes(generate(o)); } }),
          h(IconButton, { icon: 'copy', label: 'Copy password', onClick: function () { try { navigator.clipboard && navigator.clipboard.writeText(res.value); } catch (e) { /* blocked */ } } })
        )
      ),
      h(StrengthMeter, { score: scoreBits(res.bits), detail: Math.round(res.bits) + ' bits' }),
      h('div', { className: 'kd-seg', role: 'tablist' },
        h('button', { type: 'button', role: 'tab', 'aria-selected': !isWords, className: cx('kd-seg-btn', !isWords && 'kd-seg-on'), onClick: function () { set('mode', 'random'); } }, 'Random'),
        h('button', { type: 'button', role: 'tab', 'aria-selected': isWords, className: cx('kd-seg-btn', isWords && 'kd-seg-on'), onClick: function () { set('mode', 'memorable'); } }, 'Memorable')
      ),
      h('label', { className: 'kd-range' },
        h('span', { className: 'kd-range-top' }, h('span', null, isWords ? 'Words' : 'Characters'), h('span', { className: 'kd-range-val' }, isWords ? o.words : o.length)),
        h('input', { type: 'range', min: isWords ? 3 : 8, max: isWords ? 8 : 64, value: isWords ? o.words : o.length, onChange: function (e) { set(isWords ? 'words' : 'length', +e.target.value); } })
      ),
      h(Toggle, { label: isWords ? 'Capitalise' : 'Uppercase letters', checked: o.upper, onChange: function (v) { set('upper', v); } }),
      h(Toggle, { label: 'Numbers', checked: o.numbers, onChange: function (v) { set('numbers', v); } }),
      h(Toggle, { label: isWords ? 'Hyphens between words' : 'Symbols', checked: o.symbols, onChange: function (v) { set('symbols', v); } }),
      props.onUse && h(Button, { variant: 'primary', block: true, onClick: function () { props.onUse(res.value); } }, 'Use password')
    );
  }
  function colorize(v) {
    var out = [];
    for (var i = 0; i < v.length; i++) {
      var ch = v.charAt(i);
      var cls = /[0-9]/.test(ch) ? 'kd-ch-num' : /[A-Za-z ]/.test(ch) ? null : 'kd-ch-sym';
      out.push(cls ? h('span', { key: i, className: cls }, ch) : ch);
    }
    return out;
  }

  /* ---------- TotpCode ---------- */
  function TotpCode(props) {
    var period = props.period || 30;
    var now = function () { return period - (Math.floor(Date.now() / 1000) % period); };
    var s = useState(props.remaining != null ? props.remaining : now()), left = s[0], setLeft = s[1];
    useEffect(function () {
      if (props.remaining != null) return;
      var t = setInterval(function () { setLeft(now()); }, 1000);
      return function () { clearInterval(t); };
    }, [period, props.remaining]);
    var code = String(props.code || '000000');
    var shown = code.length === 6 ? code.slice(0, 3) + ' ' + code.slice(3) : code;
    var r = 9, circ = 2 * Math.PI * r, frac = left / period, low = left <= 5;
    return h('div', { className: cx('kd-totp', low && 'kd-totp-low') },
      h('div', { className: 'kd-secret-body' },
        h('div', { className: 'kd-secret-label' }, props.label || 'Two-factor code'),
        h('div', { className: 'kd-totp-code' }, shown)
      ),
      h('svg', { className: 'kd-totp-ring', width: 28, height: 28, viewBox: '0 0 24 24', role: 'img', 'aria-label': left + ' seconds left' },
        h('circle', { cx: 12, cy: 12, r: r, className: 'kd-totp-track' }),
        h('circle', { cx: 12, cy: 12, r: r, className: 'kd-totp-arc', strokeDasharray: circ, strokeDashoffset: circ * (1 - frac), transform: 'rotate(-90 12 12)' }),
        h('text', { x: 12, y: 15.5, textAnchor: 'middle', className: 'kd-totp-num' }, left)
      ),
      h(IconButton, { icon: 'copy', size: 'sm', label: 'Copy code' })
    );
  }

  /* ---------- Toast ---------- */
  function Toast(props) {
    var tone = props.tone || 'neutral';
    return h('div', { className: cx('kd-toast', 'kd-toast-' + tone), role: 'status' },
      h(Icon, { name: props.icon || (tone === 'danger' ? 'alert' : 'check'), size: 18, className: 'kd-toast-icon' }),
      h('span', { className: 'kd-toast-text' }, props.children),
      props.action && h('button', { type: 'button', className: 'kd-toast-action', onClick: props.onAction }, props.action)
    );
  }

  var api = { Logo: Logo, Icon: Icon, Button: Button, IconButton: IconButton, TextField: TextField, Toggle: Toggle, Badge: Badge, SearchBar: SearchBar, VaultChip: VaultChip, ItemIcon: ItemIcon, ItemRow: ItemRow, SecretField: SecretField, StrengthMeter: StrengthMeter, PasswordGenerator: PasswordGenerator, TotpCode: TotpCode, Toast: Toast, ICON_NAMES: Object.keys(ICONS) };
  window.Kandado = Object.assign(window.Kandado || {}, api);
})();
