//! F1 Telemetry Processing Pipeline
mod telemetry;
mod processor;
mod metrics;
mod dashboard;

use tokio::sync::broadcast;
use dashboard::DashboardData;
use processor::{TelemetryProcessor, PacketDecoder};
use metrics::Metrics;

use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tokio::time::{interval, timeout, Duration};
use tokio::signal;

const UDP_PORT: u16 = 20777;
const BUFFER_SIZE: usize = 2048;
const INACTIVITY_TIMEOUT_SECS: u64 = 5;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("{}", "=".repeat(70));
    println!("F1 TELEMETRY PROCESSOR - RUST PIPELINE");
    println!("{}", "=".repeat(70));
    println!("\nSimulating real F1 pit wall processing systems");
    println!("Requirements: <10ms latency, <0.1% packet loss");
    println!("\nFeatures:");
    println!("  • Realistic processing delays (0.1-5ms)");
    println!("  • Occasional processing spikes (simulating CPU load)");
    println!("  • Network packet corruption simulation");
    println!("  • Automatic shutdown after {}s inactivity", INACTIVITY_TIMEOUT_SECS);
    println!("{}", "=".repeat(70));
    
    // parse command line args
    let args: Vec<String> = std::env::args().collect();
    let simulate_load = !args.contains(&"--no-simulation".to_string());
    
    if !simulate_load {
        println!("\n[MODE] Running in ideal mode (--no-simulation flag detected)");
    } else {
        println!("\n[MODE] Running with realistic load simulation");
        println!("[INFO] Expect to see some packet drops and higher latencies!");
    }
    
    let metrics = Arc::new(RwLock::new(Metrics::new()));
    let metrics_clone = Arc::clone(&metrics);
    
    let (dashboard_tx, _) = broadcast::channel::<DashboardData>(100);
    let dashboard_tx_clone = dashboard_tx.clone();
    
    tokio::spawn(async move {
        dashboard::start_dashboard(dashboard_tx_clone).await;
    });

    let mut processor = TelemetryProcessor::new(Arc::clone(&metrics), simulate_load);
    let decoder = PacketDecoder::new(simulate_load);

    let socket = UdpSocket::bind(format!("127.0.0.1:{}", UDP_PORT)).await?;
    println!("\n[UDP] Listening on port {}", UDP_PORT);
    println!("[INFO] Waiting for telemetry stream...\n");
    
    // spawn metrics printer
    let metrics_handle = tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(2));
        loop {
            interval.tick().await;
            let metrics = metrics_clone.read().await;
            if metrics.packets_received > 0 {
                metrics.print_summary();
            }
        }
    });
    
    let shutdown = signal::ctrl_c();
    tokio::pin!(shutdown);
    
    
    let mut buffer = vec![0u8; BUFFER_SIZE];
    let mut decisions_made = 0u64;
    
    loop {
        tokio::select! {
            _ = &mut shutdown => {
                println!("\n[SHUTDOWN] Received Ctrl+C, shutting down gracefully...");
                break;
            }
            
            result = timeout(Duration::from_secs(INACTIVITY_TIMEOUT_SECS), 
                           socket.recv_from(&mut buffer)) => {
                match result {
                    Ok(Ok((len, _addr))) => {
                        
                        // update received metrics
                        {
                            let mut m = metrics.write().await;
                            m.packets_received += 1;
                            m.bytes_received += len as u64;
                        }
                        
                        // decode packet (may fail due to corruption)
                        let packet = match decoder.decode(&buffer[..len]) {
                            Ok(p) => p,
                            Err(e) => {
                                if decisions_made % 100 == 0 {
                                    eprintln!("❌ [CORRUPT] {}", e);
                                }
                                let mut m = metrics.write().await;
                                m.packets_dropped += 1;
                                continue;
                            }
                        };
                        
                        // process packet with latency guarantee
                        match processor.process_packet(packet.clone()).await {
                            Ok(_) => {
                                // send telemetry to dashboard
                                let dashboard_data = DashboardData::from(&packet);
                                let _ = dashboard_tx.send(dashboard_data);
                                
                                // Show performance stats periodically
                                if decisions_made % 5000 == 0 {
                                    let (used, capacity) = processor.buffer_stats();
                                    println!("  [BUFFER] {}/{} slots | {} packets processed", 
                                        used, capacity, decisions_made);
                                }
                                
                                decisions_made += 1;
                            }
                            Err(e) => {
                                // Packet dropped due to latency
                                if decisions_made % 10 == 0 {  // Show more drops
                                    eprintln!("⏱️  [DROP] {}", e);
                                }
                            }
                        }
                    }
                    Ok(Err(e)) => {
                        eprintln!("[ERROR] UDP receive failed: {}", e);
                    }
                    Err(_) => {
                        println!("\n⏰ [TIMEOUT] No packets for {}s, shutting down...", 
                               INACTIVITY_TIMEOUT_SECS);
                        break;
                    }
                }
            }
        }
    }
    
    metrics_handle.abort();
    
    println!("\n{}", "=".repeat(70));
    println!("FINAL STATISTICS");
    println!("{}", "=".repeat(70));
    
    let final_metrics = metrics.read().await;
    final_metrics.print_summary();
    

    let (_, _, p99_ms) = final_metrics.latency_stats();
    let loss_rate = final_metrics.packet_loss_rate();
    
    println!("\n  PERFORMANCE ASSESSMENT:");
    if p99_ms < 10.0 && loss_rate < 0.1 {
        println!("✅ System meets requirements!");
    } else {
        println!("❌ System does NOT meet requirements:");
        if p99_ms >= 10.0 {
            println!("   - P99 latency {:.2}ms exceeds 10ms limit", p99_ms);
        }
        if loss_rate >= 0.1 {
            println!("   - Packet loss {:.2}% exceeds 0.1% limit", loss_rate);
        }
    }
    
    Ok(())
}