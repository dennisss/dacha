use crypto::ccm::{BlockCipherBuffer, CCM};
use nordic_proto::nordic::NetworkConfig;

use crate::constants::*;
use crate::packet::PacketBuffer;

const CCM_LENGTH_SIZE: usize = 2;
const CCM_NONCE_SIZE: usize = 13; // 15 - CCM_LENGTH_SIZE
                                  // TODO: Dedup with the constant in the packet.rs file.
const CCM_TAG_SIZE: usize = 4;

#[derive(Debug)]
pub enum PacketEncryptionError {
    /// When encrypting or decrypting a packet, we weren't able to find a
    /// configured remote device.
    UnknownRemoteDevice = 1,
    BadMAC = 2,

    /// TODO: Check this.
    OldPacketCounter = 3,
}

pub struct PacketCipher<'a, BlockCipher> {
    packet: &'a mut PacketBuffer,
    ccm: CCM<BlockCipher, [u8; CCM_NONCE_SIZE]>,
}

impl<'a, BlockCipher: BlockCipherBuffer> PacketCipher<'a, BlockCipher> {
    // NOTE: I want to keep the network config locked for as little time as
    // possible.

    /// Creates a new instance for encrypting/decrypting a single packet. This
    /// mainly prepares the crypto keys and does not perform any cryptographic
    /// operations yet.
    ///
    /// NOTE: block_cipher_factory should be a function which generates an
    /// AES-128 block cipher implementation given a key.
    pub fn create<BlockCipherFactory: FnOnce(&[u8; LINK_KEY_SIZE]) -> BlockCipher>(
        packet: &'a mut PacketBuffer,
        network_config: &NetworkConfig,
        block_cipher_factory: BlockCipherFactory,
        remote_address: &RadioAddress,
        from_address: &RadioAddress,
    ) -> Result<Self, PacketEncryptionError> {
        let link = network_config
            .links()
            .iter()
            .find(|l| l.address() == &remote_address[..])
            .ok_or(PacketEncryptionError::UnknownRemoteDevice)?;

        let mut nonce = [0u8; CCM_NONCE_SIZE];
        nonce[0..4].copy_from_slice(&packet.counter().to_le_bytes());
        // TODO: Check the sizes of these?
        nonce[4..8].copy_from_slice(from_address);
        nonce[8..].copy_from_slice(link.iv());

        let key = *array_ref![link.key(), 0, LINK_KEY_SIZE];
        let block_cipher = block_cipher_factory(&key);

        let ccm = CCM::new(block_cipher, CCM_TAG_SIZE, CCM_LENGTH_SIZE, nonce);

        Ok(Self { packet, ccm })
    }

    pub fn encrypt(mut self) -> Result<(), PacketEncryptionError> {
        self.ccm.encrypt_inplace(self.packet.ciphertext_mut(), &[]);
        Ok(())
    }

    pub fn decrypt(mut self) -> Result<(), PacketEncryptionError> {
        if let Err(_) = self.ccm.decrypt_inplace(self.packet.ciphertext_mut(), &[]) {
            return Err(PacketEncryptionError::BadMAC);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crypto::ccm::aes::AES128BlockEncryptor;
    use protobuf::text::ParseTextProto;

    #[test]
    fn test_crypto() {
        let data = &[
            19, 232, 232, 232, 232, 3, 0, 0, 0, 64, 72, 89, 33, 13, 87, 94, 124, 249, 158, 53,
        ];

        let mut packet = PacketBuffer::new();
        packet.raw_mut()[0..data.len()].copy_from_slice(data);

        let network_config = NetworkConfig::parse_text(
            r#"
            address: "\xE7\xE7\xE7\xE7"
            last_packet_counter: 1
    
            links {
                address: "\xE8\xE8\xE8\xE8"
                key: "\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c\x0d\x0e\x0f\x10"
                iv: "\x11\x12\x13\x14\x15"
            }
            "#,
        )
        .unwrap();

        let address = *packet.remote_address();

        let packet_cipher = PacketCipher::create(
            &mut packet,
            &network_config,
            |key| AES128BlockEncryptor::new(key),
            &address,
            &address,
        )
        .unwrap();

        packet_cipher.decrypt().unwrap();

        println!("> {:02x?}", packet.ciphertext_mut());

        println!("< {:02x?}", packet.as_bytes());

        println!("{:?}", common::bytes::Bytes::from(packet.as_bytes()));

        assert_eq!(packet.data(), b"Thanks\n");
        assert_eq!(packet.counter(), 3);
    }
}
