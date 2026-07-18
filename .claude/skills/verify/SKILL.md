---
name: verify
description: Build, launch, and drive the rain-curtain page headlessly to verify motion changes.
---

# Verifying rain-portfolio changes

The surface is a browser GUI (wasm/Yew). Verify by building with trunk,
serving `dist/`, and driving with playwright-core + system Edge.

## Build

```powershell
$env:NO_COLOR='true'; trunk build
```

Gotcha: this shell sets `NO_COLOR=1`, which trunk rejects
(`invalid value '1' for '--no-color'`) — it must be `true`/`false`.

## Launch + drive

No global playwright. Install `playwright-core` in a scratch dir
(`npm i playwright-core`) and launch the installed Edge:
`chromium.launch({ channel: 'msedge', headless: true })`. Serve `dist/` with a
tiny node http server (any port); set `Content-Type: application/wasm` for
`.wasm`. A favicon 404 in the console is expected noise.

Wait for `.curtains .string .seg`, then give the sim ~1s before sampling.

## Flows worth driving

- **Idle**: after ~1.5s, seg `style.transform` values should show small
  drifting translates — the wind.
- **Click**: `mouse.click` kicks columns apart around the click x, with
  distance falloff (near column ~235px, far column ~42px).
- **Resize**: `setViewportSize` rebuilds the ropes; every slice must still
  carry a transform afterward.

## Always check reduced motion

**`.seg` is `position: absolute; top: 0`, so the sim's inline `transform` is
the only thing that lays a column out.** Any path where the frame loop doesn't
run leaves every slice stacked at the column top, hidden behind the wall
(`z-index: 2` vs the curtains' `1`) — the page looks completely empty. This
already shipped once as a bug via the `prefers-reduced-motion` early return.

So drive **both** `reducedMotion: 'no-preference'` and `'reduce'` (a Playwright
`newPage` option), and assert layout, not just motion:

- no slice has an empty `style.transform`
- a healthy count of slices sit below `.arches`'s `getBoundingClientRect().bottom`
- under `reduce`, the columns are laid out but do **not** move between samples

Windows 11 commonly has animation effects off, so `reduce` is a real user
configuration, not an edge case.

Evidence: screenshot each phase, and sample a few columns' first/mid/last seg
`style.transform` via `page.evaluate` — magnitudes tell you more than pixels.
