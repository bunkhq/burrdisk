//! The icon of the app that opens a kind of file, shown beside files of that kind in
//! place of the generic type glyph. Each extension is looked up once, off the UI
//! thread; until its icon arrives, or when no app claims the extension, callers fall
//! back to the vector glyph.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use gpui::{App, RenderImage};

use crate::filetype::{self, Category};
use crate::scan::NodeKind;

/// Pixel size icons are fetched at: sharp at the sizes they're drawn (13–22pt) on
/// 2× screens and at Windows' 1.2× scale.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
const SIZE: u32 = 64;

enum Slot {
    Loading,
    Ready(Option<Arc<RenderImage>>),
}

thread_local! {
    static CACHE: RefCell<HashMap<String, Slot>> = RefCell::new(HashMap::new());
}

/// The icon of the app that opens `name`, if one is known yet. A miss starts the
/// lookup and repaints the windows once it lands.
pub fn for_item(name: &str, kind: NodeKind, cx: &mut App) -> Option<Arc<RenderImage>> {
    let ext = extension_for(name, kind)?;
    for_extension(&ext, cx)
}

/// The same, for a bare extension like "mp4".
pub fn for_extension(ext: &str, cx: &mut App) -> Option<Arc<RenderImage>> {
    let hit = CACHE.with(|c| match c.borrow().get(ext) {
        Some(Slot::Ready(icon)) => Some(icon.clone()),
        Some(Slot::Loading) => Some(None),
        None => None,
    });
    if let Some(icon) = hit {
        return icon;
    }
    CACHE.with(|c| c.borrow_mut().insert(ext.to_string(), Slot::Loading));
    let ext = ext.to_string();
    let job = {
        let ext = ext.clone();
        cx.background_executor().spawn(async move { load(&ext) })
    };
    cx.spawn(async move |cx| {
        let icon = job.await.map(Arc::new);
        let landed = icon.is_some();
        cx.update(|cx| {
            CACHE.with(|c| c.borrow_mut().insert(ext, Slot::Ready(icon)));
            if landed {
                cx.refresh_windows();
            }
        })
        .ok();
    })
    .detach();
    None
}

/// The extension to look up for an item, or `None` when the generic glyph says more:
/// folders, apps and other programs (which open themselves), and Hidden space.
pub fn extension_for(name: &str, kind: NodeKind) -> Option<String> {
    match filetype::category(name, kind) {
        Category::Folder | Category::Program | Category::Hidden => None,
        _ => filetype::extension(name),
    }
}

/// Straight-alpha BGRA pixels, as GPUI draws them.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(dead_code))]
fn image(width: u32, height: u32, bgra: Vec<u8>) -> Option<RenderImage> {
    let buffer = image::RgbaImage::from_raw(width, height, bgra)?;
    Some(RenderImage::new(vec![image::Frame::new(buffer)]))
}

#[cfg(windows)]
fn load(ext: &str) -> Option<RenderImage> {
    use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
    // Shell lookups want COM; balance whatever this thread didn't already have.
    let initialized = unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) } >= 0;
    let icon = windows::app_icon(ext);
    if initialized {
        unsafe { CoUninitialize() };
    }
    icon
}

