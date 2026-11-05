//! Platform console and dialog helpers.

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    fn FreeConsole() -> i32;
}

/// Close the console window when this process is its only user.
#[cfg(windows)]
pub(crate) fn detach_console_if_owner() {
    let mut list = [0u32; 4];
    // SAFETY: the pointer and length describe a stack buffer we own.
    unsafe {
        let attached = GetConsoleProcessList(list.as_mut_ptr(), list.len() as u32);
        if attached == 1 {
            FreeConsole();
        }
    }
}

#[cfg(not(windows))]
pub(crate) fn detach_console_if_owner() {}

#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(
        hwnd: *mut std::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        kind: u32,
    ) -> i32;
}

/// GUI failures would otherwise vanish when the console is detached.
pub(crate) fn report_ui_error(message: &str) {
    #[cfg(windows)]
    {
        let text: Vec<u16> = message.encode_utf16().chain(std::iter::once(0)).collect();
        let caption: Vec<u16> = "Super Champ X2"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: both pointers reference null-terminated buffers alive here.
        unsafe {
            MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10);
        }
    }
    #[cfg(not(windows))]
    {
        // The terminal (or the desktop launcher's journal) is the only place
        // left to report a failed editor start.
        eprintln!("{message}");
    }
}
