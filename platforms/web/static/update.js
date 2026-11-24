// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

// Update check. A fresh start always loads the latest deploy (sw.js is
// network-first), but an installed app is usually resumed from memory, not
// restarted. So compare this page's build (build.txt, written by
// `cargo xtask web`) with the deployed one when the app comes back to the
// foreground, at most once an hour (one tiny request per user per active
// hour, no polling), and offer a reload. Never reload by
// itself: that would interrupt a game. The offer sits in the page's own
// interface, which is hidden while the engine owns the screen.

const MIN_INTERVAL_MS = 60 * 60 * 1000;

async function deployed() {
  const response = await fetch(new URL("./build.txt", import.meta.url), { cache: "no-store" });
  if (!response.ok) throw new Error(`build.txt: ${response.status}`);
  return (await response.text()).trim();
}

/**
 * Starts checking; resolves to `{ build, check, button }`, or null without a build.
 * `source` gives the deployed build (replaceable in tests).
 */
export async function startUpdateCheck(footer, source = deployed) {
  let running;
  try {
    running = await source();
  } catch {
    return null;
  }
  const button = document.createElement("button");
  button.className = "primary wide";
  button.hidden = true;
  button.textContent = "Update available · Reload";
  button.addEventListener("click", () => location.reload());
  footer.prepend(button);

  let checked = Date.now();
  const check = async () => {
    if (!button.hidden) return;
    checked = Date.now();
    try {
      const latest = await source();
      if (latest && latest !== running) button.hidden = false;
    } catch {
      // Offline or a deploy in progress: try again later.
    }
  };
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible" && Date.now() - checked >= MIN_INTERVAL_MS) check();
  });
  return { build: running, check, button };
}
