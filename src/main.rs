use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use yew::prelude::*;

const PHRASE: &str = "他人事の音がする";
const COLUMN_COUNT: usize = 50;
// Each column is a string built from vertically-stacked joints; more joints =
// smoother bending. Each joint holds enough text to fill its slice.
const SEGMENTS: usize = 9;
const SEG_REPEATS: usize = 3;

// A single click ripple: where it landed (0..1 across the width) and when.
#[derive(Clone, Copy)]
struct Ripple {
    x: f64,
    start: f64,
}

// Ripple tuning.
const SPREAD: f64 = 0.55; // seconds for the wave to cross the full width
const DECAY: f64 = 0.6; // seconds — how fast each string settles
const FREQ: f64 = 2.2; // Hz — swing frequency of the strings
const AMP: f64 = 1.0; // base kick strength
const DIST_SIGMA: f64 = 0.28; // how far the kick reaches around the click
const DOWN_PROP: f64 = 0.18; // seconds for the swing to travel down a string
const SWING_PX: f64 = 60.0; // horizontal swing of the free (bottom) end, px

#[derive(Properties, PartialEq)]
struct CurtainsProps {
    container_ref: NodeRef,
}

#[function_component]
fn Curtains(props: &CurtainsProps) -> Html {
    // Each joint holds a short run of the phrase; it's clipped to its slice.
    let seg_text: String = PHRASE.repeat(SEG_REPEATS);

    let columns = (0..COLUMN_COUNT)
        .map(|i| {
            // Deterministic variation from the index.
            let opacity = 0.4 + ((i * 7) % 30) as f64 / 100.0; // 0.18 – 0.48
            let offset = ((i * 13) % 50) as f64; // uneven top → drip
            // Desynchronized, gentle drift: slow duration + negative start delay
            // so each column is at a different point in its sway/bounce cycle.
            let drift_dur = 4.5 + ((i * 11) % 40) as f64 / 10.0; // 4.5 – 8.4s
            let drift_delay = -(((i * 7) % 60) as f64) / 10.0; // -0.0 – -5.9s
            // Drip the top down by `offset` but shrink the height by the same
            // amount so every column's bottom stays flush with the floor — that
            // way the vertical bounce lifts the bottom edge into view.
            let style = format!(
                "opacity: {opacity}; margin-top: {offset}px; \
                 height: calc(100% - {offset}px); \
                 animation-duration: {drift_dur}s; animation-delay: {drift_delay}s;"
            );

            // The column is a `.string` of stacked `.seg` joints. The whole
            // string runs the autonomous bounce/sway; the ripple loop swings
            // each joint independently so the string bends like real fabric.
            let joints = (0..SEGMENTS)
                .map(|_| html! { <div class="seg">{ &seg_text }</div> })
                .collect::<Html>();

            html! {
                <div class="string" style={style}>{ joints }</div>
            }
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
            // White wall; the bottom edge follows three arches that meet at points.
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

/// Write every joint's horizontal displacement for the current moment of a
/// ripple, and return `true` while the ripple is still alive.
fn apply_ripple(container: &web_sys::Element, ripple: &Ripple, now: f64) -> bool {
    let elapsed = (now - ripple.start) / 1000.0; // seconds since the click

    // The last string to be reached needs SPREAD; then the swing has to travel
    // down it (DOWN_PROP) and settle (a few DECAY constants). After that, rest.
    let done = elapsed > SPREAD + DOWN_PROP + DECAY * 6.0;

    let columns = container.children();
    let ncol = columns.length();
    for i in 0..ncol {
        let Some(col) = columns.item(i) else { continue };
        let cx = if ncol > 1 {
            i as f64 / (ncol as f64 - 1.0)
        } else {
            0.5
        };

        // Per-column kick: strings part *away* from the impact, harder the
        // closer they are, and only once the surface wave reaches them.
        let signed = cx - ripple.x;
        let dist = signed.abs();
        let dir = if signed > 0.0 {
            1.0
        } else if signed < 0.0 {
            -1.0
        } else {
            0.0
        };
        let reach = AMP / (1.0 + (dist / DIST_SIGMA).powi(2));
        let col_local = elapsed - dist * SPREAD;

        let joints = col.children();
        let k = joints.length();
        for j in 0..k {
            let Some(seg) = joints
                .item(j)
                .and_then(|c| c.dyn_into::<web_sys::HtmlElement>().ok())
            else {
                continue;
            };
            // Depth of this joint along the string, 0 (pinned top) → 1 (free end).
            let depth = (j as f64 + 0.5) / k as f64;

            let dx = if done {
                0.0
            } else {
                // The swing propagates down the string, so lower joints lag.
                let local = col_local - depth * DOWN_PROP;
                if local <= 0.0 {
                    0.0
                } else {
                    let envelope = (-local / DECAY).exp();
                    let swing = (std::f64::consts::TAU * FREQ * local).sin();
                    // Amplitude grows toward the free end (pinned top stays put).
                    dir * reach * envelope * swing * depth * SWING_PX
                }
            };

            let _ = seg.style().set_property("--dx", &format!("{dx}px"));
        }
    }

    !done
}

#[function_component]
fn App() -> Html {
    let curtains_ref = use_node_ref();
    // Persistent state that survives re-renders.
    let ripple = use_mut_ref(|| None::<Ripple>);
    let running = use_mut_ref(|| false);
    let raf = use_mut_ref(|| None::<Closure<dyn FnMut(f64)>>);

    // Build the animation-frame callback once, on mount. It reads the current
    // ripple, updates every column, and reschedules itself until the ripple
    // has fully settled.
    {
        let ripple = ripple.clone();
        let running = running.clone();
        let raf = raf.clone();
        let curtains_ref = curtains_ref.clone();
        use_effect_with((), move |_| {
            let step_ripple = ripple.clone();
            let step_running = running.clone();
            let step_raf = raf.clone();
            let step_container = curtains_ref.clone();
            let cb = Closure::wrap(Box::new(move |time: f64| {
                let current = *step_ripple.borrow();
                let keep_going = match (current, step_container.cast::<web_sys::Element>()) {
                    (Some(r), Some(container)) => apply_ripple(&container, &r, time),
                    _ => false,
                };
                if keep_going {
                    if let Some(cb) = step_raf.borrow().as_ref() {
                        let _ = web_sys::window()
                            .unwrap()
                            .request_animation_frame(cb.as_ref().unchecked_ref());
                    }
                } else {
                    *step_ripple.borrow_mut() = None;
                    *step_running.borrow_mut() = false;
                }
            }) as Box<dyn FnMut(f64)>);
            *raf.borrow_mut() = Some(cb);
            || ()
        });
    }

    // Left click drops a ripple at the cursor and kicks off the loop.
    let onmousedown = {
        let curtains_ref = curtains_ref.clone();
        let ripple = ripple.clone();
        let running = running.clone();
        let raf = raf.clone();
        Callback::from(move |e: MouseEvent| {
            if e.button() != 0 {
                return; // left button only
            }
            let Some(container) = curtains_ref.cast::<web_sys::Element>() else {
                return;
            };
            let w = container.client_width() as f64;
            if w <= 0.0 {
                return;
            }
            let window = web_sys::window().unwrap();
            let now = window.performance().map(|p| p.now()).unwrap_or(0.0);
            *ripple.borrow_mut() = Some(Ripple {
                x: (e.client_x() as f64 / w).clamp(0.0, 1.0),
                start: now,
            });
            // Start the loop only if it isn't already running.
            if !*running.borrow() {
                *running.borrow_mut() = true;
                if let Some(cb) = raf.borrow().as_ref() {
                    let _ = window.request_animation_frame(cb.as_ref().unchecked_ref());
                }
            }
        })
    };

    html! {
        <main class="stage" {onmousedown}>
            <Curtains container_ref={curtains_ref.clone()} />
            <Arches />
        </main>
    }
}

fn main() {
    yew::Renderer::<App>::new().render();
}
