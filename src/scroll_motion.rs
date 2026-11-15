//! Transient UIScrollView-style edge resistance and spring return.
//! Does not enter the profile or change a list's content extent.
pub fn rubber(value: f64, maximum: f64, viewport: f64) -> f64 {
    let edge = value.clamp(0., maximum.max(0.));
    let delta = value - edge;
    let extent = viewport.max(1.);
    edge + delta.signum() * extent * (1. - 1. / (1. + 0.55 * delta.abs() / extent))
}
pub fn settle(value: f64, maximum: f64, elapsed: f64) -> f64 {
    let edge = value.clamp(0., maximum.max(0.));
    let result = edge + (value - edge) * (-16. * elapsed.clamp(0., 0.1)).exp();
    if (result - edge).abs() < 0.1 {
        edge
    } else {
        result
    }
}
/// UIScrollView's normal decelerationRate=.998 per millisecond. Integrate it
/// analytically so coalescing pointer events does not alter the travel distance.
pub fn coast(value: f64, velocity: &mut f64, maximum: f64, elapsed: f64) -> f64 {
    let dt = elapsed.clamp(0., 0.1);
    let edge = value.clamp(0., maximum.max(0.));
    if value != edge {
        *velocity = 0.;
        return settle(value, maximum, dt);
    }
    let decay = (-2.00200267 * dt).exp();
    let next = value + *velocity * (1. - decay) / 2.00200267;
    *velocity *= decay;
    if velocity.abs() < 4. {
        *velocity = 0.;
    }
    let bounded = next.clamp(0., maximum.max(0.));
    if bounded != next {
        *velocity = 0.;
        bounded + (next - bounded) * 0.35
    } else {
        next
    }
}
