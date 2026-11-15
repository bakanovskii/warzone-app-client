//! TriggerAction MoveCameraAction -> SceneBattle.onTriggerScrollToX:Y:timems:
//! Source0x628a9: easeOut cubic, milliseconds*.001, vertical anchor3*height/5.
use crate::{Camera, Projection};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub serial: u64,
    pub point: [i32; 2],
    pub milliseconds: u32,
}
impl Request {
    pub fn target(&self, projection: &Projection, camera: &Camera) -> [f64; 2] {
        let p = projection.project(self.point[0] as f64, self.point[1] as f64);
        // Root y=.6screenHeight-worldY*zoom; native camera uses screen center.
        [
            p[0].trunc(),
            p[1].trunc() - camera.height as f64 * 0.1 / camera.zoom,
        ]
    }
    pub fn pose(&self, from: [f64; 2], to: [f64; 2], seconds: f64) -> ([f64; 2], bool) {
        let t = if self.milliseconds == 0 {
            1.
        } else {
            (seconds * 1000. / self.milliseconds as f64).clamp(0., 1.)
        };
        let p = 1. + (t - 1.).powi(3);
        (
            [
                from[0] + (to[0] - from[0]) * p,
                from[1] + (to[1] - from[1]) * p,
            ],
            t >= 1.,
        )
    }
}
