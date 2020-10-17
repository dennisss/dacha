use common::bytes::Bytes;
use common::async_std::fs::File;
use common::async_std::io::ReadExt;
use common::errors::*;
use parsing::cstruct::parse_cstruct_be;
use parsing::*;

use crate::raster::canvas::{Canvas, Path, PathBuilder, SubPath};
use common::io::StreamableExt;
use image::{Color, Colorspace, Image};
use math::matrix::{Matrix3f, Vector2f, Vector2i, Vector3f, Vector3u};
use minifb::MouseMode;
use std::f32::consts::PI;

pub mod vm;

#[derive(Debug)]
pub struct FontFileHeader {
    offset_table: OffsetTable,
    directory_table: Vec<DirectoryTableEntry>,
}

impl FontFileHeader {
    parser!(parse<&[u8], Self> => seq!(c => {
        let offset_table = c.next(OffsetTable::parse)?;

        let mut directory_table = vec![];
        for i in 0..offset_table.num_tables {
            directory_table.push(c.next(DirectoryTableEntry::parse)?);
        }

        Ok(Self { offset_table, directory_table })
    }));
}

#[derive(Default, Reflect, Debug)]
pub struct OffsetTable {
    version: u32,
    num_tables: u16,
    search_range: u16,
    entry_selector: u16,
    range_shift: u16,
}

impl OffsetTable {
    fn parse(input: &[u8]) -> ParseResult<Self, &[u8]> {
        let mut v = Self::default();
        let rest = parse_cstruct_be(input, &mut v)?;
        Ok((v, rest))
    }
}

#[derive(Default, Reflect, Debug)]
pub struct DirectoryTableEntry {
    /// NOTE: Bytes will always be between 0x20-0x7E (ASCII printable)
    tag: [u8; 4],

    checksum: u32,
    offset: u32,
    length: u32,
}

impl DirectoryTableEntry {
    fn parse(input: &[u8]) -> ParseResult<Self, &[u8]> {
        let mut v = Self::default();
        let rest = parse_cstruct_be(input, &mut v)?;
        Ok((v, rest))
    }
}

#[derive(Default, Reflect, Debug)]
pub struct FontHeaderTable {
    major_version: u16,
    minor_version: u16,
    font_revision: u32, // TODO: This is 'FIXED' dtype.
    checksum_adjustment: u32,
    magic_number: u32,
    // TODO: Must parse these.
    flags: u16,
    units_per_em: u16,
    created: i64,  // LONGDATETIME
    modified: i64, // LONGDATETIME
    x_min: i16,
    y_min: i16,
    x_max: i16,
    y_max: i16,
    mac_style: u16,
    lowest_rec_ppem: u16,
    font_direction_hint: i16,
    index_to_loc_format: i16,
    glyph_data_format: i16,
}

impl FontHeaderTable {
    fn parse(input: &[u8]) -> ParseResult<Self, &[u8]> {
        let mut v = Self::default();
        let rest = parse_cstruct_be(input, &mut v)?;
        Ok((v, rest))
    }
}

pub struct FontHeaderFlags(u16);

impl FontHeaderFlags {
    // TODO: Assert that this is true.
    fn baseline_at_zero(&self) -> bool {
        self.0 & 0b1 != 0
    }

    fn lsb_at_zero(&self) -> bool {
        self.0 & 0b10 != 0
    }
}

#[derive(Debug)]
pub struct MaxProfileTable {
    base: MaxProfileTable05,
    v10: Option<MaxProfileTable10>,
}

impl MaxProfileTable {
    fn parse(mut input: &[u8]) -> Result<Self> {
        let mut base = MaxProfileTable05::default();
        input = parse_cstruct_be(input, &mut base)?;

        let mut v10 = None;

        if base.version == 0x00005000 {
            // No extra fields
        } else if base.version == 0x00010000 {
            let mut v = MaxProfileTable10::default();
            input = parse_cstruct_be(input, &mut v)?;
            v10 = Some(v);
        } else {
            return Err(err_msg("Unknown maxp version"));
        }

        if !input.is_empty() {
            return Err(err_msg("Extra bytes at end of table"));
        }

        Ok(Self { base, v10 })
    }
}

