//! Executable icon extraction.
//!
//! Turns an exe path into a small PNG data URL, so the usage breakdown can show
//! each app's real icon next to its name. Shell APIs only - no elevation, no
//! bundled asset pack, nothing to keep in sync with the user's installed apps.

/// Largest icon we bother with: the rows render at ~14 CSS px, so a 32 px
/// source is already retina and anything bigger is bytes over IPC for nothing.
const MAX_EDGE: u32 = 64;

#[cfg(windows)]
pub fn data_url(exe_path: &str) -> Option<String> {
    let (pixels, width, height) = unsafe { extract_rgba(exe_path) }?;
    encode(&pixels, width, height)
}

#[cfg(not(windows))]
pub fn data_url(_exe_path: &str) -> Option<String> {
    // ponytail: NSWorkspace.icon(forFile:) on macOS, .desktop lookup on Linux.
    // Neither platform has the system-proxy routing that fills this column yet.
    None
}

fn encode(pixels: &[u8], width: u32, height: u32) -> Option<String> {
    use base64::Engine;

    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(pixels))
        .ok()?;

    Some(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(&png)
    ))
}

/// SAFETY: every handle obtained here is released on all paths; the GDI calls
/// are given a header they fully describe and a buffer sized from it.
#[cfg(windows)]
unsafe fn extract_rgba(exe_path: &str) -> Option<(Vec<u8>, u32, u32)> {
    use windows_sys::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows_sys::Win32::UI::Shell::ExtractIconExW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, ICONINFO};

    let wide: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut icon = std::ptr::null_mut();
    if ExtractIconExW(wide.as_ptr(), 0, &mut icon, std::ptr::null_mut(), 1) == 0 || icon.is_null() {
        return None;
    }

    let mut info: ICONINFO = std::mem::zeroed();
    let got_info = GetIconInfo(icon, &mut info) != 0;
    DestroyIcon(icon);
    if !got_info {
        return None;
    }
    // The mask is only needed for pre-alpha icons, which are handled below.
    if !info.hbmMask.is_null() {
        DeleteObject(info.hbmMask.cast());
    }
    if info.hbmColor.is_null() {
        return None;
    }

    let mut bitmap: BITMAP = std::mem::zeroed();
    let described = GetObjectW(
        info.hbmColor.cast(),
        std::mem::size_of::<BITMAP>() as i32,
        std::ptr::addr_of_mut!(bitmap).cast(),
    ) != 0;

    let width = bitmap.bmWidth.clamp(0, MAX_EDGE as i32) as u32;
    let height = bitmap.bmHeight.clamp(0, MAX_EDGE as i32) as u32;
    if !described || width == 0 || height == 0 {
        DeleteObject(info.hbmColor.cast());
        return None;
    }

    let mut header: BITMAPINFO = std::mem::zeroed();
    header.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width as i32,
        // Negative: rows top-down, which is the order PNG wants.
        biHeight: -(height as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..header.bmiHeader
    };

    let mut buffer = vec![0u8; (width * height * 4) as usize];
    let dc = GetDC(std::ptr::null_mut());
    let rows = GetDIBits(
        dc,
        info.hbmColor,
        0,
        height,
        buffer.as_mut_ptr().cast(),
        &mut header,
        DIB_RGB_COLORS,
    );
    ReleaseDC(std::ptr::null_mut(), dc);
    DeleteObject(info.hbmColor.cast());

    if rows == 0 {
        return None;
    }

    // GDI hands back BGRA; PNG wants RGBA.
    // ponytail: a pre-Vista icon keeps its transparency in the separate mask
    // bitmap and leaves alpha at zero throughout. Rather than composite the
    // mask, draw those opaque - they are rare, and a square icon beats an
    // invisible one.
    let no_alpha = buffer.chunks_exact(4).all(|pixel| pixel[3] == 0);
    for pixel in buffer.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        if no_alpha {
            pixel[3] = 255;
        }
    }

    Some((buffer, width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixels_encode_to_a_png_data_url() {
        let pixels = vec![255u8; 2 * 2 * 4];
        let url = encode(&pixels, 2, 2).expect("2x2 RGBA should encode");
        assert!(url.starts_with("data:image/png;base64,"));
    }

    #[test]
    fn a_path_with_no_icon_yields_none() {
        assert!(data_url("Z:\\definitely\\not\\here.exe").is_none());
    }

    /// Reads a real icon out of a real binary, so the GDI plumbing is exercised
    /// rather than merely compiled.
    #[cfg(windows)]
    #[test]
    fn extracts_an_icon_from_a_system_binary() {
        let url = data_url("C:\\Windows\\explorer.exe").expect("explorer.exe has an icon");
        assert!(url.len() > 200, "suspiciously small icon payload");
    }
}
