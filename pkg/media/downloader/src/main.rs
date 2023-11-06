#[macro_use]
extern crate macros;
#[macro_use]
extern crate common;

use std::{collections::HashMap, sync::Arc, time::Duration};

use chromium_cdm::{DecryptedBlock, SessionEvent, SubsampleEntry};
use common::{bytes::Bytes, errors::*, list::Appendable};
use executor::sync::Mutex;
use file::project_path;
use http::uri::Uri;
use media_downloader::{Codec, ContentProtection, MediaTrackKind};
use reflection::ParseFrom;

/*
MPD schema:
- https://github.com/Dash-Industry-Forum/MPEG-Conformance-and-reference-source/blob/master/conformance/MPDValidator/schemas/DASH-MPD.xsd

Expend the prolog element to have 'xmlns="urn:mpeg:dash:schema:mpd:2011"'

The Spec:
- https://ptabdata.blob.core.windows.net/files/2020/IPR2020-01688/v67_EXHIBIT%201067%20-%20ISO-IEC%2023009-1%202019(E)%20-%20Info.%20Tech.%20-%20Dynamic%20Adaptive%20Streaming%20Over%20HTTP%20(DASH).pdf


Example files:

    https://cdn.bitmovin.com/content/assets/art-of-motion_drm/mpds/11331.mpd

    https://storage.googleapis.com/shaka-demo-assets/sintel-widevine/dash.mpd


    https://storage.googleapis.com/shaka-demo-assets/angel-one-widevine/dash.mpd

    https://storage.googleapis.com/shaka-demo-assets/angel-one-clearkey/dash.mpd
    https://storage.googleapis.com/shaka-demo-assets/angel-one/dash.mpd

*/

/*
    https://storage.googleapis.com/shaka-demo-assets/angel-one-widevine/dash.mpd

    https://storage.googleapis.com/shaka-demo-assets/angel-one-clearkey/dash.mpd

    => Video is at: https://storage.googleapis.com/shaka-demo-assets/angel-one-clearkey/v-0576p-1400k-libx264.mp4

    <dict>
      <key>drm.clearKeys.FEEDF00DEEDEADBEEFF0BAADF00DD00D</key>
      <string>00112233445566778899AABBCCDDEEFF</string>
    </dict>

    https://storage.googleapis.com/shaka-demo-assets/angel-one/dash.mpd

    => Video is at: https://storage.googleapis.com/shaka-demo-assets/angel-one/video_576p_1.5M_h264.mp4

    SegmentBase is the easy


    ffmpeg -decryption_key 00112233445566778899AABBCCDDEEFF -i testdata/dash/angle_one_clearkey.mp4 -max_muxing_queue_size 9999 testdata/dash/angle_one_clearkey_decrypt_ffmpeg.mp4


mediaPresentationDuration="P0Y0M0DT0H3M30.000S"

3*60 + 30 = 210s
=> Need 53 segments.


4000 duration @ 1000 timescale per segment

How to detect CENC is defined in

https://www.w3.org/TR/2014/WD-encrypted-media-20140828/cenc-format.html

*/

pub struct WidevineMediaDecryptor {
    cdm: Arc<chromium_cdm::ContentDecryptionModule>,
    key_id: Bytes,
    decrypted_block: Mutex<DecryptedBlock>,
}