/// 'maxp' table
/// Base table for version 0.5
#[derive(Default, Reflect, Debug)]
pub struct MaxProfileTable05 {
    version: u32, // FIXED
    num_glyphs: u16,
}

#[derive(Default, Reflect, Debug)]
pub struct MaxProfileTable10 {
    max_points: u16,
    max_contours: u16,
    max_composite_points: u16,
    max_composite_contours: u16,
    max_zones: u16,
    max_twilight_points: u16,
    max_storage: u16,
    max_function_defs: u16,
    max_instruction_defs: u16,
    max_stack_elements: u16,
    max_size_of_instructions: u16,
    max_component_elements: u16,
    max_component_depth: u16,
}

#[derive(Debug)]
pub enum Glyph {
    // TODO: Also include the bbox as specified in the file.
    Simple(SimpleGlyph),
    Composite,
}

#[derive(Default, Reflect, Debug)]
pub struct GlyphHeader {
    num_contours: i16,

    // TODO: Verify these
    x_min: i16,
    y_min: i16,
    x_max: i16,
    y_max: i16,
}

impl GlyphHeader {
    fn parse(input: &[u8]) -> ParseResult<Self, &[u8]> {
        let mut v = Self::default();
        let rest = parse_cstruct_be(input, &mut v)?;
        Ok((v, rest))
    }
}

#[derive(Debug)]
pub struct ContourPoint {
    x: i16,
    y: i16,
    on_curve: bool,
}

impl ContourPoint {
    fn to_vector(&self) -> Vector2i {
        Vector2i::from_slice(&[self.x as isize, self.y as isize])
    }
}

#[derive(Debug)]
pub struct SimpleGlyph {
    instructions: Vec<u8>,
    contours: Vec<Vec<ContourPoint>>,
}

impl SimpleGlyph {
    parser!(parse<&[u8], Self> => seq!(c => {
        if c.next(length)? == 0 {
            return Ok(Self { instructions: vec![], contours: vec![] });
        }

        let header = c.next(GlyphHeader::parse)?;
        if header.num_contours < 0 {
            return Err(err_msg("Composite not supported"));
        }

        let mut end_indices = vec![];
        for i in 0..header.num_contours {
            end_indices.push(c.next(parsing::binary::be_u16)? as usize);
        }

        let inst_len = c.next(parsing::binary::be_u16)? as usize;
        let instructions = c.next(take_exact(inst_len))?.to_vec();

        let num_points = if end_indices.is_empty() { 0 } else { end_indices.last().unwrap() + 1 };

        let mut flags = vec![];
        while flags.len() < num_points {
            let f = SimpleGlyphFlags(c.next(parsing::binary::be_u8)?);
            let multiples =
                if f.repeating() {
                    let mut additional = c.next(parsing::binary::be_u8)? as usize;
                    additional + 1
                } else {
                    1
                };

            for i in 0..multiples {
                flags.push(f);
            }
        }

        if flags.len() != num_points {
            return Err(err_msg("Too many flags"));
        }

        let mut x: Vec<i16> = vec![];
        for f in &flags {
            let val: i16 =
                if f.x_short() {
                    let v = c.next(parsing::binary::be_u8)? as i16;
                    if f.x_same_or_positive_short() {
                        v
                    } else {
                        -1 * v
                    }
                } else {
                    if f.x_same_or_positive_short() {
                        x.push(x.last().cloned().unwrap_or(0));
                        continue;
                    } else {
                        c.next(parsing::binary::be_i16)?
                    }
                };

            x.push(val + x.last().cloned().unwrap_or(0));
        }

        let mut y: Vec<i16> = vec![];
        for f in &flags {
            let val =
                if f.y_short() {
                    let v = c.next(parsing::binary::be_u8)? as i16;
                    if f.y_same_or_positive_short() {
                        v
                    } else {
                        -1 * v
                    }
                } else {
                    if f.y_same_or_positive_short() {
                        y.push(y.last().cloned().unwrap_or(0));
                        continue;
                    } else {
                        c.next(parsing::binary::be_i16)?
                    }
                };

            y.push(val + y.last().cloned().unwrap_or(0));
        }

        let mut contours = vec![];

        let mut i = 0;
        for end_i in end_indices {
            if i > end_i {
                return Err(err_msg("Non-monotonic contour end index"));
            }

            let mut pts = vec![];
            while i <= end_i {
                pts.push(ContourPoint {
                    x: x[i],
                    y: y[i],
                    on_curve: flags[i].on_curve()
                });
                i += 1;
            }

            contours.push(pts);
        }

        Ok(Self { instructions, contours })
    }));
}

