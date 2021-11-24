extern crate byteorder;
extern crate compression;

use std::fs::File;
use std::io::{Read, Seek};

use common::async_std::path::{Path, PathBuf};
use common::bits::*;
use common::errors::*;
use compression::deflate::*;
use compression::gzip::*;
use compression::huffman::*;
use compression::tar::{FileMetadata, FileMetadataMask};
use compression::zlib::*;

// https://zlib.net/feldspar.html
// TODO: Blah b[D=5, L=18]!
// Should becode as 'Blah blah blah blah blah!'

// TODO: Implement zlib format https://www.ietf.org/rfc/rfc1950.txt

// TODO: Maintain a histogram of characters in the block to determine when to
// cut the block?

async fn run() -> Result<()> {
    let mut tar = compression::tar::Reader::open(
        "/home/dennis/workspace/dacha/built/pkg/home_hub/bundle.tar",
    )
    .await?;

    tar.extract_files(&PathBuf::from(std::env::current_dir()?).join("/tmp/bundle"))
        .await?;

    // while let Some(entry) = tar.read_entry().await? {
    //     println!("{:#?}", entry);

    //     let data = tar.read_data(&entry).await;
    //     println!("{:?}", data);
    // }

    /*
    let mut tar = compression::tar::Writer::open("testdata/tar/built.tar").await?;

    let options = compression::tar::AppendFileOption {
        root_dir: std::env::current_dir()?,
        mask: FileMetadataMask {}
    };

    tar.append_file(&options.root_dir.join("data"), &options).await?;

    tar.finish().await?;
    */

    Ok(())
}

fn main() -> Result<()> {
    common::async_std::task::block_on(run())

    // let mut window = MatchingWindow::new();
    // let chars = b"Blah blah blah blah blah!";

    // let mut i = 0;
    // while i < chars.len() {
    // 	let mut n = 1;
    // 	if let Some(m) = window.find_match(&chars[i..]) {
    // 		println!("{:?}", m);
    // 		n = m.length;
    // 	} else {
    // 		println!("Literal: {}", chars[i] as char);
    // 	}

    // 	window.extend_from_slice(&chars[i..(i+n)]);
    // 	i += n;
    // }

    // assert_eq!(i, chars.len());

    /*
        let header = Header {
            compression_method: CompressionMethod::Deflate,
            is_text: true,
            mtime: 10,
            extra_flags: 2, // < Max compression (slowest algorithm)
            os: GZIP_UNIX_OS,
            extra_field: None,
            filename: Some("lorem_ipsum.txt".into()),
            comment: None,
            header_validated: false
        };

        let mut infile = File::open("testdata/lorem_ipsum.txt")?;
        let mut indata = Vec::new();
        infile.read_to_end(&mut indata)?;


        let mut outfile = File::create("testdata/out/lorem_ipsum.txt.test.gz")?;
        write_gzip(header, &indata, &mut outfile)?;

        return Ok(());
    */

    // let data = std::fs::read("/home/dennis/Downloads/dmg_sound.zip")?;
    // compression::zip::read_zip_file(&data)?;

    /*
    ///
    let mut f = File::open("testdata/out/lorem_ipsum.txt.test.gz")?;
    let gz = read_gzip(&mut f)?;
    println!("{:?}", gz);
    */

    // TODO: Assert that we now at the end of the file after reading.
}