#[async_trait]
impl video::mp4_protection::MediaDecryptor for WidevineMediaDecryptor {
    async fn decrypt_media_data(
        &self,
        data: &[u8],
        iv: &[u8],
        subsamples: &[video::mp4::SampleEncryptionBoxSubsample],
        out: &mut Vec<u8>,
    ) -> Result<()> {
        // NOTE: For CENC encryption, all ciphertexts are treated as one contiguous
        // chunk (ignoring the plaintext and with no padding) so we join them all for
        // decryption. Also note that the Widevine CDM sometimes gives back kNoKey
        // errors when given more than one subsample in CENC mode, so for simplicity, we
        // do all the subsample extraction for the CDM in this code.

        // TODO: Also implement ciphertext concatenation in the Clear key
        // implementation.

        let mut cipher_text = vec![];

        let mut pos = 0;
        for s in subsamples.iter() {
            pos += s.bytes_of_clear_data as usize;

            cipher_text.extend_from_slice(&data[pos..(pos + s.bytes_of_encrypted_data as usize)]);
            pos += s.bytes_of_encrypted_data as usize;
        }

        let inner_subsamples = vec![chromium_cdm::SubsampleEntry {
            clear_bytes: 0,
            cipher_bytes: cipher_text.len() as u32,
        }];

        let mut decrypted_block = self.decrypted_block.lock().await;

        // TODO: Loop / backoff if keys aren't available.
        // Firefox has no throttling logic on kNoKey?

        loop {
            let r = self
                .cdm
                .decrypt(
                    &cipher_text,
                    &self.key_id,
                    &iv,
                    &inner_subsamples,
                    &mut decrypted_block,
                )
                .await;
            if r.is_err() {
                println!("ERR");

                executor::sleep(Duration::from_millis(2000)).await?;

                // cipher_data.pop();
                // inner_subsamples[0].cipher_bytes -= 1;

                return Ok(());
            } else {
                break;
            }
        }

        let decrypted_data = &decrypted_block.get_mut()[..];
        if decrypted_data.len() != cipher_text.len() {
            return Err(err_msg("Wrong number of bytes were decrypted"));
        }

        let mut input_pos = 0;
        let mut decrypted_pos = 0;
        for s in subsamples.iter() {
            out.extend_from_slice(&data[input_pos..(input_pos + s.bytes_of_clear_data as usize)]);
            input_pos += s.bytes_of_clear_data as usize;

            out.extend_from_slice(
                &decrypted_data
                    [decrypted_pos..(decrypted_pos + s.bytes_of_encrypted_data as usize)],
            );
            input_pos += s.bytes_of_encrypted_data as usize;
            decrypted_pos += s.bytes_of_encrypted_data as usize;
        }

        Ok(())
    }
}

