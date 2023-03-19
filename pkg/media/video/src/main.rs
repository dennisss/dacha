#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;
#[macro_use]
extern crate parsing;

use common::errors::*;
use video::h264::*;
use video::mp4::*;

/*
MP4 most info in ISO/IEC 14496-12:2008(E)
(also -14)
*/

/*
Most of the format is defined in ISO 14496-12

MP4 parsing guidance: https://dev.to/alfg/a-quick-dive-into-mp4-57fo#:~:text=The%20MP4%20byte%20structure%20is,also%20known%20as%20a%20FourCC.

"The definitions of boxes are given in the syntax description language (SDL) defined in MPEG-4 (see reference
in clause 2). Comments in the code fragments in this specification indicate informative material."

BigEndian


aligned(8) class Box (unsigned int(32) boxtype,
    optional unsigned int(8)[16] extended_type) {
    unsigned int(32) size;
    unsigned int(32) type = boxtype;
    if (size==1) {
        unsigned int(64) largesize;
    } else if (size==0) {
        // box extends to end of file
    }
    if (boxtype=='uuid') {
        unsigned int(8)[16] usertype = extended_type;
    }
}




// Container: File
// Mandatory: Yes
// Quantity: Exactly one
aligned(8) class MovieBox extends Box(‘moov’){
}


# Container: Movie Box ('moov')
# Mandatory: Yes
# Quantity: One or more
aligned(8) class TrackBox extends Box('trak') {
}


# Container: Track Box (‘trak’)
# Mandatory: Yes
# Quantity: Exactly one
aligned(8) class MediaBox extends Box(‘mdia’) {
}


# Container: Media Box (‘mdia’)
# Mandatory: Yes
# Quantity: Exactly one
aligned(8) class MediaInformationBox extends Box(‘minf’) {
}


# Container: Sample Table Box (‘stbl’)
# Mandatory: Yes
# Quantity: Exactly one
aligned(8) class SampleTableBox extends Box(‘stbl’) {
}

# Container: Track Box (‘trak’)
# Mandatory: No
# Quantity: Zero or one
aligned(8) class EditBox extends Box(‘edts’) {
}

# Container: Media Information Box (‘minf’) or Meta Box (‘meta’)
# Mandatory: Yes (required within ‘minf’ box) and No (optional within ‘meta’ box)
# Quantity: Exactly one
aligned(8) class DataInformationBox extends Box(‘dinf’) {
}

aligned(8) class UserDataBox extends Box(‘udta’) {
}
*/

/*
First we parse a



Buffer  {}

a string is a null terminated UTF-8 string.

Decode as a string (requires having an underlying buffer)
    - May also want to know the size (e.g. if we should make it )

 */

fn print_boxes(data: &[u8], indent: &str) -> Result<()> {
    let mut i = 0;

    let inner_indent = format!("{}  ", indent);

    while i < data.len() {
        let (box_header, rest) = BoxHeader::parse(&data[i..])?;
        let box_contents = &data[(i + BoxHeader::size_of())..(i + box_header.length as usize)];

        println!(
            "{}{} : {}",
            indent,
            box_header.typ.as_str(),
            box_header.length,
        );

        // TODO: When serializing these, we should align each box to 8 byte offsets.

        match box_header.typ.as_str() {
            "free" => {
                println!("{}FREE", inner_indent);
            }
            "moov" | "trak" | "mdia" | "minf" | "stbl" | "edts" | "dinf" | "udta" => {
                print_boxes(box_contents, &inner_indent)?;
            }

            "dref" => {
                let (box_body, rest) = DataReferenceBox::parse(&box_contents)?;
                assert!(rest.is_empty());
                println!("{}{:?}", inner_indent, box_body);

                // TODO: Verify number of boxes against box_body.entry_count.
                print_boxes(&box_body.boxes, &inner_indent)?;
            }

            "stsd" => {
                let (box_body, rest) = SampleDescriptionBox::parse(&box_contents)?;
                assert!(rest.is_empty());
                println!("{}{:?}", inner_indent, box_body);

                // TODO: Verify number of boxes against box_body.entry_count.
                print_boxes(&box_body.entry_data, &inner_indent)?;
            }
            "avc1" => {
                let (box_body, rest) = VisualSampleEntry::parse(&box_contents)?;
                // assert!(rest.is_empty());
                println!("{}{:?}", inner_indent, box_body);

                print_boxes(rest, &inner_indent)?;
            }

            "mdat" => {
                // NOTE: THe format will change depending on the coded.

                let mut input = &box_contents[..];

                while !input.is_empty() {
                    let len = parse_next!(input, parsing::binary::be_u32);
                    input = &input[(len as usize)..];

                    println!("{}NALU Size: {}", inner_indent, len);
                }
            }
            _ => {
                let (data, rest) = BoxData::parse(&box_contents, &box_header.typ)?;
                assert!(rest.is_empty());

                println!("{}{:?}", inner_indent, data);
            }
        }

        i += box_header.length as usize;
    }

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    {
        let data = file::read("image.h264").await?;

        let mut iter = H264BitStreamIterator::new(&data);

        while let Some(nalu) = iter.next() {
            println!("NALU: Size {}", nalu.len());

            let (header, rest) = NALUnitHeader::parse(&nalu[..])?;
            println!("{:?}", header);

            if header.nal_unit_type == NALUnitType::PPS || header.nal_unit_type == NALUnitType::SPS
            {
                println!("{:x?}", &nalu[..]);
            }
        }
    }

    {
        let data = file::read("image.mp4").await?;

        print_boxes(&data, "")?;
    }

    Ok(())
}
