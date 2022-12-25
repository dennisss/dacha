use common::errors::*;

use crate::proc::current_exe;
use crate::virtual_memory::*;

pub struct TLSSegment {
    /// Initial value of all non-zero TLS variables (the '.tdata' section in the
    /// binary).
    data: &'static [u8],

    /// Number of bytes taken up by TLS data. If larger than 'data.len()', then
    /// zero padding needs to be added.
    memory_size: usize,
}

impl TLSSegment {
    /// Retrieves the data associated with the PT_TLS segment in the running
    /// binary.
    pub fn find() -> Result<TLSSegment> {
        let exe = current_exe()?;
        let vmem = VirtualMemoryMap::read_current()?;

        // Get all mapped memory associated with our current binary.
        let mut exe_areas = vec![];

        for area in &vmem.areas {
            if area.path == exe && !area.permissions.write {
                exe_areas.push(area);
            }
        }

        exe_areas.sort_by_key(|v| v.offset);

        // Read the program headers from the executable. We assume that Linux has
        // minimally mapped up to the PT_PHDR into memory.
        let elf = {
            if exe_areas.is_empty() || exe_areas[0].offset != 0 {
                return Err(err_msg(
                    "Expected at least the first pages of the ELF to be mapped into memory",
                ));
            }

            let exe_headers = unsafe {
                core::slice::from_raw_parts(
                    exe_areas[0].start_address as *const u8,
                    (exe_areas[0].end_address - exe_areas[0].start_address) as usize,
                )
            };

            elf::ELF::parse_some(exe_headers, true, false)?
        };

        // Find the PT_TLS program header.
        let tls_header = {
            let mut seg = None;
            for segment in &elf.program_headers {
                if segment.typ == elf::ProgramHeaderType::PT_TLS.to_value() {
                    seg = Some(segment);
                    break;
                }
            }

            seg.ok_or_else(|| err_msg("ELF contains no PT_TLS segment"))?
        };

        // Find a region of memory mapped to the ELF file region.
        let tls_data = {
            let mut data = None;

            for memory_area in &exe_areas {
                let end_offset =
                    (memory_area.end_address - memory_area.start_address) + memory_area.offset;

                if memory_area.offset <= tls_header.offset
                    && end_offset >= (tls_header.offset + tls_header.file_size)
                {
                    data = Some(unsafe {
                        core::slice::from_raw_parts(
                            (memory_area.start_address + (tls_header.offset - memory_area.offset))
                                as *const u8,
                            tls_header.file_size as usize,
                        )
                    });
                    break;
                }
            }

            data.ok_or_else(|| err_msg("Failed to find TLS segment in memory."))?
        };

        Ok(TLSSegment {
            data: tls_data,
            memory_size: tls_header.mem_size as usize,
        })
    }

    pub fn memory_size(&self) -> usize {
        self.memory_size
    }

    pub fn copy_to(&self, out: &mut [u8]) {
        assert_eq!(out.len(), self.memory_size);

        out[0..self.data.len()].copy_from_slice(self.data);

        for v in &mut out[self.data.len()..] {
            *v = 0;
        }
    }
}
