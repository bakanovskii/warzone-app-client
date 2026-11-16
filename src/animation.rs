//! Complete original ambient timelines. Nested clips share elapsed time, but
//! loop independently; a parent wrapping never resets its children's clocks.
use crate::{Scene, Sprite, Track};
use serde::Deserialize;
use std::{collections::HashMap, path::Path, sync::Arc};

const STACK: usize = 64;

#[derive(Debug, Deserialize)]
pub struct Clip {
    pub texture: String,
    pub symbol: String,
    pub fps: f64,
    pub offset: [f64; 2],
    pub roots: Vec<usize>,
    pub nodes: Vec<Node>,
}
#[derive(Debug, Deserialize)]
pub struct Node {
    pub pos: [f64; 2],
    #[serde(flatten)]
    pub kind: Kind,
}
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Kind {
    Clip { frames: Vec<Vec<usize>> },
    Image { sprite: Sprite },
    Change { target: usize },
}
#[derive(Clone, Copy, Default)]
pub struct Visit {
    index: usize,
    offset: [f64; 2],
}
// The bounded inline stack avoids a heap allocation for every animated object
// on every frame. Only a few visible children are traversed, never all frames.
#[allow(clippy::large_enum_variant)]
pub enum Frame<'a> {
    Flat(std::iter::Cloned<std::slice::Iter<'a, Sprite>>),
    Graph {
        clip: &'a Clip,
        tick: usize,
        stack: [Visit; STACK],
        len: usize,
    },
}
impl<'a> Frame<'a> {
    pub fn graph(clip: &'a Clip, seconds: f64) -> Self {
        let mut stack = [Visit::default(); STACK];
        for (entry, &index) in stack.iter_mut().zip(clip.roots.iter().rev()) {
            *entry = Visit {
                index,
                offset: clip.offset,
            };
        }
        Self::Graph {
            clip,
            tick: (seconds.max(0.) * clip.fps).floor() as usize,
            stack,
            len: clip.roots.len(),
        }
    }
}
impl Iterator for Frame<'_> {
    type Item = Sprite;
    fn next(&mut self) -> Option<Sprite> {
        match self {
            Self::Flat(iter) => iter.next(),
            Self::Graph {
                clip,
                tick,
                stack,
                len,
            } => {
                while *len > 0 {
                    *len -= 1;
                    let Visit { index, offset } = stack[*len];
                    match &clip.nodes[index].kind {
                        Kind::Clip { frames } => {
                            for &index in frames[*tick % frames.len()].iter().rev() {
                                let pos = clip.nodes[index].pos;
                                stack[*len] = Visit {
                                    index,
                                    offset: [offset[0] + pos[0], offset[1] + pos[1]],
                                };
                                *len += 1;
                            }
                        }
                        Kind::Change { target } => {
                            // AnimationChange supplies the replacement position;
                            // the referenced object's old pos must not be added.
                            stack[*len] = Visit {
                                index: *target,
                                offset,
                            };
                            *len += 1;
                        }
                        Kind::Image { sprite } => {
                            let mut sprite = sprite.clone();
                            sprite.rect[0] += offset[0];
                            sprite.rect[1] += offset[1];
                            return Some(sprite);
                        }
                    }
                }
                None
            }
        }
    }
}
impl Clip {
    fn validate(&self, scene: &Scene) -> Result<(), String> {
        let image = scene
            .textures
            .get(&self.texture)
            .ok_or("Missing ambient atlas")?;
        if !self.fps.is_finite()
            || !(0.1..=240.).contains(&self.fps)
            || self.roots.is_empty()
            || self.roots.len() > 2
            || self.nodes.is_empty()
            || self.nodes.len() > 12000
            || self.offset.iter().any(|v| !v.is_finite())
        {
            return Err("Invalid ambient graph".into());
        }
        // Validate every branch once, including inactive frames and references.
        // Cached depth/stack/draw bounds also reject cycles and runaway graphs.
        fn bounds(
            clip: &Clip,
            i: usize,
            active: &mut [bool],
            memo: &mut [Option<[usize; 3]>],
        ) -> Result<[usize; 3], String> {
            let node = clip.nodes.get(i).ok_or("Invalid ambient node reference")?;
            if active[i] {
                return Err("Ambient reference cycle".into());
            }
            if let Some(b) = memo[i] {
                return Ok(b);
            }
            active[i] = true;
            let b = match &node.kind {
                Kind::Image { .. } => [1, 1, 1],
                Kind::Change { target } => {
                    let [d, s, n] = bounds(clip, *target, active, memo)?;
                    [d + 1, s, n]
                }
                Kind::Clip { frames } => {
                    if frames.is_empty() || frames.len() > 2000 {
                        return Err("Invalid ambient frame count".into());
                    }
                    let mut b = [1, 1, 0];
                    for frame in frames {
                        let mut draws = 0;
                        for (j, &child) in frame.iter().enumerate() {
                            let [depth, stack, count] = bounds(clip, child, active, memo)?;
                            b[0] = b[0].max(depth + 1);
                            b[1] = b[1].max(frame.len() - j - 1 + stack);
                            draws += count;
                        }
                        b[2] = b[2].max(draws);
                    }
                    b
                }
            };
            if b[0] > 32 || b[1] + clip.roots.len() > STACK || b[2] > 128 {
                return Err("Ambient traversal exceeds supported bounds".into());
            }
            active[i] = false;
            memo[i] = Some(b);
            Ok(b)
        }
        for node in &self.nodes {
            if node.pos.iter().any(|v| !v.is_finite() || v.abs() > 100000.) {
                return Err("Invalid ambient position".into());
            }
            if let Kind::Image { sprite } = &node.kind {
                let [x, y, w, h] = sprite.src;
                if w == 0
                    || h == 0
                    || x.checked_add(w).is_none_or(|v| v > image.width)
                    || y.checked_add(h).is_none_or(|v| v > image.height)
                    || sprite.rect.iter().any(|v| !v.is_finite())
                    || sprite.rect[2] <= 0.
                    || sprite.rect[3] <= 0.
                {
                    return Err("Invalid ambient sprite".into());
                }
            }
        }
        let mut active = vec![false; self.nodes.len()];
        let mut memo = vec![None; self.nodes.len()];
        for i in 0..self.nodes.len() {
            bounds(self, i, &mut active, &mut memo)?;
        }
        for &i in &self.roots {
            bounds(self, i, &mut active, &mut memo)?;
        }
        Ok(())
    }
}

