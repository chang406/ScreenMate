use serde::Serialize;
use std::time::Duration;
use tauri::{Emitter, WebviewWindow};

const POLL_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
struct CursorPayload {
    x: f64,
    y: f64,
    inside: bool,
}

pub async fn run(window: WebviewWindow) {
    let mut last: Option<CursorPayload> = None;

    loop {
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
