use base_error::*;

use crate::proto::*;

/// TODO: Auto-generate this in the binproto language. Requires support for
/// unions on enum values.
#[derive(Debug, Clone)]
pub enum BlockParams {
    Metadata(MetadataParams),
    Thumbnail(ThumbnailParams),
    GCode(GCodeParams),
}

impl BlockParams {
    pub fn parse(typ: BlockType, input: &[u8]) -> Result<(Self, &[u8])> {
        match typ {
            BlockType::GCode => GCodeParams::parse(input).map(|(v, rest)| (Self::GCode(v), rest)),
            BlockType::FileMetadata
            | BlockType::SlicerMetadata
            | BlockType::PrinterMetadata
            | BlockType::PrintMetadata => {
                MetadataParams::parse(input).map(|(v, rest)| (Self::Metadata(v), rest))
            }
            BlockType::Thumbnail => {
                ThumbnailParams::parse(input).map(|(v, rest)| (Self::Thumbnail(v), rest))
            }
            BlockType::Unknown(v) => Err(format_err!("Unsupported bgcode block type: {}", v)),
        }
    }
}
