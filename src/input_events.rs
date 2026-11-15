//! Bound presentation work for high-rate mice without reordering button edges.
use sdl2::event::Event;
/// Gesture thresholds are CSS pixels on the web, not Retina buffer pixels.
pub fn slop(pixels: i32) -> i32 {
    (pixels as f64 * warzone_app_client::browser::display_density()).round() as i32
}
/// SDL reports mouse positions in window units; the renderer can use a larger
/// drawing buffer (DPI, fullscreen, browser canvas resize). Hit targets use it.
#[cfg(any(target_os = "emscripten", test))]
pub fn for_output(mut event: Event, window: (u32, u32), output: (u32, u32)) -> Event {
    let sx = output.0 as f64 / window.0.max(1) as f64;
    let sy = output.1 as f64 / window.1.max(1) as f64;
    let x = |v: i32| (v as f64 * sx).round() as i32;
    let y = |v: i32| (v as f64 * sy).round() as i32;
    match &mut event {
        Event::MouseButtonDown { x: px, y: py, .. } | Event::MouseButtonUp { x: px, y: py, .. } => {
            *px = x(*px);
            *py = y(*py);
        }
        Event::MouseMotion {
            x: px,
            y: py,
            xrel,
            yrel,
            ..
        } => {
            *px = x(*px);
            *py = y(*py);
            *xrel = x(*xrel);
            *yrel = y(*yrel);
        }
        Event::MouseWheel {
            mouse_x, mouse_y, ..
        } => {
            *mouse_x = x(*mouse_x);
            *mouse_y = y(*mouse_y);
        }
        Event::Window {
            win_event: sdl2::event::WindowEvent::SizeChanged(w, h),
            ..
        } => {
            *w = output.0 as i32;
            *h = output.1 as i32;
        }
        _ => {}
    }
    event
}
pub fn coalesce(events: impl Iterator<Item = Event>, cancel_pointer: bool) -> Vec<Event> {
    let mut out = Vec::new();
    let mut run = 0;
    for event in events {
        // A second finger may cancel before the first finger's synthetic down
        // has been drained. Dropping both queued edges prevents an accidental
        // placement/purchase; wheel events still carry the intended pinch.
        if cancel_pointer
            && matches!(
                event,
                Event::MouseButtonDown { .. } | Event::MouseButtonUp { .. }
            )
        {
            continue;
        }
        if let Event::MouseMotion {
            window_id,
            which,
            x,
            y,
            xrel,
            yrel,
            timestamp,
            mousestate,
        } = event
        {
            if let Some(Event::MouseMotion {
                window_id: previous_window,
                which: previous_mouse,
                ..
            }) = out.last()
            {
                if *previous_window != window_id || *previous_mouse != which {
                    run = 0;
                }
            } else {
                run = 0;
            }
            if run >= 2 {
                if let Some(Event::MouseMotion {
                    x: old_x,
                    y: old_y,
                    xrel: old_dx,
                    yrel: old_dy,
                    timestamp: old_time,
                    mousestate: old_buttons,
                    ..
                }) = out.last_mut()
                {
                    *old_x = x;
                    *old_y = y;
                    *old_dx = old_dx.saturating_add(xrel);
                    *old_dy = old_dy.saturating_add(yrel);
                    *old_time = timestamp;
                    *old_buttons = mousestate;
                }
            } else {
                out.push(Event::MouseMotion {
                    window_id,
                    which,
                    x,
                    y,
                    xrel,
                    yrel,
                    timestamp,
                    mousestate,
                });
                run += 1;
            }
        } else {
            run = 0;
            out.push(event);
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    use sdl2::mouse::{MouseButton, MouseState};
    #[test]
    fn browser_mouse_units_match_the_rendered_button_after_fullscreen() {
        let event = Event::MouseButtonUp {
            timestamp: 1,
            window_id: 1,
            which: 0,
            mouse_btn: MouseButton::Left,
            clicks: 1,
            x: 980,
            y: 720,
        };
        let Event::MouseButtonUp { x, y, .. } = for_output(event, (1100, 760), (1920, 1080)) else {
            panic!()
        };
        assert_eq!((x, y), (1711, 1023));
        assert!(warzone_app_client::interface::contains(
            [1646., 972., 125., 103.],
            [x as f64, y as f64]
        ));
    }
    #[test]
    fn mouse_flood_preserves_total_pan_button_edges_and_final_pointer() {
        let down = Event::MouseButtonDown {
            timestamp: 0,
            window_id: 1,
            which: 0,
            mouse_btn: MouseButton::Left,
            clicks: 1,
            x: 0,
            y: 0,
        };
        let up = Event::MouseButtonUp {
            timestamp: 1001,
            window_id: 1,
            which: 0,
            mouse_btn: MouseButton::Left,
            clicks: 1,
            x: 1000,
            y: 2000,
        };
        let motions = (1..=1000).map(|i| Event::MouseMotion {
            timestamp: i as u32,
            window_id: 1,
            which: 0,
            mousestate: MouseState::from_sdl_state(1),
            x: i,
            y: 2 * i,
            xrel: 1,
            yrel: 2,
        });
        let reduced = coalesce(
            std::iter::once(down.clone())
                .chain(motions)
                .chain(std::iter::once(up.clone())),
            false,
        );
        assert_eq!(reduced.len(), 4);
        assert_eq!(reduced[0], down);
        assert_eq!(reduced[3], up);
        let total = reduced.iter().fold([0, 0], |mut sum, e| {
            if let Event::MouseMotion { xrel, yrel, .. } = e {
                sum[0] += xrel;
                sum[1] += yrel;
            }
            sum
        });
        assert_eq!(total, [1000, 2000]);
        assert!(matches!(
            reduced[2],
            Event::MouseMotion {
                x: 1000,
                y: 2000,
                ..
            }
        ));
    }

    #[test]
    fn pinch_discards_a_queued_tap_but_preserves_zoom() {
        let down = Event::MouseButtonDown {
            timestamp: 0,
            window_id: 1,
            which: 0,
            mouse_btn: MouseButton::Left,
            clicks: 1,
            x: 50,
            y: 50,
        };
        let up = Event::MouseButtonUp {
            timestamp: 1,
            window_id: 1,
            which: 0,
            mouse_btn: MouseButton::Left,
            clicks: 1,
            x: 50,
            y: 50,
        };
        let wheel = Event::MouseWheel {
            timestamp: 1,
            window_id: 1,
            which: 0,
            x: 0,
            y: 1,
            direction: sdl2::mouse::MouseWheelDirection::Normal,
            precise_x: 0.,
            precise_y: 1.,
            mouse_x: 50,
            mouse_y: 50,
        };
        let events = coalesce([down, wheel.clone(), up].into_iter(), true);
        assert_eq!(events, vec![wheel]);
    }
}
