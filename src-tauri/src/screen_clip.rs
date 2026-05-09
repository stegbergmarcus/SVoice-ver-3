#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone)]
pub struct CapturedImage {
    pub media_type: String,
    pub data_base64: String,
    pub text_data_base64: String,
    pub data_url: String,
}

#[derive(Debug, Clone, Copy)]
pub struct DragRect {
    pub start_x: f64,
    pub start_y: f64,
    pub end_x: f64,
    pub end_y: f64,
    pub scale_factor: f64,
    pub origin_x: i32,
    pub origin_y: i32,
}

pub fn normalize_drag_rect(drag: DragRect) -> Option<PhysicalRect> {
    let left = drag.start_x.min(drag.end_x);
    let top = drag.start_y.min(drag.end_y);
    let right = drag.start_x.max(drag.end_x);
    let bottom = drag.start_y.max(drag.end_y);

    let width = ((right - left) * drag.scale_factor).round() as u32;
    let height = ((bottom - top) * drag.scale_factor).round() as u32;
    if width < 8 || height < 8 {
        return None;
    }

    Some(PhysicalRect {
        x: drag.origin_x + (left * drag.scale_factor).round() as i32,
        y: drag.origin_y + (top * drag.scale_factor).round() as i32,
        width,
        height,
    })
}

pub fn fit_within_long_edge(width: u32, height: u32, max_long_edge: u32) -> (u32, u32) {
    let long = width.max(height);
    if long <= max_long_edge || max_long_edge == 0 {
        return (width, height);
    }
    let scale = max_long_edge as f64 / long as f64;
    (
        ((width as f64) * scale).round().max(1.0) as u32,
        ((height as f64) * scale).round().max(1.0) as u32,
    )
}

pub const MAX_LONG_EDGE: u32 = 1568;

fn encode_png_base64(img: &image::RgbaImage) -> anyhow::Result<String> {
    use image::codecs::png::PngEncoder;
    use image::{ColorType, ImageEncoder};

    let mut png = Vec::new();
    PngEncoder::new(&mut png).write_image(
        img.as_raw(),
        img.width(),
        img.height(),
        ColorType::Rgba8.into(),
    )?;

    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD.encode(&png))
}

pub fn enhance_for_text(img: &image::RgbaImage) -> image::RgbaImage {
    let mut out = image::RgbaImage::new(img.width(), img.height());
    for (x, y, px) in img.enumerate_pixels() {
        let r = px[0] as u32;
        let g = px[1] as u32;
        let b = px[2] as u32;
        let luma = ((77 * r + 150 * g + 29 * b) >> 8) as i16;
        let contrasted = ((luma - 128) * 2 + 128).clamp(0, 255) as u8;
        out.put_pixel(x, y, image::Rgba([contrasted, contrasted, contrasted, 255]));
    }
    out
}

#[cfg(windows)]
pub fn monitor_under_cursor() -> anyhow::Result<MonitorRect> {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    unsafe {
        let mut pt = POINT::default();
        GetCursorPos(&mut pt)?;
        let monitor = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            anyhow::bail!("GetMonitorInfoW misslyckades");
        }
        let RECT {
            left,
            top,
            right,
            bottom,
        } = info.rcMonitor;
        Ok(MonitorRect {
            x: left,
            y: top,
            width: (right - left).max(1) as u32,
            height: (bottom - top).max(1) as u32,
        })
    }
}

#[cfg(not(windows))]
pub fn monitor_under_cursor() -> anyhow::Result<MonitorRect> {
    anyhow::bail!("skärmklipp stöds bara på Windows")
}