#[derive(Deserialize)]
struct Pack {
    format: String,
    clips: Vec<Arc<Clip>>,
}

fn same(a: &[Sprite], b: &[Sprite]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.src == b.src
                && a.rect
                    .iter()
                    .zip(b.rect)
                    .all(|(a, b)| (a - b).abs() < 0.00001)
        })
}

pub fn load(scene: &mut Scene, pack: &Path) -> Result<(), String> {
    let raw =
        std::fs::read(crate::contained_file(pack, "ambient.json")?).map_err(|e| e.to_string())?;
    if raw.len() > 16 * 1024 * 1024 {
        return Err("Ambient pack too large".into());
    }
    let data: Pack = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    if data.format != "warzone-ambient-v1" || data.clips.len() > 2000 {
        return Err("Invalid ambient pack".into());
    }
    let mut by_texture: HashMap<String, Vec<Arc<Clip>>> = HashMap::new();
    for clip in data.clips {
        clip.validate(scene)?;
        by_texture
            .entry(clip.texture.clone())
            .or_default()
            .push(clip);
    }
    let bind = |t: &mut Track| {
        if t.fps <= 0. {
            return;
        }
        if let Some(clip) = by_texture.get(&t.texture).and_then(|clips| {
            clips.iter().find(|clip| {
                (0..t.frames.len().min(3)).all(|i| {
                    same(
                        &t.frames[i],
                        &Frame::graph(clip, (i as f64 + 0.00001) / t.fps).collect::<Vec<_>>(),
                    )
                })
            })
        }) {
            t.ambient = Some(Arc::clone(clip));
        }
    };
    // Original scene.json stays byte-identical: saved seed ancestry and gameplay
    // receipts remain compatible. Only the in-memory cosmetic Track gains a clip.
    for_each_track(scene, bind);
    Ok(())
}

