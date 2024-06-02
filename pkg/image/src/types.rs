// TODO: Deduplicate this stuff with the individual decoders code (though we
// also don't want to link to the full decoders for the purposes of sniffing
// types).

pub enum ImageType {
    JPEG,
    PNG,
    QOI,
    BMP,
}

impl ImageType {
    pub fn from_header(header: &[u8]) -> Option<ImageType> {
        if header.starts_with(b"qoif") {
            return Some(ImageType::QOI);
        }

        if header.starts_with(b"\xff\xd8\xff") {
            return Some(ImageType::JPEG);
        }

        if header.starts_with(b"\x89\x50\x4e\x47\x0d\x0a\x1a\x0a") {
            return Some(ImageType::PNG);
        }

        for magic in ["BM", "BA", "CI", "CP", "IC", "PT"] {
            if header.starts_with(magic.as_bytes()) {
                return Some(ImageType::BMP);
            }
        }

        None
    }

    pub fn from_extension(extension: &str) -> Option<Self> {
        todo!()
    }

    pub fn widely_supported(&self) -> bool {
        match self {
            ImageType::JPEG | ImageType::PNG => true,
            _ => false,
        }
    }
}
