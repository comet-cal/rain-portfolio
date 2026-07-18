use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use yew::prelude::*;

const PHRASE: &str = "他人事の音がする";
const COLUMN_COUNT: usize = 50;

// ---- Text geometry ----
// A slice carries exactly this many characters and is sized by that text, so
// its height is exactly this many glyph advances whatever the font metrics
// are. That is what makes a boundary fall precisely between two characters:
// the column reads as one unbroken stream rather than a stack of blocks.
const CHARS_PER_SEG: usize = 2;
// Rough glyph advance (font-size + letter-spacing, see style.css). Used only
// to guess how many slices cover the viewport — real heights are measured
// from the DOM, so an error here only changes how far the curtain hangs.
const GLYPH_PX_EST: f64 = 18.9;
// Extra joints past the bottom edge so a swinging string never runs short.
const SEG_SLACK: usize = 3;

// ---- String physics: each column is a Verlet rope pinned at the top ----
const GRAVITY: f64 = 2400.0; // px/s² — pendulum pull that straightens a string
const DRAG: f64 = 1.1; // 1/s — air drag; how quickly swings die out
const SUBSTEP: f64 = 1.0 / 120.0; // s — fixed physics timestep
const MAX_FRAME_DT: f64 = 1.0 / 30.0; // clamp frame gaps (tab switches etc.)
const ITERS: usize = 10; // constraint passes per substep — rope inextensibility

// ---- Wind: always-on idle sway, as a horizontal force field ----
const WIND_ACCEL: f64 = 90.0; // px/s² peak
const WIND_FREQ: f64 = 0.16; // Hz — primary sway
const WIND_FREQ2: f64 = 0.045; // Hz — slow gust modulation
const WIND_SPATIAL: f64 = 5.0; // radians of phase across the width

// ---- Mouse brush: the cursor is a round collider the strings part around ----
const BRUSH_R: f64 = 70.0; // px
const BRUSH_Y: f64 = 0.35; // vertical push scale — strings mostly part sideways
                           // Fraction of the overlap resolved per substep. Below 1.0 the collider has
                           // finite grip: rope tension can slide a string around a moving cursor
                           // instead of the string sticking to its front and being carried along.
const BRUSH_SOFT: f64 = 0.18;

// ---- Click: a kick that sweeps across the columns ----
const SPREAD: f64 = 0.55; // s for the kick to cross the full width
const DIST_SIGMA: f64 = 0.28; // how far the kick reaches around the click
const KICK: f64 = 240.0; // px/s given to the free end of the nearest string

#[derive(Clone, Copy)]
struct Pt {
    x: f64,
    y: f64,
    px: f64,
    py: f64,
}

/// One hanging string: SEGMENTS + 1 simulated points, `pts[0]` pinned at the
/// top of its column. Coordinates are px in `.curtains` space.
struct Strand {
    cx: f64, // 0..1 across the width
    rest_x: f64,
    top_y: f64,
    seg_len: f64,
    pts: Vec<Pt>,
}

struct Sim {
    strands: Vec<Strand>,
    w: f64,
    h: f64,
}

/// One click, swept across the columns over `SPREAD` seconds; each string is
/// kicked exactly once when the sweep reaches it.
struct Click {
    x: f64,     // 0..1 across the width
    start: f64, // ms, performance.now() timeline
    kicked: Vec<bool>,
}

/// Idle-sway force: a traveling wave with per-column phase and a slow gust.
fn wind_accel(cx: f64, t: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let main = (tau * WIND_FREQ * t - cx * WIND_SPATIAL).sin();
    let gust = 0.5 + 0.5 * (tau * WIND_FREQ2 * t + cx * 2.3).sin();
    WIND_ACCEL * main * (0.35 + 0.65 * gust)
}

/// Measure the DOM (column positions, heights) and lay every rope out at rest.
/// Walks `.curtains` positionally: children are columns, grandchildren joints.
fn build_sim(container: &web_sys::Element) -> Sim {
    let w = container.client_width() as f64;
    let h = container.client_height() as f64;
    let cols = container.children();
    let ncol = cols.length();
    let mut strands = Vec::with_capacity(ncol as usize);
    for i in 0..ncol {
        let Some(col) = cols
            .item(i)
            .and_then(|c| c.dyn_into::<web_sys::HtmlElement>().ok())
        else {
            continue;
        };
        let cx = if ncol > 1 {
            i as f64 / (ncol - 1) as f64
        } else {
            0.5
        };
        let rest_x = col.offset_left() as f64 + col.offset_width() as f64 / 2.0;
        let top_y = col.offset_top() as f64;
        let k = (col.children().length() as usize).max(1);
        let seg_len = (col.client_height() as f64).max(1.0) / k as f64;
        let pts = (0..=k)
            .map(|j| {
                let y = top_y + j as f64 * seg_len;
                Pt {
                    x: rest_x,
                    y,
                    px: rest_x,
                    py: y,
                }
            })
            .collect();
        strands.push(Strand {
            cx,
            rest_x,
            top_y,
            seg_len,
            pts,
        });
    }
    Sim { strands, w, h }
}

