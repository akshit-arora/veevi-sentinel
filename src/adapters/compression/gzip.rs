use crate::ports::compression::StreamCompressor;
use async_compression::tokio::bufread::{GzipDecoder, GzipEncoder};
use tokio::io::{AsyncRead, BufReader};

pub struct GzipAdapter;

impl StreamCompressor for GzipAdapter {
    fn compress_stream(
        &self,
        input: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Box<dyn AsyncRead + Unpin + Send> {
        let reader = BufReader::new(input);
        let encoder = GzipEncoder::new(reader);
        Box::new(encoder)
    }

    fn decompress_stream(
        &self,
        input: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Box<dyn AsyncRead + Unpin + Send> {
        let reader = BufReader::new(input);
        let decoder = GzipDecoder::new(reader);
        Box::new(decoder)
    }
}
