//! OS cursor polling (16ms) for per-pixel click-through, plus app-driven window dragging.
//!
//! Dragging: the overlay used to call the OS "move window" loop (`startDragging`). Windows keeps
//! a window's top edge (normally its title bar) from going above the screen, and our window's top
//! is empty transparent space above the character - so the character could not be moved up past
//! a certain height. Instead, the overlay calls `begin_drag` / `end_drag` and this loop moves the
//! window itself by the cursor delta; positions set by the app are not clamped by the OS.
//! The point the user grabbed always stays under the cursor (which cannot leave the screen), so
//! the character can be hidden almost entirely but never lost.

use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{Emitter, PhysicalPosition, WebviewWindow};

const POLL_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
struct CursorPayload {
    x: f64,
    y: f64,
    inside: bool,
}

/// Window position and cursor position (both physical, global) when the drag started.
#[derive(Clone, Copy, Debug, PartialEq)]
struct DragState {
    window_start: (i32, i32),
    cursor_start: (f64, f64),
}

static DRAG: Mutex<Option<DragState>> = Mutex::new(None);

/// Where the window goes for the current cursor position: start + cursor delta.
fn drag_target(state: DragState, cursor: (f64, f64)) -> (i32, i32) {
    let dx = (cursor.0 - state.cursor_start.0).round() as i32;
    let dy = (cursor.1 - state.cursor_start.1).round() as i32;
    (state.window_start.0 + dx, state.window_start.1 + dy)
}

/// Starts moving the window with the cursor (called by the overlay once a press turns into a drag).
#[tauri::command]
pub fn begin_drag(window: WebviewWindow) -> Result<(), String> {
    let cursor = window.cursor_position().map_err(|e| e.to_string())?;
    let pos = window.outer_position().map_err(|e| e.to_string())?;
    if let Ok(mut drag) = DRAG.lock() {
        *drag = Some(DragState {
            window_start: (pos.x, pos.y),
            cursor_start: (cursor.x, cursor.y),
        });
    }
    Ok(())
}

/// Stops the drag (mouse released, window blurred, or missed release detected by the overlay).
#[tauri::command]
pub fn end_drag() {
    if let Ok(mut drag) = DRAG.lock() {
        *drag = None;
    }
}

pub async fn run(window: WebviewWindow) {
    let mut last: Option<CursorPayload> = None;
    let mut last_drag_pos: Option<(i32, i32)> = None;

    loop {
        // Move the window first so the cursor payload below is computed against the new origin.
        let drag = DRAG.lock().ok().and_then(|d| *d);
        match (drag, window.cursor_position()) {
            (Some(state), Ok(cursor)) => {
                let target = drag_target(state, (cursor.x, cursor.y));
                if last_drag_pos != Some(target) {
                    last_drag_pos = Some(target);
                    let _ = window.set_position(PhysicalPosition::new(target.0, target.1));
                }
            }
            _ => last_drag_pos = None,
        }

        if let (Ok(cursor), Ok(origin), Ok(size)) = (
            window.cursor_position(),
            window.outer_position(),
            window.outer_size(),
        ) {
            let local_x = cursor.x - origin.x as f64;
            let local_y = cursor.y - origin.y as f64;
            let inside = local_x >= 0.0
                && local_y >= 0.0
                && local_x < size.width as f64
                && local_y < size.height as f64;

            let payload = CursorPayload {
                x: local_x,
                y: local_y,
                inside,
            };

            if last != Some(payload) {
                last = Some(payload);
                let _ = window.emit("cursor-position", payload);
            }
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> DragState {
        DragState {
            window_start: (1000, 300),
            cursor_start: (1320.0, 1000.0),
        }
    }

    #[test]
    fn window_follows_cursor_delta() {
        assert_eq!(drag_target(state(), (1320.0, 1000.0)), (1000, 300));
        assert_eq!(drag_target(state(), (1400.5, 950.0)), (1081, 250));
    }

    #[test]
    fn window_top_can_go_above_the_screen() {
        // Grabbed the feet (cursor y=1000) and moved the cursor to the screen top (y=0):
        // the window top goes far above the screen - the OS drag loop would have stopped at y=0.
        assert_eq!(drag_target(state(), (1320.0, 0.0)), (1000, -700));
    }

    #[test]
    fn window_can_go_further_than_half_off_the_left_edge() {
        // Grabbed near the character's right edge (window x + 600) and moved to the left edge.
        let s = DragState {
            window_start: (1000, 300),
            cursor_start: (1600.0, 900.0),
        };
        assert_eq!(drag_target(s, (0.0, 900.0)).0, -600);
    }
}
