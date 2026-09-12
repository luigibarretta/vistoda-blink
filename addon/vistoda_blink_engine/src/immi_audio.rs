//! Audio negotiation metadata, never an implicit microphone enable.
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

const PRESENT: u64 = 1 << 32;

#[derive(Default)]
pub struct AudioOffer(AtomicU64);

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct AudioOfferSnapshot {
    pub offered: bool,
    pub format: Option<u32>,
    pub codec: Option<&'static str>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub stream_aec: bool,
    pub microphone_enabled: bool,
}

impl AudioOffer {
    pub fn observe(&self, format: u32) {
        self.0.store(PRESENT | u64::from(format), Ordering::Release);
    }

    pub fn clear(&self) {
        self.0.store(0, Ordering::Release);
    }

    pub fn snapshot(&self) -> AudioOfferSnapshot {
        let value = self.0.load(Ordering::Acquire);
        let format = u32::try_from(value & u64::from(u32::MAX))
            .ok()
            .filter(|_| value & PRESENT != 0);
        let supported = format.is_some_and(|word| (0xa000_0000..=0xa000_0003).contains(&word));
        AudioOfferSnapshot {
            offered: format.is_some(),
            format,
            codec: format.filter(|_| supported).map(|word| {
                // The native codec-1 encoder is a no-op, not proven PCM uplink.
                if word & 1 == 0 {
                    "native_codec_1"
                } else {
                    "aac_adts"
                }
            }),
            sample_rate: supported.then_some(16_000),
            channels: supported.then_some(1),
            stream_aec: supported && format.is_some_and(|word| word & 2 != 0),
            microphone_enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AudioOffer;

    #[test]
    fn no_offer_is_not_an_offered_zero_or_supported_audio() {
        let offer = AudioOffer::default();
        assert!(!offer.snapshot().offered);
        offer.observe(0);
        assert_eq!(offer.snapshot().format, Some(0));
        assert_eq!(offer.snapshot().codec, None);
    }

    #[test]
    fn supported_formats_never_implicitly_enable_microphone() {
        let offer = AudioOffer::default();
        for word in 0xa000_0000..=0xa000_0003 {
            offer.observe(word);
            let result = offer.snapshot();
            assert_eq!(result.sample_rate, Some(16_000));
            assert_eq!(result.channels, Some(1));
            assert_eq!(result.stream_aec, word & 2 != 0);
            assert!(!result.microphone_enabled);
        }
        offer.observe(0xa000_0006);
        assert_eq!(offer.snapshot().codec, None);
        assert!(!offer.snapshot().stream_aec);
        offer.clear();
        assert!(!offer.snapshot().offered);
    }
}