/// Kick every string a click sweep has newly reached, then drop spent clicks.
fn apply_clicks(sim: &mut Sim, clicks: &mut Vec<Click>, now_ms: f64) {
    for c in clicks.iter_mut() {
        let elapsed = (now_ms - c.start) / 1000.0;
        for (i, s) in sim.strands.iter_mut().enumerate() {
            if i >= c.kicked.len() || c.kicked[i] {
                continue;
            }
            let signed = s.cx - c.x;
            let dist = signed.abs();
            if elapsed < dist * SPREAD {
                continue;
            }
            c.kicked[i] = true;
            let dir = if signed >= 0.0 { 1.0 } else { -1.0 };
            let reach = 1.0 / (1.0 + (dist / DIST_SIGMA).powi(2));
            let n = (s.pts.len() - 1).max(1) as f64;
            for (j, p) in s.pts.iter_mut().enumerate().skip(1) {
                let depth = j as f64 / n;
                // In Verlet, shifting `prev` by v·dt adds velocity v.
                p.px -= dir * reach * KICK * depth.powf(1.3) * SUBSTEP;
            }
        }
    }
    clicks.retain(|c| (now_ms - c.start) / 1000.0 < SPREAD + 0.1);
}

/// One fixed physics step: integrate, push strings out of the cursor circle,
/// then re-tighten the ropes with distance constraints.
fn substep(sim: &mut Sim, t: f64, wind_on: bool, mouse: Option<(f64, f64)>) {
    let dt = SUBSTEP;
    let damp = (1.0 - DRAG * dt).max(0.0);
    for s in &mut sim.strands {
        let ax = if wind_on { wind_accel(s.cx, t) } else { 0.0 };
        for p in s.pts.iter_mut().skip(1) {
            let vx = (p.x - p.px) * damp;
            let vy = (p.y - p.py) * damp;
            p.px = p.x;
            p.py = p.y;
            p.x += vx + ax * dt * dt;
            p.y += vy + GRAVITY * dt * dt;
        }
        if let Some((mx, my)) = mouse {
            for p in s.pts.iter_mut().skip(1) {
                let dx = p.x - mx;
                let dy = p.y - my;
                let d2 = dx * dx + dy * dy;
                if d2 < BRUSH_R * BRUSH_R && d2 > 1e-12 {
                    let d = d2.sqrt();
                    let push = (BRUSH_R - d) * BRUSH_SOFT;
                    p.x += dx / d * push;
                    p.y += dy / d * push * BRUSH_Y;
                }
            }
        }
        for _ in 0..ITERS {
            s.pts[0].x = s.rest_x;
            s.pts[0].y = s.top_y;
            for j in 1..s.pts.len() {
                let (head, tail) = s.pts.split_at_mut(j);
                let a = &mut head[j - 1];
                let b = &mut tail[0];
                let dx = b.x - a.x;
                let dy = b.y - a.y;
                let d = (dx * dx + dy * dy).sqrt().max(1e-9);
                let diff = (d - s.seg_len) / d;
                if j == 1 {
                    // `a` is the pin — only the free point moves.
                    b.x -= dx * diff;
                    b.y -= dy * diff;
                } else {
                    let half = 0.5 * diff;
                    a.x += dx * half;
                    a.y += dy * half;
                    b.x -= dx * half;
                    b.y -= dy * half;
                }
            }
        }
    }
}

/// Write each slice's transform: its top edge is carried to the rope point
/// above it and the slice is rotated onto the segment below, so with
/// `transform-origin: 50% 0` consecutive slices chain into one bent string.
fn render(sim: &Sim, container: &web_sys::Element) {
    let cols = container.children();
    for (i, s) in sim.strands.iter().enumerate() {
        let Some(col) = cols.item(i as u32) else {
            continue;
        };
        let segs = col.children();
        for j in 0..segs.length() {
            let Some(seg) = segs
                .item(j)
                .and_then(|c| c.dyn_into::<web_sys::HtmlElement>().ok())
            else {
                continue;
            };
            let (Some(p), Some(q)) = (s.pts.get(j as usize), s.pts.get(j as usize + 1)) else {
                continue;
            };
            let dx = p.x - s.rest_x;
            let dy = p.y - (s.top_y + j as f64 * s.seg_len);
            // CSS rotate() is clockwise with y down: tilting the local "down"
            // axis onto the segment vector (sx, sy) needs atan2(-sx, sy).
            let rot = (-(q.x - p.x)).atan2(q.y - p.y).to_degrees();
            let _ = seg.style().set_property(
                "transform",
                &format!("translate({dx:.2}px, {dy:.2}px) rotate({rot:.2}deg)"),
            );
        }
    }
}

