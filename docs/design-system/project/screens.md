# Screens

Every desktop and extension screen Kandado ships, by the IDs the previous design system used (D = desktop, E = browser extension), with the board in the **Kandado Redesign** canvas that draws it in Hardware. Each board has a dark twin on the canvas's Hardware · Dark page (same file name plus `-Dark`). Boards with more than one state switch them with the board's **state** tweak.

**No board yet** means the screen still follows the previous system's description and needs a Hardware board before it is built. Mobile (M-01 to M-75) is deferred: the canvas has four mobile direction boards (Unlock, Items, Login detail, Generator sheet) and nothing else.

## Desktop

| ID | Screen | Board | Notes |
| --- | --- | --- | --- |
| D-01 | Sign in | `Desktop-SignIn` | |
| D-02 | Create account | `Desktop-MasterPassword` | Email verify step: no board yet |
| D-03 | Recovery phrase | `Desktop-RecoveryPhrase` | |
| D-04 | Unlock | `Desktop-Unlock` | Forgot password: `Desktop-Recover` |
| D-05 | First-run import | **No board yet** | Settings › Import (`Desktop-Settings-Import`) has the source grid to reuse |
| D-06 | Empty vault | `Desktop-Empty` | |
| D-07 | Login detail | `Main` | |
| D-08 | Alias detail | `Desktop-AliasDetail` | |
| D-09 | Card detail | **No board yet** | |
| D-10 | Note detail | **No board yet** | |
| D-11 | Identity detail | **No board yet** | |
| D-12 | Password detail | **No board yet** | |
| D-13 | Passkey detail | `Desktop-PasskeyDetail` | |
| D-14 | Search results | `Desktop-Search` | States: results, none |
| D-15 | Multi-select | **No board yet** | |
| D-16 | Item history | `Desktop-History` | |
| D-17 | Narrow window | **No board yet** | |
| D-18 | Create menu | **No board yet** | `Desktop-Empty` has the kind tiles |
| D-19 | Login form | `Desktop-ItemForm` | |
| D-20 | Add two-factor | `Desktop-AddTotp` | |
| D-21 | Alias form | `Desktop-AliasForm` | |
| D-22 | Card form | **No board yet** | |
| D-23 | Note form | **No board yet** | |
| D-24 | Identity form | **No board yet** | |
| D-25 | Delete forever dialog | **No board yet** | `Desktop-Trash` shows the key, not the confirm |
| D-26 | Discard changes dialog | **No board yet** | |
| D-27 | Create vault dialog | `Desktop-Vault` | Light colours are the `vault-*` tokens |
| D-28 | Vault settings | `Desktop-VaultSettings` | Members, roles, pending invites, delete |
| D-29 | Share vault dialog | `Desktop-ShareVault` | |
| D-30 | Invitation received | `Desktop-Invitation` | Banner and dialog on one board |
| D-31 | Share item dialog | `Desktop-ShareItem` | |
| D-32 | Create secure link dialog | `Desktop-SecureLink` | States: options, ready |
| D-33 | Secure links | `Desktop-SecureLinks` | States: list, empty. Reached from Tools › Secure links in the sidebar |
| D-34 | Move items dialog | **No board yet** | |
| D-35 | Monitor overview | `Desktop-Monitor` | Called Health in the app. States: issues, clear |
| D-36 | Breach detail | `Desktop-Monitor-Breach` | |
| D-37 | Weak passwords | `Desktop-Monitor-Weak` | |
| D-38 | Reused passwords | `Desktop-Monitor-Reused` | |
| D-39 | Missing two-factor | `Desktop-Monitor-TwoFactor` | |
| D-40 | Email monitoring | `Desktop-Monitor-Email` | |
| D-41 | Trash | `Desktop-Trash` | |
| D-42 | Empty trash dialog | **No board yet** | |
| D-43 | Settings › General and appearance | `Desktop-Settings-General` | |
| D-44 | Settings › Security | `Desktop-Settings-Security` | |
| D-45 | Settings › Autofill and browser | `Desktop-Settings-Autofill` | |
| D-46 | Settings › Aliases | `Desktop-Settings-Aliases` | |
| D-47 | Settings › Import | `Desktop-Settings-Import` | |
| D-48 | Settings › Export | `Desktop-Settings-Export` | |
| D-49 | Settings › Devices | `Desktop-Settings-Devices` | |
| D-50 | Settings › Account | `Desktop-Settings-Account` | Delete account confirm: `Desktop-DeleteAccount` |
| D-51 | Keyboard shortcuts | `Desktop-Shortcuts` | |
| D-52 | Sign out dialog | `Desktop-SignOut` | |
| D-53 | System tray menu | **No board yet** | Drawn by the OS; needs only labels and the mark |
| D-54 | Banners | `Desktop-Banners` | States: offline, error (sync error), update |

Shared parts: `Desktop-Sidebar` (vault sidebar, with Tools › Health, Password generator, Secure links, Trash) and `Settings-Nav` (settings menu, with Aliases).

## Browser extension

| ID | Screen | Board | Notes |
| --- | --- | --- | --- |
| E-01 | Popup, signed out | `Extension-SignedOut` | |
| E-02 | Popup, locked | `Extension-Locked` | |
| E-03 | Popup, home | `Extension-Popup` | |
| E-04 | Popup, search | **No board yet** | |
| E-05 | Popup, nothing for this site | `Extension-NoMatch` | |
| E-06 | Popup login detail | `Extension-Detail` | |
| E-07 | Popup card detail | **No board yet** | |
| E-08 | Popup alias detail | `Extension-AliasDetail` | |
| E-09 | Popup create menu | **No board yet** | |
| E-10 | Popup create login | `Extension-NewLogin` | |
| E-11 | Popup generator | `Extension-Generator` | |
| E-12 | Popup create alias | `Extension-NewAlias` | |
| E-13 | Popup menu | **No board yet** | |
| E-14 | Popup vault filter | **No board yet** | |
| E-15 | Popup breached site | `Extension-Breached` | |
| E-16 | In-page login dropdown | `Extension-Autofill` | |
| E-17 | In-page locked dropdown | **No board yet** | |
| E-18 | In-page new password | **No board yet** | |
| E-19 | In-page email field | `Extension-EmailField` | |
| E-20 | In-page card | **No board yet** | |
| E-21 | In-page identity | **No board yet** | |
| E-22 | In-page two-factor code | **No board yet** | |
| E-23 | Save login prompt | `Extension-Autofill` | Drawn beside the dropdown on the same board |
| E-24 | Update password prompt | **No board yet** | |
| E-25 | Passkey create | `Extension-PasskeyCreate` | |
| E-26 | Passkey sign in | `Extension-PasskeySignIn` | |
| E-27 | Welcome tab | `Extension-Welcome` | |
| E-28 to E-31 | Settings tab: General, Autofill, Excluded sites, Security | `Extension-Settings` | All four sections on one board |

## Still without a board

Desktop: D-05, D-09 to D-12, D-15, D-17, D-18, D-22 to D-26, D-34, D-42, D-53, and the email-verify step of D-02.
Extension: E-04, E-07, E-09, E-13, E-14, E-17, E-18, E-20, E-21, E-22, E-24.
