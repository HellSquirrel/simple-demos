//! Blackbody Candle: a reacting, buoyant 2D flow of a candle flame, rendered with Planck's law through the
//! CIE 1931 observer. `physics` is the simulation, `render` turns its state into light and readings, and this file is
//! the plain C ABI the page calls: JS reads the results straight out of linear memory.
#![allow(static_mut_refs, clippy::too_many_arguments)]

pub mod physics;
pub mod render;
pub use physics::{Sim, H, N, W};
pub use render::{visible_share, Renderer, BH, BW};

static mut SIM: Option<Sim> = None;
static mut REND: Option<Renderer> = None;
#[inline(always)]
fn sim() -> &'static mut Sim { unsafe { SIM.as_mut().unwrap_unchecked() } }
#[inline(always)]
fn rend() -> &'static mut Renderer { unsafe { REND.as_mut().unwrap_unchecked() } }

#[no_mangle] pub extern "C" fn fl_init() { unsafe { SIM = Some(Sim::new()); REND = Some(Renderer::new()); } }
#[no_mangle] pub extern "C" fn fl_seed(s: u32) { sim().seed(s); }
#[no_mangle] pub extern "C" fn fl_reset() { sim().reset(); }
#[no_mangle] pub extern "C" fn fl_step(dt: f32) { sim().step(dt); }
#[no_mangle] pub extern "C" fn fl_render() { rend().render(sim()); }
#[no_mangle] pub extern "C" fn fl_strike() -> i32 { sim().strike() as i32 }
#[no_mangle] pub extern "C" fn fl_blow(speed: f32, dur: f32) { sim().blow(speed, dur); }
#[no_mangle] pub extern "C" fn fl_poke(x: f32, y: f32, vx: f32, vy: f32) { sim().poke(x, y, vx, vy); }
#[no_mangle] pub extern "C" fn fl_set_room(o2: f32, g: f32, swirl: f32, draft: f32) { sim().set_room(o2, g, swirl, draft); }
#[no_mangle] pub extern "C" fn fl_set_camera(expo: f32, wb_k: f32, chemi_gain: f32) {
    let r = rend(); r.expo = expo; r.chemi_gain = chemi_gain; r.set_wb(wb_k);
}
#[no_mangle] pub extern "C" fn fl_set_view(v: u32) { rend().view = v; }
#[no_mangle] pub extern "C" fn fl_grey_rgb(t: f32, gain: f32) { rend().grey_rgb(t, gain); }
#[no_mangle] pub extern "C" fn fl_visible_share(t: f32) -> f32 { visible_share(t as f64) as f32 }
#[no_mangle] pub extern "C" fn fl_linear() -> *const f32 { rend().linear().as_ptr() }
#[no_mangle] pub extern "C" fn fl_bloom() -> *const f32 { rend().bloom().as_ptr() }
#[no_mangle] pub extern "C" fn fl_stats() -> *const f32 { rend().stats.as_ptr() }
#[no_mangle] pub extern "C" fn fl_matrix() -> *const f32 { rend().matrix().as_ptr() }
#[no_mangle] pub extern "C" fn fl_scratch() -> *const f32 { rend().scratch().as_ptr() }
#[no_mangle] pub extern "C" fn fl_dims() -> u32 { ((W as u32) << 16) | H as u32 }
#[no_mangle] pub extern "C" fn fl_bloomdims() -> u32 { ((BW as u32) << 16) | BH as u32 }
#[no_mangle] pub extern "C" fn fl_head_x() -> u32 { physics::HEAD_X as u32 }
