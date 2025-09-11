//! Performance metrics tracking
use std::collections::VecDeque;

pub struct Metrics {
    pub packets_received: u64,
    pub packets_processed: u64,
    pub packets_dropped: u64,
    pub bytes_received: u64,
    
    // latency in microseconds
    latencies: VecDeque<u64>,
    max_samples: usize,
    
    pub start_time: std::time::Instant,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            packets_received: 0,
            packets_processed: 0,
            packets_dropped: 0,
            bytes_received: 0,
            latencies: VecDeque::with_capacity(1000),
            max_samples: 1000,
            start_time: std::time::Instant::now(),
        }
    }
    
    pub fn add_latency(&mut self, latency_us: u64) {
        if self.latencies.len() >= self.max_samples {
            self.latencies.pop_front();
        }
        self.latencies.push_back(latency_us);
    }
    
    pub fn latency_stats(&self) -> (f64, f64, f64) {
        if self.latencies.is_empty() {
            return (0.0, 0.0, 0.0);
        }
        
        // sorted vector for %ile calculation
        let mut data: Vec<u64> = self.latencies.iter().copied().collect();
        data.sort_unstable();
        

        let sum: u64 = data.iter().sum();
        let mean = sum as f64 / data.len() as f64;
        
        let median = if data.len() % 2 == 0 {
            let mid = data.len() / 2;
            (data[mid - 1] + data[mid]) as f64 / 2.0
        } else {
            data[data.len() / 2] as f64
        };
        
        // P99 calculation
        let p99_index = ((data.len() as f64 * 0.99) as usize).min(data.len() - 1);
        let p99 = data[p99_index] as f64;
        
        // convert from microseconds to milliseconds
        (mean / 1000.0, median / 1000.0, p99 / 1000.0)
    }
    
    pub fn throughput_pps(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        if elapsed > 0.0 {
            self.packets_received as f64 / elapsed
        } else {
            0.0
        }
    }
    
    pub fn packet_loss_rate(&self) -> f64 {
        if self.packets_received > 0 {
            (self.packets_dropped as f64 / self.packets_received as f64) * 100.0
        } else {
            0.0
        }
    }
    
    pub fn print_summary(&self) {
        let (mean_ms, median_ms, p99_ms) = self.latency_stats();
        let elapsed = self.start_time.elapsed().as_secs_f64();
        
        println!("\n[RUST METRICS] Pipeline Performance:");
        println!("  Packets: {} received, {} processed, {} dropped", 
                 self.packets_received, self.packets_processed, self.packets_dropped);
        println!("  Throughput: {:.0} pps", self.throughput_pps());
        println!("  Latency: mean={:.3}ms median={:.3}ms p99={:.3}ms", 
                 mean_ms, median_ms, p99_ms);
        println!("  Packet loss: {:.2}%", self.packet_loss_rate());
        println!("  Runtime: {:.1}s", elapsed);
        
        if p99_ms > 10.0 {
            println!("  !!  WARNING: P99 latency exceeds 10ms!");
        }
        if self.packet_loss_rate() > 0.1 {
            println!("  !!  WARNING: Packet loss exceeds 0.1% tolerance!");
        }
    }
}