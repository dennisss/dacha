#![feature(cstr_from_bytes_until_nul)]

/*
See documentation here:
- https://man7.org/linux/man-pages/man5/elf.5.html

Constants defined in elf.h
*/

#[macro_use]
extern crate common;
#[macro_use]
extern crate parsing;

pub mod demangle;

use std::collections::BTreeMap;
use std::ffi::CStr;

use common::async_std::fs;
use common::async_std::path::Path;
use common::bytes::Bytes;
use common::errors::*;

use parsing::binary::*;
use parsing::*;

pub const SHT_SYMTAB: u32 = 2;

/// Type of the string section.
pub const SHT_STRTAB: u32 = 3;

pub const STT_FUNC: u8 = 2;

pub const PT_NOTE: u32 = 4;
pub const SHT_NOTE: u32 = 7;
pub const ELF_NOTE_GNU: &'static [u8] = b"GNU\0";
pub const NT_GNU_BUILD_ID: u32 = 3;

pub struct ELF {
    pub file: Vec<u8>,

    pub header: FileHeader,

    pub program_headers: Vec<ProgramHeader>,

    pub section_headers: Vec<SectionHeader>,
}

impl ELF {
    pub async fn read<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::read_impl(path.as_ref()).await
    }

    async fn read_impl(path: &Path) -> Result<Self> {
        let file = fs::read(path).await?;
        let header = FileHeader::parse(&file)?.0;

        let mut program_headers = vec![];
        {
            for i in 0..(header.program_header_entry_count as u64) {
                let start =
                    header.program_header_offset + i * (header.program_header_entry_size as u64);
                let end = start + header.program_header_entry_size as u64;

                let (h, rest) =
                    ProgramHeader::parse(&file[(start as usize)..(end as usize)], &header.ident)?;
                if rest.len() != 0 {
                    return Err(err_msg("Didn't parse entire program header"));
                }

                program_headers.push(h);
            }
        }

        let mut section_headers = vec![];
        {
            for i in 0..(header.section_header_entry_count as u64) {
                let start =
                    header.section_header_offset + i * (header.section_header_entry_size as u64);
                let end = start + header.section_header_entry_size as u64;

                let (h, rest) =
                    SectionHeader::parse(&file[(start as usize)..(end as usize)], &header.ident)?;
                if rest.len() != 0 {
                    return Err(err_msg("Didn't parse entire section header"));
                }

                if h.typ == SHT_STRTAB {
                    // TODO: Verify that the first and last byte of the data is
                    // 0.

                    // let strings = Bytes::from(
                    //     &file[(h.offset as usize)..(h.offset as usize +
                    // h.size as usize)], );
                    // println!("{:?}", strings);
                }

                section_headers.push(h);
            }
        }

        // println!("{:#x?}", program_headers);
        // println!("{:#x?}", section_headers);

        Ok(Self {
            file,
            header,
            program_headers,
            section_headers,
        })
    }

    fn section_data(&self, index: usize) -> &[u8] {
        let s = &self.section_headers[index];
        &self.file[(s.offset as usize)..(s.offset as usize + s.size as usize)]
    }

    // TODO: Consider other options for generating this:
    // https://lists.llvm.org/pipermail/llvm-dev/2016-June/100456.html
    pub fn build_id(&self) -> Result<Option<&[u8]>> {
        let shstrtab = StringTable {
            data: self.section_data(self.header.section_names_entry_index as usize),
        };

        // TODO: Finish implementing this.
        for (i, p) in self.program_headers.iter().enumerate() {
            if p.typ != PT_NOTE {
                continue;
            }

            // let note = Note::parse(TODO, p.align,
            // &self.header.ident)?;
        }

        // TODO: Also search through program headers for PT_NOTE types.

        for (i, section) in self.section_headers.iter().enumerate() {
            if section.typ != SHT_NOTE {
                continue;
            }

            let section_name = shstrtab.get(section.name_offset as usize)?;

            // NOTE: The VDSO library doesn't follow this naming pattern.
            /*
            if section_name != ".note.gnu.build-id" {
                continue;
            }
            */

            let notes = Note::parse(self.section_data(i), section.addr_align, &self.header.ident)?;

            // TODO: Validate there isn't more than one build id note.
            for note in notes {
                if note.name != ELF_NOTE_GNU || note.typ != NT_GNU_BUILD_ID {
                    continue;
                }

                return Ok(Some(note.desc));
            }
        }

        Ok(None)
    }

    pub fn print(&self) -> Result<()> {
        let shstrtab = StringTable {
            data: self.section_data(self.header.section_names_entry_index as usize),
        };

        // TODO: Change the address zero padding length based on whether we are dealing
        // with a 32-bit or 64-bit architecture.

        for p in self.program_headers.iter() {
            println!(
                "{:08x} [{:08x}] - {:08x}: {:?} {:?}",
                p.vaddr,
                p.paddr,
                p.vaddr + p.mem_size,
                ProgramHeaderType::from_value(p.typ),
                p.flags
            );
            println!("{:?}", p);
        }

        println!("");

        println!("Sections:");

        for (i, section) in self.section_headers.iter().enumerate() {
            let name = shstrtab.get(section.name_offset as usize)?;
            println!(
                "{:08x} - {:08x} {:?}",
                section.addr,
                section.addr + section.size,
                name
            );

            if section.typ == SHT_SYMTAB {
                let symbol_strtab = StringTable {
                    data: self.section_data(section.link as usize),
                };

                let mut data = self.section_data(i);

                while !data.is_empty() {
                    let sym = parse_next!(data, |v| Symbol::parse(v, &self.header.ident));

                    if sym.typ() == STT_FUNC {
                        continue;
                    }

                    let sym_name = symbol_strtab.get(sym.name as usize)?;
                    // println!("=> {}", sym_name);

                    // if sym.

                    if sym.typ() == STT_FUNC {
                        let related_section = &self.section_headers[sym.section_index as usize];
                        assert!(sym.value >= related_section.addr);
                        assert!(
                            sym.value + sym.size <= related_section.addr + related_section.size
                        );
                    }

                    /*
                    let file_start_offset = related_section.offset + (sym.value - related_section.addr);
                    let file_end_offset = file_start_offset + sym.size;

                    // 14a87, 14a9f, 26874
                    let search_offset = 0x14a87;
                    if search_offset >= file_start_offset && search_offset < file_end_offset {
                        println!("Found in {}", sym_name);
                    }
                    */
                }
            }
        }

        Ok(())
    }

    pub fn function_symbols(&self) -> Result<BTreeMap<u64, FunctionSymbol>> {
        let mut out = BTreeMap::<u64, FunctionSymbol>::new();

        for (i, section) in self.section_headers.iter().enumerate() {
            if section.typ != SHT_SYMTAB {
                continue;
            }

            let symbol_strtab = StringTable {
                data: self.section_data(section.link as usize),
            };

            let mut data = self.section_data(i);

            while !data.is_empty() {
                let sym = parse_next!(data, |v| Symbol::parse(v, &self.header.ident));

                if sym.typ() != STT_FUNC || sym.size == 0 {
                    continue;
                }

                let sym_name = symbol_strtab.get(sym.name as usize)?;

                let related_section = &self.section_headers[sym.section_index as usize];
                let file_start_offset = related_section.offset + (sym.value - related_section.addr);
                let file_end_offset = file_start_offset + sym.size;

                // TODO: Have a good way which one is best.
                // TODO: Instead search for the next smallest and largest symbols to check for
                // overlap. TODO: Must also comapre the end offset.
                if let Some(existing_symbol) = out.get(&file_start_offset) {
                    if existing_symbol.file_end_offset != file_end_offset {
                        return Err(err_msg("Overlapping functions"));
                    }

                    // Prefer to keep the non-__ prefixed symbol.
                    if sym_name.starts_with("__") {
                        continue;
                    }

                    // println!("Dup: {} {}", existing_symbol.name, sym_name);
                }

                out.insert(
                    file_start_offset,
                    FunctionSymbol {
                        name: crate::demangle::demangle_name(sym_name),
                        file_start_offset,
                        file_end_offset,
                    },
                );
            }
        }

        Ok(out)
    }
}

