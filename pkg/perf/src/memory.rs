use std::collections::{BTreeMap, HashMap};
use std::ffi::{CStr, CString};
use std::ops::Bound::{Excluded, Included, Unbounded};

use common::concat_slice::ConcatSlicePair;
use common::errors::*;
use elf::*;
use parsing::binary::*;
use sys::bindings::*;
use sys::VirtualMemoryMap;

// TODO: Add logic here to read all available build-ids.

/// Description of all the symbols/files loaded in a process.
pub struct MemoryMap {
    regions: VirtualMemoryMap,
    build_ids: HashMap<String, Vec<u8>>,
    symbols: BTreeMap<u64, MemoryMappedSymbol>,
}

pub struct MemoryMappedSymbol {
    pub start_address: u64,
    pub end_address: u64,
    pub area_index: usize,
    pub function_name: Option<String>,
}

impl MemoryMap {
    pub async fn read_self() -> Result<Self> {
        let regions = VirtualMemoryMap::read_current()?;

        let mut symbols = BTreeMap::<u64, MemoryMappedSymbol>::new();

        let mut build_ids = HashMap::new();

        // TODO: Read these from memory mapped data.
        let mut symbols_per_file = HashMap::new();

        for (area_index, area) in regions.areas.iter().enumerate() {
            // Non-executable areas aren't interesting to us as we are primarily focused on
            // symbolizing stack traces.
            if !area.permissions.execute {
                continue;
            }

            if !area.path.starts_with("/") {
                symbols.insert(
                    area.start_address,
                    MemoryMappedSymbol {
                        start_address: area.start_address,
                        end_address: area.end_address,
                        area_index,
                        function_name: None,
                    },
                );
                continue;
            }

            let file_symbols = match symbols_per_file.get(&area.path) {
                Some(v) => v,
                None => {
                    /*
                    TODO: When area.path == "[vdso]", we can read the ELF from memory to get the build id.
                    */

                    let object = ELF::read(&area.path).await?;
                    let symbols = object.function_symbols()?;
                    symbols_per_file.insert(&area.path, symbols);
                    if let Some(id) = object.build_id()? {
                        build_ids.insert(area.path.clone(), id.to_vec());
                    }
                    symbols_per_file.get(&area.path).unwrap()
                }
            };

            // Get all symbols that start in the mapped memory area.
            for (_, symbol) in file_symbols.range((
                Included(area.offset),
                Excluded(area.offset + (area.end_address - area.start_address)),
            )) {
                let start_address = area.start_address + (symbol.file_start_offset - area.offset);
                let end_address =
                    start_address + (symbol.file_end_offset - symbol.file_start_offset);

                if end_address >= area.end_address {
                    return Err(err_msg("Symbol overlaps end of mapped area"));
                }

                symbols.insert(
                    start_address,
                    MemoryMappedSymbol {
                        start_address,
                        end_address,
                        area_index,
                        function_name: Some(symbol.name.clone()),
                    },
                );
            }
        }

        Ok(Self {
            regions,
            symbols,
            build_ids,
        })
    }

    pub fn regions(&self) -> &VirtualMemoryMap {
        &self.regions
    }

    pub fn build_id(&self, file_path: &str) -> Option<&[u8]> {
        self.build_ids.get(file_path).map(|v| v.as_ref())
    }

    pub fn lookup_symbol(&self, instruction_pointer: u64) -> Option<&MemoryMappedSymbol> {
        self.symbols
            .range((Unbounded, Included(instruction_pointer)))
            .next_back()
            .map(|(_, v)| v)
    }
}
