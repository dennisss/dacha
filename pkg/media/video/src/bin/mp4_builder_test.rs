// This tests going from an H264 stream to an MP4 in various formats.

#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;
#[macro_use]
extern crate file;

use common::errors::*;
use file::LocalPath;
use file::LocalPathBuf;
use video::h264::*;
use video::mp4::*;

#[derive(Args)]
struct Args {
    #[arg(default = false)]
    write_goldens: bool,
}

fn quick_diff(a: &[u8], b: &[u8]) {
    const MIN_EQUAL: usize = 6;
    const MAX_EQUAL: usize = 8;
    const MAX_SEARCH_DISTANCE: usize = 4096;

    fn get_chunk(data: &[u8], i: usize) -> &[u8] {
        let n = core::cmp::min(data.len() - i, MAX_EQUAL);
        &data[i..(i + n)]
    }

    fn partial_equal(a: &[u8], b: &[u8]) -> bool {
        if a.len() < MIN_EQUAL || b.len() < MIN_EQUAL {
            return false;
        }

        let n = core::cmp::min(a.len(), b.len());
        &a[..n] == &b[..n]
    }

    let mut i = 0;
    let mut j = 0;

    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            i += 1;
            j += 1;
        } else {
            let mut i_next = i;
            let mut j_next = j;

            let mut count = 0;

            // TODO: Limit search distance of this.
            let mut found = false;
            while count < MAX_SEARCH_DISTANCE && (i_next < a.len() || j_next < b.len()) {
                if i_next < a.len() {
                    let a_i = get_chunk(a, i_next);
                    let b_j = get_chunk(b, j);

                    if partial_equal(a_i, b_j) {
                        println!("Delete {} bytes at {}", i_next - i, i);
                        i = i_next;
                        found = true;
                        break;
                    }

                    i_next += 1;
                }

                if j_next < b.len() {
                    let a_i = get_chunk(a, i);
                    let b_j = get_chunk(b, j_next);

                    if partial_equal(a_i, b_j) {
                        println!("Insert {} bytes at {}", j_next - j, i);
                        j = j_next;
                        found = true;
                        break;
                    }

                    j_next += 1;
                }

                count += 1;
            }

            // TODO: Deduplicate these messages across consecutive bytes.
            if !found {
                println!("Modify byte at index {}", i);
                i += 1;
                j += 1;
            }
        }
    }

    if i < a.len() {
        println!("Delete {} bytes at {}", a.len() - i, i);
    }

    if j < b.len() {
        println!("Insert {} bytes at {}", i, b.len() - j);
    }
}

async fn run_test(
    input_data: &[u8],
    mp4_options: MP4BuilderOptions,
    golden_path: &LocalPath,
    write_golden: bool,
) -> Result<()> {
    println!("TEST: {}", golden_path.as_str());

    let mut mp4_builder = video::mp4::MP4Builder::new(1920, 1080, 30, mp4_options)?;
    mp4_builder.append(input_data, None, true)?;

    let mut out = vec![];

    // TODO: Also test other fields in this.
    while let Some(event) = mp4_builder.consume() {
        out.extend_from_slice(&event.data);
    }

    if write_golden {
        file::write(golden_path, &out).await?;
        return Ok(());
    }

    /*
    let test_output = golden_path
        .parent()
        .unwrap()
        .join(format!("{}-test.mp4", golden_path.file_stem().unwrap()));
    file::write(&test_output, &out).await?;
    */

    if !file::exists(&golden_path).await? {
        println!("=> NO GOLDEN");
        return Ok(());
    }

    let expected_data = file::read(golden_path).await?;

    quick_diff(&expected_data, &out);

    if out != expected_data {
        println!("=> FAIL DIFF");
    } else {
        println!("=> PASS")
    }
    println!("");

    Ok(())
}

#[executor_main]
async fn main() -> Result<()> {
    /*
    quick_diff(
        &[
            0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 2, 2, 0, 0, 0, 0, 0,
        ],
        &[0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 0],
    );
    */

    /*
    quick_diff(
        &[0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        &[0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    );

    return Ok(());
    */

    let args = common::args::parse_args::<Args>()?;

    let data = file::read(file::project_path!("testdata/video/clock-5s.h264")).await?;

    // TODO: Also run all tests with user injected timestamps.

    // TODO: Test with injecting individual frames at a time.

    // TODO: Switch to using a multi-fragment segment size.
    {
        let mut options = MP4BuilderOptions::default();
        options.fragment = Some(1);
        options.max_segment_size = Some(1);
        options.independent_segments = true;

        run_test(
            &data,
            options,
            &project_path!("testdata/video/derived/clock-5s-multi-seg-independent-fragmented.mp4"),
            args.write_goldens,
        )
        .await?;
    }

    // Test:
    // - No fragmentation. Everything in one file with a complete MOOV at the end/
    {
        let mut options = MP4BuilderOptions::default();

        run_test(
            &data,
            options,
            &project_path!("testdata/video/derived/clock-5s-basic.mp4"),
            args.write_goldens,
        )
        .await?;
    }

    // Test:
    // - Basic fragmented.
    // - 1 key frame per fragment
    // - 1 file/segment containing all data.
    {
        let mut options = MP4BuilderOptions::default();
        options.fragment = Some(1);

        run_test(
            &data,
            options,
            &project_path!("testdata/video/derived/clock-5s-fragmented.mp4"),
            args.write_goldens,
        )
        .await?;
    }

    // Test:
    // - Multiple segments
    // - Each segment is fragmented.
    // TODO: Switch to using a multi-fragment segment size.
    {
        let mut options = MP4BuilderOptions::default();
        options.fragment = Some(1);
        options.max_segment_size = Some(1);

        run_test(
            &data,
            options,
            &project_path!("testdata/video/derived/clock-5s-multi-seg-fragmented.mp4"),
            args.write_goldens,
        )
        .await?;
    }

    // options.max_chunk_size = 1;

    Ok(())
}