#[derive(Clone, Copy)]
pub struct SimpleGlyphFlags(u8);

impl SimpleGlyphFlags {
    fn on_curve(&self) -> bool {
        self.0 & 0x01 != 0
    }
    fn x_short(&self) -> bool {
        self.0 & 0x02 != 0
    }
    fn y_short(&self) -> bool {
        self.0 & 0x04 != 0
    }
    fn repeating(&self) -> bool {
        self.0 & 0x08 != 0
    }
    fn x_same_or_positive_short(&self) -> bool {
        self.0 & 0x10 != 0
    }
    fn y_same_or_positive_short(&self) -> bool {
        self.0 & 0x20 != 0
    }

    // TODO: Check that upper reserved bit is always zero
}

#[derive(Debug)]
pub struct CharacterMappingTable {
    header: CharacterMappingTableHeader,
    subtables: Vec<CharacterMappingSubTable>,
}

impl CharacterMappingTable {
    fn parse(input: &[u8]) -> ParseResult<Self, &[u8]> {
        let (header, rest) = CharacterMappingTableHeader::parse(input)?;

        // TODO: Verify that all bytes in this table are accounted for.
        let mut subtables = vec![];
        for encoding in &header.encodings {
            let table = CharacterMappingSubTable::parse(&input[(encoding.offset as usize)..])?.0;
            subtables.push(table);
        }

        // TODO: The 'input' returned here doesn't make sense. Consider just returning
        // the value.
        Ok((Self { header, subtables }, input))
    }
}

#[derive(Debug)]
pub struct CharacterMappingTableHeader {
    version: u16,
    encodings: Vec<CharacterEncodingEntry>,
}

impl CharacterMappingTableHeader {
    parser!(parse<&[u8], Self> => seq!(c => {
        let version = c.next(parsing::binary::be_u16)?;  // TODO: Check must be zero.
        let num_tables = c.next(parsing::binary::be_u16)? as usize;

        let mut encodings = vec![];
        for i in 0..num_tables {
            encodings.push(c.next(CharacterEncodingEntry::parse)?);
        }

        Ok(Self { version, encodings })
    }));
}

#[derive(Default, Reflect, Debug)]
pub struct CharacterEncodingEntry {
    platform_id: u16,
    encoding_id: u16,
    offset: u32,
}

impl CharacterEncodingEntry {
    fn parse(input: &[u8]) -> ParseResult<Self, &[u8]> {
        let mut v = Self::default();
        let rest = parse_cstruct_be(input, &mut v)?;
        Ok((v, rest))
    }
}

#[derive(Debug)]
pub enum CharacterMappingSubTable {
    Segments(CharacterSegmentMapping),
}

impl CharacterMappingSubTable {
    fn lookup(&self, code: u16) -> Result<u16> {
        match self {
            CharacterMappingSubTable::Segments(table) => table.lookup(code),
        }
    }

    fn lookup_all(&self) -> Result<Vec<(u16, u16)>> {
        match self {
            CharacterMappingSubTable::Segments(table) => table.lookup_all(),
        }
    }

    parser!(parse<&[u8], Self> => seq!(c => {
        let format = c.next(parsing::binary::be_u16)?;
        if format == 4 {
            let table = c.next(CharacterSegmentMapping::parse)?;
            Ok(CharacterMappingSubTable::Segments(table))
        } else {
            Err(err_msg("Unknown cmap subtable format"))
        }
    }));
}

