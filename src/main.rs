use std::time::Duration;

use surge_ping::{Client, Config, ICMP, PingIdentifier, PingSequence};

#[tokio::main]
async fn main() {
    let ip = tokio::net::lookup_host(format!("{}:0", "google.com"))
        .await
        .expect("host lookup error")
        .next()
        .map(|val| val.ip())
        .unwrap();

    let mut config_builder = Config::builder();

    /*
    if let Some(interface) = opt.iface {
        config_builder = config_builder.interface(&interface);
    }

     */

    if ip.is_ipv6() {
        config_builder = config_builder.kind(ICMP::V6);
    }
    let config = config_builder.build();

    let payload = vec![0; 64];
    let client = Client::new(&config).unwrap();
    let mut pinger = client.pinger(ip, PingIdentifier(111)).await;
    pinger.timeout(Duration::from_secs(1));

    match pinger.ping(PingSequence(0), &payload).await {
        Ok((packet, rtt)) => {
            println!("{:?} {:0.2?}", packet, rtt);
        }
        Err(e) => println!("{}", e),
    };
}
