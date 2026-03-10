pub const OPUS_SAMPLE_RATE_HZ: usize = 48_000;
pub const TRANSCRIPT_SAMPLE_RATE_HZ: usize = 16_000;
pub const MAX_OPUS_FRAME_SAMPLES_PER_CHANNEL: usize = 5_760;
pub const ENDPOINT_SILENCE_MS: f32 = 650.0;
pub const MIN_CHUNK_MS: f32 = 800.0;
pub const MAX_CHUNK_MS: f32 = 12_000.0;
pub const MIN_VOICED_MS: f32 = 250.0;
pub const VAD_RMS_THRESHOLD: f32 = 0.010;

pub fn is_voiced(frame: &[f32]) -> bool {
    if frame.is_empty() {
        return false;
    }
    let mut square_sum = 0.0f32;
    for sample in frame {
        square_sum += sample * sample;
    }
    let rms = (square_sum / frame.len() as f32).sqrt();
    rms >= VAD_RMS_THRESHOLD
}

pub fn to_mono_f32(samples: &[i16], samples_per_channel: usize, channel_count: usize) -> Vec<f32> {
    if channel_count <= 1 {
        let mono = &samples[..samples_per_channel];
        return mono.iter().map(|s| *s as f32 / 32_768.0).collect();
    }
    let mut out = Vec::with_capacity(samples_per_channel);
    for idx in 0..samples_per_channel {
        let left = samples[idx * channel_count] as f32;
        let right = samples[idx * channel_count + 1] as f32;
        out.push(((left + right) * 0.5) / 32_768.0);
    }
    out
}

pub fn downsample_48k_to_16k(samples_48k: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(samples_48k.len() / 3);
    for chunk in samples_48k.chunks_exact(3) {
        out.push((chunk[0] + chunk[1] + chunk[2]) / 3.0);
    }
    out
}

