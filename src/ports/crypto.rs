use tokio::io::AsyncRead;

pub trait StreamEncryptor: Send + Sync {
    /// Wraps an input stream with an encryption layer
    fn encrypt_stream(&self, input: Box<dyn AsyncRead + Unpin + Send>) -> Box<dyn AsyncRead + Unpin + Send>;
    
    /// Wraps an input stream with a decryption layer
    fn decrypt_stream(&self, input: Box<dyn AsyncRead + Unpin + Send>) -> Box<dyn AsyncRead + Unpin + Send>;
}
