use std::{
    num::{NonZeroU32, NonZeroU8},
    time::Duration,
};

use mediasoup::{
    prelude::*,
    producer::DirectProducer,
};
use opus::{Application as OpusApplication, Channels as OpusChannels, Encoder as OpusEncoder};
use thiserror::Error;
use tokio::time::sleep;
use uuid::Uuid;
use wiab_core::agent::SpeechClip;

const RTP_SAMPLE_RATE_HZ: u32 = 48_000;
const RTP_CHANNELS: u8 = 2;
const RTP_FRAME_DURATION_MS: u64 = 20;
const RTP_SAMPLES_PER_CHANNEL_PER_FRAME: usize =
    (RTP_SAMPLE_RATE_HZ as usize * RTP_FRAME_DURATION_MS as usize) / 1000;
const RTP_PAYLOAD_TYPE: u8 = 111;
const OPUS_PACKET_BUFFER_BYTES: usize = 4_096;

#[derive(Debug, Error)]
pub enum AgentAudioTransportError {
    #[error("speech clip mime type '{0}' is not supported for WebRTC injection")]
    UnsupportedMimeType(String),
    #[error("speech clip is not a valid PCM16 WAV file: {0}")]
    InvalidWav(String),
    #[error("speech clip does not contain any audio samples")]
    EmptyClip,
    #[error("failed to create mediasoup direct producer: {0}")]
    ProducerCreate(String),
    #[error("mediasoup direct producer was not created as a direct producer")]
    NotDirectProducer,
    #[error("failed to initialize Opus encoder: {0}")]
    EncoderInit(String),
    #[error("failed to encode Opus frame: {0}")]
    Encoder(String),
    #[error("failed to send RTP packet into mediasoup: {0}")]
    ProducerSend(String),
}

#[derive(Debug)]
pub struct AgentAudioSource {
    producer_id: String,
    producer: DirectProducer,
    ssrc: u32,
    sequence_number: u16,
    timestamp: u32,
}

impl AgentAudioSource {
    pub async fn new(direct_transport: &DirectTransport) -> Result<Self, AgentAudioTransportError> {
        let seed = Uuid::new_v4();
        let seed_bytes = seed.as_bytes();
        let ssrc =
            u32::from_be_bytes([seed_bytes[0], seed_bytes[1], seed_bytes[2], seed_bytes[3]]);
        let sequence_number = u16::from_be_bytes([seed_bytes[4], seed_bytes[5]]);
        let timestamp =
            u32::from_be_bytes([seed_bytes[6], seed_bytes[7], seed_bytes[8], seed_bytes[9]]);

        let producer = direct_transport
            .produce(ProducerOptions::new(MediaKind::Audio, agent_audio_rtp_parameters(ssrc)))
            .await
            .map_err(|err| AgentAudioTransportError::ProducerCreate(err.to_string()))?;
        let Producer::Direct(producer) = producer else {
            return Err(AgentAudioTransportError::NotDirectProducer);
        };

        let producer_id = Producer::from(producer.clone()).id().to_string();

        Ok(Self {
            producer_id,
            producer,
            ssrc,
            sequence_number,
            timestamp,
        })
    }

    pub fn producer_id(&self) -> String {
        self.producer_id.clone()
    }

    pub async fn play_clip(
        &mut self,
        clip: &SpeechClip,
        initial_attach_delay: Option<Duration>,
    ) -> Result<(), AgentAudioTransportError> {
        let packets = self.packetize_clip(clip)?;
        if packets.is_empty() {
            return Ok(());
        }

        if let Some(delay) = initial_attach_delay {
            sleep(delay).await;
        }

        let packet_count = packets.len();
        for (index, packet) in packets.into_iter().enumerate() {
            self.producer
                .send(packet)
                .map_err(|err| AgentAudioTransportError::ProducerSend(err.to_string()))?;

            if index + 1 < packet_count {
                sleep(Duration::from_millis(RTP_FRAME_DURATION_MS)).await;
            }
        }

        Ok(())
    }

