//! Layout tests do not call libmpv or require native graphics.
use super::ffi::{Handle, LogMessage, enable_warning_capture};
use std::mem::{align_of, offset_of, size_of};

#[test]
fn warning_capture_is_best_effort_and_requests_only_warn() {
    unsafe extern "C" fn supported(_: *mut Handle, level: *const std::ffi::c_char) -> i32 {
        assert_eq!(unsafe { std::ffi::CStr::from_ptr(level) }, c"warn");
        0
    }
    unsafe extern "C" fn unavailable(_: *mut Handle, _: *const std::ffi::c_char) -> i32 {
        -3
    }
    // These stubs deliberately do not dereference the mock handle.
    unsafe {
        assert!(!enable_warning_capture(None, std::ptr::null_mut()));
        assert!(!enable_warning_capture(
            Some(unavailable),
            std::ptr::null_mut()
        ));
        assert!(enable_warning_capture(
            Some(supported),
            std::ptr::null_mut()
        ));
    }
}

#[test]
fn native_log_message_matches_client_h_layout() {
    let pointer = size_of::<*const std::ffi::c_char>();
    assert_eq!(offset_of!(LogMessage, prefix), 0);
    assert_eq!(offset_of!(LogMessage, level), pointer);
    assert_eq!(offset_of!(LogMessage, text), pointer * 2);
    assert_eq!(offset_of!(LogMessage, log_level), pointer * 3);
    assert_eq!(
        align_of::<LogMessage>(),
        align_of::<*const std::ffi::c_char>()
    );
}