#[cfg(windows)]
pub fn capture_region(rect: PhysicalRect) -> anyhow::Result<CapturedImage> {
    use image::imageops::FilterType;
    use image::RgbaImage;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
        GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT,
        DIB_RGB_COLORS, HGDIOBJ, SRCCOPY,
    };

    if rect.width == 0 || rect.height == 0 {
        anyhow::bail!("tom skärmklippsyta");
    }

    unsafe {
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            anyhow::bail!("GetDC misslyckades");
        }
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.is_invalid() {
            let _ = ReleaseDC(None, screen_dc);
            anyhow::bail!("CreateCompatibleDC misslyckades");
        }
        let bitmap = CreateCompatibleBitmap(screen_dc, rect.width as i32, rect.height as i32);
        if bitmap.is_invalid() {
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            anyhow::bail!("CreateCompatibleBitmap misslyckades");
        }
        let old = SelectObject(mem_dc, HGDIOBJ(bitmap.0));
        BitBlt(
            mem_dc,
            0,
            0,
            rect.width as i32,
            rect.height as i32,
            Some(screen_dc),
            rect.x,
            rect.y,
            SRCCOPY | CAPTUREBLT,
        )?;

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: rect.width as i32,
                biHeight: -(rect.height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut pixels = vec![0u8; rect.width as usize * rect.height as usize * 4];
        let rows = GetDIBits(
            mem_dc,
            bitmap,
            0,
            rect.height,
            Some(pixels.as_mut_ptr() as *mut core::ffi::c_void),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        let _ = SelectObject(mem_dc, old);
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);

        if rows == 0 {
            anyhow::bail!("GetDIBits misslyckades");
        }

        for px in pixels.chunks_exact_mut(4) {
            px.swap(0, 2);
            px[3] = 255;
        }
        let img = RgbaImage::from_raw(rect.width, rect.height, pixels)
            .ok_or_else(|| anyhow::anyhow!("kunde inte skapa bildbuffer"))?;
        let (out_w, out_h) = fit_within_long_edge(rect.width, rect.height, MAX_LONG_EDGE);
        let img = if (out_w, out_h) == (rect.width, rect.height) {
            img
        } else {
            image::imageops::resize(&img, out_w, out_h, FilterType::Lanczos3)
        };

        let text_img = enhance_for_text(&img);
        let data_base64 = encode_png_base64(&img)?;
        let text_data_base64 = encode_png_base64(&text_img)?;
        Ok(CapturedImage {
            media_type: "image/png".into(),
            data_url: format!("data:image/png;base64,{data_base64}"),
            data_base64,
            text_data_base64,
        })
    }
}

#[cfg(not(windows))]
pub fn capture_region(_rect: PhysicalRect) -> anyhow::Result<CapturedImage> {
    anyhow::bail!("skärmklipp stöds bara på Windows")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_drag_rect_handles_reverse_drag_and_scale() {
        let rect = normalize_drag_rect(DragRect {
            start_x: 220.0,
            start_y: 140.0,
            end_x: 20.0,
            end_y: 40.0,
            scale_factor: 1.5,
            origin_x: -1920,
            origin_y: 0,
        })
        .unwrap();

        assert_eq!(
            rect,
            PhysicalRect {
                x: -1890,
                y: 60,
                width: 300,
                height: 150,
            }
        );
    }

    #[test]
    fn normalize_drag_rect_rejects_tiny_selection() {
        let rect = normalize_drag_rect(DragRect {
            start_x: 10.0,
            start_y: 10.0,
            end_x: 12.0,
            end_y: 12.0,
            scale_factor: 1.0,
            origin_x: 0,
            origin_y: 0,
        });

        assert!(rect.is_none());
    }

    #[test]
    fn fit_within_long_edge_preserves_aspect_ratio() {
        assert_eq!(fit_within_long_edge(3000, 1500, 1500), (1500, 750));
        assert_eq!(fit_within_long_edge(800, 1200, 1568), (800, 1200));
    }

    #[test]
    fn enhance_for_text_preserves_dimensions() {
        let img = image::RgbaImage::from_pixel(23, 11, image::Rgba([80, 120, 160, 255]));

        let enhanced = enhance_for_text(&img);

        assert_eq!(enhanced.dimensions(), (23, 11));
    }

    #[test]
    fn enhance_for_text_outputs_grayscale_pixels() {
        let img =
            image::RgbaImage::from_vec(2, 1, vec![10, 20, 30, 255, 220, 200, 180, 255]).unwrap();

        let enhanced = enhance_for_text(&img);

        for px in enhanced.pixels() {
            assert_eq!(px[0], px[1]);
            assert_eq!(px[1], px[2]);
            assert_eq!(px[3], 255);
        }
    }
}
