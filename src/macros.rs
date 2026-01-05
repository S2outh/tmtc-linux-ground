
#[macro_export]
macro_rules! parse_beacon {
    ($data: ident, $beacon:ident, $nats_client:ident)  => {
        match $beacon.from_bytes($data, &mut crc_ccitt) {
            Ok(()) => {
                println!("parsed {}", stringify!($beacon));
                let serialized_telem = $beacon.serialize();
                for (address, bytes) in serialized_telem {
                    $nats_client.publish(address, bytes.into()).await.unwrap();
                }
            }
            Err(e) => {
                match e {
                    ParseError::WrongId => (),
                    ParseError::BadCRC => eprintln!("{} with bad crc received", stringify!($beacon)),
                    ParseError::OutOfMemory => eprintln!("{} could not be parsed: not enough bytes", stringify!($beacon)),
                }
            }
        }
    };
    ($data: ident, $beacon:ident, $nats_client:ident, ($($field:ident),*)) => {
        match $beacon.from_bytes($data, &mut crc_ccitt) {
            Ok(()) => {
                println!("parsed {}", stringify!($beacon));
                $(
                    println!("{} > {}: {:?}", stringify!($beacon), stringify!($field), $beacon.$field);
                )*
                let serialized_telem = $beacon.serialize();
                for (address, bytes) in serialized_telem {
                    $nats_client.publish(address, bytes.into()).await.unwrap();
                }
            }
            Err(e) => {
                match e {
                    ParseError::WrongId => (),
                    ParseError::BadCRC => eprintln!("{} with bad crc received", stringify!($beacon)),
                    ParseError::OutOfMemory => eprintln!("{} could not be parsed: not enough bytes", stringify!($beacon)),
                }
            }
        }
    };
}

