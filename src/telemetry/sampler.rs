use crate::telemetry::collect::{RawSnapshot, collect_raw, sample_from_raw};
use crate::telemetry::model::TelemetrySample;

pub struct TelemetrySampler {
    previous: Option<RawSnapshot>,
}

impl TelemetrySampler {
    pub fn new() -> Self {
        Self { previous: None }
    }

    pub fn sample(&mut self, reader: &dyn crate::platform::TextReader) -> TelemetrySample {
        let raw = collect_raw(reader);
        let sample = sample_from_raw(raw.clone(), self.previous.as_ref());
        self.previous = Some(raw);
        sample
    }
}