#[cfg(windows)]
mod windows {
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};

    use gpui::RenderImage;
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW,
        HBITMAP, ReleaseDC,
    };
    use windows_sys::Win32::UI::Shell::{
        ASSOCF_INIT_IGNOREUNKNOWN, ASSOCF_NOTRUNCATE, ASSOCSTR, ASSOCSTR_APPICONREFERENCE, ASSOCSTR_EXECUTABLE,
        AssocQueryStringW, SHDefExtractIconW, SHLoadIndirectString,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

    use super::SIZE;

    /// Programs that stand in for "no app", or host one, rather than open the file.
    const HOSTS: &[&str] = &["openwith.exe", "rundll32.exe", "dllhost.exe", "explorer.exe", "cmd.exe"];

    fn wide(s: impl AsRef<std::ffi::OsStr>) -> Vec<u16> {
        s.as_ref().encode_wide().chain(Some(0)).collect()
    }

    fn assoc(ext: &str, what: ASSOCSTR) -> Option<String> {
        let ext = wide(format!(".{ext}"));
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let flags = ASSOCF_INIT_IGNOREUNKNOWN | ASSOCF_NOTRUNCATE;
        let hr = unsafe {
            AssocQueryStringW(flags, what, ext.as_ptr(), std::ptr::null(), buf.as_mut_ptr(), &mut len)
        };
        if hr < 0 || len <= 1 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize - 1]))
    }

    pub fn app_icon(ext: &str) -> Option<RenderImage> {
        if let Some(exe) = assoc(ext, ASSOCSTR_EXECUTABLE).map(PathBuf::from) {
            let file = exe.file_name()?.to_string_lossy().to_ascii_lowercase();
            if HOSTS.contains(&file.as_str()) {
                return None;
            }
            if let Some(icon) = exe_icon(&exe) {
                return Some(icon);
            }
        }
        // Store apps have no program file to read; they name a logo resource instead.
        store_logo(ext)
    }

    fn exe_icon(exe: &Path) -> Option<RenderImage> {
        let path = wide(exe);
        let mut icon: HICON = std::ptr::null_mut();
        let hr = unsafe { SHDefExtractIconW(path.as_ptr(), 0, 0, &mut icon, std::ptr::null_mut(), SIZE) };
        if hr != 0 || icon.is_null() {
            return None;
        }
        let pixels = icon_pixels(icon);
        unsafe { DestroyIcon(icon) };
        let (w, h, bgra) = pixels?;
        super::image(w, h, bgra)
    }

    /// An icon's pixels as straight-alpha BGRA. Old icons without an alpha channel
    /// take their transparency from the mask.
    fn icon_pixels(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
        let mut info: ICONINFO = unsafe { std::mem::zeroed() };
        if unsafe { GetIconInfo(icon, &mut info) } == 0 {
            return None;
        }
        let read = |bitmap: HBITMAP| -> Option<(u32, u32, Vec<u8>)> {
            if bitmap.is_null() {
                return None;
            }
            let mut bm: BITMAP = unsafe { std::mem::zeroed() };
            let got = unsafe { GetObjectW(bitmap, size_of::<BITMAP>() as i32, (&raw mut bm).cast()) };
            if got == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
                return None;
            }
            let (w, h) = (bm.bmWidth as u32, bm.bmHeight as u32);
            let mut bmi: BITMAPINFO = unsafe { std::mem::zeroed() };
            bmi.bmiHeader = BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w as i32,
                // Negative height asks for rows top to bottom.
                biHeight: -(h as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..unsafe { std::mem::zeroed() }
            };
            let mut buf = vec![0u8; (w * h * 4) as usize];
            let dc = unsafe { GetDC(std::ptr::null_mut()) };
            let lines = unsafe { GetDIBits(dc, bitmap, 0, h, buf.as_mut_ptr().cast(), &mut bmi, DIB_RGB_COLORS) };
            unsafe { ReleaseDC(std::ptr::null_mut(), dc) };
            (lines > 0).then_some((w, h, buf))
        };
        let color = read(info.hbmColor);
        let mask = read(info.hbmMask);
        unsafe {
            if !info.hbmColor.is_null() {
                DeleteObject(info.hbmColor);
            }
            if !info.hbmMask.is_null() {
                DeleteObject(info.hbmMask);
            }
        }
        let (w, h, mut bgra) = color?;
        if bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
            let (_, _, mask) = mask?;
            for (pixel, m) in bgra.as_chunks_mut::<4>().0.iter_mut().zip(mask.as_chunks::<4>().0) {
                pixel[3] = if m[0] == 0 { 255 } else { 0 };
            }
        }
        Some((w, h, bgra))
    }

    /// A Store app's logo: its icon reference resolves to a PNG inside the package.
    fn store_logo(ext: &str) -> Option<RenderImage> {
        let reference = assoc(ext, ASSOCSTR_APPICONREFERENCE)?;
        if !reference.starts_with('@') {
            return None;
        }
        let source = wide(&reference);
        let mut buf = vec![0u16; 1024];
        let hr = unsafe { SHLoadIndirectString(source.as_ptr(), buf.as_mut_ptr(), buf.len() as u32, std::ptr::null()) };
        if hr < 0 {
            return None;
        }
        let end = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
        let path = PathBuf::from(String::from_utf16_lossy(&buf[..end]));
        let mut rgba = image::open(&path).ok()?.into_rgba8();
        // Logos come larger than needed; scale down so a list of them stays light.
        if rgba.width() > SIZE * 2 {
            rgba = image::imageops::resize(&rgba, SIZE, SIZE, image::imageops::FilterType::Triangle);
        }
        let (w, h) = rgba.dimensions();
        let mut bgra = rgba.into_raw();
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        super::image(w, h, bgra)
    }
}

#[cfg(target_os = "macos")]
fn load(ext: &str) -> Option<RenderImage> {
    let (w, h, bgra) = macos::app_icon(ext)?;
    image(w, h, bgra)
}

