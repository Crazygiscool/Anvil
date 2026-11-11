use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TelemetrySample {
    pub load_average: Option<LoadAverage>,
    pub cpu: CpuCounters,
    pub memory: MemoryCounters,
    pub disk: DiskCounters,
    pub network: NetworkCounters,
    pub rates: Option<TelemetryRates>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LoadAverage {
    pub one: f64,
    pub five: f64,
    pub fifteen: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct CpuCounters {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
}

impl CpuCounters {
    pub fn total(self) -> u64 {
        self.user
            + self.nice
            + self.system
            + self.idle
            + self.iowait
            + self.irq
            + self.softirq
            + self.steal
    }

    pub fn busy(self) -> u64 {
        self.total()
            .saturating_sub(self.idle)
            .saturating_sub(self.iowait)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MemoryCounters {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_free_bytes: u64,
    pub swap_used_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DiskCounters {
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub read_ops: u64,
    pub write_ops: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NetworkCounters {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TelemetryRates {
    pub elapsed_seconds: f64,
    pub cpu_usage: Option<f64>,
    pub disk_read_bytes_per_second: Option<f64>,
    pub disk_write_bytes_per_second: Option<f64>,
    pub network_rx_bytes_per_second: Option<f64>,
    pub network_tx_bytes_per_second: Option<f64>,
}

#[allow(dead_code)]
pub fn unsupported_telemetry_sample() -> TelemetrySample {
    TelemetrySample {
        load_average: None,
        cpu: CpuCounters {
            user: 0,
            nice: 0,
            system: 0,
            idle: 0,
            iowait: 0,
            irq: 0,
            softirq: 0,
            steal: 0,
        },
        memory: MemoryCounters {
            total_bytes: 0,
            available_bytes: 0,
            used_bytes: 0,
            free_bytes: 0,
            swap_total_bytes: 0,
            swap_free_bytes: 0,
            swap_used_bytes: 0,
        },
        disk: DiskCounters {
            read_bytes: 0,
            write_bytes: 0,
            read_ops: 0,
            write_ops: 0,
        },
        network: NetworkCounters {
            rx_bytes: 0,
            tx_bytes: 0,
            rx_packets: 0,
            tx_packets: 0,
        },
        rates: None,
    }
}