fn viewport_height() -> f64 {
    web_sys::window()
        .and_then(|w| w.inner_height().ok())
        .and_then(|v| v.as_f64())
        .unwrap_or(900.0)
}

#[derive(Properties, PartialEq)]
struct CurtainsProps {
    container_ref: NodeRef,
    height: f64,
}

#[function_component]
fn Curtains(props: &CurtainsProps) -> Html {
    let phrase: Vec<char> = PHRASE.chars().collect();

    let columns = (0..COLUMN_COUNT)
        .map(|i| {
            let opacity = 0.4 + ((i * 7) % 30) as f64 / 100.0;
            let offset = ((i * 13) % 50) as f64; // uneven top → drip
            // No height: the column is as tall as the joints it holds, which
            // is what lets `build_sim` recover the exact slice height from it.
            let style = format!("opacity: {opacity}; margin-top: {offset}px;");

            let joints = ((props.height - offset).max(0.0)
                / (GLYPH_PX_EST * CHARS_PER_SEG as f64))
                .ceil() as usize
                + SEG_SLACK;

            // Each column enters the phrase at a different character. Without
            // this every column repeats the same 8-glyph cycle in step, and
            // its dense kanji line up across the width as horizontal rows.
            let phase = (i * 3) % phrase.len();

            let segs = (0..joints)
                .map(|j| {
                    // The characters at this joint's depth in the column's
                    // stream, so the phrase continues across the boundary
                    // instead of restarting.
                    let text: String = (0..CHARS_PER_SEG)
                        .map(|k| phrase[(phase + j * CHARS_PER_SEG + k) % phrase.len()])
                        .collect();
                    html! { <div class="seg">{ text }</div> }
                })
                .collect::<Html>();

            html! { <div class="string" style={style}>{ segs }</div> }
        })
        .collect::<Html>();

    html! {
        <div class="curtains" ref={props.container_ref.clone()} aria-hidden="true">
            { columns }
        </div>
    }
}

#[function_component]
fn Arches() -> Html {
    html! {
        <svg class="arches" viewBox="0 0 300 230" preserveAspectRatio="none"
             xmlns="http://www.w3.org/2000/svg">
            <path class="wall"
                  d="M0,0 H300 V100
                     A50,60 0 0 0 200,100
                     A50,60 0 0 0 100,100
                     A50,60 0 0 0 0,100 Z" />
            <path class="arch" vector-effect="non-scaling-stroke"
                  d="M0,100 A50,60 0 0 1 100,100" />
            <path class="arch" vector-effect="non-scaling-stroke"
                  d="M100,100 A50,60 0 0 1 200,100" />
            <path class="arch" vector-effect="non-scaling-stroke"
                  d="M200,100 A50,60 0 0 1 300,100" />
        </svg>
    }
}

