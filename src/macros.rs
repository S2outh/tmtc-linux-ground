
#[macro_export]
macro_rules! parse_beacon {
    ($data: ident, $beacon:ident, $nats_client:ident)  => {
        parse_beacon!($data, $beacon, $nats_client, ());
    };
    ($data: ident, $beacon:ident, $nats_client:ident, ($($field:ident),*)) => {
        match $beacon.from_bytes($data, &mut crc_ccitt) {
            Ok(()) => {
                println!("[BEACON] Parsed {} at {}", stringify!($beacon), $beacon.timestamp);
                $(
                    println!("[TELEM] {}: {:?}", stringify!($field), $beacon.$field);
                )*
                let serialized_telem = $beacon.serialize();
                for (address, bytes) in serialized_telem {
                    $nats_client.publish(address, bytes.into()).await.unwrap();
                }
            }
            Err(e) => {
                match e {
                    ParseError::WrongId => (),
                    ParseError::BadCRC => eprintln!("[ERROR] {} with bad crc received", stringify!($beacon)),
                    ParseError::OutOfMemory => eprintln!("[ERROR] {} could not be parsed: not enough bytes", stringify!($beacon)),
                }
            }
        }
    };
}

