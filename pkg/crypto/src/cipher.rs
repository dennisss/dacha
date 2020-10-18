pub trait BlockCipher {
    fn block_size(&self) -> usize;

    fn encrypt_block(&self, block: &[u8], out: &mut [u8]);

    fn decrypt_block(&self, block: &[u8], out: &mut [u8]);
}