#[executor_main]
async fn main() -> Result<()> {
    let client = http::SimpleClient::new(http::SimpleClientOptions::default());

    let document_uri = "https://cdn.bitmovin.com/content/assets/art-of-motion_drm/mpds/11331.mpd"
        .parse::<Uri>()?;

    let license_server_uri = "https://cwip-shaka-proxy.appspot.com/no_auth".parse::<Uri>()?;

    let request = http::RequestBuilder::new()
        .method(http::Method::GET)
        .uri2(document_uri.clone())
        .build()?;

    let res = client
        .request(
            &request.head,
            Bytes::new(),
            &http::ClientRequestContext::default(),
        )
        .await?;

    let media = media_downloader::mpd::read_mpd(&res.body, Some(document_uri))?;

    let mut best_by_kind = HashMap::new();

    for track in &media.tracks {
        if let Some(lang) = &track.metadata.language {
            if lang != "en" {
                continue;
            }
        }

        if track.kind == MediaTrackKind::Unknown {
            continue;
        }

        let compression_factor = match track.metadata.codec {
            Some(Codec::AV1) => 1.4,
            Some(Codec::H265) | Some(Codec::VP9) => 1.15,
            Some(Codec::H264) | _ => 1.0,
        };

        let size = match track.kind {
            MediaTrackKind::Video => {
                let size = track
                    .metadata
                    .frame_size
                    .as_ref()
                    .ok_or_else(|| err_msg("Unknown size for video track"))?;
                (size.height * size.width)
            }
            MediaTrackKind::Audio => track
                .metadata
                .audio_bitrate
                .ok_or_else(|| err_msg("Unknown bitrate for audio track"))?,
            _ => 0,
        };

        let score = (track.metadata.bandwidth as f64) * (size as f64).log2() * compression_factor;

        if let Some((existing_score, existing_track)) = best_by_kind.get(&track.kind) {
            if *existing_score > score {
                continue;
            }
        }

        best_by_kind.insert(track.kind, (score, track));
    }

    // println!("{:#?}", media);

    for (_, track) in best_by_kind.values() {
        if track.kind != MediaTrackKind::Video {
            continue;
        }

        println!("Download: {:?}", track);

        let mut key_id = None;

        let widevine_meta = {
            let mut v = None;
            for cp in &track.content_protection {
                if let ContentProtection::Widevine(w) = cp {
                    v = Some(w);
                }
                if let ContentProtection::CENC(v) = cp {
                    key_id = v.default_key_id.clone();
                }
            }

            v.ok_or_else(|| err_msg("Didn't find widevine protection"))?
        };

        let key_id = key_id.ok_or_else(|| err_msg("No default kid present in mpd"))?;

        let mut init_data = vec![];
        for pssh in &widevine_meta.pssh {
            init_data.extend_from_slice(&pssh);
        }

        let cdm = Arc::new(chromium_cdm::ContentDecryptionModule::create().await?);

        let session_id = cdm
            .create_session(
                chromium_cdm::SessionType::kTemporary,
                chromium_cdm::InitDataType::kCenc,
                &init_data,
            )
            .await?;

        println!("CDM Session Id: {}", session_id);

        let mut all_keys = None;

        loop {
            let event = cdm.poll_event().await?;

            match &event {
                SessionEvent::SessionMessage {
                    message_type,
                    session_id,
                    message,
                } => {
                    let request = http::RequestBuilder::new()
                        .method(http::Method::POST)
                        .uri2(license_server_uri.clone())
                        .build()?;

                    let res = client
                        .request(
                            &request.head,
                            message.clone().into(),
                            &http::ClientRequestContext::default(),
                        )
                        .await?;

                    // TODO: Check status.
                    println!("Get License: {:?}", res.head.status_code);
                    println!("{:?}", res.body);

                    cdm.update_session(&session_id, &res.body).await?;
                }
                SessionEvent::SessionKeysChange {
                    session_id,
                    has_additional_usable_key,
                    keys,
                } => {
                    // TODO: Check we got the right key?
                    // TODO: Check the key status.
                    println!("Got keys: {:?}", event);

                    let mut found = false;
                    for key in keys {
                        if &key.key_id == &key_id {
                            println!("=> Got the right key!");
                            found = true;
                            break;
                        }
                    }

                    all_keys = Some(keys.clone());

                    if found {
                        break;
                    }
                }
                e @ _ => {
                    println!("Unhandled event: {:?}", e);
                }
            }
        }

        // TODO: Do this continously to keep keys in sync.
        let cdm2 = cdm.clone();
        let child_task = executor::spawn(async move {
            println!("CHECK EVENTS");
            loop {
                let event = cdm2.poll_event().await.unwrap();

                println!("EVENT: {:?}", event);
            }
        });

        executor::sleep(Duration::from_secs(2)).await?;

        // TODO: Need to verify that we are actually looking at an mp4.
        for (segment_i, segment) in track.segments.iter().enumerate() {
            // TODO: Implement the byte range.

            println!("Segment: {:?}", segment);

            let raw_data = {
                let request = http::RequestBuilder::new()
                    .method(http::Method::GET)
                    .uri(segment.url.as_str())
                    .build()?;

                let res = client
                    .request(
                        &request.head,
                        Bytes::new(),
                        &http::ClientRequestContext::default(),
                    )
                    .await?;
                assert!(res.head.status_code == http::status_code::OK);

                res.body
            };

            println!("Downloaded: {} bytes", raw_data.len());

            let decryptor = WidevineMediaDecryptor {
                cdm: cdm.clone(),
                key_id: key_id.clone(),
                decrypted_block: Mutex::new(chromium_cdm::DecryptedBlock::new()),
            };

            let decoded_video = video::mp4_protection::decrypt_video(&raw_data, &decryptor).await?;

            file::write(
                project_path!("testdata/widevine").join(format!("{}.mp4", segment_i)),
                &decoded_video,
            )
            .await?;

            // Now write to: testdata/widevine
        }
    }

    /*
    Prioritization Score:
    => score = log2(height * width) * bandwidth * compressionFactor

    where 'compressionFactor'  = 1.0 for H264
                               = 1.15 for VP9 / H265
                               = 1.4 for AV1

    Scores within 5% are considered equal and then we just prioritize based on preferred format.


    - Bandwidths within 5% are considered equal
    - VP9

      <Representation id="10" bandwidth="860874" codecs="vp09.00.30.08.00.02.02.02.00" mimeType="video/webm" sar="1:1" width="720" height="576">

      vs

      <Representation id="15" bandwidth="4495918" codecs="avc1.4d401e" mimeType="video/mp4" sar="1:1" width="720" height="576">

    Main BaseURL locations:

    In addition to the document level (the level above the MPD level), base URL information may be present
    on the following levels:
    — On MPD level in MPD.BaseURL element. For details, refer to subclause 5.3.1.2.
    — On Period level in Period.BaseURL element. For details, refer to subclause 5.3.2.2.
    — On Adaptation Set level in AdaptationSet.BaseURL element. For details, refer to subclause 5.3.3.2.
    — On Representation level in Representation.BaseURL. For details, refer to subclause 5.3.5.2.

    SegmentBase is in:
    - Period
    - AdaptionSet
    - REpresentation


    Segment usage is described in 5.3.9 of ISO/IEC 23009-1:2019


    How to tell how many segments there are is defined in section 5.3.9.5.3


    See also https://github.com/yt-dlp/yt-dlp

    */

    //

    Ok(())
}
