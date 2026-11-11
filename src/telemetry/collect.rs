use crate::platform::TextReader;
use crate::telemetry::model::*;
use std::io;
use std::path::Path;
use std::time::Instant;

pub fn collect_raw(reader: &dyn TextReader) -> RawSnapshot {
    RawSnapshot {
        load_average: parse_load_average(reader).ok(),
        cpu: parse_cpu(reader).unwrap_or(CpuCounters {
            user: 0,
            nice: 0,
            system: 0,
            idle: 0,
            iowait: 0,
            irq: 0,
            softirq: 0,
            steal: 0,
        }),
        memory: parse_memory(reader).unwrap_or(MemoryCounters {
            total_bytes: 0,
            available_bytes: 0,
            used_bytes: 0,
            free_bytes: 0,
            swap_total_bytes: 0,
            swap_free_bytes: 0,
            swap_used_bytes: 0,
        }),
        disk: parse_disk(reader).unwrap_or(DiskCounters {
            read_bytes: 0,
            write_bytes: 0,
            read_ops: 0,
            write_ops: 0,
        }),
        network: parse_network(reader).unwrap_or(NetworkCounters {
            rx_bytes: 0,
            tx_bytes: 0,
            rx_packets: 0,
            tx_packets: 0,
        }),
        captured_at: Instant::now(),
    }
}

pub fn sample_from_raw(raw: RawSnapshot, previous: Option<&RawSnapshot>) -> TelemetrySample {
    let rates = previous.map(|previous| {
        let elapsed = raw.captured_at.duration_since(previous.captured_at);
        let elapsed_seconds = if elapsed.as_secs_f64() > 0.0 {
            elapsed.as_secs_f64()
        } else {
            1.0
        };

        let cpu_total_delta = raw.cpu.total().saturating_sub(previous.cpu.total()) as i128;
        let cpu_busy_delta = raw.cpu.busy().saturating_sub(previous.cpu.busy()) as i128;
        let cpu_usage = if cpu_total_delta > 0 {
            Some((cpu_busy_delta as f64 / cpu_total_delta as f64).clamp(0.0, 1.0))
        } else {
            None
        };

        TelemetryRates {
            elapsed_seconds,
            cpu_usage,
            disk_read_bytes_per_second: Some(
                (raw.disk.read_bytes.saturating_sub(previous.disk.read_bytes)) as f64
                    / elapsed_seconds,
            ),
            disk_write_bytes_per_second: Some(
                (raw.disk
                    .write_bytes
                    .saturating_sub(previous.disk.write_bytes)) as f64
                    / elapsed_seconds,
            ),
            network_rx_bytes_per_second: Some(
                (raw.network
                    .rx_bytes
                    .saturating_sub(previous.network.rx_bytes)) as f64
                    / elapsed_seconds,
            ),
            network_tx_bytes_per_second: Some(
                (raw.network
                    .tx_bytes
                    .saturating_sub(previous.network.tx_bytes)) as f64
                    / elapsed_seconds,
            ),
        }
    });

    TelemetrySample {
        load_average: raw.load_average,
        cpu: raw.cpu,
        memory: raw.memory,
        disk: raw.disk,
        network: raw.network,
        rates,
    }
}

#[derive(Debug, Clone)]

pub struct RawSnapshot {
    pub load_average: Option<LoadAverage>,
    pub cpu: CpuCounters,
    pub memory: MemoryCounters,
    pub disk: DiskCounters,
    pub network: NetworkCounters,
    pub captured_at: Instant,
}

pub fn parse_cpu(reader: &dyn TextReader) -> Result<CpuCounters, io::Error> {
    let raw = reader.read_to_string(Path::new("/proc/stat"))?;
    let line = raw
        .lines()
        .find(|line| line.starts_with("cpu "))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing cpu line"))?;
    let fields: Vec<&str> = line.split_whitespace().skip(1).collect();
    let number = |index: usize| {
        fields
            .get(index)
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0)
    };

    Ok(CpuCounters {
        user: number(0),
        nice: number(1),
        system: number(2),
        idle: number(3),
        iowait: number(4),
        irq: number(5),
        softirq: number(6),
        steal: number(7),
    })
}

pub fn parse_memory(reader: &dyn TextReader) -> Result<MemoryCounters, io::Error> {
    let raw = reader.read_to_string(Path::new("/proc/meminfo"))?;
    let total = key_b(&raw, "MemTotal:");
    let mut available = key_b(&raw, "MemAvailable:");
    if available == 0 {
        available = key_b(&raw, "MemFree:") + key_b(&raw, "Buffers:") + key_b(&raw, "Cached:");
    }
    if total == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing memory data",
        ));
    }

    let swap_total_bytes = key_b(&raw, "SwapTotal:");
    let swap_free_bytes = key_b(&raw, "SwapFree:");

    Ok(MemoryCounters {
        total_bytes: total,
        available_bytes: available,
        used_bytes: total.saturating_sub(available),
        free_bytes: key_b(&raw, "MemFree:"),
        swap_total_bytes,
        swap_free_bytes,
        swap_used_bytes: swap_total_bytes.saturating_sub(swap_free_bytes),
    })
}