#[cfg(target_os = "macos")]
mod macos {
    use objc::runtime::{Class, NO, Object, YES};
    use objc::{Encode, Encoding, msg_send, sel, sel_impl};

    use super::SIZE;

    #[link(name = "AppKit", kind = "framework")]
    #[link(name = "UniformTypeIdentifiers", kind = "framework")]
    unsafe extern "C" {}

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    }

    unsafe impl Encode for Rect {
        fn encode() -> Encoding {
            unsafe { Encoding::from_str("{CGRect={CGPoint=dd}{CGSize=dd}}") }
        }
    }

    /// NSCompositingOperationCopy
    const COPY: usize = 1;

    pub fn app_icon(ext: &str) -> Option<(u32, u32, Vec<u8>)> {
        unsafe {
            let pool: *mut Object = msg_send![Class::get("NSAutoreleasePool")?, new];
            let icon = draw(ext);
            let _: () = msg_send![pool, drain];
            icon
        }
    }

    unsafe fn draw(ext: &str) -> Option<(u32, u32, Vec<u8>)> {
        unsafe {
            let text = std::ffi::CString::new(ext).ok()?;
            let ns_ext: *mut Object = msg_send![Class::get("NSString")?, stringWithUTF8String: text.as_ptr()];
            let ut_type: *mut Object = msg_send![Class::get("UTType")?, typeWithFilenameExtension: ns_ext];
            if ut_type.is_null() {
                return None;
            }
            let workspace: *mut Object = msg_send![Class::get("NSWorkspace")?, sharedWorkspace];
            let app: *mut Object = msg_send![workspace, URLForApplicationToOpenContentType: ut_type];
            if app.is_null() {
                return None;
            }
            let path: *mut Object = msg_send![app, path];
            let icon: *mut Object = msg_send![workspace, iconForFile: path];
            if icon.is_null() {
                return None;
            }

            let side = SIZE as isize;
            let rep: *mut Object = msg_send![Class::get("NSBitmapImageRep")?, alloc];
            let color_space: *mut Object =
                msg_send![Class::get("NSString")?, stringWithUTF8String: c"NSCalibratedRGBColorSpace".as_ptr()];
            let rep: *mut Object = msg_send![rep,
                initWithBitmapDataPlanes: std::ptr::null_mut::<*mut u8>()
                pixelsWide: side
                pixelsHigh: side
                bitsPerSample: 8isize
                samplesPerPixel: 4isize
                hasAlpha: YES
                isPlanar: NO
                colorSpaceName: color_space
                bytesPerRow: side * 4
                bitsPerPixel: 32isize];
            if rep.is_null() {
                return None;
            }
            let context: *mut Object =
                msg_send![Class::get("NSGraphicsContext")?, graphicsContextWithBitmapImageRep: rep];
            let pixels = if context.is_null() {
                None
            } else {
                let graphics = Class::get("NSGraphicsContext")?;
                let _: () = msg_send![graphics, saveGraphicsState];
                let _: () = msg_send![graphics, setCurrentContext: context];
                let all = Rect { x: 0., y: 0., w: SIZE as f64, h: SIZE as f64 };
                let none = Rect { x: 0., y: 0., w: 0., h: 0. };
                let _: () = msg_send![icon, drawInRect: all fromRect: none operation: COPY fraction: 1.0f64];
                let _: () = msg_send![context, flushGraphics];
                let _: () = msg_send![graphics, restoreGraphicsState];
                let data: *const u8 = msg_send![rep, bitmapData];
                (!data.is_null()).then(|| std::slice::from_raw_parts(data, (SIZE * SIZE * 4) as usize).to_vec())
            };
            let _: () = msg_send![rep, release];
            let mut bgra = pixels?;
            // The bitmap is premultiplied RGBA; GPUI wants straight BGRA.
            for pixel in bgra.as_chunks_mut::<4>().0 {
                let a = pixel[3] as u32;
                if a > 0 && a < 255 {
                    for c in &mut pixel[..3] {
                        *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
                    }
                }
                pixel.swap(0, 2);
            }
            Some((SIZE, SIZE, bgra))
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn load(_ext: &str) -> Option<RenderImage> {
    None
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    #[ignore = "depends on the apps installed on this machine"]
    fn finds_app_icons() {
        for ext in ["pdf", "txt", "jpg", "png", "mp4", "mp3", "zip", "docx", "json", "rs", "html", "nothing-opens-this"] {
            let icon = super::load(ext);
            println!("{ext}: {:?}", icon.map(|i| i.size(0)));
        }
    }
}
