use simple_config::Config;
use tmtc_linux_ground::GSTConfig;

#[tokio::main]
async fn main() {
    let mut config = GSTConfig::new();
    config.parse_file("tmtc.conf").expect("could not parse config file");
    config.parse_cli().expect("could not parse cli");

    tmtc_linux_ground::run(config).await
        .expect("telemetry receiver service finished with non zero exit code");
}
