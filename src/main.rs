use std::net::IpAddr;
use std::time::Duration;

use surge_ping::{Client, Config, ICMP, PingIdentifier, PingSequence};
use tokio::time::MissedTickBehavior;

const PING_INTERVAL: Duration = Duration::from_secs(60);
const PING_TIMEOUT: Duration = Duration::from_secs(1);

#[tokio::main]
async fn main() {
    let ip = tokio::net::lookup_host(format!("{}:0", "google.com"))
        .await
        .expect("host lookup error")
        .next()
        .map(|val| val.ip())
        .unwrap();

    /*
    if let Some(interface) = opt.iface {
        config_builder = config_builder.interface(&interface);
    }
    */

    let client_v4 = Client::new(&Config::builder().kind(ICMP::V4).build()).unwrap();
    let client_v6 = Client::new(&Config::builder().kind(ICMP::V6).build()).unwrap();

    if ip.is_ipv4() {
        ping_loop(&client_v4, ip).await;
    } else if ip.is_ipv6() {
        ping_loop(&client_v6, ip).await;
    }
}

#[derive(Debug)]
struct SharedState {
    n_t_pings: u16, // total pings
    n_s_pings: u16  // successful pings
}

// continuously ping given address
async fn ping_loop(client: &Client, ip: IpAddr) {
    let mut state = SharedState {
        n_t_pings: 0,
        n_s_pings: 0
    };

    let mut interval = tokio::time::interval(PING_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let ping_identifier = PingIdentifier(rand::random());
    let mut n_sequence = 0;

    let mut pinger = client.pinger(ip, ping_identifier).await;
    pinger.timeout(PING_TIMEOUT);

    loop {
        interval.tick().await;
        let ping_sequence = PingSequence(n_sequence);

        match pinger.ping(ping_sequence, &[]).await {
            Ok((packet, _rtt)) => {
                if packet.get_identifier() == ping_identifier && packet.get_sequence() == ping_sequence {
                    state.n_s_pings += 1;
                }
            },
            _ => {}
        };

        if state.n_t_pings == u16::MAX {
            state.n_t_pings = 0;
            state.n_s_pings = 0;
        } else {
            state.n_t_pings += 1;
        }

        if n_sequence == u16::MAX {
            n_sequence = 0;
        } else {
            n_sequence += 1;
        }
    }
}