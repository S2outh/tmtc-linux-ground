mod macros;

use serde;
use serde_cbor;

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use simple_config::Config;
use embedded_io_adapters::tokio_1::FromTokio;
use openlst_driver::{lst_receiver::{LSTMessage, LSTReceiver, LSTTelemetry}, lst_sender::{LSTCmd, LSTSender}};
use tokio_serial::{SerialPortBuilderExt, SerialStream};
use tokio::{io::{WriteHalf, split}, sync::mpsc, time};

use south_common::{LSTBeacon, EPSBeacon, SensorboardBeacon, Beacon, ParseError};

const OPENLST_HWID: u16 = 0x2DEC;

#[derive(Debug)]
pub enum GSTError {
    ConnectNATS(async_nats::ConnectErrorKind),
    SubscribeNATS(async_nats::SubscribeError),
    SerialError(tokio_serial::Error),
}

// This might be removed if a beacon type is used for local lst
// telemetry in the future
#[derive(serde::Serialize)]
struct NatsTelemetry<T: serde::Serialize> {
    timestamp: u64,
    value: T,
}
impl<T: serde::Serialize> NatsTelemetry<T> {
    pub fn new(timestamp: u64, value: T) -> Self {
        Self { timestamp, value }
    }
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

struct CborSerializer;
impl south_common::Serializer for CborSerializer {
    type Error = serde_cbor::Error;
    fn serialize<T: serde::Serialize>(&self, value: &T)
        -> Result<std::vec::Vec<u8>, Self::Error> {
        serde_cbor::to_vec(value)
    }
}

async fn nats_thread(config: GSTConfig, mut receiver: mpsc::Receiver<(&'static str, Vec<u8>)>) {
    loop {
        let nats_client = loop {
            match async_nats::ConnectOptions::with_user_and_password(config.nats_user.clone(), config.nats_pwd.clone())
                .connect(config.nats_address.clone())
                .await.map_err(|e| GSTError::ConnectNATS(e.kind())) {

                Ok(client) => {
                    println!("[NATS] succesfully connected to NATS server on {} with user {}", config.nats_address, config.nats_user);
                    break client;
                },
                Err(e) => eprintln!("[ERROR] Could not connect to NATS server: {:?}, retrying in 3s", e),
            }
            time::sleep(Duration::from_secs(3)).await;
        };
        loop {
            let (address, bytes) = receiver.recv().await.unwrap();
            if let Err(e) = nats_client.publish(address, bytes.into()).await {
                eprintln!("[ERROR] lost connection to NATS server: {:?}", e);
                break;
            }
        }
    }
}

async fn telemetry_request_thread(mut lst_sender: LSTSender<FromTokio<WriteHalf<SerialStream>>>) {
    const LST_TM_INTERVALL: Duration = Duration::from_secs(10);
    loop {
        time::sleep(LST_TM_INTERVALL).await;
        if let Err(e) = lst_sender.send_cmd(LSTCmd::GetTelem).await {
            eprintln!("[ERROR] could not send cmd over serial: {:?}", e);
        }
    }
}

async fn local_lst_telemetry(nats_sender: &Option<mpsc::Sender<(&'static str, Vec<u8>)>>, tm: LSTTelemetry) {

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_millis() as u64;

    println!("[LST] Received Telemetry at {}", timestamp);

    print_lst_value!(tm, uptime);
    print_lst_value!(tm, rssi);
    print_lst_value!(tm, lqi);
    print_lst_value!(tm, packets_sent);
    print_lst_value!(tm, packets_good);
    print_lst_value!(tm, packets_rejected_checksum);

    pub_lst_value!(nats_sender, tm, timestamp, (
        uptime,
        rssi,
        lqi,
        packets_sent,
        packets_good,
        packets_rejected_checksum,
        packets_rejected_other
    ));
}

pub async fn run(config: GSTConfig) -> Result<(), GSTError> {

    // Initialize UART and LST
    let (uart_rx, uart_tx) =
        split(tokio_serial::new(config.serial_port.clone(), config.serial_baud)
            .open_native_async()
            .map_err(|e| GSTError::SerialError(e))?);

    let mut lst_receiver = LSTReceiver::new(FromTokio::new(uart_rx));
    let lst_sender = LSTSender::new(FromTokio::new(uart_tx), OPENLST_HWID);

    tokio::spawn(telemetry_request_thread(lst_sender));

    // Initialize beacons(
    let mut lst_beacon = LSTBeacon::new();
    let mut eps_beacon = EPSBeacon::new();
    let mut sensorboard_beacon = SensorboardBeacon::new();

    // Connect to nats
    let nats_sender = if config.connect {
        let (sender, receiver) = mpsc::channel(30);
        tokio::spawn(nats_thread(config, receiver));
        Some(sender)
    } else {
        None
    };

    loop {
        match lst_receiver.receive().await {
            Ok(msg) => {
                match msg {
                    LSTMessage::Relay(data) => {
                        parse_beacon!(data, lst_beacon, nats_sender, (uptime, rssi, packets_good));
                        parse_beacon!(data, eps_beacon, nats_sender, (bat1_voltage));
                        parse_beacon!(data, sensorboard_beacon, nats_sender, (imu1_accel_full_range, baro_pressure, internal_temperature));
                    },
                    LSTMessage::Telem(tm) => {
                        local_lst_telemetry(&nats_sender, tm).await;
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


#[derive(Config, Clone)]
pub struct GSTConfig {
    // -- Nats
    pub connect: bool,
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
            connect: true,
            nats_address: String::from("127.0.0.1"),
            nats_user: String::from("nats"),
            nats_pwd: String::from("nats"),
            serial_port: String::from("/dev/ttyUSB0"),
            serial_baud: 115_200,
        }
    }
}

