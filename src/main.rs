use std::env;
use std::ffi::c_void;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::ptr;

use chrono::Local;
use image::{ImageBuffer, Rgba};
use windows_sys::Win32::Foundation::{HWND, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleBitmap,
    CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits, HBITMAP, HDC,
    ReleaseDC, SRCCOPY, SelectObject,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_SNAPSHOT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, GetSystemMetrics, MSG, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, TranslateMessage, WM_HOTKEY,
};

const HOTKEY_ID: i32 = 1;

fn main() {
    if let Err(error) = run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let output_dir = screenshot_dir()?;
    fs::create_dir_all(&output_dir)?;

    if env::args().any(|argument| argument == "--capture-once") {
        let path = save_screenshot(&output_dir)?;
        println!("Saved {}", path.display());
        return Ok(());
    }

    if env::args().any(|argument| argument == "--check-hotkey") {
        register_print_screen_hotkey()?;
        unsafe {
            UnregisterHotKey(ptr::null_mut(), HOTKEY_ID);
        }
        println!("Print Screen hotkey is available.");
        return Ok(());
    }

    register_print_screen_hotkey()?;
    println!("Print Screen Capture is running.");
    println!("Press Print Screen to save a PNG screenshot.");
    println!("Output folder: {}", output_dir.display());
    println!("Press Ctrl+C to quit.");

    message_loop(&output_dir);

    unsafe {
        UnregisterHotKey(ptr::null_mut(), HOTKEY_ID);
    }

    Ok(())
}

fn screenshot_dir() -> io::Result<PathBuf> {
    let pictures = env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .map(|path| path.join("Pictures"))
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "USERPROFILE is not set"))?;

    Ok(pictures.join("PrintScreenCapture"))
}

fn register_print_screen_hotkey() -> io::Result<()> {
    let registered = unsafe {
        RegisterHotKey(
            ptr::null_mut::<c_void>() as HWND,
            HOTKEY_ID,
            MOD_NOREPEAT,
            VK_SNAPSHOT as u32,
        )
    };

    if registered == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn message_loop(output_dir: &Path) {
    let mut message = MSG::default();

    while unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) } > 0 {
        if message.message == WM_HOTKEY && message.wParam == HOTKEY_ID as WPARAM {
            match save_screenshot(output_dir) {
                Ok(path) => println!("Saved {}", path.display()),
                Err(error) => eprintln!("Capture failed: {error}"),
            }
        }

        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn save_screenshot(output_dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let pixels = capture_virtual_screen()?;
    let filename = format!("screenshot-{}.png", Local::now().format("%Y%m%d-%H%M%S"));
    let path = output_dir.join(filename);

    pixels.save(&path)?;
    Ok(path)
}

fn capture_virtual_screen() -> Result<ImageBuffer<Rgba<u8>, Vec<u8>>, Box<dyn std::error::Error>> {
    let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

    if width <= 0 || height <= 0 {
        return Err("virtual screen has no drawable area".into());
    }

    let screen_dc = DesktopDc::new()?;
    let memory_dc = MemoryDc::new(screen_dc.0)?;
    let bitmap = CompatibleBitmap::new(screen_dc.0, width, height)?;
    let selected = SelectedObject::new(memory_dc.0, bitmap.0)?;

    let copied = unsafe {
        BitBlt(
            memory_dc.0,
            0,
            0,
            width,
            height,
            screen_dc.0,
            x,
            y,
            SRCCOPY | CAPTUREBLT,
        )
    };

    if copied == 0 {
        return Err(io::Error::last_os_error().into());
    }

    let mut bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: 0,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [Default::default(); 1],
    };

    let mut data = vec![0_u8; width as usize * height as usize * 4];
    let scan_lines = unsafe {
        GetDIBits(
            memory_dc.0,
            bitmap.0,
            0,
            height as u32,
            data.as_mut_ptr().cast(),
            &mut bitmap_info,
            DIB_RGB_COLORS,
        )
    };

    if scan_lines == 0 {
        return Err(io::Error::last_os_error().into());
    }

    drop(selected);

    for pixel in data.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        pixel[3] = 255;
    }

    ImageBuffer::<Rgba<u8>, Vec<u8>>::from_raw(width as u32, height as u32, data)
        .ok_or_else(|| "captured pixel buffer has an invalid size".into())
}

struct DesktopDc(HDC);

impl DesktopDc {
    fn new() -> io::Result<Self> {
        let dc = unsafe { GetDC(ptr::null_mut()) };
        if dc.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(dc))
        }
    }
}

impl Drop for DesktopDc {
    fn drop(&mut self) {
        unsafe {
            ReleaseDC(ptr::null_mut(), self.0);
        }
    }
}

struct MemoryDc(HDC);

impl MemoryDc {
    fn new(source: HDC) -> io::Result<Self> {
        let dc = unsafe { CreateCompatibleDC(source) };
        if dc.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(dc))
        }
    }
}

impl Drop for MemoryDc {
    fn drop(&mut self) {
        unsafe {
            DeleteDC(self.0);
        }
    }
}

struct CompatibleBitmap(HBITMAP);

impl CompatibleBitmap {
    fn new(source: HDC, width: i32, height: i32) -> io::Result<Self> {
        let bitmap = unsafe { CreateCompatibleBitmap(source, width, height) };
        if bitmap.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(bitmap))
        }
    }
}

impl Drop for CompatibleBitmap {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.0);
        }
    }
}

struct SelectedObject {
    dc: HDC,
    original: *mut c_void,
}

impl SelectedObject {
    fn new(dc: HDC, bitmap: HBITMAP) -> io::Result<Self> {
        let original = unsafe { SelectObject(dc, bitmap) };
        if original.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self { dc, original })
        }
    }
}

impl Drop for SelectedObject {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.original);
        }
    }
}
