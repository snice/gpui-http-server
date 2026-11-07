//! QR code rendering as plain GPUI elements.

use gpui_kit::*;
use qrcode::{Color, EcLevel, QrCode};

/// Renders `text` as a QR code on a white tile (scannable in light and dark themes).
pub fn qr_tile(text: &str, size: f32) -> AnyElement {
    let Ok(code) = QrCode::with_error_correction_level(text, EcLevel::M) else {
        return div().child("无法生成二维码").into_any_element();
    };
    let width = code.width();
    let colors = code.to_colors();
    let quiet = 2usize; // quiet zone, in modules
    let cell = (size / (width + quiet * 2) as f32).floor().max(2.0);
    let inner = cell * width as f32;
    let pad = cell * quiet as f32;

    let mut rows = div().flex().flex_col().w(px(inner)).h(px(inner));
    for y in 0..width {
        let mut row = div().flex().flex_row().h(px(cell));
        let mut x = 0;
        while x < width {
            let dark = colors[y * width + x] == Color::Dark;
            let mut end = x;
            while end < width && (colors[y * width + end] == Color::Dark) == dark {
                end += 1;
            }
            let run = div().w(px(cell * (end - x) as f32)).h(px(cell));
            row = row.child(if dark { run.bg(black()) } else { run });
            x = end;
        }
        rows = rows.child(row);
    }
    div()
        .p(px(pad))
        .rounded_xl()
        .bg(white())
        .border_1()
        .border_color(hsla(0., 0., 0., 0.08))
        .child(rows)
        .into_any_element()
}
