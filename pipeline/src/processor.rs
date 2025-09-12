//! core telemetry processor for performance metrics only
use crate::metrics::Metrics;
use crate::telemetry::{FastTelemetry, TelemetryPacket};
use rand::Rng;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const MAX_LATENCY_MS: u64 = 10;

pub struct TelemetryProcessor {
    metrics: Arc<RwLock<Metrics>>,
    packet_buffer: Vec<Vec<u8>>, // raw bytes instead of deserialized packets
    buffer_capacity: usize,
    packets_since_last_gc: usize,
    simulate_load: bool,
}

impl TelemetryProcessor {
    pub fn new(metrics: Arc<RwLock<Metrics>>, simulate_load: bool) -> Self {
        Self {
            metrics,
            packet_buffer: Vec::with_capacity(1000),
            buffer_capacity: 1000,
            packets_since_last_gc: 0,
            simulate_load,
        }
    }

    pub async fn process_packet_zero_copy(&mut self, data: Vec<u8>) -> Result<(), String> {
        let process_start = Instant::now();

        // Zero-copy decode only parse what we need
        let mut fast_telemetry = FastTelemetry::new(&data);

        // only decode critical fields for processing
        let packet_id = fast_telemetry
            .packet_id()
            .map_err(|e| format!("Failed to decode packet ID: {}", e))?;

        let priority = fast_telemetry.priority().unwrap_or(1);

        if self.simulate_load {
            // only decode speed for load sim
            if let Ok(speed) = fast_telemetry.speed() {
                self.simulate_processing_work_fast(speed, priority).await;
            }
        }

        let latency_us = process_start.elapsed().as_micros() as u64;
        let latency_ms = latency_us as f64 / 1000.0;

        if latency_ms > MAX_LATENCY_MS as f64 {
            let mut metrics = self.metrics.write().await;
            metrics.packets_dropped += 1;
            metrics.add_latency(latency_us);
            return Err(format!(
                "Packet {} dropped - latency {:.2}ms > {}ms",
                packet_id, latency_ms, MAX_LATENCY_MS
            ));
        }

        // update metrics
        let mut metrics = self.metrics.write().await;
        metrics.packets_processed += 1;
        metrics.add_latency(latency_us);

        // store raw bytes in ring buffer (no deserialization)
        if self.packet_buffer.len() >= self.buffer_capacity {
            self.packet_buffer.remove(0);
        }
        self.packet_buffer.push(data);

        // garbage collection perodic sim
        self.packets_since_last_gc += 1;
        if self.packets_since_last_gc > 10000 {
            if self.simulate_load {
                tokio::time::sleep(Duration::from_micros(500)).await;
            }
            self.packets_since_last_gc = 0;
        }

        Ok(())
    }

    async fn simulate_processing_work_fast(&self, speed: u16, priority: u8) {
        let mut rng = rand::thread_rng();

        // priority-based processing time
        let base_delay_us = match priority {
            0 => rng.gen_range(50..200),  // Critical: fast path
            1 => rng.gen_range(100..500), // High: normal path
            _ => rng.gen_range(200..800), // Lower: can be slower
        };

        // speed-based adjustment (high speed = more processing)
        let delay_us = if speed > 300 {
            base_delay_us * 2
        } else {
            base_delay_us
        };

        // occasional spike (5% chance)
        let final_delay_us = if rng.gen_bool(0.05) {
            delay_us * 10
        } else {
            delay_us
        };

        // busy wait for more accurate timing
        let start = Instant::now();
        while start.elapsed().as_micros() < final_delay_us as u128 {
            std::hint::spin_loop();
        }
    }

    pub fn buffer_stats(&self) -> (usize, usize) {
        (self.packet_buffer.len(), self.buffer_capacity)
    }
}

pub struct PacketDecoder {
    simulate_corruption: bool,
}

impl PacketDecoder {
    pub fn new(simulate_corruption: bool) -> Self {
        Self {
            simulate_corruption,
        }
    }

    pub fn decode_raw(&self, data: &[u8]) -> Result<Vec<u8>, String> {
        if self.simulate_corruption {
            let mut rng = rand::thread_rng();
            if rng.gen_bool(0.001) {
                // 0.1% corruption rate
                return Err("Packet corrupted during transmission".to_string());
            }
        }

        Ok(data.to_vec())
    }

    pub fn decode_full(&self, data: &[u8]) -> Result<TelemetryPacket, String> {
        TelemetryPacket::from_bytes(data).map_err(|e| format!("Decode error: {}", e))
    }
}