#[derive(Clone, Debug)]
pub struct FunctionSymbol {
    pub name: String,
    pub file_start_offset: u64,
    pub file_end_offset: u64,
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: u32,

    /// In executable and shared object files, this is a virtual address.
    pub value: u64, // 32-bit if on 32

    pub size: u64, // Typed
    pub info: u8,
    pub other: u8,

    /// Index of the section associated with this symbol.
    pub section_index: u16,
}

impl Symbol {
    fn typ(&self) -> u8 {
        self.info & 0x0f
    }

    fn bind(&self) -> u8 {
        self.info >> 4
    }

    fn parse<'a>(mut input: &'a [u8], ident: &FileIdentifier) -> Result<(Self, &'a [u8])> {
        let name = parse_next!(input, |v| ident.parse_u32(v));

        match ident.format {
            Format::I32 => {
                let value = parse_next!(input, |v| ident.parse_addr(v));
                let size = parse_next!(input, |v| ident.parse_addr(v));
                let info = parse_next!(input, be_u8);
                let other = parse_next!(input, be_u8);
                let section_index = parse_next!(input, |v| ident.parse_u16(v));

                Ok((
                    Self {
                        name,
                        value,
                        size,
                        info,
                        other,
                        section_index,
                    },
                    input,
                ))
            }
            Format::I64 => {
                let info = parse_next!(input, be_u8);
                let other = parse_next!(input, be_u8);
                let section_index = parse_next!(input, |v| ident.parse_u16(v));
                let value = parse_next!(input, |v| ident.parse_addr(v));
                let size = parse_next!(input, |v| ident.parse_addr(v));

                Ok((
                    Self {
                        name,
                        value,
                        size,
                        info,
                        other,
                        section_index,
                    },
                    input,
                ))
            }
        }
    }
}

