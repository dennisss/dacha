use std::sync::Arc;

use common::errors::*;
use crypto::cipher::BlockCipher;
use crypto::utils::xor_inplace;

use crate::mp4::{BoxClass, BoxData, SampleEncryptionBoxSubsample};

#[async_trait]
pub trait MediaDecryptor {
    async fn decrypt_media_data(
        &self,
        data: &[u8],
        iv: &[u8],
        subsamples: &[SampleEncryptionBoxSubsample],
        out: &mut Vec<u8>,
    ) -> Result<()>;
}

pub struct AesCtrClearKeyDecryptor {
    key: Vec<u8>,
}

impl AesCtrClearKeyDecryptor {
    pub fn new(key: Vec<u8>) -> Self {
        Self { key }
    }
}

#[async_trait]
impl MediaDecryptor for AesCtrClearKeyDecryptor {
    async fn decrypt_media_data(
        &self,
        mut data: &[u8],
        iv: &[u8],
        subsamples: &[SampleEncryptionBoxSubsample],
        out: &mut Vec<u8>,
    ) -> Result<()> {
        let cipher = crypto::aes::AESBlockCipher::create(&self.key)?;

        let mut block = [0u8; 16];
        block[0..8].copy_from_slice(&iv);

        for sub_sample in subsamples {
            // TODO: Check for out of bounds accesses. We currently assume one 'senc' is
            // used for fragment.

            {
                let (clear, rest) = data.split_at(sub_sample.bytes_of_clear_data as usize);
                out.extend_from_slice(clear);
                data = rest;
            }

            if sub_sample.bytes_of_encrypted_data > 0 {
                let (mut encrypted_data, rest) =
                    data.split_at(sub_sample.bytes_of_encrypted_data as usize);
                data = rest;

                for encrypted_chunk in encrypted_data.chunks(16) {
                    let mut block_mask = [0u8; 16];
                    cipher.encrypt_block(&block, &mut block_mask);

                    // Increment counter for next time.
                    *array_mut_ref![block, 8, 8] =
                        (u64::from_be_bytes(*array_mut_ref![block, 8, 8]) + 1).to_be_bytes();

                    xor_inplace(encrypted_chunk, &mut block_mask[0..encrypted_chunk.len()]);

                    out.extend_from_slice(&block_mask[0..encrypted_chunk.len()]);
                }
            }
        }

        Ok(())
    }
}

pub struct MP4Decryptor {
    media_decryptor: Arc<dyn MediaDecryptor>,
}

impl MP4Decryptor {}

pub async fn decrypt_video(data: &[u8], decryptor: &dyn MediaDecryptor) -> Result<Vec<u8>> {
    let mut out = vec![];

    let mut current_senc_box = None;

    let mut remaining = data;
    while !remaining.is_empty() {
        let (mut inst, rest) = BoxClass::parse(remaining)?;
        let raw = &remaining[..(remaining.len() - rest.len())];
        remaining = rest;

        // TODO: Also support non-fragmented videos.

        let mut changed = false;

        inst.visit_children_mut(|v| {
            if let Some(children) = v.children_mut() {
                let mut i = 0;
                while i < children.len() {
                    let mut remove_box = false;

                    // TODO: Do we need to un-next any of the data in this.
                    // TODO: Need to see is "saio" / "saiz" contain non-encrpytion related info.
                    let t = children[i].typ.as_str();
                    if t == "encv" || t == "saio" || t == "saiz" || t == "pssh" {
                        remove_box = true;
                    }

                    // Expected to be nested inside of a 'moof' box
                    if let BoxData::SampleEncryptionBox(v) = &children[i].value {
                        if current_senc_box.is_some() {
                            return Err(err_msg("Already have a SENC box"));
                        }

                        current_senc_box = Some(v.clone());

                        remove_box = true;
                    }

                    if remove_box {
                        children.remove(i);
                        continue;
                    }

                    i += 1;
                }
            }

            Ok(())
        })?;

        if inst.typ.as_str() == "mdat" && current_senc_box.is_some() {
            println!("[mdat]");

            let mut raw_data = match &inst.value {
                BoxData::Unknown(v) => &v[..],
                _ => panic!(),
            };

            let mut decoded_data = vec![];

            let senc = current_senc_box.take().unwrap();

            for sample in senc.samples {
                // sample.iv

                // sample.iv
                // sub_samples.subsamples

                let sub_samples = sample
                    .subsamples
                    .ok_or_else(|| err_msg("Expected to use subsample enc"))?;

                // TODO: Check for out of bounds accesses. We currently assume one 'senc' is
                // used for fragment.
                // (also must verify that we decoded all of the data).

                let mut input_count = 0;
                for sub_sample in &sub_samples.subsamples {
                    input_count += sub_sample.bytes_of_clear_data as usize;
                    input_count += sub_sample.bytes_of_encrypted_data as usize;
                }

                if input_count > raw_data.len() {
                    return Err(err_msg("Too many bytes in sub samples"));
                }

                let (input_segment, rest) = raw_data.split_at(input_count);
                raw_data = rest;

                decryptor
                    .decrypt_media_data(
                        input_segment,
                        &sample.iv,
                        &sub_samples.subsamples,
                        &mut decoded_data,
                    )
                    .await?;
            }

            if !raw_data.is_empty() {
                return Err(err_msg("Didn't decode all data"));
            }

            let mut serialized_mdata = vec![];
            BoxClass {
                typ: "mdat".into(),
                value: BoxData::Unknown(decoded_data),
            }
            .serialize(&mut serialized_mdata);

            out.extend_from_slice(&serialized_mdata);

            // println!("{:#?}", senc);

            //
        } else {
            println!("Passthrough {}", inst.typ.as_str());

            let mut serialized_box = vec![];
            inst.serialize(&mut serialized_box)?;
            out.extend_from_slice(&serialized_box);

            if serialized_box.len() > raw.len() {
                return Err(err_msg("Boxes got bigger across decoding"));
            }

            if serialized_box.len() < raw.len() {
                let diff = raw.len() - serialized_box.len();
                println!("Pad {}", diff);

                let freebox = BoxClass {
                    typ: "free".into(),
                    value: BoxData::Unknown(vec![0u8; diff - 8]),
                };

                let mut serialized_box = vec![];
                freebox.serialize(&mut serialized_box)?;
                out.extend_from_slice(&serialized_box);
            }
        }
    }

    Ok(out)
}
