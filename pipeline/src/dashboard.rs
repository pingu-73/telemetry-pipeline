//! Real-time telemetry dashboard server
use axum::{
    extract::ws::{WebSocket, WebSocketUpgrade, Message},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use serde::Serialize;
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};

#[derive(Debug, Clone, Serialize)]
pub struct DashboardData {
    pub timestamp: u64,
    pub speed: u16,
    pub throttle: f32,
    pub brake: f32,
    pub gear: i8,
    pub rpm: u16,
    pub drs: bool,
    pub fuel_flow: f32,
    pub engine_temp: i16,
    pub tyre_temps: Vec<i16>,
    pub lap_distance: f32,
    pub car_number: u8,
    pub driver: String,
}

impl From<&crate::telemetry::TelemetryPacket> for DashboardData {
    fn from(packet: &crate::telemetry::TelemetryPacket) -> Self {
        Self {
            timestamp: packet.t,
            speed: packet.spd,
            throttle: packet.thr,
            brake: packet.brk,
            gear: packet.g,
            rpm: packet.rpm,
            drs: packet.drs,
            fuel_flow: packet.fuel,
            engine_temp: packet.h2ot,
            tyre_temps: packet.tt.clone(),
            lap_distance: 0.0,
            car_number: 81,  // TODO: Hardcoded
            driver: "PIA".to_string(),  // TODO: Hardcoded
        }
    }
}

pub async fn start_dashboard(tx: broadcast::Sender<DashboardData>) {
    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(websocket_handler))
        .with_state(tx);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
        .await
        .unwrap();
    
    println!(" [DASHBOARD] F1 Telemetry Dashboard: http://127.0.0.1:8080");
    
    axum::serve(listener, app).await.unwrap();
}

async fn index() -> Html<&'static str> {
    Html(include_str!("dashboard.html"))
}

async fn websocket_handler(
    ws: WebSocketUpgrade,
    axum::extract::State(tx): axum::extract::State<broadcast::Sender<DashboardData>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, tx))
}

async fn handle_socket(mut socket: WebSocket, tx: broadcast::Sender<DashboardData>) {
    let mut rx = tx.subscribe();
    
    let mut ping_interval = interval(Duration::from_secs(30));
    
    loop {
        tokio::select! {
            Ok(data) = rx.recv() => {
                let json = serde_json::to_string(&data).unwrap();
                if socket.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
            _ = ping_interval.tick() => {
                if socket.send(Message::Ping(vec![])).await.is_err() {
                    break;
                }
            }
        }
    }
}