#[derive(Debug)]
struct HorizontalMetricsTable {
    /// Should be one record per glyph.
    records: Vec<HorizontalMetricRecord>,
}

impl HorizontalMetricsTable {
    fn parser<'a>(num_glyphs: u16, num_hmetrics: u16) -> impl Parser<Self, &'a [u8]> {
        seq!(c => {
            if num_hmetrics > num_glyphs {
                return Err(err_msg("More h-metrics than glyphs"));
            }

            let mut records = vec![];
            for i in 0..num_hmetrics {
                let advance_width = c.next(parsing::binary::be_u16)?;
                let left_side_bearing = c.next(parsing::binary::be_i16)?;
                records.push(HorizontalMetricRecord { advance_width, left_side_bearing });
            }

            if records.is_empty() && records.len() < (num_glyphs as usize) {
                return Err(err_msg("Must have at least one h-metric in this case"));
            }

            for i in 0..(num_glyphs - num_hmetrics) {
                let advance_width = records.last().unwrap().advance_width;
                let left_side_bearing = c.next(parsing::binary::be_i16)?;
                records.push(HorizontalMetricRecord { advance_width, left_side_bearing });
            }

            Ok(Self { records })
        })
    }
}

#[derive(Debug)]
pub struct HorizontalMetricRecord {
    advance_width: u16,

    // TODO: In some cases, we need to ensure that this is equal to 0 or xmin in the glyphs
    left_side_bearing: i16,
}

fn length(input: &[u8]) -> ParseResult<usize, &[u8]> {
    Ok((input.len(), input))
}

/// NOTE: The end_codes, start_codes, id_delta, and id_range_offset tables
/// should all be the same size (equal to the number of segments)
#[derive(Debug)]
pub struct CharacterSegmentMapping {
    header: CharacterSegmentMappingHeader,
    end_codes: Vec<u16>,
    start_codes: Vec<u16>,
    id_delta: Vec<i16>,
    /// If non-zero, is an offset in bytes in the original font file from the
    /// position of the offset to an entry in glyph_id_array to use as the base
    /// id.
    id_range_offset: Vec<u16>,
    glyph_id_array: Vec<u16>,
}

impl CharacterSegmentMapping {
    fn lookup(&self, code: u16) -> Result<u16> {
        // TODO: We can unwrap this if we know the last segment ends at 0xFFFF
        let segment_idx = common::algorithms::lower_bound(&self.end_codes, &code)
            .ok_or(err_msg("Code larger than all segments"))?;

        if code < self.start_codes[segment_idx] {
            // Unknown glyph.
            return Ok(0);
        }

        self.lookup_in_segment(code, segment_idx)
    }

    fn lookup_in_segment(&self, code: u16, segment_idx: usize) -> Result<u16> {
        let mut glyph_id = if self.id_range_offset[segment_idx] != 0 {
            // TODO: Check that all this stuff is in range.

            let offset = (((self.id_range_offset[segment_idx] / 2) as usize)
                + ((code - self.start_codes[segment_idx]) as usize))
                .checked_sub(self.id_range_offset.len() - segment_idx)
                .ok_or(err_msg("Offset did not exceed id_range_offset table"))?;

            *self
                .glyph_id_array
                .get(offset)
                .ok_or(err_msg("Offset out of range of glyph id array"))?
        } else {
            code
        };

        if glyph_id != 0 {
            glyph_id = glyph_id.wrapping_add(self.id_delta[segment_idx] as u16);
        }

        Ok(glyph_id)
    }

    fn lookup_all(&self) -> Result<Vec<(u16, u16)>> {
        let mut pairs = vec![];
        for segment_idx in 0..self.start_codes.len() {
            for code in self.start_codes[segment_idx]..(self.end_codes[segment_idx] + 1) {
                pairs.push((code, self.lookup_in_segment(code, segment_idx)?));
            }
        }

        Ok(pairs)
    }

