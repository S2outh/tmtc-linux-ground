use simple_config::Config;

#[derive(Debug)]
pub enum TMTCError {
    ConnectNATS(async_nats::ConnectErrorKind),
    SubscribeNATS(async_nats::SubscribeError),
}

pub async fn run(config: TMTCConfig) -> Result<(), TMTCError> {
    let nats_client = async_nats::ConnectOptions::with_user_and_password(config.nats_user, config.nats_pwd)
        .connect(config.nats_address)
        .await.map_err(|e| TMTCError::ConnectNATS(e.kind()))?;


    Ok(())
}


#[derive(Config)]
pub struct TMTCConfig {
    // -- Nats
    pub nats_address: String,
    pub nats_user: String,
    pub nats_pwd: String,
}
impl TMTCConfig {
    /// Creates a new configuration with default values
    pub fn new() -> Self {
        Self {
            nats_address: String::from("127.0.0.1"),
            nats_user: String::from("nats"),
            nats_pwd: String::from("nats"),
        }
    }
}

