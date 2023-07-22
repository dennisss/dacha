use alloc::boxed::Box;

use common::{errors::*, io::Readable};
use compression::{gzip::GzipDecoder, readable::TransformReadable};
use file::project_path;
use math::array::Array;

pub struct MNISTDataset {
    pub training_images: Array<u8>,
    pub training_labels: Array<u8>,
    pub test_images: Array<u8>,
    pub test_labels: Array<u8>,
}

impl MNISTDataset {
    // TODO: Check against stanard checksums for all the files which we erad.

    pub async fn load() -> Result<Self> {
        let training_images = read_mnist_file(
            project_path!("third_party/datasets/mnist/train-images-idx3-ubyte.gz"),
            false,
        )
        .await?;

        let training_labels = read_mnist_file(
            project_path!("third_party/datasets/mnist/train-labels-idx1-ubyte.gz"),
            true,
        )
        .await?;

        let test_images = read_mnist_file(
            project_path!("third_party/datasets/mnist/t10k-images-idx3-ubyte.gz"),
            false,
        )
        .await?;

        let test_labels = read_mnist_file(
            project_path!("third_party/datasets/mnist/t10k-labels-idx1-ubyte.gz"),
            true,
        )
        .await?;

        Ok(Self {
            training_images,
            training_labels,
            test_images,
            test_labels,
        })
    }
}

async fn read_mnist_file<P: AsRef<file::LocalPath>>(
    path: P,
    is_label_file: bool,
) -> Result<Array<u8>> {
    let mut data = vec![];
    // TODO: Auto-detect a compression decoder based on file extensions.
    TransformReadable::new(file::LocalFile::open(path)?, Box::new(GzipDecoder::new()))
        .read_to_end(&mut data)
        .await?;

    if is_label_file {
        read_mnist_label_data(&data)
    } else {
        read_mnist_image_data(&data)
    }
}

fn read_mnist_label_data(mut input: &[u8]) -> Result<Array<u8>> {
    let magic = parse_next!(input, parsing::binary::be_u32);
    if magic != 0x00000801 {
        return Err(err_msg("Incorrect magic for label file"));
    }

    let num_elements = parse_next!(input, parsing::binary::be_u32) as usize;
    let data = parse_next!(input, |i| parsing::take_exact(num_elements)(i));

    if !input.is_empty() {
        return Err(err_msg("Extra unparsed data at the end of the label file"));
    }

    Ok(Array::from_slice(data))
}

fn read_mnist_image_data(mut input: &[u8]) -> Result<Array<u8>> {
    let magic = parse_next!(input, parsing::binary::be_u32);
    if magic != 0x00000803 {
        return Err(err_msg("Incorrect magic for image file"));
    }

    let num_images = parse_next!(input, parsing::binary::be_u32) as usize;
    let num_rows = parse_next!(input, parsing::binary::be_u32) as usize;
    let num_cols = parse_next!(input, parsing::binary::be_u32) as usize;

    let data = parse_next!(input, |i| parsing::take_exact(
        num_images * num_cols * num_rows
    )(i));

    if !input.is_empty() {
        return Err(err_msg("Extra unparsed data at the end of the label file"));
    }

    Ok(Array::from_slice(data).reshape(&[num_images, num_rows, num_cols]))
}
