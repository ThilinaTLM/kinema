//! Minimal client/render ABI. All pointers returned by wait_event are borrowed until its next call.
use std::ffi::{c_char, c_int, c_void};
pub type Handle = c_void;
pub type RenderContext = c_void;
#[repr(C)]
pub struct Event {
    pub id: c_int,
    pub error: c_int,
    pub userdata: u64,
    pub data: *mut c_void,
}
#[repr(C)]
pub struct Property {
    pub name: *const c_char,
    pub format: c_int,
    pub data: *mut c_void,
}
#[repr(C)]
pub struct EndFile {
    pub reason: c_int,
    pub error: c_int,
    pub playlist_entry_id: i64,
    pub playlist_insert_id: i64,
    pub playlist_insert_num_entries: c_int,
}
#[repr(C)]
pub struct Param {
    pub kind: c_int,
    pub data: *mut c_void,
}
impl Param {
    pub fn new(kind: c_int, data: *mut c_void) -> Self {
        Self { kind, data }
    }
    pub fn end() -> Self {
        Self::new(0, std::ptr::null_mut())
    }
}
// mpv_node / mpv_node_list from client.h. Union members are read only after
// checking the format discriminant. Lists and strings remain event-owned.
#[repr(C)]
pub union NodeValue {
    pub string: *const c_char,
    pub flag: c_int,
    pub int64: i64,
    pub double: f64,
    pub list: *const NodeList,
}
#[repr(C)]
pub struct Node {
    pub value: NodeValue,
    pub format: c_int,
}
#[repr(C)]
pub struct NodeList {
    pub num: c_int,
    pub values: *const Node,
    pub keys: *const *const c_char,
}
pub type Callback = unsafe extern "C" fn(*mut c_void);

pub struct Api {
    pub version: unsafe extern "C" fn() -> std::os::raw::c_ulong,
    #[cfg(feature = "experimental-gpu")]
    pub get_property_string: unsafe extern "C" fn(*mut Handle, *const c_char) -> *mut c_char,
    #[cfg(feature = "experimental-gpu")]
    pub free: unsafe extern "C" fn(*mut c_void),
    pub create: unsafe extern "C" fn() -> *mut Handle,
    pub initialize: unsafe extern "C" fn(*mut Handle) -> c_int,
    pub terminate: unsafe extern "C" fn(*mut Handle),
    pub error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    pub option: unsafe extern "C" fn(*mut Handle, *const c_char, *const c_char) -> c_int,
    pub observe: unsafe extern "C" fn(*mut Handle, u64, *const c_char, c_int) -> c_int,
    pub command: unsafe extern "C" fn(*mut Handle, u64, *const *const c_char) -> c_int,
    pub wait: unsafe extern "C" fn(*mut Handle, f64) -> *mut Event,
    pub render_create:
        unsafe extern "C" fn(*mut *mut RenderContext, *mut Handle, *mut Param) -> c_int,
    pub render_callback: unsafe extern "C" fn(*mut RenderContext, Option<Callback>, *mut c_void),
    pub render_update: unsafe extern "C" fn(*mut RenderContext) -> u64,
    pub render: unsafe extern "C" fn(*mut RenderContext, *mut Param) -> c_int,
    pub render_free: unsafe extern "C" fn(*mut RenderContext),
}
