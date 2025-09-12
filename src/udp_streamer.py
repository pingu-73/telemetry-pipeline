import socket
import time
from collections import deque
import statistics

from data_source import F1DataSource
from config import *

class UDPTelemetryStreamer:
    def __init__(self, target_ip: str = '127.0.0.1', target_port: int = UDP_PORT):
        self.target_ip = target_ip
        self.target_port = target_port
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        
        # socket to non-blocking for low latency
        self.socket.setblocking(0)
        
        # matrix
        self.packets_sent = 0
        self.packets_dropped = 0
        self.bytes_sent = 0
        self.start_time = None
        self.current_lap = 0
        self.total_laps = 0
        
        # latency tracking
        self.latencies_us = deque(maxlen=1000)  # microseconds
        
        print(f"[UDP] Streamer initialized -> {target_ip}:{target_port}")
        print("[UDP] Simulating telemetry")
        print(f"[UDP] Target: {BASE_FREQUENCY_HZ}Hz, <{MAX_LATENCY_MS}ms latency")
    
    def stream_session(self, data_source: F1DataSource, realtime: bool = True):
        self.start_time = time.time()
        packet_interval_s = PACKET_INTERVAL_MS / 1000.0
        
        self.total_laps = data_source.total_laps

        print("\n[UDP] Starting telemetry stream")
        print(f"[UDP] Streaming {self.total_laps} lap(s)")
        print(f"[UDP] Frequency: {BASE_FREQUENCY_HZ}Hz")
        print(f"[UDP] Max latency: {MAX_LATENCY_MS}ms")
        print(f"[UDP] Packet interval: {PACKET_INTERVAL_MS:.1f}ms")
        
        last_packet_time = time.perf_counter()
        first_packet = True
        
        for packet in data_source.generate_packets(realtime=realtime):
            # skip timing check for 1st packet (init overhead)
            if first_packet:
                last_packet_time = time.perf_counter()
                self.start_time = time.time()
                first_packet = False
            
            # measure send latency
            send_start = time.perf_counter()
            
            packet_bytes = packet.to_udp_bytes() # compact binary format
            
            try:
                # send packet (non-blocking)
                self.socket.sendto(packet_bytes, (self.target_ip, self.target_port))
                
                # latency in microseconds
                send_latency_us = (time.perf_counter() - send_start) * 1_000_000
                self.latencies_us.append(send_latency_us)
                
                # check exceed latency requirement
                if send_latency_us > MAX_LATENCY_MS * 1000:
                    self.packets_dropped += 1
                    print(f"[WARN] Packet {packet.packet_id} dropped - "f"latency {send_latency_us/1000:.2f}ms > {MAX_LATENCY_MS}ms limit")
                
                self.packets_sent += 1
                self.bytes_sent += len(packet_bytes)
                
            except BlockingIOError:
                # socket buffer full -> packet loss
                self.packets_dropped += 1
                print(f"[WARN] Packet {packet.packet_id} dropped - buffer full")
            
            if realtime and not first_packet:  # skip for first packet
                target_time = last_packet_time + packet_interval_s
                current_time = time.perf_counter()
                
                if current_time < target_time:
                    time.sleep(target_time - current_time) # maintaining freq
                elif current_time > target_time + (MAX_LATENCY_MS / 1000):
                    # too far behind
                    lag_ms = (current_time - target_time) * 1000
                    print(f"[CRITICAL] Stream lagging by {lag_ms:.1f}ms - "f"would drop packets!")
                
                last_packet_time = time.perf_counter()
            
            # track current lap
            if hasattr(data_source, 'current_lap'):
                self.current_lap = data_source.current_lap
            
            if self.packets_sent % BASE_FREQUENCY_HZ == 0:
                self._print_metrics()
                
    def _print_metrics(self):
        elapsed = time.time() - self.start_time
        pps = self.packets_sent / elapsed
        mbps = (self.bytes_sent * 8) / (elapsed * 1_000_000)
        
        if self.latencies_us:
            avg_latency_us = statistics.mean(self.latencies_us)
            p99_latency_us = sorted(self.latencies_us)[int(len(self.latencies_us) * 0.99)]
            
            # milisicond conversion
            avg_latency_ms = avg_latency_us / 1000 
            p99_latency_ms = p99_latency_us / 1000
        else:
            avg_latency_ms = p99_latency_ms = 0
        
        packet_loss_rate = (self.packets_dropped / max(self.packets_sent, 1)) * 100
        
        _lap_info = f" | Lap {self.current_lap}/{self.total_laps}" if self.total_laps > 0 else ""

        print("\n[METRICS] Telemetry Performance:")
        print(f"  Packets sent: {self.packets_sent:,}")
        print(f"  Data rate: {pps:.0f} pps (target: {BASE_FREQUENCY_HZ} pps)")
        print(f"  Throughput: {mbps:.2f} Mbps")
        print(f"  Avg latency: {avg_latency_ms:.3f}ms")
        print(f"  P99 latency: {p99_latency_ms:.3f}ms (limit: {MAX_LATENCY_MS}ms)")
        print(f"  Packet loss: {packet_loss_rate:.2f}% ({self.packets_dropped} dropped)")
        
        if p99_latency_ms > MAX_LATENCY_MS:
            print("  !!  WARNING: P99 latency exceeds requirement!")
        if packet_loss_rate > 0.1:
            print("  !!  WARNING: Packet loss exceeds 0.1% tolerance!")
    
    def close(self):
        self.socket.close()
        print("\n[UDP] Streamer closed")
        print(f"[UDP] Final stats: {self.packets_sent:,} sent, {self.packets_dropped} dropped")