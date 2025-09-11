import sys
import os

sys.path.append(os.path.dirname(os.path.abspath(__file__)))

from data_source import F1DataSource
from udp_streamer import UDPTelemetryStreamer
from config import *

def main():
    print("=" * 70)
    print("TELEMETRY PIPELINE - SIMULATION")
    print("=" * 70)
    
    print("\n[1] Initilizing data source")
    data_source = F1DataSource(year=DEFAULT_YEAR, race=DEFAULT_RACE)
    
    print("\n[2] Loading F1 session data")
    if not data_source.load_session('R'):
        print("[ERROR] Failed to load session")
        return
    
    print("\n[3] Initilizing UDP Streamer")
    streamer = UDPTelemetryStreamer()
    
    print("\n[4] Starting F1 Telemetry Stream")
    print(f"Streaming at {BASE_FREQUENCY_HZ}Hz")
    
    try:
        streamer.stream_session(data_source, realtime=True)
    except KeyboardInterrupt:
        print("\n\n[STOP] Stream interrupted")
    finally:
        streamer.close()
    
    print("\n[COMPLETE] Working Perfectly")

if __name__ == "__main__":
    main()