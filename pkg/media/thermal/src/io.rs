use common::errors::*;
use sstable::record_log::*;
use media_proto::media::ImageFrameProto;
use file::LocalPath;
use protobuf::Message;
use compression::deflate::*;

pub struct VideoWriterOptions {
    pub deflate: bool,
}

pub struct VideoWriter {
    writer: RecordWriter,
    buf: Vec<u8>,
    options: VideoWriterOptions
}

impl VideoWriter {
    pub async fn create_new(path: &LocalPath, options: VideoWriterOptions) -> Result<Self> {
        let mut writer = RecordWriter::create_new(&path).await?;

        Ok(Self {
            writer,
            buf: vec![],
            options
        })
    }

    pub async fn append(&mut self, mut frame: ImageFrameProto) -> Result<()> {
        
        if self.options.deflate && !frame.deflated() {
            self.buf.clear();

            compression::transform::transform_to_vec(
                Deflater::new(DeflaterOptions::fast()),
                frame.data(),
                &mut self.buf
            )?;
            frame.set_data(&self.buf[..]);

            frame.set_deflated(true);
        }

        self.buf.clear();
        frame.serialize_to(&protobuf::SerializeOptions::default(), &mut self.buf)?;
        self.writer.append(&self.buf).await?;
        Ok(())
    }

    pub async fn flush(&mut self) -> Result<()> {
        self.writer.flush().await?;
        Ok(())
    }
}

pub struct VideoReader {
    reader: RecordReader,
    buf: Vec<u8>
}

impl VideoReader {
    pub async fn open(path: &LocalPath) -> Result<Self> {
        let mut reader = RecordReader::open(path).await?;
        Ok(Self { reader, buf: vec![] })
    }

    pub fn offset(&self) -> u64 {
        self.reader.offset()
    }

    // TODO: Parallelize the inflation across many frames.
    pub async fn next(&mut self) -> Result<Option<ImageFrameProto>> {
        let data = match self.reader.read().await? {
            Some(v) => v,
            None => return Ok(None)
        };

        let mut out = ImageFrameProto::default();
        out.parse_merge(&data)?;

        if out.deflated() {
            self.buf.clear();

            compression::transform::transform_to_vec(
                Inflater::new(),
                out.data(),
                &mut self.buf
            )?;
            out.set_data(&self.buf[..]);

            out.set_deflated(false);
        }

        Ok(Some(out))
    }

}