struct StringTable<'a> {
    data: &'a [u8],
}

impl<'a> StringTable<'a> {
    fn get(&self, index: usize) -> Result<&str> {
        let s = CStr::from_bytes_until_nul(&self.data[index..])?.to_str()?;
        Ok(s)
    }
}

enum_def!(Format u8 =>
    I32 = 1,
    I64 = 2
);

enum_def!(Endian u8 =>
    Little = 1,
    Big = 2
);

#[derive(Debug)]
pub struct FileIdentifier {
    format: Format,
    endian: Endian,
    version: u8,
    os_abi: u8,
    abi_version: u8,
}

impl FileIdentifier {
    parser!(parse<&[u8], Self> => seq!(c => {
        let magic = c.next(take_exact(4))?;
        if &magic != b"\x7FELF" {
            return Err(err_msg("Bad magic"));
        }

        let format = Format::from_value(c.next(be_u8)?)?;
        let endian = Endian::from_value(c.next(be_u8)?)?;

        let version = c.next(be_u8)?;
        if version != 1 {
            return Err(err_msg("Unknown ELF version"));
        }

        let os_abi = c.next(be_u8)?;

        let abi_version = c.next(be_u8)?;

        // TODO: Verify is all zeros.
        let padding = c.next(take_exact(7))?;

        Ok(Self {
            format, endian, version, os_abi, abi_version
        })
    }));

    fn parse_addr<'a>(&self, input: &'a [u8]) -> Result<(u64, &'a [u8])> {
        match (self.format, self.endian) {
            (Format::I32, Endian::Little) => map(le_u32, |v| v as u64)(input),
            (Format::I32, Endian::Big) => map(be_u32, |v| v as u64)(input),
            (Format::I64, Endian::Little) => le_u64(input),
            (Format::I64, Endian::Big) => be_u64(input),
        }
    }

    fn parse_u16<'a>(&self, input: &'a [u8]) -> Result<(u16, &'a [u8])> {
        match self.endian {
            Endian::Little => le_u16(input),
            Endian::Big => be_u16(input),
        }
    }

    fn parse_u32<'a>(&self, input: &'a [u8]) -> Result<(u32, &'a [u8])> {
        match self.endian {
            Endian::Little => le_u32(input),
            Endian::Big => be_u32(input),
        }
    }
}

#[derive(Debug)]
pub struct FileHeader {
    ident: FileIdentifier,
    typ: u16,
    machine: u16,
    version: u32,
    entry_point: u64,
    flags: u32,
    program_header_offset: u64,
    program_header_entry_size: u16,
    program_header_entry_count: u16,

    section_header_offset: u64,
    section_header_entry_size: u16,
    section_header_entry_count: u16,

    section_names_entry_index: u16,
}

impl FileHeader {
    fn parse<'a>(mut input: &'a [u8]) -> Result<(Self, &'a [u8])> {
        let ident = parse_next!(input, FileIdentifier::parse);
        let typ = parse_next!(input, |v| ident.parse_u16(v));
        let machine = parse_next!(input, |v| ident.parse_u16(v));
        let version = parse_next!(input, |v| ident.parse_u32(v));
        let entry_point = parse_next!(input, |v| ident.parse_addr(v));
        let program_header_offset = parse_next!(input, |v| ident.parse_addr(v));
        let section_header_offset = parse_next!(input, |v| ident.parse_addr(v));
        let flags = parse_next!(input, |v| ident.parse_u32(v));
        let header_size = parse_next!(input, |v| ident.parse_u16(v));
        let program_header_entry_size = parse_next!(input, |v| ident.parse_u16(v));
        let program_header_entry_count = parse_next!(input, |v| ident.parse_u16(v));
        let section_header_entry_size = parse_next!(input, |v| ident.parse_u16(v));
        let section_header_entry_count = parse_next!(input, |v| ident.parse_u16(v));
        let section_names_entry_index = parse_next!(input, |v| ident.parse_u16(v));

        Ok((
            Self {
                ident,
                typ,
                machine,
                version,
                entry_point,
                flags,
                program_header_offset,
                program_header_entry_size,
                program_header_entry_count,
                section_header_offset,
                section_header_entry_size,
                section_header_entry_count,
                section_names_entry_index,
            },
            input,
        ))
    }
}

