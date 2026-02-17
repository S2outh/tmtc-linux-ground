
#[macro_export]
macro_rules! parse_beacon {
    ($data: ident, $beacon:ident, $nats_sender:ident $(, ($($field:ident),*))?) => {
        match $beacon.from_bytes($data, &mut crc_ccitt) {
            Ok(()) => {
                println!("[BEACON] Parsed {} at {}", stringify!($beacon), $beacon.timestamp);
                $($(
                    if let Some(value) = $beacon.$field {
                        println!("[TELEM] {}: {:#?}", stringify!($field), value);
                    }
                )*)?
                if let Some(sender) = &$nats_sender {
                    match $beacon.serialize(&CborSerializer) {
                        Ok(serialized) => {
                            for value in serialized {
                                let _ = sender.send(value).await;
                            }
                        },
                        Err(_) => eprintln!("[ERROR] could not serialize received value")
                    }
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


#[macro_export]
macro_rules! print_lst_value {
    ($lst_telem:ident, $field:ident) => {
        println!("[LST] {}: {:?}", stringify!($field), $lst_telem.$field);
    }
}

#[macro_export]
macro_rules! pub_lst_value {
    ($nats_sender: ident, $lst_telem:ident, $timestamp: ident, ($(($def:ident, $field:ident)),*)) => {
        if let Some(sender) = $nats_sender {
            $(
                let serialized = $lst_telem.$field.serialize_ground(&ground_tm_defs::groundstation::lst::$def, $timestamp, &CborSerializer)
                                    .expect("could not serialize value");
                for v in serialized {
                    let _ = sender.send(v).await;
                }
            )*
        }
    }
}
