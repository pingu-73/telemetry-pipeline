import os
import time
import fastf1
from opentelemetry import metrics
from opentelemetry.sdk.metrics import MeterProvider
from opentelemetry.sdk.resources import Resource
from opentelemetry.exporter.otlp.proto.grpc.metric_exporter import OTLPMetricExporter
from opentelemetry.sdk.metrics.export import PeriodicExportingMetricReader, ConsoleMetricExporter

from config import (
    TARGET_CAR_NUMBER,
    DEFAULT_YEAR,
    DEFAULT_RACE,
    TARGET_SESSION,
    CACHE_DIR,
)

OLTP_ENDPOINT = "http://localhost:4317"


class OpenTele:
    def __init__(self):
        resource = Resource.create(
            {
                "service.name": "f1-telemetery",
                "car.number": str(TARGET_CAR_NUMBER),
                "driver.id": "PIA",
            }
        )

        exporter = OTLPMetricExporter(endpoint=OLTP_ENDPOINT, insecure=True)
        reader = PeriodicExportingMetricReader(exporter, export_interval_millis=1000)

        console_reader = PeriodicExportingMetricReader(
            ConsoleMetricExporter(), export_interval_millis=5000
        )

        provider = MeterProvider(
            resource=resource, metric_readers=[reader, console_reader]
        )
        metrics.set_meter_provider(provider)
        self.meter = metrics.get_meter("f1-telemetery", "1.0.0")
        self.provider = provider

        # Instruments defination
        self.gauge_speed = self.meter.create_gauge("car.speed", unit="km/h", description="speed of car")
        self.counter_packets = self.meter.create_counter("packets.sent", unit="packets", description="total telemetry packets sent")
        self.counter_dropped = self.meter.create_counter("packets.dropped",unit="packets",description="packets dropped due to latency or buffer overflow")
        self.counter_bytes = self.meter.create_counter("bytes.sent", unit="By", description="total bytes sent over UDP")
        self.histogram_latency = self.meter.create_histogram("send.latency", unit="us", description="per-packet send latency")
        self.gauge_throughput = self.meter.create_gauge("throughput.pps", unit="packets/s", description="packets per second")
        self.gauge_throughput_mbps = self.meter.create_gauge("throughput.mbps", unit="Mbit/s", description="throughput in megabits/s")
        self.gauge_latency_avg = self.meter.create_gauge("latency.avg", unit="ms", description="average send latency")
        self.gauge_latency_p99 = self.meter.create_gauge("latency.p99", unit="ms", description="P99 send latency")
        self.gauge_loss_rate = self.meter.create_gauge("packet.loss_rate", unit="%", description="packet loss percentage")

        print(f"[OTEL] Initialized -> exporting to {OLTP_ENDPOINT}")

    def load_data(self):
        print("[OTEL] Loading F1 session data...")
        os.makedirs(CACHE_DIR, exist_ok=True)
        fastf1.Cache.enable_cache(CACHE_DIR)
        session = fastf1.get_session(DEFAULT_YEAR, DEFAULT_RACE, TARGET_SESSION)
        session.load(telemetry=True)
        laps = session.laps.pick_drivers(TARGET_CAR_NUMBER)
        self.telemetery = laps.pick_fastest().get_telemetry()
        print(f"[OTEL] Loaded {len(self.telemetery)} telemetry samples")

    def stream(self):
        print("[OTEL] Streaming telemetry as OpenTelemetry metrics...")
        for _, row in self.telemetery.iterrows():
            attributes = {"lap_number": "1", "track_status": "Clear"}

            speed = float(row["Speed"])
            self.gauge_speed.set(speed, attributes)
            self.counter_packets.add(1, attributes)

            time.sleep(0.002)

        print("[OTEL] Stream complete")

    def shutdown(self):
        self.provider.shutdown()
        print("[OTEL] Metrics provider shut down")


if __name__ == "__main__":
    source = OpenTele()
    source.load_data()
    source.stream()
    source.shutdown()