pub fn extract_rtp_payload(rtp_packet: &[u8]) -> Option<&[u8]> {
    if rtp_packet.len() < 12 {
        return None;
    }

    // RTP version 2.
    if (rtp_packet[0] >> 6) != 2 {
        return None;
    }

    let has_padding = (rtp_packet[0] & 0x20) != 0;
    let has_extension = (rtp_packet[0] & 0x10) != 0;
    let csrc_count = (rtp_packet[0] & 0x0f) as usize;

    let mut header_len = 12usize.checked_add(csrc_count.checked_mul(4)?)?;
    if rtp_packet.len() < header_len {
        return None;
    }

    if has_extension {
        if rtp_packet.len() < header_len + 4 {
            return None;
        }

        // RFC3550 extension length is expressed in 32-bit words.
        let extension_length_words =
            u16::from_be_bytes([rtp_packet[header_len + 2], rtp_packet[header_len + 3]]) as usize;
        let extension_length_bytes = extension_length_words.checked_mul(4)?;
        header_len = header_len
            .checked_add(4)?
            .checked_add(extension_length_bytes)?;
        if rtp_packet.len() < header_len {
            return None;
        }
    }

    let mut payload_end = rtp_packet.len();
    if has_padding {
        let padding_len = *rtp_packet.last()? as usize;
        if padding_len == 0 || padding_len > payload_end.saturating_sub(header_len) {
            return None;
        }
        payload_end -= padding_len;
    }

    if payload_end <= header_len {
        return None;
    }

    Some(&rtp_packet[header_len..payload_end])
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_voiced ──────────────────────────────────────────────────────────────

    #[test]
    fn is_voiced_empty_frame_returns_false() {
        assert!(!is_voiced(&[]));
    }

    #[test]
    fn is_voiced_silent_frame_returns_false() {
        assert!(!is_voiced(&[0.009_f32]));
    }

    #[test]
    fn is_voiced_loud_frame_returns_true() {
        assert!(is_voiced(&[1.0_f32]));
    }

    #[test]
    fn is_voiced_threshold_exact_returns_true() {
        assert!(is_voiced(&[VAD_RMS_THRESHOLD]));
    }

    // ── to_mono_f32 ────────────────────────────────────────────────────────────

    #[test]
    fn to_mono_f32_single_channel() {
        let result = to_mono_f32(&[16384_i16], 1, 1);
        assert!((result[0] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn to_mono_f32_mono_zero_sample() {
        let result = to_mono_f32(&[0_i16], 1, 1);
        assert_eq!(result, vec![0.0_f32]);
    }

    #[test]
    fn to_mono_f32_stereo_mixes_channels() {
        // left=16384, right=16384 → (16384+16384)*0.5/32768 = 0.5
        let result = to_mono_f32(&[16384_i16, 16384_i16], 1, 2);
        assert!((result[0] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn to_mono_f32_stereo_asymmetric() {
        // left=16384 (0.5), right=0 → (16384+0)*0.5/32768 = 0.25
        let result = to_mono_f32(&[16384_i16, 0_i16], 1, 2);
        assert!((result[0] - 0.25).abs() < 1e-5);
    }

    // ── downsample_48k_to_16k ──────────────────────────────────────────────────

    #[test]
    fn downsample_48k_to_16k_averages_triplet() {
        let result = downsample_48k_to_16k(&[1.0, 2.0, 3.0]);
        assert_eq!(result.len(), 1);
        assert!((result[0] - 2.0).abs() < 1e-6);
    }

    #[test]
    fn downsample_48k_to_16k_empty_input() {
        assert_eq!(downsample_48k_to_16k(&[]), Vec::<f32>::new());
    }

    #[test]
    fn downsample_48k_to_16k_multiple_triplets() {
        let result = downsample_48k_to_16k(&[0.0, 0.0, 0.0, 3.0, 3.0, 3.0]);
        assert_eq!(result.len(), 2);
        assert!((result[0] - 0.0).abs() < 1e-6);
        assert!((result[1] - 3.0).abs() < 1e-6);
    }

    // ── extract_rtp_payload ────────────────────────────────────────────────────

    /// Build a minimal v2 RTP packet: 12-byte fixed header + extra bytes.
    fn make_rtp(first_byte: u8, extra: &[u8]) -> Vec<u8> {
        let mut pkt = vec![
            first_byte, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        pkt.extend_from_slice(extra);
        pkt
    }

    #[test]
    fn extract_rtp_payload_too_short_returns_none() {
        assert_eq!(extract_rtp_payload(&[0x80; 11]), None);
    }

    #[test]
    fn extract_rtp_payload_wrong_version_returns_none() {
        // version bits = 01 (0x40)
        assert_eq!(extract_rtp_payload(&make_rtp(0x40, &[0xFF])), None);
    }

    #[test]
    fn extract_rtp_payload_valid_minimal() {
        let pkt = make_rtp(0x80, &[0xDE, 0xAD]);
        assert_eq!(extract_rtp_payload(&pkt), Some(&[0xDE, 0xAD][..]));
    }

    #[test]
    fn extract_rtp_payload_csrc_makes_header_exceed_packet() {
        // csrc_count=1 → header_len=16, packet is only 14 bytes
        let pkt = make_rtp(0x81, &[0xFF, 0xFF]);
        assert_eq!(extract_rtp_payload(&pkt), None);
    }

    #[test]
    fn extract_rtp_payload_extension_too_short_for_ext_header() {
        // extension bit set but packet ends exactly at 12 bytes (need 16 for ext header)
        let pkt = make_rtp(0x90, &[]);
        assert_eq!(extract_rtp_payload(&pkt), None);
    }

    #[test]
    fn extract_rtp_payload_extension_length_exceeds_packet() {
        // ext_length_words=1 → 4 extra bytes → total header = 12+4+4=20
        // packet is 18 bytes → too short
        let mut pkt = make_rtp(0x90, &[]);
        pkt.extend_from_slice(&[0x00, 0x00, 0x00, 0x01, 0xFF, 0xFF]);
        assert_eq!(extract_rtp_payload(&pkt), None);
    }

    #[test]
    fn extract_rtp_payload_valid_with_zero_length_extension() {
        // ext_length_words=0 → header_len=16, payload = last 2 bytes
        let mut pkt = make_rtp(0x90, &[]);
        pkt.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0xBE, 0xEF]);
        assert_eq!(extract_rtp_payload(&pkt), Some(&[0xBE, 0xEF][..]));
    }

    #[test]
    fn extract_rtp_payload_padding_zero_returns_none() {
        // padding bit set, last byte = 0 → padding_len == 0
        let pkt = make_rtp(0xA0, &[0xDE, 0x00]);
        assert_eq!(extract_rtp_payload(&pkt), None);
    }

    #[test]
    fn extract_rtp_payload_padding_oversized_returns_none() {
        // padding_len=5, but payload area is only 2 bytes (14-12)
        let pkt = make_rtp(0xA0, &[0xDE, 0x05]);
        assert_eq!(extract_rtp_payload(&pkt), None);
    }

    #[test]
    fn extract_rtp_payload_padding_consumes_entire_payload_returns_none() {
        // padding_len=2, payload area=2, payload_end=12=header_len → None
        let pkt = make_rtp(0xA0, &[0xDE, 0x02]);
        assert_eq!(extract_rtp_payload(&pkt), None);
    }

    #[test]
    fn extract_rtp_payload_valid_with_padding() {
        // padding_len=1, payload_end=13 > header_len=12 → Some([0xDE])
        let pkt = make_rtp(0xA0, &[0xDE, 0x01]);
        assert_eq!(extract_rtp_payload(&pkt), Some(&[0xDE][..]));
    }
}
