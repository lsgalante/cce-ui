/// Put `text` on the clipboard.
pub fn copy_to_clipboard(text: &str) {
    imp::copy(text);
}

/// The clipboard's text, if it has any.
pub fn read_from_clipboard() -> Option<String> {
    imp::read()
}

#[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
mod imp {
    pub fn copy(text: &str) {
        let text = text.to_string();
        std::thread::spawn(move || {
            if let Ok(mut child) = std::process::Command::new("wl-copy")
                .stdin(std::process::Stdio::piped())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    use std::io::Write;
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
            } else if let Ok(mut child) = std::process::Command::new("xclip")
                .arg("-selection")
                .arg("clipboard")
                .stdin(std::process::Stdio::piped())
                .spawn()
            {
                if let Some(mut stdin) = child.stdin.take() {
                    use std::io::Write;
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
            }
        });
    }

    pub fn read() -> Option<String> {
        if let Ok(output) = std::process::Command::new("wl-paste")
            .arg("-n")
            .output() {
            if output.status.success() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    return Some(text);
                }
            }
        }
        if let Ok(output) = std::process::Command::new("xclip")
            .arg("-selection")
            .arg("clipboard")
            .arg("-o")
            .output() {
            if output.status.success() {
                if let Ok(text) = String::from_utf8(output.stdout) {
                    return Some(text);
                }
            }
        }
        None
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;

    pub fn copy(text: &str) {
        let board = NSPasteboard::generalPasteboard();
        board.clearContents();
        // SAFETY: an extern static AppKit defines.
        let ty = unsafe { NSPasteboardTypeString };
        board.setString_forType(&NSString::from_str(text), ty);
    }

    pub fn read() -> Option<String> {
        // SAFETY: as above.
        let ty = unsafe { NSPasteboardTypeString };
        NSPasteboard::generalPasteboard().stringForType(ty).map(|s| s.to_string())
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::RefCell;

    #[derive(Default)]
    struct Page {
        /// What a read answers: the last paste the shell saw, or the
        /// page's own last copy, whichever came later.
        text: Option<String>,
        /// A copy not yet handed to a `copy` / `cut` event.
        copied: Option<String>,
    }

    thread_local! {
        static PAGE: RefCell<Page> = RefCell::new(Page::default());
    }

    pub fn copy(text: &str) {
        PAGE.with(|p| {
            let mut p = p.borrow_mut();
            p.text = Some(text.to_string());
            p.copied = Some(text.to_string());
        });
        write_text(text);
    }

    pub fn read() -> Option<String> {
        PAGE.with(|p| p.borrow().text.clone())
    }

    /// Write through the async Clipboard API, if the page has one: it is
    /// undefined outside a secure context, and calling into undefined
    /// would throw through the wasm frames. Its promise is awaited only
    /// to keep a refusal (no user activation, no focus) off the console
    /// as an unhandled rejection; the `copy` event path covers it.
    fn write_text(text: &str) {
        let Some(nav) = web_sys::window().map(|w| w.navigator()) else { return };
        let has = js_sys::Reflect::get(&nav, &"clipboard".into()).is_ok_and(|c| !c.is_undefined() && !c.is_null());
        if !has {
            return;
        }
        let promise = nav.clipboard().write_text(text);
        wasm_bindgen_futures::spawn_local(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
        });
    }

    /// The copy a `copy` / `cut` event should carry, taken.
    pub(crate) fn take_copied() -> Option<String> {
        PAGE.with(|p| p.borrow_mut().copied.take())
    }

    /// A `paste` event's text: what reads answer from now on.
    pub(crate) fn pasted(text: String) {
        PAGE.with(|p| p.borrow_mut().text = Some(text));
    }
}

/// The browser shell's half: the clipboard events it answers.
#[cfg(target_arch = "wasm32")]
pub(crate) use imp::{pasted, take_copied};