pub fn parse_disk(reader: &dyn TextReader) -> Result<DiskCounters, io::Error> {
    let raw = reader.read_to_string(Path::new("/proc/diskstats"))?;
    let mut read_bytes = 0u64;
    let mut write_bytes = 0u64;
    let mut read_ops = 0u64;
    let mut write_ops = 0u64;

    // /proc/diskstats layout: major minor name reads reads_merged sectors_read ms_reading
    //                        writes writes_merged sectors_written ...
    for line in raw.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 10 {
            continue;
        }
        let sectors_read = fields[5].parse::<u64>().unwrap_or(0);
        let sectors_written = fields[9].parse::<u64>().unwrap_or(0);
        read_bytes = read_bytes.saturating_add(sectors_read.saturating_mul(512));
        write_bytes = write_bytes.saturating_add(sectors_written.saturating_mul(512));
        read_ops = read_ops.saturating_add(fields[3].parse::<u64>().unwrap_or(0));
        write_ops = write_ops.saturating_add(fields[7].parse::<u64>().unwrap_or(0));
    }

    Ok(DiskCounters {
        read_bytes,
        write_bytes,
        read_ops,
        write_ops,
    })
}

pub fn parse_network(reader: &dyn TextReader) -> Result<NetworkCounters, io::Error> {
    let raw = reader.read_to_string(Path::new("/proc/net/dev"))?;
    let mut rx_bytes = 0u64;
    let mut tx_bytes = 0u64;
    let mut rx_packets = 0u64;
    let mut tx_packets = 0u64;

    // /proc/net/dev layout: interface: rx_bytes rx_packets rx_errs rx_drop rx_fifo rx_frame
    //                              rx_compressed rx_multicast tx_bytes tx_packets tx_errs ...
    for line in raw.lines().skip(2) {
        let (interface, rest) = match line.split_once(':') {
            Some(pair) => pair,
            None => continue,
        };
        let interface = interface.trim();
        if interface == "lo" || interface.starts_with("lo:") {
            continue;
        }

        let fields: Vec<&str> = rest.split_whitespace().collect();
        if fields.len() < 10 {
            continue;
        }
        rx_bytes = rx_bytes.saturating_add(fields[0].parse::<u64>().unwrap_or(0));
        rx_packets = rx_packets.saturating_add(fields[1].parse::<u64>().unwrap_or(0));
        tx_bytes = tx_bytes.saturating_add(fields[8].parse::<u64>().unwrap_or(0));
        tx_packets = tx_packets.saturating_add(fields[9].parse::<u64>().unwrap_or(0));
    }

    Ok(NetworkCounters {
        rx_bytes,
        tx_bytes,
        rx_packets,
        tx_packets,
    })
}

fn parse_load_average(reader: &dyn TextReader) -> Result<LoadAverage, io::Error> {
    let raw = reader.read_to_string(Path::new("/proc/loadavg"))?;
    let fields: Vec<&str> = raw.split_whitespace().collect();
    if fields.len() < 3 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "malformed loadavg",
        ));
    }
    Ok(LoadAverage {
        one: fields[0]
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad loadavg"))?,
        five: fields[1]
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad loadavg"))?,
        fifteen: fields[2]
            .parse()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad loadavg"))?,
    })
}

fn key_b(source: &str, key: &str) -> u64 {
    source
        .lines()
        .find(|line| line.starts_with(key))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
        .saturating_mul(1024)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockReader {
        files: HashMap<String, String>,
    }

    impl MockReader {
        fn new(files: &[(&str, &str)]) -> Self {
            Self {
                files: files
                    .iter()
                    .map(|(path, content)| ((*path).to_string(), (*content).to_string()))
                    .collect(),
            }
        }
    }

    impl TextReader for MockReader {
        fn read_to_string(&self, path: &Path) -> Result<String, io::Error> {
            self.files
                .get(path.to_string_lossy().as_ref())
                .cloned()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "missing fixture"))
        }

        fn read_dir_names(&self, _path: &Path) -> Result<Vec<String>, io::Error> {
            Ok(Vec::new())
        }

        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path.to_string_lossy().as_ref())
        }
    }

    #[test]
    fn parse_memory_reports_free_and_swap_used() {
        let reader = MockReader::new(&[(
            "/proc/meminfo",
            "MemTotal:       1000 kB\nMemFree:         250 kB\nMemAvailable:    600 kB\nSwapTotal:       500 kB\nSwapFree:        300 kB\n",
        )]);

        let memory = parse_memory(&reader).unwrap();

        assert_eq!(memory.total_bytes, 1000 * 1024);
        assert_eq!(memory.free_bytes, 250 * 1024);
        assert_eq!(memory.available_bytes, 600 * 1024);
        assert_eq!(memory.used_bytes, 400 * 1024);
        assert_eq!(memory.swap_total_bytes, 500 * 1024);
        assert_eq!(memory.swap_free_bytes, 300 * 1024);
        assert_eq!(memory.swap_used_bytes, 200 * 1024);
    }

    #[test]
    fn parse_disk_reports_operation_counts() {
        let reader = MockReader::new(&[(
            "/proc/diskstats",
            "   8       0 sda 100 0 2048 5 200 0 4096 10 0 0 0 0 0 0 0\n",
        )]);

        let disk = parse_disk(&reader).unwrap();

        assert_eq!(disk.read_bytes, 2048 * 512);
        assert_eq!(disk.write_bytes, 4096 * 512);
        assert_eq!(disk.read_ops, 100);
        assert_eq!(disk.write_ops, 200);
    }

    #[test]
    fn parse_network_reports_packet_counts_and_skips_loopback() {
        let reader = MockReader::new(&[(
            "/proc/net/dev",
            "Inter-|   Receive                                                |  Transmit\n face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed\n    lo: 1000 10 0 0 0 0 0 0 1000 10 0 0 0 0 0 0\n  eth0: 2048 20 0 0 0 0 0 0 4096 40 0 0 0 0 0 0\n",
        )]);

        let network = parse_network(&reader).unwrap();

        assert_eq!(network.rx_bytes, 2048);
        assert_eq!(network.tx_bytes, 4096);
        assert_eq!(network.rx_packets, 20);
        assert_eq!(network.tx_packets, 40);
    }
}
