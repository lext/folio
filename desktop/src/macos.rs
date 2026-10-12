//! macOS application-menu adjustments.
// objc 0.2 macros test a cfg that newer compilers do not know.
#![allow(unexpected_cfgs)]
use objc::{
    class, msg_send,
    runtime::{Object, Sel},
    sel, sel_impl,
};

/// The default application menu gives Cmd+H to "Hide", and a menu shortcut is
/// taken before the window sees the key. Clear that shortcut so a Folio binding
/// on Cmd+H works; the menu item itself stays. Call on the main thread once the
/// menu exists. Calling it again does nothing.
pub fn release_hide_shortcut() {
    // SAFETY: plain AppKit getters and one setter on the main thread; every
    // object is checked for nil before it is messaged for a non-object result.
    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let menu: *mut Object = msg_send![app, mainMenu];
        if menu.is_null() {
            return;
        }
        let empty: *mut Object = msg_send![class!(NSString), string];
        let groups: isize = msg_send![menu, numberOfItems];
        for group in 0..groups {
            let item: *mut Object = msg_send![menu, itemAtIndex: group];
            let submenu: *mut Object = msg_send![item, submenu];
            if submenu.is_null() {
                continue;
            }
            let entries: isize = msg_send![submenu, numberOfItems];
            for entry in 0..entries {
                let entry: *mut Object = msg_send![submenu, itemAtIndex: entry];
                // A separator has no action; its null selector matches nothing.
                let action: Sel = msg_send![entry, action];
                if action == sel!(hide:) {
                    let _: () = msg_send![entry, setKeyEquivalent: empty];
                }
            }
        }
    }
}
