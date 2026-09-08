use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHasher};
use chacha20::ChaCha20;
use chacha20::cipher::{KeyIvInit, StreamCipher};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

pub struct EncryptedStream<S> {
    inner: S,
    // Symmetric ciphers use the same key. We keep separate instances
    // to maintain the stream state (nonce) independently for Read and Write.
    cipher_rx: ChaCha20,
    cipher_tx: ChaCha20,
}

impl<S: AsyncRead + AsyncWrite + Unpin> EncryptedStream<S> {
    pub fn new(inner: S, challenge: &str, username: &str, pass_hash: &Vec<u8>) -> Self {
        // 1. Derive a key from the challenge, username, and password hash
        let mut key_material = Vec::new();
        key_material.extend_from_slice(username.as_bytes());
        key_material.extend_from_slice(pass_hash);

        let argon2 = Argon2::default();
        let key = argon2
            .hash_password(
                &key_material,
                &SaltString::encode_b64(challenge.as_bytes())
                    .unwrap_or_else(|_| panic!("Failed to encode salt")),
            )
            .expect("Key derivation failed")
            .hash
            .unwrap();

        let key_slice = &key.as_bytes()[..32];

        // TODO: use unique IVs (Nonces) for RX and TX.
        let iv = [0u8; 12];

        Self {
            inner,
            cipher_rx: ChaCha20::new_from_slices(&key_slice, &iv)
                .expect("Invalid key or IV length"),
            cipher_tx: ChaCha20::new_from_slices(&key_slice, &iv)
                .expect("Invalid key or IV length"),
        }
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for EncryptedStream<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let filled_before = buf.filled().len();

        // Poll the underlying stream
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);

        if let Poll::Ready(Ok(())) = result {
            let filled_after = buf.filled().len();
            let newly_read = filled_after - filled_before;

            if newly_read > 0 {
                // Decrypt the bytes in place by XORing with the keystream
                let data = &mut buf.filled_mut()[filled_before..filled_after];
                self.cipher_rx.apply_keystream(data);
            }
        }
        result
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for EncryptedStream<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // Encrypt the outgoing buffer
        let mut encrypted_buf = buf.to_vec();
        self.cipher_tx.apply_keystream(&mut encrypted_buf);

        // Write the ciphertext to the underlying socket
        Pin::new(&mut self.inner).poll_write(cx, &encrypted_buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
