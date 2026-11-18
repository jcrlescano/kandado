// Host access to all sites, which in-page autofill needs. It is asked for on the
// welcome tab (E-27) and in Autofill settings (E-29), with an explanation, never
// silently at install. The worker registers the content script when it is given.

const ALL_SITES = { origins: ["https://*/*", "http://*/*"] };

export async function hasSiteAccess(): Promise<boolean> {
  try {
    return await chrome.permissions.contains(ALL_SITES);
  } catch {
    return false;
  }
}

/** Must run from a click: browsers only show the permission prompt for a user gesture. */
export async function requestSiteAccess(): Promise<boolean> {
  try {
    return await chrome.permissions.request(ALL_SITES);
  } catch {
    return false;
  }
}

export async function removeSiteAccess(): Promise<void> {
  await chrome.permissions.remove(ALL_SITES).catch(() => false);
}
