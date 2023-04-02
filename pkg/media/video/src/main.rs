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


*/

fn print_boxes(data: &[u8], indent: &str) -> Result<()> {
    let mut i = 0;

    let inner_indent = format!("{}  ", indent);

    let mut remaining = data;
    while !remaining.is_empty() {
        let (inst, rest) = Box::parse(remaining)?;
        let raw = &remaining[..(remaining.len() - rest.len())];
        remaining = rest;

        let mut serialized = vec![];
        inst.serialize(&mut serialized)?;
        assert_eq!(&serialized[..], raw);

        match inst.typ.as_str() {
            "mdat" => {
                let box_contents = match &inst.value {
                    BoxData::Unknown(v) => &v[..],
                    _ => panic!(),
                };

                // NOTE: THe format will change depending on the coded.

                let mut input = &box_contents[..];

                while !input.is_empty() {
                    let len = parse_next!(input, parsing::binary::be_u32);
                    input = &input[(len as usize)..];

                    println!("{}NALU Size: {}", inner_indent, len);
                }
            }
            _ => {
                println!("{:#?}", inst);
            }
        }
    }

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    {
        let data = file::read("image.h264").await?;

        // TODO: Read the width/height from the H264 SPS.
        let mut builder = MP4Builder::new(1296, 972, 30)?;
        builder.append(&data)?;
        let mp4_data = builder.finish()?;

        file::write("generated.mp4", mp4_data).await?;

        return Ok(());

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
