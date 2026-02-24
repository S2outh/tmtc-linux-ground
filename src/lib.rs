#![feature(const_trait_impl)]
#![feature(const_cmp)]

mod macros;
mod ground_tm_defs;
extern crate alloc;

use std::{convert::Infallible, time::{Duration, SystemTime, UNIX_EPOCH}};

use simple_config::Config;
use embedded_io_adapters::tokio_1::FromTokio;
use openlst_driver::{lst_receiver::{LSTMessage, LSTReceiver, LSTTelemetry}, lst_sender::{LSTCmd, LSTSender}};
use tokio_serial::{SerialPortBuilderExt, SerialStream};
use tokio::{io::{WriteHalf, split}, sync::mpsc, time};

use south_common::{
    beacons::{LSTBeacon, EPSBeacon, HighRateUpperSensorBeacon, LowRateUpperSensorBeacon, LowerSensorBeacon},
    tmtc_system::{Beacon, ParseError, ground_tm::{Serializer, SerializableTMValue}}
};

const OPENLST_HWID: u16 = 0x2DEC;

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

struct CborSerializer;
impl Serializer for CborSerializer {
    type Error = minicbor_serde::error::EncodeError<Infallible>;
    fn serialize_value<T: serde::Serialize>(&self, value: &T)
        -> Result<alloc::vec::Vec<u8>, Self::Error> {
        minicbor_serde::to_vec(value)
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
        if let Err(e) = lst_sender.cmd(LSTCmd::GetTelem).await {
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

    print_lst_values!(tm, (
        Rssi,
        Lqi,
        PacketsGood,
        PacketsRejectedChecksum,
        PacketsRejectedOther
    ));

    pub_lst_values!(nats_sender, tm, timestamp, (
        Uptime,
        Rssi,
        Lqi,
        PacketsSent,
        PacketsGood,
        PacketsRejectedChecksum,
        PacketsRejectedOther
    ));
}

pub async fn run(config: GSTConfig) -> Result<(), GSTError> {

    // Initialize UART and LST
    let (uart_rx, uart_tx) =
        split(tokio_serial::new(config.serial_port.clone(), config.serial_baud)
            .open_native_async()
            .map_err(GSTError::SerialError)?);

    let mut lst_receiver = LSTReceiver::new(FromTokio::new(uart_rx));
    let lst_sender = LSTSender::new(FromTokio::new(uart_tx), OPENLST_HWID);

    tokio::spawn(telemetry_request_thread(lst_sender));

    // Initialize beacons
    let mut lst_beacon = LSTBeacon::new();
    let mut eps_beacon = EPSBeacon::new();
    let mut high_rate_upper_beacon = HighRateUpperSensorBeacon::new();
    let mut low_rate_upper_beacon = LowRateUpperSensorBeacon::new();
    let mut lower_sensor_beacon = LowerSensorBeacon::new();

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
                        parse_beacon!(data, lst_beacon, nats_sender, (packets_sent));
                        parse_beacon!(data, eps_beacon, nats_sender, (bat1_voltage));
                        parse_beacon!(data, high_rate_upper_beacon, nats_sender);
                        parse_beacon!(data, low_rate_upper_beacon, nats_sender, (gps_ecef));
                        parse_beacon!(data, lower_sensor_beacon, nats_sender);
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

