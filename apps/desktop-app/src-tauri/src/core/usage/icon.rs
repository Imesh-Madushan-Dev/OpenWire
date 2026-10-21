//! Executable icon extraction.
//!
//! Turns an exe path into a small PNG data URL, so the usage breakdown can show
//! each app's real icon next to its name. Shell APIs only - no elevation, no
//! bundled asset pack, nothing to keep in sync with the user's installed apps.
//!
//! Three sources, tried in the order Explorer would show them: the icon built
//! into the executable; for a packaged (Store/MSIX) app, which usually ships
//! none, the logo its manifest names; and otherwise Windows' own generic
//! application icon. Plenty of executables carry no icon at all - svchost,
//! curl, most command-line tools - and the generic one is what Explorer and
//! GlassWire show for them too.

/// Largest icon we bother with: the rows render at ~14 CSS px, so a 32 px
/// source is already retina and anything bigger is bytes over IPC for nothing.
const MAX_EDGE: u32 = 64;

#[cfg(windows)]
pub fn data_url(exe_path: &str) -> Option<String> {
    if let Some((pixels, width, height)) = unsafe { embedded_rgba(exe_path) } {
        return encode(&pixels, width, height);
    }
    package_logo(std::path::Path::new(exe_path)).or_else(generic)
}

#[cfg(not(windows))]
pub fn data_url(_exe_path: &str) -> Option<String> {
    // ponytail: NSWorkspace.icon(forFile:) on macOS, .desktop lookup on Linux.
    // Neither platform has the system-proxy routing that fills this column yet.
    None
}

fn encode(pixels: &[u8], width: u32, height: u32) -> Option<String> {
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(&mut png, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(pixels))
        .ok()?;
    Some(png_data_url(&png))
}

fn png_data_url(png: &[u8]) -> String {
    use base64::Engine;

    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(png)
    )
}

/// Windows' icon for an executable that has none of its own. The same for
/// every such app, so drawn once.
#[cfg(windows)]
fn generic() -> Option<String> {
    use std::sync::OnceLock;

    static GENERIC: OnceLock<Option<String>> = OnceLock::new();
    GENERIC
        .get_or_init(|| {
            let (pixels, width, height) = unsafe { generic_rgba() }?;
            encode(&pixels, width, height)
        })
        .clone()
}

// ---------------------------------------------------------------------------
// Packaged apps
// ---------------------------------------------------------------------------

/// How far above an executable its package's manifest may sit. Packages keep
/// helpers in subfolders (`Dashboard\Widgets.exe`), never deep ones.
#[cfg(windows)]
const PACKAGE_DEPTH: usize = 3;

/// A logo is a few KB; anything far past that is not one worth shipping.
#[cfg(windows)]
const MAX_LOGO_BYTES: u64 = 256 * 1024;

/// The logo a packaged app's manifest gives the application `exe` runs as.
#[cfg(windows)]
fn package_logo(exe: &std::path::Path) -> Option<String> {
    use std::path::{Component, Path};

    let (root, manifest) = exe
        .ancestors()
        .skip(1)
        .take(PACKAGE_DEPTH)
        .find_map(|dir| {
            std::fs::read_to_string(dir.join("AppxManifest.xml"))
                .ok()
                .map(|text| (dir, text))
        })?;

    let logo = Path::new(manifest_logo(&manifest, exe.file_name()?.to_str()?)?);
    // The manifest is a file on disk like any other; never let it point the
    // reader outside its own package.
    if !logo
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return None;
    }

    let file = logo_variant(&root.join(logo))?;
    if std::fs::metadata(&file).ok()?.len() > MAX_LOGO_BYTES {
        return None;
    }
    let png = std::fs::read(&file).ok()?;
    png.starts_with(b"\x89PNG\r\n\x1a\n")
        .then(|| png_data_url(&png))
}

/// The app-list logo a manifest gives the application `exe_name` runs as, or
/// failing that the first one it gives any application - helper executables
/// are rarely an application of their own.
#[cfg(windows)]
fn manifest_logo<'a>(manifest: &'a str, exe_name: &str) -> Option<&'a str> {
    const LOGO: &str = "Square44x44Logo";

    manifest
        .split("<Application ")
        .skip(1)
        .find(|app| {
            attribute(app, "Executable").is_some_and(|exe| {
                exe.rsplit(['\\', '/'])
                    .next()
                    .is_some_and(|name| name.eq_ignore_ascii_case(exe_name))
            })
        })
        .and_then(|app| attribute(app, LOGO))
        .or_else(|| attribute(manifest, LOGO))
}

