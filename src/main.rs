use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use yew::prelude::*;

const PHRASE: &str = "他人事の音がする";
const COLUMN_COUNT: usize = 50;
// Thin slices bend smoothly; each holds enough text to fill its slice.
const SEGMENTS: usize = 20;
const SEG_REPEATS: usize = 2;

// ---- Wind: always-on idle sway (replaces the CSS drift) ----
const WIND_AMP: f64 = 16.0; // px at the free end
const WIND_FREQ: f64 = 0.16; // Hz — primary sway
const WIND_FREQ2: f64 = 0.045; // Hz — slow gust modulation
const WIND_SPATIAL: f64 = 5.0; // radians of phase across the width
const WIND_DEPTH_LAG: f64 = 1.7; // radians of phase from top to bottom

// ---- Ripple (click) ----
const SPREAD: f64 = 0.55; // s for the wave to cross the full width
const DECAY: f64 = 0.6; // s — how fast each string settles
const FREQ: f64 = 2.2; // Hz — swing frequency
const AMP: f64 = 1.0; // base kick strength
const DIST_SIGMA: f64 = 0.28; // how far the kick reaches around the click
const DOWN_PROP: f64 = 0.18; // s for the swing to travel down a string
const SWING_PX: f64 = 60.0; // horizontal swing of the free end, px
const RIPPLE_LIFE: f64 = SPREAD + DOWN_PROP + DECAY * 6.0;

#[derive(Clone, Copy)]
struct Ripple {
    x: f64,     // 0..1 across the width
    start: f64, // ms, performance.now() timeline
}

/// Idle sway: a traveling wave with per-column phase and a slow gust cycle.
fn wind_dx(cx: f64, depth: f64, t: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let main = (tau * WIND_FREQ * t - cx * WIND_SPATIAL - depth * WIND_DEPTH_LAG).sin();
    let gust = 0.5 + 0.5 * (tau * WIND_FREQ2 * t + cx * 2.3).sin();
    WIND_AMP * depth.powf(1.4) * main * (0.35 + 0.65 * gust)
}

/// One click's contribution to a joint, in px. `now` is in ms.
fn ripple_dx(r: &Ripple, cx: f64, depth: f64, now: f64) -> f64 {
    let elapsed = (now - r.start) / 1000.0;
    let signed = cx - r.x;
    let dist = signed.abs();
    let dir = if signed >= 0.0 { 1.0 } else { -1.0 };
    let reach = AMP / (1.0 + (dist / DIST_SIGMA).powi(2));
    // Wave reaches this column, then travels down the string.
    let local = elapsed - dist * SPREAD - depth * DOWN_PROP;
    if local <= 0.0 {
        return 0.0;
    }
    let envelope = (-local / DECAY).exp();
    dir * reach * envelope * (std::f64::consts::TAU * FREQ * local).sin() * depth * SWING_PX
}

/// Total horizontal displacement of a joint at `depth` on column `cx`.
fn total_dx(cx: f64, depth: f64, t_ms: f64, ripples: &[Ripple]) -> f64 {
    let mut dx = wind_dx(cx, depth, t_ms / 1000.0);
    for r in ripples {
        dx += ripple_dx(r, cx, depth, t_ms);
    }
    dx
}

/// Write transform for every joint for this frame. Each slice is translated
/// by the curve and rotated by its local slope, so slices join into a bend.
fn apply_frame(container: &web_sys::Element, ripples: &[Ripple], t_ms: f64) {
    let columns = container.children();
    let ncol = columns.length();
    for i in 0..ncol {
        let Some(col) = columns.item(i) else { continue };
        let cx = if ncol > 1 {
            i as f64 / (ncol as f64 - 1.0)
        } else {
            0.5
        };
        let col_h = (col.client_height() as f64).max(1.0);
        let joints = col.children();
        let k = joints.length().max(1);
        let seg_h = col_h / k as f64;
        let dd = 1.0 / k as f64; // one slice of depth

        for j in 0..k {
            let Some(seg) = joints
                .item(j)
                .and_then(|c| c.dyn_into::<web_sys::HtmlElement>().ok())
            else {
                continue;
            };
            let depth = (j as f64 + 0.5) / k as f64;
            let dx = total_dx(cx, depth, t_ms, ripples);
            // Local slope of the curve → tilt the slice to follow it.
            let dx_below = total_dx(cx, depth + dd, t_ms, ripples);
            let rot = ((dx_below - dx) / seg_h).atan().to_degrees();
            let _ = seg.style().set_property(
                "transform",
                &format!("translateX({dx:.2}px) rotate({rot:.2}deg)"),
            );
        }
    }
}

#[derive(Properties, PartialEq)]
struct CurtainsProps {
    container_ref: NodeRef,
}

#[function_component]
fn Curtains(props: &CurtainsProps) -> Html {
    let seg_text: String = PHRASE.repeat(SEG_REPEATS);

    let columns = (0..COLUMN_COUNT)
        .map(|i| {
            let opacity = 0.4 + ((i * 7) % 30) as f64 / 100.0;
            let offset = ((i * 13) % 50) as f64; // uneven top → drip
            let style = format!(
                "opacity: {opacity}; margin-top: {offset}px; \
                 height: calc(100% - {offset}px);"
            );

            let joints = (0..SEGMENTS)
                .map(|_| html! { <div class="seg">{ &seg_text }</div> })
                .collect::<Html>();

            html! { <div class="string" style={style}>{ joints }</div> }
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
    let ripples = use_mut_ref(Vec::<Ripple>::new);
    let raf = use_mut_ref(|| None::<Closure<dyn FnMut(f64)>>);

    // One always-running frame loop: wind + any live ripples.
    {
        let ripples = ripples.clone();
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

            if !reduced {
                let step_ripples = ripples.clone();
                let step_raf = raf.clone();
                let step_container = curtains_ref.clone();
                let cb = Closure::wrap(Box::new(move |time: f64| {
                    if let Some(container) = step_container.cast::<web_sys::Element>() {
                        let mut rs = step_ripples.borrow_mut();
                        rs.retain(|r| (time - r.start) / 1000.0 < RIPPLE_LIFE);
                        apply_frame(&container, &rs, time);
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
            }
            || ()
        });
    }

    // Left click drops a ripple; multiple clicks layer on top of each other.
    let onmousedown = {
        let curtains_ref = curtains_ref.clone();
        let ripples = ripples.clone();
        Callback::from(move |e: MouseEvent| {
            if e.button() != 0 {
                return;
            }
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
            ripples.borrow_mut().push(Ripple {
                x: (e.client_x() as f64 / w).clamp(0.0, 1.0),
                start: now,
            });
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