    parser!(parse<&[u8], Self> => seq!(c => {
        let length1 = c.next(length)?;

        let header = c.next(CharacterSegmentMappingHeader::parse)?;
        let seg_count = header.seg_count_x2 / 2;  // TODO: Check divisible by 2

        // TODO: Verify that final start/end codes are 0xFFFF

        // TODO: Check that these are in sorted ascending order.
        let mut end_codes = vec![];
        for i in 0..seg_count {
            end_codes.push(c.next(parsing::binary::be_u16)?);
        }

        let reserved_pad = c.next(parsing::binary::be_u16)?;
        if reserved_pad != 0 {
            return Err(err_msg("Expected zero padding"));
        }

        let mut start_codes = vec![];
        for i in 0..seg_count {
            start_codes.push(c.next(parsing::binary::be_u16)?);
        }

        let mut id_delta = vec![];
        for i in 0..seg_count {
            id_delta.push(c.next(parsing::binary::be_i16)?);
        }

        // TODO: Check that all of these are divisible by 2 bytes.
        let mut id_range_offset = vec![];
        for i in 0..seg_count {
            let off = c.next(parsing::binary::be_u16)?;
            if off % 2 != 0 {
                return Err(err_msg("Offset not divisible by u16's"));
            }
            id_range_offset.push(off);
        }

        let length2 = c.next(length)?;
        // 2 is for the 'format' identifier parsed earlier.
        // TODO: Need to check for going negative to not crash.
        let remaining_len = (header.length as usize) - 2 - (length1 - length2);

        let glyph_id_slice = c.next(take_exact(remaining_len))?;

        let glyph_id_array = complete(many(parsing::binary::be_u16))(glyph_id_slice)?.0;

        // TODO: Adding id_delta is modulo u16 range (wrapping)

        Ok(Self {
            header,
            end_codes, start_codes, id_delta, id_range_offset,
            glyph_id_array
        })
    }));
}

#[derive(Default, Reflect, Debug)]
pub struct CharacterSegmentMappingHeader {
    length: u16,
    language: u16,
    seg_count_x2: u16,
    search_range: u16,
    entry_selector: u16,
    range_shift: u16,
}

impl CharacterSegmentMappingHeader {
    fn parse(input: &[u8]) -> ParseResult<Self, &[u8]> {
        let mut v = Self::default();
        let rest = parse_cstruct_be(input, &mut v)?;
        Ok((v, rest))
    }
}

#[derive(Default, Reflect, Debug)]
pub struct HorizontalHeaderTable {
    major_version: u16,
    minor_version: u16,
    ascender: i16,
    descender: i16,
    line_gap: i16,
    advance_width_max: u16,
    min_left_side_bearing: i16,
    min_right_side_bearing: i16,
    x_max_extent: i16,
    caret_slope_rise: i16,
    caret_slope_run: i16,
    caret_offset: i16,
    reserved1: i16,
    reserved2: i16,
    reserved3: i16,
    reserved4: i16,
    metric_data_format: i16,
    num_hmetrics: u16,
}

impl HorizontalHeaderTable {
    fn parse(input: &[u8]) -> ParseResult<Self, &[u8]> {
        let mut v = Self::default();
        let rest = parse_cstruct_be(input, &mut v)?;
        Ok((v, rest))
    }
}

pub struct OpenTypeFont {
    head: FontHeaderTable,
    maxp: MaxProfileTable,

    /// Byte offset of each glyph in 'glyf'.
    /// Extracted from the 'loca' table.
    index_to_loc: Vec<usize>,

    cmap: CharacterMappingTable,

    hhea: HorizontalHeaderTable,
    hmtx: HorizontalMetricsTable,

    glyf: Bytes,
}

