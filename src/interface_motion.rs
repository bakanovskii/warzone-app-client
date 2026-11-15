//! CHUDBottom.updateAnimation: hide previous parent for0.15s, then show the
//! selected parent for0.15s. Generic PopupDialog remains immediate. No save state.
use crate::{Camera, interface::View};
const LEG: f64 = 0.15000000596046448;
fn ease(t: f64) -> f64 {
    let t = t.clamp(0., 1.);
    let (mut lo, mut hi): (f64, f64) = (0., 1.);
    for _ in 0..20 {
        let p = (lo + hi) * 0.5;
        let x = 3. * (1. - p).powi(2) * p * 0.42 + 3. * (1. - p) * p * p * 0.58 + p * p * p;
        if x < t { lo = p } else { hi = p }
    }
    let p = (lo + hi) * 0.5;
    3. * (1. - p) * p * p + p * p * p
}
#[derive(Default)]
pub struct BottomPlayback {
    key: Option<String>,
    size: [u32; 2],
    travel: f64,
    latest: Option<View>,
    previous: Option<(View, f64)>,
    started: f64,
}
fn shift(view: &mut View, dy: f64) {
    for draw in &mut view.draws {
        if draw.node.parents.iter().any(|p| p == "source_bottom_slide") {
            draw.node.rect[1] += dy;
            if let Some(r) = &mut draw.clip {
                r[1] += dy;
            }
        }
    }
}
impl BottomPlayback {
    pub fn blocks_point(&self, seconds: f64, point: [f64; 2]) -> bool {
        self.active(seconds)
            && point[1]
                >= self.size[1] as f64 - self.travel.max(self.previous.as_ref().map_or(0., |p| p.1))
    }
    pub fn active(&self, seconds: f64) -> bool {
        self.previous.is_some() && seconds - self.started < 2. * LEG
    }
    pub fn apply(
        &mut self,
        key: String,
        travel: f64,
        camera: &Camera,
        seconds: f64,
        view: &mut View,
    ) {
        let size = [camera.width, camera.height];
        if self.size != size {
            self.key = None;
            self.previous = None;
            self.latest = None;
            self.size = size;
        }
        if self.key.as_ref() != Some(&key) {
            if let Some(old) = self.latest.take() {
                self.previous = Some((old, self.travel));
                self.started = seconds;
            }
            self.key = Some(key);
            self.travel = travel;
        }
        // Only the small retained bottom-HUD view is cached, before world
        // markers, dialogs, tutorial masks or rewards are appended by Ui.
        self.latest = Some(view.clone());
        let elapsed = (seconds - self.started).max(0.);
        if let Some((previous, travel)) = &self.previous {
            if elapsed < LEG {
                *view = previous.clone();
                shift(view, *travel * ease(elapsed / LEG));
            } else if elapsed < 2. * LEG {
                shift(view, self.travel * (1. - ease((elapsed - LEG) / LEG)));
            } else {
                self.previous = None;
                return;
            }
            // UIKit animations without AllowUserInteraction hold these
            // controls until the animation completes. Never run a stale hit.
            let top = camera.height as f64 - self.travel.max(*travel);
            view.hits.retain(|h| h.rect[1] + h.rect[3] <= top);
            view.blocks
                .push([0., top, camera.width as f64, camera.height as f64 - top]);
        }
    }
}
