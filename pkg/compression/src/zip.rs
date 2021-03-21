use common::errors::*;
use parsing::*;
use parsing::binary::*;
use crypto::checksum::crc::CRC32Hasher;
use crypto::hasher::Hasher;

include!(concat!(env!("OUT_DIR"), "/src/zip.rs"));


const LOCAL_FILE_HEADER_SIG: u32 = 0x04034b50;
const ARCHIVE_EXTRA_DATA_SIG: u32 = 0x08064b50;
const CENTRAL_DIR_SIG: u32 = 0x02014b50;
const DIGITAL_SIGNATURE_SIG: u32 = 0x05054b50;
const ZIP64_END_OF_CENTRAL_DIR_SIG: u32 = 0x06064b50;
const ZIP64_END_OF_CENTRAL_DIR_LOCATOR_SIG: u32 = 0x07064b50;
const END_OF_CENTRAL_DIR_SIG: u32 = 0x06054b50;

pub fn read_zip_file(mut input: &[u8]) -> Result<()> { 
    while !input.is_empty() {
        let sig = parse_next!(input, le_u32);

        if sig == LOCAL_FILE_HEADER_SIG {
            let header = parse_next!(input, LocalFileHeader::parse);
            // println!("{:?}", header);

            let file_name = std::str::from_utf8(&header.file_name)?;
            println!("File: {}", file_name);

            if header.compressed_size > 0 {
                let file_data = {
                    // TODO: Check in range.
                    let (data, rest) = input.split_at(header.compressed_size as usize);
                    input = rest;
                    data
                };

                // TODO: If we just need to check the checksum, we could decompress in chunks instead of saving the entire file in memory.
                let mut uncompressed: Vec<u8> = vec![];

                if let CompressionMethod::Stored = header.compression_method {
                    uncompressed.extend_from_slice(file_data);
                } else if let CompressionMethod::Deflated = header.compression_method {
                    uncompressed.resize(header.uncompressed_size as usize, 0);

                    let mut inflater = crate::deflate::Inflater::new();
                    let progress = inflater.update(&mut std::io::Cursor::new(file_data), &mut uncompressed)?;
                    if /* !progress.done || progress.input_read != file_data.len() || */ progress.output_written != uncompressed.len() {
                        println!("{:?}", header);
                        println!("{:?}", progress);

                        return Err(err_msg("Too many/few deflate bytes"));
                    }

                } else {
                    return Err(format_err!("Unsupported compression method: {:?}", header.compression_method));
                }



                let mut hasher = CRC32Hasher::new();
                hasher.update(&uncompressed);
                let expected_checksum = hasher.finish_u32();

                // NOTE: This will be zero when the data descriptor is present.
                if expected_checksum != header.checksum {
                    println!("{:X} != {:X}", expected_checksum, header.checksum);
                    return Err(err_msg("Checksum mismatch"));
                }

                // TODO: Sometimes has a header of 0x08074b50.
                if header.flags & 1 << 3 != 0 {
                    let data_desc = parse_next!(input, DataDescriptor::parse);
                    println!("{:?}", data_desc);
                }
            }
        } else if sig == CENTRAL_DIR_SIG {
            let header = parse_next!(input, CentralDirectoryFileHeader::parse);
            println!("{:?}", header);

        } else if sig == END_OF_CENTRAL_DIR_SIG {
            let header = parse_next!(input, EndOfCentralDirectoryRecord::parse);
            println!("{:?}", header);
        } else {
            return Err(format_err!("Unknown ZIP section signature: 0x{:X}", sig));
        }
    }

    Ok(())
}



/*
    All little endian numbers.
*/