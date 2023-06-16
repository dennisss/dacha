// This binary computes values for parity blocks for test files. These are
// stored in the code repository as golden values. For the purposes of backwards
// compatibility, we must ensure that they don't change over time.

#[macro_use]
extern crate macros;

use common::errors::*;
use file::project_path;
use storage::erasure::reed_solomon::*;

#[derive(Args)]
struct Args {
    #[arg(default = false)]
    overwrite: bool,
}

struct ReedSolomonSegmentEncoder {
    block_size: usize,
    n: usize,
    m: usize,
    encoder: VandermondReedSolomonEncoder,
}

impl ReedSolomonSegmentEncoder {
    /// For up to N data blocks (which are each at most block_size in length),
    /// computes M block_size length blocks of parity code data.
    ///
    /// The input blocks are interpreted with zero padding up to N * block_size
    /// bytes.
    fn encode_segment(&self, data_blocks: &[&[u8]]) -> Vec<Vec<u8>> {
        // TODO: Arena allocate these so that they stay within
        let mut code_blocks = vec![];
        for i in 0..self.m {
            code_blocks.push(vec![]);
        }

        let mut words = vec![0u8; self.n];

        for block_i in 0..self.block_size {
            // Gather one byte from each block.
            for i in 0..self.n {
                let w = {
                    if i < data_blocks.len() {
                        data_blocks[i].get(block_i).cloned().unwrap_or(0)
                    } else {
                        0
                    }
                };

                words[i] = w;
            }

            // Compute all parity bytes for this chain of bytes.
            for i in 0..self.m {
                code_blocks[i].push(self.encoder.compute_parity_word(&words, i));
            }
        }

        code_blocks
    }
}

#[executor_main]
async fn main() -> Result<()> {
    let args = common::args::parse_args::<Args>()?;

    let block_size = 128;

    let empty_block = vec![0u8; block_size];

    let n_m = &[(3, 2), (6, 3), (4, 2), (10, 4)];

    let files = &[
        ("lorem_ipsum", project_path!("testdata/lorem_ipsum.txt")), // 3703 bytes
        ("random_4096", project_path!("testdata/random/random_4096")),
    ];

    let output_path = project_path!("pkg/storage/goldens");

    for (file_key, file) in files {
        let data = file::read(file).await?;

        for (n, m) in n_m.iter().cloned() {
            let encoder = VandermondReedSolomonEncoder::new(
                n,
                m,
                VandermondReedSolomonEncoder::STANDARD_POLY,
            );

            let encoder = ReedSolomonSegmentEncoder {
                n,
                m,
                block_size,
                encoder,
            };

            let mut data_files = vec![];
            for i in 0..n {
                data_files.push(vec![]);
            }

            let mut code_files = vec![];
            for i in 0..m {
                code_files.push(vec![]);
            }

            let mut i = 0;
            let segment_size = n * block_size;

            while i < data.len() {
                let mut j = i + segment_size;
                j = j.min(data.len());

                let segment = &data[i..j];

                let data_blocks = segment.chunks(block_size).collect::<Vec<_>>();
                for i in 0..n {
                    data_files[i].extend_from_slice(&data_blocks[i][..]);
                }

                let code_blocks = encoder.encode_segment(&data_blocks);
                for i in 0..m {
                    code_files[i].extend_from_slice(&code_blocks[i][..]);
                }

                i += segment_size;
            }

            for i in 0..m {
                let path =
                    output_path.join(format!("{}-rs-{}-{}-{}-c{}", file_key, n, m, block_size, i));
                file::write(&path, &code_files[i]).await?;
            }

            // TODO: Test deleting up to 'm' blocks and recovering the data.
        }
    }

    Ok(())
}
