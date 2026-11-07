use crate::telemetry::TelemetrySample;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum AppEvent {
    #[allow(dead_code)]
    RefreshTick,
    TelemetrySample(Box<TelemetrySample>),
    Shutdown,
}
