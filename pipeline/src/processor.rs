//! core telemetry processor for performance metrics only
use crate::telemetry::TelemetryPacket;
use crate::metrics::Metrics;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::time::{Duration, Instant};
use rand::Rng;

const MAX_LATENCY_MS: u64 = 10;

pub struct TelemetryProcessor {
    metrics: Arc<RwLock<Metrics>>,
    packet_buffer: Vec<TelemetryPacket>,
    buffer_capacity: usize,
    packets_since_last_gc: usize,
    simulate_load: bool,
    
    // performance thresholds only for logs
    critical_thresholds: ThresholdConfig,
}

struct ThresholdConfig {
    max_engine_temp: i16,
    max_tyre_temp: i16,
}

impl TelemetryProcessor {
    pub fn new(metrics: Arc<RwLock<Metrics>>, simulate_load: bool) -> Self {
        Self {
            metrics,
            packet_buffer: Vec::with_capacity(1000),
            buffer_capacity: 1000,
            packets_since_last_gc: 0,
            simulate_load,
            critical_thresholds: ThresholdConfig {
                max_engine_temp: 130,   // warn above 130°C
                max_tyre_temp: 140,     // tyres normal temp 90-130°C
            },
        }
    }

    pub async fn process_packet(&mut self, packet: TelemetryPacket) -> Result<(), String> {
        let process_start = Instant::now();
        
        if self.simulate_load {
            self.simulate_processing_work(&packet).await;
        }
        
        // only log critical issues
        self.check_critical_only(&packet);
        
        let latency_us = process_start.elapsed().as_micros() as u64;
        let latency_ms = latency_us as f64 / 1000.0;
        
        if latency_ms > MAX_LATENCY_MS as f64 {
            let mut metrics = self.metrics.write().await;
            metrics.packets_dropped += 1;
            metrics.add_latency(latency_us);
            return Err(format!("Packet {} dropped - latency {:.2}ms > {}ms", 
                             packet.id, latency_ms, MAX_LATENCY_MS));
        }
        
        // update metrics
        let mut metrics = self.metrics.write().await;
        metrics.packets_processed += 1;
        metrics.add_latency(latency_us);
        
        // ring buffer behavior
        if self.packet_buffer.len() >= self.buffer_capacity {
            self.packet_buffer.remove(0); // remove oldest
        }
        self.packet_buffer.push(packet);
        
        // garbage collection perodic simulation
        self.packets_since_last_gc += 1;
        if self.packets_since_last_gc > 10000 {
            if self.simulate_load {
                tokio::time::sleep(Duration::from_micros(500)).await;
            }
            self.packets_since_last_gc = 0;
        }
        
        Ok(())
    }
    
    async fn simulate_processing_work(&self, packet: &TelemetryPacket) {
        let mut rng = rand::thread_rng();
        
        // base processing time
        let base_delay_us = rng.gen_range(100..500);
        
        // more processing for high-speed packets
        let delay_us = if packet.spd > 300 {
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
        
        // busy wait
        let start = Instant::now();
        while start.elapsed().as_micros() < final_delay_us as u128 {
            std::hint::spin_loop();
        }
    }
    
    fn check_critical_only(&self, packet: &TelemetryPacket) {
        // engine overheating (>130°C is critical)
        if packet.h2ot > self.critical_thresholds.max_engine_temp {
            println!("🔥 [CRITICAL] Engine temp {}°C exceeds safe limit!", packet.h2ot);
        }
        
        // tyre temp only if REALLY high (>140°C)
        let max_tyre = *packet.tt.iter().max().unwrap_or(&0);
        if max_tyre > self.critical_thresholds.max_tyre_temp {
            println!("!!  [CRITICAL] Tyre overheating: {}°C!", max_tyre);
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
        Self { simulate_corruption }
    }
    
    pub fn decode(&self, data: &[u8]) -> Result<TelemetryPacket, String> {
        if self.simulate_corruption {
            let mut rng = rand::thread_rng();
            if rng.gen_bool(0.001) {  // 0.1% corruption rate
                return Err("Packet corrupted during transmission".to_string());
            }
        }
        
        TelemetryPacket::from_bytes(data)
            .map_err(|e| format!("Decode error: {}", e))
    }
}