/// The first value of attribute `name` in `xml`. Matched as a whole name, so
/// asking for `Logo` never finds `Square44x44Logo`.
#[cfg(windows)]
fn attribute<'a>(xml: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let (at, _) = xml
        .match_indices(&needle)
        .find(|(at, _)| xml[..*at].ends_with(char::is_whitespace))?;
    xml[at + needle.len()..].split('"').next()
}

/// The file Windows would draw for a logo reference. The manifest names
/// `Assets\AppList.png`, but what ships is a set of qualified variants such
/// as `AppList.targetsize-32_altform-unplated.png`.
#[cfg(windows)]
fn logo_variant(reference: &std::path::Path) -> Option<std::path::PathBuf> {
    let stem = reference.file_stem()?.to_str()?.to_ascii_lowercase();
    let extension = reference.extension()?.to_str()?.to_ascii_lowercase();

    std::fs::read_dir(reference.parent()?)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?.to_ascii_lowercase();
            Some((variant_rank(&name, &stem, &extension)?, entry.path()))
        })
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, path)| path)
}

/// Orders a logo's variants best first: unplated (drawn for a dark taskbar,
/// no background tile), then the smallest at or above 32 px. `None` for files
/// that are not a variant of this logo, or are meant for a light or
/// high-contrast taskbar.
#[cfg(windows)]
fn variant_rank(name: &str, stem: &str, extension: &str) -> Option<(bool, bool, u32)> {
    let qualifiers = name
        .strip_prefix(stem)?
        .strip_suffix(extension)?
        .strip_suffix('.')?;
    let qualifiers = match qualifiers {
        "" => "",
        dotted => dotted.strip_prefix('.')?,
    };
    if ["lightunplated", "contrast-", "theme-light"]
        .iter()
        .any(|unwanted| qualifiers.contains(unwanted))
    {
        return None;
    }

    let unplated = qualifiers.contains("altform-unplated");
    // `scale-N` is relative to the nominal 44 px.
    let edge = qualifiers
        .split('_')
        .find_map(|qualifier| {
            qualifier
                .strip_prefix("targetsize-")
                .and_then(|size| size.parse().ok())
                .or_else(|| {
                    qualifier
                        .strip_prefix("scale-")
                        .and_then(|scale| scale.parse::<u32>().ok())
                        .map(|scale| 44 * scale / 100)
                })
        })
        .unwrap_or(44);
    Some((!unplated, edge < 32, edge.abs_diff(32)))
}

// ---------------------------------------------------------------------------
// GDI
// ---------------------------------------------------------------------------

/// The executable's own first icon, if it has one.
#[cfg(windows)]
unsafe fn embedded_rgba(exe_path: &str) -> Option<(Vec<u8>, u32, u32)> {
    use windows_sys::Win32::UI::Shell::ExtractIconExW;
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;

    let wide: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut icon = std::ptr::null_mut();
    if ExtractIconExW(wide.as_ptr(), 0, &mut icon, std::ptr::null_mut(), 1) == 0 || icon.is_null() {
        return None;
    }
    let rgba = icon_rgba(icon);
    DestroyIcon(icon);
    rgba
}

