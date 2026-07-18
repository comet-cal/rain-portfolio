# rain-portfolio

A portfolio inspired by the song 他人事の音がする or I Hear the Sound of Someone Else's Problems by あめのむらくもP. Built with [Yew](https://yew.rs/) and
compiled to WebAssembly. Columns of Japanese text hang from a white arched wall
like strings of a curtain, drift on their own, and part in a ripple when you
click — as if a raindrop landed in water.

## Features WIP

- **Autonomous motion** — every text column sways and bounces on its own,
  desynchronized so the curtain ripples instead of moving as one block.
- **Click ripple** — a left click sends a wave outward across the columns. Each
  column is a jointed string: the swing travels down it and settles with
  damping, so the strings bend and open like real hanging fabric.
- **Arched wall** — a white SVG arcade frames the top of the scene.
- Respects `prefers-reduced-motion`.

## Tech stack

- [Yew](https://yew.rs/) (function components, CSR)
- [Trunk](https://trunkrs.dev/) for building/serving the WASM bundle
- Plain CSS for layout, the arches, and the ambient animation
- `web-sys` + `requestAnimationFrame` for the per-frame ripple physics

## Development

Prerequisites:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk
```

Serve with live reload:

```sh
trunk serve --open
```

Build a production bundle into `dist/`:

```sh
trunk build --release
```

## Deployment

Live at **https://comet-cal.github.io/rain-portfolio/**.

Every push to `main` builds the WASM bundle and publishes it to GitHub Pages
via `.github/workflows/deploy.yml`. You can also run it by hand from the
repository's Actions tab (**Deploy to GitHub Pages** → *Run workflow*).

One-time setup in the repository: **Settings → Pages → Build and deployment →
Source: GitHub Actions**. The workflow fails at its "Configure Pages" step
until that is set.

The site is served from a subdirectory (`/rain-portfolio/`), so the release
build passes a matching base path — otherwise the page would request its
stylesheet and `.wasm` from the domain root and 404:

```sh
trunk build --release --public-url /rain-portfolio/
```

The workflow takes that prefix from the Pages configuration, so renaming the
repository does not break the build. Local `trunk serve` needs no base path.

## Layout

| Path         | Purpose                                              |
| ------------ | ---------------------------------------------------- |
| `src/main.rs`| Yew app: columns, arches, and the click ripple loop  |
| `style.css`  | Layout, arches, and the ambient drift animation      |
| `index.html` | Trunk entry point                                    |
| `trunk.toml` | Trunk build/serve configuration                      |

## Tuning

Motion constants live at the top of `src/main.rs`:

- `SEGMENTS` — joints per string (more = smoother bending)
- `DOWN_PROP` — how much the swing lags going down each string (whip)
- `SWING_PX` — how far the free end throws on a click
- `FREQ` / `DECAY` — swing speed and how quickly a string settles
- `SPREAD` / `DIST_SIGMA` — ripple speed and reach across columns

The ambient sway/bounce lives in the `drift` keyframes in `style.css`.
