pub mod bindings {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

use core::fmt::Debug;
use core::ops::Deref;
use std::sync::Arc;
use std::sync::Mutex;

use base_error::*;

static GLOBAL_INIT: Mutex<Option<i32>> = Mutex::new(None);

/// NOTE: This is automatically called internally before attempting to open a
/// display.
pub fn init() -> Result<()> {
    let mut guard = GLOBAL_INIT.lock().unwrap();

    let status = match *guard {
        Some(v) => v,
        None => {
            let v = unsafe { bindings::XInitThreads() };
            *guard = Some(v);
            v
        }
    };

    if status == 0 {
        return Err(format_err!("XInitThreads failed: {}", status));
    }

    Ok(())
}

pub struct Display {
    ptr: Arc<XPtr<bindings::_XDisplay>>,
}

impl Clone for Display {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr.clone(),
        }
    }
}

impl Display {
    pub fn open_default() -> Result<Self> {
        init()?;

        let display = unsafe { bindings::XOpenDisplay(core::ptr::null()) };
        if display == core::ptr::null_mut() {
            return Err(err_msg("Failed to open default display"));
        }

        Ok(Self {
            ptr: Arc::new(XPtr { ptr: display }),
        })
    }

    pub fn root_window(&self) -> Result<Window> {
        // TODO: Figure out how to check for success.
        let root_window =
            unsafe { bindings::XRootWindow(self.ptr.ptr, bindings::XDefaultScreen(self.ptr.ptr)) };
        Ok(Window {
            display: self.clone(),
            num: root_window,
        })
    }
}

pub struct Window {
    display: Display,
    num: u64,
}

impl Window {
    pub fn id(&self) -> u64 {
        self.num
    }

    /// Returns the visible name of the window in the window manager.
    pub fn name(&self) -> Result<Option<String>> {
        for name_prop in ["_NET_WM_VISIBLE_NAME", "_NET_WM_NAME", "WM_NAME"] {
            if let Some(name) = self.get_string_property(name_prop)? {
                return Ok(Some(name));
            }
        }

        // TODO: Figure out how to error check this.

        // NOTE: This checks the default 'WM_NAME' property.
        let mut window_name = core::ptr::null_mut();
        let status =
            unsafe { bindings::XFetchName(self.display.ptr.ptr, self.num, &mut window_name) };
        // if status == 0 {
        //     return Ok(None);
        // }

        if window_name == core::ptr::null_mut() {
            return Ok(None);
        }

        let window_name = XPtr { ptr: window_name };

        Ok(Some(
            unsafe { std::ffi::CStr::from_ptr(window_name.ptr) }
                .to_str()?
                .to_string(),
        ))
    }

    pub fn attrs(&self) -> Result<bindings::XWindowAttributes> {
        let mut attrs = bindings::XWindowAttributes::default();

        let status =
            unsafe { bindings::XGetWindowAttributes(self.display.ptr.ptr, self.num, &mut attrs) };
        if status == 0 {
            return Err(err_msg("XGetWindowAttributes failed"));
        }

        Ok(attrs)
    }

    pub fn list_properties(&self) -> Result<Vec<String>> {
        let mut num_props = 0;
        let atoms =
            unsafe { bindings::XListProperties(self.display.ptr.ptr, self.num, &mut num_props) };

        if atoms == core::ptr::null_mut() {
            return Ok(vec![]);
        }

        let atoms = XPtr { ptr: atoms };

        let atoms_slice = unsafe { core::slice::from_raw_parts(atoms.ptr, num_props as usize) };

        let mut out = vec![];
        for atom in atoms_slice {
            let name = unsafe { bindings::XGetAtomName(self.display.ptr.ptr, *atom) };
            if name == core::ptr::null_mut() {
                return Err(err_msg("XGetAtomName failed"));
            }

            let name = XPtr { ptr: name };

            let s = unsafe { std::ffi::CStr::from_ptr(name.ptr) }
                .to_str()?
                .to_string();
            out.push(s)
        }

        Ok(out)
    }

    pub fn get_full_image(
        &self,
        attrs: &bindings::XWindowAttributes,
    ) -> Result<XPtr<bindings::_XImage>> {
        let ptr = unsafe {
            bindings::XGetImage(
                self.display.ptr.ptr,
                self.num,
                0,
                0,
                attrs.width as u32,
                attrs.height as u32,
                bindings::XAllPlanes(),
                bindings::ZPixmap as i32,
            )
        };

        if ptr == core::ptr::null_mut() {
            return Err(err_msg("XGetImage failed"));
        }

        Ok(XPtr { ptr })
    }