impl OpenTypeFont {
    pub async fn open(path: &str) -> Result<Self> {
        let mut f = File::open(path).await?;

        let mut buf = vec![];
        f.read_to_end(&mut buf).await?;

        let (v, rest) = FontFileHeader::parse(&buf)?;

        let mut ranges = vec![];
        ranges.push((0, buf.len() - rest.len())); // Header

        let mut tables = std::collections::HashMap::new();

        for entry in &v.directory_table {
            let tag = std::str::from_utf8(&entry.tag)?;
            println!("{}", tag);

            if entry.offset % 4 != 0 {
                return Err(err_msg("Table not aligned to word boundaries"));
            }

            let padded_len = 4 * common::ceil_div(entry.length as usize, 4);

            let start_off = entry.offset as usize;
            let end_off = start_off + (entry.length as usize);
            let padded_off = (entry.offset as usize) + padded_len;

            for padding in &buf[end_off..padded_off] {
                if *padding != 0 {
                    return Err(err_msg("Non-zero padding"));
                }
            }

            let expected_sum = (&buf[start_off..padded_off])
                .chunks(4)
                .fold(0, |sum: u32, word| {
                    let (word, _) = parsing::binary::be_u32(word).unwrap();
                    sum.wrapping_add(word)
                });

            // TODO: Implement head checksum.
            if tag != "head" && expected_sum != entry.checksum {
                println!("{:x} {:x}", expected_sum, entry.checksum);
                return Err(err_msg("Checksum does not match"));
            }

            if padded_off > buf.len() {
                return Err(err_msg("Table beyond end of file"));
            }

            tables.insert(entry.tag, (start_off, end_off));

            ranges.push((start_off, padded_off));
        }

        ranges.sort();

        // Verify that all parts of the file are accounted for.
        {
            let mut last_offset = 0;
            for (s, e) in ranges {
                if s == last_offset {
                    last_offset = e;
                } else {
                    return Err(err_msg("Unaccounted for range of file"));
                }
            }

            if last_offset != buf.len() {
                return Err(err_msg("Extra bytes at end of file"));
            }
        }

        // Required tables: 'cmap', 'head', 'glyf', 'loca', 'name', 'maxp'

        let head = {
            let (start, end) = *tables.get(b"head").ok_or(err_msg("Missing head table"))?;
            FontHeaderTable::parse(&buf[start..end])?.0
        };

        let maxp = {
            let (start, end) = *tables.get(b"maxp").ok_or(err_msg("Missing maxp table"))?;
            MaxProfileTable::parse(&buf[start..end])?
        };

        let index_to_loc = {
            let (start, end) = *tables.get(b"loca").ok_or(err_msg("Missing loca table"))?;
            let data = &buf[start..end];
            if head.index_to_loc_format == 0 {
                complete(many(parsing::binary::be_u16))(data)?
                    .0
                    .into_iter()
                    .map(|v| 2 * v as usize)
                    .collect::<Vec<_>>()
            } else if head.index_to_loc_format == 1 {
                complete(many(parsing::binary::be_u32))(data)?
                    .0
                    .into_iter()
                    .map(|v| v as usize)
                    .collect::<Vec<_>>()
            } else {
                return Err(err_msg("Unknown loca format"));
            }
        };

        // TODO: Check that last glyph offset equals the size of the table.
        if index_to_loc.len() != (maxp.base.num_glyphs as usize) + 1 {
            return Err(err_msg("loca and maxp mismatch"));
        }

        let cmap = {
            let (start, end) = *tables.get(b"cmap").ok_or(err_msg("Missing cmap table"))?;
            CharacterMappingTable::parse(&buf[start..end])?.0
        };

        let hhea = {
            let (start, end) = *tables.get(b"hhea").ok_or(err_msg("Missing hhea table"))?;
            complete(HorizontalHeaderTable::parse)(&buf[start..end])?.0
        };

        let hmtx = {
            let (start, end) = *tables.get(b"hmtx").ok_or(err_msg("Missing hmtx table"))?;
            complete(HorizontalMetricsTable::parser(
                maxp.base.num_glyphs,
                hhea.num_hmetrics,
            ))(&buf[start..end])?
            .0
        };

        let glyf = {
            let (start, end) = *tables.get(b"glyf").ok_or(err_msg("Missing glyf table"))?;
            &buf[start..end]
        };

        // TODO: Check for any unknown tables.

        let head_flags = FontHeaderFlags(head.flags);
        if !head_flags.baseline_at_zero() || head_flags.lsb_at_zero() {}

        Ok(Self {
            head,
            maxp,
            index_to_loc,
            cmap,
            hhea,
            hmtx,
            glyf: glyf.into(),
        })
    }