#[function_component]
fn App() -> Html {
    let curtains_ref = use_node_ref();
    let clicks = use_mut_ref(Vec::<Click>::new);
    // Cursor as a collider, in client coords. `Some` only while the left
    // button is held: the curtains are parted by dragging, not by hovering.
    let mouse = use_mut_ref(|| None::<(f64, f64)>);
    let raf = use_mut_ref(|| None::<Closure<dyn FnMut(f64)>>);
    // Drives how many joints each column needs to reach the bottom edge.
    let height = use_state(viewport_height);

    // Re-flow the columns when the window changes height. The frame loop
    // notices the container resize on its own and rebuilds the ropes.
    {
        let height = height.clone();
        use_effect_with((), move |_| {
            let win = web_sys::window().unwrap();
            let cb = Closure::wrap(Box::new(move || height.set(viewport_height()))
                as Box<dyn FnMut()>);
            let _ = win.add_event_listener_with_callback("resize", cb.as_ref().unchecked_ref());
            move || {
                if let Some(win) = web_sys::window() {
                    let _ = win.remove_event_listener_with_callback(
                        "resize",
                        cb.as_ref().unchecked_ref(),
                    );
                }
                drop(cb);
            }
        });
    }

    // One always-running frame loop: fixed-timestep rope physics + render.
    {
        let clicks = clicks.clone();
        let mouse = mouse.clone();
        let raf = raf.clone();
        let curtains_ref = curtains_ref.clone();
        use_effect_with((), move |_| {
            let reduced = web_sys::window()
                .and_then(|w| {
                    w.match_media("(prefers-reduced-motion: reduce)")
                        .ok()
                        .flatten()
                })
                .map(|m| m.matches())
                .unwrap_or(false);

            // The loop always runs so clicks and the mouse brush stay
            // interactive; reduced motion only silences the idle wind.
            let wind_on = !reduced;
            let step_clicks = clicks.clone();
            let step_mouse = mouse.clone();
            let step_raf = raf.clone();
            let step_container = curtains_ref.clone();
            let mut sim: Option<Sim> = None;
            let mut acc = 0.0_f64;
            let mut last_t: Option<f64> = None;
            let mut prev_mouse: Option<(f64, f64)> = None;
            let cb = Closure::wrap(Box::new(move |time: f64| {
                if let Some(container) = step_container.cast::<web_sys::Element>() {
                    let w = container.client_width() as f64;
                    let h = container.client_height() as f64;
                    let stale = sim.as_ref().map_or(true, |s| s.w != w || s.h != h);
                    if stale {
                        sim = Some(build_sim(&container));
                    }
                    if let Some(sim) = sim.as_mut() {
                        apply_clicks(sim, &mut step_clicks.borrow_mut(), time);

                        let cur_mouse = (*step_mouse.borrow()).map(|(mx, my)| {
                            let r = container.get_bounding_client_rect();
                            (mx - r.left(), my - r.top())
                        });

                        let dt = last_t
                            .map(|lt| ((time - lt) / 1000.0).clamp(0.0, MAX_FRAME_DT))
                            .unwrap_or(0.0);
                        last_t = Some(time);
                        acc += dt;
                        let n_steps = (acc / SUBSTEP).floor() as usize;
                        for k in 0..n_steps {
                            // Sweep the cursor across substeps so a fast move
                            // still brushes every string on its path.
                            let f = (k + 1) as f64 / n_steps as f64;
                            let m = match (prev_mouse, cur_mouse) {
                                (Some(a), Some(b)) => {
                                    Some((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f))
                                }
                                (None, cur) => cur,
                                (_, None) => None,
                            };
                            substep(sim, time / 1000.0, wind_on, m);
                        }
                        acc -= n_steps as f64 * SUBSTEP;
                        prev_mouse = cur_mouse;
                        render(sim, &container);
                    }
                }
                if let Some(cb) = step_raf.borrow().as_ref() {
                    let _ = web_sys::window()
                        .unwrap()
                        .request_animation_frame(cb.as_ref().unchecked_ref());
                }
            }) as Box<dyn FnMut(f64)>);
            *raf.borrow_mut() = Some(cb);
            if let Some(cb) = raf.borrow().as_ref() {
                let _ = web_sys::window()
                    .unwrap()
                    .request_animation_frame(cb.as_ref().unchecked_ref());
            }
            || ()
        });
    }

    // Left press kicks the curtains apart and grabs the cursor as a collider;
    // clicks layer on top of each other.
    let onmousedown = {
        let curtains_ref = curtains_ref.clone();
        let clicks = clicks.clone();
        let mouse = mouse.clone();
        Callback::from(move |e: MouseEvent| {
            if e.button() != 0 {
                return;
            }
            *mouse.borrow_mut() = Some((e.client_x() as f64, e.client_y() as f64));
            let Some(container) = curtains_ref.cast::<web_sys::Element>() else {
                return;
            };
            let w = container.client_width() as f64;
            if w <= 0.0 {
                return;
            }
            let now = web_sys::window()
                .and_then(|win| win.performance())
                .map(|p| p.now())
                .unwrap_or(0.0);
            clicks.borrow_mut().push(Click {
                x: (e.client_x() as f64 / w).clamp(0.0, 1.0),
                start: now,
                kicked: vec![false; COLUMN_COUNT],
            });
        })
    };

    // Dragging with the button down sweeps the collider through the strings.
    // `buttons()` bit 0 is the left button — if it came up outside the window
    // we never saw the mouseup, so this also releases a stale drag.
    let onmousemove = {
        let mouse = mouse.clone();
        Callback::from(move |e: MouseEvent| {
            let mut m = mouse.borrow_mut();
            if m.is_none() {
                return; // hovering without a press: curtains stay untouched
            }
            *m = if e.buttons() & 1 != 0 {
                Some((e.client_x() as f64, e.client_y() as f64))
            } else {
                None
            };
        })
    };
    // Release (or leaving the stage) lets the strings swing back.
    let onmouseup = {
        let mouse = mouse.clone();
        Callback::from(move |e: MouseEvent| {
            if e.button() == 0 {
                *mouse.borrow_mut() = None;
            }
        })
    };
    let onmouseleave = {
        let mouse = mouse.clone();
        Callback::from(move |_: MouseEvent| {
            *mouse.borrow_mut() = None;
        })
    };

    html! {
        <main class="stage" {onmousedown} {onmousemove} {onmouseup} {onmouseleave}>
            <Curtains container_ref={curtains_ref.clone()} height={*height} />
            <Arches />
        </main>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