/// Stable traversal of cosmetic tracks, also used to retain transient clip
/// references through localization's text-only serde projection.
pub(crate) fn for_each_track(scene: &mut Scene, mut visit: impl FnMut(&mut Track)) {
    for e in &mut scene.entities {
        for t in &mut e.tracks {
            visit(t);
        }
    }
    if let Some(c) = &mut scene.world {
        for d in c.catalog.values_mut() {
            for ts in d
                .variants
                .values_mut()
                .chain(d.stages.values_mut().flat_map(|v| v.values_mut()))
            {
                for t in ts {
                    visit(t);
                }
            }
        }
        for work in c.working.values_mut() {
            for ts in work.normal.values_mut().chain(work.alternate.values_mut()) {
                for t in ts {
                    visit(t);
                }
            }
        }
    }
    if let Some(c) = &mut scene.upgrades {
        for m in c.models.values_mut() {
            for ts in m.variants.values_mut() {
                for t in ts {
                    visit(t);
                }
            }
        }
    }
    if let Some(c) = &mut scene.combat {
        for ts in c
            .sprites
            .values_mut()
            .chain(c.views.values_mut().flat_map(|v| v.sprites.values_mut()))
        {
            for t in ts {
                visit(t);
            }
        }
        // One-shot combat animations retain their existing duration and clocks.
    }
}

/// Stable object-local phase; independent of draw order, selection, camera,
/// refresh rate and simulation transactions. Body and shadow use the same time.
pub fn idle_time(uid: &str, seconds: f64) -> f64 {
    let mut hash = uid.bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    });
    // Avalanche short neighbouring battle IDs too (p1/p2/e1/e2). FNV alone
    // leaves their upper bits almost identical and synchronizes those squads.
    hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d049bb133111eb);
    hash ^= hash >> 31;
    seconds + (hash >> 11) as f64 / ((1u64 << 53) as f64) * 97.
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn neighbouring_short_battle_ids_have_independent_phases() {
        let phases: std::collections::BTreeSet<_> = (1..=8)
            .map(|i| (idle_time(&format!("p{i}"), 0.) * 8.) as u64 % 50)
            .collect();
        assert!(phases.len() >= 5);
        assert_eq!(idle_time("p1", 3.25) - idle_time("p1", 2.25), 1.);
    }

    #[test]
    fn parent_wrap_and_change_do_not_reset_child_clock_or_add_old_position() {
        let clip: Clip = serde_json::from_value(json!({
            "texture":"test", "symbol":"nested", "fps":1., "offset":[0.,35.], "roots":[0],
            "nodes":[
                {"kind":"clip","pos":[0.,0.],"frames":[[1],[3]]},
                {"kind":"clip","pos":[2.,4.],"frames":[[2],[4],[5]]},
                {"kind":"image","pos":[0.,0.],"sprite":{"src":[0,0,8,8],"rect":[-1.,-2.,8.,8.]}},
                {"kind":"change","pos":[12.,14.],"target":1},
                {"kind":"image","pos":[0.,0.],"sprite":{"src":[8,0,8,8],"rect":[-1.,-2.,8.,8.]}},
                {"kind":"image","pos":[0.,0.],"sprite":{"src":[16,0,8,8],"rect":[-1.,-2.,8.,8.]}}
            ]
        }))
        .unwrap();
        for (time, x, y, src) in [
            (0.1, 1., 37., 0),
            (1.1, 11., 47., 8),
            (2.1, 1., 37., 16),
            (3.1, 11., 47., 0),
            (217.1, 11., 47., 8),
        ] {
            let frame: Vec<_> = Frame::graph(&clip, time).collect();
            assert_eq!(frame.len(), 1);
            assert_eq!(frame[0].rect, [x, y, 8., 8.]);
            assert_eq!(frame[0].src[0], src);
        }
    }
}
