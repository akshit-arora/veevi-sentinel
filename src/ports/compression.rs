use tokio::io::AsyncRead;

pub trait StreamCompressor: Send + Sync {
    /// Wraps an input stream with a compression layer
    fn compress_stream(&self, input: Box<dyn AsyncRead + Unpin + Send>) -> Box<dyn AsyncRead + Unpin + Send>;
    
    /// Wraps an input stream with a decompression layer
    fn decompress_stream(&self, input: Box<dyn AsyncRead + Unpin + Send>) -> Box<dyn AsyncRead + Unpin + Send>;
}
