use crate::ports::crypto::StreamEncryptor;
use anyhow::{anyhow, Result};
use chacha20poly1305::aead::{
    stream::{DecryptorBE32, EncryptorBE32},
    OsRng,
};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305};
use rand::RngCore;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

const CHUNK_SIZE: usize = 1024 * 1024; // 1 MB chunks

pub struct ChaChaAdapter {
    key: [u8; 32],
}

impl ChaChaAdapter {
    pub fn new(key: &[u8]) -> Result<Self> {
        if key.len() != 32 {
            return Err(anyhow!("XChaCha20-Poly1305 key must be 32 bytes"));
        }
        let mut k = [0u8; 32];
        k.copy_from_slice(key);
        Ok(Self { key: k })
    }
}

impl StreamEncryptor for ChaChaAdapter {
    fn encrypt_stream(
        &self,
        mut input: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Box<dyn AsyncRead + Unpin + Send> {
        let (read_half, mut write_half) = tokio::io::duplex(CHUNK_SIZE * 2);
        let key = self.key.clone().into();

        tokio::spawn(async move {
            let mut nonce = [0u8; 19];
            OsRng.fill_bytes(&mut nonce);
            
            // Write the nonce unencrypted at the very beginning of the stream
            if write_half.write_all(&nonce).await.is_err() {
                return;
            }

            let aead = XChaCha20Poly1305::new(&key);
            let mut encryptor = EncryptorBE32::from_aead(aead, &nonce.into());

            let mut buffer = vec![0u8; CHUNK_SIZE];
            loop {
                match input.read(&mut buffer).await {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        let ciphertext = encryptor
                            .encrypt_next(&buffer[..n])
                            .expect("Encryption failure");
                        
                        // Write 4-byte length prefix
                        let len_bytes = (ciphertext.len() as u32).to_le_bytes();
                        if write_half.write_all(&len_bytes).await.is_err() {
                            break;
                        }

                        if write_half.write_all(&ciphertext).await.is_err() {
                            break; // Reader closed the stream
                        }
                    }
                    Err(_) => break, // Read error
                }
            }

            // Write the final authenticated block
            let final_ciphertext = encryptor.encrypt_last(b"" as &[u8]).expect("Encryption failure");
            let len_bytes = (final_ciphertext.len() as u32).to_le_bytes();
            let _ = write_half.write_all(&len_bytes).await;
            let _ = write_half.write_all(&final_ciphertext).await;
        });

        Box::new(read_half)
    }

    fn decrypt_stream(
        &self,
        mut input: Box<dyn AsyncRead + Unpin + Send>,
    ) -> Box<dyn AsyncRead + Unpin + Send> {
        let (read_half, mut write_half) = tokio::io::duplex(CHUNK_SIZE * 2);
        let key = self.key.clone().into();

        tokio::spawn(async move {
            let mut nonce = [0u8; 19];
            // Read the unencrypted nonce from the beginning
            if input.read_exact(&mut nonce).await.is_err() {
                return;
            }

            let aead = XChaCha20Poly1305::new(&key);
            let mut decryptor = DecryptorBE32::from_aead(aead, &nonce.into());

            loop {
                let mut len_buf = [0u8; 4];
                if input.read_exact(&mut len_buf).await.is_err() {
                    break;
                }
                let chunk_len = u32::from_le_bytes(len_buf) as usize;
                
                let mut buffer = vec![0u8; chunk_len];
                if input.read_exact(&mut buffer).await.is_err() {
                    break;
                }

                // If this is the final 16-byte block from encrypt_last(b"")
                if chunk_len == 16 {
                    if let Ok(plaintext) = decryptor.decrypt_last(&buffer[..]) {
                        let _ = write_half.write_all(&plaintext).await;
                    }
                    break;
                } else {
                    if let Ok(plaintext) = decryptor.decrypt_next(&buffer[..]) {
                        if write_half.write_all(&plaintext).await.is_err() {
                            break;
                        }
                    } else {
                        break; // MAC validation failed or corruption
                    }
                }
            }
        });

        Box::new(read_half)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;
    
    #[tokio::test]
    async fn test_encryption_decryption_pipeline() {
        let key = b"12345678901234567890123456789012";
        let adapter = ChaChaAdapter::new(key).unwrap();
        
        let original_data = b"Hello World! This is a test of the zero knowledge stream!";
        let original_stream = &original_data[..];
        
        let encrypted_stream = adapter.encrypt_stream(Box::new(original_stream));
        let mut decrypted_stream = adapter.decrypt_stream(encrypted_stream);
        
        let mut output = Vec::new();
        decrypted_stream.read_to_end(&mut output).await.unwrap();
        
        assert_eq!(output, original_data);
    }
}