    fn packetize_clip(
        &mut self,
        clip: &SpeechClip,
    ) -> Result<Vec<Vec<u8>>, AgentAudioTransportError> {
        if clip.mime_type != "audio/wav" {
            return Err(AgentAudioTransportError::UnsupportedMimeType(
                clip.mime_type.clone(),
            ));
        }

        let wav = decode_pcm16_wav(&clip.audio_bytes)?;
        let mono_samples = downmix_to_mono(&wav.samples, wav.channels);
        if mono_samples.is_empty() {
            return Err(AgentAudioTransportError::EmptyClip);
        }

        let stereo_samples = resample_to_stereo(&mono_samples, wav.sample_rate);
        let mut encoder = OpusEncoder::new(
            RTP_SAMPLE_RATE_HZ,
            OpusChannels::Stereo,
            OpusApplication::Voip,
        )
        .map_err(|err| AgentAudioTransportError::EncoderInit(err.to_string()))?;

        let frame_sample_count = RTP_SAMPLES_PER_CHANNEL_PER_FRAME * RTP_CHANNELS as usize;
        let frame_count = stereo_samples.len().div_ceil(frame_sample_count);
        let mut packets = Vec::with_capacity(frame_count);
        let mut opus_buffer = vec![0_u8; OPUS_PACKET_BUFFER_BYTES];

        for frame_index in 0..frame_count {
            let start = frame_index * frame_sample_count;
            let end = ((frame_index + 1) * frame_sample_count).min(stereo_samples.len());
            let mut frame = vec![0_i16; frame_sample_count];
            frame[..end.saturating_sub(start)].copy_from_slice(&stereo_samples[start..end]);

            let encoded_len = encoder
                .encode(&frame, &mut opus_buffer)
                .map_err(|err| AgentAudioTransportError::Encoder(err.to_string()))?;

            packets.push(build_rtp_packet(
                self.sequence_number,
                self.timestamp,
                frame_index == 0,
                self.ssrc,
                &opus_buffer[..encoded_len],
            ));
            self.sequence_number = self.sequence_number.wrapping_add(1);
            self.timestamp = self
                .timestamp
                .wrapping_add(RTP_SAMPLES_PER_CHANNEL_PER_FRAME as u32);
        }

        Ok(packets)
    }
}

#[derive(Debug)]
struct WavPcm16 {
    sample_rate: u32,
    channels: u16,
    samples: Vec<i16>,
}

fn agent_audio_rtp_parameters(ssrc: u32) -> RtpParameters {
    RtpParameters {
        mid: None,
        codecs: vec![RtpCodecParameters::Audio {
            mime_type: MimeTypeAudio::Opus,
            payload_type: RTP_PAYLOAD_TYPE,
            clock_rate: NonZeroU32::new(RTP_SAMPLE_RATE_HZ).expect("constant sample rate"),
            channels: NonZeroU8::new(RTP_CHANNELS).expect("constant channel count"),
            parameters: RtpCodecParametersParameters::default(),
            rtcp_feedback: vec![],
        }],
        header_extensions: Vec::new(),
        encodings: vec![RtpEncodingParameters {
            ssrc: Some(ssrc),
            ..RtpEncodingParameters::default()
        }],
        rtcp: RtcpParameters::default(),
    }
}

fn build_rtp_packet(
    sequence_number: u16,
    timestamp: u32,
    marker: bool,
    ssrc: u32,
    payload: &[u8],
) -> Vec<u8> {
    let mut packet = Vec::with_capacity(12 + payload.len());
    packet.push(0x80);
    packet.push(if marker {
        0x80 | RTP_PAYLOAD_TYPE
    } else {
        RTP_PAYLOAD_TYPE
    });
    packet.extend_from_slice(&sequence_number.to_be_bytes());
    packet.extend_from_slice(&timestamp.to_be_bytes());
    packet.extend_from_slice(&ssrc.to_be_bytes());
    packet.extend_from_slice(payload);
    packet
}

