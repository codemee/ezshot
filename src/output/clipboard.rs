use anyhow::{Context, Result};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{BITMAPINFOHEADER, BI_RGB};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

use crate::capture::screen::ScreenBitmap;

pub fn copy_to_clipboard(bmp: &ScreenBitmap) -> Result<()> {
    // ── CF_DIB ──────────────────────────────────────────────────────────────
    // CF_DIB 慣例：bottom-up（biHeight 為正值）；top-down 的 DIB 放到剪貼簿後
    // Windows 的 CF_BITMAP 自動合成會失敗，導致 GetImage() 回傳 null
    let header = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: bmp.width,
        biHeight: bmp.height, // 正值 = bottom-up
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        biSizeImage: (bmp.width * bmp.height * 4) as u32,
        biXPelsPerMeter: 2835,
        biYPelsPerMeter: 2835,
        biClrUsed: 0,
        biClrImportant: 0,
    };
    let dib_total = std::mem::size_of::<BITMAPINFOHEADER>() + bmp.data.len();
    let dib_mem = unsafe {
        let hmem = GlobalAlloc(GMEM_MOVEABLE, dib_total).context("GlobalAlloc DIB")?;
        let ptr = GlobalLock(hmem) as *mut u8;
        anyhow::ensure!(!ptr.is_null(), "GlobalLock DIB failed");
        std::ptr::copy_nonoverlapping(
            &header as *const _ as *const u8,
            ptr,
            std::mem::size_of::<BITMAPINFOHEADER>(),
        );
        // bottom-up：第 0 列存圖片最後一列（像素倒序寫入），同時補 alpha=0xFF
        let pixel_dst = ptr.add(std::mem::size_of::<BITMAPINFOHEADER>());
        let row_size = (bmp.width * 4) as usize;
        for row in 0..bmp.height as usize {
            let src_row = bmp.height as usize - 1 - row;
            let src = bmp.data.as_ptr().add(src_row * row_size);
            let dst = pixel_dst.add(row * row_size);
            std::ptr::copy_nonoverlapping(src, dst, row_size);
            for col in 0..bmp.width as usize {
                *dst.add(col * 4 + 3) = 0xFF;
            }
        }
        GlobalUnlock(hmem);
        hmem
    };

    // ── PNG（Electron / Chromium 系應用：Claude Code、VS Code、瀏覽器…）──
    // 轉換 BGRA → RGBA 並編碼成 PNG bytes
    let png_mem = (|| -> Option<_> {
        let mut rgba = vec![0u8; bmp.data.len()];
        for (i, chunk) in bmp.data.chunks_exact(4).enumerate() {
            rgba[i * 4]     = chunk[2]; // R
            rgba[i * 4 + 1] = chunk[1]; // G
            rgba[i * 4 + 2] = chunk[0]; // B
            rgba[i * 4 + 3] = 0xFF;     // A
        }
        let img = image::RgbaImage::from_raw(bmp.width as u32, bmp.height as u32, rgba)?;
        let mut buf = std::io::Cursor::new(Vec::<u8>::new());
        img.write_to(&mut buf, image::ImageFormat::Png).ok()?;
        let png_bytes = buf.into_inner();
        unsafe {
            let hmem = GlobalAlloc(GMEM_MOVEABLE, png_bytes.len()).ok()?;
            let ptr = GlobalLock(hmem) as *mut u8;
            if ptr.is_null() { return None; }
            std::ptr::copy_nonoverlapping(png_bytes.as_ptr(), ptr, png_bytes.len());
            GlobalUnlock(hmem);
            Some(hmem)
        }
    })();

    // ── 一次性放入剪貼簿 ───────────────────────────────────────────────────
    unsafe {
        OpenClipboard(HWND(std::ptr::null_mut())).context("OpenClipboard")?;
        EmptyClipboard().context("EmptyClipboard")?;

        SetClipboardData(8, windows::Win32::Foundation::HANDLE(dib_mem.0))
            .context("SetClipboardData CF_DIB")?;

        if let Some(hmem) = png_mem {
            let fmt = RegisterClipboardFormatW(windows::core::w!("PNG"));
            if fmt != 0 {
                let _ = SetClipboardData(fmt, windows::Win32::Foundation::HANDLE(hmem.0));
            }
        }

        CloseClipboard().context("CloseClipboard")?;
        Ok(())
    }
}
