//! Trackpad pinch. GPUI 0.2 drops macOS magnify gestures, so on macOS we add a
//! `magnifyWithEvent:` handler to GPUI's view class and forward each step over
//! a channel.

use futures::channel::mpsc::UnboundedReceiver;
use gpui::{Pixels, Point, Window};

/// One step of a pinch gesture.
#[derive(Debug, Clone, Copy)]
pub struct Pinch {
    /// Scale change for this step: above 1 zooms in.
    pub factor: f32,
    /// Pointer position in window coordinates, like GPUI's mouse events.
    pub position: Point<Pixels>,
}

/// Start forwarding pinch gestures on `window`. Returns `None` where pinch
/// isn't supported. The app has one window, so the last caller gets the events.
pub fn listen(window: &Window) -> Option<UnboundedReceiver<Pinch>> {
    // Test windows have no native view to hook.
    if cfg!(test) {
        return None;
    }
    imp::listen(window)
}

#[cfg(target_os = "macos")]
// objc 0.2's `msg_send!` and `sel!` check a `cargo-clippy` cfg that newer rustc flags.
#[allow(unexpected_cfgs)]
mod imp {
    use std::sync::{Mutex, Once};

    use cocoa::base::id;
    use cocoa::foundation::{NSPoint, NSRect};
    use futures::channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
    use gpui::{point, px, Window};
    use objc::runtime::{class_addMethod, object_getClass, Class, Object, Sel};
    use objc::{msg_send, sel, sel_impl};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::Pinch;

    static SENDER: Mutex<Option<UnboundedSender<Pinch>>> = Mutex::new(None);

    pub fn listen(window: &Window) -> Option<UnboundedReceiver<Pinch>> {
        // GPUI's own `Window::window_handle` returns its window id; we want the AppKit view.
        let handle = HasWindowHandle::window_handle(window).ok()?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else { return None };
        let view = appkit.ns_view.as_ptr() as *const Object;

        static INSTALL: Once = Once::new();
        INSTALL.call_once(|| unsafe {
            let imp: extern "C" fn(&Object, Sel, id) = magnify_with_event;
            class_addMethod(
                object_getClass(view) as *mut Class,
                sel!(magnifyWithEvent:),
                std::mem::transmute::<extern "C" fn(&Object, Sel, id), objc::runtime::Imp>(imp),
                c"v@:@".as_ptr(),
            );
        });

        let (tx, rx) = unbounded();
        *SENDER.lock().ok()? = Some(tx);
        Some(rx)
    }

    extern "C" fn magnify_with_event(this: &Object, _: Sel, event: id) {
        let pinch = unsafe {
            let magnification: f64 = msg_send![event, magnification];
            let location: NSPoint = msg_send![event, locationInWindow];
            let window: id = msg_send![this, window];
            let content: id = msg_send![window, contentView];
            let frame: NSRect = msg_send![content, frame];
            // AppKit's y runs up from the bottom; GPUI's runs down from the top.
            Pinch {
                factor: (1.0 + magnification).max(0.1) as f32,
                position: point(px(location.x as f32), px((frame.size.height - location.y) as f32)),
            }
        };
        if let Some(tx) = SENDER.lock().ok().as_ref().and_then(|s| s.as_ref()) {
            let _ = tx.unbounded_send(pinch);
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use futures::channel::mpsc::UnboundedReceiver;
    use gpui::Window;

    use super::Pinch;

    pub fn listen(_: &Window) -> Option<UnboundedReceiver<Pinch>> {
        None
    }
}
