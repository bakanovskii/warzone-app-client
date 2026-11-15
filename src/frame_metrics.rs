//! Bounded render timing evidence. Never touches simulation time or saved state.
use std::collections::BTreeMap;
#[derive(Default)]
struct Samples {
    count: u64,
    total_ms: f64,
    maximum_ms: f64,
    over_budget: u64,
    values: Vec<f64>,
}
impl Samples {
    fn push(&mut self, ms: f64) {
        self.count += 1;
        self.total_ms += ms;
        self.maximum_ms = self.maximum_ms.max(ms);
        self.over_budget += u64::from(ms > 1000. / 60.);
        if self.values.len() < 2048 {
            self.values.push(ms)
        } else {
            let i = (self.count as usize - 1) % 2048;
            self.values[i] = ms;
        }
    }
    fn report(&self) -> serde_json::Value {
        let mut sorted = self.values.clone();
        sorted.sort_by(f64::total_cmp);
        let percentile = |p: f64| {
            sorted
                .get(((sorted.len().saturating_sub(1)) as f64 * p).ceil() as usize)
                .copied()
                .unwrap_or(0.)
        };
        serde_json::json!({"frames":self.count,"sampled_frames":sorted.len(),"mean_ms":if self.count==0{0.}else{self.total_ms/self.count as f64},"p50_ms":percentile(0.5),"p95_ms":percentile(0.95),"p99_ms":percentile(0.99),"max_ms":self.maximum_ms,"over_60hz_budget":self.over_budget})
    }
}
#[derive(Default)]
pub struct FrameMetrics {
    phases: BTreeMap<&'static str, (Samples, Samples)>,
    parts: BTreeMap<(&'static str, &'static str), (Samples, Samples)>,
}
impl FrameMetrics {
    pub fn observe(&mut self, phase: &'static str, ms: f64) {
        if !ms.is_finite() || ms < 0. {
            return;
        }
        let row = self.phases.entry(phase).or_default();
        row.0.push(ms);
        // First30 frames in each mode are separately visible in all_frames.
        // Excluding them only from steady_state exposes cold dialog spikes too.
        if row.0.count > 30 {
            row.1.push(ms);
        }
    }
    pub fn observe_parts(&mut self, phase: &'static str, ms: f64, parts: &[(&'static str, f64)]) {
        self.observe(phase, ms);
        let warm = self.phases.get(phase).is_some_and(|r| r.0.count > 30);
        for &(part, value) in parts {
            if value.is_finite() && value >= 0. {
                let row = self.parts.entry((phase, part)).or_default();
                row.0.push(value);
                if warm {
                    row.1.push(value);
                }
            }
        }
    }
    pub fn report(&self) -> serde_json::Value {
        serde_json::json!({"format":"warzone-desktop-render-timings-v1","scope":"CPU frame/update/draw/present wall time, before voluntary60Hz sleep; per-mode first30 frames excluded only from steady_state; each quantile uses at most last2048frames, maxima/counts span entire run; not GPU timestamp queries.","modes":self.phases.iter().map(|(name,(all,warm))|(*name,serde_json::json!({"all_frames":all.report(),"steady_state":warm.report(),"parts":self.parts.iter().filter(|((mode,_),_)|mode==name).map(|((_,part),(all,warm))|(*part,serde_json::json!({"all_frames":all.report(),"steady_state":warm.report()}))).collect::<BTreeMap<_,_>>()}))).collect::<BTreeMap<_,_>>()})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn component_timings_share_each_modes_warmup_and_bounded_storage() {
        let mut m = FrameMetrics::default();
        for _ in 0..2100 {
            m.observe_parts(
                "base",
                10.,
                &[("update", 2.), ("draw", 7.), ("present", 1.)],
            );
        }
        m.observe_parts("shop", 30., &[("draw", 29.), ("present", 1.)]);
        let r = m.report();
        let base = &r["modes"]["base"];
        assert_eq!(base["steady_state"]["frames"], 2070);
        assert_eq!(base["parts"]["draw"]["steady_state"]["frames"], 2070);
        assert_eq!(
            base["parts"]["draw"]["steady_state"]["sampled_frames"],
            2048
        );
        assert_eq!(base["parts"]["draw"]["steady_state"]["mean_ms"], 7.);
        assert_eq!(r["modes"]["shop"]["steady_state"]["frames"], 0);
    }
}
