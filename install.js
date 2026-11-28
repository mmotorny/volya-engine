// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Installing Volya Engine as an app, which keeps its storage (browsers may
// delete a plain tab's data when they need space). How depends on the
// device, so the interface says only what applies:
//
// - iPhone and iPad: no API; Safari's (and Chrome's) Share menu has Add to
//   Home Screen. The Home Screen app keeps its own storage, separate from the
//   browser's, so the game must be imported there.
// - Desktop Chrome: the page may offer installation itself
//   (`beforeinstallprompt`); otherwise the address bar's install icon. The
//   installed app shares the browser's storage.
//
// Loaded early by main.js: Chrome offers installation once, soon after load.

let offer = null;
const listeners = new Set();
addEventListener("beforeinstallprompt", (event) => {
  event.preventDefault();
  offer = event;
  for (const f of listeners) f();
});
addEventListener("appinstalled", () => {
  offer = null;
  for (const f of listeners) f();
});

/** "ios" or "desktop" (the UX page sets `volyaPlatform` per frame). */
export const platform = () =>
  globalThis.volyaPlatform ??
  (/iPhone|iPad|iPod/.test(navigator.userAgent) || (navigator.platform === "MacIntel" && navigator.maxTouchPoints > 1)
    ? "ios"
    : "desktop");

/** Whether the browser is Chrome or another Chromium browser (they share the
 * address bar's install icon). */
export const chromium = () => globalThis.volyaChromium ?? Boolean(navigator.userAgentData);

/** Whether this page runs as the installed app. */
export const installed = () =>
  globalThis.volyaInstalled ?? (matchMedia("(display-mode: standalone)").matches || navigator.standalone === true);

/** Whether the browser offers to install the app from the page. */
export const canOffer = () => Boolean(offer ?? globalThis.volyaInstallOffer);

/** Shows the browser's install prompt; resolves to whether it was accepted. */
export async function install() {
  const prompt = offer ?? globalThis.volyaInstallOffer;
  if (!prompt) return false;
  await prompt.prompt();
  const { outcome } = await prompt.userChoice;
  return outcome === "accepted";
}

export const onChange = (f) => listeners.add(f);

/** iOS's Share symbol: a box with an arrow out of its top. */
export const SHARE_ICON = `<svg class="share-icon" viewBox="0 0 16 20" aria-label="Share" role="img"><path d="M8 1.5v11M4.5 5 8 1.5 11.5 5M5.5 8.5H3v10h10v-10h-2.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/></svg>`;
