use simple_config::Config;
use embedded_io_adapters::tokio_1::FromTokio;
use openlst_driver::lst_receiver;
use tokio_serial::{SerialPortBuilderExt, SerialStream};

use south_common::{LowRateTelemetry, MidRateTelemetry, DynBeacon, ParseError};

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

    let mut lst_receiver = lst_receiver::LSTReceiver::new(FromTokio::new(uart_rx));

    let mut low_rate_telemetry = LowRateTelemetry::new();
    let mut mid_rate_telemetry = MidRateTelemetry::new();

    loop {
        match lst_receiver.receive().await {
            Ok(msg) => {
                match msg {
                    lst_receiver::LSTMessage::Relay(data) => {
                        println!("data: {}", data.len());
                        match low_rate_telemetry.from_bytes(data, &mut crc_ccitt) {
                            Ok(()) => {
                                println!("parsed low rate telem");
                                println!("lst uptime: {}", low_rate_telemetry.uptime);
                                println!("lst lqi: {}", low_rate_telemetry.lqi);
                                println!("lst rssi: {}", low_rate_telemetry.rssi);
                                println!("lst send: {}", low_rate_telemetry.packets_send);
                                println!("lst good: {}", low_rate_telemetry.packets_good);
                                let serialized_telem = low_rate_telemetry.serialize();
                                for (address, bytes) in serialized_telem {
                                    nats_client.publish(address, bytes.into()).await.unwrap();
                                }
                            }
                            Err(e) => {
                                match e {
                                    ParseError::WrongId => (),
                                    ParseError::BadCRC => eprintln!("low rate telemetry message with bad crc received"),
                                    ParseError::OutOfMemory => eprintln!("low rate telemetry message could not be parsed: not enough bytes"),
                                }
                            }
                        }

                        match mid_rate_telemetry.from_bytes(data, &mut crc_ccitt) {
                            Ok(()) => {
                                println!("parsed mid rate telem");
                                println!("bat 1 voltage: {}", mid_rate_telemetry.bat1_voltage);
                                println!("internal temp: {}", mid_rate_telemetry.internal_temperature);
                                let serialized_telem = mid_rate_telemetry.serialize();
                                for (address, bytes) in serialized_telem {
                                    nats_client.publish(address, bytes.into()).await.unwrap();
                                }
                            }
                            Err(e) => {
                                match e {
                                    ParseError::WrongId => (),
                                    ParseError::BadCRC => eprintln!("mid rate telemetry message with bad crc received"),
                                    ParseError::OutOfMemory => eprintln!("mid rate telemetry message could not be parsed: not enough bytes"),
                                }
                            }
                        }
                    },
                    _ => () // Ignore internal messages for now
                }
            },
            Err(e) => {
                eprintln!("error in receiving frame: {:?}", e);
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