#[cfg(windows)]
unsafe fn generic_rgba() -> Option<(Vec<u8>, u32, u32)> {
    use windows_sys::Win32::UI::Shell::{
        SHGetStockIconInfo, SHGSI_ICON, SHGSI_LARGEICON, SHSTOCKICONINFO, SIID_APPLICATION,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::DestroyIcon;

    let mut info: SHSTOCKICONINFO = std::mem::zeroed();
    info.cbSize = std::mem::size_of::<SHSTOCKICONINFO>() as u32;
    if SHGetStockIconInfo(SIID_APPLICATION, SHGSI_ICON | SHGSI_LARGEICON, &mut info) < 0
        || info.hIcon.is_null()
    {
        return None;
    }
    let rgba = icon_rgba(info.hIcon);
    DestroyIcon(info.hIcon);
    rgba
}

/// SAFETY: `icon` must be a live icon handle, which stays the caller's to
/// destroy. Every handle obtained here is released on all paths; the GDI calls
/// are given a header they fully describe and a buffer sized from it.
#[cfg(windows)]
unsafe fn icon_rgba(
    icon: windows_sys::Win32::UI::WindowsAndMessaging::HICON,
) -> Option<(Vec<u8>, u32, u32)> {
    use windows_sys::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetIconInfo, ICONINFO};

    let mut info: ICONINFO = std::mem::zeroed();
    if GetIconInfo(icon, &mut info) == 0 {
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

    /// Reads a real icon out of a real binary, so the GDI plumbing is exercised
    /// rather than merely compiled.
    #[cfg(windows)]
    #[test]
    fn extracts_an_icon_from_a_system_binary() {
        let url = data_url("C:\\Windows\\explorer.exe").expect("explorer.exe has an icon");
        assert!(url.len() > 200, "suspiciously small icon payload");
    }

    /// svchost.exe carries no icon of its own, nor does a file that is gone -
    /// an app updated since its path was recorded. Both still get a picture.
    #[cfg(windows)]
    #[test]
    fn an_executable_without_an_icon_gets_the_generic_one() {
        let generic = generic().expect("Windows has a generic application icon");
        assert_eq!(data_url("C:\\Windows\\System32\\svchost.exe"), Some(generic.clone()));
        assert_eq!(data_url("Z:\\definitely\\not\\here.exe"), Some(generic));
    }

    #[cfg(windows)]
    #[test]
    fn a_manifest_names_the_logo_of_the_application_an_exe_runs_as() {
        let manifest = r#"
            <Package><Properties><Logo>Assets\StoreLogo.png</Logo></Properties>
            <Applications>
              <Application Id="App" Executable="Main.exe">
                <uap:VisualElements DisplayName="Main" Square44x44Logo="Assets\Main.png" />
              </Application>
              <Application Id="Helper" Executable="Tools\helper.exe">
                <uap:VisualElements
                  Square44x44Logo="Assets\Helper.png" />
              </Application>
            </Applications></Package>"#;

        assert_eq!(manifest_logo(manifest, "HELPER.EXE"), Some(r"Assets\Helper.png"));
        assert_eq!(manifest_logo(manifest, "main.exe"), Some(r"Assets\Main.png"));
        assert_eq!(
            manifest_logo(manifest, "updater.exe"),
            Some(r"Assets\Main.png"),
            "an exe that is no application takes the package's first logo"
        );
        assert_eq!(attribute(manifest, "Logo"), None, "whole attribute names only");
    }

    #[cfg(windows)]
    #[test]
    fn the_variant_drawn_is_unplated_and_nearest_32px() {
        let mut ranked: Vec<_> = [
            "applist.png",
            "applist.scale-200.png",
            "applist.targetsize-16_altform-unplated.png",
            "applist.targetsize-32.png",
            "applist.targetsize-48_altform-unplated.png",
            "applist.targetsize-32_altform-unplated.png",
        ]
        .into_iter()
        .map(|name| (variant_rank(name, "applist", "png").expect(name), name))
        .collect();
        ranked.sort();

        let order: Vec<_> = ranked.into_iter().map(|(_, name)| name).collect();
        assert_eq!(
            order[..3],
            [
                "applist.targetsize-32_altform-unplated.png",
                "applist.targetsize-48_altform-unplated.png",
                "applist.targetsize-16_altform-unplated.png",
            ]
        );
        for skipped in [
            "applistwide.png",
            "applist.targetsize-32_altform-lightunplated.png",
            "applist.scale-200_altform-colorful_theme-light.png",
        ] {
            assert_eq!(variant_rank(skipped, "applist", "png"), None, "{skipped}");
        }
    }

    /// A packaged app's helper executable has no icon of its own, so this is
    /// the logo or nothing. Skipped where no such package is installed.
    #[cfg(windows)]
    #[test]
    fn a_packaged_app_shows_its_manifest_logo() {
        let exe = std::path::Path::new(
            r"C:\Windows\SystemApps\Microsoft.Windows.StartMenuExperienceHost_cw5n1h2txyewy\StartMenuExperienceHost.exe",
        );
        if !exe.exists() {
            return;
        }
        let url = package_logo(exe).expect("the Start menu package has a logo");
        assert!(url.starts_with("data:image/png;base64,"));
        assert_ne!(Some(url), generic());
    }
}
