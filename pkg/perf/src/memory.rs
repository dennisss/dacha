use std::ffi::{CStr, CString};
use std::collections::{HashMap, BTreeMap};
use std::ops::Bound::{Included, Excluded, Unbounded};

use common::errors::*;
use common::concat_slice::ConcatSlicePair;
use sys::bindings::*;
use sys::VirtualMemoryMap;
use parsing::binary::*;
use elf::*;

// TODO: Add logic here to read all available build-ids.

/// Description of all the symbols/files loaded in a process.
pub struct MemoryMap {
    regions: VirtualMemoryMap,
    symbols: BTreeMap<u64, MemoryMappedSymbolInternal>
}

struct MemoryMappedSymbolInternal {
    pub start_address: u64,
    pub end_address: u64,
    pub area_index: usize,
    pub function_name: Option<String>
}

pub struct MemoryMappedSymbol<'a> {
    pub start_address: u64,
    pub end_address: u64,
    pub file_path: &'a str,
    pub function_name: Option<&'a str>
}

impl MemoryMap {
    pub async fn read_self() -> Result<Self> {
        let regions = VirtualMemoryMap::read_current()?;

        let mut symbols = BTreeMap::<u64, MemoryMappedSymbolInternal>::new();
    
        // TODO: Read these from memory mapped data.
        let mut symbols_per_file = HashMap::new();
    
        for (area_index, area) in regions.areas.iter().enumerate() {
            // Non-executable areas aren't interesting to us as we are primarily focused on symbolizing stack traces.
            if !area.permissions.execute {
                continue;
            }
    
            if !area.path.starts_with("/") {
                symbols.insert(area.start_address, MemoryMappedSymbolInternal {
                    start_address: area.start_address,
                    end_address: area.end_address,
                    area_index,
                    function_name: None
                });
                continue;
            }
    
            let file_symbols = match symbols_per_file.get(&area.path) {
                Some(v) => v,
                None => {
                    let object = ELF::read(&area.path).await?;
                    let symbols = object.function_symbols()?;
                    symbols_per_file.insert(&area.path, symbols);
                    symbols_per_file.get(&area.path).unwrap()
                }
            };
    
            // Get all symbols that start in the mapped memory area.
            for (_, symbol) in file_symbols.range((Included(area.offset), Excluded(area.offset + (area.end_address - area.start_address)))) {
    
                let start_address = area.start_address + (symbol.file_start_offset - area.offset);
                let end_address = start_address + (symbol.file_end_offset - symbol.file_start_offset);
    
                if end_address >= area.end_address {
                    return Err(err_msg("Symbol overlaps end of mapped area"));
                }
    
                symbols.insert(start_address, MemoryMappedSymbolInternal {
                    start_address,
                    end_address,
                    area_index,
                    function_name: Some(symbol.name.clone())
                });
            }
        }
    
        Ok(Self {
            regions,
            symbols
        })
    }

    pub fn lookup_symbol(&self, instruction_pointer: u64) -> Option<MemoryMappedSymbol> {
        if let Some((_, symbol)) = self.symbols.range((Unbounded, Included(instruction_pointer))).next_back() {
            Some(MemoryMappedSymbol {
                start_address: symbol.start_address,
                end_address: symbol.end_address,
                file_path: &self.regions.areas[symbol.area_index].path,
                function_name: symbol.function_name.as_ref().map(|s| s.as_str())
            })
        } else {
            None
        }
    }

}