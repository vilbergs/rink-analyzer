//! Taking over the macOS menu bar. The app runs as a bare binary, not an
//! `.app` bundle, and macOS then leaves the previous app's menus (often
//! Finder's) in the menu bar even though our window is in front. The usual
//! workaround: activate the Dock, then activate ourselves again.

use std::time::Duration;

use gpui::App;

/// How long the Dock stays active before we take focus back.
const DOCK_HOLD: Duration = Duration::from_millis(80);

pub fn take_menu_bar(cx: &mut App) {
    if !imp::activate_dock() {
        return;
    }
    cx.spawn(async move |cx| {
        cx.background_executor().timer(DOCK_HOLD).await;
        cx.update(|cx| cx.activate(true)).ok();
    })
    .detach();
}

#[cfg(target_os = "macos")]
// objc 0.2's `msg_send!` and `sel!` check a `cargo-clippy` cfg that newer rustc flags.
#[allow(unexpected_cfgs)]
mod imp {
    use cocoa::base::{id, nil};
    use cocoa::foundation::NSString;
    use objc::{class, msg_send, sel, sel_impl};

    /// `NSApplicationActivateIgnoringOtherApps`.
    const IGNORING_OTHER_APPS: u64 = 1 << 1;

    /// Activate the Dock. Returns false if it isn't running.
    pub fn activate_dock() -> bool {
        unsafe {
            let bundle = NSString::alloc(nil).init_str("com.apple.dock");
            let apps: id = msg_send![class!(NSRunningApplication), runningApplicationsWithBundleIdentifier: bundle];
            let dock: id = msg_send![apps, firstObject];
            if dock == nil {
                return false;
            }
            let _: bool = msg_send![dock, activateWithOptions: IGNORING_OTHER_APPS];
            true
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    pub fn activate_dock() -> bool {
        false
    }
}