// TODO: Define this with define_c_enum and only store in 32 bits
// TODO: Switch over all usages to this.
enum_def_with_unknown!(ProgramHeaderType u32 =>
    PT_NULL = 0,
    PT_LOAD = 1,
    PT_DYNAMIC = 2,
    PT_INTERP = 3,
    PT_NOTE = 4,
    PT_SHLIB = 5,
    PT_PHDR = 6,
    PT_TLS = 7
);

define_bit_flags!(SegmentFlags u32 {
    // Segment is executable
    PF_X = 1 << 0,

    // Segment is writable
    PF_W = 1 << 1,

    // Segment is readable
    PF_R = 1 << 2
});

#[derive(Debug)]
pub struct ProgramHeader {
    pub typ: u32,
    pub flags: SegmentFlags,
    pub offset: u64,
    pub vaddr: u64,
    pub paddr: u64,
    pub file_size: u64,
    pub mem_size: u64,
    pub align: u64,
}

impl ProgramHeader {
    fn parse<'a>(mut input: &'a [u8], ident: &FileIdentifier) -> Result<(Self, &'a [u8])> {
        let typ = parse_next!(input, |v| ident.parse_u32(v));
        let mut flags = SegmentFlags::empty();
        if ident.format == Format::I64 {
            flags = SegmentFlags::from_raw(parse_next!(input, |v| ident.parse_u32(v)));
        }
        let offset = parse_next!(input, |v| ident.parse_addr(v));
        let vaddr = parse_next!(input, |v| ident.parse_addr(v));
        let paddr = parse_next!(input, |v| ident.parse_addr(v));
        let file_size = parse_next!(input, |v| ident.parse_addr(v));
        let mem_size = parse_next!(input, |v| ident.parse_addr(v));
        if ident.format == Format::I32 {
            flags = SegmentFlags::from_raw(parse_next!(input, |v| ident.parse_u32(v)));
        }
        let align = parse_next!(input, |v| ident.parse_addr(v));

        Ok((
            Self {
                typ,
                flags,
                offset,
                vaddr,
                paddr,
                file_size,
                mem_size,
                align,
            },
            input,
        ))
    }
}

#[derive(Debug)]
pub struct SectionHeader {
    name_offset: u32,
    typ: u32,
    flags: u64,
    addr: u64,
    offset: u64,
    size: u64,
    link: u32,
    info: u32,
    addr_align: u64,
    entry_size: u64,
}

impl SectionHeader {
    fn parse<'a>(mut input: &'a [u8], ident: &FileIdentifier) -> Result<(Self, &'a [u8])> {
        let name_offset = parse_next!(input, |v| ident.parse_u32(v));
        let typ = parse_next!(input, |v| ident.parse_u32(v));
        let flags = parse_next!(input, |v| ident.parse_addr(v));
        let addr = parse_next!(input, |v| ident.parse_addr(v));
        let offset = parse_next!(input, |v| ident.parse_addr(v));
        let size = parse_next!(input, |v| ident.parse_addr(v));
        let link = parse_next!(input, |v| ident.parse_u32(v));
        let info = parse_next!(input, |v| ident.parse_u32(v));
        let addr_align = parse_next!(input, |v| ident.parse_addr(v));
        let entry_size = parse_next!(input, |v| ident.parse_addr(v));

        Ok((
            Self {
                name_offset,
                typ,
                flags,
                addr,
                offset,
                size,
                link,
                info,
                addr_align,
                entry_size,
            },
            input,
        ))
    }
}

pub struct Note<'a> {
    name: &'a [u8],
    typ: u32,
    desc: &'a [u8],
}

impl<'a> Note<'a> {
    fn parse(mut input: &'a [u8], alignment: u64, ident: &FileIdentifier) -> Result<Vec<Self>> {
        let original_input_length = input.len();

        // Consume enough inputs to advance us to position that is aligned relative to
        // the start of the note buffer.
        let consume_padding = move |mut input: &'a [u8]| {
            let current_position = original_input_length - input.len();
            let pad_amount =
                common::block_size_remainder(alignment, current_position as u64) as usize;

            let padding: &[u8] = parse_next!(input, take_exact(pad_amount));
            for b in padding {
                if *b != 0 {
                    return Err(err_msg("Expected only zero padding"));
                }
            }

            Ok(((), input))
        };

        let mut out = vec![];

        while !input.is_empty() {
            // Parsing the Elf32_Nhdr/Elf64_Nhdr struct.
            let name_length = parse_next!(input, |v| ident.parse_u32(v));
            let desc_length = parse_next!(input, |v| ident.parse_u32(v));
            let typ = parse_next!(input, |v| ident.parse_u32(v));

            let name = parse_next!(input, take_exact(name_length as usize));
            let _ = parse_next!(input, consume_padding);

            let desc = parse_next!(input, take_exact(desc_length as usize));
            let _ = parse_next!(input, consume_padding);

            out.push(Self { name, typ, desc });
        }

        Ok(out)
    }
}