    pub fn pid(&self) -> Result<Option<u32>> {
        let prop = match self.get_property("_NET_WM_PID")? {
            Some(v) => v,
            None => return Ok(None),
        };

        if prop.nitems != 1 || prop.format != 32 {
            // TODO: Also check the 'typ'.
            return Err(err_msg("Bad format for PID property"));
        }

        let val = unsafe { *core::mem::transmute::<_, *mut u32>(prop.data) };

        Ok(Some(val))
    }

    pub fn client_list(&self) -> Result<Vec<Window>> {
        let property = self
            .get_property("_NET_CLIENT_LIST")?
            .ok_or_else(|| err_msg("Missing client list"))?;

        let windows = unsafe {
            core::slice::from_raw_parts(
                core::mem::transmute::<_, *const bindings::Window>(property.data),
                property.nitems,
            )
        };

        Ok(windows
            .iter()
            .map(|v| Self {
                display: self.display.clone(),
                num: *v,
            })
            .collect())
    }

    fn get_property(&self, name: &str) -> Result<Option<Property>> {
        let property_name = std::ffi::CString::new(name).unwrap();

        // TODO: Error check this.
        let property =
            unsafe { bindings::XInternAtom(self.display.ptr.ptr, property_name.as_ptr(), 1) };
        if property == (bindings::None as u64) {
            return Ok(None);
        }

        let offset = 0; // Start offset in the values to retrieve.
        let len = 100; // Max values to retrieve.

        let mut actual_type = 0;
        let mut actual_format = 0;
        let mut nitems = 0;
        let mut bytes_after = 0;
        let mut prop = core::ptr::null_mut();

        let status = unsafe {
            bindings::XGetWindowProperty(
                self.display.ptr.ptr,
                self.num,
                property,
                offset,
                len,
                0,                                // 'delete'
                bindings::AnyPropertyType as u64, // requested type.
                &mut actual_type,
                &mut actual_format,
                &mut nitems,
                &mut bytes_after,
                &mut prop,
            )
        };

        if status != bindings::Success as i32 {
            return Err(err_msg("XGetWindowProperty failed"));
        }

        if actual_type == (bindings::None as u64) {
            // Property is missing on this window.

            assert_eq!(bytes_after, 0);
            assert_eq!(actual_format, 0);
            assert_eq!(prop, core::ptr::null_mut());
            return Ok(None);
        }

        assert_ne!(prop, core::ptr::null_mut());

        // TODO: Check that prop is non-null (if so, wrap in a ptr immediately)
        let prop = XPtr { ptr: prop };

        if bytes_after != 0 {
            return Err(err_msg("Overflow while reading property failure"));
        }

        let prop = Property {
            data: prop,
            nitems: nitems as usize,
            typ: actual_type,
            format: actual_format,
        };

        Ok(Some(prop))
    }

    fn get_string_property(&self, name: &str) -> Result<Option<String>> {
        let prop = match self.get_property(name)? {
            Some(v) => v,
            None => return Ok(None),
        };

        if prop.format != 8 {
            return Err(err_msg("Invalid format for string property."));
        }

        let s = core::str::from_utf8(unsafe {
            core::slice::from_raw_parts(prop.data.ptr, prop.nitems)
        })?;

        Ok(Some(s.to_string()))
    }
}

// TODO: Don't allow debugging as 'data' may point to an empty list (actually
// always one that at least has one null)
#[derive(Debug)]
struct Property {
    // TODO: Make this a void pointer to avoid deferencing it without checking the length.
    data: XPtr<u8>,
    nitems: usize,
    typ: u64,
    format: i32,
}

/// Non-null pointer to const memory allocated by X11.
pub struct XPtr<T> {
    ptr: *mut T,
}

impl<T: Debug> Debug for XPtr<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.as_ref())
    }
}

impl<T> Drop for XPtr<T> {
    fn drop(&mut self) {
        assert!(unsafe { bindings::XFree(core::mem::transmute(self.ptr)) } != 0);
    }
}

impl<T> Deref for XPtr<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<T> AsRef<T> for XPtr<T> {
    fn as_ref(&self) -> &T {
        unsafe { &*self.ptr }
    }
}
