use std::ffi::CString;

use common::errors::*;
use elf::ELF;

use crate::file::blocking_read_to_string;
use crate::readlink;
use crate::virtual_memory::*;

const EXE_PATH: &'static [u8] = b"/proc/self/exe\0";

pub fn current_exe() -> Result<String> {
    let mut buf = vec![0u8; 4096];

    let n = unsafe { readlink(EXE_PATH.as_ptr(), &mut buf) }?;

    // TODO: Make sure we always check for this.
    if n >= buf.len() {
        return Err(err_msg("Path length overflowed buffer"));
    }

    buf.truncate(n + 1);

    Ok(CString::from_vec_with_nul(buf)?.into_string()?)
}

pub fn num_cpus() -> Result<usize> {
    // TODO: Verify that all CPUs are numbered from 0 to N-1
    // TODO: Try /proc/cpuinfo if this is not evaluate.

    let mut total = 0;

    let data = blocking_read_to_string("/proc/stat")?;
    for line in data.lines() {
        if let Some(rest) = line.strip_prefix("cpu") {
            if let Some(c) = rest.chars().next() {
                if c.is_numeric() {
                    total += 1;
                }
            }
        }
    }

    Ok(total)
}

#[derive(Clone, Debug)]
pub struct Mount {
    pub device: String,
    pub mount_point: String,
    pub fs_type: String,
    pub options: String,
}

pub fn mounts() -> Result<Vec<Mount>> {
    let mut out = vec![];
    let data = blocking_read_to_string("/proc/mounts")?;
    for line in data.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let mut fields = line.split(' ');

        let device = fields.next().unwrap().to_string();
        let mount_point = fields.next().unwrap().to_string();
        let fs_type = fields.next().unwrap().to_string();
        let options = fields.next().unwrap().to_string();

        out.push(Mount {
            device,
            mount_point,
            fs_type,
            options,
        });
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_num_cpus() {
        let num = num_cpus().unwrap();
        assert!(num >= 2 && num < 1000);
        println!("Num CPUs: {}", num);
    }
}
