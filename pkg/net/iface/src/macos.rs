use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use common::errors::*;

use objc2_core_foundation::{
    CFArrayGetCount, CFArrayGetValueAtIndex, CFStringGetCString,
    CFString, CFRetained, CFStringBuiltInEncodings
};
use objc2_system_configuration::{
    kSCNetworkInterfaceTypeEthernet, kSCNetworkInterfaceTypeIEEE80211,
    SCNetworkInterfaceCopyAll, SCNetworkInterface
};

use crate::types::*;


impl NetworkInterface {
    // Enumerates everything but the addresses.
    pub(crate) fn list_basic() -> Result<Vec<Self>> {
        let mut out = vec![];

        unsafe {
            let interfaces = SCNetworkInterface::all();

            for sc_if in interfaces.cast_unchecked::<SCNetworkInterface>().iter() {                
                let name = match get_cf_string(sc_if.bsd_name()) {
                    Some(v) => v,
                    None => continue
                };

                let description = match get_cf_string(sc_if.localized_display_name()) {
                    Some(v) => v,
                    None => String::new()
                };

                let if_type = sc_if.interface_type();

                let typ = {
                    if if_type == Some(kSCNetworkInterfaceTypeEthernet.into()) {
                        NetworkInterfaceType::PhysicalEthernet
                    } else if if_type == Some(kSCNetworkInterfaceTypeIEEE80211.into()) {
                        NetworkInterfaceType::PhysicalWireless
                    } else {
                        NetworkInterfaceType::Unknown
                    }
                };

                // TODO: Check for errors.
                let index = {
                    let c_name = CString::new(name.clone())?;
                    libc::if_nametoindex(c_name.as_ptr())
                };

                out.push(Self {
                    index,
                    name,
                    description,
                    typ,
                    addrs: vec![],
                });
            }
        }

        Ok(out)
    }

}

unsafe fn get_cf_string(cf_str: Option<CFRetained<CFString>>) -> Option<String> {
    let cf_str = match cf_str {
        Some(v) => v,
        None => return None
    };

    let mut buf = [0i8; 64];
    let success = CFStringGetCString(
        &cf_str,
        buf.as_mut_ptr() as *mut c_char,
        buf.len() as isize,
        CFStringBuiltInEncodings::EncodingUTF8.0,
    );
    if !success {
        return None;
    }

    let c_str = match CStr::from_ptr(buf.as_ptr()).to_str() {
        Ok(v) => v,
        Err(_) => return None 
    };

    Some(c_str.to_string())
}
