mod macros;

use simple_config::Config;
use embedded_io_adapters::tokio_1::FromTokio;
use openlst_driver::lst_receiver::{LSTMessage, LSTReceiver};
use tokio_serial::{SerialPortBuilderExt, SerialStream};

use south_common::{LowRateTelemetry, MidRateTelemetry, HighRateTelemetry, Beacon, ParseError};


#[derive(Debug)]
pub enum GSTError {
    ConnectNATS(async_nats::ConnectErrorKind),
    SubscribeNATS(async_nats::SubscribeError),
    SerialError(tokio_serial::Error),
}

fn crc_ccitt(bytes: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for byte in bytes {
        crc ^= (*byte as u16) << 8;
        for _ in 0..8 {
            if (crc & 0x8000) != 0 {
                crc = (crc << 1) ^ 0x1021;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

pub async fn run(config: GSTConfig) -> Result<(), GSTError> {
    let nats_client = async_nats::ConnectOptions::with_user_and_password(config.nats_user, config.nats_pwd)
        .connect(config.nats_address)
        .await.map_err(|e| GSTError::ConnectNATS(e.kind()))?;
    
    let uart_rx: SerialStream =
        tokio_serial::new(config.serial_port, config.serial_baud)
            .open_native_async()
            .map_err(|e| GSTError::SerialError(e))?;

    let mut lst_receiver = LSTReceiver::new(FromTokio::new(uart_rx));

    let mut low_rate_telemetry = LowRateTelemetry::new();
    let mut mid_rate_telemetry = MidRateTelemetry::new();
    let mut high_rate_telemetry = HighRateTelemetry::new();

    loop {
        match lst_receiver.receive().await {
            Ok(msg) => {
                match msg {
                    LSTMessage::Relay(data) => {
                        parse_beacon!(data, low_rate_telemetry, nats_client, (uptime, rssi, packets_good));
                        parse_beacon!(data, mid_rate_telemetry, nats_client, (bat1_voltage));
                        parse_beacon!(data, high_rate_telemetry, nats_client, (imu1_accel_full_range, internal_temperature));
                    },
                    LSTMessage::Telem(_) => {
                        println!("[LST] Telem");
                        // TODO
                    },
                    LSTMessage::Ack => println!("[LST] Ack"),
                    LSTMessage::Nack => println!("[LST] Nack"),
                    LSTMessage::Unknown(a) => println!("[LST] Unknown: {}", a),
                }
            },
            Err(e) => {
                eprintln!("[ERROR] error in receiving frame: {:?}", e);
            }
        }
    }
}


#[derive(Config)]
pub struct GSTConfig {
    // -- Nats
    pub nats_address: String,
    pub nats_user: String,
    pub nats_pwd: String,
    // -- Serial
    pub serial_port: String,
    pub serial_baud: u32,
}
impl GSTConfig {
    /// Creates a new configuration with default values
    pub fn new() -> Self {
        Self {
            nats_address: String::from("127.0.0.1"),
            nats_user: String::from("nats"),
            nats_pwd: String::from("nats"),
            serial_port: String::from("/dev/ttyUSB0"),
            serial_baud: 115_200,
        }
    }
}

