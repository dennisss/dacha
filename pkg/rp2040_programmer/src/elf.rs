use common::async_std::fs;
use common::async_std::path::Path;
use common::bytes::Bytes;
use common::errors::*;

use parsing::binary::*;
use parsing::*;

pub struct ELF {
    pub file: Vec<u8>,

    pub program_headers: Vec<ProgramHeader>,

    pub section_headers: Vec<SectionHeader>,
}

impl ELF {
    pub async fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::open_impl(path.as_ref()).await
    }

    async fn open_impl(path: &Path) -> Result<Self> {
        let file = fs::read(path).await?;
        let header = FileHeader::parse(&file)?.0;

        println!("{:?}", header);

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

                if h.typ == 0x03 {
                    let strings = Bytes::from(
                        &file[(h.offset as usize)..(h.offset as usize + h.size as usize)],
                    );
                    println!("{:?}", strings);
                }

                section_headers.push(h);
            }
        }

        println!("{:#x?}", program_headers);
        println!("{:#x?}", section_headers);

        Ok(Self {
            file,
            program_headers,
            section_headers,
        })
    }
}

/*
To program it, we will basically go through all of the
*/

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

#[derive(Debug)]
pub struct ProgramHeader {
    pub typ: u32,
    /// Only present in 64-bit.
    pub flags: u32,
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
        let mut flags = 0;
        if ident.format == Format::I64 {
            flags = parse_next!(input, |v| ident.parse_u32(v));
        }
        let offset = parse_next!(input, |v| ident.parse_addr(v));
        let vaddr = parse_next!(input, |v| ident.parse_addr(v));
        let paddr = parse_next!(input, |v| ident.parse_addr(v));
        let file_size = parse_next!(input, |v| ident.parse_addr(v));
        let mem_size = parse_next!(input, |v| ident.parse_addr(v));
        if ident.format == Format::I32 {
            flags = parse_next!(input, |v| ident.parse_u32(v));
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