    // TODO: Precompute paths for all glpyhs.
    pub fn char_glyph(&self, code: u16) -> Result<(SimpleGlyph, &HorizontalMetricRecord)> {
        let glyph_id = self.cmap.subtables[0].lookup(code)? as usize;

        // TODO: Check that index_to_loc[0] is always zero and the last one is
        // consistent with the end of the table. TODO: 53.
        let glyph_start = self.index_to_loc[glyph_id];
        let glyph_end = self.index_to_loc[glyph_id + 1];

        // TODO: Ensure that the entire data is parsed (need to think about padding
        // too).
        //        println!("{} {}", glyph_start, glyph_end);
        let (g, _) = SimpleGlyph::parse(&self.glyf[glyph_start..glyph_end])?;
        let metrics = &self.hmtx.records[glyph_id];

        Ok((g, metrics))
    }
}

fn draw_glyph(canvas: &mut Canvas, g: &SimpleGlyph, color: &Color) -> Result<()> {
    if g.contours.is_empty() {
        return Ok(());
    }

    let mut path_builder = PathBuilder::new();

    for contour in &g.contours {
        // TODO: Check that there are at least two points in the contour. Otherwise it
        // is invalid.

        if !contour.is_empty() {
            if !contour[0].on_curve {
                return Err(err_msg("Expected first point to be on curve"));
            }

            path_builder.move_to(contour[0].to_vector().cast());
        }

        let mut i = 1;
        while i < contour.len() {
            let p = contour[i].to_vector();
            let p_on_curve = contour[i].on_curve;
            i += 1;

            if p_on_curve {
                path_builder.line_to(p.cast());
            } else {
                let mut curve = vec![p.cast()];
                while i < contour.len() && !contour[i].on_curve {
                    curve.push(contour[i].to_vector().cast());
                    i += 1;
                }

                // TODO: Check if this is correct.
                if i == contour.len() {
                    curve.push(contour[0].to_vector().cast());
                } else {
                    curve.push(contour[i].to_vector().cast());
                    i += 1;
                }

                path_builder.curve_to(&curve);
            }
        }

        path_builder.close();
    }

    let path = path_builder.build();
    canvas.fill_path(&path, color)?;

    Ok(())
}

enum TextAlign {
    Left,
    Center,
    Right,
}

enum VerticalAlign {
    Top,
    Baseline,
    Bottom,
}

// TODO: https://github.com/rust-lang/rust/issues/63033

/*
    To draw a glyph:

    - Assume line height is
    - Flip and scale to font size
    - Need current (x,y) at the baseline
    - Translate glyph to have LSB at (x,y)
    - Increment x by advance width

    - For now, pick the baseline

    - Baseline located at hhea.ascender + (hhea.line_gap / 2)

    GPU acceleration:
    - Take path and triangulate.
    - Discard triangles that are outside the polygon
    - Main constraint while doing triangulation is that the path

Simple algorithm:
- Pick a point, sweep radially out until we find the closest edge.
- Form a triangle and add the new edges to the next list.
- Each triangle adds at least two new edges so 2n * n = O(n^2)

- Fast segment-point
    - Maintain line normal and
*/

// need to support a paint

pub enum Paint {
    Solid(Color),
}

