/// <reference path="./vendor/kandado.d.ts" />
// The design system components (`window.Kandado`), loaded once for every
// extension page. The bundle is copied from docs/ by scripts/sync-ui.mjs.
import "./react-global";
import "./vendor/kandado-bundle.js";
import "./vendor/tokens.css";
import "./vendor/kandado-bundle.css";
import "./fonts.css";
import "./extension.css";

export const K = window.Kandado;