fn decode_pcm16_wav(bytes: &[u8]) -> Result<WavPcm16, AgentAudioTransportError> {
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(AgentAudioTransportError::InvalidWav(
            "missing RIFF/WAVE header".to_owned(),
        ));
    }

    let mut offset = 12usize;
    let mut channels = None;
    let mut sample_rate = None;
    let mut data_chunk = None;

    while offset + 8 <= bytes.len() {
        let chunk_id = &bytes[offset..offset + 4];
        let chunk_size = u32::from_le_bytes([
            bytes[offset + 4],
            bytes[offset + 5],
            bytes[offset + 6],
            bytes[offset + 7],
        ]) as usize;
        let data_start = offset + 8;
        let data_end = data_start.saturating_add(chunk_size);
        if data_end > bytes.len() {
            return Err(AgentAudioTransportError::InvalidWav(
                "chunk exceeds file bounds".to_owned(),
            ));
        }

        match chunk_id {
            b"fmt " => {
                if chunk_size < 16 {
                    return Err(AgentAudioTransportError::InvalidWav(
                        "fmt chunk is too small".to_owned(),
                    ));
                }

                let audio_format = u16::from_le_bytes([bytes[data_start], bytes[data_start + 1]]);
                if audio_format != 1 {
                    return Err(AgentAudioTransportError::InvalidWav(format!(
                        "unsupported WAV format {audio_format}, expected PCM"
                    )));
                }

                let wav_channels =
                    u16::from_le_bytes([bytes[data_start + 2], bytes[data_start + 3]]);
                if !(1..=2).contains(&wav_channels) {
                    return Err(AgentAudioTransportError::InvalidWav(format!(
                        "unsupported channel count {wav_channels}"
                    )));
                }

                let wav_sample_rate = u32::from_le_bytes([
                    bytes[data_start + 4],
                    bytes[data_start + 5],
                    bytes[data_start + 6],
                    bytes[data_start + 7],
                ]);
                let bits_per_sample =
                    u16::from_le_bytes([bytes[data_start + 14], bytes[data_start + 15]]);
                if bits_per_sample != 16 {
                    return Err(AgentAudioTransportError::InvalidWav(format!(
                        "unsupported bits per sample {bits_per_sample}, expected 16"
                    )));
                }

                channels = Some(wav_channels);
                sample_rate = Some(wav_sample_rate);
            }
            b"data" => {
                data_chunk = Some(&bytes[data_start..data_end]);
            }
            _ => {}
        }

        offset = data_end + (chunk_size % 2);
    }

    let channels = channels.ok_or_else(|| {
        AgentAudioTransportError::InvalidWav("missing fmt chunk".to_owned())
    })?;
    let sample_rate = sample_rate.ok_or_else(|| {
        AgentAudioTransportError::InvalidWav("missing sample rate".to_owned())
    })?;
    let data = data_chunk.ok_or_else(|| {
        AgentAudioTransportError::InvalidWav("missing data chunk".to_owned())
    })?;

    if data.len() % 2 != 0 {
        return Err(AgentAudioTransportError::InvalidWav(
            "PCM16 data chunk length is not even".to_owned(),
        ));
    }

    let samples = data
        .chunks_exact(2)
        .map(|sample| i16::from_le_bytes([sample[0], sample[1]]))
        .collect::<Vec<_>>();

    Ok(WavPcm16 {
        sample_rate,
        channels,
        samples,
    })
}

fn downmix_to_mono(samples: &[i16], channels: u16) -> Vec<f32> {
    if channels == 1 {
        return samples.iter().map(|sample| *sample as f32).collect();
    }

    samples
        .chunks_exact(channels as usize)
        .map(|frame| {
            let sum = frame.iter().map(|sample| i32::from(*sample)).sum::<i32>();
            sum as f32 / channels as f32
        })
        .collect()
}

fn resample_to_stereo(mono_samples: &[f32], input_rate_hz: u32) -> Vec<i16> {
    if mono_samples.is_empty() {
        return Vec::new();
    }

    let output_frame_count = ((mono_samples.len() as u64) * RTP_SAMPLE_RATE_HZ as u64)
        .div_ceil(input_rate_hz as u64) as usize;
    let mut stereo_samples = Vec::with_capacity(output_frame_count * RTP_CHANNELS as usize);

    for output_index in 0..output_frame_count {
        let source_position =
            (output_index as f64) * (input_rate_hz as f64) / (RTP_SAMPLE_RATE_HZ as f64);
        let lower_index = source_position.floor() as usize;
        let upper_index = (lower_index + 1).min(mono_samples.len() - 1);
        let fraction = (source_position - lower_index as f64) as f32;
        let lower = mono_samples[lower_index];
        let upper = mono_samples[upper_index];
        let interpolated = lower + ((upper - lower) * fraction);
        let sample = interpolated.round().clamp(i16::MIN as f32, i16::MAX as f32) as i16;
        stereo_samples.push(sample);
        stereo_samples.push(sample);
    }

    stereo_samples
}