pub async fn open_font() -> Result<()> {
    // TODO: Verify the encoding/platform and that there is exactly one subtable.
    //    println!(
    //        "{} {}",
    //        hhea.ascender + hhea.descender + hhea.line_gap,
    //        head.units_per_em
    //    );
    //
    //    return Ok(());

    let font = OpenTypeFont::open("testdata/noto-sans.ttf").await?;

    const HEIGHT: usize = 650;
    const WIDTH: usize = 800;
    const SCALE: usize = 4;

    let mut canvas = Canvas::create(HEIGHT, WIDTH, SCALE);

    draw_loop(canvas, |canvas, window| {
        canvas.drawing_buffer.clear_white();

        // Key font actions:
        // - Distance above/below baseline of one line (does not change based on text)
        // - given some text, the width of that text.
        // -

        let text = b"Hello world $_%!";

        let mut x = 10.0;
        let mut y = 300.0;

        let font_size = 30.0; // 14px font.

        let color = Color::from_slice_with_shape(3, 1, &[0, 0, 0]);

        for c in text {
            let (g, metrics) = font.char_glyph(*c as u16)?;

            canvas.save();

            let scale = font_size / (font.head.units_per_em as f32);

            canvas.translate(-1.0 * (metrics.left_side_bearing as f32), 0.0);
            canvas.scale(scale, -1.0 * scale);
            canvas.translate(x, y);

            draw_glyph(canvas, &g, &color)?;

            canvas.restore()?;

            x += (metrics.advance_width as f32) * scale;
        }

        {
            let red = Color::from_slice_with_shape(3, 1, &[255, 0, 0]);
            let mut pb = PathBuilder::new();
            pb.move_to(Vector2f::from_slice(&[100.0, 400.0]));
            pb.line_to(Vector2f::from_slice(&[400.0, 100.0]));
            pb.line_to(Vector2f::from_slice(&[550.0, 300.0]));
            pb.line_to(Vector2f::from_slice(&[100.0, 400.0]));
            canvas.stroke_path(&pb.build(), 5.0, &red)?;
        }

        if let Some((mx, my)) = window.get_mouse_pos(MouseMode::Discard) {
            let mut builder = PathBuilder::new();
            builder.ellipse(
                Vector2f::from_slice(&[mx, my]),
                Vector2f::from_slice(&[10.0, 10.0]),
                0.0,
                2.0 * PI,
            );

            canvas.fill_path(&builder.build(), &color)?;
        }

        Ok(())
    })
    .await?;

    //    crate::raster::bresenham_line(
    //        &mut img,
    //        Vector2i::from_slice(&[0, 301]),
    //        Vector2i::from_slice(&[400, 301]),
    //        &Color::from_slice_with_shape(4, 1, &[255, 0, 0, 1]),
    //    );
    //
    //    crate::raster::bresenham_line(
    //        &mut img,
    //        Vector2i::from_slice(&[0, 200]),
    //        Vector2i::from_slice(&[400, 200]),
    //        &Color::from_slice_with_shape(4, 1, &[255, 0, 0, 1]),
    //    );

    Ok(())
}

async fn draw_loop<F: FnMut(&mut Canvas, &minifb::Window) -> Result<()>>(
    mut canvas: Canvas,
    mut f: F,
) -> Result<()> {
    let mut window_options = minifb::WindowOptions::default();

    let mut window = minifb::Window::new(
        "Image",
        canvas.display_buffer.width(),
        canvas.display_buffer.height(),
        window_options,
    )
    .unwrap();

    // 30 FPS
    window.limit_update_rate(Some(std::time::Duration::from_micros(33333)));

    let mut data = vec![0u32; canvas.display_buffer.array.data.len()];

    while window.is_open() {
        if window.is_key_pressed(minifb::Key::Escape, minifb::KeyRepeat::No) {
            break;
        }

        let start_time = std::time::Instant::now();

        f(&mut canvas, &window)?;

        canvas
            .drawing_buffer
            .downsample(&mut canvas.display_buffer)
            .await;

        for (i, color) in canvas
            .display_buffer
            .array
            .flat()
            .chunks_exact(canvas.display_buffer.channels())
            .enumerate()
        {
            data[i] = ((color[0] as u32) << 16) | ((color[1] as u32) << 8) | (color[2] as u32);
        }

        let end_time = std::time::Instant::now();

        println!("frame: {}ms", (end_time - start_time).as_millis());

        // TODO: Only update once as the image will be static.
        window.update_with_buffer(
            &data,
            canvas.display_buffer.width(),
            canvas.display_buffer.height(),
        )?;
    }

    Ok(())
}
